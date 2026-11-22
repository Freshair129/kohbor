//! Supabase Auth for the desktop account surface.
//!
//! Browser authorization and HTTP responses stay inside the native process. The renderer receives
//! only a status, display name, avatar URL, and the live-match hand-off guard.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const SUPABASE_HOST: &str = "wmmvawvfnwdfzxvumlfc.supabase.co";
const SUPABASE_AUTH_ROOT: &str = "https://wmmvawvfnwdfzxvumlfc.supabase.co/auth/v1";
// Publishable keys are public by design. Never put a service-role key in this application.
const SUPABASE_PUBLISHABLE_KEY: &str = "sb_publishable_ReyVHSfs29Wzq_n49Pq_2A_y_A7qke9";
const REFRESH_TOKEN_FILE: &str = "account-refresh-token.dpapi";
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const CALLBACK_POLL: Duration = Duration::from_millis(100);
const REFRESH_SKEW: Duration = Duration::from_secs(120);
const REFRESH_POLL: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

const CALLBACK_OK_PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Kohbor sign-in</title></head><body>
<main><h1>กลับไปที่ Kohbor ได้เลย</h1><p>You can return to Kohbor.</p></main>
</body></html>"#;
const CALLBACK_ERROR_PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Kohbor sign-in</title></head><body>
<main><h1>เข้าสู่ระบบไม่สำเร็จ</h1><p>Sign-in did not complete. You can return to Kohbor.</p></main>
</body></html>"#;
const CALLBACK_NOT_FOUND_PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Kohbor</title></head><body>
<p>เส้นทางนี้ไม่พร้อมใช้งาน · This address is not available.</p></body></html>"#;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountUiStatus {
    state: &'static str,
    display_name: Option<String>,
    avatar_url: Option<String>,
    live_match_blocks_handoff: bool,
}

impl AccountUiStatus {
    fn new(
        state: &'static str,
        display_name: Option<String>,
        avatar_url: Option<String>,
        live_match_blocks_handoff: bool,
    ) -> Self {
        Self {
            state,
            display_name,
            avatar_url,
            live_match_blocks_handoff,
        }
    }
}

#[derive(Clone)]
pub(crate) struct SupabaseAuth {
    inner: Arc<AuthInner>,
}

struct AuthInner {
    refresh_path: PathBuf,
    match_active: Arc<dyn Fn() -> bool + Send + Sync>,
    operation: Mutex<()>,
    session: Mutex<AuthSession>,
    signing_in: AtomicBool,
    cancel_epoch: AtomicU64,
    refresh_thread_started: AtomicBool,
    shutting_down: AtomicBool,
}

#[derive(Default)]
struct AuthSession {
    loaded: bool,
    access_token: Option<SecretString>,
    refresh_token: Option<SecretString>,
    expires_at: Option<SystemTime>,
    display_name: Option<String>,
    avatar_url: Option<String>,
}

impl fmt::Debug for AuthSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthSession")
            .field(
                "access_token",
                &self.access_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .field("display_name", &self.display_name)
            .field("avatar_url", &self.avatar_url)
            .finish()
    }
}

struct SecretString(String);

impl SecretString {
    fn new(value: String) -> Self {
        Self(value)
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Clone, Copy, Debug)]
enum AuthFailure {
    Callback,
    Cancelled,
    InvalidResponse,
    RefreshRejected,
    Storage,
    Transport,
}

struct CallbackGate {
    expected_state: SecretString,
    consumed: bool,
}

enum CallbackDecision {
    Rejected,
    Accepted(SecretString),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallbackFailure {
    AlreadyConsumed,
    Fragment,
    Malformed,
    MissingCode,
    MissingState,
    Path,
    ProviderRejected,
    StateMismatch,
}

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
    user: Option<TokenUser>,
}

#[derive(Deserialize)]
struct TokenUser {
    user_metadata: Option<UserMetadata>,
}

#[derive(Deserialize)]
struct UserMetadata {
    full_name: Option<String>,
    name: Option<String>,
    avatar_url: Option<String>,
    picture: Option<String>,
}

#[derive(Serialize)]
struct PkceGrant<'a> {
    auth_code: &'a str,
    code_verifier: &'a str,
}

#[derive(Serialize)]
struct RefreshGrant<'a> {
    refresh_token: &'a str,
}

