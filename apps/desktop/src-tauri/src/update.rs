//! Pure updater policy scaffold. No network, filesystem, timer, or restart behavior lives here.
// Scaffold: the full updater backend (option 2) is not wired yet, so much of this is unused in a normal
// build; it stays tested. Only the release-page action is live.
#![allow(dead_code)]

use serde_json::{json, Value};
use std::cmp::Ordering;

pub const BUILD_WITHOUT_UPDATER: &str = "BUILD_WITHOUT_UPDATER";
pub const NO_ENDPOINT: &str = "NO_ENDPOINT";
pub const NO_PUBKEY: &str = "NO_PUBKEY";
pub const MATCH_ACTIVE: &str = "MATCH_ACTIVE";
pub const NETWORK: &str = "NETWORK";
pub const SIGNATURE: &str = "SIGNATURE";
pub const BACKEND_UNAVAILABLE: &str = "BACKEND_UNAVAILABLE";
pub const BAD_RESPONSE: &str = "BAD_RESPONSE";

// The owner provides these only after the update service and trust configuration are approved.
pub const UPDATE_ENDPOINT: Option<&str> = None;
pub const UPDATE_PUBKEY: Option<&str> = None;

/// Owner decision (option 1): the manual "check for updates" button only opens this page in the
/// default browser. Hard-coded: no user-supplied input ever reaches the opener. No download, no
/// install, no request from the app itself.
pub const RELEASE_PAGE_URL: &str = "https://github.com/Freshair129/kohbor/releases/latest";

/// Reply of the open-release-page action. `opener` receives only `RELEASE_PAGE_URL`. Refused (and
/// the opener never called) while a match is pregame/live.
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    NotConfigured(&'static str),
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading,
    ReadyToInstall,
    Failed(&'static str),
}

pub trait UpdateBackend: Send + Sync {
    fn check(&self) -> Result<Option<UpdateInfo>, &'static str>;
    fn download_and_install(&self, info: &UpdateInfo) -> Result<(), &'static str>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NullBackend;

impl UpdateBackend for NullBackend {
    fn check(&self) -> Result<Option<UpdateInfo>, &'static str> {
        Err(BACKEND_UNAVAILABLE)
    }

    fn download_and_install(&self, _info: &UpdateInfo) -> Result<(), &'static str> {
        Err(BACKEND_UNAVAILABLE)
    }
}

