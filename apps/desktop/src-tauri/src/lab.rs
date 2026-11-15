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
        saved_checked: bool,
    }

    fn slot() -> &'static Mutex<Slot> {
        static SLOT: OnceLock<Mutex<Slot>> = OnceLock::new();
        SLOT.get_or_init(|| {
            Mutex::new(Slot {
                resolved: kohbor_lab::resolve_effective(),
                last_trace: None,
                saved_checked: false,
            })
        })
    }

    fn with_slot<T>(f: impl FnOnce(&mut Slot) -> T) -> T {
        let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }

    fn data_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
        use tauri::Manager;
        let dir = match crate::demo_data_dir_override() {
            Some(dir) => dir,
            None => app
                .path()
                .app_data_dir()
                .map_err(|_| "DATA_DIR_UNAVAILABLE".to_owned())?,
        };
        // Once per run: when no KOHBOR_LAB_PROFILE selected a file, pick up what a previous
        // Lab save left at the fixed location so edits survive a restart.
        with_slot(|s| {
            if !s.saved_checked {
                s.saved_checked = true;
                if matches!(s.resolved.source, kohbor_lab::Source::Embedded)
                    && s.resolved.rejected.is_none()
                {
                    if let Some(found) = kohbor_lab::dev_override::load_saved(&dir) {
                        s.resolved = found;
                    }
                }
            }
        });
        Ok(dir)
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
    pub fn lab_status(app: tauri::AppHandle) -> Value {
        if !enabled() {
            return json!({"available": false});
        }
        let _ = data_dir(&app);
        with_slot(|s| {
            let mut value = describe_with_context(s);
            value["available"] = json!(true);
            value
        })
    }

    #[tauri::command(async)]
    pub fn lab_describe(app: tauri::AppHandle) -> Result<Value, String> {
        require()?;
        data_dir(&app)?;
        Ok(with_slot(|s| describe_with_context(s)))
    }

    #[tauri::command(async)]
    pub fn lab_last_trace() -> Result<Value, String> {
        require()?;
        Ok(with_slot(|s| s.last_trace.clone().unwrap_or(Value::Null)))
    }

    #[tauri::command(async)]
    pub fn lab_export(app: tauri::AppHandle) -> Result<String, String> {
        require()?;
        data_dir(&app)?;
        Ok(with_slot(|s| s.resolved.effective.export_json()))
    }

    /// Validate edits against the whole profile and write the fixed dev override file
    /// `<app data dir>/lab/profile.override.json` (never a renderer-supplied or
    /// environment-supplied path; `KOHBOR_LAB_PROFILE` only selects what is READ at
    /// startup). The result is kept for the next detector rebuild.
    #[tauri::command(async)]
    pub fn lab_apply_edits(app: tauri::AppHandle, edits: Vec<Edit>) -> Result<Value, String> {
        require()?;
        let data_dir = data_dir(&app)?;
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
            let path = kohbor_lab::dev_override::save_under(&data_dir, &edited)
                .map_err(|_| "SAVE_REFUSED".to_owned())?;
            let edited = edited.with_source(kohbor_lab::Source::Override(path.clone()));
            s.resolved.effective = edited;
            s.resolved.source = kohbor_lab::Source::Override(path);
            s.resolved.rejected = None;
            Ok(describe_with_context(s))
        })
    }

    #[derive(Deserialize)]
    pub struct LayoutIn {
        pub x_percent: f64,
        pub y_percent: f64,
        pub scale: f64,
        pub enabled: bool,
    }

    fn core_layout(input: &LayoutIn) -> kohbor_lab::BannerLayout {
        kohbor_lab::BannerLayout {
            x_percent: input.x_percent,
            y_percent: input.y_percent,
            scale: input.scale,
            enabled: input.enabled,
        }
    }

    fn host_layout(layout: kohbor_lab::BannerLayout) -> crate::overlay::BannerLayout {
        crate::overlay::BannerLayout {
            x_percent: layout.x_percent,
            y_percent: layout.y_percent,
            scale: layout.scale,
            enabled: layout.enabled,
        }
    }

    /// The layout the overlay uses at show time in dev/test builds.
    pub fn banner_layout() -> crate::overlay::BannerLayout {
        host_layout(with_slot(|s| s.resolved.effective.layout().banner))
    }

    fn layout_view(s: &Slot) -> Value {
        let described = describe_with_context(s);
        let zones: Vec<Value> = crate::overlay::PROTECTED_REGIONS
            .iter()
            .map(|(name, x0, y0, x1, y1)| json!({"name": name, "x0": x0, "y0": y0, "x1": x1, "y1": y1}))
            .collect();
        json!({
            "banner": described["layout"]["banner"],
            "default": described["layout"]["default"]["banner"],
            "bounds": described["layout"]["bounds"],
            "hud_zones": zones,
            "source": described["source"],
            "override_path": described["override_path"],
            "applies": "on the next overlay show (the overlay re-places itself before every show)",
        })
    }

    #[tauri::command(async)]
    pub fn lab_layout_get(app: tauri::AppHandle) -> Result<Value, String> {
        require()?;
        data_dir(&app)?;
        Ok(with_slot(|s| layout_view(s)))
    }

    /// Where the real host would put the window for `layout` on a monitor of the given size:
    /// the same `overlay::place_overlay_with` the overlay uses, so the editor draws the clamped
    /// rectangle, not an ideal one. `stage` is the art area; the status strip hangs below it.
    #[tauri::command(async)]
    pub fn lab_layout_preview(
        layout: LayoutIn,
        monitor_w: u32,
        monitor_h: u32,
    ) -> Result<Value, String> {
        require()?;
        if !(320..=16_384).contains(&monitor_w) || !(240..=16_384).contains(&monitor_h) {
            return Err("MONITOR_SIZE_OUT_OF_RANGE".to_owned());
        }
        let monitor = crate::overlay::MonitorRect {
            x: 0,
            y: 0,
            w: monitor_w,
            h: monitor_h,
        };
        let placed =
            crate::overlay::place_overlay_with(&monitor, &host_layout(core_layout(&layout)));
        Ok(match placed {
            None => json!({"disabled": true}),
            Some(window) => {
                let (stage_w, stage_logical_h) = crate::overlay::STAGE_LOGICAL;
                let stage_h = (f64::from(window.w) * f64::from(stage_logical_h)
                    / f64::from(stage_w))
                .round() as u32;
                json!({
                    "disabled": false,
                    "window": {"x": window.x, "y": window.y, "w": window.w, "h": window.h},
                    "stage": {"x": window.x, "y": window.y, "w": window.w, "h": stage_h.min(window.h)},
                })
            }
        })
    }

    /// Validate the layout like a profile file and write the fixed dev override file.
    #[tauri::command(async)]
    pub fn lab_layout_apply(app: tauri::AppHandle, layout: LayoutIn) -> Result<Value, String> {
        require()?;
        let data_dir = data_dir(&app)?;
        with_slot(|s| {
            let edited = s
                .resolved
                .effective
                .with_layout(core_layout(&layout))
                .map_err(|e| format!("{}: {}", e.code, e.detail))?;
            let path = kohbor_lab::dev_override::save_under(&data_dir, &edited)
                .map_err(|_| "SAVE_REFUSED".to_owned())?;
            s.resolved.effective = edited.with_source(kohbor_lab::Source::Override(path.clone()));
            s.resolved.source = kohbor_lab::Source::Override(path);
            s.resolved.rejected = None;
            Ok(layout_view(s))
        })
    }

    /// The layout as JSON for copying; no paths, no identifiers.
    #[tauri::command(async)]
    pub fn lab_layout_export(app: tauri::AppHandle) -> Result<String, String> {
        require()?;
        data_dir(&app)?;
        Ok(with_slot(|s| {
            let banner = s.resolved.effective.describe_json()["layout"]["banner"].clone();
            serde_json::to_string_pretty(&json!({"layout": {"banner": banner}})).unwrap_or_default()
                + "\n"
        }))
    }

    /// The reference screenshot as a data URL, only from the fixed dev location under the app
    /// data dir (never from the repository or the installer). `None` when it is absent.
    #[tauri::command(async)]
    pub fn lab_reference_image(app: tauri::AppHandle) -> Result<Option<String>, String> {
        use base64::Engine;
        require()?;
        let data_dir = data_dir(&app)?;
        let bytes = kohbor_lab::dev_override::read_reference_image(&data_dir)
            .map_err(|_| "REFERENCE_IMAGE_REFUSED".to_owned())?;
        Ok(bytes.map(|b| {
            format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(b)
            )
        }))
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