impl SupabaseAuth {
    pub(crate) fn new(
        app_data_dir: PathBuf,
        match_active: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            inner: Arc::new(AuthInner {
                refresh_path: app_data_dir.join(REFRESH_TOKEN_FILE),
                match_active,
                operation: Mutex::new(()),
                session: Mutex::new(AuthSession::default()),
                signing_in: AtomicBool::new(false),
                cancel_epoch: AtomicU64::new(0),
                refresh_thread_started: AtomicBool::new(false),
                shutting_down: AtomicBool::new(false),
            }),
        }
    }

    pub(crate) fn start_refresh_loop(&self) {
        if self
            .inner
            .refresh_thread_started
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let weak = Arc::downgrade(&self.inner);
        let spawned = thread::Builder::new()
            .name("supabase-auth-refresh".to_owned())
            .spawn(move || {
                loop {
                    let Some(inner) = weak.upgrade() else {
                        return;
                    };
                    if inner.shutting_down.load(Ordering::Acquire) {
                        return;
                    }
                    let auth = Self { inner };
                    let _ = auth.status();
                    for _ in 0..REFRESH_POLL.as_secs() {
                        if auth.inner.shutting_down.load(Ordering::Acquire) {
                            return;
                        }
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            });
        if spawned.is_err() {
            self.inner
                .refresh_thread_started
                .store(false, Ordering::Release);
        }
    }

    pub(crate) fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::Release);
        self.cancel_sign_in();
    }

    pub(crate) fn cancel_sign_in(&self) {
        self.inner.cancel_epoch.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn status(&self) -> AccountUiStatus {
        if self.inner.signing_in.load(Ordering::Acquire) {
            return self.snapshot(Some("signing_in"));
        }
        let _operation = match self.inner.operation.try_lock() {
            Ok(operation) => operation,
            Err(std::sync::TryLockError::WouldBlock) => return self.snapshot(None),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return self.snapshot(Some("service_unavailable"));
            }
        };
        match self.ensure_session_current() {
            Ok(()) => self.snapshot(None),
            Err(AuthFailure::RefreshRejected) => self.snapshot(Some("signed_out")),
            Err(_) => self.snapshot(Some("service_unavailable")),
        }
    }

    pub(crate) fn sign_in(&self) -> AccountUiStatus {
        let cancel_epoch = self.inner.cancel_epoch.load(Ordering::Acquire);
        if self
            .inner
            .signing_in
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return self.snapshot(Some("signing_in"));
        }
        let _busy = SigningInGuard(&self.inner.signing_in);
        let Ok(_operation) = self.inner.operation.lock() else {
            return self.snapshot(Some("service_unavailable"));
        };
        if self.cancelled_since(cancel_epoch) {
            return self.snapshot(Some("signed_out"));
        }

        match self.ensure_session_current() {
            Ok(()) => {
                if self.has_usable_access_token() {
                    return self.snapshot(Some("signed_in"));
                }
            }
            Err(AuthFailure::RefreshRejected) => {}
            Err(_) => return self.snapshot(Some("service_unavailable")),
        }

        if (self.inner.match_active)() || self.cancelled_since(cancel_epoch) {
            return self.snapshot(Some("signed_out"));
        }

        let Some(verifier) = new_pkce_verifier() else {
            return self.snapshot(Some("service_unavailable"));
        };
        let Some(state) = new_state() else {
            return self.snapshot(Some("service_unavailable"));
        };
        let challenge = pkce_challenge(verifier.expose());
        let server = match bind_loopback() {
            Ok(server) => server,
            Err(_) => return self.snapshot(Some("service_unavailable")),
        };
        let Some(address) = server.server_addr().to_ip() else {
            return self.snapshot(Some("service_unavailable"));
        };
        let handoff_deadline = Instant::now() + HANDOFF_TIMEOUT;
        let url = authorization_url(address.port(), &challenge, state.expose());
        if (self.inner.match_active)() || self.cancelled_since(cancel_epoch) {
            return self.snapshot(Some("signed_out"));
        }
        if !open_system_browser(&url) {
            return self.snapshot(Some("service_unavailable"));
        }

        let code = match wait_for_callback(
            server,
            state,
            handoff_deadline,
            cancel_epoch,
            &self.inner.cancel_epoch,
        ) {
            Ok(code) => code,
            Err(AuthFailure::Cancelled) => return self.snapshot(Some("signed_out")),
            Err(_) => return self.snapshot(Some("service_unavailable")),
        };
        if self.cancelled_since(cancel_epoch) {
            return self.snapshot(Some("signed_out"));
        }

        let body = match serde_json::to_vec(&PkceGrant {
            auth_code: code.expose(),
            code_verifier: verifier.expose(),
        }) {
            Ok(body) => body,
            Err(_) => return self.snapshot(Some("service_unavailable")),
        };
        let response = match post_json("/auth/v1/token?grant_type=pkce", &body, None) {
            Ok(response) if (200..300).contains(&response.status) => response,
            _ => return self.snapshot(Some("service_unavailable")),
        };
        let tokens = match parse_token_response(&response.body) {
            Ok(tokens) => tokens,
            Err(_) => return self.snapshot(Some("service_unavailable")),
        };
        if self.cancelled_since(cancel_epoch) {
            revoke_local_session(&tokens.access_token);
            return self.snapshot(Some("signed_out"));
        }
        if self
            .persist_refresh_token(tokens.refresh_token.as_str())
            .is_err()
        {
            if self.cancelled_since(cancel_epoch) {
                let _ = self.remove_refresh_token();
                revoke_local_session(&tokens.access_token);
                return self.snapshot(Some("signed_out"));
            }
            return self.snapshot(Some("service_unavailable"));
        }
        if self.cancelled_since(cancel_epoch) {
            let _ = self.remove_refresh_token();
            revoke_local_session(&tokens.access_token);
            return self.snapshot(Some("signed_out"));
        }
        let Ok(mut session) = self.inner.session.lock() else {
            if self.cancelled_since(cancel_epoch) {
                let _ = self.remove_refresh_token();
                revoke_local_session(&tokens.access_token);
                return self.snapshot(Some("signed_out"));
            }
            return self.snapshot(Some("service_unavailable"));
        };
        if self.cancelled_since(cancel_epoch) {
            drop(session);
            let _ = self.remove_refresh_token();
            revoke_local_session(&tokens.access_token);
            return self.snapshot(Some("signed_out"));
        }
        *session = tokens.into_session();
        if self.cancelled_since(cancel_epoch) {
            let access = session
                .access_token
                .as_ref()
                .map(|token| token.expose().to_owned());
            *session = AuthSession {
                loaded: true,
                ..AuthSession::default()
            };
            drop(session);
            let _ = self.remove_refresh_token();
            if let Some(access) = access {
                revoke_local_session(&access);
            }
            return self.snapshot(Some("signed_out"));
        }
        self.snapshot_from_session(&session, Some("signed_in"))
    }

    pub(crate) fn sign_out(&self) -> Result<(), &'static str> {
        self.cancel_sign_in();
        let Ok(_operation) = self.inner.operation.try_lock() else {
            return Err("ACCOUNT_SIGN_OUT_FAILED");
        };
        // Refresh first when there is no usable access token. Failure does not prevent local
        // token deletion below.
        let _ = self.ensure_session_current();
        let access = match self.inner.session.lock() {
            Ok(session) => session
                .access_token
                .as_ref()
                .map(|token| token.expose().to_owned()),
            Err(_) => None,
        };
        // The server request is best effort when offline. Local sign-out always removes the
        // protected refresh token, so the app cannot silently restore this session next launch.
        let _ = post_json("/auth/v1/logout?scope=local", &[], access.as_deref());
        let remove_result = match fs::remove_file(&self.inner.refresh_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("ACCOUNT_SIGN_OUT_FAILED"),
        };
        if let Ok(mut session) = self.inner.session.lock() {
            *session = AuthSession {
                loaded: true,
                ..AuthSession::default()
            };
        }
        remove_result
    }

    fn cancelled_since(&self, epoch: u64) -> bool {
        self.inner.cancel_epoch.load(Ordering::Acquire) != epoch
            || self.inner.shutting_down.load(Ordering::Acquire)
    }

    fn has_usable_access_token(&self) -> bool {
        self.inner.session.lock().is_ok_and(|session| {
            session.access_token.is_some()
                && session
                    .expires_at
                    .is_some_and(|expiry| expiry > SystemTime::now())
        })
    }

    /// Load the protected refresh token once, and refresh when the access token is absent or
    /// within the two-minute expiry window. The caller holds `operation` across this method.
    fn ensure_session_current(&self) -> Result<(), AuthFailure> {
        let refresh_token = {
            let mut session = self
                .inner
                .session
                .lock()
                .map_err(|_| AuthFailure::Storage)?;
            if !session.loaded {
                match fs::metadata(&self.inner.refresh_path) {
                    Ok(_) => {
                        let refresh =
                            crate::secrets::load_or_create(&self.inner.refresh_path, || None)
                                .and_then(|bytes| String::from_utf8(bytes).ok())
                                .filter(|refresh| !refresh.trim().is_empty());
                        if let Some(refresh) = refresh {
                            session.refresh_token = Some(SecretString::new(refresh));
                        } else {
                            self.discard_unreadable_refresh_token();
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => self.discard_unreadable_refresh_token(),
                }
                session.loaded = true;
            }

            let needs_refresh = session.refresh_token.is_some()
                && (session.access_token.is_none()
                    || session
                        .expires_at
                        .is_none_or(|expiry| expiry <= SystemTime::now() + REFRESH_SKEW));
            if needs_refresh {
                session
                    .refresh_token
                    .as_ref()
                    .map(|token| token.expose().to_owned())
            } else {
                None
            }
        };

        let Some(refresh_token) = refresh_token else {
            return Ok(());
        };
        let body = serde_json::to_vec(&RefreshGrant {
            refresh_token: &refresh_token,
        })
        .map_err(|_| AuthFailure::InvalidResponse)?;
        let response = post_json("/auth/v1/token?grant_type=refresh_token", &body, None)?;
        if response.status == 400 || response.status == 401 {
            self.remove_refresh_token()?;
            if let Ok(mut session) = self.inner.session.lock() {
                *session = AuthSession {
                    loaded: true,
                    ..AuthSession::default()
                };
            }
            return Err(AuthFailure::RefreshRejected);
        }
        if !(200..300).contains(&response.status) {
            return Err(AuthFailure::Transport);
        }
        let tokens = parse_token_response(&response.body)?;
        let rotated_session = tokens.into_session();
        let rotated_refresh = rotated_session
            .refresh_token
            .as_ref()
            .map(|token| token.expose().to_owned())
            .ok_or(AuthFailure::InvalidResponse)?;
        let mut session = match self.inner.session.lock() {
            Ok(session) => session,
            Err(poisoned) => poisoned.into_inner(),
        };
        *session = rotated_session;
        drop(session);
        self.persist_rotated_refresh_token(&rotated_refresh)?;
        Ok(())
    }

    fn persist_refresh_token(&self, refresh_token: &str) -> Result<(), AuthFailure> {
        let parent = self
            .inner
            .refresh_path
            .parent()
            .ok_or(AuthFailure::Storage)?;
        fs::create_dir_all(parent).map_err(|_| AuthFailure::Storage)?;
        crate::secrets::store(&self.inner.refresh_path, refresh_token.as_bytes())
            .ok_or(AuthFailure::Storage)
    }

    fn remove_refresh_token(&self) -> Result<(), AuthFailure> {
        match fs::remove_file(&self.inner.refresh_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(AuthFailure::Storage),
        }
    }

    fn discard_unreadable_refresh_token(&self) {
        let _ = fs::remove_file(&self.inner.refresh_path);
        crate::log_warn!(
            "account",
            "stored_token_discarded",
            "Stored account token was unreadable; signed out"
        );
    }

    fn persist_rotated_refresh_token(&self, refresh_token: &str) -> Result<(), AuthFailure> {
        // The refresh response invalidated the previous token; don't leave it as a restart fallback.
        let _ = self.remove_refresh_token();
        if self.persist_refresh_token(refresh_token).is_ok()
            || self.persist_refresh_token(refresh_token).is_ok()
        {
            return Ok(());
        }
        // Keep the rotated token in memory and remove any stale or partial on-disk value.
        let _ = self.remove_refresh_token();
        Err(AuthFailure::Storage)
    }

    fn snapshot(&self, state_override: Option<&'static str>) -> AccountUiStatus {
        match self.inner.session.lock() {
            Ok(session) => self.snapshot_from_session(&session, state_override),
            Err(_) => AccountUiStatus::new(
                "service_unavailable",
                None,
                None,
                (self.inner.match_active)(),
            ),
        }
    }

    fn snapshot_from_session(
        &self,
        session: &AuthSession,
        state_override: Option<&'static str>,
    ) -> AccountUiStatus {
        let state = state_override.unwrap_or_else(|| {
            if self.inner.signing_in.load(Ordering::Acquire) {
                "signing_in"
            } else if session.access_token.is_some() {
                "signed_in"
            } else {
                "signed_out"
            }
        });
        AccountUiStatus::new(
            state,
            session.display_name.clone(),
            session.avatar_url.clone(),
            (self.inner.match_active)(),
        )
    }
}

struct SigningInGuard<'a>(&'a AtomicBool);

