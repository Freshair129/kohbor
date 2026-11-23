//! Trusted updater policy and state machine.
//!
//! The plugin backend is compiled only for `--features updater`. Renderer input never supplies
//! an endpoint, signing key, download URL, or install path.
#![cfg_attr(not(feature = "updater"), allow(dead_code))]

use serde_json::{Map, Value, json};
use std::cmp::Ordering;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};

pub const BUILD_WITHOUT_UPDATER: &str = "BUILD_WITHOUT_UPDATER";
pub const NO_ENDPOINT: &str = "NO_ENDPOINT";
pub const NO_PUBKEY: &str = "NO_PUBKEY";
pub const MATCH_ACTIVE: &str = "MATCH_ACTIVE";
pub const NETWORK: &str = "NETWORK";
pub const SIGNATURE: &str = "SIGNATURE";
pub const BACKEND_UNAVAILABLE: &str = "BACKEND_UNAVAILABLE";
pub const BAD_RESPONSE: &str = "BAD_RESPONSE";

// Trusted updater origin and key supplied by the product owner. The renderer cannot override either.
pub const UPDATE_ENDPOINT: Option<&str> =
    Some("https://github.com/Freshair129/kohbor-updates/releases/latest/download/latest.json");
pub const UPDATE_PUBKEY: Option<&str> = Some(
    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQ2RjBDRDgzQjI5RDQ2NzcKUldSM1JwMnlnODN3UmxvTlNiRnplNDlsV3hCVE5iN0ZqUWoyZWdscXVHQVZZRGEyNkU5NTUvSGUK",
);

/// Owner decision: the fallback only opens the fixed release page. The renderer cannot select a URL.
pub const RELEASE_PAGE_URL: &str = "https://github.com/Freshair129/kohbor/releases/latest";

/// Reply of the open-release-page action. Refused (and the opener is not called) during a match.
pub fn open_release_page_reply(match_active: bool, opener: impl FnOnce(&str) -> bool) -> Value {
    if match_active {
        return json!({"opened": false, "reason": MATCH_ACTIVE, "url": null});
    }
    if opener(RELEASE_PAGE_URL) {
        json!({"opened": true, "reason": null, "url": RELEASE_PAGE_URL})
    } else {
        json!({"opened": false, "reason": "OPEN_FAILED", "url": RELEASE_PAGE_URL})
    }
}

const MAX_NOTES_CHARS: usize = 2_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    /// Kept as a string and rendered as text by React; it is never interpreted as HTML.
    pub notes: String,
    pub pub_date: Option<String>,
}

impl UpdateInfo {
    pub fn new(
        version: impl Into<String>,
        notes: impl Into<String>,
        pub_date: Option<String>,
    ) -> Self {
        let mut info = Self {
            version: version.into(),
            notes: notes.into(),
            pub_date,
        };
        info.truncate_notes();
        info
    }

    fn truncate_notes(&mut self) {
        if let Some((end, _)) = self.notes.char_indices().nth(MAX_NOTES_CHARS) {
            self.notes.truncate(end);
        }
    }
}

pub type BackendResult<T> = Result<T, &'static str>;
pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type UpdateProgress<'a> = Arc<dyn Fn(u8) + Send + Sync + 'a>;

/// Three distinct backend operations preserve the user's separate check, download, and install actions.
pub trait UpdateBackend: Send + Sync {
    fn check(&self) -> BackendFuture<'_, BackendResult<Option<UpdateInfo>>>;
    fn download<'a>(
        &'a self,
        info: &'a UpdateInfo,
        progress: UpdateProgress<'a>,
    ) -> BackendFuture<'a, BackendResult<()>>;
    fn install(&self, info: &UpdateInfo) -> BackendResult<()>;
}

#[derive(Clone, Copy, Debug, Default)]
#[cfg(any(not(feature = "updater"), test))]
pub struct NullBackend;

