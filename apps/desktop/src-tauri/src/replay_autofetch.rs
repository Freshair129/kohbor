//! Bounded, opt-in replay acquisition for the current in-memory finished-match session.
//!
//! The per-match budget is six OpenDota calls total: one lookup, at most one request POST,
//! and at most four polls at 1, 3, 6, and 12 minutes after the POST. The poll offsets are absolute
//! from the POST start, making the total wait bounded at 12 minutes. Parse status is watched for
//! at most 30 minutes after queueing. All calls pass through OpenDotaService's shared daily request ledger. Valve downloads are plain HTTP by measurement,
//! so the URL is accepted only after exact host/path validation and WinHTTP redirects are disabled.

use crate::hero_stats::transport::{
    Cancellation, HttpResponse, TransportError, ValveWinHttpTransport,
};
use crate::intake::lock;
use crate::opendota::{OpenDotaService, ReplayApiFailure};
use crate::replay_library::{ReplayLibrary, ReplayStatus};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const COMPRESSED_CAP_BYTES: u64 = 200 * 1024 * 1024;
pub(crate) const DECOMPRESSED_CAP_BYTES: u64 = 1024 * 1024 * 1024;
const FETCH_DISK_FLOOR_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const POLL_SCHEDULE: [Duration; 4] = [
    Duration::from_secs(60),
    Duration::from_secs(3 * 60),
    Duration::from_secs(6 * 60),
    Duration::from_secs(12 * 60),
];
const MAX_LOOKUP_CALLS: usize = 6;
const FETCH_POLL_INTERVAL: Duration = Duration::from_millis(250);
const PARSE_WAIT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const LOCAL_REPLAY_CHECK_INTERVAL: Duration = Duration::from_secs(1);
static NEXT_STAGE_FILE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FetchState {
    Off,
    WaitingForMatchEnd,
    LookingUp,
    Requested,
    Downloading,
    Parsing,
    Ready,
    NotAvailableYet,
    NotFound,
    Expired,
    RateLimited,
    Offline,
    Failed,
    SkippedLocal,
}

impl FetchState {
    pub(crate) fn reply_name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::WaitingForMatchEnd => "waiting_for_match_end",
            Self::LookingUp => "looking_up",
            Self::Requested => "requested",
            Self::Downloading => "downloading",
            Self::Parsing => "parsing",
            Self::Ready => "ready",
            Self::NotAvailableYet => "not_available_yet",
            Self::NotFound => "not_found",
            Self::Expired => "expired",
            Self::RateLimited => "rate_limited",
            Self::Offline => "offline",
            Self::Failed => "failed",
            Self::SkippedLocal => "not_found",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FetchReason {
    ConsentOff,
    WaitingForMatchEnd,
    LookupStarted,
    RequestedOnce,
    InvalidUrl,
    LocalReplayFound,
    NetworkUnavailable,
    RateLimitReceived,
    LookupNotFound,
    LookupBudgetEnded,
    DownloadStarted,
    DownloadRejected,
    DownloadExpired,
    Downloaded,
    DecodeRejected,
    ParseQueued,
    ParseReady,
    ParseFailed,
    Cancelled,
    DiskFloor,
    DiskUnknown,
    DiskWrite,
    CompressedCleanupFailed,
    ParseWaitTimeout,
    BundleValidationFailed,
}

impl FetchReason {
    fn code(self) -> &'static str {
        match self {
            Self::ConsentOff => "consent_off",
            Self::WaitingForMatchEnd => "waiting_for_match_end",
            Self::LookupStarted => "lookup_started",
            Self::RequestedOnce => "requested_once",
            Self::InvalidUrl => "invalid_url",
            Self::LocalReplayFound => "local_replay_found",
            Self::NetworkUnavailable => "network_unavailable",
            Self::RateLimitReceived => "rate_limit_received",
            Self::LookupNotFound => "lookup_not_found",
            Self::LookupBudgetEnded => "lookup_budget_ended",
            Self::DownloadStarted => "download_started",
            Self::DownloadRejected => "download_rejected",
            Self::DownloadExpired => "download_expired",
            Self::Downloaded => "downloaded",
            Self::DecodeRejected => "decode_rejected",
            Self::ParseQueued => "parse_queued",
            Self::ParseReady => "parse_ready",
            Self::ParseFailed => "parse_failed",
            Self::Cancelled => "cancelled",
            Self::DiskFloor => "disk_floor",
            Self::DiskUnknown => "disk_unknown",
            Self::DiskWrite => "disk_write",
            Self::CompressedCleanupFailed => "compressed_cleanup_failed",
            Self::ParseWaitTimeout => "parse_wait_timeout",
            Self::BundleValidationFailed => "bundle_validation_failed",
        }
    }
}

/// This is the complete shape of every replay-fetch log event. It contains no match ID, URL,
/// salt, filesystem path, account ID, or name; callers cannot attach arbitrary strings.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
struct TransitionLog {
    state: FetchState,
    reason: FetchReason,
    count: u64,
    duration_ms: u64,
}

trait TransitionSink: Send + Sync {
    fn record(&self, event: TransitionLog);
}

struct NativeTransitionSink;

impl TransitionSink for NativeTransitionSink {
    fn record(&self, event: TransitionLog) {
        crate::log_info!(
            "replay",
            "autofetch_transition",
            "automatic replay fetch changed state",
            state = event.state.reply_name(),
            reason = event.reason.code(),
            count = event.count,
            duration_ms = event.duration_ms
        );
    }
}

pub(crate) trait OpenDotaReplayApi: Send + Sync {
    fn lookup(
        &self,
        match_id: &str,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, ReplayApiFailure>;
    fn request_replay(
        &self,
        match_id: &str,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, ReplayApiFailure>;
    fn cancel(&self, cancellation: &Cancellation);
}

struct LedgerReplayApi(Arc<OpenDotaService>, Arc<AtomicBool>);

impl OpenDotaReplayApi for LedgerReplayApi {
    fn lookup(
        &self,
        match_id: &str,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, ReplayApiFailure> {
        self.0
            .replay_lookup(match_id, cancellation, &self.1)
    }

    fn request_replay(
        &self,
        match_id: &str,
        cancellation: &Cancellation,
    ) -> Result<HttpResponse, ReplayApiFailure> {
        self.0
            .replay_request(match_id, true, cancellation, &self.1)
    }

    fn cancel(&self, cancellation: &Cancellation) {
        self.0.cancel_replay_request(cancellation);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedReplayUrl {
    host: String,
    path: String,
}

pub(crate) fn validate_replay_url(
    match_id: &str,
    url: &str,
) -> Result<ValidatedReplayUrl, &'static str> {
    if match_id.is_empty() || !match_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("invalid_match_id");
    }
    let rest = url.strip_prefix("http://").ok_or("invalid_scheme")?;
    if rest.contains('?') || rest.contains('#') || rest.contains('@') || rest.contains('\\') {
        return Err("invalid_url");
    }
    let (authority, path) = rest.split_once('/').ok_or("invalid_path")?;
    if authority.contains(':') || authority.is_empty() {
        return Err("invalid_authority");
    }
    let host_digits = authority
        .strip_prefix("replay")
        .and_then(|host| host.strip_suffix(".valve.net"))
        .ok_or("invalid_host")?;
    if host_digits.is_empty() || !host_digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("invalid_host");
    }
    if host_digits.len() > 20 || match_id.len() > 20 {
        return Err("invalid_length");
    }
    let path = format!("/{path}");
    let replay_name = path.strip_prefix("/570/").ok_or("invalid_path")?;
    let prefix = format!("{match_id}_");
    let salt = replay_name
        .strip_prefix(&prefix)
        .and_then(|name| name.strip_suffix(".dem.bz2"))
        .ok_or("invalid_match_id")?;
    if salt.is_empty() || !salt.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("invalid_salt");
    }
    if salt.len() > 20 {
        return Err("invalid_length");
    }
    Ok(ValidatedReplayUrl {
        host: authority.to_owned(),
        path,
    })
}

pub(crate) trait ValveReplayTransport: Send + Sync {
    /// Streams the body in bounded chunks. A non-200 status is returned without following redirects.
    fn download(
        &self,
        url: &ValidatedReplayUrl,
        cancellation: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), TransportError>,
    ) -> Result<u16, TransportError>;
    fn cancel(&self, cancellation: &Cancellation);
}

struct NativeValveReplayTransport(ValveWinHttpTransport);

impl Default for NativeValveReplayTransport {
    fn default() -> Self {
        Self(ValveWinHttpTransport::default())
    }
}

impl ValveReplayTransport for NativeValveReplayTransport {
    fn download(
        &self,
        url: &ValidatedReplayUrl,
        cancellation: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), TransportError>,
    ) -> Result<u16, TransportError> {
        self.0.download_stream(
            &url.host,
            &url.path,
            cancellation,
            usize::try_from(COMPRESSED_CAP_BYTES).unwrap_or(usize::MAX),
            sink,
        )
    }

    fn cancel(&self, cancellation: &Cancellation) {
        self.0.cancel_request(cancellation);
    }
}

trait DiskSpaceProbe: Send + Sync {
    fn available_bytes(&self, path: &Path) -> Option<u64>;
}

struct NativeDiskSpaceProbe;