impl Drop for SigningInGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl TokenResponse {
    fn into_session(self) -> AuthSession {
        let metadata = self.user.and_then(|user| user.user_metadata);
        let display_name = metadata.as_ref().and_then(|metadata| {
            metadata
                .full_name
                .as_ref()
                .filter(|name| !name.trim().is_empty())
                .or_else(|| {
                    metadata
                        .name
                        .as_ref()
                        .filter(|name| !name.trim().is_empty())
                })
                .cloned()
        });
        let avatar_url = metadata.as_ref().and_then(|metadata| {
            metadata
                .avatar_url
                .as_ref()
                .or(metadata.picture.as_ref())
                .filter(|url| url.starts_with("https://"))
                .cloned()
        });
        let expires_at = SystemTime::now().checked_add(Duration::from_secs(self.expires_in));
        AuthSession {
            loaded: true,
            access_token: Some(SecretString::new(self.access_token)),
            refresh_token: Some(SecretString::new(self.refresh_token)),
            expires_at,
            display_name,
            avatar_url,
        }
    }
}

fn parse_token_response(body: &[u8]) -> Result<TokenResponse, AuthFailure> {
    let tokens: TokenResponse =
        serde_json::from_slice(body).map_err(|_| AuthFailure::InvalidResponse)?;
    if tokens.access_token.is_empty() || tokens.refresh_token.is_empty() || tokens.expires_in == 0 {
        return Err(AuthFailure::InvalidResponse);
    }
    if SystemTime::now()
        .checked_add(Duration::from_secs(tokens.expires_in))
        .is_none()
    {
        return Err(AuthFailure::InvalidResponse);
    }
    Ok(tokens)
}

