use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

const FILE_NAME: &str = "overlay-layout.json";
const CARD_BASE_WIDTH_PX: f64 = 232.0;
const CARD_BASE_HEIGHT_PX: f64 = 32.0;
const CARD_DEFAULT_SCALE: f64 = 0.75;
/// Measured base height of the RISK (danger meter) pill before its module scale, in px at 1920 wide.
const GMETER_BASE_HEIGHT_PX: f64 = 30.4;
const CARD_GAP_PX: f64 = 4.0;
/// Owner decision 2026-11-22 (DEC-EVENT-RUNE-SPAWN revision 5): only the objective card may enter
/// the top of the protected announcement region, by at most this many px at 1080 (scaled with height).
pub(crate) const OBJECTIVE_CARD_ANNOUNCEMENT_OVERLAP_PX_AT_1080: f64 = 24.0;
const GMETER_DEFAULT: crate::hud::HudModuleLayout = crate::hud::DEFAULT_HUD_LAYOUTS[0].1;
/// Default: centred under the RISK pill with a 4 px gap (reference 1920x1080).
pub(crate) const DEFAULT_LAYOUT: WidgetLayout = WidgetLayout {
    x_percent: GMETER_DEFAULT.x_percent,
    y_percent: GMETER_DEFAULT.y_percent
        + (GMETER_BASE_HEIGHT_PX * GMETER_DEFAULT.scale / 2.0
            + CARD_GAP_PX
            + CARD_BASE_HEIGHT_PX * CARD_DEFAULT_SCALE / 2.0)
            / 1080.0
            * 100.0,
    scale: CARD_DEFAULT_SCALE,
};
/// The revision 4 default; a saved copy of it is read as the current default.
const LEGACY_DEFAULT_LAYOUT: WidgetLayout = WidgetLayout {
    x_percent: 62.5,
    y_percent: 6.5,
    scale: 1.0,
};
const SAFE_EDGE_PX: f64 = 8.0;
const SCALE_BOUNDS: (f64, f64) = (0.75, 2.0);
const DEFAULT_VIEWPORT: (u32, u32) = (1920, 1080);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WidgetLayout {
    pub(crate) x_percent: f64,
    pub(crate) y_percent: f64,
    pub(crate) scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutViewport {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Default for LayoutViewport {
    fn default() -> Self {
        Self {
            width: DEFAULT_VIEWPORT.0,
            height: DEFAULT_VIEWPORT.1,
        }
    }
}

impl LayoutViewport {
    pub(crate) fn from_window(window: &tauri::WebviewWindow<tauri::Wry>) -> Self {
        window
            .primary_monitor()
            .ok()
            .flatten()
            .map(|monitor| {
                let size = monitor.size();
                Self {
                    width: size.width.max(1),
                    height: size.height.max(1),
                }
            })
            .unwrap_or_default()
    }
}

/// Per-widget clamp rules. Every widget is fully blocked from the protected regions unless its
/// rules carry an owner-approved allowance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClampRules {
    pub(crate) announcement_overlap_px_at_1080: f64,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const FULLY_BLOCKED: ClampRules = ClampRules {
    announcement_overlap_px_at_1080: 0.0,
};

pub(crate) const OBJECTIVE_CARD_RULES: ClampRules = ClampRules {
    announcement_overlap_px_at_1080: OBJECTIVE_CARD_ANNOUNCEMENT_OVERLAP_PX_AT_1080,
};

#[derive(Clone, Copy)]
struct ClampBounds {
    half_width: f64,
    half_height: f64,
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

pub(crate) struct LayoutService {
    path: PathBuf,
    current: Mutex<WidgetLayout>,
    revision: AtomicU64,
}

impl LayoutService {
    pub(crate) fn open(data_dir: &Path) -> Self {
        let path = data_dir.join(FILE_NAME);
        let (layout, invalid) = match fs::read(&path) {
            Ok(bytes) => parse_layout(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (DEFAULT_LAYOUT, false),
            Err(_) => (DEFAULT_LAYOUT, true),
        };
        if invalid {
            crate::log_warn!(
                "layout",
                "layout_invalid",
                "invalid overlay layout; using the objective-card default"
            );
        }
        Self {
            path,
            current: Mutex::new(layout),
            revision: AtomicU64::new(0),
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    pub(crate) fn widget(&self, viewport: LayoutViewport) -> WidgetLayout {
        clamp_widget(*lock(&self.current), viewport)
    }

    pub(crate) fn get(&self, viewport: LayoutViewport) -> Value {
        document_value(self.widget(viewport))
    }

    pub(crate) fn patch(&self, request: &Value, viewport: LayoutViewport) -> Result<Value, String> {
        let requested = parse_patch(request)?;
        self.store_if_changed(clamp_widget(requested, viewport))?;
        Ok(self.get(viewport))
    }

    pub(crate) fn reset(&self, viewport: LayoutViewport) -> Result<Value, String> {
        self.store_if_changed(clamp_widget(DEFAULT_LAYOUT, viewport))?;
        Ok(self.get(viewport))
    }

    fn store_if_changed(&self, layout: WidgetLayout) -> Result<(), String> {
        let mut current = lock(&self.current);
        if *current == layout {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&document_value(layout))
            .map_err(|_| "LAYOUT_WRITE_FAILED".to_owned())?;
        crate::atomic::write(&self.path, &bytes).map_err(|_| "LAYOUT_WRITE_FAILED".to_owned())?;
        *current = layout;
        self.revision.fetch_add(1, Ordering::AcqRel);
        crate::log_info!(
            "layout",
            "layout_changed",
            "overlay widget layout changed",
            widget = "objective_card",
            x_percent = layout.x_percent,
            y_percent = layout.y_percent,
            scale = layout.scale
        );
        Ok(())
    }
}

fn document_value(layout: WidgetLayout) -> Value {
    json!({
        "schema_version": 1,
        "widgets": {
            "objective_card": {
                "x_percent": layout.x_percent,
                "y_percent": layout.y_percent,
                "scale": layout.scale,
            }
        }
    })
}

fn parse_layout(bytes: &[u8]) -> (WidgetLayout, bool) {
    let Ok(root) = serde_json::from_slice::<Value>(bytes) else {
        return (DEFAULT_LAYOUT, true);
    };
    let Some(root) = root.as_object() else {
        return (DEFAULT_LAYOUT, true);
    };
    if root.len() != 2
        || root.get("schema_version").and_then(Value::as_u64) != Some(1)
        || !root.contains_key("widgets")
    {
        return (DEFAULT_LAYOUT, true);
    }
    let Some(widgets) = root.get("widgets").and_then(Value::as_object) else {
        return (DEFAULT_LAYOUT, true);
    };
    let invalid = widgets.keys().any(|key| key != "objective_card");
    if invalid {
        return (DEFAULT_LAYOUT, true);
    }
    let Some(entry) = widgets.get("objective_card") else {
        return (DEFAULT_LAYOUT, true);
    };
    match parse_widget_entry(entry) {
        Some(layout) if layout == LEGACY_DEFAULT_LAYOUT => (DEFAULT_LAYOUT, false),
        Some(layout) => (clamp_widget(layout, LayoutViewport::default()), false),
        None => (DEFAULT_LAYOUT, true),
    }
}

fn parse_widget_entry(value: &Value) -> Option<WidgetLayout> {
    let entry = value.as_object()?;
    if entry.len() != 3 {
        return None;
    }
    Some(WidgetLayout {
        x_percent: finite_number(entry.get("x_percent")?)?,
        y_percent: finite_number(entry.get("y_percent")?)?,
        scale: finite_number(entry.get("scale")?)?,
    })
}

fn parse_patch(value: &Value) -> Result<WidgetLayout, String> {
    let invalid = || "LAYOUT_INVALID".to_owned();
    let request = value.as_object().ok_or_else(invalid)?;
    if request.len() != 4 || request.get("widget").and_then(Value::as_str) != Some("objective_card")
    {
        return Err(invalid());
    }
    Ok(WidgetLayout {
        x_percent: finite_number(request.get("x_percent").ok_or_else(invalid)?)
            .ok_or_else(invalid)?,
        y_percent: finite_number(request.get("y_percent").ok_or_else(invalid)?)
            .ok_or_else(invalid)?,
        scale: finite_number(request.get("scale").ok_or_else(invalid)?).ok_or_else(invalid)?,
    })
}

fn finite_number(value: &Value) -> Option<f64> {
    value.as_f64().filter(|value| value.is_finite())
}

fn clamp_widget(layout: WidgetLayout, viewport: LayoutViewport) -> WidgetLayout {
    clamp_widget_with(layout, viewport, OBJECTIVE_CARD_RULES)
}

/// The protected regions as percentages, with the rule's announcement allowance applied to the
/// announcement region's top edge (a fraction of the screen height, so it scales with height).
fn protected_regions(rules: ClampRules) -> [(f64, f64, f64, f64); 4] {
    crate::overlay::PROTECTED_REGIONS.map(|(name, left, top, right, bottom)| {
        let top = if name == "announcement" {
            top + rules.announcement_overlap_px_at_1080.max(0.0) / 1080.0
        } else {
            top
        };
        (left * 100.0, top * 100.0, right * 100.0, bottom * 100.0)
    })
}

fn clamp_widget_with(
    layout: WidgetLayout,
    viewport: LayoutViewport,
    rules: ClampRules,
) -> WidgetLayout {
    let width = f64::from(viewport.width.max(1));
    let height = f64::from(viewport.height.max(1));
    let scale = clamp_value(
        layout.scale,
        SCALE_BOUNDS.0,
        SCALE_BOUNDS.1,
        DEFAULT_LAYOUT.scale,
    );
    let bounds = clamp_bounds(scale, width, height);
    let start = WidgetLayout {
        x_percent: clamp_value(
            layout.x_percent,
            bounds.min_x,
            bounds.max_x,
            DEFAULT_LAYOUT.x_percent,
        ),
        y_percent: clamp_value(
            layout.y_percent,
            bounds.min_y,
            bounds.max_y,
            DEFAULT_LAYOUT.y_percent,
        ),
        scale,
    };

    let mut x_candidates = vec![bounds.min_x, bounds.max_x, start.x_percent];
    let mut y_candidates = vec![bounds.min_y, bounds.max_y, start.y_percent];
    for (left, top, right, bottom) in protected_regions(rules) {
        x_candidates.push(clamp_value(
            left - bounds.half_width,
            bounds.min_x,
            bounds.max_x,
            start.x_percent,
        ));
        x_candidates.push(clamp_value(
            right + bounds.half_width,
            bounds.min_x,
            bounds.max_x,
            start.x_percent,
        ));
        y_candidates.push(clamp_value(
            top - bounds.half_height,
            bounds.min_y,
            bounds.max_y,
            start.y_percent,
        ));
        y_candidates.push(clamp_value(
            bottom + bounds.half_height,
            bounds.min_y,
            bounds.max_y,
            start.y_percent,
        ));
    }

    let mut closest = None;
    let mut closest_distance = f64::INFINITY;
    for x in x_candidates {
        for y in &y_candidates {
            let candidate = WidgetLayout {
                x_percent: x,
                y_percent: *y,
                scale,
            };
            if collides(candidate, bounds, rules) {
                continue;
            }
            let dx = (x - start.x_percent) * width / 100.0;
            let dy = (*y - start.y_percent) * height / 100.0;
            let distance = dx * dx + dy * dy;
            if distance < closest_distance {
                closest = Some(candidate);
                closest_distance = distance;
            }
        }
    }
    closest.unwrap_or(WidgetLayout {
        scale,
        ..DEFAULT_LAYOUT
    })
}

fn clamp_bounds(scale: f64, width: f64, height: f64) -> ClampBounds {
    let width_scale = width / 1920.0;
    let width_percent = CARD_BASE_WIDTH_PX * width_scale * scale / width * 100.0;
    let height_percent = CARD_BASE_HEIGHT_PX * width_scale * scale / height * 100.0;
    let margin_x = SAFE_EDGE_PX / width * 100.0;
    let margin_y = SAFE_EDGE_PX / height * 100.0;
    ClampBounds {
        half_width: width_percent / 2.0,
        half_height: height_percent / 2.0,
        min_x: margin_x + width_percent / 2.0,
        max_x: 100.0 - margin_x - width_percent / 2.0,
        min_y: margin_y + height_percent / 2.0,
        max_y: 100.0 - margin_y - height_percent / 2.0,
    }
}

fn collides(layout: WidgetLayout, bounds: ClampBounds, rules: ClampRules) -> bool {
    protected_regions(rules)
        .iter()
        .any(|(left, top, right, bottom)| {
            layout.x_percent - bounds.half_width < *right
                && layout.x_percent + bounds.half_width > *left
                && layout.y_percent - bounds.half_height < *bottom
                && layout.y_percent + bounds.half_height > *top
        })
}

fn clamp_value(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback.clamp(min, max)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{
        clamp_widget, clamp_widget_with, parse_layout, parse_patch, LayoutViewport, WidgetLayout,
        DEFAULT_LAYOUT, FULLY_BLOCKED, GMETER_BASE_HEIGHT_PX, GMETER_DEFAULT,
        LEGACY_DEFAULT_LAYOUT, OBJECTIVE_CARD_RULES, SCALE_BOUNDS,
    };
    use serde::Deserialize;
    use serde_json::{json, Value};

    #[derive(Deserialize)]
    struct ClampCase {
        name: String,
        viewport: LayoutViewportJson,
        request: WidgetLayoutJson,
        expected: WidgetLayoutJson,
    }

    #[derive(Deserialize)]
    struct LayoutViewportJson {
        width: u32,
        height: u32,
    }

    #[derive(Deserialize)]
    struct WidgetLayoutJson {
        x_percent: f64,
        y_percent: f64,
        scale: f64,
    }

    impl From<WidgetLayoutJson> for WidgetLayout {
        fn from(value: WidgetLayoutJson) -> Self {
            Self {
                x_percent: value.x_percent,
                y_percent: value.y_percent,
                scale: value.scale,
            }
        }
    }

    fn cases() -> Vec<ClampCase> {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/layout-clamp-cases.json"
        ))
        .unwrap()
    }

    #[test]
    fn clamp_matches_shared_cases_for_every_protected_region_edges_and_scale_bounds() {
        for case in cases() {
            let actual = clamp_widget(
                case.request.into(),
                LayoutViewport {
                    width: case.viewport.width,
                    height: case.viewport.height,
                },
            );
            let expected: WidgetLayout = case.expected.into();
            assert!(
                (actual.x_percent - expected.x_percent).abs() < 1.0e-9,
                "{} x: {} != {}",
                case.name,
                actual.x_percent,
                expected.x_percent
            );
            assert!(
                (actual.y_percent - expected.y_percent).abs() < 1.0e-9,
                "{} y: {} != {}",
                case.name,
                actual.y_percent,
                expected.y_percent
            );
            assert_eq!(actual.scale, expected.scale, "{} scale", case.name);
            assert!((SCALE_BOUNDS.0..=SCALE_BOUNDS.1).contains(&actual.scale));
        }
    }

    #[test]
    fn default_card_sits_centred_under_the_risk_pill_within_the_owner_allowance() {
        for viewport in [
            LayoutViewport {
                width: 1920,
                height: 1080,
            },
            LayoutViewport {
                width: 2560,
                height: 1440,
            },
        ] {
            let actual = clamp_widget(DEFAULT_LAYOUT, viewport);
            assert_eq!(actual, DEFAULT_LAYOUT, "the default needs no clamping");
            let width = f64::from(viewport.width);
            let height = f64::from(viewport.height);
            let px = width / 1920.0;
            let pill_bottom = GMETER_DEFAULT.y_percent * height / 100.0
                + GMETER_BASE_HEIGHT_PX * GMETER_DEFAULT.scale * px / 2.0;
            let card_top = actual.y_percent * height / 100.0 - 16.0 * actual.scale * px;
            let card_bottom = actual.y_percent * height / 100.0 + 16.0 * actual.scale * px;
            assert!(
                (card_top - pill_bottom - 4.0 * px).abs() < 1.0e-6,
                "4 px gap under RISK"
            );
            assert_eq!(
                actual.x_percent, GMETER_DEFAULT.x_percent,
                "centred on RISK"
            );
            let card_width = 232.0 * actual.scale * px;
            let pill_width = 170.0 * GMETER_DEFAULT.scale * px;
            assert!(
                card_width <= pill_width * 1.6 + 1.0e-9,
                "compact: at most 1.6x RISK"
            );
            assert!(card_bottom <= height * 0.08 + 24.0 * height / 1080.0);
            assert!(card_top >= height * 0.05);
        }
    }

    #[test]
    fn dragging_the_card_deeper_into_the_announcement_region_stops_at_the_allowance() {
        let deep = WidgetLayout {
            x_percent: 50.0,
            y_percent: 12.0,
            scale: 1.0,
        };
        let actual = clamp_widget(deep, LayoutViewport::default());
        let bottom_px = actual.y_percent * 1080.0 / 100.0 + 16.0;
        assert!(
            (bottom_px - (1080.0 * 0.08 + 24.0)).abs() < 1.0e-6,
            "bottom {bottom_px}"
        );
        let at_1440 = clamp_widget(
            deep,
            LayoutViewport {
                width: 2560,
                height: 1440,
            },
        );
        let bottom_1440 = at_1440.y_percent * 1440.0 / 100.0 + 16.0 * 2560.0 / 1920.0;
        assert!(
            (bottom_1440 - (1440.0 * 0.08 + 32.0)).abs() < 1.0e-6,
            "scaled with height"
        );
    }

    #[test]
    fn widgets_without_the_owner_allowance_stay_fully_blocked_from_the_announcement_region() {
        let actual = clamp_widget_with(DEFAULT_LAYOUT, LayoutViewport::default(), FULLY_BLOCKED);
        let bottom_px = actual.y_percent * 1080.0 / 100.0 + 16.0 * actual.scale;
        assert!(bottom_px <= 1080.0 * 0.08 + 1.0e-9, "bottom {bottom_px}");
        assert_ne!(FULLY_BLOCKED, OBJECTIVE_CARD_RULES);
    }

    #[test]
    fn a_saved_revision_4_default_reads_as_the_new_default_and_a_user_layout_still_wins() {
        let raw = serde_json::to_vec(&json!({
            "schema_version": 1,
            "widgets": {"objective_card": {
                "x_percent": LEGACY_DEFAULT_LAYOUT.x_percent,
                "y_percent": LEGACY_DEFAULT_LAYOUT.y_percent,
                "scale": LEGACY_DEFAULT_LAYOUT.scale,
            }}
        }))
        .unwrap();
        assert_eq!(parse_layout(&raw), (DEFAULT_LAYOUT, false));
        let moved = br#"{"schema_version":1,"widgets":{"objective_card":{"x_percent":20,"y_percent":40,"scale":1}}}"#;
        assert_eq!(parse_layout(moved).0.x_percent, 20.0);
    }

    #[test]
    fn corrupt_layout_roots_and_entries_fall_back_to_default_and_are_reported() {
        for raw in [
            br#"{"schema_version":2,"widgets":{}}"#.as_slice(),
            br#"{"schema_version":1,"widgets":{},"extra":true}"#,
            br#"{"schema_version":1,"widgets":{"objective_card":{"x_percent":4,"y_percent":"bad","scale":1}}}"#,
            br#"{"schema_version":1,"widgets":{"objective_card":{"x_percent":4,"y_percent":5,"scale":1,"extra":0}}}"#,
        ] {
            let (layout, invalid) = parse_layout(raw);
            assert_eq!(layout, DEFAULT_LAYOUT);
            assert!(invalid);
        }
    }

    #[test]
    fn parse_clamps_out_of_range_coordinates_and_scale_instead_of_applying_raw_values() {
        let request = json!({
            "widget": "objective_card",
            "x_percent": -30,
            "y_percent": 120,
            "scale": 99
        });
        let parsed = parse_patch(&request).unwrap();
        let clamped = clamp_widget(parsed, LayoutViewport::default());
        assert!(clamped.x_percent > 0.0);
        assert!(clamped.y_percent < 100.0);
        assert_eq!(clamped.scale, SCALE_BOUNDS.1);
    }

    #[test]
    fn patch_parser_rejects_unknown_keys_widgets_and_wrong_types() {
        for invalid in [
            json!({"widget": "risk_bar", "x_percent": 1, "y_percent": 2, "scale": 1}),
            json!({"widget": "objective_card", "x_percent": "1", "y_percent": 2, "scale": 1}),
            json!({"widget": "objective_card", "x_percent": 1, "y_percent": 2, "scale": 1, "extra": 0}),
            Value::Null,
        ] {
            assert!(parse_patch(&invalid).is_err());
        }
    }
}