/// Demo-only release builds (`--features demo` without `lab`, no debug assertions) still register
/// the `lab::dev::*` handler names; these stubs keep that list compiling and answer
/// "unavailable". No Lab logic, no file or environment access.
#[cfg(all(feature = "demo", not(any(feature = "lab", debug_assertions))))]
pub mod dev {
    use serde_json::{json, Value};

    const UNAVAILABLE: &str = "CAPABILITY_UNAVAILABLE";

    #[tauri::command(async)]
    pub fn lab_status() -> Value {
        json!({"available": false})
    }
    #[tauri::command(async)]
    pub fn lab_describe() -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_last_trace() -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_export() -> Result<String, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_apply_edits() -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_replay() -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_layout_get() -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_layout_preview() -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_layout_apply() -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_layout_export() -> Result<String, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_reference_image() -> Result<Option<String>, String> {
        Err(UNAVAILABLE.to_owned())
    }
}

/// The banner layout the overlay uses at show time in dev/test builds. The host lead wires this
/// into `overlay::effective_banner_layout()` (one line under the same cfg).
#[cfg(any(feature = "lab", debug_assertions))]
#[allow(dead_code)]
pub fn banner_layout() -> crate::overlay::BannerLayout {
    dev::banner_layout()
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

#[cfg(test)]
mod tests {
    use super::*;
    use kohbor_events::Phase;

    /// The host path must emit exactly what the plain detector emits (release and dev).
    #[test]
    fn host_observe_equals_plain_observe_on_the_golden_replay() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../fixtures/health-danger-golden.json"
        );
        let fixture: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("golden fixture")).unwrap();
        let mut checked = 0;
        for trace in fixture["traces"].as_array().unwrap() {
            let mut plain = Detector::with_config(Config::default());
            let mut host = Detector::with_config(current_config());
            for sample in trace["samples"].as_array().unwrap() {
                let tick = Tick {
                    at_ms: sample["t_ms"].as_u64().unwrap(),
                    session_epoch: 1,
                    phase: Phase::Live,
                    trusted: sample["availability"] == "fresh",
                    kills: Some(0),
                    deaths: Some(0),
                    kill_streak: None,
                    combined_score: Some(0),
                    level: Some(1),
                    hp: sample["hp"].as_u64().map(|n| n as u32),
                    max_hp: sample["max_hp"].as_u64().map(|n| n as u32),
                    mana_percent: Some(100),
                    alive: sample["alive"].as_bool(),
                };
                assert_eq!(observe(&mut host, &tick), plain.observe(&tick));
                checked += 1;
            }
        }
        assert!(checked > 20);
    }

    /// The Lab profile default for the banner must be the host's compiled default.
    #[cfg(any(feature = "lab", debug_assertions))]
    #[test]
    fn lab_layout_default_equals_the_overlay_default() {
        let lab = kohbor_lab::embedded().layout().banner;
        let host = crate::overlay::DEFAULT_BANNER_LAYOUT;
        assert_eq!(lab.x_percent, host.x_percent);
        assert_eq!(lab.y_percent, host.y_percent);
        assert_eq!(lab.scale, host.scale);
        assert_eq!(lab.enabled, host.enabled);
        assert_eq!(super::dev::banner_layout(), host);
    }

    #[test]
    fn default_host_config_is_the_detector_default() {
        // Dev builds without KOHBOR_LAB set resolve to the embedded default.
        if std::env::var("KOHBOR_LAB").is_err() {
            assert_eq!(current_config(), Config::default());
        }
    }
}