fn new_pkce_verifier() -> Option<SecretString> {
    let mut random = [0u8; 32];
    getrandom::getrandom(&mut random).ok()?;
    Some(SecretString::new(URL_SAFE_NO_PAD.encode(random)))
}

fn new_state() -> Option<SecretString> {
    let mut random = [0u8; 32];
    getrandom::getrandom(&mut random).ok()?;
    Some(SecretString::new(URL_SAFE_NO_PAD.encode(random)))
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn authorization_url(port: u16, challenge: &str, state: &str) -> String {
    let redirect_to = format!("http://127.0.0.1:{port}/callback?s={state}");
    format!(
        "{SUPABASE_AUTH_ROOT}/authorize?provider=google&redirect_to={}&code_challenge={challenge}&code_challenge_method=s256",
        percent_encode(redirect_to.as_bytes())
    )
}

fn percent_encode(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(*byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

fn bind_loopback() -> Result<tiny_http::Server, AuthFailure> {
    tiny_http::Server::http("127.0.0.1:0").map_err(|_| AuthFailure::Callback)
}

fn callback_path(url: &str) -> bool {
    url.split(|character| character == '?' || character == '#')
        .next()
        == Some("/callback")
}

fn wait_for_callback(
    server: tiny_http::Server,
    state: SecretString,
    deadline: Instant,
    cancel_epoch: u64,
    cancel_signal: &AtomicU64,
) -> Result<SecretString, AuthFailure> {
    let mut gate = CallbackGate::new(state);
    loop {
        if cancel_signal.load(Ordering::Acquire) != cancel_epoch {
            return Err(AuthFailure::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AuthFailure::Callback);
        }
        let request = server
            .recv_timeout(remaining.min(CALLBACK_POLL))
            .map_err(|_| AuthFailure::Callback)?;
        let Some(request) = request else {
            continue;
        };
        if !request
            .remote_addr()
            .is_some_and(|address| address.ip().is_loopback())
        {
            let _ = request.respond(response(403, CALLBACK_ERROR_PAGE));
            continue;
        }
        if !callback_path(request.url()) {
            let _ = request.respond(response(400, CALLBACK_NOT_FOUND_PAGE));
            continue;
        }
        if request.method() != &tiny_http::Method::Get {
            let _ = request.respond(response(405, CALLBACK_ERROR_PAGE));
            continue;
        }
        match callback_decision(&mut gate, request.url()) {
            CallbackDecision::Accepted(code) => {
                let _ = request.respond(response(200, CALLBACK_OK_PAGE));
                return Ok(code);
            }
            CallbackDecision::Rejected => {
                let _ = request.respond(response(400, CALLBACK_ERROR_PAGE));
                continue;
            }
        }
    }
}

fn response(status: u16, body: &'static str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let content_type = tiny_http::Header::from_bytes(
        b"Content-Type" as &[u8],
        b"text/html; charset=utf-8" as &[u8],
    )
    .expect("static response header");
    let cache_control = tiny_http::Header::from_bytes(b"Cache-Control", b"no-store")
        .expect("static response header");
    let referrer_policy = tiny_http::Header::from_bytes(b"Referrer-Policy", b"no-referrer")
        .expect("static response header");
    tiny_http::Response::from_string(body)
        .with_status_code(tiny_http::StatusCode(status))
        .with_header(content_type)
        .with_header(cache_control)
        .with_header(referrer_policy)
}

impl CallbackGate {
    fn new(expected_state: SecretString) -> Self {
        Self {
            expected_state,
            consumed: false,
        }
    }

    fn accept(&mut self, url: &str) -> Result<SecretString, CallbackFailure> {
        if self.consumed {
            return Err(CallbackFailure::AlreadyConsumed);
        }
        let code = parse_callback(url, self.expected_state.expose())?;
        self.consumed = true;
        Ok(code)
    }
}

fn callback_decision(gate: &mut CallbackGate, url: &str) -> CallbackDecision {
    if !callback_path(url) {
        return CallbackDecision::Rejected;
    }
    match gate.accept(url) {
        Ok(code) => CallbackDecision::Accepted(code),
        Err(_) => CallbackDecision::Rejected,
    }
}

fn parse_callback(url: &str, expected_state: &str) -> Result<SecretString, CallbackFailure> {
    if url.contains('#') {
        return Err(CallbackFailure::Fragment);
    }
    let (path, query) = url.split_once('?').ok_or(CallbackFailure::Path)?;
    if path != "/callback" {
        return Err(CallbackFailure::Path);
    }
    let mut code: Option<String> = None;
    let mut redirect_state: Option<String> = None;
    let mut provider_rejected = false;
    for pair in query.split('&') {
        let (raw_name, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(raw_name).ok_or(CallbackFailure::Malformed)?;
        match name.as_str() {
            "code" => {
                if code.is_some() {
                    return Err(CallbackFailure::Malformed);
                }
                code = Some(percent_decode(raw_value).ok_or(CallbackFailure::Malformed)?);
            }
            "s" => {
                if redirect_state.is_some() {
                    return Err(CallbackFailure::Malformed);
                }
                redirect_state = Some(percent_decode(raw_value).ok_or(CallbackFailure::Malformed)?);
            }
            "error" => provider_rejected = true,
            _ => return Err(CallbackFailure::Malformed),
        }
    }
    if provider_rejected {
        return Err(CallbackFailure::ProviderRejected);
    }
    let code = code
        .filter(|code| !code.is_empty() && code.len() <= 4096)
        .ok_or(CallbackFailure::MissingCode)?;
    let redirect_state = redirect_state.ok_or(CallbackFailure::MissingState)?;
    if !constant_time_eq(redirect_state.as_bytes(), expected_state.as_bytes()) {
        return Err(CallbackFailure::StateMismatch);
    }
    Ok(SecretString::new(code))
}

fn percent_decode(value: &str) -> Option<String> {
    let input = value.as_bytes();
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        match input[index] {
            b'+' => output.push(b' '),
            b'%' => {
                let high = *input.get(index + 1)?;
                let low = *input.get(index + 2)?;
                output.push((hex_value(high)? << 4) | hex_value(low)?);
                index += 2;
            }
            byte => output.push(byte),
        }
        index += 1;
    }
    String::from_utf8(output).ok()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let max = left.len().max(right.len());
    for index in 0..max {
        difference |= usize::from(
            left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0),
        );
    }
    difference == 0
}

#[cfg(windows)]
fn open_system_browser(url: &str) -> bool {
    // Match the existing default-browser route used by the updater (`explorer.exe`, no shell text).
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .is_ok()
}

#[cfg(not(windows))]
fn open_system_browser(_url: &str) -> bool {
    false
}

#[cfg(windows)]
fn post_json(path: &str, body: &[u8], bearer: Option<&str>) -> Result<HttpResponse, AuthFailure> {
    use std::ffi::c_void;
    use std::ptr;
    use windows::Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPTION_REDIRECT_POLICY,
        WINHTTP_OPTION_REDIRECT_POLICY_NEVER, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
        WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
        WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
    };
    use windows::core::PCWSTR;

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let agent = wide("Kohbor");
    let session = unsafe {
        WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    };
    if session.is_null() {
        return Err(AuthFailure::Transport);
    }
    let session = Handle(session);
    unsafe {
        WinHttpSetTimeouts(session.0, 10_000, 10_000, 15_000, 15_000)
            .map_err(|_| AuthFailure::Transport)?;
    }
    let host = wide(SUPABASE_HOST);
    let connection = unsafe { WinHttpConnect(session.0, PCWSTR(host.as_ptr()), 443, 0) };
    if connection.is_null() {
        return Err(AuthFailure::Transport);
    }
    let connection = Handle(connection);
    let method = wide("POST");
    let path = wide(path);
    let request = unsafe {
        WinHttpOpenRequest(
            connection.0,
            PCWSTR(method.as_ptr()),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    };
    if request.is_null() {
        return Err(AuthFailure::Transport);
    }
    let request = Handle(request);
    let redirect_policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_le_bytes();
    unsafe {
        WinHttpSetOption(
            Some(request.0 as *const c_void),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(redirect_policy.as_slice()),
        )
        .map_err(|_| AuthFailure::Transport)?;
    }
    let mut headers =
        format!("Content-Type: application/json\r\napikey: {SUPABASE_PUBLISHABLE_KEY}\r\n");
    if let Some(bearer) = bearer {
        headers.push_str("Authorization: Bearer ");
        headers.push_str(bearer);
        headers.push_str("\r\n");
    }
    let headers: Vec<u16> = headers.encode_utf16().collect();
    let body_length = u32::try_from(body.len()).map_err(|_| AuthFailure::InvalidResponse)?;
    let body_pointer = if body.is_empty() {
        None
    } else {
        Some(body.as_ptr().cast::<c_void>())
    };
    unsafe {
        WinHttpSendRequest(
            request.0,
            Some(&headers),
            body_pointer,
            body_length,
            body_length,
            0,
        )
        .map_err(|_| AuthFailure::Transport)?;
        WinHttpReceiveResponse(request.0, ptr::null_mut()).map_err(|_| AuthFailure::Transport)?;
    }
    let mut status = 0u32;
    let mut status_bytes = std::mem::size_of::<u32>() as u32;
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast::<c_void>()),
            &mut status_bytes,
            ptr::null_mut(),
        )
        .map_err(|_| AuthFailure::Transport)?;
    }
    let status = u16::try_from(status).map_err(|_| AuthFailure::InvalidResponse)?;
    if !(200..300).contains(&status) {
        // In particular, never read or expose an Auth error response body.
        return Ok(HttpResponse {
            status,
            body: Vec::new(),
        });
    }
    let mut response_body = Vec::new();
    loop {
        let mut available = 0u32;
        unsafe {
            WinHttpQueryDataAvailable(request.0, &mut available)
                .map_err(|_| AuthFailure::Transport)?;
        }
        if available == 0 {
            break;
        }
        if response_body.len().saturating_add(available as usize) > MAX_RESPONSE_BYTES {
            return Err(AuthFailure::InvalidResponse);
        }
        let mut chunk = vec![0u8; available as usize];
        let mut read = 0u32;
        unsafe {
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr().cast::<c_void>(),
                available,
                &mut read,
            )
            .map_err(|_| AuthFailure::Transport)?;
        }
        chunk.truncate(read as usize);
        response_body.extend_from_slice(&chunk);
    }
    Ok(HttpResponse {
        status,
        body: response_body,
    })
}

