//! MVP-1 W3 scaffold (frozen at W0): capture thread, T0 frame channel, status events.
//! `start` and the commands are placeholders until W3. Monitoring values are unknown (`None`), never 0.

// W0 scaffold: W3 removes this allow when the module is wired.
#![allow(dead_code)]

use serde_json::Value;
use std::sync::Arc;
use tauri::ipc::{Channel, InvokeResponseBody};

/// Own-hero facts read from the intake snapshot. Every field is unknown (`None`) unless the snapshot says otherwise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GsiSnapshot {
    /// "radiant" or "dire".
    pub team: Option<String>,
    pub alive: Option<bool>,
    pub hp_pct: Option<u8>,
    /// True only while the snapshot carries an own-hero object (verified, pregame or live).
    pub in_match: bool,
    pub age_ms: Option<u64>,
}

impl GsiSnapshot {
    /// Parses an own-snapshot 2.0.0 value. Missing or invalid fields stay `None`.
    pub fn from_snapshot(snapshot: &Value) -> Self {
        let own = &snapshot["own"];
        let value = |name: &str| &own["fields"][name]["value"];
        let hp = value("hp").as_u64();
        let max_hp = value("max_hp").as_u64();
        let hp_pct = match (hp, max_hp) {
            (Some(hp), Some(max)) if max > 0 => Some(((hp * 100) / max).min(100) as u8),
            _ => None,
        };
        Self {
            team: own["team"].as_str().map(str::to_owned),
            alive: value("alive").as_bool(),
            hp_pct,
            in_match: own.is_object(),
            age_ms: snapshot["source"]["age_ms"].as_u64(),
        }
    }
}

/// Read-only accessor for the own-hero facts.
#[derive(Clone)]
pub struct GsiView(Arc<dyn Fn() -> GsiSnapshot + Send + Sync>);

impl GsiView {
    pub fn new(read: impl Fn() -> GsiSnapshot + Send + Sync + 'static) -> Self {
        Self(Arc::new(read))
    }
    pub fn read(&self) -> GsiSnapshot {
        (self.0)()
    }
}

/// App-group resource sample (the same one `runtime_get_status` uses). `None` is unknown, never 0.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResourceSample {
    pub cpu_percent: Option<f64>,
    pub working_set_bytes: Option<u64>,
}

/// Read-only accessor. Monitoring only: no code path may use it to admit, throttle or refuse anything.
#[derive(Clone)]
pub struct ResourceView(Arc<dyn Fn() -> ResourceSample + Send + Sync>);

impl ResourceView {
    pub fn new(read: impl Fn() -> ResourceSample + Send + Sync + 'static) -> Self {
        Self(Arc::new(read))
    }
    pub fn read(&self) -> ResourceSample {
        (self.0)()
    }
}

/// Starts the capture host. W0 placeholder: starts nothing.
pub fn start(_app: tauri::AppHandle, _gsi: GsiView, _resources: ResourceView) {}

#[tauri::command(async)]
pub(crate) fn minimap_subscribe(_on_frame: Channel<InvokeResponseBody>) -> Result<(), String> {
    Err("CAPTURE_NOT_IMPLEMENTED".to_owned())
}

#[tauri::command(async)]
pub(crate) fn capture_status() -> Value {
    serde_json::json!({"phase": "unsupported", "reason": "not_implemented"})
}

#[tauri::command(async)]
pub(crate) fn capture_invalidate() -> Result<(), String> {
    Err("CAPTURE_NOT_IMPLEMENTED".to_owned())
}

#[cfg(test)]
mod tests {
    use super::GsiSnapshot;
    use serde_json::json;

    #[test]
    fn unknown_snapshot_gives_unknown_fields() {
        let s = GsiSnapshot::from_snapshot(&json!({"own": null, "source": {"age_ms": null}}));
        assert_eq!(s, GsiSnapshot::default());
    }

    #[test]
    fn snapshot_fields_are_read_and_hp_percent_is_derived() {
        let s = GsiSnapshot::from_snapshot(&json!({
            "own": {"team": "dire", "fields": {
                "hp": {"value": 150, "availability": "fresh"},
                "max_hp": {"value": 600, "availability": "fresh"},
                "alive": {"value": true, "availability": "fresh"}}},
            "source": {"age_ms": 120}
        }));
        assert_eq!(s.team.as_deref(), Some("dire"));
        assert_eq!(s.hp_pct, Some(25));
        assert_eq!(s.alive, Some(true));
        assert!(s.in_match);
        assert_eq!(s.age_ms, Some(120));
    }

    #[test]
    fn zero_max_hp_is_unknown_not_zero_percent() {
        let s = GsiSnapshot::from_snapshot(&json!({
            "own": {"fields": {"hp": {"value": 0}, "max_hp": {"value": 0}}}, "source": {}
        }));
        assert_eq!(s.hp_pct, None);
    }
}