impl DiskSpaceProbe for NativeDiskSpaceProbe {
    fn available_bytes(&self, path: &Path) -> Option<u64> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let mut path_wide: Vec<u16> = path.as_os_str().encode_wide().collect();
            path_wide.push(0);
            let mut available = 0_u64;
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn GetDiskFreeSpaceExW(
                    path: *const u16,
                    available_to_caller: *mut u64,
                    total_bytes: *mut u64,
                    free_bytes: *mut u64,
                ) -> i32;
            }
            // SAFETY: the path is a nul-terminated UTF-16 buffer and output pointers are valid.
            let ok = unsafe {
                GetDiskFreeSpaceExW(
                    path_wide.as_ptr(),
                    &mut available,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            (ok != 0).then_some(available)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            None
        }
    }
}

trait ReplayLibraryPort: Send + Sync {
    fn staging_dir(&self) -> Result<PathBuf, &'static str>;
    fn has_match_replay(&self, match_id: &str) -> bool;
    fn queue_fetched_dem(&self, match_id: &str, path: &Path) -> Result<(), &'static str>;
    fn cancel_queued_fetched_dem(&self, match_id: &str, path: &Path) -> bool;
    fn fetched_status(&self, match_id: &str) -> Option<ReplayStatus>;
    fn bundle_validated(&self, match_id: &str) -> bool;
}

impl ReplayLibraryPort for ReplayLibrary {
    fn staging_dir(&self) -> Result<PathBuf, &'static str> {
        self.autofetch_staging_dir()
    }
    fn has_match_replay(&self, match_id: &str) -> bool {
        ReplayLibrary::has_match_replay(self, match_id)
    }
    fn queue_fetched_dem(&self, match_id: &str, path: &Path) -> Result<(), &'static str> {
        ReplayLibrary::queue_fetched_dem(self, match_id, path).map(|_| ())
    }
    fn cancel_queued_fetched_dem(&self, match_id: &str, path: &Path) -> bool {
        ReplayLibrary::cancel_queued_fetched_dem(self, match_id, path)
    }
    fn fetched_status(&self, match_id: &str) -> Option<ReplayStatus> {
        ReplayLibrary::fetched_status(self, match_id)
    }
    fn bundle_validated(&self, match_id: &str) -> bool {
        self.open_bundle(match_id).is_ok()
    }
}

#[derive(Clone, Copy)]
struct Limits {
    compressed: u64,
    decompressed: u64,
    disk_floor: u64,
    poll_schedule: [Duration; 4],
    parse_timeout: Duration,
}

impl Limits {
    const fn production() -> Self {
        Self {
            compressed: COMPRESSED_CAP_BYTES,
            decompressed: DECOMPRESSED_CAP_BYTES,
            disk_floor: FETCH_DISK_FLOOR_BYTES,
            poll_schedule: POLL_SCHEDULE,
            parse_timeout: PARSE_WAIT_TIMEOUT,
        }
    }
}

struct ActiveWork {
    match_id: String,
    cancellation: Cancellation,
    parsing: bool,
    dem_path: Option<PathBuf>,
}

struct Inner {
    enabled: Arc<AtomicBool>,
    stopping: AtomicBool,
    library: Arc<dyn ReplayLibraryPort>,
    api: Arc<dyn OpenDotaReplayApi>,
    valve: Arc<dyn ValveReplayTransport>,
    disk: Arc<dyn DiskSpaceProbe>,
    limits: Limits,
    barrier: Arc<dyn Fn() -> bool + Send + Sync>,
    current: Mutex<(Option<String>, FetchState, Option<Instant>)>,
    attempted: Mutex<HashSet<String>>,
    active: Mutex<Option<ActiveWork>>,
    logger: Arc<dyn TransitionSink>,
}

#[derive(Clone)]
pub(crate) struct ReplayAutoFetch {
    inner: Arc<Inner>,
}

impl ReplayAutoFetch {
    pub(crate) fn open(
        library: ReplayLibrary,
        opendota: Arc<OpenDotaService>,
        enabled: bool,
        barrier: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        let enabled_gate = Arc::new(AtomicBool::new(enabled));
        Self::open_with_gate(
            Arc::new(library),
            Arc::new(LedgerReplayApi(opendota, enabled_gate.clone())),
            Arc::new(NativeValveReplayTransport::default()),
            Arc::new(NativeDiskSpaceProbe),
            enabled_gate,
            barrier,
            Limits::production(),
            Arc::new(NativeTransitionSink),
        )
    }

    fn open_with_ports(
        library: Arc<dyn ReplayLibraryPort>,
        api: Arc<dyn OpenDotaReplayApi>,
        valve: Arc<dyn ValveReplayTransport>,
        disk: Arc<dyn DiskSpaceProbe>,
        enabled: bool,
        barrier: Arc<dyn Fn() -> bool + Send + Sync>,
        limits: Limits,
        logger: Arc<dyn TransitionSink>,
    ) -> Self {
        Self::open_with_gate(
            library,
            api,
            valve,
            disk,
            Arc::new(AtomicBool::new(enabled)),
            barrier,
            limits,
            logger,
        )
    }

    fn open_with_gate(
        library: Arc<dyn ReplayLibraryPort>,
        api: Arc<dyn OpenDotaReplayApi>,
        valve: Arc<dyn ValveReplayTransport>,
        disk: Arc<dyn DiskSpaceProbe>,
        enabled: Arc<AtomicBool>,
        barrier: Arc<dyn Fn() -> bool + Send + Sync>,
        limits: Limits,
        logger: Arc<dyn TransitionSink>,
    ) -> Self {
        if let Ok(staging) = library.staging_dir() {
            cleanup_interrupted_staging(&staging);
        }
        Self {
            inner: Arc::new(Inner {
                enabled,
                stopping: AtomicBool::new(false),
                library,
                api,
                valve,
                disk,
                limits,
                barrier,
                current: Mutex::new((None, FetchState::Off, None)),
                attempted: Mutex::new(HashSet::new()),
                active: Mutex::new(None),
                logger,
            }),
        }
    }

    pub(crate) fn start(&self, session_match_id: Arc<dyn Fn() -> Option<String> + Send + Sync>) {
        let fetcher = self.clone();
        let _ = thread::Builder::new()
            .name("replay-autofetch".to_owned())
            .spawn(move || {
                while !fetcher.inner.stopping.load(Ordering::Acquire) {
                    if !fetcher.inner.enabled.load(Ordering::Acquire) {
                        thread::sleep(FETCH_POLL_INTERVAL);
                        continue;
                    }
                    let Some(match_id) = session_match_id() else {
                        thread::sleep(FETCH_POLL_INTERVAL);
                        continue;
                    };
                    if !(match_id.bytes().all(|byte| byte.is_ascii_digit()) && !match_id.is_empty())
                    {
                        thread::sleep(FETCH_POLL_INTERVAL);
                        continue;
                    }
                    if (fetcher.inner.barrier)() {
                        fetcher.transition(
                            &match_id,
                            FetchState::WaitingForMatchEnd,
                            FetchReason::WaitingForMatchEnd,
                            0,
                        );
                        thread::sleep(FETCH_POLL_INTERVAL);
                        continue;
                    }
                    if !fetcher.was_attempted(&match_id) {
                        fetcher.attempt_match(&match_id);
                    }
                    thread::sleep(FETCH_POLL_INTERVAL);
                }
            });
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.inner.enabled.store(enabled, Ordering::Release);
        if enabled {
            return;
        }
        let active = {
            let active = lock(&self.inner.active);
            active.as_ref().map(|work| {
                work.cancellation.cancel();
                (
                    work.match_id.clone(),
                    work.cancellation.clone(),
                    work.parsing,
                    work.dem_path.clone(),
                )
            })
        };
        if let Some((match_id, cancellation, parsing, dem_path)) = active {
            self.inner.api.cancel(&cancellation);
            self.inner.valve.cancel(&cancellation);
            if parsing {
                if let Some(path) = dem_path {
                    self.inner
                        .library
                        .cancel_queued_fetched_dem(&match_id, &path);
                }
            }
            self.transition(
                &match_id,
                FetchState::Off,
                FetchReason::ConsentOff,
                0,
            );
        } else {
            self.transition("", FetchState::Off, FetchReason::ConsentOff, 0);
        }
    }

