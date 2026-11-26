use super::protocol::{OutgoingRequest, MAX_RESPONSE_BYTES};
use super::providers::endpoint::RequestEndpoint;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

const TOTAL_TIMEOUT_MS: u64 = 120_000;
const CONNECT_TIMEOUT_MS: i32 = 15_000;

#[derive(Clone, Debug, Default)]
pub(crate) struct Cancellation {
    cancelled: Arc<AtomicBool>,
    reason: Arc<AtomicU8>,
}
impl Cancellation {
    pub(crate) fn cancel(&self) {
        self.cancel_with_reason(1);
    }
    pub(crate) fn cancel_for_match_phase(&self) {
        self.cancel_with_reason(2);
    }
    pub(crate) fn cancel_if_barrier_monitor_unavailable(&self) {
        self.cancel_with_reason(3);
    }
    fn cancel_with_reason(&self, reason: u8) {
        let _ = self
            .reason
            .compare_exchange(0, reason, Ordering::AcqRel, Ordering::Acquire);
        self.cancelled.store(true, Ordering::Release);
    }
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub(crate) fn reason_code(&self) -> Option<&'static str> {
        match self.reason.load(Ordering::Acquire) {
            1 => Some("user_cancelled"),
            2 => Some("match_phase_started"),
            3 => Some("barrier_monitor_unavailable"),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransportError {
    Network,
    Tls,
    Timeout,
    TooLarge,
    Cancelled,
    NotSent,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ResponseMeta {
    pub(crate) status: u16,
    pub(crate) retry_after: Option<String>,
    pub(crate) bytes_received: u64,
    pub(crate) body: Vec<u8>,
}

pub(crate) trait Transport: Send + Sync {
    fn post(
        &self,
        endpoint: &RequestEndpoint,
        request: &OutgoingRequest,
        cancellation: &Cancellation,
        on_chunk: &mut dyn FnMut(u16, &[u8]),
    ) -> Result<ResponseMeta, TransportError>;
    fn get(
        &self,
        endpoint: &RequestEndpoint,
        headers: &[(String, String)],
        cancellation: &Cancellation,
    ) -> Result<ResponseMeta, TransportError>;
    fn cancel_current(&self) {}
}

#[cfg(windows)]
pub(crate) struct WinHttpTransport {
    active: Mutex<Option<Arc<RequestHandle>>>,
}

#[cfg(windows)]
impl Default for WinHttpTransport {
    fn default() -> Self {
        Self {
            active: Mutex::new(None),
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
    fn ptr(&self) -> *mut std::ffi::c_void {
        self.raw as *mut std::ffi::c_void
    }
    fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            unsafe {
                let _ = windows::Win32::Networking::WinHttp::WinHttpCloseHandle(self.ptr());
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
            unsafe {
                let _ = windows::Win32::Networking::WinHttp::WinHttpCloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
impl WinHttpTransport {
    fn send(
        &self,
        endpoint: &RequestEndpoint,
        method: &str,
        headers: &[(String, String)],
        body: &[u8],
        cancellation: &Cancellation,
        on_chunk: &mut dyn FnMut(u16, &[u8]),
    ) -> Result<ResponseMeta, TransportError> {
        use std::ffi::c_void;
        use std::sync::mpsc;
        use windows::core::{w, PCWSTR};
        use windows::Win32::Networking::WinHttp::{
            WinHttpAddRequestHeaders, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
            WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData,
            WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
            WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_ADDREQ_FLAG_ADD, WINHTTP_DISABLE_COOKIES,
            WINHTTP_FLAG_SECURE, WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_OPTION_REDIRECT_POLICY,
            WINHTTP_OPTION_REDIRECT_POLICY_NEVER, WINHTTP_QUERY_FLAG_NUMBER,
            WINHTTP_QUERY_RETRY_AFTER, WINHTTP_QUERY_STATUS_CODE,
        };
        if cancellation.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        if body.len() > 6 * 1024 * 1024 {
            return Err(TransportError::TooLarge);
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
            return Err(TransportError::NotSent);
        }
        let _session = InternetHandle(session);
        unsafe {
            WinHttpSetTimeouts(
                session,
                CONNECT_TIMEOUT_MS,
                CONNECT_TIMEOUT_MS,
                CONNECT_TIMEOUT_MS,
                TOTAL_TIMEOUT_MS as i32,
            )
        }
        .map_err(|_| TransportError::NotSent)?;
        let mut host: Vec<u16> = endpoint
            .host
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let connection =
            unsafe { WinHttpConnect(session, PCWSTR(host.as_mut_ptr()), endpoint.port, 0) };
        if connection.is_null() {
            return Err(TransportError::Network);
        }
        let _connection = InternetHandle(connection);
        let mut path: Vec<u16> = endpoint
            .path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut method_wide: Vec<u16> = method.encode_utf16().chain(std::iter::once(0)).collect();
        let flags = if endpoint.secure {
            WINHTTP_FLAG_SECURE
        } else {
            Default::default()
        };
        let request = unsafe {
            WinHttpOpenRequest(
                connection,
                PCWSTR(method_wide.as_mut_ptr()),
                PCWSTR(path.as_mut_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                flags,
            )
        };
        if request.is_null() {
            return Err(TransportError::NotSent);
        }
        let handle = Arc::new(RequestHandle {
            raw: request as isize,
            closed: AtomicBool::new(false),
        });
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle.clone());
        let cleanup = || {
            if let Ok(mut current) = self.active.lock() {
                *current = None;
            }
            handle.close();
        };
        let redirect = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_le_bytes();
        let cookies = WINHTTP_DISABLE_COOKIES.to_le_bytes();
        let configure = unsafe {
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
                Some(&cookies),
            )
        });
        if configure.is_err() {
            cleanup();
            return Err(TransportError::NotSent);
        }
        let headers = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        let headers_wide: Vec<u16> = headers.encode_utf16().collect();
        let finished = Arc::new(AtomicBool::new(false));
        let timed_out = Arc::new(AtomicBool::new(false));
        let (done_tx, done_rx) = mpsc::channel();
        let watchdog_handle = handle.clone();
        let watchdog_done = done_rx;
        let watchdog_finished = finished.clone();
        let watchdog_timeout = timed_out.clone();
        let watchdog = std::thread::Builder::new()
            .name("analytics-http-timeout".into())
            .spawn(move || {
                if watchdog_done
                    .recv_timeout(std::time::Duration::from_millis(TOTAL_TIMEOUT_MS))
                    .is_err()
                {
                    if !watchdog_finished.load(Ordering::Acquire) {
                        watchdog_timeout.store(true, Ordering::Release);
                        watchdog_handle.close();
                    }
                }
            })
            .map_err(|_| {
                cleanup();
                TransportError::NotSent
            })?;
        let mut result = ResponseMeta::default();
        let response = (|| {
            let body_len = u32::try_from(body.len()).map_err(|_| TransportError::TooLarge)?;
            if !headers_wide.is_empty() {
                unsafe {
                    WinHttpAddRequestHeaders(request, &headers_wide, WINHTTP_ADDREQ_FLAG_ADD)
                }
                .map_err(|_| TransportError::NotSent)?;
            }
            let optional = if body.is_empty() {
                None
            } else {
                Some(body.as_ptr().cast::<c_void>())
            };
            unsafe { WinHttpSendRequest(request, None, optional, body_len, body_len, 0) }
                .map_err(|error| map_winhttp(error.code().0 as u32))?;
            if cancellation.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            unsafe { WinHttpReceiveResponse(request, std::ptr::null_mut()) }
                .map_err(|error| map_winhttp(error.code().0 as u32))?;
            let mut status = 0_u32;
            let mut status_bytes = std::mem::size_of::<u32>() as u32;
            let mut index = 0_u32;
            unsafe {
                WinHttpQueryHeaders(
                    request,
                    WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                    PCWSTR::null(),
                    Some((&mut status as *mut u32).cast()),
                    &mut status_bytes,
                    &mut index,
                )
            }
            .map_err(|error| map_winhttp(error.code().0 as u32))?;
            result.status = u16::try_from(status).unwrap_or(0);
            result.retry_after = query_retry_after(request, WINHTTP_QUERY_RETRY_AFTER);
            loop {
                if cancellation.is_cancelled() {
                    return Err(TransportError::Cancelled);
                }
                let mut available = 0_u32;
                unsafe { WinHttpQueryDataAvailable(request, &mut available) }
                    .map_err(|error| map_winhttp(error.code().0 as u32))?;
                if available == 0 {
                    break;
                }
                if result.body.len().saturating_add(available as usize) > MAX_RESPONSE_BYTES {
                    return Err(TransportError::TooLarge);
                }
                let mut chunk = vec![0_u8; available as usize];
                let mut read = 0_u32;
                unsafe {
                    WinHttpReadData(request, chunk.as_mut_ptr().cast(), available, &mut read)
                }
                .map_err(|error| map_winhttp(error.code().0 as u32))?;
                if read == 0 {
                    break;
                }
                chunk.truncate(read as usize);
                result.bytes_received = result.bytes_received.saturating_add(read as u64);
                result.body.extend_from_slice(&chunk);
                on_chunk(result.status, &chunk);
            }
            Ok(())
        })();
        finished.store(true, Ordering::Release);
        let _ = done_tx.send(());
        let _ = watchdog.join();
        cleanup();
        if cancellation.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        if timed_out.load(Ordering::Acquire) {
            return Err(TransportError::Timeout);
        }
        response?;
        Ok(result)
    }
}

#[cfg(windows)]
impl Transport for WinHttpTransport {
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
    fn post(
        &self,
        endpoint: &RequestEndpoint,
        request: &OutgoingRequest,
        cancellation: &Cancellation,
        on_chunk: &mut dyn FnMut(u16, &[u8]),
    ) -> Result<ResponseMeta, TransportError> {
        self.send(
            endpoint,
            "POST",
            &request.headers,
            &request.body,
            cancellation,
            on_chunk,
        )
    }
    fn get(
        &self,
        endpoint: &RequestEndpoint,
        headers: &[(String, String)],
        cancellation: &Cancellation,
    ) -> Result<ResponseMeta, TransportError> {
        self.send(endpoint, "GET", headers, &[], cancellation, &mut |_, _| {})
    }
}

#[cfg(windows)]
fn query_retry_after(request: *mut std::ffi::c_void, header: u32) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinHttp::WinHttpQueryHeaders;
    let mut bytes = 0_u32;
    let mut index = 0_u32;
    let _ = unsafe {
        WinHttpQueryHeaders(
            request,
            header,
            PCWSTR::null(),
            None,
            &mut bytes,
            &mut index,
        )
    };
    if bytes < 2 || bytes > 512 || !bytes.is_multiple_of(2) {
        return None;
    }
    let mut buffer = vec![0_u16; bytes as usize / 2];
    unsafe {
        WinHttpQueryHeaders(
            request,
            header,
            PCWSTR::null(),
            Some(buffer.as_mut_ptr().cast()),
            &mut bytes,
            &mut index,
        )
    }
    .ok()?;
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

#[cfg(windows)]
fn map_winhttp(code: u32) -> TransportError {
    match code & 0xffff {
        12002 => TransportError::Timeout,
        12017 => TransportError::Cancelled,
        12037 | 12038 | 12044 | 12045 | 12057 | 12157 | 12175 => TransportError::Tls,
        12007 | 12029..=12031 => TransportError::Network,
        _ => TransportError::Network,
    }
}

#[cfg(not(windows))]
#[derive(Default)]
pub(crate) struct WinHttpTransport;

#[cfg(not(windows))]
impl Transport for WinHttpTransport {
    fn post(
        &self,
        _endpoint: &RequestEndpoint,
        _request: &OutgoingRequest,
        cancellation: &Cancellation,
        _on_chunk: &mut dyn FnMut(u16, &[u8]),
    ) -> Result<ResponseMeta, TransportError> {
        if cancellation.is_cancelled() {
            Err(TransportError::Cancelled)
        } else {
            Err(TransportError::NotSent)
        }
    }
    fn get(
        &self,
        _endpoint: &RequestEndpoint,
        _headers: &[(String, String)],
        cancellation: &Cancellation,
    ) -> Result<ResponseMeta, TransportError> {
        if cancellation.is_cancelled() {
            Err(TransportError::Cancelled)
        } else {
            Err(TransportError::NotSent)
        }
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[derive(Default)]
    pub(crate) struct FakeTransport {
        pub(crate) calls: AtomicUsize,
        pub(crate) status: u16,
        pub(crate) chunks: Vec<Vec<u8>>,
        pub(crate) retry_after: Option<String>,
        pub(crate) endpoints: Mutex<Vec<RequestEndpoint>>,
    }
    impl Transport for FakeTransport {
        fn post(
            &self,
            endpoint: &RequestEndpoint,
            _request: &OutgoingRequest,
            cancellation: &Cancellation,
            on_chunk: &mut dyn FnMut(u16, &[u8]),
        ) -> Result<ResponseMeta, TransportError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.endpoints.lock().unwrap().push(endpoint.clone());
            if cancellation.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            let status = if self.status == 0 { 200 } else { self.status };
            let mut response = ResponseMeta {
                status,
                retry_after: self.retry_after.clone(),
                ..Default::default()
            };
            for chunk in &self.chunks {
                response.bytes_received += chunk.len() as u64;
                response.body.extend_from_slice(chunk);
                on_chunk(status, chunk);
            }
            Ok(response)
        }
        fn get(
            &self,
            endpoint: &RequestEndpoint,
            _headers: &[(String, String)],
            cancellation: &Cancellation,
        ) -> Result<ResponseMeta, TransportError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.endpoints.lock().unwrap().push(endpoint.clone());
            if cancellation.is_cancelled() {
                Err(TransportError::Cancelled)
            } else {
                let mut response = ResponseMeta {
                    status: if self.status == 0 { 200 } else { self.status },
                    retry_after: self.retry_after.clone(),
                    ..Default::default()
                };
                for chunk in &self.chunks {
                    response.bytes_received =
                        response.bytes_received.saturating_add(chunk.len() as u64);
                    response.body.extend_from_slice(chunk);
                }
                Ok(response)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::providers::endpoint::Endpoint;
    use crate::analytics::transport::fake::FakeTransport;
    use std::sync::atomic::Ordering;

    #[test]
    fn fake_transport_records_exact_host_path_and_does_not_follow_redirect_status() {
        let fake = FakeTransport {
            status: 302,
            ..Default::default()
        };
        let endpoint = Endpoint::parse("https://api.example.test/v1")
            .unwrap()
            .request_endpoint("/chat/completions");
        let response = fake
            .post(
                &endpoint,
                &OutgoingRequest {
                    headers: Vec::new(),
                    body: b"{}".to_vec(),
                },
                &Cancellation::default(),
                &mut |_, _| {},
            )
            .unwrap();
        assert_eq!(response.status, 302);
        assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
        let endpoints = fake.endpoints.lock().unwrap();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].host, "api.example.test");
        assert_eq!(endpoints[0].path, "/v1/chat/completions");
    }

    #[test]
    fn fake_transport_get_records_models_endpoint_without_network_io() {
        let fake = FakeTransport {
            chunks: vec![b"{\"data\":[]}".to_vec()],
            ..Default::default()
        };
        let endpoint = Endpoint::parse("http://127.0.0.1:4000/v1")
            .unwrap()
            .request_endpoint("/models");
        let response = fake.get(&endpoint, &[], &Cancellation::default()).unwrap();
        assert_eq!(response.body, b"{\"data\":[]}");
        assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
        assert_eq!(fake.endpoints.lock().unwrap()[0].path, "/v1/models");
    }
}
