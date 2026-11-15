use serde_json::{Map, Value};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::Emitter;

use crate::intake::Runtime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum HudModule {
    Gmeter,
    Kda,
    Gold,
    Gpm,
    Nw,
    Xpm,
}

#[cfg_attr(not(test), allow(dead_code))]
impl HudModule {
    #[cfg_attr(not(test), allow(dead_code))]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gmeter => "gmeter",
            Self::Kda => "kda",
            Self::Gold => "gold",
            Self::Gpm => "gpm",
            Self::Nw => "nw",
            Self::Xpm => "xpm",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct HudModuleLayout {
    pub x_percent: f64,
    pub y_percent: f64,
    pub scale: f64,
    pub enabled: bool,
}

impl HudModuleLayout {
    #[cfg_attr(not(test), allow(dead_code))]
    fn clamped(self) -> Self {
        Self {
            x_percent: clamp_value(self.x_percent, 0.0, 100.0),
            y_percent: clamp_value(self.y_percent, 0.0, 100.0),
            scale: clamp_value(self.scale, 0.2, 3.0),
            enabled: self.enabled,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn clamp_value(value: f64, min: f64, max: f64) -> f64 {
    if value.is_nan() {
        min
    } else {
        value.clamp(min, max)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_HUD_LAYOUTS: [(HudModule, HudModuleLayout); 6] = [
    (
        HudModule::Gmeter,
        HudModuleLayout {
            x_percent: 50.0,
            y_percent: 5.5,
            scale: 0.65,
            enabled: true,
        },
    ),
    (
        HudModule::Kda,
        HudModuleLayout {
            x_percent: 9.64,
            y_percent: 2.2,
            scale: 0.78,
            enabled: true,
        },
    ),
    (
        HudModule::Gold,
        HudModuleLayout {
            x_percent: 13.56,
            y_percent: 2.2,
            scale: 0.78,
            enabled: true,
        },
    ),
    (
        HudModule::Gpm,
        HudModuleLayout {
            x_percent: 17.23,
            y_percent: 2.2,
            scale: 0.78,
            enabled: true,
        },
    ),
    (
        HudModule::Nw,
        HudModuleLayout {
            x_percent: 21.02,
            y_percent: 2.2,
            scale: 0.78,
            enabled: true,
        },
    ),
    (
        HudModule::Xpm,
        HudModuleLayout {
            x_percent: 24.82,
            y_percent: 2.2,
            scale: 0.78,
            enabled: true,
        },
    ),
];

/// The Lab will hook here later; both builds use the owner-approved default for now.
#[cfg_attr(not(test), allow(dead_code))]
pub fn effective_hud_layouts() -> [(HudModule, HudModuleLayout); 6] {
    #[cfg(any(feature = "lab", debug_assertions))]
    {
        DEFAULT_HUD_LAYOUTS
    }
    #[cfg(not(any(feature = "lab", debug_assertions)))]
    {
        DEFAULT_HUD_LAYOUTS
    }
}

pub fn place_hud_window(monitor: &crate::overlay::MonitorRect) -> crate::overlay::Rect {
    crate::overlay::Rect {
        x: monitor.x,
        y: monitor.y,
        w: monitor.w,
        h: (f64::from(monitor.h) * 0.09).round().max(1.0) as u32,
    }
}

/// Module centers are relative to the HUD window, whose origin equals the monitor origin.
#[cfg_attr(not(test), allow(dead_code))]
pub fn module_pixel_center(
    monitor: &crate::overlay::MonitorRect,
    layout: &HudModuleLayout,
) -> (f64, f64) {
    let layout = layout.clamped();
    (
        f64::from(monitor.w) * layout.x_percent / 100.0,
        f64::from(monitor.h) * layout.y_percent / 100.0,
    )
}

pub struct HudWindowSpec {
    pub label: &'static str,
    pub url: &'static str,
    pub transparent: bool,
    pub decorations: bool,
    pub always_on_top: bool,
    pub skip_taskbar: bool,
    pub focusable: bool,
    pub focused: bool,
    pub shadow: bool,
    pub resizable: bool,
    pub visible: bool,
    pub ignore_cursor_events: bool,
}

pub const fn hud_window_spec() -> HudWindowSpec {
    HudWindowSpec {
        label: "hud",
        url: "index.html?view=hud",
        transparent: true,
        decorations: false,
        always_on_top: true,
        skip_taskbar: true,
        focusable: false,
        focused: false,
        shadow: false,
        resizable: false,
        visible: false,
        ignore_cursor_events: true,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub const fn hud_capability_permissions() -> &'static [&'static str] {
    &["core:event:default"]
}

const HUD_STAT_FIELDS: [&str; 6] = ["kills", "deaths", "assists", "gold", "gpm", "xpm"];
const HUD_POLL_INTERVAL: Duration = Duration::from_millis(500);
const HUD_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(2);
const HUD_STATE_EVENT: &str = "hud://state";

/// The snapshot builder exposes fresh/stale only after verifying ownership and pregame/live phase.
/// Any other top-level availability is deliberately reduced to an all-unknown hidden payload.
pub fn build_hud_state(snapshot: &Value) -> Value {
    let visible = matches!(
        snapshot.get("availability").and_then(Value::as_str),
        Some("fresh" | "stale")
    ) && snapshot
        .pointer("/own/fields")
        .and_then(Value::as_object)
        .is_some();

    let mut fields = Map::new();
    for name in HUD_STAT_FIELDS {
        let value = if visible {
            snapshot_field(snapshot, name)
        } else {
            unknown_field()
        };
        fields.insert(name.to_owned(), value);
    }

    serde_json::json!({
        "schema": 1,
        "visible": visible,
        "fields": fields,
        "net_worth": {"value": null, "availability": "unsupported"}
    })
}

fn unknown_field() -> Value {
    serde_json::json!({"value": null, "availability": "unknown"})
}

fn snapshot_field(snapshot: &Value, name: &str) -> Value {
    let Some(field) = snapshot.pointer(&format!("/own/fields/{name}")) else {
        return unknown_field();
    };
    let Some(availability) = field.get("availability").and_then(Value::as_str) else {
        return unknown_field();
    };

    match availability {
        "fresh" | "stale" => {
            let Some(value) = field
                .get("value")
                .filter(|value| value.as_i64().is_some() || value.as_u64().is_some())
            else {
                return unknown_field();
            };
            serde_json::json!({"value": value, "availability": availability})
        }
        "unknown" | "invalid" | "unsupported" => {
            serde_json::json!({"value": null, "availability": availability})
        }
        _ => unknown_field(),
    }
}

#[derive(Default)]
struct EmissionTracker {
    last_payload: Option<Value>,
    last_emitted_at: Option<Instant>,
}

impl EmissionTracker {
    fn should_emit(&self, payload: &Value, now: Instant) -> bool {
        self.last_payload.as_ref() != Some(payload)
            || self
                .last_emitted_at
                .is_none_or(|last| now.saturating_duration_since(last) >= HUD_KEEPALIVE_INTERVAL)
    }

    fn record_emitted(&mut self, payload: Value, now: Instant) {
        self.last_payload = Some(payload);
        self.last_emitted_at = Some(now);
    }
}

#[derive(Default)]
pub(crate) struct Controller {
    stop: Option<Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl Controller {
    pub(crate) fn start(
        app: tauri::AppHandle<tauri::Wry>,
        runtime: Arc<Runtime>,
        window: Option<tauri::WebviewWindow<tauri::Wry>>,
    ) -> Self {
        let Some(window) = window else {
            return Self::default();
        };
        match Self::spawn("hud-state", move |stop| {
            run_state_thread(app, runtime, window, stop)
        }) {
            Ok(controller) => controller,
            Err(_) => {
                crate::log_warn!("hud", "thread_unavailable", "HUD state thread unavailable");
                Self::default()
            }
        }
    }

    fn spawn(
        name: &str,
        worker: impl FnOnce(Receiver<()>) + Send + 'static,
    ) -> std::io::Result<Self> {
        let (stop_tx, stop_rx) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || worker(stop_rx))?;
        Ok(Self {
            stop: Some(stop_tx),
            join: Some(join),
        })
    }

    pub(crate) fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_state_thread(
    app: tauri::AppHandle<tauri::Wry>,
    runtime: Arc<Runtime>,
    window: tauri::WebviewWindow<tauri::Wry>,
    stop: Receiver<()>,
) {
    let mut emission = EmissionTracker::default();
    loop {
        let snapshot = crate::commands::own_snapshot_value(runtime.as_ref());
        let payload = build_hud_state(&snapshot);
        let visible = payload["visible"].as_bool().unwrap_or(false);
        let now = Instant::now();

        if !visible {
            hide_window(&window);
        }

        let mut state_delivered = true;
        if emission.should_emit(&payload, now) {
            match app.emit_to("hud", HUD_STATE_EVENT, payload.clone()) {
                Ok(()) => emission.record_emitted(payload.clone(), now),
                Err(_) => {
                    state_delivered = false;
                    hide_window(&window);
                    crate::log_warn!("hud", "state_emit_failed", "HUD state event unavailable");
                }
            }
        }

        if visible && state_delivered {
            show_window(&window);
        }

        match stop.recv_timeout(HUD_POLL_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn show_window(window: &tauri::WebviewWindow<tauri::Wry>) {
    match window.is_visible() {
        Ok(true) => return,
        Ok(false) => {}
        Err(_) => {
            hide_window(window);
            crate::log_warn!(
                "hud",
                "visibility_unavailable",
                "HUD visibility unavailable"
            );
            return;
        }
    }

    if window.set_ignore_cursor_events(true).is_err() {
        hide_window(window);
        crate::log_warn!("hud", "click_through_failed", "HUD remains hidden");
        return;
    }
    if !crate::place_hud_window(window) {
        hide_window(window);
        return;
    }
    if window.show().is_err() {
        hide_window(window);
        crate::log_warn!("hud", "show_failed", "HUD remains hidden");
        return;
    }
    if window.set_ignore_cursor_events(true).is_err() {
        hide_window(window);
        crate::log_warn!(
            "hud",
            "click_through_failed",
            "HUD hidden after click-through failure"
        );
    }
}

fn hide_window(window: &tauri::WebviewWindow<tauri::Wry>) {
    match window.is_visible() {
        Ok(false) => {}
        Ok(true) | Err(_) => {
            if window.hide().is_err() {
                crate::log_warn!("hud", "hide_failed", "HUD hide failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_hud_state, effective_hud_layouts, hud_capability_permissions, hud_window_spec,
        module_pixel_center, place_hud_window, Controller, EmissionTracker, HudModule,
        HudModuleLayout, DEFAULT_HUD_LAYOUTS,
    };
    use crate::overlay::{MonitorRect, Rect};
    use kohbor_gsi::admission::{
        Admitted, Field, HeroFields, OwnCounters, OwnLoadout, Rejection, Team, UnsupportedReason,
    };
    use kohbor_gsi::snapshot::SourceState;
    use kohbor_gsi::{build_snapshot, build_snapshot_with_loadout, Outcome, Ownership, Phase};
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn layout(x_percent: f64, y_percent: f64, scale: f64) -> HudModuleLayout {
        HudModuleLayout {
            x_percent,
            y_percent,
            scale,
            enabled: true,
        }
    }

    fn monitor(x: i32, y: i32, w: u32, h: u32) -> MonitorRect {
        MonitorRect { x, y, w, h }
    }

    fn field(value: Value, availability: &str) -> Value {
        json!({"value": value, "availability": availability})
    }

    fn source(age_ms: Option<u64>) -> SourceState {
        SourceState {
            generation: 1,
            sequence: age_ms.map(|_| 1),
            age_ms,
            session_epoch: age_ms.map(|_| 1),
            identity_revision: 1,
        }
    }

    fn admitted_snapshot(phase: Phase, ownership: Ownership, age_ms: u64) -> Value {
        let outcome = Outcome::Admitted(Box::new(Admitted {
            phase,
            ownership,
            team: Some(Team::Radiant),
            match_id: None,
            steamid64: None,
            counters: OwnCounters {
                gold: Field::Value(1234),
                kills: Field::Value(7),
                kill_streak: Field::Value(0),
                deaths: Field::Value(2),
                assists: Field::Value(11),
                last_hits: Field::Value(80),
                denies: Field::Value(5),
                gpm: Field::Value(456),
                xpm: Field::Value(789),
                combined_score: Field::Value(0),
            },
            hero: HeroFields {
                hero_id: Field::Value("npc_dota_hero_axe".to_owned()),
                level: Field::Value(12),
                hp: Field::Value(1200),
                max_hp: Field::Value(1500),
                mana_percent: Field::Value(60),
                alive: Field::Value(true),
                respawn_seconds: Field::Value(0),
            },
        }));
        build_snapshot_with_loadout(&outcome, &OwnLoadout::unknown(), source(Some(age_ms)))
    }

    fn unavailable_snapshot(outcome: Outcome) -> Value {
        build_snapshot(&outcome, source(None))
    }

    fn assert_hidden_unknown(state: &Value) {
        assert_eq!(state["visible"], false);
        for name in ["kills", "deaths", "assists", "gold", "gpm", "xpm"] {
            assert_eq!(
                state["fields"][name],
                json!({"value":null,"availability":"unknown"})
            );
        }
    }

    #[test]
    fn module_ids_match_renderer_contract() {
        assert_eq!(
            [
                HudModule::Gmeter.as_str(),
                HudModule::Kda.as_str(),
                HudModule::Gold.as_str(),
                HudModule::Gpm.as_str(),
                HudModule::Nw.as_str(),
                HudModule::Xpm.as_str(),
            ],
            ["gmeter", "kda", "gold", "gpm", "nw", "xpm"]
        );
    }

    #[test]
    fn default_layouts_match_owner_profile_exactly() {
        assert_eq!(
            DEFAULT_HUD_LAYOUTS,
            [
                (HudModule::Gmeter, layout(50.0, 5.5, 0.65)),
                (HudModule::Kda, layout(9.64, 2.2, 0.78)),
                (HudModule::Gold, layout(13.56, 2.2, 0.78)),
                (HudModule::Gpm, layout(17.23, 2.2, 0.78)),
                (HudModule::Nw, layout(21.02, 2.2, 0.78)),
                (HudModule::Xpm, layout(24.82, 2.2, 0.78)),
            ]
        );
        assert!(DEFAULT_HUD_LAYOUTS.iter().all(|(_, value)| value.enabled));
    }

    #[test]
    fn layout_clamps_low_x_percent() {
        assert_eq!(layout(-10.0, 20.0, 1.0).clamped().x_percent, 0.0);
    }

    #[test]
    fn layout_clamps_high_x_percent() {
        assert_eq!(layout(110.0, 20.0, 1.0).clamped().x_percent, 100.0);
    }

    #[test]
    fn layout_clamps_low_y_percent() {
        assert_eq!(layout(20.0, -1.0, 1.0).clamped().y_percent, 0.0);
    }

    #[test]
    fn layout_clamps_high_y_percent() {
        assert_eq!(layout(20.0, 101.0, 1.0).clamped().y_percent, 100.0);
    }

    #[test]
    fn layout_clamps_scale_to_minimum() {
        assert_eq!(layout(20.0, 30.0, 0.1).clamped().scale, 0.2);
    }

    #[test]
    fn layout_clamps_scale_to_maximum() {
        assert_eq!(layout(20.0, 30.0, 4.0).clamped().scale, 3.0);
    }

    #[test]
    fn non_finite_layout_values_fail_to_safe_bounds() {
        let clamped = layout(f64::NAN, f64::INFINITY, f64::NAN).clamped();
        assert_eq!(
            (clamped.x_percent, clamped.y_percent, clamped.scale),
            (0.0, 100.0, 0.2)
        );
    }

    #[test]
    fn window_spans_1920_monitor_with_rounded_nine_percent_height() {
        assert_eq!(
            place_hud_window(&monitor(0, 0, 1920, 1080)),
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 97
            }
        );
    }

    #[test]
    fn window_spans_2560_monitor_with_rounded_nine_percent_height() {
        assert_eq!(
            place_hud_window(&monitor(0, 0, 2560, 1440)),
            Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 130
            }
        );
    }

    #[test]
    fn window_keeps_monitor_origin_offset() {
        assert_eq!(
            place_hud_window(&monitor(1920, -120, 1920, 1080)),
            Rect {
                x: 1920,
                y: -120,
                w: 1920,
                h: 97
            }
        );
    }

    #[test]
    fn zero_height_monitor_still_gets_one_pixel_window() {
        assert_eq!(
            place_hud_window(&monitor(5, 7, 100, 0)),
            Rect {
                x: 5,
                y: 7,
                w: 100,
                h: 1
            }
        );
    }

    #[test]
    fn gmeter_pixel_center_is_1080p_profile_position() {
        assert_eq!(
            module_pixel_center(&monitor(0, 0, 1920, 1080), &DEFAULT_HUD_LAYOUTS[0].1),
            (960.0, 59.4)
        );
    }

    #[test]
    fn first_stat_chip_pixel_center_is_relative_to_window_origin() {
        let (x, y) = module_pixel_center(&monitor(1920, 0, 1920, 1080), &DEFAULT_HUD_LAYOUTS[1].1);
        assert!(
            (x - 185.088).abs() < 1e-6 && (y - 23.76).abs() < 1e-6,
            "{x} {y}"
        );
    }

    #[test]
    fn module_pixel_center_clamps_percentages() {
        assert_eq!(
            module_pixel_center(&monitor(0, 0, 1000, 500), &layout(150.0, -2.0, 0.78)),
            (1000.0, 0.0)
        );
    }

    #[test]
    fn effective_layouts_are_compiled_defaults_until_lab_hook_exists() {
        assert_eq!(effective_hud_layouts(), DEFAULT_HUD_LAYOUTS);
    }

    #[test]
    fn hud_spec_uses_dedicated_label_and_url() {
        let spec = hud_window_spec();
        assert_eq!((spec.label, spec.url), ("hud", "index.html?view=hud"));
    }

    #[test]
    fn hud_window_spec_has_required_passive_flags() {
        let spec = hud_window_spec();
        assert!(spec.transparent && spec.always_on_top && spec.skip_taskbar);
        assert!(!spec.decorations && !spec.focusable && !spec.focused);
        assert!(!spec.shadow && !spec.resizable && !spec.visible);
        assert!(spec.ignore_cursor_events);
    }

    #[test]
    fn hud_spec_flags_match_overlay_spec() {
        let hud = hud_window_spec();
        let overlay = crate::overlay::overlay_window_spec();
        assert_eq!(
            (
                hud.transparent,
                hud.decorations,
                hud.always_on_top,
                hud.skip_taskbar,
                hud.focusable,
                hud.focused,
                hud.shadow,
                hud.resizable,
                hud.visible,
                hud.ignore_cursor_events,
            ),
            (
                overlay.transparent,
                overlay.decorations,
                overlay.always_on_top,
                overlay.skip_taskbar,
                overlay.focusable,
                overlay.focused,
                overlay.shadow,
                overlay.resizable,
                overlay.visible,
                overlay.ignore_cursor_events,
            )
        );
    }

    #[test]
    fn hud_capability_is_listen_only_and_targets_hud() {
        assert_eq!(hud_capability_permissions(), &["core:event:default"]);
        let capability: Value =
            serde_json::from_str(include_str!("../capabilities/hud.json")).unwrap();
        assert_eq!(capability["windows"], json!(["hud"]));
        assert_eq!(
            capability["permissions"],
            json!(hud_capability_permissions())
        );
    }

    #[test]
    fn fresh_verified_snapshot_exposes_six_own_stats() {
        let state = build_hud_state(&admitted_snapshot(Phase::Live, Ownership::Verified, 0));
        assert_eq!(state["visible"], true);
        assert_eq!(state["fields"]["kills"], field(json!(7), "fresh"));
        assert_eq!(state["fields"]["deaths"], field(json!(2), "fresh"));
        assert_eq!(state["fields"]["assists"], field(json!(11), "fresh"));
        assert_eq!(state["fields"]["gold"], field(json!(1234), "fresh"));
        assert_eq!(state["fields"]["gpm"], field(json!(456), "fresh"));
        assert_eq!(state["fields"]["xpm"], field(json!(789), "fresh"));
    }

    #[test]
    fn stale_snapshot_keeps_observed_values_and_marks_them_stale() {
        let state = build_hud_state(&admitted_snapshot(Phase::Live, Ownership::Verified, 2001));
        assert_eq!(state["visible"], true);
        assert_eq!(state["fields"]["kills"], field(json!(7), "stale"));
        assert_eq!(state["fields"]["gold"], field(json!(1234), "stale"));
    }

    #[test]
    fn unknown_snapshot_hides_and_scrubs_all_fields() {
        assert_hidden_unknown(&build_hud_state(&unavailable_snapshot(Outcome::NotPlaying)));
    }

    #[test]
    fn invalid_snapshot_hides_and_scrubs_all_fields() {
        assert_hidden_unknown(&build_hud_state(&unavailable_snapshot(Outcome::Rejected(
            Rejection::SourceInvalid,
        ))));
    }

    #[test]
    fn unsupported_snapshot_hides_and_scrubs_all_fields() {
        assert_hidden_unknown(&build_hud_state(&unavailable_snapshot(
            Outcome::Unsupported(UnsupportedReason::Spectating),
        )));
    }

    #[test]
    fn spectator_snapshot_does_not_expose_own_stats() {
        assert_hidden_unknown(&build_hud_state(&unavailable_snapshot(
            Outcome::Unsupported(UnsupportedReason::Spectating),
        )));
    }

    #[test]
    fn absent_verified_owner_does_not_expose_own_stats() {
        assert_hidden_unknown(&build_hud_state(&admitted_snapshot(
            Phase::Live,
            Ownership::Unverified,
            0,
        )));
    }

    #[test]
    fn non_pregame_live_phase_snapshot_does_not_expose_own_stats() {
        for phase in [Phase::Draft, Phase::PostGame] {
            assert_hidden_unknown(&build_hud_state(&admitted_snapshot(
                phase,
                Ownership::Verified,
                0,
            )));
        }
    }

    #[test]
    fn verified_pregame_snapshot_is_visible() {
        assert_eq!(
            build_hud_state(&admitted_snapshot(Phase::Pregame, Ownership::Verified, 0))["visible"],
            true
        );
    }

    #[test]
    fn mismatched_owner_snapshot_hides_and_scrubs_all_fields() {
        assert_hidden_unknown(&build_hud_state(&admitted_snapshot(
            Phase::Live,
            Ownership::Mismatch,
            0,
        )));
    }

    #[test]
    fn fresh_availability_without_own_fields_fails_closed() {
        let state = build_hud_state(&json!({"availability":"fresh", "own":null}));
        assert_hidden_unknown(&state);
    }

    #[test]
    fn malformed_counter_field_does_not_turn_into_zero() {
        let mut source = admitted_snapshot(Phase::Live, Ownership::Verified, 0);
        source["own"]["fields"]["kills"] = json!({"value":"bad", "availability":"fresh"});
        let state = build_hud_state(&source);
        assert_eq!(
            state["fields"]["kills"],
            json!({"value":null,"availability":"unknown"})
        );
    }

    #[test]
    fn unknown_counter_availability_never_keeps_a_numeric_value() {
        let mut source = admitted_snapshot(Phase::Live, Ownership::Verified, 0);
        source["own"]["fields"]["kills"] = json!({"value":7,"availability":"unknown"});
        let state = build_hud_state(&source);
        assert_eq!(
            state["fields"]["kills"],
            json!({"value":null,"availability":"unknown"})
        );
    }

    #[test]
    fn supported_field_availability_is_copied_without_top_level_substitution() {
        let mut source = admitted_snapshot(Phase::Live, Ownership::Verified, 0);
        source["own"]["fields"]["kills"] = json!({"value":null,"availability":"invalid"});
        let state = build_hud_state(&source);
        assert_eq!(state["visible"], true);
        assert_eq!(
            state["fields"]["kills"],
            json!({"value":null,"availability":"invalid"})
        );
    }

    #[test]
    fn payload_has_exact_schema_and_unsupported_net_worth() {
        let state = build_hud_state(&admitted_snapshot(Phase::Live, Ownership::Verified, 0));
        assert_eq!(
            state,
            json!({
                "schema": 1,
                "visible": true,
                "fields": {
                    "kills": {"value":7,"availability":"fresh"},
                    "deaths": {"value":2,"availability":"fresh"},
                    "assists": {"value":11,"availability":"fresh"},
                    "gold": {"value":1234,"availability":"fresh"},
                    "gpm": {"value":456,"availability":"fresh"},
                    "xpm": {"value":789,"availability":"fresh"}
                },
                "net_worth": {"value":null,"availability":"unsupported"}
            })
        );
    }

    #[test]
    fn emission_tracker_sends_initial_state() {
        let tracker = EmissionTracker::default();
        assert!(tracker.should_emit(&json!({"visible":false}), Instant::now()));
    }

    #[test]
    fn emission_tracker_skips_unchanged_state_before_keepalive() {
        let start = Instant::now();
        let payload = json!({"visible":false});
        let mut tracker = EmissionTracker::default();
        assert!(tracker.should_emit(&payload, start));
        tracker.record_emitted(payload.clone(), start);
        assert!(!tracker.should_emit(&payload, start + Duration::from_millis(1999)));
    }

    #[test]
    fn emission_tracker_sends_changed_state_immediately() {
        let start = Instant::now();
        let mut tracker = EmissionTracker::default();
        tracker.record_emitted(json!({"visible":false}), start);
        assert!(tracker.should_emit(&json!({"visible":true}), start + Duration::from_millis(1)));
    }

    #[test]
    fn emission_tracker_sends_keepalive_at_two_seconds() {
        let start = Instant::now();
        let payload = json!({"visible":false});
        let mut tracker = EmissionTracker::default();
        tracker.record_emitted(payload.clone(), start);
        assert!(tracker.should_emit(&payload, start + Duration::from_secs(2)));
    }

    #[test]
    fn emission_tracker_retries_payload_until_recorded_successfully() {
        let start = Instant::now();
        let payload = json!({"visible":true});
        let tracker = EmissionTracker::default();
        assert!(tracker.should_emit(&payload, start));
        assert!(tracker.should_emit(&payload, start + Duration::from_millis(500)));
    }

    #[test]
    fn emission_tracker_keeps_visibility_changes_even_when_values_are_equal() {
        let start = Instant::now();
        let mut tracker = EmissionTracker::default();
        tracker.record_emitted(json!({"visible":false,"value":1}), start);
        assert!(tracker.should_emit(
            &json!({"visible":true,"value":1}),
            start + Duration::from_millis(500)
        ));
    }

    #[test]
    fn controller_stop_joins_its_worker() {
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stopped = stopped.clone();
        let mut controller = Controller::spawn("hud-stop-test", move |stop| {
            let _ = stop.recv();
            worker_stopped.store(true, Ordering::SeqCst);
        })
        .unwrap();

        controller.stop();
        assert!(stopped.load(Ordering::SeqCst));
        assert!(controller.join.is_none());
    }

    #[test]
    fn controller_stop_is_idempotent() {
        let mut controller = Controller::spawn("hud-stop-twice-test", |stop| {
            let _ = stop.recv();
        })
        .unwrap();
        controller.stop();
        controller.stop();
        assert!(controller.join.is_none());
    }
}