#[cfg(any(not(feature = "updater"), test))]
impl UpdateBackend for NullBackend {
    fn check(&self) -> BackendFuture<'_, BackendResult<Option<UpdateInfo>>> {
        Box::pin(async { Err(BACKEND_UNAVAILABLE) })
    }

    fn download<'a>(
        &'a self,
        _info: &'a UpdateInfo,
        _progress: UpdateProgress<'a>,
    ) -> BackendFuture<'a, BackendResult<()>> {
        Box::pin(async { Err(BACKEND_UNAVAILABLE) })
    }

    fn install(&self, _info: &UpdateInfo) -> BackendResult<()> {
        Err(BACKEND_UNAVAILABLE)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    NotConfigured(&'static str),
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading { info: UpdateInfo, progress: u8 },
    Ready(UpdateInfo),
    Installing,
    Failed(&'static str),
}

struct UpdateData {
    state: UpdateState,
}

/// Thread-safe controller. Backend operations never hold the state lock across an await, so the UI
/// can read `checking` and download progress while the plugin is waiting on the network.
pub struct UpdateController<B: UpdateBackend> {
    endpoint: Option<&'static str>,
    pubkey: Option<&'static str>,
    backend: B,
    current_version: &'static str,
    updater_enabled: bool,
    data: Mutex<UpdateData>,
}

impl<B: UpdateBackend> UpdateController<B> {
    pub fn new(
        endpoint: Option<&'static str>,
        pubkey: Option<&'static str>,
        backend: B,
        current_version: &'static str,
    ) -> Self {
        Self::new_with_feature(
            endpoint,
            pubkey,
            backend,
            current_version,
            cfg!(feature = "updater"),
        )
    }

    fn new_with_feature(
        endpoint: Option<&'static str>,
        pubkey: Option<&'static str>,
        backend: B,
        current_version: &'static str,
        updater_enabled: bool,
    ) -> Self {
        let state = match configuration_reason(updater_enabled, endpoint, pubkey) {
            Some(reason) => UpdateState::NotConfigured(reason),
            None => UpdateState::Idle,
        };
        Self {
            endpoint,
            pubkey,
            backend,
            current_version,
            updater_enabled,
            data: Mutex::new(UpdateData { state }),
        }
    }

    pub fn status(&self, match_active: bool) -> Value {
        self.reply(match_active, None)
    }

    /// A check never downloads. The result is only offered when its SemVer is strictly newer.
    pub async fn check(&self, match_active: bool) -> Value {
        if match_active {
            return self.reply(true, Some(MATCH_ACTIVE));
        }
        if let Some(reason) = self.configuration_reason() {
            self.transition(UpdateState::NotConfigured(reason));
            return self.status(false);
        }
        if !self.begin_check() {
            return self.status(false);
        }

        let result = self.backend.check().await;
        let next = match result {
            Ok(Some(mut info)) => {
                info.truncate_notes();
                if is_newer(&info.version, self.current_version) {
                    UpdateState::Available(info)
                } else {
                    UpdateState::UpToDate
                }
            }
            Ok(None) => UpdateState::UpToDate,
            Err(reason) => UpdateState::Failed(known_reason(reason)),
        };
        self.transition(next);
        self.status(false)
    }

    /// Downloads and verifies the offered artifact but never installs it.
    pub async fn download(&self, match_active: bool) -> Value {
        if match_active {
            return self.reply(true, Some(MATCH_ACTIVE));
        }
        let info = match self.begin_download() {
            Some(info) => info,
            None => return self.status(false),
        };

        let state = &self.data;
        let progress = Arc::new(move |percent: u8| {
            let mut data = lock_state(state);
            if let UpdateState::Downloading { info, progress } = &data.state {
                let next = percent.min(100);
                if next != *progress {
                    data.state = UpdateState::Downloading {
                        info: info.clone(),
                        progress: next,
                    };
                }
            }
        }) as UpdateProgress<'_>;

        let result = self.backend.download(&info, progress).await;
        match result {
            Ok(()) => self.transition(UpdateState::Ready(info)),
            Err(reason) => self.transition(UpdateState::Failed(known_reason(reason))),
        }
        self.status(false)
    }

    /// Installs a verified download only after an explicit command and a second match-state check.
    pub fn install(
        &self,
        confirm: bool,
        match_active: bool,
        before_install: impl FnOnce() -> BackendResult<()>,
    ) -> Value {
        if match_active {
            return self.reply(true, Some(MATCH_ACTIVE));
        }
        if !confirm {
            return self.status(false);
        }
        let info = match self.begin_install() {
            Some(info) => info,
            None => return self.status(false),
        };

        if let Err(reason) = before_install() {
            if reason == MATCH_ACTIVE {
                self.transition(UpdateState::Ready(info));
                return self.reply(true, Some(MATCH_ACTIVE));
            }
            self.transition(UpdateState::Failed(known_reason(reason)));
            return self.status(false);
        }

        match self.backend.install(&info) {
            Ok(()) => self.status(false), // Windows NSIS exits after launching the installer.
            Err(reason) => {
                self.transition(UpdateState::Failed(known_reason(reason)));
                self.status(false)
            }
        }
    }

    fn begin_check(&self) -> bool {
        let mut data = lock_state(&self.data);
        if !matches!(
            data.state,
            UpdateState::Idle
                | UpdateState::UpToDate
                | UpdateState::Failed(_)
                | UpdateState::Available(_)
        ) {
            return false;
        }
        let previous = data.state.clone();
        data.state = UpdateState::Checking;
        drop(data);
        log_state_change(&previous, &UpdateState::Checking);
        true
    }

    fn begin_download(&self) -> Option<UpdateInfo> {
        let mut data = lock_state(&self.data);
        let UpdateState::Available(info) = &data.state else {
            return None;
        };
        let info = info.clone();
        let previous = data.state.clone();
        data.state = UpdateState::Downloading {
            info: info.clone(),
            progress: 0,
        };
        drop(data);
        log_state_change(
            &previous,
            &UpdateState::Downloading {
                info: info.clone(),
                progress: 0,
            },
        );
        Some(info)
    }

    fn begin_install(&self) -> Option<UpdateInfo> {
        let mut data = lock_state(&self.data);
        let UpdateState::Ready(info) = &data.state else {
            return None;
        };
        let info = info.clone();
        let previous = data.state.clone();
        data.state = UpdateState::Installing;
        drop(data);
        log_state_change(&previous, &UpdateState::Installing);
        Some(info)
    }

    fn transition(&self, next: UpdateState) {
        let mut data = lock_state(&self.data);
        if data.state == next {
            return;
        }
        let previous = std::mem::replace(&mut data.state, next.clone());
        drop(data);
        log_state_change(&previous, &next);
    }

    fn configuration_reason(&self) -> Option<&'static str> {
        configuration_reason(self.updater_enabled, self.endpoint, self.pubkey)
    }

    fn reply(&self, match_active: bool, reason_override: Option<&'static str>) -> Value {
        let data = lock_state(&self.data);
        let state_name = state_name(&data.state);
        let reason = reason_override.or_else(|| match &data.state {
            UpdateState::NotConfigured(reason) | UpdateState::Failed(reason) => Some(*reason),
            _ => None,
        });
        let info = match &data.state {
            UpdateState::Available(info)
            | UpdateState::Ready(info)
            | UpdateState::Downloading { info, .. } => Some(info),
            _ => None,
        };
        let available = info.map_or(Value::Null, |info| {
            json!({
                "version": &info.version,
                "notes": &info.notes,
                "pub_date": &info.pub_date,
            })
        });
        let download_progress = match &data.state {
            UpdateState::Downloading { progress, .. } => Some(*progress),
            _ => None,
        };

        json!({
            "state": state_name,
            "current_version": self.current_version,
            "reason": reason,
            "available": available,
            "download_progress": download_progress,
            "match_active": match_active,
        })
    }
}

fn lock_state(mutex: &Mutex<UpdateData>) -> MutexGuard<'_, UpdateData> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn state_name(state: &UpdateState) -> &'static str {
    match state {
        UpdateState::NotConfigured(_) => "not_configured",
        UpdateState::Idle => "idle",
        UpdateState::Checking => "checking",
        UpdateState::UpToDate => "up_to_date",
        UpdateState::Available(_) => "available",
        UpdateState::Downloading { .. } => "downloading",
        UpdateState::Ready(_) => "ready",
        UpdateState::Installing => "installing",
        UpdateState::Failed(_) => "failed",
    }
}

fn log_state_change(previous: &UpdateState, next: &UpdateState) {
    let previous_reason = state_reason(previous);
    let next_reason = state_reason(next);
    if state_name(previous) == state_name(next) && previous_reason == next_reason {
        return;
    }
    let mut fields = Map::new();
    fields.insert(
        "state".to_owned(),
        Value::String(state_name(next).to_owned()),
    );
    if let Some(reason) = next_reason {
        fields.insert("reason".to_owned(), Value::String(reason.to_owned()));
    }
    crate::logging::emit(
        crate::logging::Level::Info,
        "update",
        "state_changed",
        "updater state changed",
        fields,
    );
}

fn state_reason(state: &UpdateState) -> Option<&'static str> {
    match state {
        UpdateState::NotConfigured(reason) | UpdateState::Failed(reason) => Some(*reason),
        _ => None,
    }
}

