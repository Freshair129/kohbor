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
    x_percent: 49.46,
    y_percent: 10.3,
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

/// The layout used at show time. Release builds always use the compiled default; only builds with
/// the `lab` feature (or debug assertions) may substitute a validated dev profile (Kohbor Lab).
pub fn effective_banner_layout() -> BannerLayout {
    #[cfg(any(feature = "lab", debug_assertions))]
    {
        crate::lab::banner_layout()
    }
    #[cfg(not(any(feature = "lab", debug_assertions)))]
    {
        DEFAULT_BANNER_LAYOUT
    }
}

/// Place the whole transparent window in physical pixels on the selected monitor with the
/// default layout.
#[cfg_attr(not(test), allow(dead_code))]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoDiagnosticOutcome {
    Started,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoDiagnostic {
    pub playback_id: String,
    pub event_type: Option<String>,
    pub outcome: VideoDiagnosticOutcome,
    pub error_code: Option<u8>,
    pub reason_code: &'static str,
}

impl VideoDiagnostic {
    pub fn log_event_name(&self) -> &'static str {
        match self.outcome {
            VideoDiagnosticOutcome::Started => "video_started",
            VideoDiagnosticOutcome::Failed => "video_failed",
        }
    }

    pub fn log_message(&self) -> &'static str {
        match self.outcome {
            VideoDiagnosticOutcome::Started => "overlay video started playing",
            VideoDiagnosticOutcome::Failed => "overlay video failed",
        }
    }
}

/// Parse the overlay renderer's bounded video lifecycle event. It contains no URL,
/// media path, or user-provided text; the overlay capability needs event access only.
pub fn parse_video_diagnostic(payload: &Value) -> Option<VideoDiagnostic> {
    let object = payload.as_object()?;
    if object.len() != 5 {
        return None;
    }
    let playback_id = object.get("playback_id")?.as_str()?;
    let digits = playback_id.strip_prefix("pb_")?;
    if !(10..=20).contains(&digits.len()) || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let event_type = match object.get("event_type")? {
        Value::Null => None,
        Value::String(value)
            if !value.is_empty()
                && value.len() <= 80
                && value.as_bytes()[0].is_ascii_lowercase()
                && value.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                }) =>
        {
            Some(value.clone())
        }
        _ => return None,
    };
    let error_code = match object.get("error_code")? {
        Value::Null => None,
        Value::Number(value) => {
            let value = u8::try_from(value.as_u64()?).ok()?;
            if !(1..=4).contains(&value) {
                return None;
            }
            Some(value)
        }
        _ => return None,
    };
    let reason_code = match object.get("reason_code")?.as_str()? {
        "NONE" => "NONE",
        "VIDEO_ERROR" => "VIDEO_ERROR",
        "VIDEO_URL_UNAVAILABLE" => "VIDEO_URL_UNAVAILABLE",
        "VIDEO_START_TIMEOUT" => "VIDEO_START_TIMEOUT",
        _ => return None,
    };
    let outcome = match object.get("outcome")?.as_str()? {
        "started" if error_code.is_none() && reason_code == "NONE" => {
            VideoDiagnosticOutcome::Started
        }
        "failed" if reason_code != "NONE" => VideoDiagnosticOutcome::Failed,
        _ => return None,
    };
    Some(VideoDiagnostic {
        playback_id: playback_id.to_owned(),
        event_type,
        outcome,
        error_code,
        reason_code,
    })
}

