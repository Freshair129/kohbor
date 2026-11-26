use super::request::OutgoingRequest;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub(crate) const CONNECT_TIMEOUT_MS: i32 = 4_000;
pub(crate) const TOTAL_TIMEOUT_MS: u64 = 10_000;
pub(crate) const BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;

/// Log-safe class for a WinHTTP error code. Never carries the URL, query or body.
pub(crate) fn failure_class(code: u32) -> &'static str {
    match code {
        12007 => "dns",
        12029..=12031 => "connect",
        12002 => "timeout",
        12037 | 12038 | 12044 | 12045 | 12057 | 12157 | 12175 => "tls",
        12017 => "cancelled",
        12005 | 12006 | 12009 | 12018 => "setup",
        _ => "network",
    }
}

#[cfg(windows)]
fn last_win32() -> u32 {
    unsafe { windows::Win32::Foundation::GetLastError() }.0
}

#[cfg(windows)]
fn error_win32(error: &windows::core::Error) -> u32 {
    (error.code().0 as u32) & 0xFFFF
}

/// One fixed HTTPS origin the WinHTTP transport may reach, with its own body limit and log module.
/// The host is a compile-time constant; no caller can supply a host or URL at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub(crate) host: &'static str,
    pub(crate) body_limit: usize,
    /// Log module for transport failures (`draft` or `news`); messages never carry the path.
    pub(crate) log_module: &'static str,
}

/// Public draft statistics (STATS-002).
pub(crate) const OPENDOTA: Endpoint = Endpoint {
    host: "api.opendota.com",
    body_limit: BODY_LIMIT_BYTES,
    log_module: "draft",
};

/// Local WinHTTP setup failed before any connect or send: logged, and the request does not count
/// toward the table budget (STATS-002, owner decision 2026-11-22).
#[cfg(windows)]
fn setup_failure(module: &'static str, stage: &'static str, code: u32) -> TransportError {
    crate::log_info!(
        module,
        "fetch_transport_error",
        if module == "draft" {
            "draft statistics request failed before any network I/O"
        } else {
            "request failed before any network I/O"
        },
        stage = stage,
        win32 = code,
        class = "setup"
    );
    TransportError::NotSent
}

/// Logs the failing WinHTTP stage with its Win32 code and class, then maps to `Network`.
/// Used only once a connect or send has been attempted, so the request counts.
#[cfg(windows)]
fn network_failure(module: &'static str, stage: &'static str, code: u32) -> TransportError {
    crate::log_info!(
        module,
        "fetch_transport_error",
        if module == "draft" {
            "draft statistics request failed in transport"
        } else {
            "request failed in transport"
        },
        stage = stage,
        win32 = code,
        class = failure_class(code)
    );
    TransportError::Network
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn same_request(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HttpResponse {
    pub(crate) status: u16,
    pub(crate) retry_after: Option<String>,
    pub(crate) body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransportError {
    /// Failed after a connect or send was attempted.
    Network,
    Timeout,
    TooLarge,
    /// Cancelled after a connect or send was attempted.
    Cancelled,
    /// Failed during local setup before any network I/O. Does not count toward the budget.
    NotSent,
    /// Cancelled before any connect or send. Does not count toward the budget.
    CancelledBeforeSend,
}

impl TransportError {
    /// Whether the host attempted network I/O (connect or send) for this request.
    pub(crate) fn reached_network(self) -> bool {
        !matches!(self, Self::NotSent | Self::CancelledBeforeSend)
    }
}

pub(crate) trait Transport: Send + Sync {
    fn get(
        &self,
        request: OutgoingRequest,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, TransportError>;

    /// Must interrupt the outstanding native request when revocation occurs.
    fn cancel_current(&self) {}

    /// Cancel only the request represented by this token. This lets independent consent switches
    /// share the same WinHTTP transport without revoking one another's in-flight request.
    fn cancel_request(&self, cancellation: &Cancellation) {
        cancellation.cancel();
    }
}

#[cfg(windows)]
pub(crate) struct WinHttpTransport {
    active: std::sync::Mutex<Option<Arc<RequestHandle>>>,
    serial: std::sync::Mutex<()>,
}

#[cfg(windows)]
impl Default for WinHttpTransport {
    fn default() -> Self {
        Self {
            active: std::sync::Mutex::new(None),
            serial: std::sync::Mutex::new(()),
        }
    }
}

#[cfg(windows)]
struct RequestHandle {
    raw: isize,
    closed: AtomicBool,
    cancellation: Cancellation,
}

#[cfg(windows)]
impl RequestHandle {
    fn raw(&self) -> *mut std::ffi::c_void {
        self.raw as *mut std::ffi::c_void
    }

    fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            use windows::Win32::Networking::WinHttp::WinHttpCloseHandle;
            unsafe {
                let _ = WinHttpCloseHandle(self.raw());
            }
        }
    }
}

#[cfg(windows)]
impl Drop for RequestHandle {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(windows)]
struct InternetHandle(*mut std::ffi::c_void);

#[cfg(windows)]
impl Drop for InternetHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            use windows::Win32::Networking::WinHttp::WinHttpCloseHandle;
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
impl WinHttpTransport {
    fn clear_active(&self, handle: &Arc<RequestHandle>) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, handle))
        {
            *active = None;
        }
    }
}

