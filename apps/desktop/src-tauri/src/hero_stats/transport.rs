use super::request::OutgoingRequest;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub(crate) const CONNECT_TIMEOUT_MS: i32 = 4_000;
pub(crate) const TOTAL_TIMEOUT_MS: u64 = 10_000;
pub(crate) const BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default)]
pub(crate) struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
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
    Network,
    Timeout,
    TooLarge,
    Cancelled,
}

pub(crate) trait Transport: Send + Sync {
    fn get(
        &self,
        request: OutgoingRequest,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, TransportError>;

    /// Must interrupt the outstanding native request when revocation occurs.
    fn cancel_current(&self) {}
}

#[cfg(windows)]
pub(crate) struct WinHttpTransport {
    active: std::sync::Mutex<Option<Arc<RequestHandle>>>,
}

#[cfg(windows)]
impl Default for WinHttpTransport {
    fn default() -> Self {
        Self {
            active: std::sync::Mutex::new(None),
        }
    }
}

#[cfg(windows)]
struct RequestHandle {
    raw: isize,
    closed: AtomicBool,
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
        use std::ffi::c_void;
        use std::sync::mpsc;
        use windows::core::{w, PCWSTR};
        use windows::Win32::Networking::WinHttp::{
            WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
            WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
            WinHttpSetOption, WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_NO_PROXY,
            WINHTTP_DISABLE_COOKIES, WINHTTP_FLAG_SECURE, WINHTTP_OPTION_DISABLE_FEATURE,
            WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
            WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        };

        if cancellation.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        if self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
        {
            return Err(TransportError::Network);
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
            return Err(TransportError::Network);
        }
        let _session = InternetHandle(session);
        if unsafe {
            WinHttpSetTimeouts(
                session,
                CONNECT_TIMEOUT_MS,
                CONNECT_TIMEOUT_MS,
                CONNECT_TIMEOUT_MS,
                i32::try_from(TOTAL_TIMEOUT_MS).unwrap_or(i32::MAX),
            )
        }
        .is_err()
        {
            return Err(TransportError::Network);
        }
        let cookies = WINHTTP_DISABLE_COOKIES.to_le_bytes();
        if unsafe {
            WinHttpSetOption(
                Some(session.cast_const()),
                WINHTTP_OPTION_DISABLE_FEATURE,
                Some(&cookies),
            )
        }
        .is_err()
        {
            return Err(TransportError::Network);
        }
        let connection = unsafe { WinHttpConnect(session, w!("api.opendota.com"), 443, 0) };
        if connection.is_null() {
            return Err(TransportError::Network);
        }
        let _connection = InternetHandle(connection);
        let path = request.path();
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
            return Err(TransportError::Network);
        }
        let handle = Arc::new(RequestHandle {
            raw: request_raw as isize,
            closed: AtomicBool::new(false),
        });
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle.clone());
        let clear = |transport: &Self, handle: &Arc<RequestHandle>| {
            transport.clear_active(handle);
            handle.close();
        };

        let redirect = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_le_bytes();
        let no_cookies = WINHTTP_DISABLE_COOKIES.to_le_bytes();
        if unsafe {
            WinHttpSetOption(
                Some(request_raw.cast_const()),
                WINHTTP_OPTION_REDIRECT_POLICY,
                Some(&redirect),
            )
        }
        .is_err()
            || unsafe {
                WinHttpSetOption(
                    Some(request_raw.cast_const()),
                    WINHTTP_OPTION_DISABLE_FEATURE,
                    Some(&no_cookies),
                )
            }
            .is_err()
        {
            clear(self, &handle);
            return Err(TransportError::Network);
        }

        // WinHTTP's phase timeouts do not impose an overall deadline. Close the active request at ten seconds;
        // cancellation and this watchdog share a close-once handle to avoid a late response racing revocation.
        let timed_out = Arc::new(AtomicBool::new(false));
        let (finished_tx, finished_rx) = mpsc::channel();
        let watchdog_handle = handle.clone();
        let watchdog_timed_out = timed_out.clone();
        let watchdog = std::thread::Builder::new()
            .name("draft-stats-timeout".to_owned())
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
                TransportError::Network
            })?;

        let response = (|| {
            if cancellation.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            unsafe { WinHttpSendRequest(request_raw, None, None, 0, 0, 0) }
                .map_err(|_| TransportError::Network)?;
            if cancellation.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            unsafe { WinHttpReceiveResponse(request_raw, std::ptr::null_mut()) }
                .map_err(|_| TransportError::Network)?;
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
            .map_err(|_| TransportError::Network)?;
            let retry_after = query_retry_after(request_raw);
            let mut body = Vec::new();
            loop {
                if cancellation.is_cancelled() {
                    return Err(TransportError::Cancelled);
                }
                let mut available = 0_u32;
                unsafe { WinHttpQueryDataAvailable(request_raw, &mut available) }
                    .map_err(|_| TransportError::Network)?;
                if available == 0 {
                    break;
                }
                if body.len().saturating_add(available as usize) > BODY_LIMIT_BYTES {
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
                .map_err(|_| TransportError::Network)?;
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
        if cancellation.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        if timed_out.load(Ordering::Acquire) {
            return Err(TransportError::Timeout);
        }
        response
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
impl Transport for WinHttpTransport {
    fn get(
        &self,
        _request: OutgoingRequest,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, TransportError> {
        if cancellation.is_cancelled() {
            Err(TransportError::Cancelled)
        } else {
            Err(TransportError::Network)
        }
    }
}