    pub(crate) fn stop_for_exit(&self) {
        self.inner.stopping.store(true, Ordering::Release);
        let cancellation = lock(&self.inner.active)
            .as_ref()
            .map(|active| active.cancellation.clone());
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
            self.inner.api.cancel(&cancellation);
            self.inner.valve.cancel(&cancellation);
        }
    }

    pub(crate) fn state_for(&self, match_id: Option<&str>) -> FetchState {
        let current = lock(&self.inner.current);
        if let Some(match_id) = match_id {
            if current.0.as_deref() == Some(match_id) {
                return current.1;
            }
            if !self.inner.enabled.load(Ordering::Acquire) {
                return FetchState::Off;
            }
            return if (self.inner.barrier)() {
                FetchState::WaitingForMatchEnd
            } else {
                FetchState::LookingUp
            };
        }
        FetchState::Off
    }

    fn was_attempted(&self, match_id: &str) -> bool {
        lock(&self.inner.attempted).contains(match_id)
    }

    fn attempt_match(&self, match_id: &str) {
        if !self.inner.enabled.load(Ordering::Acquire)
            || self.inner.stopping.load(Ordering::Acquire)
        {
            self.transition(match_id, FetchState::Off, FetchReason::ConsentOff, 0);
            return;
        }
        if (self.inner.barrier)() {
            self.transition(
                match_id,
                FetchState::WaitingForMatchEnd,
                FetchReason::WaitingForMatchEnd,
                0,
            );
            return;
        }
        if self.inner.library.has_match_replay(match_id) {
            self.transition(
                match_id,
                FetchState::SkippedLocal,
                FetchReason::LocalReplayFound,
                0,
            );
            lock(&self.inner.attempted).insert(match_id.to_owned());
            return;
        }
        {
            let mut attempted = lock(&self.inner.attempted);
            if !attempted.insert(match_id.to_owned()) {
                return;
            }
        }
        let cancellation = Cancellation::default();
        *lock(&self.inner.active) = Some(ActiveWork {
            match_id: match_id.to_owned(),
            cancellation: cancellation.clone(),
            parsing: false,
            dem_path: None,
        });
        if !self.work_is_live(&cancellation) {
            self.transition(match_id, FetchState::Off, FetchReason::Cancelled, 0);
        }
        let started = Instant::now();
        let result = self.fetch_one(match_id, &cancellation);
        let mut active = lock(&self.inner.active);
        if active
            .as_ref()
            .is_some_and(|work| work.match_id == match_id)
        {
            *active = None;
        }
        if matches!(result, FetchState::Off | FetchState::SkippedLocal) {
            let reason = if result == FetchState::Off {
                FetchReason::Cancelled
            } else {
                FetchReason::LocalReplayFound
            };
            self.transition(match_id, result, reason, duration_ms(started));
        }
    }

    fn work_is_live(&self, cancellation: &Cancellation) -> bool {
        !cancellation.is_cancelled()
            && self.inner.enabled.load(Ordering::Acquire)
            && !self.inner.stopping.load(Ordering::Acquire)
    }

    fn fetch_one(&self, match_id: &str, cancellation: &Cancellation) -> FetchState {
        let mut request_count = 0_u64;
        if !self.work_is_live(cancellation) {
            return FetchState::Off;
        }
        self.transition(
            match_id,
            FetchState::LookingUp,
            FetchReason::LookupStarted,
            request_count,
        );
        let mut response = match self.lookup(match_id, cancellation, &mut request_count) {
            Ok(response) => response,
            Err(state) => return state,
        };
        let mut post_started = None;
        let mut replay_url = match replay_url_from_response(&response) {
            Ok(Some(url)) => Some(url),
            Ok(None) => {
                if request_count >= MAX_LOOKUP_CALLS as u64 {
                    self.transition(
                        match_id,
                        FetchState::NotAvailableYet,
                        FetchReason::LookupBudgetEnded,
                        request_count,
                    );
                    return FetchState::NotAvailableYet;
                }
                if !self.work_is_live(cancellation) {
                    return FetchState::Off;
                }
                post_started = Some(Instant::now());
                match self.inner.api.request_replay(match_id, cancellation) {
                    Ok(post) => {
                        if !self.work_is_live(cancellation) {
                            return FetchState::Off;
                        }
                        request_count += 1;
                        match classify_api_status(post.status) {
                            Some(state) => {
                                return self.finish_api_state(match_id, state, request_count)
                            }
                            None => {}
                        }
                        self.transition(
                            match_id,
                            FetchState::Requested,
                            FetchReason::RequestedOnce,
                            request_count,
                        );
                    }
                    Err(error) => {
                        if !self.work_is_live(cancellation) {
                            return FetchState::Off;
                        }
                        return self.finish_api_failure(match_id, error, request_count);
                    }
                }
                None
            }
            Err(()) => return self.finish_api_state(match_id, FetchState::Failed, request_count),
        };

        if replay_url.is_none() {
            let post_started = post_started.unwrap_or_else(Instant::now);
            for (poll_index, offset) in self.inner.limits.poll_schedule.iter().copied().enumerate()
            {
                if request_count >= MAX_LOOKUP_CALLS as u64 {
                    break;
                }
                let wait = offset.saturating_sub(post_started.elapsed());
                match self.sleep_or_cancel(match_id, cancellation, wait) {
                    SleepResult::Continue => {}
                    SleepResult::LocalReplay => return FetchState::SkippedLocal,
                    SleepResult::Cancelled => return FetchState::Off,
                }
                response = match self.lookup(match_id, cancellation, &mut request_count) {
                    Ok(response) => response,
                    Err(state) => return state,
                };
                match replay_url_from_response(&response) {
                    Ok(Some(url)) => {
                        replay_url = Some(url);
                        break;
                    }
                    Ok(None) => {}
                    Err(()) => {
                        return self.finish_api_state(match_id, FetchState::Failed, request_count)
                    }
                }
                if poll_index + 1 == self.inner.limits.poll_schedule.len() {
                    self.transition(
                        match_id,
                        FetchState::NotAvailableYet,
                        FetchReason::LookupBudgetEnded,
                        request_count,
                    );
                    return FetchState::NotAvailableYet;
                }
            }
        }
        let Some(url) = replay_url else {
            self.transition(
                match_id,
                FetchState::NotAvailableYet,
                FetchReason::LookupBudgetEnded,
                request_count,
            );
            return FetchState::NotAvailableYet;
        };
        if self.inner.library.has_match_replay(match_id) {
            return FetchState::SkippedLocal;
        }
        let validated = match validate_replay_url(match_id, &url) {
            Ok(url) => url,
            Err(_) => {
                self.transition(
                    match_id,
                    FetchState::Failed,
                    FetchReason::InvalidUrl,
                    request_count,
                );
                return FetchState::Failed;
            }
        };
        self.transition(
            match_id,
            FetchState::Downloading,
            FetchReason::DownloadStarted,
            request_count,
        );
        let staging = match self.inner.library.staging_dir() {
            Ok(path) => path,
            Err(_) => {
                self.transition(match_id, FetchState::Failed, FetchReason::DiskWrite, request_count);
                return FetchState::Failed;
            }
        };
        let (compressed_path, dem_path) = match new_stage_paths(&staging) {
            Ok(paths) => paths,
            Err(_) => {
                self.transition(match_id, FetchState::Failed, FetchReason::DiskWrite, request_count);
                return FetchState::Failed;
            }
        };
        let mut staged = StagedFiles::new(compressed_path.clone(), dem_path.clone());
        if let Err(reason) = self.disk_floor_ok(&staging, self.inner.limits.compressed) {
            self.transition(match_id, FetchState::Failed, reason, request_count);
            return FetchState::Failed;
        }
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&compressed_path)
        {
            Ok(file) => file,
            Err(_) => {
                self.transition(match_id, FetchState::Failed, FetchReason::DiskWrite, request_count);
                return FetchState::Failed;
            }
        };
        let mut compressed = 0_u64;
        let mut local_seen = false;
        let mut disk_failure = None;
        let mut last_local_check = None;
        let download = self
            .inner
            .valve
            .download(&validated, cancellation, &mut |chunk| {
                if !self.work_is_live(cancellation) {
                    return Err(TransportError::Cancelled);
                }
                if last_local_check.map_or(true, |last: Instant| {
                    last.elapsed() >= LOCAL_REPLAY_CHECK_INTERVAL
                }) {
                    last_local_check = Some(Instant::now());
                    if self.inner.library.has_match_replay(match_id) {
                        local_seen = true;
                        cancellation.cancel();
                        return Err(TransportError::Cancelled);
                    }
                }
                let next = compressed.saturating_add(chunk.len() as u64);
                if next > self.inner.limits.compressed {
                    return Err(TransportError::TooLarge);
                }
                if let Err(reason) = self.disk_floor_ok(&staging, chunk.len() as u64) {
                    disk_failure = Some(reason);
                    return Err(TransportError::TooLarge);
                }
                file.write_all(chunk)
                    .map_err(|_| TransportError::LocalFailure)?;
                compressed = next;
                Ok(())
            });
        drop(file);
        if local_seen {
            self.transition(
                match_id,
                FetchState::SkippedLocal,
                FetchReason::LocalReplayFound,
                request_count,
            );
            return FetchState::SkippedLocal;
        }
        if !self.work_is_live(cancellation) {
            self.transition(
                match_id,
                FetchState::Off,
                FetchReason::Cancelled,
                request_count,
            );
            return FetchState::Off;
        }
        match download {
            Err(TransportError::Network | TransportError::Timeout) => {
                return self.finish_api_state(match_id, FetchState::Offline, request_count);
            }
            Err(TransportError::LocalFailure) => {
                self.transition(match_id, FetchState::Failed, FetchReason::DiskWrite, request_count);
                return FetchState::Failed;
            }
            Err(TransportError::NotSent) => {
                self.transition(match_id, FetchState::Failed, FetchReason::DownloadRejected, request_count);
                return FetchState::Failed;
            }
            Err(TransportError::TooLarge) => {
                let reason = disk_failure.unwrap_or(FetchReason::DownloadRejected);
                self.transition(match_id, FetchState::Failed, reason, request_count);
                return FetchState::Failed;
            }
            Err(TransportError::Cancelled | TransportError::CancelledBeforeSend) => {
                self.transition(
                    match_id,
                    FetchState::Off,
                    FetchReason::Cancelled,
                    request_count,
                );
                return FetchState::Off;
            }
            Ok(404) => {
                self.transition(
                    match_id,
                    FetchState::Expired,
                    FetchReason::DownloadExpired,
                    request_count,
                );
                return FetchState::Expired;
            }
            Ok(200) => {}
            Ok(429) => {
                self.transition(
                    match_id,
                    FetchState::RateLimited,
                    FetchReason::RateLimitReceived,
                    request_count,
                );
                return FetchState::RateLimited;
            }
            Ok(_) => {
                self.transition(
                    match_id,
                    FetchState::Failed,
                    FetchReason::DownloadRejected,
                    request_count,
                );
                return FetchState::Failed;
            }
        }
        self.transition(
            match_id,
            FetchState::Downloading,
            FetchReason::Downloaded,
            request_count,
        );
        let compressed_sync = OpenOptions::new()
            .write(true)
            .open(&compressed_path)
            .and_then(|file| file.sync_all());
        if compressed_sync.is_err() {
            self.transition(match_id, FetchState::Failed, FetchReason::DiskWrite, request_count);
            return FetchState::Failed;
        }
        if self.inner.library.has_match_replay(match_id) {
            self.transition(
                match_id,
                FetchState::SkippedLocal,
                FetchReason::LocalReplayFound,
                request_count,
            );
            return FetchState::SkippedLocal;
        }
        if let Err(reason) =
            self.disk_floor_ok(&staging, self.inner.limits.decompressed)
        {
            self.transition(match_id, FetchState::Failed, reason, request_count);
            return FetchState::Failed;
        }
        let mut decode_local_seen = false;
        let mut last_decode_local_check = None;
        if let Err(reason) = decode_staged_zstd(
            &compressed_path,
            &dem_path,
            self.inner.limits.decompressed,
            |write_bytes| self.disk_floor_ok(&staging, write_bytes),
            || {
                if !self.work_is_live(cancellation) {
                    return true;
                }
                if last_decode_local_check.map_or(true, |last: Instant| {
                    last.elapsed() >= LOCAL_REPLAY_CHECK_INTERVAL
                }) {
                    last_decode_local_check = Some(Instant::now());
                    if self.inner.library.has_match_replay(match_id) {
                        decode_local_seen = true;
                        cancellation.cancel();
                        return true;
                    }
                }
                false
            },
        ) {
            if decode_local_seen {
                self.transition(
                    match_id,
                    FetchState::SkippedLocal,
                    FetchReason::LocalReplayFound,
                    request_count,
                );
                return FetchState::SkippedLocal;
            }
            if reason == "cancelled" || !self.work_is_live(cancellation) {
                return FetchState::Off;
            }
            let log_reason = match reason {
                "disk_unknown" => FetchReason::DiskUnknown,
                "disk_floor" => FetchReason::DiskFloor,
                "write_failed" => FetchReason::DiskWrite,
                _ => FetchReason::DecodeRejected,
            };
            self.transition(match_id, FetchState::Failed, log_reason, request_count);
            return FetchState::Failed;
        }
        if let Err(reason) = remove_compressed_stage(&mut staged) {
            self.transition(match_id, FetchState::Failed, reason, request_count);
            return FetchState::Failed;
        }
        if self.inner.library.has_match_replay(match_id) {
            self.transition(
                match_id,
                FetchState::SkippedLocal,
                FetchReason::LocalReplayFound,
                request_count,
            );
            return FetchState::SkippedLocal;
        }
        let can_queue = {
            let mut active = lock(&self.inner.active);
            if let Some(active) = active
                .as_mut()
                .filter(|active| active.match_id == match_id)
            {
                if self.work_is_live(cancellation) {
                    active.parsing = true;
                    active.dem_path = Some(dem_path.clone());
                    self.transition(
                        match_id,
                        FetchState::Parsing,
                        FetchReason::ParseQueued,
                        request_count,
                    );
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if !can_queue || !self.work_is_live(cancellation) {
            return FetchState::Off;
        }
        if self
            .inner
            .library
            .queue_fetched_dem(match_id, &dem_path)
            .is_err()
        {
            if self.work_is_live(cancellation) {
                self.transition(
                    match_id,
                    FetchState::Failed,
                    FetchReason::ParseFailed,
                    request_count,
                );
            }
            return if self.work_is_live(cancellation) {
                FetchState::Failed
            } else {
                FetchState::Off
            };
        }
        staged.dem = None; // ReplayLibrary owns deletion once the queued request is accepted.
        if !self.work_is_live(cancellation) {
            self.inner
                .library
                .cancel_queued_fetched_dem(match_id, &dem_path);
            return FetchState::Off;
        }
        let parse_started = Instant::now();
        loop {
            if self.inner.stopping.load(Ordering::Acquire) {
                return FetchState::Parsing;
            }
            if cancellation.is_cancelled() || !self.inner.enabled.load(Ordering::Acquire) {
                return FetchState::Off;
            }
            if parse_started.elapsed() >= self.inner.limits.parse_timeout {
                self.transition(
                    match_id,
                    FetchState::Failed,
                    FetchReason::ParseWaitTimeout,
                    request_count,
                );
                return FetchState::Failed;
            }
            match self.inner.library.fetched_status(match_id) {
                Some(ReplayStatus::Ready) if self.inner.library.bundle_validated(match_id) => {
                    if !self.work_is_live(cancellation) {
                        return FetchState::Off;
                    }
                    self.transition(
                        match_id,
                        FetchState::Ready,
                        FetchReason::ParseReady,
                        request_count,
                    );
                    return FetchState::Ready;
                }
                Some(ReplayStatus::Ready) => {
                    self.transition(
                        match_id,
                        FetchState::Failed,
                        FetchReason::BundleValidationFailed,
                        request_count,
                    );
                    return FetchState::Failed;
                }
                Some(
                    ReplayStatus::Failed { .. } | ReplayStatus::NeedsPack | ReplayStatus::Evicted,
                ) => {
                    self.transition(
                        match_id,
                        FetchState::Failed,
                        FetchReason::ParseFailed,
                        request_count,
                    );
                    return FetchState::Failed;
                }
                Some(ReplayStatus::Available | ReplayStatus::Queued | ReplayStatus::Parsing) => {
                    thread::sleep(FETCH_POLL_INTERVAL);
                }
                None => {
                    self.transition(
                        match_id,
                        FetchState::Failed,
                        FetchReason::ParseFailed,
                        request_count,
                    );
                    return FetchState::Failed;
                }
            }
        }
    }

    fn lookup(
        &self,
        match_id: &str,
        cancellation: &Cancellation,
        request_count: &mut u64,
    ) -> Result<HttpResponse, FetchState> {
        if !self.work_is_live(cancellation) {
            return Err(FetchState::Off);
        }
        if self.inner.library.has_match_replay(match_id) {
            return Err(FetchState::SkippedLocal);
        }
        if !self.work_is_live(cancellation) {
            return Err(FetchState::Off);
        }
        match self.inner.api.lookup(match_id, cancellation) {
            Ok(response) => {
                *request_count += 1;
                if !self.work_is_live(cancellation) {
                    return Err(FetchState::Off);
                }
                if self.inner.library.has_match_replay(match_id) {
                    return Err(FetchState::SkippedLocal);
                }
                if let Some(state) = classify_api_status(response.status) {
                    Err(self.finish_api_state(match_id, state, *request_count))
                } else {
                    Ok(response)
                }
            }
            Err(error) => {
                if !self.work_is_live(cancellation) {
                    Err(FetchState::Off)
                } else {
                    Err(self.finish_api_failure(match_id, error, *request_count))
                }
            }
        }
    }

    fn finish_api_failure(
        &self,
        match_id: &str,
        error: ReplayApiFailure,
        count: u64,
    ) -> FetchState {
        let state = match error {
            ReplayApiFailure::RateLimited | ReplayApiFailure::BudgetExhausted => {
                FetchState::RateLimited
            }
            ReplayApiFailure::Offline => FetchState::Offline,
            ReplayApiFailure::Busy => FetchState::NotAvailableYet,
            ReplayApiFailure::Cancelled if !self.inner.enabled.load(Ordering::Acquire) => {
                FetchState::Off
            }
            ReplayApiFailure::Cancelled => FetchState::Failed,
            ReplayApiFailure::Failed => FetchState::Failed,
        };
        self.finish_api_state(match_id, state, count)
    }

    fn finish_api_state(&self, match_id: &str, state: FetchState, count: u64) -> FetchState {
        let reason = match state {
            FetchState::RateLimited => FetchReason::RateLimitReceived,
            FetchState::Offline => FetchReason::NetworkUnavailable,
            FetchState::NotFound => FetchReason::LookupNotFound,
            FetchState::NotAvailableYet => FetchReason::LookupBudgetEnded,
            _ => FetchReason::DownloadRejected,
        };
        self.transition(match_id, state, reason, count);
        state
    }

    fn sleep_or_cancel(
        &self,
        match_id: &str,
        cancellation: &Cancellation,
        delay: Duration,
    ) -> SleepResult {
        let until = Instant::now() + delay;
        let mut last_local_check = None;
        while Instant::now() < until {
            if last_local_check.map_or(true, |last: Instant| {
                last.elapsed() >= LOCAL_REPLAY_CHECK_INTERVAL
            }) {
                last_local_check = Some(Instant::now());
                if self.inner.library.has_match_replay(match_id) {
                    cancellation.cancel();
                    return SleepResult::LocalReplay;
                }
            }
            if cancellation.is_cancelled()
                || !self.inner.enabled.load(Ordering::Acquire)
                || self.inner.stopping.load(Ordering::Acquire)
            {
                return SleepResult::Cancelled;
            }
            thread::sleep(FETCH_POLL_INTERVAL.min(until.saturating_duration_since(Instant::now())));
        }
        SleepResult::Continue
    }

    fn disk_floor_ok(&self, path: &Path, upcoming_write: u64) -> Result<(), FetchReason> {
        let free = self
            .inner
            .disk
            .available_bytes(path)
            .ok_or(FetchReason::DiskUnknown)?;
        let required = self
            .inner
            .limits
            .disk_floor
            .saturating_add(upcoming_write);
        if free < required {
            Err(FetchReason::DiskFloor)
        } else {
            Ok(())
        }
    }

    fn transition(&self, match_id: &str, state: FetchState, reason: FetchReason, count: u64) {
        let changed = {
            let mut current = lock(&self.inner.current);
            if current.0.as_deref() == Some(match_id) && current.1 == state {
                false
            } else {
                let now = Instant::now();
                let started = if current.0.as_deref() == Some(match_id) {
                    current.2.unwrap_or(now)
                } else {
                    now
                };
                current.0 = (!match_id.is_empty()).then(|| match_id.to_owned());
                current.1 = state;
                current.2 = Some(started);
                true
            }
        };
        if changed {
            let duration_ms = lock(&self.inner.current)
                .2
                .map(|started| u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX))
                .unwrap_or(0);
            let event = TransitionLog {
                state,
                reason,
                count,
                duration_ms,
            };
            self.inner.logger.record(event);
        }
    }
}