#[cfg(not(windows))]
fn post_json(
    _path: &str,
    _body: &[u8],
    _bearer: Option<&str>,
) -> Result<HttpResponse, AuthFailure> {
    Err(AuthFailure::Transport)
}

fn revoke_local_session(access_token: &str) {
    let _ = post_json("/auth/v1/logout?scope=local", &[], Some(access_token));
}

#[tauri::command]
pub(crate) async fn account_status(
    auth: tauri::State<'_, SupabaseAuth>,
) -> Result<AccountUiStatus, String> {
    let service = auth.inner().clone();
    let fallback = service.clone();
    let status = tauri::async_runtime::spawn_blocking(move || service.status())
        .await
        .unwrap_or_else(|_| fallback.snapshot(Some("service_unavailable")));
    Ok(status)
}

#[tauri::command]
pub(crate) async fn account_sign_in(
    auth: tauri::State<'_, SupabaseAuth>,
) -> Result<AccountUiStatus, String> {
    let service = auth.inner().clone();
    let fallback = service.clone();
    let status = tauri::async_runtime::spawn_blocking(move || service.sign_in())
        .await
        .unwrap_or_else(|_| fallback.snapshot(Some("service_unavailable")));
    Ok(status)
}

#[tauri::command]
pub(crate) fn account_cancel_sign_in(auth: tauri::State<'_, SupabaseAuth>) {
    auth.cancel_sign_in();
}

