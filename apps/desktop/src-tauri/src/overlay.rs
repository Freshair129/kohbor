use serde_json::{json, Value};

#[derive(Debug, Clone, Copy)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

// Pinned by tests as the placement contract (docs/design/S6-OVERLAY-SLICE.md section 3); the runtime path
// derives the same geometry through `place_overlay`, so these are not read outside tests.
#[cfg_attr(not(test), allow(dead_code))]
pub const STAGE_LOGICAL: (u32, u32) = (720, 220);
pub const WINDOW_LOGICAL: (u32, u32) = (720, 256);

// Pinned by tests as the placement contract (docs/design/S6-OVERLAY-SLICE.md section 3); the runtime path
// derives the same geometry through `place_overlay`, so these are not read outside tests.
#[cfg_attr(not(test), allow(dead_code))]
pub const PROTECTED_REGIONS: [(&str, f64, f64, f64, f64); 4] = [
    ("minimap", 0.0, 0.70, 0.20, 1.0),
    ("bottom_hud", 0.0, 0.78, 1.0, 1.0),
    ("top_bar", 0.0, 0.0, 1.0, 0.05),
    ("announcement", 0.30, 0.08, 0.70, 0.19),
];

fn scale_for(monitor: &MonitorRect) -> f64 {
    (f64::from(monitor.h) / 1080.0).min(0.9 * f64::from(monitor.w) / 720.0)
}

fn scaled(value: u32, scale: f64) -> u32 {
    (f64::from(value) * scale).round().max(1.0) as u32
}