#[cfg(windows)]
impl Transport for WinHttpTransport {
    fn get(
        &self,
        request: OutgoingRequest,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, TransportError> {
        self.get_from(OPENDOTA, &request.path(), cancellation)
    }

    fn cancel_current(&self) {
        if let Some(handle) = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            handle.close();
        }
    }

    fn cancel_request(&self, cancellation: &Cancellation) {
        cancellation.cancel();
        if let Some(handle) = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .filter(|handle| handle.cancellation.same_request(cancellation))
        {
            handle.close();
        }
    }
}

#[cfg(windows)]
impl WinHttpTransport {
    /// GET `path` from the fixed `endpoint` over HTTPS: no proxy, no redirects, no cookies, a
    /// ten-second overall deadline and the endpoint's body limit.
    pub(crate) fn get_from(
        &self,
        endpoint: Endpoint,
        path: &str,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, TransportError> {
        use std::ffi::c_void;
        use std::sync::mpsc;
        use windows::core::{w, PCWSTR};
        use windows::Win32::Networking::WinHttp::{
            WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
            WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
            WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
            WINHTTP_QUERY_STATUS_CODE,
        };
        let module = endpoint.log_module;

        let _serial = self
            .serial
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if cancellation.is_cancelled() {
            return Err(TransportError::CancelledBeforeSend);
        }
        if self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
        {
            return Err(TransportError::NotSent);
        }

        let session = unsafe {
            WinHttpOpen(
                w!("Kohbor"),
                WINHTTP_ACCESS_TYPE_NO_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        };
        if session.is_null() {
            return Err(setup_failure(module, "open", last_win32()));
        }
        let _session = InternetHandle(session);
        configure_session(module, session)?;
        // WINHTTP_OPTION_DISABLE_FEATURE is valid on request handles only. Setting it on the
        // session failed with ERROR_WINHTTP_INCORRECT_HANDLE_TYPE (12018) before any I/O, so every
        // live request reported a network failure (owner build 6195c3c, 2026-11-22). Cookies are
        // disabled on the request handle below.
        let mut host_wide: Vec<u16> = endpoint.host.encode_utf16().collect();
        host_wide.push(0);
        let connection = unsafe { WinHttpConnect(session, PCWSTR(host_wide.as_ptr()), 443, 0) };
        if connection.is_null() {
            return Err(setup_failure(module, "connect_handle", last_win32()));
        }
        let _connection = InternetHandle(connection);
        let mut path_wide: Vec<u16> = path.encode_utf16().collect();
        path_wide.push(0);
        let request_raw = unsafe {
            WinHttpOpenRequest(
                connection,
                w!("GET"),
                PCWSTR(path_wide.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            )
        };
        if request_raw.is_null() {
            return Err(setup_failure(module, "open_request", last_win32()));
        }
        let handle = Arc::new(RequestHandle {
            raw: request_raw as isize,
            closed: AtomicBool::new(false),
            cancellation: cancellation.clone(),
        });
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle.clone());
        let clear = |transport: &Self, handle: &Arc<RequestHandle>| {
            transport.clear_active(handle);
            handle.close();
        };

        if let Err(error) = configure_request(module, request_raw) {
            clear(self, &handle);
            return Err(error);
        }

        // WinHTTP's phase timeouts do not impose an overall deadline. Close the active request at ten seconds;
        // cancellation and this watchdog share a close-once handle to avoid a late response racing revocation.
        let timed_out = Arc::new(AtomicBool::new(false));
        let (finished_tx, finished_rx) = mpsc::channel();
        let watchdog_handle = handle.clone();
        let watchdog_timed_out = timed_out.clone();
        let watchdog = std::thread::Builder::new()
            .name(format!("{module}-http-timeout"))
            .spawn(move || {
                if finished_rx
                    .recv_timeout(std::time::Duration::from_millis(TOTAL_TIMEOUT_MS))
                    .is_err()
                {
                    watchdog_timed_out.store(true, Ordering::Release);
                    watchdog_handle.close();
                }
            })
            .map_err(|_| {
                clear(self, &handle);
                TransportError::NotSent
            })?;

        let response = (|| {
            if cancellation.is_cancelled() {
                return Err(TransportError::CancelledBeforeSend);
            }
            unsafe { WinHttpSendRequest(request_raw, None, None, 0, 0, 0) }
                .map_err(|error| network_failure(module, "send", error_win32(&error)))?;
            if cancellation.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            unsafe { WinHttpReceiveResponse(request_raw, std::ptr::null_mut()) }
                .map_err(|error| network_failure(module, "receive", error_win32(&error)))?;
            let mut status = 0_u32;
            let mut status_bytes = std::mem::size_of::<u32>() as u32;
            let mut status_index = 0_u32;
            unsafe {
                WinHttpQueryHeaders(
                    request_raw,
                    WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                    PCWSTR::null(),
                    Some((&mut status as *mut u32).cast::<c_void>()),
                    &mut status_bytes,
                    &mut status_index,
                )
            }
            .map_err(|error| network_failure(module, "status", error_win32(&error)))?;
            let retry_after = query_retry_after(request_raw);
            let mut body = Vec::new();
            loop {
                if cancellation.is_cancelled() {
                    return Err(TransportError::Cancelled);
                }
                let mut available = 0_u32;
                unsafe { WinHttpQueryDataAvailable(request_raw, &mut available) }
                    .map_err(|error| network_failure(module, "read", error_win32(&error)))?;
                if available == 0 {
                    break;
                }
                if body.len().saturating_add(available as usize) > endpoint.body_limit {
                    return Err(TransportError::TooLarge);
                }
                let start = body.len();
                body.resize(start + available as usize, 0);
                let mut read = 0_u32;
                unsafe {
                    WinHttpReadData(
                        request_raw,
                        body[start..].as_mut_ptr().cast::<c_void>(),
                        available,
                        &mut read,
                    )
                }
                .map_err(|error| network_failure(module, "read", error_win32(&error)))?;
                body.truncate(start + read as usize);
                if read == 0 {
                    break;
                }
            }
            Ok(HttpResponse {
                status: u16::try_from(status).unwrap_or(u16::MAX),
                retry_after,
                body,
            })
        })();
        let _ = finished_tx.send(());
        let _ = watchdog.join();
        clear(self, &handle);
        // A request that never reached the network keeps its pre-I/O error (not counted).
        if let Err(error) = &response {
            if !error.reached_network() {
                return Err(*error);
            }
        }
        if cancellation.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        if timed_out.load(Ordering::Acquire) {
            return Err(TransportError::Timeout);
        }
        response
    }
}

/// Session options. Local only: no DNS, connect or I/O happens here.
#[cfg(windows)]
fn configure_session(
    module: &'static str,
    session: *mut std::ffi::c_void,
) -> Result<(), TransportError> {
    use windows::Win32::Networking::WinHttp::WinHttpSetTimeouts;
    unsafe {
        WinHttpSetTimeouts(
            session,
            CONNECT_TIMEOUT_MS,
            CONNECT_TIMEOUT_MS,
            CONNECT_TIMEOUT_MS,
            i32::try_from(TOTAL_TIMEOUT_MS).unwrap_or(i32::MAX),
        )
    }
    .map_err(|error| setup_failure(module, "timeouts", error_win32(&error)))
}

/// Request options: no redirects, no cookies. `WINHTTP_OPTION_DISABLE_FEATURE` belongs here, on
/// the request handle, never on the session. Local only: no I/O happens here.
#[cfg(windows)]
fn configure_request(
    module: &'static str,
    request: *mut std::ffi::c_void,
) -> Result<(), TransportError> {
    use windows::Win32::Networking::WinHttp::{
        WinHttpSetOption, WINHTTP_DISABLE_COOKIES, WINHTTP_OPTION_DISABLE_FEATURE,
        WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
    };
    let redirect = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_le_bytes();
    let no_cookies = WINHTTP_DISABLE_COOKIES.to_le_bytes();
    unsafe {
        WinHttpSetOption(
            Some(request.cast_const()),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&redirect),
        )
    }
    .and_then(|()| unsafe {
        WinHttpSetOption(
            Some(request.cast_const()),
            WINHTTP_OPTION_DISABLE_FEATURE,
            Some(&no_cookies),
        )
    })
    .map_err(|error| setup_failure(module, "request_options", error_win32(&error)))
}

#[cfg(windows)]
fn query_retry_after(request: *mut std::ffi::c_void) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinHttp::{WinHttpQueryHeaders, WINHTTP_QUERY_RETRY_AFTER};
    let mut bytes = 0_u32;
    let mut index = 0_u32;
    let _ = unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_RETRY_AFTER,
            PCWSTR::null(),
            None,
            &mut bytes,
            &mut index,
        )
    };
    if bytes < 2 || !bytes.is_multiple_of(2) || bytes as usize > 512 {
        return None;
    }
    let mut buffer = vec![0_u16; bytes as usize / 2];
    if unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_RETRY_AFTER,
            PCWSTR::null(),
            Some(buffer.as_mut_ptr().cast()),
            &mut bytes,
            &mut index,
        )
    }
    .is_err()
    {
        return None;
    }
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Some(
        String::from_utf16_lossy(&buffer[..length])
            .trim()
            .to_owned(),
    )
}