#[tauri::command]
pub(crate) async fn account_sign_out(auth: tauri::State<'_, SupabaseAuth>) -> Result<(), String> {
    let service = auth.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.sign_out())
        .await
        .map_err(|_| "ACCOUNT_SIGN_OUT_FAILED".to_owned())?
        .map_err(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{
        AuthSession, CallbackDecision, CallbackFailure, CallbackGate, SecretString, SupabaseAuth,
        bind_loopback, callback_decision, new_pkce_verifier, parse_callback, pkce_challenge,
    };

    #[test]
    fn pkce_verifier_has_rfc_length_and_unreserved_charset() {
        let verifier = new_pkce_verifier().expect("system RNG");
        let value = verifier.expose();
        assert!((43..=128).contains(&value.len()));
        assert!(
            value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        );
    }

    #[test]
    fn pkce_challenge_matches_the_rfc_7636_s256_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn callback_rejects_a_mismatched_state() {
        assert!(matches!(
            parse_callback("/callback?code=one&s=wrong", "expected"),
            Err(CallbackFailure::StateMismatch)
        ));
    }

    #[test]
    fn callback_without_a_pkce_code_is_rejected_and_fragments_are_not_parsed() {
        assert!(matches!(
            parse_callback("/callback?s=expected#access_token=implicit", "expected"),
            Err(CallbackFailure::Fragment)
        ));
        assert!(matches!(
            parse_callback("/callback?s=expected", "expected"),
            Err(CallbackFailure::MissingCode)
        ));
        assert!(matches!(
            parse_callback("/callback?code=one&state=expected", "expected"),
            Err(CallbackFailure::Malformed)
        ));
    }

    #[test]
    fn authorization_redirect_carries_state_inside_redirect_and_callback_matches_it() {
        let url = super::authorization_url(43210, "challenge", "expected");
        let redirect_param = url
            .split_once("redirect_to=")
            .expect("authorize redirect_to")
            .1
            .split_once("&code_challenge=")
            .expect("authorize PKCE challenge")
            .0;
        assert_eq!(
            super::percent_decode(redirect_param).as_deref(),
            Some("http://127.0.0.1:43210/callback?s=expected")
        );
        assert!(!url.contains("&state="));
        let code = parse_callback("/callback?code=one&s=expected", "expected")
            .expect("matching redirect state and code");
        assert_eq!(code.expose(), "one");
    }

    #[test]
    fn bad_state_missing_state_and_other_paths_do_not_end_callback_wait() {
        let mut gate = CallbackGate::new(SecretString::new("expected".to_owned()));
        for invalid in [
            "/elsewhere?s=expected&code=wrong-path",
            "/callback?code=missing-s",
            "/callback?code=wrong-s&s=other",
            "/callback?s=expected",
        ] {
            assert!(matches!(
                callback_decision(&mut gate, invalid),
                CallbackDecision::Rejected
            ));
            assert!(
                !gate.consumed,
                "invalid callback must leave the listener armed"
            );
        }
        let CallbackDecision::Accepted(code) =
            callback_decision(&mut gate, "/callback?code=valid&s=expected")
        else {
            panic!("matching s and code should complete the hand-off");
        };
        assert_eq!(code.expose(), "valid");
    }

    #[test]
    fn status_and_sign_out_do_not_wait_for_the_operation_lock() {
        use std::path::PathBuf;
        use std::sync::Arc;
        use std::sync::atomic::Ordering;

        let auth = SupabaseAuth::new(PathBuf::from("unused-account-test-dir"), Arc::new(|| false));
        let _operation = auth.inner.operation.lock().expect("operation mutex");
        let cancelled_before = auth.inner.cancel_epoch.load(Ordering::Acquire);

        assert_eq!(auth.status().state, "signed_out");
        assert_eq!(auth.sign_out(), Err("ACCOUNT_SIGN_OUT_FAILED"));
        assert_eq!(
            auth.inner.cancel_epoch.load(Ordering::Acquire),
            cancelled_before + 1,
            "sign-out cancels sign-in before returning from a busy operation"
        );
    }

    #[test]
    fn callback_gate_rejects_a_second_callback() {
        let mut gate = CallbackGate::new(SecretString::new("expected".to_owned()));
        // Percent-encoded redirect state follows the same parser path as a browser redirect.
        let url = "/callback?code=first&s=ex%70ected";
        assert!(gate.accept(&url).is_ok());
        assert!(matches!(
            gate.accept(url),
            Err(CallbackFailure::AlreadyConsumed)
        ));
    }

    #[test]
    fn callback_listener_binds_only_to_ipv4_loopback() {
        let server = bind_loopback().expect("loopback listener");
        let address = server.server_addr().to_ip().expect("TCP address");
        assert_eq!(address.ip(), std::net::Ipv4Addr::LOCALHOST);
    }

    #[test]
    fn access_and_refresh_tokens_are_redacted_from_debug_output() {
        const ACCESS: &str = "access-token-secret-value";
        const REFRESH: &str = "refresh-token-secret-value";
        let session = AuthSession {
            access_token: Some(SecretString::new(ACCESS.to_owned())),
            refresh_token: Some(SecretString::new(REFRESH.to_owned())),
            ..AuthSession::default()
        };
        let output = format!("{session:?}");
        assert!(!output.contains(ACCESS));
        assert!(!output.contains(REFRESH));
        assert!(output.contains("[REDACTED]"));
    }
}
