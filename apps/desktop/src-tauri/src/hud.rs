use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, OnceLock};
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

#[cfg_attr(not(test), allow(dead_code))]
pub fn effective_hud_layouts() -> [(HudModule, HudModuleLayout); 6] {
    #[cfg(any(feature = "lab", debug_assertions))]
    {
        crate::lab::hud_layouts()
    }
    #[cfg(not(any(feature = "lab", debug_assertions)))]
    {
        DEFAULT_HUD_LAYOUTS
    }
}

pub fn strip_height_percent(layouts: &[(HudModule, HudModuleLayout); 6]) -> f64 {
    layouts
        .iter()
        .filter(|(_, layout)| layout.enabled && layout.y_percent.is_finite())
        .map(|(_, layout)| layout.y_percent + 3.5)
        .fold(9.0_f64, f64::max)
        .min(100.0)
}

pub fn place_hud_window(
    monitor: &crate::overlay::MonitorRect,
    layouts: &[(HudModule, HudModuleLayout); 6],
) -> crate::overlay::Rect {
    crate::overlay::Rect {
        x: monitor.x,
        y: monitor.y,
        w: monitor.w,
        h: (f64::from(monitor.h) * strip_height_percent(layouts) / 100.0)
            .round()
            .max(1.0) as u32,
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
#[cfg(any(feature = "lab", debug_assertions))]
const HUD_LAYOUT_EVENT: &str = "hud://layout";

#[cfg(any(feature = "lab", debug_assertions))]
#[derive(Default)]
struct HudLayoutEmission {
    last_payload: Option<Value>,
    last_revision: Option<u64>,
    last_emitted_at: Option<Instant>,
}

#[cfg(any(feature = "lab", debug_assertions))]
impl HudLayoutEmission {
    fn should_emit(&self, payload: &Value, revision: u64, now: Instant) -> bool {
        self.last_payload.as_ref() != Some(payload)
            || self.last_revision != Some(revision)
            || self
                .last_emitted_at
                .is_none_or(|last| now.saturating_duration_since(last) >= HUD_KEEPALIVE_INTERVAL)
    }

    fn record_emitted(&mut self, payload: Value, revision: u64, now: Instant) {
        self.last_payload = Some(payload);
        self.last_revision = Some(revision);
        self.last_emitted_at = Some(now);
    }
}

#[cfg(any(feature = "lab", debug_assertions))]
fn hud_layout_payload(layouts: &[(HudModule, HudModuleLayout); 6]) -> Value {
    let modules: Vec<Value> = layouts
        .iter()
        .map(|(module, layout)| {
            serde_json::json!({
                "id": module.as_str(),
                "x_percent": layout.x_percent,
                "y_percent": layout.y_percent,
                "scale": layout.scale,
                "enabled": layout.enabled,
            })
        })
        .collect();
    serde_json::json!({
        "schema": 1,
        "strip_height_percent": strip_height_percent(layouts),
        "modules": modules,
    })
}

#[cfg(any(feature = "lab", debug_assertions))]
fn synthetic_preview_state() -> Value {
    // Preview data is synthetic and must never be confused with an observed match snapshot.
    serde_json::json!({
        "schema": 1,
        "visible": true,
        "fields": {
            "kills": {"value": 5, "availability": "fresh"},
            "deaths": {"value": 2, "availability": "fresh"},
            "assists": {"value": 8, "availability": "fresh"},
            "gold": {"value": 1234, "availability": "fresh"},
            "gpm": {"value": 456, "availability": "fresh"},
            "xpm": {"value": 789, "availability": "fresh"},
        },
        "net_worth": {"value": 21450, "availability": "fresh", "approximate": false},
        // Marker for the renderer (extra key only in this Lab preview payload): the HUD shows a
        // visible SYNTHETIC badge so the numbers can never be read as an observed match state.
        "synthetic": true,
    })
}

/// True while the REAL (non-preview) state says the HUD is visible, i.e. a verified owner is in a
/// pregame/live match. The Lab preview refuses to run then, so synthetic numbers never replace
/// real match data.
#[cfg(any(feature = "lab", debug_assertions))]
static REAL_HUD_VISIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(any(feature = "lab", debug_assertions))]
pub fn real_hud_visible() -> bool {
    REAL_HUD_VISIBLE.load(std::sync::atomic::Ordering::SeqCst)
}

#[cfg(any(feature = "lab", debug_assertions))]
fn preview_state(snapshot: &Value, preview_active: bool) -> Value {
    REAL_HUD_VISIBLE.store(
        build_hud_state(snapshot)["visible"]
            .as_bool()
            .unwrap_or(false),
        std::sync::atomic::Ordering::SeqCst,
    );
    if preview_active {
        synthetic_preview_state()
    } else {
        build_hud_state(snapshot)
    }
}

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
        "net_worth": if visible { net_worth_field(snapshot) } else { unknown_net_worth() }
    })
}

