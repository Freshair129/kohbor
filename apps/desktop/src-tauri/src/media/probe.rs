//! Renderer decode-probe DTOs and service-thread probe state.

use std::collections::HashMap;

use serde_json::{Map, Number, Value};

const PROBE_TIMEOUT_MS: u64 = 5_000;
const FRAME_TOLERANCE_MS: u64 = 42;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeItem {
    pub path_sha256: String,
    pub url: String,
    pub duration_ms: u32,
    pub visual_tail_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeRequest {
    pub install_id: String,
    pub revision: u64,
    pub items: Vec<ProbeItem>,
}

impl ProbeRequest {
    /// Serialize the closed renderer request shape.
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "install_id".to_owned(),
            Value::String(self.install_id.clone()),
        );
        object.insert(
            "revision".to_owned(),
            Value::Number(Number::from(self.revision)),
        );
        object.insert(
            "items".to_owned(),
            Value::Array(self.items.iter().map(item_to_value).collect()),
        );
        Value::Object(object)
    }

    /// Parse a renderer request only when its keys and values pass the strict TS shape.
    pub fn from_value(value: &Value) -> Option<Self> {
        from_value(value)
    }
}

/// Serialize one request using the exact shape consumed by `parseProbeRequest`.
pub fn to_json(request: &ProbeRequest) -> Value {
    request.to_json()
}

/// Parse one request using the exact keys and value constraints in `probe.ts`.
pub fn from_value(value: &Value) -> Option<ProbeRequest> {
    let object = value.as_object()?;
    if !has_exact_keys(object, &["install_id", "revision", "items"]) {
        return None;
    }

    let install_id = object.get("install_id")?.as_str()?;
    if !valid_install_id(install_id) {
        return None;
    }
    let revision = safe_integer(object.get("revision")?, 1, MAX_SAFE_INTEGER)?;
    let raw_items = object.get("items")?.as_array()?;
    let mut items = Vec::with_capacity(raw_items.len());
    for raw_item in raw_items {
        items.push(parse_item(raw_item, install_id)?);
    }

    Some(ProbeRequest {
        install_id: install_id.to_owned(),
        revision,
        items,
    })
}

fn item_to_value(item: &ProbeItem) -> Value {
    let mut object = Map::new();
    object.insert(
        "path_sha256".to_owned(),
        Value::String(item.path_sha256.clone()),
    );
    object.insert("url".to_owned(), Value::String(item.url.clone()));
    object.insert(
        "duration_ms".to_owned(),
        Value::Number(Number::from(item.duration_ms)),
    );
    object.insert(
        "visual_tail_ms".to_owned(),
        Value::Number(Number::from(item.visual_tail_ms)),
    );
    Value::Object(object)
}

fn parse_item(value: &Value, install_id: &str) -> Option<ProbeItem> {
    let object = value.as_object()?;
    if !has_exact_keys(
        object,
        &["path_sha256", "url", "duration_ms", "visual_tail_ms"],
    ) {
        return None;
    }
    let path_sha256 = object.get("path_sha256")?.as_str()?;
    if !valid_token(path_sha256) {
        return None;
    }
    let url = object.get("url")?.as_str()?;
    if url != logical_url(install_id, path_sha256) {
        return None;
    }
    let duration_ms = safe_integer(object.get("duration_ms")?, 1, u64::from(u32::MAX))?;
    let visual_tail_ms = safe_integer(object.get("visual_tail_ms")?, 0, u64::from(u32::MAX))?;

    Some(ProbeItem {
        path_sha256: path_sha256.to_owned(),
        url: url.to_owned(),
        duration_ms: u32::try_from(duration_ms).ok()?,
        visual_tail_ms: u32::try_from(visual_tail_ms).ok()?,
    })
}

fn has_exact_keys(object: &Map<String, Value>, expected: &[&str]) -> bool {
    object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
}