pub struct UpdateController<B: UpdateBackend> {
    endpoint: Option<&'static str>,
    pubkey: Option<&'static str>,
    backend: B,
    state: UpdateState,
    current_version: &'static str,
    updater_enabled: bool,
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
            state,
            current_version,
            updater_enabled,
        }
    }

    pub fn status(&self, match_active: bool) -> Value {
        self.reply(match_active, None)
    }

    pub fn check(&mut self, match_active: bool) -> Value {
        if match_active {
            return self.reply(true, Some(MATCH_ACTIVE));
        }

        if let Some(reason) = self.configuration_reason() {
            self.state = UpdateState::NotConfigured(reason);
            return self.status(false);
        }

        if !matches!(
            self.state,
            UpdateState::Idle
                | UpdateState::UpToDate
                | UpdateState::Failed(_)
                | UpdateState::Available(_)
        ) {
            return self.status(false);
        }

        self.state = UpdateState::Checking;
        match self.backend.check() {
            Ok(Some(mut info)) => {
                info.truncate_notes();
                self.state = if is_newer(&info.version, self.current_version) {
                    UpdateState::Available(info)
                } else {
                    UpdateState::UpToDate
                };
            }
            Ok(None) => self.state = UpdateState::UpToDate,
            Err(reason) => self.state = UpdateState::Failed(known_reason(reason)),
        }
        self.status(false)
    }

    pub fn install(&mut self, confirm: bool, match_active: bool) -> Value {
        if match_active {
            return self.reply(true, Some(MATCH_ACTIVE));
        }
        if !confirm {
            return self.status(false);
        }

        let UpdateState::Available(info) = self.state.clone() else {
            return self.status(false);
        };

        self.state = UpdateState::Downloading;
        match self.backend.download_and_install(&info) {
            Ok(()) => self.state = UpdateState::ReadyToInstall,
            Err(reason) => self.state = UpdateState::Failed(known_reason(reason)),
        }
        self.status(false)
    }

    fn configuration_reason(&self) -> Option<&'static str> {
        configuration_reason(self.updater_enabled, self.endpoint, self.pubkey)
    }

    fn reply(&self, match_active: bool, reason_override: Option<&'static str>) -> Value {
        let state_name = match &self.state {
            UpdateState::NotConfigured(_) => "not_configured",
            UpdateState::Idle => "idle",
            UpdateState::Checking => "checking",
            UpdateState::UpToDate => "up_to_date",
            UpdateState::Available(_) => "available",
            UpdateState::Downloading => "downloading",
            UpdateState::ReadyToInstall => "ready_to_install",
            UpdateState::Failed(_) => "failed",
        };
        let reason = reason_override.or(match &self.state {
            UpdateState::NotConfigured(reason) | UpdateState::Failed(reason) => Some(*reason),
            _ => None,
        });
        let available = match &self.state {
            UpdateState::Available(info) => json!({
                "version": &info.version,
                "notes": &info.notes,
                "pub_date": &info.pub_date,
            }),
            _ => Value::Null,
        };

        json!({
            "state": state_name,
            "current_version": self.current_version,
            "reason": reason,
            "available": available,
            "match_active": match_active,
        })
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

/// Compares valid SemVer 2.0 versions. Malformed input is treated as unordered/equal;
/// callers that need to distinguish malformed input can use `is_newer`, which returns false.
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
    #[test]
    fn release_page_url_is_the_fixed_github_latest_release() {
        assert_eq!(
            super::RELEASE_PAGE_URL,
            "https://github.com/Freshair129/kohbor/releases/latest"
        );
    }

    #[test]
    fn open_release_page_calls_the_opener_with_only_the_fixed_url() {
        let mut seen = Vec::new();
        let reply = super::open_release_page_reply(false, |url| {
            seen.push(url.to_owned());
            true
        });
        assert_eq!(seen, vec![super::RELEASE_PAGE_URL.to_owned()]);
        assert_eq!(
            reply,
            serde_json::json!({"opened": true, "reason": null, "url": super::RELEASE_PAGE_URL})
        );
    }

    #[test]
    fn open_release_page_is_refused_during_a_match_without_calling_the_opener() {
        let reply = super::open_release_page_reply(true, |_| panic!("opener must not run"));
        assert_eq!(
            reply,
            serde_json::json!({"opened": false, "reason": "MATCH_ACTIVE", "url": null})
        );
    }

    #[test]
    fn open_release_page_reports_an_opener_failure() {
        let reply = super::open_release_page_reply(false, |_| false);
        assert_eq!(reply["opened"], false);
        assert_eq!(reply["reason"], "OPEN_FAILED");
    }

    use super::{
        compare_versions, configuration_reason, is_newer, parse_version, NullBackend,
        UpdateBackend, UpdateController, UpdateInfo, UpdateState, BACKEND_UNAVAILABLE,
        BAD_RESPONSE, BUILD_WITHOUT_UPDATER, MATCH_ACTIVE, NETWORK, NO_ENDPOINT, NO_PUBKEY,
        SIGNATURE,
    };
    use serde_json::{json, Value};
    use std::cmp::Ordering;
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::Arc;

    #[derive(Clone)]
    struct FakeBackend {
        check_result: Result<Option<UpdateInfo>, &'static str>,
        install_result: Result<(), &'static str>,
        checks: Arc<AtomicUsize>,
        installs: Arc<AtomicUsize>,
    }

    impl FakeBackend {
        fn new(
            check_result: Result<Option<UpdateInfo>, &'static str>,
            install_result: Result<(), &'static str>,
        ) -> Self {
            Self {
                check_result,
                install_result,
                checks: Arc::new(AtomicUsize::new(0)),
                installs: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl UpdateBackend for FakeBackend {
        fn check(&self) -> Result<Option<UpdateInfo>, &'static str> {
            self.checks.fetch_add(1, AtomicOrdering::SeqCst);
            self.check_result.clone()
        }

        fn download_and_install(&self, _info: &UpdateInfo) -> Result<(), &'static str> {
            self.installs.fetch_add(1, AtomicOrdering::SeqCst);
            self.install_result
        }
    }

    fn controller(
        check_result: Result<Option<UpdateInfo>, &'static str>,
        install_result: Result<(), &'static str>,
    ) -> UpdateController<FakeBackend> {
        UpdateController::new_with_feature(
            Some("test-endpoint"),
            Some("test-public-key"),
            FakeBackend::new(check_result, install_result),
            "0.3.0",
            true,
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
    fn owner_reserved_endpoint_and_key_are_unconfigured() {
        assert_eq!(super::UPDATE_ENDPOINT, None);
        assert_eq!(super::UPDATE_PUBKEY, None);
    }

    #[test]
    fn feature_missing_precedes_missing_endpoint_and_key() {
        assert_eq!(
            configuration_reason(false, None, None),
            Some(BUILD_WITHOUT_UPDATER)
        );
        assert_eq!(
            configuration_reason(false, Some("endpoint"), Some("key")),
            Some(BUILD_WITHOUT_UPDATER)
        );
    }

    #[test]
    fn missing_endpoint_precedes_missing_key() {
        assert_eq!(configuration_reason(true, None, None), Some(NO_ENDPOINT));
        assert_eq!(
            configuration_reason(true, None, Some("key")),
            Some(NO_ENDPOINT)
        );
    }

    #[test]
    fn endpoint_without_key_reports_no_pubkey() {
        assert_eq!(
            configuration_reason(true, Some("endpoint"), None),
            Some(NO_PUBKEY)
        );
    }

    #[test]
    fn complete_configuration_has_no_configuration_reason() {
        assert_eq!(
            configuration_reason(true, Some("endpoint"), Some("key")),
            None
        );
    }

    #[test]
    fn normal_build_constructor_reports_its_configuration_state() {
        let controller = UpdateController::new(None, None, NullBackend, "0.3.0");
        let reply = controller.status(false);
        let expected = if cfg!(feature = "updater") {
            NO_ENDPOINT
        } else {
            BUILD_WITHOUT_UPDATER
        };
        assert_eq!(state(&reply), "not_configured");
        assert_eq!(reason(&reply), Some(expected));
    }

    #[test]
    fn replies_have_exact_top_level_keys() {
        let controller = controller(Ok(None), Ok(()));
        let reply = controller.status(false);
        assert_eq!(
            json_keys(&reply),
            BTreeSet::from([
                "state".to_owned(),
                "current_version".to_owned(),
                "reason".to_owned(),
                "available".to_owned(),
                "match_active".to_owned(),
            ])
        );
    }

    #[test]
    fn status_check_and_install_share_the_same_reply_shape() {
        let expected = BTreeSet::from([
            "state".to_owned(),
            "current_version".to_owned(),
            "reason".to_owned(),
            "available".to_owned(),
            "match_active".to_owned(),
        ]);
        let mut controller = controller(Ok(None), Ok(()));
        assert_eq!(json_keys(&controller.status(false)), expected);
        assert_eq!(json_keys(&controller.check(false)), expected);
        assert_eq!(json_keys(&controller.install(false, false)), expected);
    }

    #[test]
    fn status_includes_current_version_and_match_flag() {
        let controller = controller(Ok(None), Ok(()));
        let reply = controller.status(true);
        assert_eq!(reply["current_version"], "0.3.0");
        assert_eq!(reply["match_active"], true);
        assert!(reply["available"].is_null());
    }

    #[test]
    fn all_states_serialize_with_the_required_names_and_shape() {
        let states = [
            (UpdateState::NotConfigured(NO_ENDPOINT), "not_configured"),
            (UpdateState::Idle, "idle"),
            (UpdateState::Checking, "checking"),
            (UpdateState::UpToDate, "up_to_date"),
            (UpdateState::Available(available_info("0.4.0")), "available"),
            (UpdateState::Downloading, "downloading"),
            (UpdateState::ReadyToInstall, "ready_to_install"),
            (UpdateState::Failed(NETWORK), "failed"),
        ];
        let expected_keys = BTreeSet::from([
            "state".to_owned(),
            "current_version".to_owned(),
            "reason".to_owned(),
            "available".to_owned(),
            "match_active".to_owned(),
        ]);
        for (controller_state, expected_name) in states {
            let mut controller = controller(Ok(None), Ok(()));
            controller.state = controller_state;
            let reply = controller.status(false);
            assert_eq!(state(&reply), expected_name);
            assert_eq!(json_keys(&reply), expected_keys);
            if expected_name == "available" {
                assert_eq!(
                    json_keys(&reply["available"]),
                    BTreeSet::from([
                        "version".to_owned(),
                        "notes".to_owned(),
                        "pub_date".to_owned(),
                    ])
                );
            } else {
                assert!(reply["available"].is_null());
            }
        }
    }

    #[test]
    fn not_configured_serializes_reason_code() {
        let controller = UpdateController::new(None, None, NullBackend, "0.3.0");
        let reply = controller.status(false);
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
    fn failed_state_serializes_reason_code() {
        let mut controller = controller(Err(SIGNATURE), Ok(()));
        let reply = controller.check(false);
        assert_eq!(state(&reply), "failed");
        assert_eq!(reason(&reply), Some(SIGNATURE));
    }

    #[test]
    fn match_active_reply_overrides_reason_without_storing_failure() {
        let mut controller = controller(Ok(None), Ok(()));
        let reply = controller.check(true);
        assert_eq!(state(&reply), "idle");
        assert_eq!(reason(&reply), Some(MATCH_ACTIVE));
        assert_eq!(state(&controller.status(false)), "idle");
        assert_eq!(reason(&controller.status(false)), None);
    }

    #[test]
    fn check_while_match_active_preserves_available_state_and_skips_backend() {
        let info = available_info("0.4.0");
        let mut controller = controller(Ok(None), Ok(()));
        controller.state = UpdateState::Available(info.clone());
        let checks = controller.backend.checks.clone();
        let reply = controller.check(true);
        assert_eq!(state(&reply), "available");
        assert_eq!(reason(&reply), Some(MATCH_ACTIVE));
        assert_eq!(controller.state, UpdateState::Available(info));
        assert_eq!(checks.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn check_is_allowed_from_idle() {
        let mut controller = controller(Ok(None), Ok(()));
        assert_eq!(state(&controller.status(false)), "idle");
        assert_eq!(state(&controller.check(false)), "up_to_date");
    }

    #[test]
    fn check_is_allowed_from_up_to_date() {
        let mut controller = controller(Ok(None), Ok(()));
        controller.state = UpdateState::UpToDate;
        assert_eq!(state(&controller.check(false)), "up_to_date");
        assert_eq!(controller.backend.checks.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn check_is_allowed_from_failed() {
        let mut controller = controller(Ok(None), Ok(()));
        controller.state = UpdateState::Failed(NETWORK);
        assert_eq!(state(&controller.check(false)), "up_to_date");
    }

    #[test]
    fn check_is_allowed_from_available() {
        let mut controller = controller(Ok(None), Ok(()));
        controller.state = UpdateState::Available(available_info("0.4.0"));
        assert_eq!(state(&controller.check(false)), "up_to_date");
    }

    #[test]
    fn check_from_transitional_or_ready_states_does_not_call_backend() {
        for state in [
            UpdateState::Checking,
            UpdateState::Downloading,
            UpdateState::ReadyToInstall,
        ] {
            let mut controller = controller(Ok(None), Ok(()));
            controller.state = state.clone();
            let checks = controller.backend.checks.clone();
            let reply = controller.check(false);
            assert_eq!(controller.state, state);
            assert_eq!(checks.load(AtomicOrdering::SeqCst), 0);
            assert_eq!(reply["reason"], Value::Null);
        }
    }

    #[test]
    fn no_update_response_marks_up_to_date() {
        let mut controller = controller(Ok(None), Ok(()));
        let reply = controller.check(false);
        assert_eq!(state(&reply), "up_to_date");
    }

    #[test]
    fn older_backend_version_marks_up_to_date() {
        let mut controller = controller(Ok(Some(available_info("0.2.9"))), Ok(()));
        assert_eq!(state(&controller.check(false)), "up_to_date");
    }

    #[test]
    fn equal_backend_version_marks_up_to_date() {
        let mut controller = controller(Ok(Some(available_info("0.3.0+build.2"))), Ok(()));
        assert_eq!(state(&controller.check(false)), "up_to_date");
    }

    #[test]
    fn newer_backend_version_marks_available() {
        let mut controller = controller(Ok(Some(available_info("0.3.1-rc.1"))), Ok(()));
        let reply = controller.check(false);
        assert_eq!(state(&reply), "available");
        assert_eq!(reply["available"]["version"], "0.3.1-rc.1");
        assert_eq!(reply["available"]["pub_date"], "2026-11-15");
    }

    #[test]
    fn backend_check_error_becomes_failed() {
        let mut controller = controller(Err(NETWORK), Ok(()));
        let reply = controller.check(false);
        assert_eq!(state(&reply), "failed");
        assert_eq!(reason(&reply), Some(NETWORK));
    }

    #[test]
    fn unknown_backend_reason_becomes_bad_response() {
        let mut controller = controller(Err("UNLISTED_CODE"), Ok(()));
        let reply = controller.check(false);
        assert_eq!(state(&reply), "failed");
        assert_eq!(reason(&reply), Some(BAD_RESPONSE));
    }

    #[test]
    fn install_without_confirmation_preserves_available_and_skips_backend() {
        let info = available_info("0.4.0");
        let mut controller = controller(Ok(None), Ok(()));
        controller.state = UpdateState::Available(info.clone());
        let installs = controller.backend.installs.clone();
        let reply = controller.install(false, false);
        assert_eq!(state(&reply), "available");
        assert_eq!(reason(&reply), None);
        assert_eq!(controller.state, UpdateState::Available(info));
        assert_eq!(installs.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn install_during_match_preserves_available_and_skips_backend() {
        let info = available_info("0.4.0");
        let mut controller = controller(Ok(None), Ok(()));
        controller.state = UpdateState::Available(info.clone());
        let installs = controller.backend.installs.clone();
        let reply = controller.install(true, true);
        assert_eq!(state(&reply), "available");
        assert_eq!(reason(&reply), Some(MATCH_ACTIVE));
        assert_eq!(controller.state, UpdateState::Available(info));
        assert_eq!(installs.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn install_from_non_available_state_is_refused() {
        let mut controller = controller(Ok(None), Ok(()));
        let installs = controller.backend.installs.clone();
        let reply = controller.install(true, false);
        assert_eq!(state(&reply), "idle");
        assert_eq!(controller.state, UpdateState::Idle);
        assert_eq!(installs.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn install_success_reaches_ready_to_install_only_after_backend_returns() {
        let mut controller = controller(Ok(None), Ok(()));
        controller.state = UpdateState::Available(available_info("0.4.0"));
        let reply = controller.install(true, false);
        assert_eq!(state(&reply), "ready_to_install");
        assert_eq!(controller.state, UpdateState::ReadyToInstall);
        assert_eq!(controller.backend.installs.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn install_backend_error_leaves_running_version_unchanged() {
        let mut controller = controller(Ok(None), Err(SIGNATURE));
        controller.state = UpdateState::Available(available_info("0.4.0"));
        let original_version = controller.current_version;
        let reply = controller.install(true, false);
        assert_eq!(state(&reply), "failed");
        assert_eq!(reason(&reply), Some(SIGNATURE));
        assert_eq!(controller.current_version, original_version);
    }

    #[test]
    fn installation_cannot_begin_before_a_user_confirmed_available_state() {
        let mut controller = controller(Ok(None), Ok(()));
        let checks = controller.backend.checks.clone();
        let installs = controller.backend.installs.clone();
        let reply = controller.install(true, false);
        assert_eq!(state(&reply), "idle");
        assert_eq!(checks.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(installs.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn notes_are_truncated_at_two_thousand_unicode_characters() {
        let notes = "ก🙂".repeat(1_001);
        let info = UpdateInfo::new("0.4.0", notes, None);
        assert_eq!(info.notes.chars().count(), 2_000);
        assert!(info.notes.is_char_boundary(info.notes.len()));
    }

    #[test]
    fn backend_notes_are_truncated_before_reply_serialization() {
        let info = UpdateInfo {
            version: "0.4.0".to_owned(),
            notes: "x".repeat(2_001),
            pub_date: None,
        };
        let mut controller = controller(Ok(Some(info)), Ok(()));
        let reply = controller.check(false);
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
    fn null_backend_reports_backend_unavailable() {
        let mut controller = UpdateController::new_with_feature(
            Some("test-endpoint"),
            Some("test-key"),
            NullBackend,
            "0.3.0",
            true,
        );
        let reply = controller.check(false);
        assert_eq!(state(&reply), "failed");
        assert_eq!(reason(&reply), Some(BACKEND_UNAVAILABLE));
    }

    #[test]
    fn prerelease_sequence_orders_numeric_identifiers_numerically() {
        let versions = [
            "0.3.0-dev.9",
            "0.3.0-dev.10",
            "0.3.0-dev.14",
            "0.3.0",
            "0.3.1-rc.1",
        ];
        for pair in versions.windows(2) {
            assert_eq!(compare_versions(pair[0], pair[1]), Ordering::Less);
        }
    }

    #[test]
    fn mvp_prerelease_precedes_release() {
        assert_eq!(compare_versions("0.2.0-mvp2.1", "0.2.0"), Ordering::Less);
    }

    #[test]
    fn equal_precedence_ignores_build_metadata() {
        assert_eq!(
            compare_versions("1.2.3+build.1", "1.2.3+other"),
            Ordering::Equal
        );
        assert!(!is_newer("1.2.3+build.1", "1.2.3+other"));
    }

    #[test]
    fn prerelease_is_lower_than_release() {
        assert_eq!(compare_versions("1.0.0-rc.1", "1.0.0"), Ordering::Less);
    }

    #[test]
    fn numeric_prerelease_identifier_is_lower_than_alphanumeric() {
        assert_eq!(compare_versions("1.0.0-1", "1.0.0-alpha"), Ordering::Less);
    }

    #[test]
    fn alphanumeric_prerelease_identifiers_compare_lexically() {
        assert_eq!(
            compare_versions("1.0.0-alpha2", "1.0.0-alpha10"),
            Ordering::Greater
        );
    }

    #[test]
    fn shorter_equal_prerelease_sequence_has_lower_precedence() {
        assert_eq!(
            compare_versions("1.0.0-alpha", "1.0.0-alpha.1"),
            Ordering::Less
        );
    }

    #[test]
    fn large_core_numbers_compare_without_integer_overflow() {
        assert!(is_newer(
            "999999999999999999999.0.0",
            "999999999999999999998.9.9"
        ));
    }

    #[test]
    fn malformed_versions_parse_as_none_and_are_not_newer() {
        for malformed in [
            "1.2",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "1.2.3-",
            "1.2.3+",
            "1.2.3-alpha..1",
            "1.2.3-01",
            " 1.2.3",
            "v1.2.3",
        ] {
            assert!(parse_version(malformed).is_none(), "accepted {malformed}");
            assert!(
                !is_newer(malformed, "0.0.0"),
                "considered {malformed} newer"
            );
        }
        assert_eq!(compare_versions("bad", "1.0.0"), Ordering::Equal);
    }

    #[test]
    fn check_with_malformed_candidate_is_not_newer() {
        let mut controller = controller(Ok(Some(available_info("not-semver"))), Ok(()));
        assert_eq!(state(&controller.check(false)), "up_to_date");
    }

    #[test]
    fn check_without_an_endpoint_does_not_call_backend() {
        let backend = FakeBackend::new(Ok(None), Ok(()));
        let checks = backend.checks.clone();
        let mut controller = UpdateController::new_with_feature(None, None, backend, "0.3.0", true);
        let reply = controller.check(false);
        assert_eq!(state(&reply), "not_configured");
        assert_eq!(reason(&reply), Some(NO_ENDPOINT));
        assert_eq!(checks.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn explicit_install_is_the_only_available_to_downloading_path() {
        let mut controller = controller(Ok(Some(available_info("0.4.0"))), Ok(()));
        assert_eq!(state(&controller.check(false)), "available");
        assert_eq!(state(&controller.status(false)), "available");
        assert_eq!(state(&controller.install(false, false)), "available");
        assert_eq!(controller.backend.installs.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(state(&controller.install(true, false)), "ready_to_install");
        assert_eq!(controller.backend.installs.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn state_reply_does_not_leak_config_values_or_release_notes() {
        let mut controller = UpdateController::new_with_feature(
            Some("private-test-endpoint"),
            Some("private-test-key"),
            FakeBackend::new(Ok(Some(available_info("0.4.0"))), Ok(())),
            "0.3.0",
            true,
        );
        let reply = controller.check(false);
        let text = reply.to_string();
        assert!(!text.contains("private-test-endpoint"));
        assert!(!text.contains("private-test-key"));
        assert!(text.contains("release notes"));
    }

    #[test]
    fn every_declared_reason_is_a_serializable_code() {
        for code in [
            BUILD_WITHOUT_UPDATER,
            NO_ENDPOINT,
            NO_PUBKEY,
            MATCH_ACTIVE,
            NETWORK,
            SIGNATURE,
            BACKEND_UNAVAILABLE,
            BAD_RESPONSE,
        ] {
            assert!(code
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte == b'_'));
        }
    }

    #[test]
    fn null_backend_install_error_preserves_version() {
        let mut controller = UpdateController::new_with_feature(
            Some("test-endpoint"),
            Some("test-key"),
            NullBackend,
            "0.3.0",
            true,
        );
        controller.state = UpdateState::Available(available_info("0.4.0"));
        let reply = controller.install(true, false);
        assert_eq!(state(&reply), "failed");
        assert_eq!(reason(&reply), Some(BACKEND_UNAVAILABLE));
        assert_eq!(controller.current_version, "0.3.0");
    }

    #[test]
    fn available_reply_has_nullable_publication_date() {
        let mut controller = controller(Ok(Some(UpdateInfo::new("0.4.0", "", None))), Ok(()));
        let reply = controller.check(false);
        assert_eq!(reply["available"]["pub_date"], Value::Null);
        assert_eq!(reply["available"]["notes"], json!(""));
    }
}
