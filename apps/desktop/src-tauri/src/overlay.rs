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

/// Banner layout: the stage CENTRE sits at `x_percent` / `y_percent` of the monitor and the stage
/// is `scale` times min(680 px * H/1080, 72 % of the width) wide. This is a single struct so a dev
/// profile (Kohbor Lab) can override it; the shipped default is the owner's G-Maiden "profile 1".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BannerLayout {
    pub x_percent: f64,
    pub y_percent: f64,
    pub scale: f64,
    pub enabled: bool,
}

pub const DEFAULT_BANNER_LAYOUT: BannerLayout = BannerLayout {
    x_percent: 50.0,
    y_percent: 10.5,
    scale: 0.45,
    enabled: true,
};

const BANNER_BASE_WIDTH_PX: f64 = 680.0;
const BANNER_MAX_WIDTH_FRACTION: f64 = 0.72;
const BANNER_SCALE_RANGE: (f64, f64) = (0.2, 3.0);

/// Width in physical pixels of the art stage (before rounding).
fn banner_stage_width(monitor: &MonitorRect, layout: &BannerLayout) -> f64 {
    let height_scale = f64::from(monitor.h) / 1080.0;
    layout
        .scale
        .clamp(BANNER_SCALE_RANGE.0, BANNER_SCALE_RANGE.1)
        * (BANNER_BASE_WIDTH_PX * height_scale)
            .min(BANNER_MAX_WIDTH_FRACTION * f64::from(monitor.w))
}

