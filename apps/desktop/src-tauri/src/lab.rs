//! Kohbor Lab host adapters (docs/design/KOHBOR-LAB.md). Dev-only: the commands and the
//! trace recorder exist only with feature `lab` or debug assertions, and every command
//! also needs `KOHBOR_LAB=1`. Release builds compile only `current_config` and `observe`,
//! which return the embedded default and the plain detector result.

use kohbor_events::{Config, Detector, Event, Tick};

#[cfg(any(feature = "lab", debug_assertions))]
pub mod dev {
    use super::*;
    use kohbor_lab::{sandbox, Resolved};
    use serde::Deserialize;
    use serde_json::{json, Value};
    use std::sync::{Mutex, OnceLock};

    #[derive(Deserialize)]
    pub struct Edit {
        pub event: String,
        pub name: String,
        pub value: Value,
    }

    struct Slot {
        resolved: Resolved,
        last_trace: Option<Value>,
    }

    fn slot() -> &'static Mutex<Slot> {
        static SLOT: OnceLock<Mutex<Slot>> = OnceLock::new();
        SLOT.get_or_init(|| {
            Mutex::new(Slot {
                resolved: kohbor_lab::resolve_effective(),
                last_trace: None,
            })
        })
    }

    fn with_slot<T>(f: impl FnOnce(&mut Slot) -> T) -> T {
        let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }

    pub fn enabled() -> bool {
        std::env::var("KOHBOR_LAB").ok().as_deref() == Some("1")
    }

    fn require() -> Result<(), String> {
        enabled()
            .then_some(())
            .ok_or_else(|| "CAPABILITY_UNAVAILABLE".to_owned())
    }

    pub fn current_config() -> Config {
        with_slot(|s| s.resolved.effective.config())
    }

    pub fn observe(detector: &mut Detector, tick: &Tick) -> Vec<Event> {
        let (events, trace) = detector.observe_traced(tick);
        let json = trace.to_json();
        with_slot(|s| s.last_trace = Some(json));
        events
    }

    fn describe_with_context(s: &Slot) -> Value {
        let mut value = s.resolved.effective.describe_json();
        value["override_path"] = match &s.resolved.source {
            kohbor_lab::Source::Override(path) => json!(path.to_string_lossy()),
            kohbor_lab::Source::Embedded => Value::Null,
        };
        value["rejected"] = s
            .resolved
            .rejected
            .as_ref()
            .map_or(Value::Null, |e| json!({"code": e.code, "detail": e.detail}));
        value["applies"] = json!("from the next session (detector rebuilt at a session boundary)");
        value
    }

    #[tauri::command(async)]
    pub fn lab_status() -> Value {
        if !enabled() {
            return json!({"available": false});
        }
        with_slot(|s| {
            let mut value = describe_with_context(s);
            value["available"] = json!(true);
            value
        })
    }

    #[tauri::command(async)]
    pub fn lab_describe() -> Result<Value, String> {
        require()?;
        Ok(with_slot(|s| describe_with_context(s)))
    }

    #[tauri::command(async)]
    pub fn lab_last_trace() -> Result<Value, String> {
        require()?;
        Ok(with_slot(|s| s.last_trace.clone().unwrap_or(Value::Null)))
    }

    #[tauri::command(async)]
    pub fn lab_export() -> Result<String, String> {
        require()?;
        Ok(with_slot(|s| s.resolved.effective.export_json()))
    }

    /// Validate edits against the whole profile, write the dev override file (requires
    /// `KOHBOR_LAB_PROFILE`), and keep the result for the next detector rebuild.
    #[tauri::command(async)]
    pub fn lab_apply_edits(edits: Vec<Edit>) -> Result<Value, String> {
        require()?;
        let path = std::env::var("KOHBOR_LAB_PROFILE").map_err(|_| "NO_OVERRIDE_PATH".to_owned())?;
        let path = std::path::PathBuf::from(path);
        with_slot(|s| {
            let list: Vec<(&str, &str, Value)> = edits
                .iter()
                .map(|e| (e.event.as_str(), e.name.as_str(), e.value.clone()))
                .collect();
            let edited = s
                .resolved
                .effective
                .with_edits(&list)
                .map_err(|e| format!("{}: {}", e.code, e.detail))?;
            kohbor_lab::dev_override::save(&path, &edited)
                .map_err(|_| "SAVE_FAILED".to_owned())?;
            let edited = edited.with_source(kohbor_lab::Source::Override(path.clone()));
            s.resolved.effective = edited;
            s.resolved.source = kohbor_lab::Source::Override(path);
            s.resolved.rejected = None;
            Ok(describe_with_context(s))
        })
    }

    /// Replay supplied ticks with the current profile, optionally compared with edits.
    #[tauri::command(async)]
    pub fn lab_replay(ticks: Value, edits: Option<Vec<Edit>>) -> Result<Value, String> {
        require()?;
        let ticks = sandbox::ticks_from_json(&ticks)?;
        with_slot(|s| {
            let base = s.resolved.effective.config();
            let mut out = sandbox::replay_json(&ticks, &base);
            if let Some(edits) = edits {
                let list: Vec<(&str, &str, Value)> = edits
                    .iter()
                    .map(|e| (e.event.as_str(), e.name.as_str(), e.value.clone()))
                    .collect();
                let edited = s
                    .resolved
                    .effective
                    .with_edits(&list)
                    .map_err(|e| format!("{}: {}", e.code, e.detail))?;
                out["compare"] = sandbox::compare(&ticks, &base, &edited.config());
            }
            Ok(out)
        })
    }
}

/// Parameters for a newly built detector.
#[cfg(any(feature = "lab", debug_assertions))]
pub fn current_config() -> Config {
    dev::current_config()
}

#[cfg(not(any(feature = "lab", debug_assertions)))]
pub fn current_config() -> Config {
    // Release: no file, no environment, no kohbor-lab call. The embedded default equals
    // `Config::default()` (kohbor-lab tests/profile.rs pins that).
    Config::default()
}

/// One detector step; dev builds also record the explanation for the Lab view.
#[cfg(any(feature = "lab", debug_assertions))]
pub fn observe(detector: &mut Detector, tick: &Tick) -> Vec<Event> {
    dev::observe(detector, tick)
}

#[cfg(not(any(feature = "lab", debug_assertions)))]
pub fn observe(detector: &mut Detector, tick: &Tick) -> Vec<Event> {
    detector.observe(tick)
}