fn configuration_reason(
    updater_enabled: bool,
    endpoint: Option<&str>,
    pubkey: Option<&str>,
) -> Option<&'static str> {
    if !updater_enabled {
        Some(BUILD_WITHOUT_UPDATER)
    } else if endpoint.is_none() {
        Some(NO_ENDPOINT)
    } else if pubkey.is_none() {
        Some(NO_PUBKEY)
    } else {
        None
    }
}

fn known_reason(reason: &'static str) -> &'static str {
    match reason {
        BUILD_WITHOUT_UPDATER
        | NO_ENDPOINT
        | NO_PUBKEY
        | MATCH_ACTIVE
        | NETWORK
        | SIGNATURE
        | BACKEND_UNAVAILABLE
        | BAD_RESPONSE => reason,
        _ => BAD_RESPONSE,
    }
}

#[cfg(feature = "updater")]
pub(crate) mod plugin;

#[derive(Debug)]
struct ParsedVersion<'a> {
    major: &'a str,
    minor: &'a str,
    patch: &'a str,
    prerelease: Option<Vec<&'a str>>,
}

fn parse_version(version: &str) -> Option<ParsedVersion<'_>> {
    let (without_build, build) = match version.split_once('+') {
        Some((version, build)) => (version, Some(build)),
        None => (version, None),
    };
    if build.is_some_and(|value| !valid_identifiers(value, false)) {
        return None;
    }

    let (core, prerelease) = match without_build.split_once('-') {
        Some((core, prerelease)) => (core, Some(prerelease)),
        None => (without_build, None),
    };
    let mut core_parts = core.split('.');
    let major = core_parts.next()?;
    let minor = core_parts.next()?;
    let patch = core_parts.next()?;
    if core_parts.next().is_some()
        || !valid_core_number(major)
        || !valid_core_number(minor)
        || !valid_core_number(patch)
    {
        return None;
    }

    let prerelease = match prerelease {
        Some(value) => {
            if !valid_identifiers(value, true) {
                return None;
            }
            Some(value.split('.').collect())
        }
        None => None,
    };

    Some(ParsedVersion {
        major,
        minor,
        patch,
        prerelease,
    })
}