fn safe_integer(value: &Value, minimum: u64, maximum: u64) -> Option<u64> {
    let number = value.as_number()?;
    let integer = if let Some(integer) = number.as_u64() {
        integer
    } else {
        let float = number.as_f64()?;
        if !float.is_finite() || float.fract() != 0.0 || float < minimum as f64 {
            return None;
        }
        if float > maximum as f64 {
            return None;
        }
        float as u64
    };
    (integer >= minimum && integer <= maximum).then_some(integer)
}

fn valid_install_id(value: &str) -> bool {
    let Some(digits) = value.strip_prefix("ins_") else {
        return false;
    };
    digits.len() == 10 && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn logical_url(install_id: &str, path_sha256: &str) -> String {
    format!("kohbor-media://localhost/v1/{install_id}/{path_sha256}")
}

/// Single-owner state for one active install/revision. Time is supplied by callers.
#[derive(Debug, Default)]
pub struct ProbeState {
    install_id: Option<String>,
    revision: Option<u64>,
    items: Vec<ProbeItem>,
    started_ms: HashMap<String, u64>,
    results: HashMap<String, Option<bool>>,
}

impl ProbeState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the current probe set and start its per-item timers.
    pub fn begin(
        &mut self,
        install_id: &str,
        revision: u64,
        items: Vec<ProbeItem>,
        now_ms: u64,
    ) -> ProbeRequest {
        self.install_id = Some(install_id.to_owned());
        self.revision = Some(revision);
        self.items = items;
        self.started_ms.clear();
        self.results.clear();
        for item in &self.items {
            self.started_ms.insert(item.path_sha256.clone(), now_ms);
            self.results.insert(item.path_sha256.clone(), None);
        }
        ProbeRequest {
            install_id: install_id.to_owned(),
            revision,
            items: self.items.clone(),
        }
    }

    /// Accept one current report once while its item timer is still running.
    pub fn report(
        &mut self,
        install_id: &str,
        revision: u64,
        path_sha256: &str,
        ok: bool,
        duration_ms: u64,
        now_ms: u64,
    ) -> bool {
        if self.install_id.as_deref() != Some(install_id) || self.revision != Some(revision) {
            return false;
        }
        let Some(started_ms) = self.started_ms.get(path_sha256).copied() else {
            return false;
        };
        let Some(result) = self.results.get(path_sha256) else {
            return false;
        };
        let Some(elapsed_ms) = now_ms.checked_sub(started_ms) else {
            return false;
        };
        if result.is_some() || elapsed_ms > PROBE_TIMEOUT_MS {
            return false;
        }
        let Some(binding) = self
            .items
            .iter()
            .find(|item| item.path_sha256 == path_sha256)
        else {
            return false;
        };

        let lower = u64::from(binding.duration_ms).saturating_sub(FRAME_TOLERANCE_MS);
        let upper = u64::from(binding.duration_ms)
            .saturating_add(u64::from(binding.visual_tail_ms))
            .saturating_add(FRAME_TOLERANCE_MS);
        let accepted = ok && duration_ms >= lower && duration_ms <= upper;
        self.results.insert(path_sha256.to_owned(), Some(accepted));
        true
    }

    /// Turn pending items whose inclusive five-second window has elapsed into failures.
    pub fn tick(&mut self, now_ms: u64) {
        for item in &self.items {
            let token = item.path_sha256.as_str();
            let pending = self.results.get(token).is_some_and(Option::is_none);
            let expired = self.started_ms.get(token).is_some_and(|started| {
                now_ms
                    .checked_sub(*started)
                    .is_some_and(|elapsed| elapsed > PROBE_TIMEOUT_MS)
            });
            if pending && expired {
                self.results.insert(token.to_owned(), Some(false));
            }
        }
    }

    /// Return pending/failed items for a renderer remount and restart their timers.
    pub fn pending(&mut self, now_ms: u64) -> Vec<ProbeItem> {
        self.tick(now_ms);
        let mut pending = Vec::new();
        for item in &self.items {
            let Some(result) = self.results.get(&item.path_sha256).copied() else {
                continue;
            };
            if result == Some(true) {
                continue;
            }
            self.results.insert(item.path_sha256.clone(), None);
            self.started_ms.insert(item.path_sha256.clone(), now_ms);
            pending.push(item.clone());
        }
        pending
    }

    pub fn result(&self, token: &str) -> Option<bool> {
        self.results.get(token).copied().flatten()
    }

    pub fn result_by_token(&self) -> &HashMap<String, Option<bool>> {
        &self.results
    }

    pub fn all_ok(&self) -> bool {
        self.results.values().all(|result| *result == Some(true))
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.install_id = None;
        self.revision = None;
        self.items.clear();
        self.started_ms.clear();
        self.results.clear();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{from_value, ProbeItem, ProbeRequest, ProbeState};

    const INSTALL_ID: &str = "ins_0000000001";
    const TOKEN_A: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const TOKEN_B: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    fn item(token: &str, duration_ms: u32, visual_tail_ms: u32) -> ProbeItem {
        ProbeItem {
            path_sha256: token.to_owned(),
            url: format!("kohbor-media://localhost/v1/{INSTALL_ID}/{token}"),
            duration_ms,
            visual_tail_ms,
        }
    }

    fn request(items: Vec<ProbeItem>) -> ProbeRequest {
        ProbeRequest {
            install_id: INSTALL_ID.to_owned(),
            revision: 1,
            items,
        }
    }

    #[test]
    fn begin_and_report_accept_a_valid_probe_result() {
        let mut state = ProbeState::new();
        let request = state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 300)], 10);
        assert_eq!(request.install_id, INSTALL_ID);
        assert_eq!(request.revision, 1);
        assert_eq!(state.result(TOKEN_A), None);
        assert!(state.report(INSTALL_ID, 1, TOKEN_A, true, 1_250, 5_010));
        assert_eq!(state.result(TOKEN_A), Some(true));
        assert!(state.all_ok());
    }

    #[test]
    fn duration_window_includes_both_tolerance_boundaries() {
        for (duration, expected) in [
            (958, Some(true)),
            (957, Some(false)),
            (1_342, Some(true)),
            (1_343, Some(false)),
        ] {
            let mut state = ProbeState::new();
            state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 300)], 0);
            assert!(state.report(INSTALL_ID, 1, TOKEN_A, true, duration, 0));
            assert_eq!(state.result(TOKEN_A), expected, "duration {duration}");
        }
    }

    #[test]
    fn false_decode_is_an_accepted_failed_result() {
        let mut state = ProbeState::new();
        state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 300)], 0);
        assert!(state.report(INSTALL_ID, 1, TOKEN_A, false, 1_000, 1));
        assert_eq!(state.result(TOKEN_A), Some(false));
        assert!(!state.all_ok());
    }

    #[test]
    fn report_at_five_thousand_ms_is_accepted_but_expired_report_is_unchanged() {
        let mut state = ProbeState::new();
        state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 0)], 20);
        assert!(state.report(INSTALL_ID, 1, TOKEN_A, true, 1_000, 5_020));

        state.begin(INSTALL_ID, 2, vec![item(TOKEN_A, 1_000, 0)], 20);
        assert!(!state.report(INSTALL_ID, 2, TOKEN_A, true, 1_000, 5_021));
        assert_eq!(state.result(TOKEN_A), None);
        state.tick(5_021);
        assert_eq!(state.result(TOKEN_A), Some(false));
    }

    #[test]
    fn duplicate_report_is_refused_without_changing_the_result() {
        let mut state = ProbeState::new();
        state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 0)], 0);
        assert!(state.report(INSTALL_ID, 1, TOKEN_A, true, 1_000, 1));
        assert!(!state.report(INSTALL_ID, 1, TOKEN_A, false, 1_000, 2));
        assert_eq!(state.result(TOKEN_A), Some(true));
    }

    #[test]
    fn wrong_install_revision_and_unknown_token_reports_do_not_change_state() {
        let mut state = ProbeState::new();
        state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 0)], 0);
        assert!(!state.report("ins_0000000002", 1, TOKEN_A, true, 1_000, 1));
        assert!(!state.report(INSTALL_ID, 2, TOKEN_A, true, 1_000, 1));
        assert!(!state.report(INSTALL_ID, 1, TOKEN_B, true, 1_000, 1));
        assert_eq!(state.result(TOKEN_A), None);
        assert_eq!(state.result(TOKEN_B), None);
    }

    #[test]
    fn begin_replaces_the_previous_probe_set_and_refuses_old_reports() {
        let mut state = ProbeState::new();
        state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 0)], 0);
        state.begin("ins_0000000002", 2, vec![item(TOKEN_B, 2_000, 0)], 1);
        assert_eq!(state.result(TOKEN_A), None);
        assert!(!state.result_by_token().contains_key(TOKEN_A));
        assert!(!state.report(INSTALL_ID, 1, TOKEN_A, true, 1_000, 2));
        assert!(state.report("ins_0000000002", 2, TOKEN_B, true, 2_000, 2));
    }

    #[test]
    fn pending_restarts_pending_and_failed_items_once_and_omits_successes() {
        let mut state = ProbeState::new();
        state.begin(
            INSTALL_ID,
            1,
            vec![item(TOKEN_A, 1_000, 0), item(TOKEN_B, 2_000, 0)],
            0,
        );
        assert!(state.report(INSTALL_ID, 1, TOKEN_A, true, 1_000, 1));
        assert!(state.report(INSTALL_ID, 1, TOKEN_B, false, 2_000, 2));

        let pending = state.pending(10);
        assert_eq!(pending, vec![item(TOKEN_B, 2_000, 0)]);
        assert_eq!(state.result(TOKEN_B), None);
        assert!(state.report(INSTALL_ID, 1, TOKEN_B, true, 2_000, 5_010));
        assert_eq!(state.result(TOKEN_B), Some(true));
    }

    #[test]
    fn tick_marks_expired_pending_items_failed_and_is_idempotent() {
        let mut state = ProbeState::new();
        state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 0)], 0);
        state.tick(5_001);
        assert_eq!(state.result(TOKEN_A), Some(false));
        let before = state.result_by_token().clone();
        state.tick(9_000);
        assert_eq!(state.result_by_token(), &before);
    }

    #[test]
    fn all_ok_requires_every_item_to_succeed_and_empty_set_is_ok() {
        let mut state = ProbeState::new();
        assert!(state.is_empty());
        assert!(state.all_ok());
        state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 0)], 0);
        assert!(!state.is_empty());
        assert!(!state.all_ok());
        state.tick(5_001);
        assert!(!state.all_ok());
        assert_eq!(state.pending(6_000), vec![item(TOKEN_A, 1_000, 0)]);
        assert!(state.report(INSTALL_ID, 1, TOKEN_A, true, 1_000, 6_001));
        assert!(state.all_ok());
    }

    #[test]
    fn report_with_a_time_before_begin_is_refused_without_deciding_the_item() {
        let mut state = ProbeState::new();
        state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 0)], 100);
        assert!(!state.report(INSTALL_ID, 1, TOKEN_A, true, 1_000, 99));
        assert_eq!(state.result(TOKEN_A), None);
        state.tick(99);
        assert_eq!(state.result(TOKEN_A), None);
    }

    #[test]
    fn timestamps_and_duration_lower_bound_use_saturating_arithmetic() {
        let mut state = ProbeState::new();
        state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 20, 0)], u64::MAX - 5_000);
        assert!(state.report(INSTALL_ID, 1, TOKEN_A, true, 0, u64::MAX));
        assert_eq!(state.result(TOKEN_A), Some(true));
    }

    #[test]
    fn clear_removes_the_set_and_results() {
        let mut state = ProbeState::new();
        state.begin(INSTALL_ID, 1, vec![item(TOKEN_A, 1_000, 0)], 0);
        state.clear();
        assert!(state.is_empty());
        assert!(state.all_ok());
        assert!(state.result_by_token().is_empty());
        assert!(!state.report(INSTALL_ID, 1, TOKEN_A, true, 1_000, 1));
    }

    #[test]
    fn probe_request_json_has_the_exact_renderer_shape_and_integer_types() {
        let request = request(vec![item(TOKEN_A, 1_000, 300)]);
        let value = request.to_json();
        assert_eq!(
            value,
            json!({
                "install_id": INSTALL_ID,
                "revision": 1,
                "items": [{
                    "path_sha256": TOKEN_A,
                    "url": format!("kohbor-media://localhost/v1/{INSTALL_ID}/{TOKEN_A}"),
                    "duration_ms": 1_000,
                    "visual_tail_ms": 300,
                }]
            })
        );
        let object = value.as_object().expect("request JSON object");
        assert_eq!(object.len(), 3);
        assert!(object.get("revision").and_then(Value::as_u64).is_some());
        let serialized_item = object
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_object)
            .expect("item JSON object");
        assert_eq!(serialized_item.len(), 4);
        assert!(serialized_item
            .get("duration_ms")
            .and_then(Value::as_u64)
            .is_some());
        assert!(serialized_item
            .get("visual_tail_ms")
            .and_then(Value::as_u64)
            .is_some());
    }

    #[test]
    fn from_value_accepts_exact_probe_ts_request_shape() {
        let value = json!({
            "install_id": INSTALL_ID,
            "revision": 1,
            "items": [{
                "path_sha256": TOKEN_A,
                "url": format!("kohbor-media://localhost/v1/{INSTALL_ID}/{TOKEN_A}"),
                "duration_ms": 1_000,
                "visual_tail_ms": 300,
            }]
        });
        assert_eq!(
            from_value(&value),
            Some(request(vec![item(TOKEN_A, 1_000, 300)]))
        );
    }

    #[test]
    fn from_value_rejects_non_strict_renderer_shapes() {
        let valid = json!({
            "install_id": INSTALL_ID,
            "revision": 1,
            "items": [{
                "path_sha256": TOKEN_A,
                "url": format!("kohbor-media://localhost/v1/{INSTALL_ID}/{TOKEN_A}"),
                "duration_ms": 1_000,
                "visual_tail_ms": 300,
            }]
        });
        let mut cases = Vec::new();
        let mut extra_request = valid.clone();
        extra_request["extra"] = json!(true);
        cases.push(extra_request);
        let mut bad_item = valid.clone();
        bad_item["items"][0]["extra"] = json!(true);
        cases.push(bad_item);
        let mut bad_install = valid.clone();
        bad_install["install_id"] = json!("ins_123");
        cases.push(bad_install);
        let mut bad_revision = valid.clone();
        bad_revision["revision"] = json!(0);
        cases.push(bad_revision);
        let mut unsafe_revision = valid.clone();
        unsafe_revision["revision"] = json!(9_007_199_254_740_992_u64);
        cases.push(unsafe_revision);
        let mut float_revision = valid.clone();
        float_revision["revision"] = json!(1.5);
        cases.push(float_revision);
        let mut bad_url = valid.clone();
        bad_url["items"][0]["url"] = json!("https://example.invalid/video.webm");
        cases.push(bad_url);
        let mut bad_duration = valid.clone();
        bad_duration["items"][0]["duration_ms"] = json!(0);
        cases.push(bad_duration);
        let mut bad_tail = valid.clone();
        bad_tail["items"][0]["visual_tail_ms"] = json!(-1);
        cases.push(bad_tail);
        for value in cases {
            assert_eq!(
                from_value(&value),
                None,
                "accepted invalid request: {value}"
            );
        }
    }
}
