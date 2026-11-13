#[derive(Debug, Clone, Copy)]
pub struct MonitorRect { pub x: i32, pub y: i32, pub w: u32, pub h: u32 }
#[derive(Debug, Clone, Copy)]
pub struct Rect { pub x: i32, pub y: i32, pub w: u32, pub h: u32 }
pub const STAGE_LOGICAL: (u32, u32) = (720, 220);
pub const WINDOW_LOGICAL: (u32, u32) = (720, 256);
pub const PROTECTED_REGIONS: [(&str, f64, f64, f64, f64); 4] = [
    ("minimap", 0.0, 0.70, 0.20, 1.0), ("bottom_hud", 0.0, 0.78, 1.0, 1.0),
    ("top_bar", 0.0, 0.0, 1.0, 0.05), ("announcement", 0.30, 0.08, 0.70, 0.19),
];
pub fn place_overlay(_m: &MonitorRect) -> Rect { unimplemented!() }
pub fn stage_rect(_m: &MonitorRect) -> Rect { unimplemented!() }
pub struct OverlayWindowSpec {
    pub label: &'static str, pub url: &'static str, pub transparent: bool, pub decorations: bool,
    pub always_on_top: bool, pub skip_taskbar: bool, pub focusable: bool, pub focused: bool,
    pub shadow: bool, pub resizable: bool, pub visible: bool, pub ignore_cursor_events: bool,
}
pub const fn overlay_window_spec() -> OverlayWindowSpec { unimplemented!() }
pub fn overlay_capability_permissions() -> &'static [&'static str] { unimplemented!() }
pub fn status_payload(_state: &str, _reason: Option<&str>) -> serde_json::Value { unimplemented!() }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_matrix_is_centred_safe_and_physical() {
        for (x, w, h) in [(0,1920,1080),(0,2560,1440),(0,3840,2160),(0,1366,768),(0,1280,720),(0,1024,768),(0,2560,1080),(0,3440,1440),(1920,1920,1080),(-1920,1920,1080)] {
            let m = MonitorRect { x, y: 0, w, h }; let r = place_overlay(&m); let s = stage_rect(&m);
            assert!(r.x >= m.x && r.y >= m.y && i64::from(r.x)+i64::from(r.w) <= i64::from(m.x)+i64::from(m.w) && i64::from(r.y)+i64::from(r.h) <= i64::from(m.y)+i64::from(m.h), "{m:?} {r:?}");
            assert!((2*i64::from(r.x)+i64::from(r.w)-2*i64::from(m.x)-i64::from(m.w)).abs() <= 2);
            assert!((i64::from(r.y)-i64::from(m.y)-(220.0*f64::from(h)/1080.0).round() as i64).abs() <= 1);
            let scale = (f64::from(h)/1080.0).min(0.9*f64::from(w)/720.0);
            assert_eq!((r.w,r.h), ((720.0*scale).round() as u32,(256.0*scale).round() as u32));
            assert_eq!((s.x,s.y,s.w,s.h), (r.x,r.y,r.w,(220.0*scale).round() as u32));
            assert!(i64::from(s.y)+i64::from(s.h) <= i64::from(r.y)+i64::from(r.h));
            for (name,x0,y0,x1,y1) in PROTECTED_REGIONS {
                let (left,top,right,bottom) = (f64::from(m.x)+x0*f64::from(w),f64::from(m.y)+y0*f64::from(h),f64::from(m.x)+x1*f64::from(w),f64::from(m.y)+y1*f64::from(h));
                assert!(!(f64::from(r.x)<right && f64::from(r.x)+f64::from(r.w)>left && f64::from(r.y)<bottom && f64::from(r.y)+f64::from(r.h)>top), "{name} {m:?} {r:?}");
            }
        }
    }
    #[test] fn protected_region_defaults_are_exact() {
        assert_eq!(PROTECTED_REGIONS, [("minimap",0.0,0.70,0.20,1.0),("bottom_hud",0.0,0.78,1.0,1.0),("top_bar",0.0,0.0,1.0,0.05),("announcement",0.30,0.08,0.70,0.19)]);
    }
    #[test] fn spec_flags() {
        let s = overlay_window_spec(); assert_eq!((s.label,s.url),("overlay","index.html?view=overlay"));
        for (name,actual,expected) in [("transparent",s.transparent,true),("decorations",s.decorations,false),("always_on_top",s.always_on_top,true),("skip_taskbar",s.skip_taskbar,true),("focusable",s.focusable,false),("focused",s.focused,false),("shadow",s.shadow,false),("resizable",s.resizable,false),("visible",s.visible,false),("ignore_cursor_events",s.ignore_cursor_events,true)] { assert_eq!(actual,expected,"{name}"); }
    }
    #[test] fn permissions_are_listen_only() { assert_eq!(overlay_capability_permissions(), &["core:event:default"]); }
    #[test] fn status_mirrors_reason_including_null() {
        assert_eq!(status_payload("unavailable",Some("AUDIO_MUTED")),serde_json::json!({"media":{"state":"unavailable","reason":"AUDIO_MUTED"}}));
        assert_eq!(status_payload("ready",None),serde_json::json!({"media":{"state":"ready","reason":null}}));
    }
}