fn valid_core_number(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn valid_identifiers(value: &str, reject_numeric_leading_zero: bool) -> bool {
    if value.is_empty() {
        return false;
    }
    value.split('.').all(|identifier| {
        !identifier.is_empty()
            && identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !(reject_numeric_leading_zero
                && is_numeric(identifier)
                && identifier.len() > 1
                && identifier.starts_with('0'))
    })
}

fn is_numeric(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn compare_numeric(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn compare_identifiers(a: &str, b: &str) -> Ordering {
    match (is_numeric(a), is_numeric(b)) {
        (true, true) => compare_numeric(a, b),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.cmp(b),
    }
}

fn compare_parsed(a: &ParsedVersion<'_>, b: &ParsedVersion<'_>) -> Ordering {
    compare_numeric(a.major, b.major)
        .then_with(|| compare_numeric(a.minor, b.minor))
        .then_with(|| compare_numeric(a.patch, b.patch))
        .then_with(|| match (&a.prerelease, &b.prerelease) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => {
                for (a_id, b_id) in a.iter().zip(b) {
                    let ordering = compare_identifiers(a_id, b_id);
                    if ordering != Ordering::Equal {
                        return ordering;
                    }
                }
                a.len().cmp(&b.len())
            }
        })
}

/// Compares valid SemVer 2.0 versions. Malformed input is treated as unordered/equal.
#[cfg(test)]
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    match (parse_version(a), parse_version(b)) {
        (Some(a), Some(b)) => compare_parsed(&a, &b),
        _ => Ordering::Equal,
    }
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => {
            compare_parsed(&candidate, &current) == Ordering::Greater
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BACKEND_UNAVAILABLE, BAD_RESPONSE, BUILD_WITHOUT_UPDATER, BackendFuture, BackendResult,
        MATCH_ACTIVE, NETWORK, NO_ENDPOINT, NO_PUBKEY, NullBackend, SIGNATURE, UpdateBackend,
        UpdateController, UpdateInfo, UpdateProgress, compare_versions, configuration_reason,
        is_newer, open_release_page_reply, parse_version,
    };
    use serde_json::{Value, json};
    use std::cmp::Ordering;
    use std::collections::BTreeSet;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Condvar, Mutex, MutexGuard};
    use std::task::{Context, Poll, Wake, Waker};

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    struct Gate {
        entered_sender: Sender<()>,
        entered_receiver: Mutex<Receiver<()>>,
        released: Mutex<bool>,
        released_signal: Condvar,
        waker: Mutex<Option<Waker>>,
    }

    impl Gate {
        fn new() -> Arc<Self> {
            let (entered_sender, entered_receiver) = mpsc::channel();
            Arc::new(Self {
                entered_sender,
                entered_receiver: Mutex::new(entered_receiver),
                released: Mutex::new(false),
                released_signal: Condvar::new(),
                waker: Mutex::new(None),
            })
        }

        fn entered(&self) {
            let _ = self.entered_sender.send(());
        }

        fn wait_entered(&self) {
            lock(&self.entered_receiver)
                .recv()
                .expect("backend reached its gate");
        }

        fn release(&self) {
            *lock(&self.released) = true;
            self.released_signal.notify_all();
            if let Some(waker) = lock(&self.waker).take() {
                waker.wake();
            }
        }

        async fn wait_async(self: &Arc<Self>) {
            GateWait { gate: self.clone() }.await;
        }

        fn wait_blocking(&self) {
            let mut released = lock(&self.released);
            while !*released {
                released = self
                    .released_signal
                    .wait(released)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
    }

    struct GateWait {
        gate: Arc<Gate>,
    }

    impl Future for GateWait {
        type Output = ();

        fn poll(self: std::pin::Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
            if *lock(&self.gate.released) {
                return Poll::Ready(());
            }
            *lock(&self.gate.waker) = Some(context.waker().clone());
            if *lock(&self.gate.released) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[derive(Clone, Default)]
    struct Calls {
        checks: Arc<AtomicUsize>,
        downloads: Arc<AtomicUsize>,
        installs: Arc<AtomicUsize>,
        progress: Arc<AtomicUsize>,
    }

    #[derive(Clone)]
    struct FakeBackend {
        check_result: Result<Option<UpdateInfo>, &'static str>,
        download_result: Result<(), &'static str>,
        install_result: Result<(), &'static str>,
        calls: Calls,
        check_gate: Option<Arc<Gate>>,
        download_gate: Option<Arc<Gate>>,
        install_gate: Option<Arc<Gate>>,
    }

    impl FakeBackend {
        fn new(
            check_result: Result<Option<UpdateInfo>, &'static str>,
            download_result: Result<(), &'static str>,
            install_result: Result<(), &'static str>,
        ) -> Self {
            Self {
                check_result,
                download_result,
                install_result,
                calls: Calls::default(),
                check_gate: None,
                download_gate: None,
                install_gate: None,
            }
        }
    }

    impl UpdateBackend for FakeBackend {
        fn check(&self) -> BackendFuture<'_, BackendResult<Option<UpdateInfo>>> {
            self.calls.checks.fetch_add(1, AtomicOrdering::SeqCst);
            let result = self.check_result.clone();
            let gate = self.check_gate.clone();
            Box::pin(async move {
                if let Some(gate) = gate {
                    gate.entered();
                    gate.wait_async().await;
                }
                result
            })
        }

        fn download<'a>(
            &'a self,
            _info: &'a UpdateInfo,
            progress: UpdateProgress<'a>,
        ) -> BackendFuture<'a, BackendResult<()>> {
            self.calls.downloads.fetch_add(1, AtomicOrdering::SeqCst);
            let result = self.download_result;
            let gate = self.download_gate.clone();
            let calls = self.calls.clone();
            Box::pin(async move {
                progress(63);
                calls.progress.store(63, AtomicOrdering::SeqCst);
                if let Some(gate) = gate {
                    gate.entered();
                    gate.wait_async().await;
                }
                result
            })
        }

        fn install(&self, _info: &UpdateInfo) -> BackendResult<()> {
            self.calls.installs.fetch_add(1, AtomicOrdering::SeqCst);
            if let Some(gate) = &self.install_gate {
                gate.entered();
                gate.wait_blocking();
            }
            self.install_result
        }
    }

    fn controller(
        check: Result<Option<UpdateInfo>, &'static str>,
        download: Result<(), &'static str>,
        install: Result<(), &'static str>,
    ) -> (UpdateController<FakeBackend>, Calls) {
        let backend = FakeBackend::new(check, download, install);
        let calls = backend.calls.clone();
        (
            UpdateController::new_with_feature(
                Some("test-endpoint"),
                Some("test-public-key"),
                backend,
                "0.3.0",
                true,
            ),
            calls,
        )
    }

    fn available_info(version: &str) -> UpdateInfo {
        UpdateInfo::new(version, "release notes", Some("2026-11-15".to_owned()))
    }

    fn state(value: &Value) -> &str {
        value["state"].as_str().expect("state string")
    }

    fn reason(value: &Value) -> Option<&str> {
        value["reason"].as_str()
    }

    fn json_keys(value: &Value) -> BTreeSet<String> {
        value
            .as_object()
            .expect("reply object")
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn release_page_uses_fixed_url_and_refuses_matches() {
        assert_eq!(
            super::RELEASE_PAGE_URL,
            "https://github.com/Freshair129/kohbor/releases/latest"
        );
        let mut seen = Vec::new();
        let opened = open_release_page_reply(false, |url| {
            seen.push(url.to_owned());
            true
        });
        assert_eq!(seen, vec![super::RELEASE_PAGE_URL]);
        assert_eq!(opened["opened"], true);
        let blocked = open_release_page_reply(true, |_| panic!("must not open in a match"));
        assert_eq!(blocked["reason"], MATCH_ACTIVE);
    }

    #[test]
    fn tauri_plugin_config_matches_embedded_update_trust_constants() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("tauri.conf.json is valid JSON");
        let updater = &config["plugins"]["updater"];
        assert_eq!(updater["endpoints"][0].as_str(), super::UPDATE_ENDPOINT);
        assert_eq!(updater["pubkey"].as_str(), super::UPDATE_PUBKEY);
        assert_eq!(updater["requireSignedVersion"], true);
        assert_eq!(updater["windows"]["installMode"], "passive");
    }

    #[test]
    fn configuration_fails_closed_in_order() {
        assert_eq!(
            configuration_reason(false, None, None),
            Some(BUILD_WITHOUT_UPDATER)
        );
        assert_eq!(configuration_reason(true, None, None), Some(NO_ENDPOINT));
        assert_eq!(
            configuration_reason(true, None, Some("key")),
            Some(NO_ENDPOINT)
        );
        assert_eq!(
            configuration_reason(true, Some("url"), None),
            Some(NO_PUBKEY)
        );
        assert_eq!(configuration_reason(true, Some("url"), Some("key")), None);
    }

    #[test]
    fn default_controller_reports_build_configuration() {
        let controller = UpdateController::new(None, None, NullBackend, "0.3.0");
        let reply = controller.status(false);
        assert_eq!(state(&reply), "not_configured");
        assert_eq!(
            reason(&reply),
            Some(if cfg!(feature = "updater") {
                NO_ENDPOINT
            } else {
                BUILD_WITHOUT_UPDATER
            })
        );
    }

    #[test]
    fn status_includes_progress_and_has_stable_keys() {
        let (controller, _) = controller(Ok(None), Ok(()), Ok(()));
        let reply = controller.status(true);
        assert_eq!(reply["current_version"], "0.3.0");
        assert_eq!(reply["match_active"], true);
        assert!(reply["available"].is_null());
        assert!(reply["download_progress"].is_null());
        assert_eq!(
            json_keys(&reply),
            BTreeSet::from([
                "state".to_owned(),
                "current_version".to_owned(),
                "reason".to_owned(),
                "available".to_owned(),
                "download_progress".to_owned(),
                "match_active".to_owned(),
            ])
        );
    }

    #[test]
    fn check_offers_only_strictly_newer_semver() {
        for version in ["0.2.9", "0.3.0", "0.3.0+build.1", "bad"] {
            let (controller, calls) = controller(Ok(Some(available_info(version))), Ok(()), Ok(()));
            let reply = block_on(controller.check(false));
            assert_eq!(state(&reply), "up_to_date", "candidate {version}");
            assert_eq!(calls.downloads.load(AtomicOrdering::SeqCst), 0);
        }

        let (controller, calls) = controller(Ok(Some(available_info("0.3.1"))), Ok(()), Ok(()));
        let reply = block_on(controller.check(false));
        assert_eq!(state(&reply), "available");
        assert_eq!(reply["available"]["version"], "0.3.1");
        assert_eq!(calls.checks.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(calls.downloads.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn check_none_and_errors_reach_up_to_date_and_failed() {
        let (none_controller, _) = controller(Ok(None), Ok(()), Ok(()));
        assert_eq!(state(&block_on(none_controller.check(false))), "up_to_date");

        let (controller, _) = controller(Err(NETWORK), Ok(()), Ok(()));
        let reply = block_on(controller.check(false));
        assert_eq!(state(&reply), "failed");
        assert_eq!(reason(&reply), Some(NETWORK));
    }

    #[test]
    fn download_progress_reaches_ready_without_installing() {
        let (controller, calls) = controller(Ok(Some(available_info("0.3.1"))), Ok(()), Ok(()));
        assert_eq!(state(&block_on(controller.check(false))), "available");
        let reply = block_on(controller.download(false));
        assert_eq!(state(&reply), "ready");
        assert_eq!(reply["available"]["version"], "0.3.1");
        assert_eq!(reply["download_progress"], Value::Null);
        assert_eq!(calls.progress.load(AtomicOrdering::SeqCst), 63);
        assert_eq!(calls.downloads.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(calls.installs.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn checking_download_progress_and_installing_are_visible_while_backend_waits() {
        let check_gate = Gate::new();
        let mut backend = FakeBackend::new(Ok(Some(available_info("0.3.1"))), Ok(()), Ok(()));
        backend.check_gate = Some(check_gate.clone());
        let controller = Arc::new(UpdateController::new_with_feature(
            Some("test-endpoint"),
            Some("test-public-key"),
            backend,
            "0.3.0",
            true,
        ));
        let worker = controller.clone();
        let check = std::thread::spawn(move || block_on(worker.check(false)));
        check_gate.wait_entered();
        assert_eq!(state(&controller.status(false)), "checking");
        check_gate.release();
        assert_eq!(state(&check.join().unwrap()), "available");

        let download_gate = Gate::new();
        let mut backend = FakeBackend::new(Ok(Some(available_info("0.3.1"))), Ok(()), Ok(()));
        backend.download_gate = Some(download_gate.clone());
        let controller = Arc::new(UpdateController::new_with_feature(
            Some("test-endpoint"),
            Some("test-public-key"),
            backend,
            "0.3.0",
            true,
        ));
        block_on(controller.check(false));
        let worker = controller.clone();
        let download = std::thread::spawn(move || block_on(worker.download(false)));
        download_gate.wait_entered();
        let downloading = controller.status(false);
        assert_eq!(state(&downloading), "downloading");
        assert_eq!(downloading["download_progress"], 63);
        download_gate.release();
        assert_eq!(state(&download.join().unwrap()), "ready");

        let install_gate = Gate::new();
        let mut backend = FakeBackend::new(Ok(Some(available_info("0.3.1"))), Ok(()), Ok(()));
        backend.install_gate = Some(install_gate.clone());
        let controller = Arc::new(UpdateController::new_with_feature(
            Some("test-endpoint"),
            Some("test-public-key"),
            backend,
            "0.3.0",
            true,
        ));
        block_on(controller.check(false));
        block_on(controller.download(false));
        let worker = controller.clone();
        let install = std::thread::spawn(move || worker.install(true, false, || Ok(())));
        install_gate.wait_entered();
        assert_eq!(state(&controller.status(false)), "installing");
        install_gate.release();
        assert_eq!(state(&install.join().unwrap()), "installing");
    }

    #[test]
    fn download_failure_does_not_install() {
        let (controller, calls) =
            controller(Ok(Some(available_info("0.3.1"))), Err(SIGNATURE), Ok(()));
        block_on(controller.check(false));
        let reply = block_on(controller.download(false));
        assert_eq!(state(&reply), "failed");
        assert_eq!(reason(&reply), Some(SIGNATURE));
        assert_eq!(calls.installs.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn install_needs_confirmation_and_ready_state() {
        let (controller, calls) = controller(Ok(Some(available_info("0.3.1"))), Ok(()), Ok(()));
        let before = Arc::new(AtomicUsize::new(0));
        let before_install = before.clone();
        let reply = controller.install(true, false, move || {
            before_install.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(())
        });
        assert_eq!(state(&reply), "idle");
        assert_eq!(calls.installs.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(before.load(AtomicOrdering::SeqCst), 0);

        block_on(controller.check(false));
        let no_confirm = controller.install(false, false, || panic!("confirmation required"));
        assert_eq!(state(&no_confirm), "available");
        assert_eq!(calls.installs.load(AtomicOrdering::SeqCst), 0);

        block_on(controller.download(false));
        let before_install = before.clone();
        let installed = controller.install(true, false, move || {
            before_install.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(())
        });
        assert_eq!(state(&installed), "installing");
        assert_eq!(calls.installs.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(before.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn install_failure_maps_reason_and_preserves_running_version() {
        let (controller, _) = controller(
            Ok(Some(available_info("0.3.1"))),
            Ok(()),
            Err(BACKEND_UNAVAILABLE),
        );
        block_on(controller.check(false));
        block_on(controller.download(false));
        let reply = controller.install(true, false, || Ok(()));
        assert_eq!(state(&reply), "failed");
        assert_eq!(reason(&reply), Some(BACKEND_UNAVAILABLE));
        assert_eq!(reply["current_version"], "0.3.0");
    }

    #[test]
    fn match_active_refuses_check_download_and_install_without_backend_calls() {
        let (controller, calls) = controller(Ok(Some(available_info("0.3.1"))), Ok(()), Ok(()));
        let check = block_on(controller.check(true));
        assert_eq!(reason(&check), Some(MATCH_ACTIVE));
        assert_eq!(calls.checks.load(AtomicOrdering::SeqCst), 0);

        block_on(controller.check(false));
        let download = block_on(controller.download(true));
        assert_eq!(state(&download), "available");
        assert_eq!(reason(&download), Some(MATCH_ACTIVE));
        assert_eq!(calls.downloads.load(AtomicOrdering::SeqCst), 0);

        block_on(controller.download(false));
        let install = controller.install(true, true, || panic!("shutdown must not run in a match"));
        assert_eq!(state(&install), "ready");
        assert_eq!(reason(&install), Some(MATCH_ACTIVE));
        assert_eq!(calls.installs.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn match_that_starts_before_install_restores_ready_state() {
        let (controller, calls) = controller(Ok(Some(available_info("0.3.1"))), Ok(()), Ok(()));
        block_on(controller.check(false));
        block_on(controller.download(false));
        let reply = controller.install(true, false, || Err(MATCH_ACTIVE));
        assert_eq!(state(&reply), "ready");
        assert_eq!(reason(&reply), Some(MATCH_ACTIVE));
        assert_eq!(calls.installs.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn no_endpoint_or_pubkey_short_circuits_the_backend() {
        let backend = FakeBackend::new(Ok(None), Ok(()), Ok(()));
        let calls = backend.calls.clone();
        let controller = UpdateController::new_with_feature(None, None, backend, "0.3.0", true);
        assert_eq!(
            reason(&block_on(controller.check(false))),
            Some(NO_ENDPOINT)
        );
        assert_eq!(calls.checks.load(AtomicOrdering::SeqCst), 0);

        let backend = FakeBackend::new(Ok(None), Ok(()), Ok(()));
        let calls = backend.calls.clone();
        let controller =
            UpdateController::new_with_feature(Some("test-endpoint"), None, backend, "0.3.0", true);
        assert_eq!(reason(&block_on(controller.check(false))), Some(NO_PUBKEY));
        assert_eq!(calls.checks.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn automatic_check_path_only_checks_and_never_downloads() {
        // The UI's automatic-check consent calls only update_check. The controller's check path
        // is independently safe: even when invoked for that consent, it only offers a candidate.
        let (controller, calls) = controller(Ok(Some(available_info("0.3.1"))), Ok(()), Ok(()));
        let result = block_on(controller.check(false));
        assert_eq!(state(&result), "available");
        assert_eq!(calls.checks.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(calls.downloads.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(calls.installs.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn notes_are_truncated_by_unicode_characters_before_serialization() {
        let notes = "อัปเดต".repeat(500);
        let info = UpdateInfo::new("0.3.1", notes, None);
        assert_eq!(info.notes.chars().count(), 2_000);

        let (controller, _) = controller(Ok(Some(info)), Ok(()), Ok(()));
        let reply = block_on(controller.check(false));
        assert_eq!(
            reply["available"]["notes"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            2_000
        );
    }

    #[test]
    fn plugin_reasons_are_allowlisted_and_unknown_reasons_are_bad_response() {
        for expected in [NETWORK, SIGNATURE, BACKEND_UNAVAILABLE, BAD_RESPONSE] {
            assert_eq!(super::known_reason(expected), expected);
        }
        assert_eq!(super::known_reason("raw-plugin-error"), BAD_RESPONSE);
    }

    #[test]
    fn null_backend_is_backend_unavailable_when_configured() {
        let controller = UpdateController::new_with_feature(
            Some("test-endpoint"),
            Some("test-public-key"),
            NullBackend,
            "0.3.0",
            true,
        );
        let reply = block_on(controller.check(false));
        assert_eq!(state(&reply), "failed");
        assert_eq!(reason(&reply), Some(BACKEND_UNAVAILABLE));
    }

    #[test]
    fn semver_prerelease_and_build_metadata_rules_are_applied() {
        assert_eq!(
            compare_versions("0.3.0-dev.10", "0.3.0-dev.2"),
            Ordering::Greater
        );
        assert!(is_newer("0.3.0", "0.3.0-rc.1"));
        assert!(!is_newer("1.2.3+build.1", "1.2.3+other"));
        assert!(!is_newer("1.2.3-rc.1", "1.2.3"));
        assert_eq!(compare_versions("1.0.0-1", "1.0.0-alpha"), Ordering::Less);
        assert_eq!(
            compare_versions("1.0.0-alpha", "1.0.0-beta"),
            Ordering::Less
        );
        assert_eq!(
            compare_versions("1.0.0-alpha.1", "1.0.0-alpha"),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("99999999999999999999.0.0", "2.0.0"),
            Ordering::Greater
        );
        assert!(parse_version("01.0.0").is_none());
        assert!(parse_version("1.0.0-01").is_none());
        assert!(!is_newer("not-semver", "0.3.0"));
    }

    #[test]
    fn replies_never_contain_endpoint_or_public_key() {
        let (controller, _) = controller(Ok(None), Ok(()), Ok(()));
        let reply = controller.status(false);
        assert_eq!(
            reply,
            json!({
                "state": "idle",
                "current_version": "0.3.0",
                "reason": null,
                "available": null,
                "download_progress": null,
                "match_active": false
            })
        );
        assert!(
            serde_json::to_string(&reply)
                .unwrap()
                .find("test-public-key")
                .is_none()
        );
        assert!(
            serde_json::to_string(&reply)
                .unwrap()
                .find("test-endpoint")
                .is_none()
        );
    }
}
