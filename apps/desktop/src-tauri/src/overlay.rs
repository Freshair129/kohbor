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

// Reference stage size; the runtime scales it through `scale_for` (see the banner layout constants).
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

/// Default banner layout (follows the owner-chosen G-Maiden defaults, DEC pending in S6-OVERLAY-SLICE):
/// the stage CENTRE sits at 89 % across and 44 % down the monitor, the banner is 0.7 of
/// min(680 px, 72 % of the width), where the 680 px part scales with the monitor height / 1080.
pub const BANNER_CENTER_PERCENT: (f64, f64) = (89.0, 44.0);
pub const BANNER_SCALE: f64 = 0.7;
const BANNER_BASE_WIDTH_PX: f64 = 680.0;
const BANNER_MAX_WIDTH_FRACTION: f64 = 0.72;

/// Width in physical pixels of the art stage (before rounding).
fn banner_stage_width(monitor: &MonitorRect) -> f64 {
    let height_scale = f64::from(monitor.h) / 1080.0;
    BANNER_SCALE
        * (BANNER_BASE_WIDTH_PX * height_scale)
            .min(BANNER_MAX_WIDTH_FRACTION * f64::from(monitor.w))
}

/// Reference-to-physical scale: the 720-reference-pixel stage becomes `banner_stage_width` wide.
fn scale_for(monitor: &MonitorRect) -> f64 {
    banner_stage_width(monitor) / f64::from(STAGE_LOGICAL.0)
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
/// The art stage CENTRE is at `BANNER_CENTER_PERCENT` of the monitor; the 36-reference-pixel
/// status strip hangs below the stage and scales with it. The window is clamped inside the
/// monitor, so near the right edge the banner is pushed left rather than cut off.
pub fn place_overlay(monitor: &MonitorRect) -> Rect {
    let scale = scale_for(monitor);
    let w = scaled(WINDOW_LOGICAL.0, scale).min(monitor.w.max(1));
    let h = scaled(WINDOW_LOGICAL.1, scale).min(monitor.h.max(1));
    let stage_h = scaled(STAGE_LOGICAL.1, scale).min(h);
    let centre_x = f64::from(monitor.x) + f64::from(monitor.w) * BANNER_CENTER_PERCENT.0 / 100.0;
    let centre_y = f64::from(monitor.y) + f64::from(monitor.h) * BANNER_CENTER_PERCENT.1 / 100.0;
    let x = (centre_x - f64::from(w) / 2.0).round();
    let y = (centre_y - f64::from(stage_h) / 2.0).round();
    let to_i32 = |value: f64| value.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
    Rect {
        x: clamp_axis(to_i32(x), w, monitor.x, monitor.w),
        y: clamp_axis(to_i32(y), h, monitor.y, monitor.h),
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
    fn default_placement_follows_the_banner_layout_on_the_matrix() {
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
            // Inside the monitor.
            assert!(
                window.x >= monitor.x
                    && window.y >= monitor.y
                    && i64::from(window.x) + i64::from(window.w)
                        <= i64::from(monitor.x) + i64::from(monitor.w)
                    && i64::from(window.y) + i64::from(window.h)
                        <= i64::from(monitor.y) + i64::from(monitor.h),
                "{monitor:?} {window:?}"
            );
            // Size: stage width = 0.7 * min(680 * H/1080, 0.72 * W); window and stage keep 720:256 and 720:220.
            let stage_width = 0.7 * (680.0 * f64::from(h) / 1080.0).min(0.72 * f64::from(w));
            let scale = stage_width / 720.0;
            assert_eq!(
                (window.w, window.h),
                (
                    (720.0 * scale).round() as u32,
                    (256.0 * scale).round() as u32
                ),
                "{monitor:?}"
            );
            assert_eq!(
                (stage.x, stage.y, stage.w, stage.h),
                (window.x, window.y, window.w, (220.0 * scale).round() as u32)
            );
            assert!(
                i64::from(stage.y) + i64::from(stage.h)
                    <= i64::from(window.y) + i64::from(window.h)
            );
            // Position: the stage centre is at 89 % / 44 % unless the clamp had to move it.
            let ideal_x = (f64::from(monitor.x) + f64::from(w) * 0.89 - f64::from(window.w) / 2.0)
                .round() as i64;
            let max_x = i64::from(monitor.x) + i64::from(w) - i64::from(window.w);
            assert_eq!(i64::from(window.x), ideal_x.min(max_x), "{monitor:?}");
            let ideal_y = (f64::from(h) * 0.44 - f64::from(stage.h) / 2.0).round() as i64;
            assert_eq!(i64::from(window.y), ideal_y, "{monitor:?}");
            // The banner sits on the right side, above the vertical middle of the screen.
            assert!(
                i64::from(window.x) + i64::from(window.w) / 2
                    > i64::from(monitor.x) + i64::from(w) / 2
            );
            // Protected regions (UNVERIFIED Dota defaults) stay clear.
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
    fn exact_numbers_at_1080p_and_1440p() {
        // 1920x1080: 476 px wide stage, window 476x169, stage 476x145; the right edge clamp moves
        // the unclamped x=1471 to 1444 (centre ~1682 px); the stage centre height is 0.44 * 1080.
        let r = place_overlay(&MonitorRect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        });
        assert_eq!(
            r,
            Rect {
                x: 1444,
                y: 403,
                w: 476,
                h: 169
            }
        );
        let s = stage_rect(&MonitorRect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        });
        assert_eq!((s.w, s.h), (476, 145));
        // 2560x1440: the 680 px part scales with H/1080 so the banner is 635 px wide.
        let r = place_overlay(&MonitorRect {
            x: 0,
            y: 0,
            w: 2560,
            h: 1440,
        });
        assert_eq!((r.w, r.h), (635, 226));
        // Unclamped x would be 1961; the monitor edge clamp gives 2560 - 635 = 1925.
        assert_eq!((r.x, r.y), (1925, 537));
    }

    #[test]
    fn width_is_capped_by_72_percent_of_a_narrow_monitor() {
        // 1024x768: 680 * 768/1080 = 483.6 < 0.72 * 1024 = 737, so the height term wins;
        // 640x1080: 0.72 * 640 = 460.8 < 680 so the width term wins.
        let narrow = MonitorRect {
            x: 0,
            y: 0,
            w: 640,
            h: 1080,
        };
        let r = place_overlay(&narrow);
        assert_eq!(r.w, (0.7 * 460.8_f64).round() as u32);
        assert!(i64::from(r.x) + i64::from(r.w) <= 640);
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