#[cfg(not(windows))]
#[derive(Default)]
pub(crate) struct WinHttpTransport;

#[cfg(not(windows))]
impl WinHttpTransport {
    pub(crate) fn get_from(
        &self,
        _endpoint: Endpoint,
        _path: &str,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, TransportError> {
        if cancellation.is_cancelled() {
            Err(TransportError::CancelledBeforeSend)
        } else {
            Err(TransportError::NotSent)
        }
    }
}

#[cfg(not(windows))]
impl Transport for WinHttpTransport {
    fn get(
        &self,
        _request: OutgoingRequest,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, TransportError> {
        // No transport on this platform: nothing reaches the network.
        if cancellation.is_cancelled() {
            Err(TransportError::CancelledBeforeSend)
        } else {
            Err(TransportError::NotSent)
        }
    }
}

#[cfg(all(test, windows))]
mod setup_tests {
    use super::{configure_request, configure_session, failure_class, InternetHandle};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Networking::WinHttp::{
        WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WINHTTP_ACCESS_TYPE_NO_PROXY,
        WINHTTP_FLAG_SECURE,
    };

    /// Regression for owner build 6195c3c (2026-11-22): a session-level
    /// WINHTTP_OPTION_DISABLE_FEATURE failed with 12018 before any I/O, so every live request
    /// reported `fetch_failed`. Opening handles makes no network request; nothing is sent.
    #[test]
    fn local_winhttp_setup_succeeds_without_any_network_io() {
        let session = unsafe {
            WinHttpOpen(
                w!("Kohbor"),
                WINHTTP_ACCESS_TYPE_NO_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        };
        assert!(!session.is_null());
        let _session = InternetHandle(session);
        assert_eq!(configure_session("draft", session), Ok(()));
        let connection = unsafe { WinHttpConnect(session, w!("api.opendota.com"), 443, 0) };
        assert!(!connection.is_null());
        let _connection = InternetHandle(connection);
        let request = unsafe {
            WinHttpOpenRequest(
                connection,
                w!("GET"),
                w!("/api/heroStats"),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            )
        };
        assert!(!request.is_null());
        let _request = InternetHandle(request);
        assert_eq!(configure_request("draft", request), Ok(()));
    }

    /// A request cancelled before send returns a pre-I/O error and so is not counted.
    /// Nothing is sent: the cancellation is checked before any handle is opened.
    #[test]
    fn pre_send_cancellation_never_reaches_the_network() {
        use super::{Cancellation, Transport, TransportError, WinHttpTransport};
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let result = WinHttpTransport::default().get(
            crate::hero_stats::request::OutgoingRequest::HeroStats,
            &cancellation,
        );
        assert_eq!(result, Err(TransportError::CancelledBeforeSend));
        assert!(!TransportError::CancelledBeforeSend.reached_network());
        assert!(!TransportError::NotSent.reached_network());
        for counted in [
            TransportError::Network,
            TransportError::Timeout,
            TransportError::TooLarge,
            TransportError::Cancelled,
        ] {
            assert!(counted.reached_network(), "{counted:?}");
        }
    }

    #[test]
    fn winhttp_codes_map_to_log_safe_classes() {
        assert_eq!(failure_class(12007), "dns");
        assert_eq!(failure_class(12029), "connect");
        assert_eq!(failure_class(12002), "timeout");
        assert_eq!(failure_class(12175), "tls");
        assert_eq!(failure_class(12018), "setup");
        assert_eq!(failure_class(1), "network");
    }
}