/// Item name -> shop cost in gold (294 entries). Third-party data, see
/// docs/governance/THIRD-PARTY-DATA.md. Parsed strictly once; a bad table makes every net worth unknown.
const ITEM_PRICES_JSON: &str = include_str!("../data/item-prices.json");

fn item_prices() -> Option<&'static HashMap<String, u32>> {
    static PRICES: OnceLock<Option<HashMap<String, u32>>> = OnceLock::new();
    PRICES
        .get_or_init(|| {
            let value: Value = serde_json::from_str(ITEM_PRICES_JSON).ok()?;
            let object = value.as_object()?;
            let mut map = HashMap::with_capacity(object.len());
            for (name, cost) in object {
                map.insert(name.clone(), u32::try_from(cost.as_u64()?).ok()?);
            }
            (!map.is_empty()).then_some(map)
        })
        .as_ref()
}

/// Cost of an internal item name, bare or `item_`-prefixed. `None` = not in the table (unknown).
pub fn item_cost(name: &str) -> Option<u32> {
    let bare = name.strip_prefix("item_").unwrap_or(name);
    item_prices()?.get(bare).copied()
}

fn unknown_net_worth() -> Value {
    serde_json::json!({"value": null, "availability": "unknown", "approximate": false})
}

/// Net worth is DERIVED (the game does not send it for the local player): gold + the shop cost of
/// every named item in the own items block (inventory, backpack, teleport, neutral; an observed empty
/// slot has a null name and costs nothing). Unknown gold, or an items block that is not a complete
/// fresh/stale observation, gives unknown (never a partial sum shown as complete). An item name that
/// is missing from the price table is counted as 0 and the result is flagged `approximate`.
pub fn net_worth_field(snapshot: &Value) -> Value {
    let live = |name: &str| -> Option<(&Value, bool)> {
        let field = snapshot.pointer(&format!("/own/fields/{name}"))?;
        match field.get("availability").and_then(Value::as_str)? {
            "fresh" => Some((field.get("value")?, false)),
            "stale" => Some((field.get("value")?, true)),
            _ => None,
        }
    };
    let (Some((gold, gold_stale)), Some((items, items_stale))) = (live("gold"), live("items"))
    else {
        return unknown_net_worth();
    };
    let (Some(gold), Some(items)) = (gold.as_u64(), items.as_array()) else {
        return unknown_net_worth();
    };
    if item_prices().is_none() {
        return unknown_net_worth();
    }
    let mut total = gold;
    let mut approximate = false;
    for item in items {
        match item.get("name") {
            Some(Value::Null) => {}
            Some(Value::String(name)) => match item_cost(name) {
                Some(cost) => total = total.saturating_add(u64::from(cost)),
                None => approximate = true,
            },
            _ => return unknown_net_worth(),
        }
    }
    if total > 10_000_000 {
        return unknown_net_worth();
    }
    serde_json::json!({
        "value": total,
        "availability": if gold_stale || items_stale { "stale" } else { "fresh" },
        "approximate": approximate,
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
    #[cfg(any(feature = "lab", debug_assertions))]
    let mut layout_emission = HudLayoutEmission::default();
    #[cfg(any(feature = "lab", debug_assertions))]
    let mut last_layout: Option<(u64, Value)> = None;
    loop {
        let layouts = effective_hud_layouts();
        let snapshot = crate::commands::own_snapshot_value(runtime.as_ref());
        #[cfg(any(feature = "lab", debug_assertions))]
        let payload = preview_state(&snapshot, crate::lab::hud_preview_active());
        #[cfg(not(any(feature = "lab", debug_assertions)))]
        let payload = build_hud_state(&snapshot);
        let visible = payload["visible"].as_bool().unwrap_or(false);
        let now = Instant::now();

        #[cfg(any(feature = "lab", debug_assertions))]
        {
            let revision = crate::lab::hud_layout_revision();
            let layout_payload = hud_layout_payload(&layouts);
            if layout_emission.should_emit(&layout_payload, revision, now) {
                match app.emit_to("hud", HUD_LAYOUT_EVENT, layout_payload.clone()) {
                    Ok(()) => layout_emission.record_emitted(layout_payload.clone(), revision, now),
                    Err(_) => crate::log_warn!(
                        "hud",
                        "layout_emit_failed",
                        "HUD layout event unavailable"
                    ),
                }
            }

            let layout_changed = last_layout
                .as_ref()
                .is_some_and(|(last_revision, previous)| {
                    *last_revision != revision || previous != &layout_payload
                });
            if visible && layout_changed && !crate::place_hud_window(&window, &layouts) {
                hide_window(&window);
            }
            last_layout = Some((revision, layout_payload));
        }

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
            show_window(&window, &layouts);
        }

        match stop.recv_timeout(HUD_POLL_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn show_window(
    window: &tauri::WebviewWindow<tauri::Wry>,
    layouts: &[(HudModule, HudModuleLayout); 6],
) {
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
    if !crate::place_hud_window(window, layouts) {
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
        build_hud_state, effective_hud_layouts, hud_capability_permissions, hud_layout_payload,
        hud_window_spec, item_cost, item_prices, module_pixel_center, net_worth_field,
        place_hud_window, preview_state, strip_height_percent, synthetic_preview_state,
        unknown_net_worth, Controller, EmissionTracker, HudLayoutEmission, HudModule,
        HudModuleLayout, DEFAULT_HUD_LAYOUTS,
    };
    use crate::overlay::{MonitorRect, Rect};
    use kohbor_gsi::admission::{
        Admitted, Field, HeroFields, OwnCounters, OwnLoadout, Rejection, Team, UnsupportedReason,
    };
    use kohbor_gsi::snapshot::SourceState;
    use kohbor_gsi::{build_snapshot, build_snapshot_with_loadout, Outcome, Ownership, Phase};
    use serde_json::{json, Value};
    use std::collections::HashMap;
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
    fn default_strip_height_is_nine_percent() {
        assert_eq!(strip_height_percent(&DEFAULT_HUD_LAYOUTS), 9.0);
    }

    #[test]
    fn strip_height_contains_module_at_twelve_percent_plus_margin() {
        let mut layouts = DEFAULT_HUD_LAYOUTS;
        layouts[2].1.y_percent = 12.0;
        assert_eq!(strip_height_percent(&layouts), 15.5);
    }

    #[test]
    fn strip_height_ignores_disabled_modules() {
        let mut layouts = DEFAULT_HUD_LAYOUTS;
        layouts[2].1.y_percent = 92.0;
        layouts[2].1.enabled = false;
        assert_eq!(strip_height_percent(&layouts), 9.0);
    }

    #[test]
    fn strip_height_caps_at_one_hundred_percent() {
        let mut layouts = DEFAULT_HUD_LAYOUTS;
        layouts[0].1.y_percent = 100.0;
        assert_eq!(strip_height_percent(&layouts), 100.0);
    }

    #[test]
    fn window_places_default_strip_at_1920_by_1080() {
        assert_eq!(
            place_hud_window(&monitor(0, 0, 1920, 1080), &DEFAULT_HUD_LAYOUTS),
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 97
            }
        );
    }

    #[test]
    fn window_places_default_strip_at_2560_by_1440() {
        assert_eq!(
            place_hud_window(&monitor(0, 0, 2560, 1440), &DEFAULT_HUD_LAYOUTS),
            Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 130
            }
        );
    }

    #[test]
    fn window_places_tall_strip_at_1920_by_1080() {
        let mut layouts = DEFAULT_HUD_LAYOUTS;
        layouts[4].1.y_percent = 40.0;
        assert_eq!(
            place_hud_window(&monitor(0, 0, 1920, 1080), &layouts),
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 470
            }
        );
    }

    #[test]
    fn window_places_tall_strip_at_2560_by_1440() {
        let mut layouts = DEFAULT_HUD_LAYOUTS;
        layouts[4].1.y_percent = 40.0;
        assert_eq!(
            place_hud_window(&monitor(0, 0, 2560, 1440), &layouts),
            Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 626
            }
        );
    }

    #[test]
    fn hud_layout_event_has_the_exact_schema_and_module_order() {
        let payload = hud_layout_payload(&DEFAULT_HUD_LAYOUTS);
        assert_eq!(
            payload,
            json!({
                "schema": 1,
                "strip_height_percent": 9.0,
                "modules": [
                    {"id":"gmeter","x_percent":50.0,"y_percent":5.5,"scale":0.65,"enabled":true},
                    {"id":"kda","x_percent":9.64,"y_percent":2.2,"scale":0.78,"enabled":true},
                    {"id":"gold","x_percent":13.56,"y_percent":2.2,"scale":0.78,"enabled":true},
                    {"id":"gpm","x_percent":17.23,"y_percent":2.2,"scale":0.78,"enabled":true},
                    {"id":"nw","x_percent":21.02,"y_percent":2.2,"scale":0.78,"enabled":true},
                    {"id":"xpm","x_percent":24.82,"y_percent":2.2,"scale":0.78,"enabled":true}
                ]
            })
        );
    }

    #[test]
    fn layout_event_tracker_sends_initial_payload() {
        let tracker = HudLayoutEmission::default();
        assert!(tracker.should_emit(&json!({"schema":1}), 0, Instant::now()));
    }

    #[test]
    fn layout_event_tracker_sends_when_revision_changes_even_if_payload_matches() {
        let start = Instant::now();
        let payload = json!({"schema":1});
        let mut tracker = HudLayoutEmission::default();
        assert!(tracker.should_emit(&payload, 1, start));
        tracker.record_emitted(payload.clone(), 1, start);
        assert!(!tracker.should_emit(&payload, 1, start + Duration::from_millis(500)));
        assert!(tracker.should_emit(&payload, 2, start + Duration::from_millis(501)));
    }

    #[test]
    fn layout_event_tracker_sends_keepalive_after_two_seconds() {
        let start = Instant::now();
        let payload = json!({"schema":1});
        let mut tracker = HudLayoutEmission::default();
        tracker.record_emitted(payload.clone(), 0, start);
        assert!(tracker.should_emit(&payload, 0, start + Duration::from_secs(2)));
    }

    #[test]
    fn no_active_preview_preserves_the_real_snapshot_state() {
        let snapshot = admitted_snapshot(Phase::Live, Ownership::Verified, 0);
        assert_eq!(preview_state(&snapshot, false), build_hud_state(&snapshot));
    }

    #[test]
    fn active_preview_uses_only_the_fixed_synthetic_values() {
        assert_eq!(
            synthetic_preview_state(),
            json!({
                "schema":1,
                "visible":true,
                "fields": {
                    "kills":{"value":5,"availability":"fresh"},
                    "deaths":{"value":2,"availability":"fresh"},
                    "assists":{"value":8,"availability":"fresh"},
                    "gold":{"value":1234,"availability":"fresh"},
                    "gpm":{"value":456,"availability":"fresh"},
                    "xpm":{"value":789,"availability":"fresh"}
                },
                "net_worth":{"value":21450,"availability":"fresh","approximate":false},
                "synthetic": true
            })
        );
    }

    #[test]
    fn real_data_never_carries_the_synthetic_marker() {
        let live = admitted_snapshot(Phase::Live, Ownership::Verified, 0);
        let real = build_hud_state(&live);
        assert!(real.get("synthetic").is_none());
        assert_eq!(
            synthetic_preview_state()["synthetic"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn active_preview_replaces_the_snapshot_state() {
        let snapshot = admitted_snapshot(Phase::Live, Ownership::Verified, 0);
        assert_eq!(preview_state(&snapshot, true), synthetic_preview_state());
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
            place_hud_window(&monitor(0, 0, 1920, 1080), &DEFAULT_HUD_LAYOUTS),
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
            place_hud_window(&monitor(0, 0, 2560, 1440), &DEFAULT_HUD_LAYOUTS),
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
            place_hud_window(&monitor(1920, -120, 1920, 1080), &DEFAULT_HUD_LAYOUTS),
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
            place_hud_window(&monitor(5, 7, 100, 0), &DEFAULT_HUD_LAYOUTS),
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
    fn effective_layouts_use_compiled_defaults_when_lab_is_disabled() {
        #[cfg(any(feature = "lab", debug_assertions))]
        if crate::lab::dev::enabled() {
            return;
        }
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
    fn payload_has_exact_schema_and_unknown_net_worth_without_items() {
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
                "net_worth": {"value":null,"availability":"unknown","approximate":false}
            })
        );
    }

    fn nw_snapshot(gold: Value, items: Value) -> Value {
        json!({"own": {"fields": {"gold": gold, "items": items}}})
    }

    fn items_value(names: &[Option<&str>]) -> Value {
        json!({"value": names.iter().enumerate().map(|(i, n)| json!({"slot": format!("inventory{i}"), "name": n})).collect::<Vec<_>>(), "availability": "fresh"})
    }

    #[test]
    fn price_table_parses_and_resolves_bare_and_prefixed_names() {
        assert_eq!(item_prices().map(HashMap::len), Some(294));
        assert_eq!(item_cost("blink"), Some(2250));
        assert_eq!(item_cost("item_blink"), Some(2250));
        assert_eq!(item_cost("not_an_item"), None);
    }

    #[test]
    fn net_worth_is_gold_plus_item_costs_and_exact_when_all_names_are_known() {
        let snapshot = nw_snapshot(
            json!({"value": 1000, "availability": "fresh"}),
            items_value(&[Some("blink"), Some("item_branches"), None]),
        );
        assert_eq!(
            net_worth_field(&snapshot),
            json!({"value": 1000 + 2250 + 55, "availability": "fresh", "approximate": false})
        );
    }

    #[test]
    fn unknown_item_name_is_counted_as_zero_and_flagged_approximate() {
        let snapshot = nw_snapshot(
            json!({"value": 500, "availability": "fresh"}),
            items_value(&[Some("blink"), Some("brand_new_item")]),
        );
        assert_eq!(
            net_worth_field(&snapshot),
            json!({"value": 2750, "availability": "fresh", "approximate": true})
        );
    }

    #[test]
    fn empty_slots_cost_nothing_and_stale_inputs_make_a_stale_result() {
        let snapshot = nw_snapshot(
            json!({"value": 300, "availability": "stale"}),
            items_value(&[None, None]),
        );
        assert_eq!(
            net_worth_field(&snapshot),
            json!({"value": 300, "availability": "stale", "approximate": false})
        );
    }

    #[test]
    fn missing_gold_or_unavailable_items_are_unknown_never_a_partial_sum() {
        let unknown = unknown_net_worth();
        assert_eq!(net_worth_field(&json!({})), unknown);
        assert_eq!(
            net_worth_field(&nw_snapshot(
                json!({"value": null, "availability": "unknown"}),
                items_value(&[Some("blink")])
            )),
            unknown
        );
        for availability in ["unknown", "invalid", "unsupported"] {
            assert_eq!(
                net_worth_field(&nw_snapshot(
                    json!({"value": 900, "availability": "fresh"}),
                    json!({"value": null, "availability": availability})
                )),
                unknown,
                "{availability}"
            );
        }
        // An item entry without a name (unknown, not observed empty) invalidates the whole sum.
        assert_eq!(
            net_worth_field(&nw_snapshot(
                json!({"value": 900, "availability": "fresh"}),
                json!({"value": [{"slot": "inventory0"}], "availability": "fresh"})
            )),
            unknown
        );
    }

    #[test]
    fn hidden_state_has_an_unknown_net_worth() {
        let state = build_hud_state(&json!({"availability": "unknown"}));
        assert_eq!(state["net_worth"], unknown_net_worth());
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
