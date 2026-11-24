//! Kohbor Lab host adapters (docs/design/KOHBOR-LAB.md). Dev-only: the commands and the
//! trace recorder exist only with feature `lab` or debug assertions, and every command
//! also needs `KOHBOR_LAB=1`. Release builds compile only `current_config` and `observe`,
//! which return the embedded default and the plain detector result.

use kohbor_events::{Config, Detector, Event, Tick};

#[cfg(feature = "lab")]
#[path = "minimap_recorder/mod.rs"]
pub(crate) mod minimap_recorder;

#[cfg(any(feature = "lab", debug_assertions))]
pub mod dev {
    use super::*;
    use kohbor_lab::{sandbox, Resolved};
    use serde::Deserialize;
    use serde_json::{json, Value};
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};

    static HUD_LAYOUT_REVISION: AtomicU64 = AtomicU64::new(0);
    static HUD_PREVIEW_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

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

    fn data_dir_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
        use tauri::Manager;
        Ok(match crate::demo_data_dir_override() {
            Some(dir) => dir,
            None => app
                .path()
                .app_data_dir()
                .map_err(|_| "DATA_DIR_UNAVAILABLE".to_owned())?,
        })
    }

    fn data_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
        let dir = data_dir_path(app)?;
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

    pub fn initialize_saved(app: &tauri::AppHandle) {
        if !enabled() {
            return;
        }
        let Ok(dir) = data_dir_path(app) else {
            return;
        };
        let selected = with_slot(|s| {
            if s.resolved.rejected.is_none()
                && matches!(s.resolved.source, kohbor_lab::Source::Override(_))
            {
                Some(s.resolved.effective.clone())
            } else {
                None
            }
        });
        let selected = selected.or_else(|| {
            let can_load_saved = with_slot(|s| {
                matches!(s.resolved.source, kohbor_lab::Source::Embedded)
                    && s.resolved.rejected.is_none()
            });
            can_load_saved
                .then(|| kohbor_lab::dev_override::load_saved(&dir))
                .flatten()
                .filter(|resolved| resolved.rejected.is_none())
                .map(|resolved| resolved.effective)
        });
        if let Some(effective) = selected {
            *hud_layout_override()
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(effective.layout().hud);
        }
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

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct HudLayoutIn {
        pub id: String,
        pub x_percent: f64,
        pub y_percent: f64,
        pub scale: f64,
        pub enabled: bool,
    }

    fn host_hud_layouts(
        effective: &kohbor_lab::Effective,
    ) -> [(crate::hud::HudModule, crate::hud::HudModuleLayout); 6] {
        host_hud_layouts_from_profile(&effective.layout().hud)
    }

    fn host_hud_layouts_from_profile(
        layouts: &[kohbor_lab::HudLayout; 6],
    ) -> [(crate::hud::HudModule, crate::hud::HudModuleLayout); 6] {
        std::array::from_fn(|index| {
            let layout = layouts[index];
            (
                crate::hud::DEFAULT_HUD_LAYOUTS[index].0,
                crate::hud::HudModuleLayout {
                    x_percent: layout.x_percent,
                    y_percent: layout.y_percent,
                    scale: layout.scale,
                    enabled: layout.enabled,
                },
            )
        })
    }

    pub fn hud_layouts() -> [(crate::hud::HudModule, crate::hud::HudModuleLayout); 6] {
        // Copy the option out FIRST so the OVERRIDE guard is dropped before the SLOT lock is taken.
        // (An `if let` scrutinee keeps its temporary guard alive through the `else` branch in
        // edition 2021, which gave the lock order OVERRIDE -> SLOT here while apply/reset take
        // SLOT -> OVERRIDE: a deadlock when the first save overlapped the 500 ms HUD poll.)
        let saved = *hud_layout_override()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match saved {
            Some(layouts) => host_hud_layouts_from_profile(&layouts),
            None => with_slot(|s| host_hud_layouts(&s.resolved.effective)),
        }
    }

    fn hud_layout_override() -> &'static Mutex<Option<[kohbor_lab::HudLayout; 6]>> {
        static HUD_LAYOUT: OnceLock<Mutex<Option<[kohbor_lab::HudLayout; 6]>>> = OnceLock::new();
        HUD_LAYOUT.get_or_init(|| Mutex::new(None))
    }

    fn set_hud_layout_override(layouts: [kohbor_lab::HudLayout; 6]) {
        *hud_layout_override()
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(layouts);
    }

    pub fn hud_layout_revision() -> u64 {
        HUD_LAYOUT_REVISION.load(Ordering::SeqCst)
    }

    fn epoch_now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0)
    }

    fn preview_active_at(deadline: u64, now_ms: u64) -> bool {
        deadline != 0 && now_ms < deadline
    }

    pub fn hud_preview_active() -> bool {
        preview_active_at(HUD_PREVIEW_UNTIL_MS.load(Ordering::SeqCst), epoch_now_ms())
    }

    fn set_hud_preview_at(
        preview_until: &AtomicU64,
        seconds: u64,
        now_ms: u64,
    ) -> Result<u64, String> {
        if !(1..=60).contains(&seconds) {
            return Err("PREVIEW_SECONDS_OUT_OF_RANGE".to_owned());
        }
        let deadline = now_ms.saturating_add(seconds.saturating_mul(1000));
        if deadline <= now_ms {
            return Err("PREVIEW_DEADLINE_OUT_OF_RANGE".to_owned());
        }
        preview_until.store(deadline, Ordering::SeqCst);
        Ok(deadline)
    }

    fn hud_layout_rows(layouts: &[kohbor_lab::HudLayout; 6]) -> Vec<Value> {
        kohbor_lab::HUD_MODULE_IDS
            .iter()
            .zip(layouts)
            .map(|(id, layout)| {
                json!({
                    "id": id,
                    "x_percent": layout.x_percent,
                    "y_percent": layout.y_percent,
                    "scale": layout.scale,
                    "enabled": layout.enabled,
                })
            })
            .collect()
    }

    fn hud_layout_view(effective: &kohbor_lab::Effective) -> Value {
        let current = effective.layout().hud;
        let defaults = kohbor_lab::embedded().layout().hud;
        let layouts = host_hud_layouts(effective);
        let source = effective.describe_json()["source"].clone();
        json!({
            "modules": hud_layout_rows(&current),
            "default": hud_layout_rows(&defaults),
            "bounds": {
                "x_percent": [kohbor_lab::PERCENT_BOUNDS.0, kohbor_lab::PERCENT_BOUNDS.1],
                "y_percent": [kohbor_lab::PERCENT_BOUNDS.0, kohbor_lab::PERCENT_BOUNDS.1],
                "scale": [kohbor_lab::SCALE_BOUNDS.0, kohbor_lab::SCALE_BOUNDS.1],
            },
            "strip_height_percent": crate::hud::strip_height_percent(&layouts),
            "source": source,
        })
    }

    fn apply_hud_layout_at(
        data_dir: &Path,
        effective: &kohbor_lab::Effective,
        modules: &[HudLayoutIn],
        revision: &AtomicU64,
    ) -> Result<kohbor_lab::Effective, String> {
        let updates: Vec<(&str, kohbor_lab::HudLayout)> = modules
            .iter()
            .map(|module| {
                (
                    module.id.as_str(),
                    kohbor_lab::HudLayout {
                        x_percent: module.x_percent,
                        y_percent: module.y_percent,
                        scale: module.scale,
                        enabled: module.enabled,
                    },
                )
            })
            .collect();
        let edited = effective
            .with_hud_layout(&updates)
            .map_err(|error| format!("{}: {}", error.code, error.detail))?;
        let path = kohbor_lab::dev_override::save_under(data_dir, &edited)
            .map_err(|_| "SAVE_REFUSED".to_owned())?;
        revision.fetch_add(1, Ordering::SeqCst);
        Ok(edited.with_source(kohbor_lab::Source::Override(path)))
    }

    fn reset_hud_layout_at(
        data_dir: &Path,
        effective: &kohbor_lab::Effective,
        revision: &AtomicU64,
    ) -> Result<kohbor_lab::Effective, String> {
        let defaults = kohbor_lab::embedded().layout().hud;
        let modules: Vec<HudLayoutIn> = kohbor_lab::HUD_MODULE_IDS
            .iter()
            .zip(defaults)
            .map(|(id, layout)| HudLayoutIn {
                id: (*id).to_owned(),
                x_percent: layout.x_percent,
                y_percent: layout.y_percent,
                scale: layout.scale,
                enabled: layout.enabled,
            })
            .collect();
        apply_hud_layout_at(data_dir, effective, &modules, revision)
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

    #[tauri::command(async)]
    pub fn lab_hud_layout_get(app: tauri::AppHandle) -> Result<Value, String> {
        require()?;
        data_dir(&app)?;
        Ok(with_slot(|s| hud_layout_view(&s.resolved.effective)))
    }

    #[tauri::command(async)]
    pub fn lab_hud_layout_apply(
        app: tauri::AppHandle,
        modules: Vec<HudLayoutIn>,
    ) -> Result<Value, String> {
        require()?;
        let data_dir = data_dir(&app)?;
        with_slot(|s| {
            let edited = apply_hud_layout_at(
                &data_dir,
                &s.resolved.effective,
                &modules,
                &HUD_LAYOUT_REVISION,
            )?;
            set_hud_layout_override(edited.layout().hud);
            s.resolved.source = edited.source().clone();
            s.resolved.effective = edited;
            s.resolved.rejected = None;
            Ok(hud_layout_view(&s.resolved.effective))
        })
    }

    #[tauri::command(async)]
    pub fn lab_hud_layout_reset(app: tauri::AppHandle) -> Result<Value, String> {
        require()?;
        let data_dir = data_dir(&app)?;
        with_slot(|s| {
            let reset =
                reset_hud_layout_at(&data_dir, &s.resolved.effective, &HUD_LAYOUT_REVISION)?;
            set_hud_layout_override(reset.layout().hud);
            s.resolved.source = reset.source().clone();
            s.resolved.effective = reset;
            s.resolved.rejected = None;
            Ok(hud_layout_view(&s.resolved.effective))
        })
    }

    /// The preview is refused while the real HUD is visible (a verified match), so synthetic numbers
    /// never replace real match data.
    pub(super) fn preview_refusal(real_hud_visible: bool) -> Result<(), String> {
        if real_hud_visible {
            Err("MATCH_ACTIVE".to_owned())
        } else {
            Ok(())
        }
    }

    #[tauri::command(async)]
    pub fn lab_hud_preview(seconds: u64) -> Result<Value, String> {
        require()?;
        // Never replace real match data with synthetic numbers.
        preview_refusal(crate::hud::real_hud_visible())?;
        let preview_until_ms = set_hud_preview_at(&HUD_PREVIEW_UNTIL_MS, seconds, epoch_now_ms())?;
        Ok(json!({"preview_until_ms": preview_until_ms}))
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

    #[cfg(test)]
    mod hud_layout_tests {
        use super::*;
        use std::path::PathBuf;
        use std::sync::atomic::{AtomicU64, Ordering};

        fn temp_data_dir(tag: &str) -> PathBuf {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "kohbor-hudlab-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }

        fn six_modules() -> Vec<HudLayoutIn> {
            [
                ("gmeter", 50.0, 5.5, 0.65),
                ("kda", 9.64, 2.2, 0.78),
                ("gold", 13.56, 2.2, 0.78),
                ("gpm", 17.23, 2.2, 0.78),
                ("nw", 21.02, 2.2, 0.78),
                ("xpm", 24.82, 2.2, 0.78),
            ]
            .into_iter()
            .map(|(id, x_percent, y_percent, scale)| HudLayoutIn {
                id: id.to_owned(),
                x_percent,
                y_percent,
                scale,
                enabled: true,
            })
            .collect()
        }

        fn embedded_with_other_edits() -> kohbor_lab::Effective {
            kohbor_lab::embedded()
                .with_layout(kohbor_lab::BannerLayout {
                    x_percent: 37.0,
                    ..kohbor_lab::embedded().layout().banner
                })
                .unwrap()
                .with_edits(&[("health_danger", "cooldown_ms", json!(60000))])
                .unwrap()
        }

        #[test]
        fn get_view_has_six_current_and_default_modules_and_bounds() {
            let view = hud_layout_view(&kohbor_lab::embedded());
            assert_eq!(view["modules"].as_array().unwrap().len(), 6);
            assert_eq!(view["default"].as_array().unwrap().len(), 6);
            assert_eq!(
                view["modules"][0],
                json!({"id":"gmeter","x_percent":50.0,"y_percent":5.5,"scale":0.65,"enabled":true})
            );
            assert_eq!(
                view["bounds"],
                json!({
                    "x_percent":[0.0,100.0],"y_percent":[0.0,100.0],"scale":[0.2,3.0]
                })
            );
            assert_eq!(view["strip_height_percent"], json!(9.0));
            assert_eq!(view["source"], json!("embedded"));
            let keys: Vec<_> = view
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(
                keys,
                [
                    "bounds",
                    "default",
                    "modules",
                    "source",
                    "strip_height_percent"
                ]
            );
        }

        #[test]
        fn apply_saves_at_the_fixed_confined_path_and_preserves_profile_values() {
            let dir = temp_data_dir("apply");
            let revision = AtomicU64::new(0);
            let mut modules = six_modules();
            modules[0].x_percent = 64.0;
            let edited =
                apply_hud_layout_at(&dir, &embedded_with_other_edits(), &modules, &revision)
                    .unwrap();
            let expected = dir.join("lab").join("profile.override.json");
            assert_eq!(
                edited.source(),
                &kohbor_lab::Source::Override(expected.clone())
            );
            let saved: Value =
                serde_json::from_str(&std::fs::read_to_string(&expected).unwrap()).unwrap();
            assert_eq!(saved["layout"]["hud"]["gmeter"]["x_percent"], json!(64.0));
            assert_eq!(saved["layout"]["banner"]["x_percent"], json!(37.0));
            assert_eq!(
                saved["events"]["health_danger"]["params"]["cooldown_ms"],
                json!(60000)
            );
            assert_eq!(revision.load(Ordering::SeqCst), 1);
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn apply_rejects_unknown_module_ids_without_writing() {
            let dir = temp_data_dir("bad-id");
            let mut modules = six_modules();
            modules[0].id = "../../override".to_owned();
            let error =
                apply_hud_layout_at(&dir, &kohbor_lab::embedded(), &modules, &AtomicU64::new(0))
                    .unwrap_err();
            assert!(error.starts_with("UNKNOWN_MODULE"), "{error}");
            assert!(!dir.join("lab").exists());
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn apply_rejects_out_of_bounds_values_without_writing() {
            let dir = temp_data_dir("bad-bounds");
            let mut modules = six_modules();
            modules[1].scale = 3.01;
            let error =
                apply_hud_layout_at(&dir, &kohbor_lab::embedded(), &modules, &AtomicU64::new(0))
                    .unwrap_err();
            assert!(error.starts_with("LAYOUT_OUT_OF_RANGE"), "{error}");
            assert!(!dir.join("lab").exists());
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn apply_rejects_duplicate_module_ids() {
            let dir = temp_data_dir("duplicate");
            let mut modules = six_modules();
            modules[1].id = "gmeter".to_owned();
            let error =
                apply_hud_layout_at(&dir, &kohbor_lab::embedded(), &modules, &AtomicU64::new(0))
                    .unwrap_err();
            assert!(error.starts_with("DUPLICATE_KEY"), "{error}");
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn apply_rejects_non_finite_values() {
            let dir = temp_data_dir("non-finite");
            let mut modules = six_modules();
            modules[0].y_percent = f64::INFINITY;
            let error =
                apply_hud_layout_at(&dir, &kohbor_lab::embedded(), &modules, &AtomicU64::new(0))
                    .unwrap_err();
            assert!(error.starts_with("LAYOUT_OUT_OF_RANGE"), "{error}");
            assert!(!dir.join("lab").exists());
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn apply_command_input_rejects_unknown_fields() {
            let parsed: Result<Vec<HudLayoutIn>, _> = serde_json::from_value(json!([{
                "id":"gmeter","x_percent":50,"y_percent":5.5,"scale":0.65,
                "enabled":true,"opacity":0.5
            }]));
            assert!(parsed.is_err());
        }

        #[test]
        fn reset_restores_only_hud_defaults_and_keeps_banner_and_event_edits() {
            let dir = temp_data_dir("reset");
            let revision = AtomicU64::new(0);
            let mut modules = six_modules();
            modules[0].x_percent = 64.0;
            let edited =
                apply_hud_layout_at(&dir, &embedded_with_other_edits(), &modules, &revision)
                    .unwrap();
            let reset = reset_hud_layout_at(&dir, &edited, &revision).unwrap();
            assert_eq!(reset.layout().hud, kohbor_lab::embedded().layout().hud);
            assert_eq!(reset.layout().banner.x_percent, 37.0);
            assert_eq!(reset.config().danger_cooldown_ms, 60000);
            assert_eq!(revision.load(Ordering::SeqCst), 2);
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn reset_writes_the_fixed_profile_override_file() {
            let dir = temp_data_dir("reset-file");
            let reset =
                reset_hud_layout_at(&dir, &kohbor_lab::embedded(), &AtomicU64::new(0)).unwrap();
            let path = dir.join("lab").join("profile.override.json");
            assert_eq!(reset.source(), &kohbor_lab::Source::Override(path.clone()));
            assert!(path.is_file());
            let saved: Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            assert_eq!(saved["layout"]["hud"]["gmeter"]["x_percent"], json!(50.0));
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn preview_is_refused_while_the_real_hud_is_visible() {
            assert_eq!(preview_refusal(true), Err("MATCH_ACTIVE".to_owned()));
            assert_eq!(preview_refusal(false), Ok(()));
        }

        #[test]
        fn preview_deadline_is_atomic_and_uses_requested_duration() {
            let preview_until = AtomicU64::new(0);
            let deadline = set_hud_preview_at(&preview_until, 7, 10_000).unwrap();
            assert_eq!(deadline, 17_000);
            assert_eq!(preview_until.load(Ordering::SeqCst), deadline);
        }

        #[test]
        fn preview_is_active_before_its_deadline_and_expires_at_deadline() {
            let preview_until = AtomicU64::new(0);
            set_hud_preview_at(&preview_until, 3, 1_000).unwrap();
            assert!(preview_active_at(
                preview_until.load(Ordering::SeqCst),
                3_999
            ));
            assert!(!preview_active_at(
                preview_until.load(Ordering::SeqCst),
                4_000
            ));
        }

        #[test]
        fn preview_duration_must_be_from_one_through_sixty_seconds() {
            let preview_until = AtomicU64::new(0);
            assert!(set_hud_preview_at(&preview_until, 0, 0).is_err());
            assert!(set_hud_preview_at(&preview_until, 61, 0).is_err());
        }
    }

    /// Lock-order regression: apply/reset take SLOT then OVERRIDE; `hud_layouts` must never hold
    /// OVERRIDE while it takes SLOT. Two threads hammer both orders; a lock-order inversion would
    /// hang, and the watchdog turns that hang into a failure instead of a stuck test run.
    #[cfg(test)]
    mod lock_order_tests {
        use super::*;
        use std::sync::mpsc;
        use std::time::Duration;

        #[test]
        fn hud_layouts_never_holds_the_override_lock_while_taking_the_slot_lock() {
            const ROUNDS: usize = 20_000;
            let defaults = kohbor_lab::embedded().layout().hud;
            let (done_tx, done_rx) = mpsc::channel();
            let writer_tx = done_tx.clone();
            let writer = std::thread::spawn(move || {
                for _ in 0..ROUNDS {
                    // The same order as lab_hud_layout_apply: SLOT, then OVERRIDE.
                    with_slot(|_| set_hud_layout_override(defaults));
                    *hud_layout_override()
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = None;
                }
                let _ = writer_tx.send("writer");
            });
            let reader = std::thread::spawn(move || {
                for _ in 0..ROUNDS {
                    let _ = hud_layouts();
                }
                let _ = done_tx.send("reader");
            });
            for _ in 0..2 {
                done_rx
                    .recv_timeout(Duration::from_secs(60))
                    .expect("lock-order inversion between hud_layouts and the apply path");
            }
            writer.join().unwrap();
            reader.join().unwrap();
            *hud_layout_override()
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = None;
        }
    }

    fn require_control_window(window: &tauri::WebviewWindow) -> Result<(), String> {
        if window.label() == "control" {
            Ok(())
        } else {
            Err("CAPABILITY_UNAVAILABLE".to_owned())
        }
    }

    #[cfg(feature = "lab")]
    fn minimap_recorder_service(
        app: &tauri::AppHandle,
    ) -> Result<std::sync::Arc<super::minimap_recorder::RecorderService>, String> {
        require()?;
        let data_dir = data_dir_path(app)?;
        let key = crate::secrets::load_or_create(&data_dir.join("install-key.bin"), || {
            crate::secrets::random::<32>().map(|key| key.to_vec())
        })
            .filter(|key| key.len() == 32)
            .ok_or_else(|| "INSTALL_KEY_UNAVAILABLE".to_owned())?;
        super::minimap_recorder::initialize(data_dir, key)
    }

    #[tauri::command(async)]
    pub fn lab_minimap_recorder_status(
        app: tauri::AppHandle,
        window: tauri::WebviewWindow,
    ) -> Result<Value, String> {
        require_control_window(&window)?;
        #[cfg(feature = "lab")]
        {
            minimap_recorder_service(&app)?.status_value()
        }
        #[cfg(not(feature = "lab"))]
        {
            let _ = app;
            Err("CAPABILITY_UNAVAILABLE".to_owned())
        }
    }

    #[tauri::command(async)]
    pub fn lab_minimap_recorder_set(
        app: tauri::AppHandle,
        window: tauri::WebviewWindow,
        enabled: bool,
        sampling_rate_fps: u8,
        total_cap_bytes: u64,
    ) -> Result<Value, String> {
        require_control_window(&window)?;
        #[cfg(feature = "lab")]
        {
            let service = minimap_recorder_service(&app)?;
            service.set_config(super::minimap_recorder::RecorderConfig {
                enabled,
                sampling_rate_fps,
                total_cap_bytes,
            })?;
            service.status_value()
        }
        #[cfg(not(feature = "lab"))]
        {
            let _ = (app, enabled, sampling_rate_fps, total_cap_bytes);
            Err("CAPABILITY_UNAVAILABLE".to_owned())
        }
    }

    #[tauri::command(async)]
    pub fn lab_minimap_recorder_delete(
        app: tauri::AppHandle,
        window: tauri::WebviewWindow,
        match_hash: String,
    ) -> Result<Value, String> {
        require_control_window(&window)?;
        #[cfg(feature = "lab")]
        {
            let service = minimap_recorder_service(&app)?;
            service.delete_match(&match_hash)?;
            service.status_value()
        }
        #[cfg(not(feature = "lab"))]
        {
            let _ = (app, match_hash);
            Err("CAPABILITY_UNAVAILABLE".to_owned())
        }
    }

    #[tauri::command(async)]
    pub fn lab_minimap_recorder_open_folder(
        app: tauri::AppHandle,
        window: tauri::WebviewWindow,
    ) -> Result<(), String> {
        require_control_window(&window)?;
        #[cfg(feature = "lab")]
        {
            let service = minimap_recorder_service(&app)?;
            service.ensure_folder()?;
            std::process::Command::new("explorer.exe")
                .arg(service.folder())
                .spawn()
                .map(|_| ())
                .map_err(|_| "MINIMAP_RECORDING_FOLDER_OPEN_FAILED".to_owned())
        }
        #[cfg(not(feature = "lab"))]
        {
            let _ = app;
            Err("CAPABILITY_UNAVAILABLE".to_owned())
        }
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
    pub fn lab_hud_layout_get() -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_hud_layout_apply(modules: Value) -> Result<Value, String> {
        let _ = modules;
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_hud_layout_reset() -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_hud_preview(seconds: u64) -> Result<Value, String> {
        let _ = seconds;
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_reference_image() -> Result<Option<String>, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_minimap_recorder_status(
        _app: tauri::AppHandle,
        _window: tauri::WebviewWindow,
    ) -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_minimap_recorder_set(
        _app: tauri::AppHandle,
        _window: tauri::WebviewWindow,
        _enabled: bool,
        _sampling_rate_fps: u8,
        _total_cap_bytes: u64,
    ) -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_minimap_recorder_delete(
        _app: tauri::AppHandle,
        _window: tauri::WebviewWindow,
        _match_hash: String,
    ) -> Result<Value, String> {
        Err(UNAVAILABLE.to_owned())
    }
    #[tauri::command(async)]
    pub fn lab_minimap_recorder_open_folder(
        _app: tauri::AppHandle,
        _window: tauri::WebviewWindow,
    ) -> Result<(), String> {
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

#[cfg(any(feature = "lab", debug_assertions))]
pub fn initialize_saved(app: &tauri::AppHandle<tauri::Wry>) {
    dev::initialize_saved(app)
}

#[cfg(any(feature = "lab", debug_assertions))]
pub fn hud_layouts() -> [(crate::hud::HudModule, crate::hud::HudModuleLayout); 6] {
    dev::hud_layouts()
}

#[cfg(any(feature = "lab", debug_assertions))]
pub fn hud_layout_revision() -> u64 {
    dev::hud_layout_revision()
}

#[cfg(any(feature = "lab", debug_assertions))]
pub fn hud_preview_active() -> bool {
    dev::hud_preview_active()
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

    #[cfg(any(feature = "lab", debug_assertions))]
    #[test]
    fn embedded_hud_defaults_equal_the_compiled_host_defaults() {
        let embedded = kohbor_lab::embedded().layout().hud;
        for (index, ((id, actual), (module, expected))) in kohbor_lab::HUD_MODULE_IDS
            .iter()
            .zip(embedded)
            .zip(crate::hud::DEFAULT_HUD_LAYOUTS)
            .enumerate()
        {
            assert_eq!(*id, module.as_str(), "module at index {index}");
            assert_eq!(actual.x_percent, expected.x_percent, "{id}.x_percent");
            assert_eq!(actual.y_percent, expected.y_percent, "{id}.y_percent");
            assert_eq!(actual.scale, expected.scale, "{id}.scale");
            assert_eq!(actual.enabled, expected.enabled, "{id}.enabled");
        }
    }

    #[test]
    fn default_host_config_is_the_detector_default() {
        // Dev builds without KOHBOR_LAB set resolve to the embedded default.
        if std::env::var("KOHBOR_LAB").is_err() {
            assert_eq!(current_config(), Config::default());
        }
    }
}