/// Reference-to-physical scale: the 720-reference-pixel stage becomes `banner_stage_width` wide.
fn scale_for(monitor: &MonitorRect, layout: &BannerLayout) -> f64 {
    banner_stage_width(monitor, layout) / f64::from(STAGE_LOGICAL.0)
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

/// Place the whole transparent window in physical pixels on the selected monitor with the
/// default layout.
pub fn place_overlay(monitor: &MonitorRect) -> Rect {
    place_overlay_with(monitor, &DEFAULT_BANNER_LAYOUT).unwrap_or(Rect {
        x: monitor.x,
        y: monitor.y,
        w: 1,
        h: 1,
    })
}

/// Place the window for `layout`; `None` when the layout is disabled. The art stage CENTRE is at
/// the layout percentages (clamped to 0..=100); the 36-reference-pixel status strip hangs below
/// the stage and scales with it. The window is clamped inside the monitor, so near an edge the
/// banner is pushed inwards rather than cut off.
pub fn place_overlay_with(monitor: &MonitorRect, layout: &BannerLayout) -> Option<Rect> {
    if !layout.enabled {
        return None;
    }
    let scale = scale_for(monitor, layout);
    let w = scaled(WINDOW_LOGICAL.0, scale).min(monitor.w.max(1));
    let h = scaled(WINDOW_LOGICAL.1, scale).min(monitor.h.max(1));
    let stage_h = scaled(STAGE_LOGICAL.1, scale).min(h);
    let centre_x =
        f64::from(monitor.x) + f64::from(monitor.w) * layout.x_percent.clamp(0.0, 100.0) / 100.0;
    let centre_y =
        f64::from(monitor.y) + f64::from(monitor.h) * layout.y_percent.clamp(0.0, 100.0) / 100.0;
    let x = (centre_x - f64::from(w) / 2.0).round();
    let y = (centre_y - f64::from(stage_h) / 2.0).round();
    let to_i32 = |value: f64| value.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
    Some(Rect {
        x: clamp_axis(to_i32(x), w, monitor.x, monitor.w),
        y: clamp_axis(to_i32(y), h, monitor.y, monitor.h),
        w,
        h,
    })
}

/// The clip stage occupies the top 220 of the 256 reference-pixel window.
// Pinned by tests as the placement contract (docs/design/S6-OVERLAY-SLICE.md section 3); the runtime path
// derives the same geometry through `place_overlay`, so these are not read outside tests.
#[cfg_attr(not(test), allow(dead_code))]
pub fn stage_rect(monitor: &MonitorRect) -> Rect {
    let window = place_overlay(monitor);
    Rect {
        h: scaled(STAGE_LOGICAL.1, scale_for(monitor, &DEFAULT_BANNER_LAYOUT)).min(window.h),
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
            // Size: stage width = 0.45 * min(680 * H/1080, 0.72 * W); window and stage keep 720:256 and 720:220.
            let stage_width = 0.45 * (680.0 * f64::from(h) / 1080.0).min(0.72 * f64::from(w));
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
            // Position: the stage centre is at 50 % / 10.5 % unless the clamp had to move it.
            let ideal_x = (f64::from(monitor.x) + f64::from(w) * 0.50 - f64::from(window.w) / 2.0)
                .round() as i64;
            let max_x = i64::from(monitor.x) + i64::from(w) - i64::from(window.w);
            assert_eq!(i64::from(window.x), ideal_x.min(max_x), "{monitor:?}");
            let ideal_y = (f64::from(h) * 0.105 - f64::from(stage.h) / 2.0).round() as i64;
            assert_eq!(i64::from(window.y), ideal_y, "{monitor:?}");
            // The banner is horizontally centred (within 1 px of the monitor centre) near the top.
            assert!(
                (2 * i64::from(window.x) + i64::from(window.w)
                    - 2 * i64::from(monitor.x)
                    - i64::from(w))
                .abs()
                    <= 2
            );
            assert!(i64::from(window.y) + i64::from(stage.h) / 2 < i64::from(h) / 4);
            // Protected regions (UNVERIFIED Dota defaults) stay clear, EXCEPT the announcement area:
            // the owner's real in-game G-Maiden choice (centre 50 % / 10.5 %) lands inside that
            // unverified default and outranks it (S6-OVERLAY-SLICE edit is PROPOSED, not made).
            for (name, x0, y0, x1, y1) in PROTECTED_REGIONS {
                if name == "announcement" {
                    continue;
                }
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
        // 1920x1080: 306 px wide stage (0.45 * 680), window 306x109, stage 306x94, centred at
        // x=960 (left 807) with the stage centre 10.5 % down (top 66).
        let m = MonitorRect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        assert_eq!(
            place_overlay(&m),
            Rect {
                x: 807,
                y: 66,
                w: 306,
                h: 109
            }
        );
        let s = stage_rect(&m);
        assert_eq!((s.w, s.h), (306, 94));
        // 2560x1440: the 680 px part scales with H/1080, so the stage is 408 px wide.
        let m = MonitorRect {
            x: 0,
            y: 0,
            w: 2560,
            h: 1440,
        };
        assert_eq!(
            place_overlay(&m),
            Rect {
                x: 1076,
                y: 89,
                w: 408,
                h: 145
            }
        );
    }

    #[test]
    fn layout_struct_overrides_and_disabled_hides() {
        let m = MonitorRect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        assert_eq!(
            DEFAULT_BANNER_LAYOUT,
            BannerLayout {
                x_percent: 50.0,
                y_percent: 10.5,
                scale: 0.45,
                enabled: true
            }
        );
        // The previous (build 8) layout is reproducible through the struct.
        let right = BannerLayout {
            x_percent: 89.0,
            y_percent: 44.0,
            scale: 0.7,
            enabled: true,
        };
        assert_eq!(
            place_overlay_with(&m, &right),
            Some(Rect {
                x: 1444,
                y: 403,
                w: 476,
                h: 169
            })
        );
        let off = BannerLayout {
            enabled: false,
            ..DEFAULT_BANNER_LAYOUT
        };
        assert_eq!(place_overlay_with(&m, &off), None);
        // Out-of-range values are clamped: percentages to 0..=100 and scale to 0.2..=3.0.
        let wild = BannerLayout {
            x_percent: 500.0,
            y_percent: -20.0,
            scale: 99.0,
            enabled: true,
        };
        let r = place_overlay_with(&m, &wild).unwrap();
        assert!(r.x >= 0 && r.y >= 0 && i64::from(r.x) + i64::from(r.w) <= 1920);
        assert!(i64::from(r.y) + i64::from(r.h) <= 1080);
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
        assert_eq!(r.w, (0.45 * 460.8_f64).round() as u32);
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