enum SleepResult {
    Continue,
    LocalReplay,
    Cancelled,
}

fn duration_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn classify_api_status(status: u16) -> Option<FetchState> {
    match status {
        200 | 202 | 204 => None,
        404 => Some(FetchState::NotFound),
        429 => Some(FetchState::RateLimited),
        500..=599 => Some(FetchState::Offline),
        _ => Some(FetchState::Failed),
    }
}

fn replay_url_from_response(response: &HttpResponse) -> Result<Option<String>, ()> {
    if response.status != 200 {
        return Err(());
    }
    let value: Value = serde_json::from_slice(&response.body).map_err(|_| ())?;
    let Some(value) = value.get("replay_url") else {
        return Ok(None);
    };
    if value.is_null() {
        Ok(None)
    } else {
        value.as_str().map(|url| Some(url.to_owned())).ok_or(())
    }
}

fn new_stage_paths(staging: &Path) -> Result<(PathBuf, PathBuf), std::io::Error> {
    fs::create_dir_all(staging)?;
    let serial = NEXT_STAGE_FILE.fetch_add(1, Ordering::Relaxed);
    let stem = format!("replay-fetch-{}-{serial}", std::process::id());
    Ok((
        staging.join(format!("{stem}.part")),
        staging.join(format!("{stem}.dem")),
    ))
}