/// Shared parse/log seam: production supplies the host logger, tests capture the same bounded
/// record without needing a live Tauri window.
pub fn parse_and_log_video_diagnostic(
    payload: &Value,
    mut log: impl FnMut(&VideoDiagnostic),
) -> Option<VideoDiagnostic> {
    let diagnostic = parse_video_diagnostic(payload)?;
    log(&diagnostic);
    Some(diagnostic)
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
            // Position: the stage centre is at 49.46 % / 10.3 % unless the clamp had to move it.
            let ideal_x = (f64::from(monitor.x) + f64::from(w) * 0.4946 - f64::from(window.w) / 2.0)
                .round() as i64;
            let max_x = i64::from(monitor.x) + i64::from(w) - i64::from(window.w);
            assert_eq!(i64::from(window.x), ideal_x.min(max_x), "{monitor:?}");
            let ideal_y = (f64::from(h) * 0.103 - f64::from(stage.h) / 2.0).round() as i64;
            assert_eq!(i64::from(window.y), ideal_y, "{monitor:?}");
            // The banner is nearly horizontally centred (within 2 % of the monitor width) near the top.
            assert!(
                (2 * i64::from(window.x) + i64::from(window.w)
                    - 2 * i64::from(monitor.x)
                    - i64::from(w))
                .abs()
                    <= (f64::from(w) * 0.02) as i64 + 2
            );
            assert!(i64::from(window.y) + i64::from(stage.h) / 2 < i64::from(h) / 4);
            // Protected regions (UNVERIFIED Dota defaults) stay clear, EXCEPT the announcement area:
            // the owner's real in-game G-Maiden choice (centre 49.46 % / 10.3 %) lands inside that
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
        // x=949.6 (left 797) with the stage centre 10.3 % down (top 64).
        let m = MonitorRect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        assert_eq!(
            place_overlay(&m),
            Rect {
                x: 797,
                y: 64,
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
                x: 1062,
                y: 86,
                w: 408,
                h: 145
            }
        );
    }

    #[test]
    fn parses_only_bounded_overlay_video_diagnostics() {
        let parsed = parse_video_diagnostic(&json!({
            "playback_id": "pb_0000000001",
            "event_type": "hero_kill",
            "outcome": "failed",
            "error_code": 4,
            "reason_code": "VIDEO_ERROR"
        }))
        .expect("valid video error event parses");
        assert_eq!(parsed.playback_id, "pb_0000000001");
        assert_eq!(parsed.event_type.as_deref(), Some("hero_kill"));
        assert_eq!(parsed.outcome, VideoDiagnosticOutcome::Failed);
        assert_eq!(parsed.error_code, Some(4));
        assert_eq!(parsed.reason_code, "VIDEO_ERROR");
        assert_eq!(parsed.log_event_name(), "video_failed");
        assert_eq!(parsed.log_message(), "overlay video failed");

        let started = parse_video_diagnostic(&json!({
            "playback_id": "pb_0000000002",
            "event_type": null,
            "outcome": "started",
            "error_code": null,
            "reason_code": "NONE"
        }))
        .expect("valid started event parses");
        assert_eq!(started.outcome, VideoDiagnosticOutcome::Started);
        assert_eq!(started.event_type, None);
        assert_eq!(started.log_event_name(), "video_started");

        for payload in [
            json!({"playback_id":"pb_0000000001","event_type":"hero_kill","outcome":"failed","error_code":9,"reason_code":"VIDEO_ERROR"}),
            json!({"playback_id":"pb_0000000001","event_type":"hero_kill","outcome":"failed","error_code":4,"reason_code":"VIDEO_ERROR","url":"secret"}),
            json!({"playback_id":"pb_0000000001","event_type":"../path","outcome":"failed","error_code":4,"reason_code":"VIDEO_ERROR"}),
        ] {
            assert!(parse_video_diagnostic(&payload).is_none());
        }
    }

    #[test]
    fn diagnostic_log_seam_emits_only_valid_overlay_records() {
        let mut logged = Vec::new();
        let payload = json!({
            "playback_id": "pb_0000000003",
            "event_type": "audition",
            "outcome": "failed",
            "error_code": null,
            "reason_code": "VIDEO_URL_UNAVAILABLE"
        });
        let parsed = parse_and_log_video_diagnostic(&payload, |diagnostic| {
            logged.push((diagnostic.log_event_name(), diagnostic.reason_code));
        })
        .expect("valid failure parses");
        assert_eq!(parsed.log_event_name(), "video_failed");
        assert_eq!(logged, [("video_failed", "VIDEO_URL_UNAVAILABLE")]);

        let invalid = json!({"playback_id":"bad","event_type":"audition","outcome":"failed","error_code":4,"reason_code":"VIDEO_ERROR"});
        assert!(
            parse_and_log_video_diagnostic(&invalid, |_| panic!("invalid event was logged"))
                .is_none()
        );

        let timed_out = parse_video_diagnostic(&json!({
            "playback_id": "pb_0000000004",
            "event_type": "audition",
            "outcome": "failed",
            "error_code": null,
            "reason_code": "VIDEO_START_TIMEOUT"
        }))
        .expect("non-starting video is an explicit bounded failure");
        assert_eq!(timed_out.reason_code, "VIDEO_START_TIMEOUT");
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
                x_percent: 49.46,
                y_percent: 10.3,
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
