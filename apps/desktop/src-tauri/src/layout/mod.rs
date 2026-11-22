use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

const FILE_NAME: &str = "overlay-layout.json";
const DEFAULT_LAYOUT: WidgetLayout = WidgetLayout {
    x_percent: 62.5,
    y_percent: 6.5,
    scale: 1.0,
};
const CARD_BASE_WIDTH_PX: f64 = 272.0;
const CARD_BASE_HEIGHT_PX: f64 = 32.0;
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
    for (_, left, top, right, bottom) in crate::overlay::PROTECTED_REGIONS {
        x_candidates.push(clamp_value(
            left * 100.0 - bounds.half_width,
            bounds.min_x,
            bounds.max_x,
            start.x_percent,
        ));
        x_candidates.push(clamp_value(
            right * 100.0 + bounds.half_width,
            bounds.min_x,
            bounds.max_x,
            start.x_percent,
        ));
        y_candidates.push(clamp_value(
            top * 100.0 - bounds.half_height,
            bounds.min_y,
            bounds.max_y,
            start.y_percent,
        ));
        y_candidates.push(clamp_value(
            bottom * 100.0 + bounds.half_height,
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
            if collides(candidate, bounds) {
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

fn collides(layout: WidgetLayout, bounds: ClampBounds) -> bool {
    crate::overlay::PROTECTED_REGIONS
        .iter()
        .any(|(_, left, top, right, bottom)| {
            layout.x_percent - bounds.half_width < right * 100.0
                && layout.x_percent + bounds.half_width > left * 100.0
                && layout.y_percent - bounds.half_height < bottom * 100.0
                && layout.y_percent + bounds.half_height > top * 100.0
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
        clamp_widget, parse_layout, parse_patch, LayoutViewport, WidgetLayout, DEFAULT_LAYOUT,
        SAFE_EDGE_PX, SCALE_BOUNDS,
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
    fn default_card_is_safe_and_fits_the_hud_band_on_both_reference_screens() {
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
            let width = f64::from(viewport.width);
            let height = f64::from(viewport.height);
            let scale = width / 1920.0;
            let card_width = 272.0 * scale;
            let card_height = 32.0 * scale;
            assert!(actual.x_percent * width / 100.0 - card_width / 2.0 >= SAFE_EDGE_PX);
            assert!(actual.x_percent * width / 100.0 + card_width / 2.0 <= width - SAFE_EDGE_PX);
            assert!(actual.y_percent * height / 100.0 - card_height / 2.0 >= height * 0.05);
            assert!(actual.y_percent * height / 100.0 + card_height / 2.0 <= height * 0.08);
            assert!(!super::collides(
                actual,
                super::clamp_bounds(actual.scale, width, height)
            ));
        }
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