struct StagedFiles {
    compressed: Option<PathBuf>,
    dem: Option<PathBuf>,
}

impl StagedFiles {
    fn new(compressed: PathBuf, dem: PathBuf) -> Self {
        Self {
            compressed: Some(compressed),
            dem: Some(dem),
        }
    }
}

impl Drop for StagedFiles {
    fn drop(&mut self) {
        for path in [&self.compressed, &self.dem].into_iter().flatten() {
            let _ = fs::remove_file(path);
        }
    }
}

fn remove_compressed_stage(staged: &mut StagedFiles) -> Result<(), FetchReason> {
    let Some(path) = staged.compressed.as_ref() else {
        return Ok(());
    };
    match fs::remove_file(path) {
        Ok(()) => staged.compressed = None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            staged.compressed = None;
        }
        Err(_) => return Err(FetchReason::CompressedCleanupFailed),
    }
    Ok(())
}

fn cleanup_interrupted_staging(staging: &Path) {
    let Ok(entries) = fs::read_dir(staging) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let _ = fs::remove_file(path);
        }
    }
}

fn decode_staged_zstd(
    compressed: &Path,
    dem: &Path,
    cap: u64,
    disk_check: impl Fn(u64) -> Result<(), FetchReason>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(), &'static str> {
    let mut source = File::open(compressed).map_err(|_| "read_failed")?;
    let mut magic = [0_u8; 4];
    source.read_exact(&mut magic).map_err(|_| "invalid_magic")?;
    if magic != [0x28, 0xb5, 0x2f, 0xfd] {
        let _ = fs::remove_file(compressed);
        let _ = fs::remove_file(dem);
        return Err("invalid_magic");
    }
    source.seek(SeekFrom::Start(0)).map_err(|_| "read_failed")?;
    let mut decoder =
        ruzstd::decoding::StreamingDecoder::new(source).map_err(|_| "decode_failed")?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dem)
        .map_err(|_| "write_failed")?;
    let mut decoded = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        if cancelled() {
            let _ = fs::remove_file(dem);
            return Err("cancelled");
        }
        let read = decoder.read(&mut buffer).map_err(|_| "decode_failed")?;
        if read == 0 {
            break;
        }
        if cancelled() {
            let _ = fs::remove_file(dem);
            return Err("cancelled");
        }
        let next = decoded.saturating_add(read as u64);
        if next > cap {
            let _ = fs::remove_file(dem);
            return Err("decompressed_too_large");
        }
        if let Err(reason) = disk_check(read as u64) {
            let _ = fs::remove_file(dem);
            return Err(reason.code());
        }
        output
            .write_all(&buffer[..read])
            .map_err(|_| "write_failed")?;
        decoded = next;
    }
    output.sync_all().map_err(|_| "write_failed")?;
    if decoded == 0 {
        let _ = fs::remove_file(dem);
        return Err("decode_failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Condvar;

    // Synthetic Zstandard frame: one final raw block containing `DEM\n`; ruzstd has no encoder.
    const TINY_SYNTHETIC_ZSTD: &[u8] = &[
        0x28, 0xb5, 0x2f, 0xfd, 0x20, 0x04, 0x21, 0x00, 0x00, b'D', b'E', b'M', b'\n',
    ];
    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn replay_url_accepts_only_the_exact_plain_http_valve_form_for_the_same_match() {
        let parsed =
            validate_replay_url("9001", "http://replay45.valve.net/570/9001_777.dem.bz2").unwrap();
        assert_eq!(parsed.host, "replay45.valve.net");
        assert_eq!(parsed.path, "/570/9001_777.dem.bz2");

        for rejected in [
            "https://replay45.valve.net/570/9001_777.dem.bz2",
            "http://replay45.attacker.example/570/9001_777.dem.bz2",
            "http://replay45.valve.net:80/570/9001_777.dem.bz2",
            "http://replay45.valve.net/570/9002_777.dem.bz2",
            "http://replay45.valve.net/570/9001_777.dem.bz2?next=evil",
            "http://replay45.valve.net/570/9001_777.dem.bz2/extra",
            "http://replay123456789012345678901.valve.net/570/9001_777.dem.bz2",
            "http://replay45.valve.net/570/9001_123456789012345678901.dem.bz2",
        ] {
            assert!(validate_replay_url("9001", rejected).is_err(), "{rejected}");
        }
        assert!(validate_replay_url(
            "123456789012345678901",
            "http://replay45.valve.net/570/123456789012345678901_777.dem.bz2"
        )
        .is_err());
    }

    fn test_dir() -> PathBuf {
        let index = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kohbor-replay-autofetch-{}-{index}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn response(status: u16, body: &[u8]) -> HttpResponse {
        HttpResponse {
            status,
            retry_after: None,
            rate_limit_remaining_minute: None,
            rate_limit_remaining_day: None,
            body: body.to_vec(),
        }
    }

    fn lookup_url(url: &str) -> HttpResponse {
        response(
            200,
            serde_json::to_string(&serde_json::json!({"replay_url": url}))
                .unwrap()
                .as_bytes(),
        )
    }

    fn no_url() -> HttpResponse {
        response(200, br#"{"match_id":9001,"replay_url":null}"#)
    }

    struct FakeApi {
        lookups: Mutex<VecDeque<Result<HttpResponse, ReplayApiFailure>>>,
        requests: Mutex<VecDeque<Result<HttpResponse, ReplayApiFailure>>>,
        get_count: AtomicUsize,
        post_count: AtomicUsize,
        sent_ids: Mutex<Vec<String>>,
    }

    impl FakeApi {
        fn new(
            lookups: impl IntoIterator<Item = Result<HttpResponse, ReplayApiFailure>>,
            requests: impl IntoIterator<Item = Result<HttpResponse, ReplayApiFailure>>,
        ) -> Self {
            Self {
                lookups: Mutex::new(lookups.into_iter().collect()),
                requests: Mutex::new(requests.into_iter().collect()),
                get_count: AtomicUsize::new(0),
                post_count: AtomicUsize::new(0),
                sent_ids: Mutex::new(Vec::new()),
            }
        }
    }

    impl OpenDotaReplayApi for FakeApi {
        fn lookup(
            &self,
            match_id: &str,
            _cancellation: &Cancellation,
        ) -> Result<HttpResponse, ReplayApiFailure> {
            self.get_count.fetch_add(1, Ordering::Relaxed);
            lock(&self.sent_ids).push(match_id.to_owned());
            lock(&self.lookups)
                .pop_front()
                .unwrap_or(Err(ReplayApiFailure::Failed))
        }
        fn request_replay(
            &self,
            match_id: &str,
            _cancellation: &Cancellation,
        ) -> Result<HttpResponse, ReplayApiFailure> {
            self.post_count.fetch_add(1, Ordering::Relaxed);
            lock(&self.sent_ids).push(match_id.to_owned());
            lock(&self.requests)
                .pop_front()
                .unwrap_or(Err(ReplayApiFailure::Failed))
        }
        fn cancel(&self, cancellation: &Cancellation) {
            cancellation.cancel();
        }
    }

    struct FakeLibrary {
        root: PathBuf,
        local: AtomicBool,
        status: Mutex<Option<ReplayStatus>>,
        status_on_queue: Mutex<ReplayStatus>,
        bundle_is_valid: AtomicBool,
        local_checks: AtomicUsize,
        queued: Mutex<Vec<PathBuf>>,
        contents: Mutex<Vec<Vec<u8>>>,
        watched_folder: Option<PathBuf>,
    }

    impl FakeLibrary {
        fn new(root: PathBuf, watched_folder: Option<PathBuf>) -> Self {
            fs::create_dir_all(root.join(".staging").join("autofetch")).unwrap();
            Self {
                root,
                local: AtomicBool::new(false),
                status: Mutex::new(None),
                status_on_queue: Mutex::new(ReplayStatus::Ready),
                bundle_is_valid: AtomicBool::new(true),
                local_checks: AtomicUsize::new(0),
                queued: Mutex::new(Vec::new()),
                contents: Mutex::new(Vec::new()),
                watched_folder,
            }
        }
        fn staging(&self) -> PathBuf {
            self.root.join(".staging").join("autofetch")
        }
    }

    impl ReplayLibraryPort for FakeLibrary {
        fn staging_dir(&self) -> Result<PathBuf, &'static str> {
            fs::create_dir_all(self.staging()).map_err(|_| "storage_unavailable")?;
            Ok(self.staging())
        }
        fn has_match_replay(&self, match_id: &str) -> bool {
            self.local_checks.fetch_add(1, Ordering::Relaxed);
            self.local.load(Ordering::Acquire)
                || self
                    .watched_folder
                    .as_ref()
                    .is_some_and(|folder| folder.join(format!("{match_id}.dem")).is_file())
        }
        fn queue_fetched_dem(&self, _match_id: &str, path: &Path) -> Result<(), &'static str> {
            let bytes = fs::read(path).map_err(|_| "invalid_input")?;
            lock(&self.contents).push(bytes);
            lock(&self.queued).push(path.to_path_buf());
            *lock(&self.status) = Some(lock(&self.status_on_queue).clone());
            fs::remove_file(path).map_err(|_| "storage_unavailable")?;
            Ok(())
        }
        fn cancel_queued_fetched_dem(&self, _match_id: &str, _path: &Path) -> bool {
            if matches!(*lock(&self.status), Some(ReplayStatus::Queued)) {
                *lock(&self.status) = Some(ReplayStatus::Failed {
                    reason: "cancelled".to_owned(),
                });
                true
            } else {
                false
            }
        }
        fn fetched_status(&self, _match_id: &str) -> Option<ReplayStatus> {
            lock(&self.status).clone()
        }
        fn bundle_validated(&self, _match_id: &str) -> bool {
            self.bundle_is_valid.load(Ordering::Acquire)
                && matches!(*lock(&self.status), Some(ReplayStatus::Ready))
        }
    }

    struct FakeDisk(Option<u64>);
    impl DiskSpaceProbe for FakeDisk {
        fn available_bytes(&self, _path: &Path) -> Option<u64> {
            self.0
        }
    }

    #[derive(Default)]
    struct FakeLogger(
        Mutex<Vec<TransitionLog>>,
        Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    );
    impl TransitionSink for FakeLogger {
        fn record(&self, event: TransitionLog) {
            lock(&self.0).push(event);
            if event.state == FetchState::LookingUp {
                let hook = lock(&self.1).take();
                if let Some(hook) = hook {
                    hook();
                }
            }
        }
    }

    #[derive(Default)]
    struct DownloadGate {
        entered: Mutex<bool>,
        release: Mutex<bool>,
        changed: Condvar,
    }

    impl DownloadGate {
        fn wait_until_entered(&self) {
            let mut entered = lock(&self.entered);
            while !*entered {
                entered = self
                    .changed
                    .wait(entered)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
        fn pause_after_first_chunk(&self) {
            *lock(&self.entered) = true;
            self.changed.notify_all();
            let mut release = lock(&self.release);
            while !*release {
                release = self
                    .changed
                    .wait(release)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
        fn release(&self) {
            *lock(&self.release) = true;
            self.changed.notify_all();
        }
    }

    struct FakeValve {
        status: u16,
        chunks: Vec<Vec<u8>>,
        after_first: Option<Arc<dyn Fn() + Send + Sync>>,
        gate: Option<Arc<DownloadGate>>,
    }

    impl FakeValve {
        fn body(body: &[u8]) -> Self {
            Self {
                status: 200,
                chunks: vec![body.to_vec()],
                after_first: None,
                gate: None,
            }
        }
    }

    impl ValveReplayTransport for FakeValve {
        fn download(
            &self,
            _url: &ValidatedReplayUrl,
            cancellation: &Cancellation,
            sink: &mut dyn FnMut(&[u8]) -> Result<(), TransportError>,
        ) -> Result<u16, TransportError> {
            if self.status != 200 {
                return Ok(self.status);
            }
            for (index, chunk) in self.chunks.iter().enumerate() {
                if cancellation.is_cancelled() {
                    return Err(TransportError::Cancelled);
                }
                sink(chunk)?;
                if index == 0 {
                    if let Some(hook) = &self.after_first {
                        hook();
                    }
                    if let Some(gate) = &self.gate {
                        gate.pause_after_first_chunk();
                    }
                }
            }
            Ok(self.status)
        }
        fn cancel(&self, cancellation: &Cancellation) {
            cancellation.cancel();
        }
    }

    fn limits(compressed: u64, decompressed: u64) -> Limits {
        Limits {
            compressed,
            decompressed,
            disk_floor: 1,
            poll_schedule: [Duration::ZERO; 4],
            parse_timeout: Duration::from_millis(50),
        }
    }

    fn fetcher(
        _root: PathBuf,
        enabled: bool,
        api: Arc<FakeApi>,
        valve: Arc<FakeValve>,
        library: Arc<FakeLibrary>,
        limits: Limits,
        logger: Arc<FakeLogger>,
    ) -> ReplayAutoFetch {
        ReplayAutoFetch::open_with_ports(
            library,
            api,
            valve,
            Arc::new(FakeDisk(Some(300 * 1024 * 1024 * 1024))),
            enabled,
            Arc::new(|| false),
            limits,
            logger,
        )
    }

    fn default_success_parts(
        root: &Path,
    ) -> (
        Arc<FakeApi>,
        Arc<FakeValve>,
        Arc<FakeLibrary>,
        Arc<FakeLogger>,
    ) {
        let api = Arc::new(FakeApi::new(
            [Ok(lookup_url(
                "http://replay45.valve.net/570/9001_777.dem.bz2",
            ))],
            [],
        ));
        let valve = Arc::new(FakeValve::body(TINY_SYNTHETIC_ZSTD));
        let library = Arc::new(FakeLibrary::new(root.to_path_buf(), None));
        let logger = Arc::new(FakeLogger::default());
        (api, valve, library, logger)
    }

    #[test]
    fn switch_off_makes_no_request() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new([], []));
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        let logger = Arc::new(FakeLogger::default());
        let service = fetcher(
            root.clone(),
            false,
            api.clone(),
            Arc::new(FakeValve::body(&[])),
            library,
            limits(64, 64),
            logger,
        );
        service.attempt_match("9001");
        assert_eq!(api.get_count.load(Ordering::Relaxed), 0);
        assert_eq!(api.post_count.load(Ordering::Relaxed), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn switch_off_after_active_work_is_published_skips_the_first_get() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new([Ok(no_url())], []));
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        let logger = Arc::new(FakeLogger::default());
        let service_slot = Arc::new(Mutex::new(None::<ReplayAutoFetch>));
        let off_slot = service_slot.clone();
        *lock(&logger.1) = Some(Arc::new(move || {
            if let Some(service) = lock(&off_slot).take() {
                service.set_enabled(false);
            }
        }));
        let service = fetcher(
            root.clone(),
            true,
            api.clone(),
            Arc::new(FakeValve::body(&[])),
            library,
            limits(64, 64),
            logger,
        );
        *lock(&service_slot) = Some(service.clone());
        service.attempt_match("9001");
        assert_eq!(api.get_count.load(Ordering::Relaxed), 0);
        assert_eq!(api.post_count.load(Ordering::Relaxed), 0);
        assert_eq!(service.state_for(Some("9001")), FetchState::Off);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn disk_floor_is_uniform_for_drive_d_and_unc_paths_and_unknown_space_fails_closed() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new([], []));
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        let logger = Arc::new(FakeLogger::default());
        let uniform = ReplayAutoFetch::open_with_ports(
            library.clone(),
            api.clone(),
            Arc::new(FakeValve::body(&[])),
            Arc::new(FakeDisk(Some(17))),
            true,
            Arc::new(|| false),
            Limits {
                compressed: 16,
                decompressed: 16,
                disk_floor: 1,
                poll_schedule: [Duration::ZERO; 4],
                parse_timeout: Duration::from_millis(10),
            },
            logger.clone(),
        );
        assert!(uniform.disk_floor_ok(Path::new(r"D:\replays"), 16).is_ok());
        assert!(uniform
            .disk_floor_ok(Path::new(r"\\server\share\replays"), 16)
            .is_ok());

        let api = Arc::new(FakeApi::new(
            [Ok(lookup_url(
                "http://replay45.valve.net/570/9001_777.dem.bz2",
            ))],
            [],
        ));
        let unknown = ReplayAutoFetch::open_with_ports(
            library,
            api,
            Arc::new(FakeValve::body(TINY_SYNTHETIC_ZSTD)),
            Arc::new(FakeDisk(None)),
            true,
            Arc::new(|| false),
            limits(64, 64),
            logger.clone(),
        );
        unknown.attempt_match("9001");
        assert_eq!(unknown.state_for(Some("9001")), FetchState::Failed);
        assert!(serde_json::to_string(&*lock(&logger.0))
            .unwrap()
            .contains("disk_unknown"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn disk_floor_reserves_the_full_cap_before_download_and_decode() {
        let root = test_dir();
        let (api, valve, library, logger) = default_success_parts(&root);
        let before_download = ReplayAutoFetch::open_with_ports(
            library.clone(),
            api,
            valve,
            Arc::new(FakeDisk(Some(64))),
            true,
            Arc::new(|| false),
            limits(64, 64),
            logger.clone(),
        );
        before_download.attempt_match("9001");
        assert_eq!(before_download.state_for(Some("9001")), FetchState::Failed);
        assert!(serde_json::to_string(&*lock(&logger.0))
            .unwrap()
            .contains("disk_floor"));
        assert!(fs::read_dir(library.staging()).unwrap().next().is_none());

        let (api, valve, library, logger) = default_success_parts(&root);
        let before_decode = ReplayAutoFetch::open_with_ports(
            library.clone(),
            api,
            valve,
            Arc::new(FakeDisk(Some(33))),
            true,
            Arc::new(|| false),
            Limits {
                compressed: 32,
                decompressed: 100,
                disk_floor: 1,
                poll_schedule: [Duration::ZERO; 4],
                parse_timeout: Duration::from_millis(10),
            },
            logger.clone(),
        );
        before_decode.attempt_match("9001");
        assert_eq!(before_decode.state_for(Some("9001")), FetchState::Failed);
        assert!(serde_json::to_string(&*lock(&logger.0))
            .unwrap()
            .contains("disk_floor"));
        assert!(fs::read_dir(library.staging()).unwrap().next().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn decode_cancellation_removes_the_partial_dem_and_staging_guard_removes_both_files() {
        let root = test_dir();
        let compressed = root.join("cancel.part");
        let dem = root.join("cancel.dem");
        fs::write(&compressed, TINY_SYNTHETIC_ZSTD).unwrap();
        let cancellation = Cancellation::default();
        let result = decode_staged_zstd(
            &compressed,
            &dem,
            64,
            |_| {
                cancellation.cancel();
                Ok(())
            },
            || cancellation.is_cancelled(),
        );
        assert_eq!(result, Err("cancelled"));
        assert!(!dem.exists());
        assert!(compressed.exists());
        drop(StagedFiles::new(compressed.clone(), dem.clone()));
        assert!(!dem.exists());
        assert!(!compressed.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_compressed_cleanup_keeps_the_path_for_the_staging_guard_retry() {
        let root = test_dir();
        let compressed = root.join("compressed.part");
        let dem = root.join("decoded.dem");
        fs::create_dir(&compressed).unwrap();
        fs::write(compressed.join("held"), b"synthetic").unwrap();
        let mut staged = StagedFiles::new(compressed.clone(), dem);
        assert_eq!(
            remove_compressed_stage(&mut staged),
            Err(FetchReason::CompressedCleanupFailed)
        );
        assert_eq!(staged.compressed.as_deref(), Some(compressed.as_path()));
        fs::remove_dir_all(&compressed).unwrap();
        drop(staged);
        assert!(!compressed.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn present_url_uses_one_lookup_and_queues_a_validated_dem() {
        let root = test_dir();
        let (api, valve, library, logger) = default_success_parts(&root);
        let service = fetcher(
            root.clone(),
            true,
            api.clone(),
            valve,
            library.clone(),
            limits(64, 64),
            logger,
        );
        service.attempt_match("9001");
        assert_eq!(api.get_count.load(Ordering::Relaxed), 1);
        assert_eq!(api.post_count.load(Ordering::Relaxed), 0);
        assert_eq!(service.state_for(Some("9001")), FetchState::Ready);
        assert_eq!(lock(&library.contents).as_slice(), &[b"DEM\n".to_vec()]);
        assert_eq!(lock(&library.queued).len(), 1);
        assert!(!lock(&library.queued)[0].exists());
        assert!(fs::read_dir(library.staging()).unwrap().next().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_url_posts_once_then_polls_within_six_call_budget() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new(
            [
                Ok(no_url()),
                Ok(no_url()),
                Ok(lookup_url("http://replay45.valve.net/570/9001_777.dem.bz2")),
            ],
            [Ok(response(202, b"{}"))],
        ));
        let valve = Arc::new(FakeValve::body(TINY_SYNTHETIC_ZSTD));
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        let logger = Arc::new(FakeLogger::default());
        let service = fetcher(
            root.clone(),
            true,
            api.clone(),
            valve,
            library,
            limits(64, 64),
            logger,
        );
        service.attempt_match("9001");
        assert_eq!(api.get_count.load(Ordering::Relaxed), 3);
        assert_eq!(api.post_count.load(Ordering::Relaxed), 1);
        assert_eq!(
            api.get_count.load(Ordering::Relaxed) + api.post_count.load(Ordering::Relaxed),
            4
        );
        assert_eq!(service.state_for(Some("9001")), FetchState::Ready);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_url_after_bounded_budget_becomes_not_available_yet() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new(
            [
                Ok(no_url()),
                Ok(no_url()),
                Ok(no_url()),
                Ok(no_url()),
                Ok(no_url()),
            ],
            [Ok(response(202, b"{}"))],
        ));
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        let logger = Arc::new(FakeLogger::default());
        let service = fetcher(
            root.clone(),
            true,
            api.clone(),
            Arc::new(FakeValve::body(&[])),
            library,
            limits(64, 64),
            logger,
        );
        service.attempt_match("9001");
        assert_eq!(service.state_for(Some("9001")), FetchState::NotAvailableYet);
        assert_eq!(api.get_count.load(Ordering::Relaxed), 5);
        assert_eq!(api.post_count.load(Ordering::Relaxed), 1);
        assert_eq!(
            api.get_count.load(Ordering::Relaxed) + api.post_count.load(Ordering::Relaxed),
            6
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parse_wait_fails_on_deadline_and_allows_a_later_match() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new(
            [
                Ok(lookup_url(
                    "http://replay45.valve.net/570/9001_777.dem.bz2",
                )),
                Ok(lookup_url(
                    "http://replay45.valve.net/570/9002_777.dem.bz2",
                )),
            ],
            [],
        ));
        let valve = Arc::new(FakeValve::body(TINY_SYNTHETIC_ZSTD));
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        *lock(&library.status_on_queue) = ReplayStatus::Queued;
        let logger = Arc::new(FakeLogger::default());
        let mut test_limits = limits(64, 64);
        test_limits.parse_timeout = Duration::from_millis(5);
        let service = fetcher(
            root.clone(),
            true,
            api.clone(),
            valve,
            library.clone(),
            test_limits,
            logger,
        );
        service.attempt_match("9001");
        assert_eq!(service.state_for(Some("9001")), FetchState::Failed);
        *lock(&library.status_on_queue) = ReplayStatus::Ready;
        service.attempt_match("9002");
        assert_eq!(api.get_count.load(Ordering::Relaxed), 2);
        assert_eq!(service.state_for(Some("9002")), FetchState::Ready);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ready_parse_with_invalid_bundle_becomes_failed() {
        let root = test_dir();
        let (api, valve, library, logger) = default_success_parts(&root);
        library.bundle_is_valid.store(false, Ordering::Release);
        let service = fetcher(
            root.clone(),
            true,
            api,
            valve,
            library,
            limits(64, 64),
            logger.clone(),
        );
        service.attempt_match("9001");
        assert_eq!(service.state_for(Some("9001")), FetchState::Failed);
        assert!(serde_json::to_string(&*lock(&logger.0))
            .unwrap()
            .contains("bundle_validation_failed"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_staging_write_setup_failure_is_failed_with_a_disk_reason() {
        let root = test_dir();
        let (api, valve, library, logger) = default_success_parts(&root);
        let staging = library.staging();
        fs::remove_dir_all(&staging).unwrap();
        fs::write(&staging, b"not a directory").unwrap();
        let service = fetcher(
            root.clone(),
            true,
            api,
            valve,
            library,
            limits(64, 64),
            logger.clone(),
        );
        service.attempt_match("9001");
        assert_eq!(service.state_for(Some("9001")), FetchState::Failed);
        assert!(serde_json::to_string(&*lock(&logger.0))
            .unwrap()
            .contains("disk_write"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn not_found_rate_limited_and_offline_map_to_explicit_states() {
        let cases = [
            (Ok(response(404, b"{}")), FetchState::NotFound),
            (Ok(response(429, b"{}")), FetchState::RateLimited),
            (Err(ReplayApiFailure::Offline), FetchState::Offline),
        ];
        for (result, expected) in cases {
            let root = test_dir();
            let api = Arc::new(FakeApi::new([result], []));
            let library = Arc::new(FakeLibrary::new(root.clone(), None));
            let logger = Arc::new(FakeLogger::default());
            let service = fetcher(
                root.clone(),
                true,
                api,
                Arc::new(FakeValve::body(&[])),
                library,
                limits(64, 64),
                logger,
            );
            service.attempt_match("9001");
            assert_eq!(service.state_for(Some("9001")), expected);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn oversized_compressed_and_decompressed_bodies_fail_and_leave_no_staging_file() {
        for (body, caps) in [
            (TINY_SYNTHETIC_ZSTD, limits(8, 64)),
            (TINY_SYNTHETIC_ZSTD, limits(64, 3)),
        ] {
            let root = test_dir();
            let (api, valve, library, logger) = default_success_parts(&root);
            let service = fetcher(
                root.clone(),
                true,
                api,
                valve,
                library.clone(),
                caps,
                logger,
            );
            service.attempt_match("9001");
            assert_eq!(service.state_for(Some("9001")), FetchState::Failed);
            assert!(fs::read_dir(library.staging()).unwrap().next().is_none());
            let _ = body;
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn non_zstandard_magic_is_rejected_and_deleted() {
        for body in [b"BZh9synthetic".as_slice(), b"not-a-frame".as_slice()] {
            let root = test_dir();
            let (api, _, library, logger) = default_success_parts(&root);
            let service = fetcher(
                root.clone(),
                true,
                api,
                Arc::new(FakeValve::body(body)),
                library.clone(),
                limits(64, 64),
                logger,
            );
            service.attempt_match("9001");
            assert_eq!(service.state_for(Some("9001")), FetchState::Failed);
            assert!(fs::read_dir(library.staging()).unwrap().next().is_none());
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn valve_redirect_is_rejected_without_following_it() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new(
            [Ok(lookup_url(
                "http://replay45.valve.net/570/9001_777.dem.bz2",
            ))],
            [],
        ));
        let valve = Arc::new(FakeValve {
            status: 302,
            chunks: vec![],
            after_first: None,
            gate: None,
        });
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        let logger = Arc::new(FakeLogger::default());
        let service = fetcher(
            root.clone(),
            true,
            api,
            valve,
            library.clone(),
            limits(64, 64),
            logger,
        );
        service.attempt_match("9001");
        assert_eq!(service.state_for(Some("9001")), FetchState::Failed);
        assert!(fs::read_dir(library.staging()).unwrap().next().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn expired_valve_replay_maps_to_expired() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new(
            [Ok(lookup_url(
                "http://replay45.valve.net/570/9001_777.dem.bz2",
            ))],
            [],
        ));
        let valve = Arc::new(FakeValve {
            status: 404,
            chunks: vec![],
            after_first: None,
            gate: None,
        });
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        let logger = Arc::new(FakeLogger::default());
        let service = fetcher(
            root.clone(),
            true,
            api,
            valve,
            library,
            limits(64, 64),
            logger,
        );
        service.attempt_match("9001");
        assert_eq!(service.state_for(Some("9001")), FetchState::Expired);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watched_folder_replay_that_appears_during_download_cancels_and_wins() {
        let root = test_dir();
        let watched = root.join("dota-replays");
        fs::create_dir_all(&watched).unwrap();
        let library = Arc::new(FakeLibrary::new(root.clone(), Some(watched.clone())));
        let api = Arc::new(FakeApi::new(
            [Ok(lookup_url(
                "http://replay45.valve.net/570/9001_777.dem.bz2",
            ))],
            [],
        ));
        let watched_file = watched.join("9001.dem");
        let create_watched = Arc::new(move || {
            fs::write(&watched_file, b"local").unwrap();
        });
        let valve = Arc::new(FakeValve {
            status: 200,
            chunks: vec![
                TINY_SYNTHETIC_ZSTD[..6].to_vec(),
                TINY_SYNTHETIC_ZSTD[6..].to_vec(),
            ],
            after_first: Some(create_watched),
            gate: None,
        });
        let logger = Arc::new(FakeLogger::default());
        let service = fetcher(
            root.clone(),
            true,
            api,
            valve,
            library.clone(),
            limits(64, 64),
            logger,
        );
        service.attempt_match("9001");
        assert_eq!(service.state_for(Some("9001")), FetchState::SkippedLocal);
        assert!(lock(&library.queued).is_empty());
        assert!(fs::read_dir(library.staging()).unwrap().next().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_replay_probe_during_download_is_throttled_to_once_per_second() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new(
            [Ok(lookup_url(
                "http://replay45.valve.net/570/9001_777.dem.bz2",
            ))],
            [],
        ));
        let chunks = TINY_SYNTHETIC_ZSTD.iter().map(|byte| vec![*byte]).collect();
        let valve = Arc::new(FakeValve {
            status: 200,
            chunks,
            after_first: None,
            gate: None,
        });
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        let logger = Arc::new(FakeLogger::default());
        let service = fetcher(
            root.clone(),
            true,
            api,
            valve,
            library.clone(),
            limits(64, 64),
            logger,
        );
        service.attempt_match("9001");
        assert_eq!(service.state_for(Some("9001")), FetchState::Ready);
        assert!(library.local_checks.load(Ordering::Relaxed) <= 10);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn existing_library_or_watched_dem_skips_open_dota_lookup() {
        let root = test_dir();
        let watched = root.join("dota-replays");
        fs::create_dir_all(&watched).unwrap();
        fs::write(watched.join("9001.dem"), b"local replay").unwrap();
        let api = Arc::new(FakeApi::new([], []));
        let library = Arc::new(FakeLibrary::new(root.clone(), Some(watched)));
        let logger = Arc::new(FakeLogger::default());
        let service = fetcher(
            root.clone(),
            true,
            api.clone(),
            Arc::new(FakeValve::body(&[])),
            library,
            limits(64, 64),
            logger,
        );
        service.attempt_match("9001");
        assert_eq!(service.state_for(Some("9001")), FetchState::SkippedLocal);
        assert_eq!(api.get_count.load(Ordering::Relaxed), 0);
        assert_eq!(api.post_count.load(Ordering::Relaxed), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn switch_off_mid_download_cancels_and_deletes_staging_but_parse_is_not_cancelled_here() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new(
            [Ok(lookup_url(
                "http://replay45.valve.net/570/9001_777.dem.bz2",
            ))],
            [],
        ));
        let gate = Arc::new(DownloadGate::default());
        let valve = Arc::new(FakeValve {
            status: 200,
            chunks: vec![
                TINY_SYNTHETIC_ZSTD[..6].to_vec(),
                TINY_SYNTHETIC_ZSTD[6..].to_vec(),
            ],
            after_first: None,
            gate: Some(gate.clone()),
        });
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        let logger = Arc::new(FakeLogger::default());
        let service = fetcher(
            root.clone(),
            true,
            api,
            valve,
            library.clone(),
            limits(64, 64),
            logger,
        );
        let worker = service.clone();
        let join = thread::spawn(move || worker.attempt_match("9001"));
        gate.wait_until_entered();
        service.set_enabled(false);
        gate.release();
        join.join().unwrap();
        assert_eq!(service.state_for(Some("9001")), FetchState::Off);
        assert!(fs::read_dir(library.staging()).unwrap().next().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn switch_off_during_a_running_parse_keeps_fetch_state_off() {
        let root = test_dir();
        let api = Arc::new(FakeApi::new(
            [Ok(lookup_url(
                "http://replay45.valve.net/570/9001_777.dem.bz2",
            ))],
            [],
        ));
        let valve = Arc::new(FakeValve::body(TINY_SYNTHETIC_ZSTD));
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        *lock(&library.status_on_queue) = ReplayStatus::Parsing;
        let logger = Arc::new(FakeLogger::default());
        let service = fetcher(
            root.clone(),
            true,
            api,
            valve,
            library.clone(),
            limits(64, 64),
            logger,
        );
        let worker = service.clone();
        let join = thread::spawn(move || worker.attempt_match("9001"));
        let deadline = Instant::now() + Duration::from_secs(2);
        while service.state_for(Some("9001")) != FetchState::Parsing && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(service.state_for(Some("9001")), FetchState::Parsing);
        service.set_enabled(false);
        assert_eq!(service.state_for(Some("9001")), FetchState::Off);
        *lock(&library.status) = Some(ReplayStatus::Ready);
        join.join().unwrap();
        assert_eq!(service.state_for(Some("9001")), FetchState::Off);
        assert!(lock(&library.queued).len() == 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_removes_interrupted_staging_files() {
        let root = test_dir();
        let library = Arc::new(FakeLibrary::new(root.clone(), None));
        fs::write(library.staging().join("old.part"), b"partial").unwrap();
        fs::write(library.staging().join("old.dem"), b"partial").unwrap();
        let api = Arc::new(FakeApi::new([], []));
        let logger = Arc::new(FakeLogger::default());
        let _service = fetcher(
            root.clone(),
            false,
            api,
            Arc::new(FakeValve::body(&[])),
            library.clone(),
            limits(64, 64),
            logger,
        );
        assert!(fs::read_dir(library.staging()).unwrap().next().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn transition_log_serialization_contains_no_match_id_url_salt_or_path() {
        let root = test_dir();
        let (api, valve, library, logger) = default_success_parts(&root);
        let service = fetcher(
            root.clone(),
            true,
            api,
            valve,
            library,
            limits(64, 64),
            logger.clone(),
        );
        service.attempt_match("9001");
        let serialized = serde_json::to_string(&*lock(&logger.0)).unwrap();
        for private in [
            "9001",
            "replay45.valve.net",
            "777",
            root.to_string_lossy().as_ref(),
        ] {
            assert!(!serialized.contains(private), "{private}");
        }
        assert!(serialized.contains("duration_ms"));
        assert!(serialized.contains("count"));
        let _ = fs::remove_dir_all(root);
    }
}