fn clamp_axis(position: i32, size: u32, origin: i32, extent: u32) -> i32 {
    let max = i64::from(origin) + i64::from(extent.saturating_sub(size));
    i64::from(position)
        .clamp(i64::from(origin), max)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// Place the whole transparent window in physical pixels on the selected monitor.
/// The art stage begins 220 reference pixels below the top edge.
pub fn place_overlay(monitor: &MonitorRect) -> Rect {
    let scale = scale_for(monitor);
    let w = scaled(WINDOW_LOGICAL.0, scale).min(monitor.w.max(1));
    let h = scaled(WINDOW_LOGICAL.1, scale).min(monitor.h.max(1));
    let centered_x = i64::from(monitor.x) + (i64::from(monitor.w) - i64::from(w)) / 2;
    let x = centered_x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    let y_offset = (220.0 * scale).round() as i64;
    let y =
        (i64::from(monitor.y) + y_offset).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    Rect {
        x: clamp_axis(x, w, monitor.x, monitor.w),
        y: clamp_axis(y, h, monitor.y, monitor.h),
        w,
        h,
    }
}

/// The clip stage occupies the top 220 of the 256 reference-pixel window.
// Pinned by tests as the placement contract (docs/design/S6-OVERLAY-SLICE.md section 3); the runtime path
// derives the same geometry through `place_overlay`, so these are not read outside tests.
#[cfg_attr(not(test), allow(dead_code))]
pub fn stage_rect(monitor: &MonitorRect) -> Rect {
    let window = place_overlay(monitor);
    Rect {
        h: scaled(STAGE_LOGICAL.1, scale_for(monitor)).min(window.h),
        ..window
    }
}

pub struct OverlayWindowSpec {
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

pub const fn overlay_window_spec() -> OverlayWindowSpec {
    OverlayWindowSpec {
        label: "overlay",
        url: "index.html?view=overlay",
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

// Mirrors capabilities/overlay.json; a test compares the two. Not read by the runtime.
#[cfg_attr(not(test), allow(dead_code))]
pub const fn overlay_capability_permissions() -> &'static [&'static str] {
    &["core:event:default"]
}

pub fn status_payload(state: &str, reason: Option<&str>) -> Value {
    json!({
        "media": {
            "state": state,
            "reason": reason,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_matrix_is_centred_safe_and_physical() {
        for (x, w, h) in [
            (0, 1920, 1080),
            (0, 2560, 1440),
            (0, 3840, 2160),
            (0, 1366, 768),
            (0, 1280, 720),
            (0, 1024, 768),
            (0, 2560, 1080),
            (0, 3440, 1440),
            (1920, 1920, 1080),
            (-1920, 1920, 1080),
        ] {
            let monitor = MonitorRect { x, y: 0, w, h };
            let window = place_overlay(&monitor);
            let stage = stage_rect(&monitor);
            assert!(
                window.x >= monitor.x
                    && window.y >= monitor.y
                    && i64::from(window.x) + i64::from(window.w)
                        <= i64::from(monitor.x) + i64::from(monitor.w)
                    && i64::from(window.y) + i64::from(window.h)
                        <= i64::from(monitor.y) + i64::from(monitor.h),
                "{monitor:?} {window:?}"
            );
            assert!(
                (2 * i64::from(window.x) + i64::from(window.w)
                    - 2 * i64::from(monitor.x)
                    - i64::from(monitor.w))
                .abs()
                    <= 2
            );
            assert!(
                (i64::from(window.y)
                    - i64::from(monitor.y)
                    - (220.0 * f64::from(h) / 1080.0).round() as i64)
                    .abs()
                    <= 1
            );
            let scale = (f64::from(h) / 1080.0).min(0.9 * f64::from(w) / 720.0);
            assert_eq!(
                (window.w, window.h),
                (
                    (720.0 * scale).round() as u32,
                    (256.0 * scale).round() as u32
                )
            );
            assert_eq!(
                (stage.x, stage.y, stage.w, stage.h),
                (window.x, window.y, window.w, (220.0 * scale).round() as u32)
            );
            assert!(
                i64::from(stage.y) + i64::from(stage.h)
                    <= i64::from(window.y) + i64::from(window.h)
            );
            for (name, x0, y0, x1, y1) in PROTECTED_REGIONS {
                let (left, top, right, bottom) = (
                    f64::from(monitor.x) + x0 * f64::from(w),
                    f64::from(monitor.y) + y0 * f64::from(h),
                    f64::from(monitor.x) + x1 * f64::from(w),
                    f64::from(monitor.y) + y1 * f64::from(h),
                );
                assert!(
                    !(f64::from(window.x) < right
                        && f64::from(window.x) + f64::from(window.w) > left
                        && f64::from(window.y) < bottom
                        && f64::from(window.y) + f64::from(window.h) > top),
                    "{name} {monitor:?} {window:?}"
                );
            }
        }
    }

    #[test]
    fn protected_region_defaults_are_exact() {
        assert_eq!(
            PROTECTED_REGIONS,
            [
                ("minimap", 0.0, 0.70, 0.20, 1.0),
                ("bottom_hud", 0.0, 0.78, 1.0, 1.0),
                ("top_bar", 0.0, 0.0, 1.0, 0.05),
                ("announcement", 0.30, 0.08, 0.70, 0.19),
            ]
        );
    }

    #[test]
    fn spec_flags() {
        let spec = overlay_window_spec();
        assert_eq!(
            (spec.label, spec.url),
            ("overlay", "index.html?view=overlay")
        );
        for (name, actual, expected) in [
            ("transparent", spec.transparent, true),
            ("decorations", spec.decorations, false),
            ("always_on_top", spec.always_on_top, true),
            ("skip_taskbar", spec.skip_taskbar, true),
            ("focusable", spec.focusable, false),
            ("focused", spec.focused, false),
            ("shadow", spec.shadow, false),
            ("resizable", spec.resizable, false),
            ("visible", spec.visible, false),
            ("ignore_cursor_events", spec.ignore_cursor_events, true),
        ] {
            assert_eq!(actual, expected, "{name}");
        }
    }

    #[test]
    fn permissions_are_listen_only() {
        assert_eq!(overlay_capability_permissions(), &["core:event:default"]);
        // The shipped capability file must say the same thing as the pinned function.
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/overlay.json")).unwrap();
        assert_eq!(capability["windows"], json!(["overlay"]));
        assert_eq!(capability["permissions"], json!(["core:event:default"]));
    }

    #[test]
    fn status_mirrors_reason_including_null() {
        assert_eq!(
            status_payload("unavailable", Some("AUDIO_MUTED")),
            json!({"media":{"state":"unavailable","reason":"AUDIO_MUTED"}})
        );
        assert_eq!(
            status_payload("ready", None),
            json!({"media":{"state":"ready","reason":null}})
        );
    }
}
