//! Edge orb (UI-009) with the vertical sound mixer panel (UI-011): platform-independent parts.
//!
//! The orb is a separate transparent, topmost, never-activating window at the left edge of the Dota monitor. It is
//! click-through except for the revealed disc and the open panel; the outermost 8 px of the monitor never take the
//! pointer. A host cursor poll (10 Hz idle, 30 Hz while the pointer charges the reveal or the orb is out) drives the
//! state machine below; the renderer only draws what the host decides. Every orb or panel click is checked by the
//! fail-closed foreground guard: an activation closes the panel, restores click-through and hides the orb until the
//! match ends.
//!
//! Window, cursor and foreground access live in `host.rs`. This file holds the preferences, the geometry and
//! protected-region check, the hit test, the reveal state machine and the guard rule, all covered by tests.

pub(crate) mod host;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) const ORB_LABEL: &str = "orb";
pub(crate) const ORB_URL: &str = "index.html?view=orb";
pub(crate) const PREFS_FILE: &str = "orb-prefs.json";
const PREFS_SCHEMA_VERSION: u8 = 1;
pub(crate) const DWELL_CHOICES_MS: [u16; 3] = [300, 600, 1000];
pub(crate) const DEFAULT_DWELL_MS: u16 = 1000;

pub(crate) const IDLE_POLL: Duration = Duration::from_millis(100);
pub(crate) const ACTIVE_POLL: Duration = Duration::from_millis(33);
/// The disc slides out for 160 ms after the dwell and accepts clicks only after that (same timing in reduced motion).
pub(crate) const SLIDE: Duration = Duration::from_millis(160);
/// Tuck back 400 ms after the pointer leaves the disc and hot zone (panel closed).
pub(crate) const TUCK_AFTER: Duration = Duration::from_millis(400);
/// An open panel closes 1.5 s after the pointer leaves it, the disc and the hot zone.
pub(crate) const CLOSE_AFTER: Duration = Duration::from_millis(1500);
/// Second foreground sample after an orb click.
pub(crate) const CLICK_SETTLE: Duration = Duration::from_millis(100);
/// Physical pixels at every monitor edge that never take the pointer.
pub(crate) const EDGE_GUARD_PX: i32 = 8;

// ---------------------------------------------------------------------------------------------------------------
// Preferences

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct OrbPrefs {
    /// Off by default: DEC-EDGE-ORB keeps the orb unavailable until native acceptance; the owner turns it on.
    pub(crate) enabled: bool,
    pub(crate) dwell_ms: u16,
    pub(crate) focus_percent: u8,
    pub(crate) music_exe: Option<String>,
    /// UI-011 optional Focus hotkey: off by default. No key assignment exists in this build (see the report).
    pub(crate) focus_hotkey: Option<String>,
}

impl Default for OrbPrefs {
    fn default() -> Self {
        Self {
            enabled: false,
            dwell_ms: DEFAULT_DWELL_MS,
            focus_percent: crate::mixer::DEFAULT_FOCUS_PERCENT,
            music_exe: None,
            focus_hotkey: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPrefs {
    schema_version: u8,
    enabled: bool,
    dwell_ms: u16,
    focus_percent: u8,
    music_exe: Option<String>,
}

pub(crate) struct PrefsStore {
    path: PathBuf,
}

impl PrefsStore {
    pub(crate) fn in_data_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(PREFS_FILE),
        }
    }

    pub(crate) fn load(&self) -> OrbPrefs {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| parse_prefs(&bytes))
            .unwrap_or_default()
    }

    pub(crate) fn save(&self, prefs: &OrbPrefs) -> Result<(), String> {
        let bytes = serde_json::to_vec(&StoredPrefs {
            schema_version: PREFS_SCHEMA_VERSION,
            enabled: prefs.enabled,
            dwell_ms: prefs.dwell_ms,
            focus_percent: prefs.focus_percent,
            music_exe: prefs.music_exe.clone(),
        })
        .map_err(|_| "ORB_PREFS_WRITE_FAILED".to_owned())?;
        crate::atomic::write(&self.path, &bytes).map_err(|_| "ORB_PREFS_WRITE_FAILED".to_owned())
    }
}

fn valid_exe(exe: &str) -> bool {
    (5..=64).contains(&exe.len())
        && exe.ends_with(".exe")
        && exe
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b' '))
}

fn parse_prefs(bytes: &[u8]) -> Option<OrbPrefs> {
    let stored: StoredPrefs = serde_json::from_slice(bytes).ok()?;
    if stored.schema_version != PREFS_SCHEMA_VERSION
        || !DWELL_CHOICES_MS.contains(&stored.dwell_ms)
        || stored.focus_percent > 100
        || stored
            .music_exe
            .as_deref()
            .is_some_and(|exe| !valid_exe(exe))
    {
        return None;
    }
    Some(OrbPrefs {
        enabled: stored.enabled,
        dwell_ms: stored.dwell_ms,
        focus_percent: stored.focus_percent,
        music_exe: stored.music_exe.map(|exe| exe.to_ascii_lowercase()),
        focus_hotkey: None,
    })
}

/// Apply a Settings patch: only `enabled`, `dwell_ms` and `focus_percent` (music choice goes through `mixer_set`).
pub(crate) fn patch_prefs(current: &OrbPrefs, patch: &Value) -> Result<OrbPrefs, String> {
    let object = patch.as_object().ok_or("ORB_PATCH_INVALID")?;
    if object.is_empty() {
        return Err("ORB_PATCH_INVALID".to_owned());
    }
    let mut next = current.clone();
    for (key, value) in object {
        match key.as_str() {
            "enabled" => next.enabled = value.as_bool().ok_or("ORB_PATCH_INVALID")?,
            "dwell_ms" => {
                let dwell = value
                    .as_u64()
                    .and_then(|dwell| u16::try_from(dwell).ok())
                    .filter(|dwell| DWELL_CHOICES_MS.contains(dwell))
                    .ok_or("ORB_PATCH_INVALID")?;
                next.dwell_ms = dwell;
            }
            "focus_percent" => {
                next.focus_percent = value
                    .as_u64()
                    .filter(|percent| (5..=100).contains(percent))
                    .map(|percent| percent as u8)
                    .ok_or("ORB_PATCH_INVALID")?;
            }
            _ => return Err("ORB_PATCH_INVALID".to_owned()),
        }
    }
    Ok(next)
}

// ---------------------------------------------------------------------------------------------------------------
// Geometry (logical px of the orb window, whose origin is the monitor's top-left corner)

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RectF {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

impl RectF {
    fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }

    fn intersects(self, other: RectF) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }

    fn to_json(self) -> Value {
        json!({"x": self.x, "y": self.y, "w": self.w, "h": self.h})
    }
}

/// The 1080p reference values of the approved prototype (proto-mixer NOTES.md), scaled by monitor height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Layout {
    pub(crate) monitor_w: f64,
    pub(crate) monitor_h: f64,
    pub(crate) s: f64,
    pub(crate) sp: f64,
    pub(crate) disc: f64,
    pub(crate) dead: f64,
    pub(crate) cy: f64,
    pub(crate) hot: RectF,
    pub(crate) out_cx: f64,
    pub(crate) panel_x: f64,
    pub(crate) panel_w: f64,
    pub(crate) window_w: f64,
}

/// Panel heights at 1080p before the zoom: without the session note (503 px measured in a synthetic browser
/// preview, test-only) and the largest accepted height (540 px measured with the note).
#[cfg(test)]
// Owner 2026-11-22: the panel is drawn at 80 % (CSS zoom sp * 0.8) so it covers less of the game.
pub(crate) const PANEL_BASE_H: f64 = 403.0;
pub(crate) const PANEL_MAX_H: f64 = 437.0;
const PANEL_BASE_W: f64 = 176.0;
/// Room kept between the panel and the minimap band for the panel's own shadow.
const SHADOW_ROOM: f64 = 8.0;

pub(crate) fn layout(monitor_w: f64, monitor_h: f64) -> Option<Layout> {
    if !monitor_w.is_finite() || !monitor_h.is_finite() || monitor_w < 640.0 || monitor_h < 480.0 {
        return None;
    }
    let s = (monitor_h / 1080.0).clamp(0.72, 1.34);
    let sp = s.clamp(0.90, 1.34);
    let disc = (52.0 * s).round();
    let r = disc / 2.0;
    let dead = (8.0 * s.max(1.0)).round();
    let cy = (monitor_h * 0.30).round();
    let hot_x1 = (r + 10.0 * s).round();
    let half_h = (r + 8.0 * s).round();
    let panel_x = dead + disc + 12.0;
    let panel_w = (PANEL_BASE_W * sp).round();
    Some(Layout {
        monitor_w,
        monitor_h,
        s,
        sp,
        disc,
        dead,
        cy,
        hot: RectF {
            x: dead,
            y: cy - half_h,
            w: hot_x1 - dead,
            h: half_h * 2.0,
        },
        out_cx: dead + r,
        panel_x,
        panel_w,
        window_w: (panel_x + panel_w + SHADOW_ROOM).ceil(),
    })
}

/// The protected regions of the overlay contract, in this monitor's logical px.
fn protected(layout: &Layout) -> Vec<RectF> {
    crate::overlay::PROTECTED_REGIONS
        .iter()
        .map(|(_, x0, y0, x1, y1)| RectF {
            x: x0 * layout.monitor_w,
            y: y0 * layout.monitor_h,
            w: (x1 - x0) * layout.monitor_w,
            h: (y1 - y0) * layout.monitor_h,
        })
        .collect()
}

/// Place a panel of `height` (renderer-measured, logical px). Lines the focus button up with the orb when there is
/// room; never touches a protected region (shadow included). `None` = it does not fit: the panel stays closed.
pub(crate) fn panel_rect(layout: &Layout, height: f64) -> Option<RectF> {
    if !height.is_finite() || height <= 0.0 || height > PANEL_MAX_H * layout.sp + 1.0 {
        return None;
    }
    let min_top = (layout.monitor_h * 0.05).ceil() + 4.0;
    let max_bottom = layout.monitor_h * 0.70 - SHADOW_ROOM;
    let preferred = layout.cy - 31.0 * layout.sp;
    let top = preferred.min(max_bottom - height).max(min_top).floor();
    let rect = RectF {
        x: layout.panel_x,
        y: top,
        w: layout.panel_w,
        h: height,
    };
    let with_shadow = RectF {
        x: rect.x,
        y: rect.y,
        w: rect.w + SHADOW_ROOM,
        h: rect.h + SHADOW_ROOM,
    };
    if protected(layout)
        .iter()
        .any(|region| region.intersects(with_shadow))
    {
        return None;
    }
    Some(rect)
}

/// Physical placement of the orb window on its monitor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
    /// Monitor rectangle in physical screen pixels: x, y, w, h.
    pub(crate) monitor: (i32, i32, i32, i32),
    pub(crate) scale: f64,
    pub(crate) layout: Layout,
}

impl Geometry {
    pub(crate) fn new(monitor: (i32, i32, i32, i32), scale: f64) -> Option<Self> {
        if !scale.is_finite()
            || scale <= 0.0
            || monitor.2 <= 2 * EDGE_GUARD_PX
            || monitor.3 <= 2 * EDGE_GUARD_PX
        {
            return None;
        }
        let layout = layout(f64::from(monitor.2) / scale, f64::from(monitor.3) / scale)?;
        Some(Self {
            monitor,
            scale,
            layout,
        })
    }

    /// Window size in physical pixels: the full monitor height (the panel is placed within it), narrow width.
    pub(crate) fn window_physical(&self) -> (u32, u32) {
        (
            (self.layout.window_w * self.scale).ceil() as u32,
            u32::try_from(self.monitor.3).unwrap_or(0),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    Outside,
    Hot,
    Disc,
    Panel,
}

/// Classify a physical cursor position. The outer 8 physical px of the top, right and bottom edges are always
/// `Outside`; the left edge and dead strip are `Hot` inside the orb's height band and `Outside` elsewhere; the disc
/// counts only once revealed, the panel only while open.
pub(crate) fn zone(
    cursor: (i32, i32),
    geometry: &Geometry,
    revealed: bool,
    panel: Option<RectF>,
) -> Zone {
    let (mx, my, mw, mh) = geometry.monitor;
    if cursor.0 < mx
        || cursor.1 < my.saturating_add(EDGE_GUARD_PX)
        || cursor.0 >= mx.saturating_add(mw).saturating_sub(EDGE_GUARD_PX)
        || cursor.1 >= my.saturating_add(mh).saturating_sub(EDGE_GUARD_PX)
    {
        return Zone::Outside;
    }
    let x = f64::from(cursor.0 - mx) / geometry.scale;
    let y = f64::from(cursor.1 - my) / geometry.scale;
    let layout = &geometry.layout;
    // Owner decision 2026-11-21: a pointer pushed against the left edge (guard + dead strip) inside the orb's height
    // band counts as Hot, so resting on the edge reveals the orb. The rest of the edge stays Outside for edge-pan.
    if cursor.0 < mx.saturating_add(EDGE_GUARD_PX) || x < layout.dead {
        return if layout.hot.contains(layout.hot.x, y) {
            Zone::Hot
        } else {
            Zone::Outside
        };
    }
    if let Some(panel) = panel {
        if panel.contains(x, y) {
            return Zone::Panel;
        }
    }
    if revealed {
        let r = layout.disc / 2.0;
        let (dx, dy) = (x - layout.out_cx, y - layout.cy);
        if dx * dx + dy * dy <= r * r {
            return Zone::Disc;
        }
    }
    if layout.hot.contains(x, y) {
        // Owner decision 2026-11-22: while tucked, the hot rect is the click target that opens the panel.
        return if revealed { Zone::Hot } else { Zone::Disc };
    }
    Zone::Outside
}

// ---------------------------------------------------------------------------------------------------------------
// Reveal state machine

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HideReason {
    NotInMatch,
    DotaNotForeground,
    ExclusiveFullscreen,
    PlacementUnavailable,
    FailedClosed,
}

impl HideReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NotInMatch => "not_in_match",
            Self::DotaNotForeground => "dota_not_foreground",
            Self::ExclusiveFullscreen => "exclusive_fullscreen",
            Self::PlacementUnavailable => "placement_unavailable",
            Self::FailedClosed => "failed_closed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Phase {
    Hidden(HideReason),
    Tucked,
    Charging { since: Instant },
    Sliding { until: Instant },
    Out { leave_since: Option<Instant> },
    Open { leave_since: Option<Instant> },
}

#[derive(Clone, Debug)]
pub(crate) struct Machine {
    pub(crate) phase: Phase,
    pub(crate) dwell: Duration,
    pub(crate) failed_closed: bool,
    pub(crate) locked_until: Option<Instant>,
    pub(crate) panel: Option<RectF>,
    pub(crate) zone: Zone,
}

impl Machine {
    pub(crate) fn new(dwell_ms: u16) -> Self {
        Self {
            phase: Phase::Hidden(HideReason::NotInMatch),
            dwell: Duration::from_millis(u64::from(dwell_ms)),
            failed_closed: false,
            locked_until: None,
            panel: None,
            zone: Zone::Outside,
        }
    }

    pub(crate) fn revealed(&self) -> bool {
        matches!(self.phase, Phase::Out { .. } | Phase::Open { .. })
    }

    pub(crate) fn panel_open(&self) -> bool {
        matches!(self.phase, Phase::Open { .. })
    }

    pub(crate) fn visible(&self) -> bool {
        !matches!(self.phase, Phase::Hidden(_))
    }

    fn locked(&self, now: Instant) -> bool {
        self.locked_until.is_some_and(|until| now < until)
    }

    fn close_panel(&mut self) {
        self.panel = None;
        if matches!(self.phase, Phase::Open { .. }) {
            self.phase = Phase::Out { leave_since: None };
        }
    }

    /// Advance with the eligibility decided by the host and the current pointer zone.
    pub(crate) fn tick(&mut self, now: Instant, eligible: Result<(), HideReason>, zone: Zone) {
        self.zone = zone;
        let eligible = if self.failed_closed {
            Err(HideReason::FailedClosed)
        } else {
            eligible
        };
        if let Err(reason) = eligible {
            self.panel = None;
            self.phase = Phase::Hidden(reason);
            return;
        }
        if self.locked(now) {
            self.panel = None;
            self.phase = Phase::Tucked;
            return;
        }
        self.locked_until = None;
        self.phase = match self.phase {
            Phase::Hidden(_) | Phase::Tucked => {
                if matches!(zone, Zone::Hot | Zone::Disc) {
                    Phase::Charging { since: now }
                } else {
                    Phase::Tucked
                }
            }
            Phase::Charging { since } => {
                if !matches!(zone, Zone::Hot | Zone::Disc) {
                    Phase::Tucked
                } else if now.saturating_duration_since(since) >= self.dwell {
                    Phase::Sliding { until: now + SLIDE }
                } else {
                    Phase::Charging { since }
                }
            }
            Phase::Sliding { until } => {
                if now >= until {
                    Phase::Out { leave_since: None }
                } else {
                    Phase::Sliding { until }
                }
            }
            Phase::Out { leave_since } => match zone {
                Zone::Hot | Zone::Disc => Phase::Out { leave_since: None },
                _ => {
                    let since = leave_since.unwrap_or(now);
                    if now.saturating_duration_since(since) >= TUCK_AFTER {
                        Phase::Tucked
                    } else {
                        Phase::Out {
                            leave_since: Some(since),
                        }
                    }
                }
            },
            Phase::Open { leave_since } => match zone {
                Zone::Hot | Zone::Disc | Zone::Panel => Phase::Open { leave_since: None },
                Zone::Outside => {
                    let since = leave_since.unwrap_or(now);
                    if now.saturating_duration_since(since) >= CLOSE_AFTER {
                        self.panel = None;
                        Phase::Out { leave_since: None }
                    } else {
                        Phase::Open {
                            leave_since: Some(since),
                        }
                    }
                }
            },
        };
    }

    /// Whether the window must take the pointer right now (all other times it is click-through).
    pub(crate) fn wants_input(&self) -> bool {
        match self.phase {
            Phase::Tucked | Phase::Charging { .. } => {
                self.zone == Zone::Disc && self.locked_until.is_none()
            }
            Phase::Out { .. } => self.zone == Zone::Disc,
            Phase::Open { .. } => matches!(self.zone, Zone::Disc | Zone::Panel),
            _ => false,
        }
    }

    pub(crate) fn poll_interval(&self) -> Duration {
        match self.phase {
            Phase::Hidden(_) => IDLE_POLL,
            Phase::Tucked if self.zone == Zone::Outside => IDLE_POLL,
            _ => ACTIVE_POLL,
        }
    }

    /// A click on the disc toggles the panel; refused unless the disc is under the pointer. Owner decision
    /// 2026-11-22: a click on the tucked disc opens the panel at once (no dwell); refused while a critical cue locks it.
    pub(crate) fn click_disc(&mut self) -> Result<bool, &'static str> {
        if self.zone != Zone::Disc {
            return Err("ORB_NOT_UNDER_POINTER");
        }
        match self.phase {
            Phase::Tucked | Phase::Charging { .. } if self.locked_until.is_some() => Err("ORB_LOCKED"),
            Phase::Tucked | Phase::Charging { .. } | Phase::Out { .. } => {
                self.phase = Phase::Open { leave_since: None };
                Ok(true)
            }
            Phase::Open { .. } => {
                self.close_panel();
                Ok(false)
            }
            _ => Err("ORB_NOT_REVEALED"),
        }
    }

    pub(crate) fn close(&mut self) {
        self.close_panel();
    }

    /// Fail closed after an activation: hidden until the match ends.
    pub(crate) fn fail_closed(&mut self) {
        self.failed_closed = true;
        self.panel = None;
        self.phase = Phase::Hidden(HideReason::FailedClosed);
    }

    pub(crate) fn match_ended(&mut self) {
        self.failed_closed = false;
        self.locked_until = None;
        self.panel = None;
    }

    /// A qualified critical alert: close the panel and lock the orb (tucked, no reveal, no clicks) for the cue.
    pub(crate) fn lock_for_critical(&mut self, now: Instant, duration: Duration) {
        self.locked_until = Some(now + duration);
        self.panel = None;
        if !matches!(self.phase, Phase::Hidden(_)) {
            self.phase = Phase::Tucked;
        }
    }

    /// The renderer's view. `charge_ms` lets it animate the 80 -> 100 % scale cue between polls.
    pub(crate) fn payload(&self, now: Instant, geometry: Option<&Geometry>) -> Value {
        let (phase, reason, charge_ms) = match self.phase {
            Phase::Hidden(reason) => ("hidden", Some(reason.as_str()), None),
            Phase::Tucked => ("tucked", None, None),
            Phase::Charging { since } => (
                "charging",
                None,
                Some(now.saturating_duration_since(since).as_millis() as u64),
            ),
            Phase::Sliding { .. } => ("sliding", None, None),
            Phase::Out { .. } => ("out", None, None),
            Phase::Open { .. } => ("open", None, None),
        };
        let layout = geometry.map(|geometry| {
            let layout = geometry.layout;
            json!({
                "disc": layout.disc, "dead": layout.dead, "cy": layout.cy, "out_cx": layout.out_cx,
                "hot": layout.hot.to_json(), "panel_x": layout.panel_x, "panel_w": layout.panel_w,
                "sp": layout.sp, "monitor_h": layout.monitor_h
            })
        });
        json!({
            "visible": self.visible(),
            "phase": phase,
            "reason": reason,
            "dwell_ms": self.dwell.as_millis() as u64,
            "charge_ms": charge_ms,
            "locked": self.locked_until.is_some(),
            "panel": self.panel.map(RectF::to_json),
            "layout": layout,
        })
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Foreground guard

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Foreground {
    pub(crate) hwnd: isize,
    pub(crate) exe: Option<String>,
}

impl Foreground {
    pub(crate) fn exe_or_unknown(&self) -> &str {
        self.exe.as_deref().unwrap_or("unknown")
    }

    pub(crate) fn is_dota(&self) -> bool {
        self.exe.as_deref() == Some(crate::mixer::GAME_EXE)
    }
}

/// Focus is kept when the pre-click foreground was Dota and the sample after the click is still that window's
/// process and not the orb. An unknown pre-click sample is not proof of anything: it counts as lost.
pub(crate) fn kept_focus(before: Option<&Foreground>, after: &Foreground, orb_hwnd: isize) -> bool {
    let Some(before) = before else {
        return false;
    };
    before.is_dota() && after.hwnd != orb_hwnd && after.exe == before.exe
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(scale: f64) -> Geometry {
        let w = (1920.0 * scale) as i32;
        let h = (1080.0 * scale) as i32;
        Geometry::new((100, 50, w, h), scale).unwrap()
    }

    fn at(geometry: &Geometry, x: f64, y: f64) -> (i32, i32) {
        (
            geometry.monitor.0 + (x * geometry.scale).round() as i32,
            geometry.monitor.1 + (y * geometry.scale).round() as i32,
        )
    }

    #[test]
    fn layout_matches_the_prototype_at_1080p() {
        let layout = layout(1920.0, 1080.0).unwrap();
        assert_eq!(layout.disc, 52.0);
        assert_eq!(layout.dead, 8.0);
        assert_eq!(layout.cy, 324.0);
        assert_eq!(
            layout.hot,
            RectF {
                x: 8.0,
                y: 290.0,
                w: 28.0,
                h: 68.0
            }
        );
        assert_eq!(layout.out_cx, 34.0);
        assert_eq!(layout.panel_w, 176.0);
        let panel = panel_rect(&layout, PANEL_BASE_H).unwrap();
        assert_eq!(panel.x, 72.0);
        // Ends above the minimap band with room for the shadow.
        assert!(panel.y + panel.h <= 1080.0 * 0.70 - 8.0);
        assert!(panel.y >= 1080.0 * 0.05);
    }

    #[test]
    fn panel_never_touches_a_protected_region_at_common_sizes() {
        for (w, h) in [
            (1280.0, 720.0),
            (1366.0, 768.0),
            (1920.0, 1080.0),
            (2560.0, 1440.0),
            (3840.0, 2160.0),
        ] {
            let layout = layout(w, h).unwrap();
            let panel = panel_rect(&layout, PANEL_BASE_H * layout.sp).expect("base panel fits");
            for region in protected(&layout) {
                assert!(!region.intersects(panel), "{w}x{h}");
            }
        }
    }

    #[test]
    fn a_panel_that_cannot_fit_stays_closed() {
        let layout = layout(1280.0, 720.0).unwrap();
        // Taller than the room above the minimap band (or the cap): refused, never drawn over it.
        assert!(panel_rect(&layout, PANEL_MAX_H * layout.sp + 2.0).is_none());
        assert!(panel_rect(&layout, f64::NAN).is_none());
        assert!(panel_rect(&layout, 2000.0).is_none());
    }

    #[test]
    fn invalid_monitor_or_scale_has_no_geometry() {
        assert!(Geometry::new((0, 0, 1920, 1080), 0.0).is_none());
        assert!(Geometry::new((0, 0, 1920, 1080), f64::NAN).is_none());
        assert!(Geometry::new((0, 0, 10, 10), 1.0).is_none());
    }

    #[test]
    fn edges_take_the_pointer_only_in_the_orb_band() {
        let g = geometry(1.0);
        let panel = panel_rect(&g.layout, PANEL_BASE_H);
        // Every edge, every state.
        for revealed in [false, true] {
            // Left edge outside the orb's height band stays free for edge-pan.
            assert_eq!(zone((107, 200 + 50), &g, revealed, panel), Zone::Outside);
            assert_eq!(zone((100, 600 + 50), &g, revealed, panel), Zone::Outside);
            // Left edge inside the band (owner 2026-11-21) is Hot, at the very edge too.
            assert_eq!(zone((107, 324 + 50), &g, revealed, panel), Zone::Hot);
            assert_eq!(zone((100, 324 + 50), &g, revealed, panel), Zone::Hot);
            assert_eq!(zone(at(&g, 0.0, 300.0), &g, revealed, panel), Zone::Hot);
            assert_eq!(zone((99, 324 + 50), &g, revealed, panel), Zone::Outside);
            assert_eq!(zone((150, 57), &g, revealed, panel), Zone::Outside);
            assert_eq!(
                zone((150, 50 + 1080 - 8), &g, revealed, panel),
                Zone::Outside
            );
        }
        assert_eq!(zone(at(&g, 8.0, 324.0), &g, false, None), Zone::Disc);
        assert_eq!(zone(at(&g, 7.0, 324.0), &g, true, None), Zone::Hot);
        assert_eq!(zone(at(&g, 7.0, 289.0), &g, true, None), Zone::Outside);
    }

    #[test]
    fn hit_zones_follow_the_state() {
        let g = geometry(1.0);
        // Tucked: the hot rect is the click target (owner 2026-11-22); revealed it is only the hot zone.
        assert_eq!(zone(at(&g, 20.0, 324.0), &g, false, None), Zone::Disc);
        assert_eq!(zone(at(&g, 35.0, 357.0), &g, false, None), Zone::Disc);
        assert_eq!(zone(at(&g, 36.0, 324.0), &g, false, None), Zone::Outside);
        assert_eq!(zone(at(&g, 20.0, 289.0), &g, false, None), Zone::Outside);
        assert_eq!(zone(at(&g, 50.0, 324.0), &g, false, None), Zone::Outside);
        // Revealed: the 26 px radius disc at x 34.
        assert_eq!(zone(at(&g, 50.0, 324.0), &g, true, None), Zone::Disc);
        assert_eq!(zone(at(&g, 61.0, 324.0), &g, true, None), Zone::Outside);
        assert_eq!(zone(at(&g, 34.0, 289.0), &g, true, None), Zone::Outside);
        // The panel counts only while a rect is open.
        let panel = panel_rect(&g.layout, PANEL_BASE_H).unwrap();
        let inside = at(&g, panel.x + 10.0, panel.y + 10.0);
        assert_eq!(zone(inside, &g, true, Some(panel)), Zone::Panel);
        assert_eq!(zone(inside, &g, true, None), Zone::Outside);
    }

    #[test]
    fn hit_test_converts_physical_pixels_at_dpi_scale() {
        let g = geometry(1.5);
        assert_eq!(zone(at(&g, 50.0, 324.0), &g, true, None), Zone::Disc);
        assert_eq!(zone(at(&g, 61.0, 324.0), &g, true, None), Zone::Outside);
        assert_eq!(g.window_physical().1, 1620);
    }

    fn run(machine: &mut Machine, start: Instant, ms: u64, zone: Zone) -> Instant {
        let mut now = start;
        let end = start + Duration::from_millis(ms);
        while now < end {
            now += Duration::from_millis(33);
            machine.tick(now, Ok(()), zone);
        }
        now
    }

    #[test]
    fn reveal_needs_the_full_dwell_then_the_slide_before_clicks() {
        let t0 = Instant::now();
        let mut machine = Machine::new(1000);
        machine.tick(t0, Ok(()), Zone::Outside);
        assert_eq!(machine.phase, Phase::Tucked);
        let now = run(&mut machine, t0, 900, Zone::Hot);
        assert!(matches!(machine.phase, Phase::Charging { .. }));
        assert!(!machine.wants_input());
        let now = run(&mut machine, now, 140, Zone::Hot);
        assert!(
            matches!(machine.phase, Phase::Sliding { .. }),
            "{:?}",
            machine.phase
        );
        machine.tick(now, Ok(()), Zone::Disc);
        assert!(!machine.wants_input(), "no clicks while sliding");
        let now = run(&mut machine, now, 170, Zone::Disc);
        assert!(matches!(machine.phase, Phase::Out { .. }));
        assert!(machine.wants_input());
        let _ = now;
    }

    #[test]
    fn leaving_before_the_dwell_resets_progress() {
        let t0 = Instant::now();
        let mut machine = Machine::new(1000);
        let now = run(&mut machine, t0, 800, Zone::Hot);
        let now = run(&mut machine, now, 50, Zone::Outside);
        assert_eq!(machine.phase, Phase::Tucked);
        let _ = run(&mut machine, now, 800, Zone::Hot);
        assert!(
            matches!(machine.phase, Phase::Charging { .. }),
            "progress is not kept"
        );
    }

    #[test]
    fn dwell_setting_is_respected() {
        let t0 = Instant::now();
        let mut machine = Machine::new(300);
        let _ = run(&mut machine, t0, 380, Zone::Hot);
        assert!(matches!(
            machine.phase,
            Phase::Sliding { .. } | Phase::Out { .. }
        ));
    }

    fn revealed_machine() -> (Machine, Instant) {
        let t0 = Instant::now();
        let mut machine = Machine::new(300);
        let now = run(&mut machine, t0, 400, Zone::Hot);
        let now = run(&mut machine, now, 200, Zone::Disc);
        assert!(machine.revealed());
        (machine, now)
    }

    #[test]
    fn disc_click_opens_and_closes_the_panel_and_leaving_tucks() {
        let (mut machine, now) = revealed_machine();
        assert_eq!(machine.click_disc(), Ok(true));
        assert!(machine.panel_open());
        machine.panel = panel_rect(&layout(1920.0, 1080.0).unwrap(), PANEL_BASE_H);
        machine.tick(now, Ok(()), Zone::Panel);
        assert!(machine.wants_input());
        let now = run(&mut machine, now, 1400, Zone::Outside);
        assert!(machine.panel_open(), "still open before 1.5 s");
        let now = run(&mut machine, now, 200, Zone::Outside);
        assert!(!machine.panel_open() && machine.panel.is_none());
        let _ = run(&mut machine, now, 450, Zone::Outside);
        assert_eq!(machine.phase, Phase::Tucked);
    }

    #[test]
    fn one_click_on_the_tucked_orb_opens_the_panel() {
        let t0 = Instant::now();
        let mut machine = Machine::new(1000);
        machine.tick(t0, Ok(()), Zone::Outside);
        assert!(!machine.wants_input(), "click-through away from the target");
        machine.tick(t0, Ok(()), Zone::Hot);
        assert!(!machine.wants_input(), "the edge strip never takes the pointer");
        machine.tick(t0, Ok(()), Zone::Disc);
        assert!(machine.wants_input(), "the tucked target takes the click");
        assert_eq!(machine.click_disc(), Ok(true));
        assert!(machine.panel_open() && machine.revealed());
        machine.tick(t0, Ok(()), Zone::Disc);
        assert!(machine.panel_open(), "stays open under the pointer");
    }

    #[test]
    fn clicks_are_refused_unless_under_the_pointer_or_while_locked() {
        let mut machine = Machine::new(1000);
        machine.tick(Instant::now(), Ok(()), Zone::Outside);
        assert_eq!(machine.click_disc(), Err("ORB_NOT_UNDER_POINTER"));
        let t0 = Instant::now();
        let mut locked = Machine::new(1000);
        locked.lock_for_critical(t0, Duration::from_millis(1500));
        locked.tick(t0, Ok(()), Zone::Disc);
        assert!(!locked.wants_input());
        assert_eq!(locked.click_disc(), Err("ORB_LOCKED"));
        let (mut machine, now) = revealed_machine();
        machine.tick(now, Ok(()), Zone::Hot);
        assert_eq!(machine.click_disc(), Err("ORB_NOT_UNDER_POINTER"));
    }

    #[test]
    fn ineligible_states_hide_and_close_immediately() {
        for reason in [
            HideReason::NotInMatch,
            HideReason::DotaNotForeground,
            HideReason::ExclusiveFullscreen,
            HideReason::PlacementUnavailable,
        ] {
            let (mut machine, now) = revealed_machine();
            machine.click_disc().unwrap();
            machine.tick(now, Err(reason), Zone::Panel);
            assert_eq!(machine.phase, Phase::Hidden(reason));
            assert!(!machine.wants_input() && machine.panel.is_none());
        }
    }

    #[test]
    fn activation_fails_closed_until_the_match_ends() {
        let (mut machine, now) = revealed_machine();
        machine.click_disc().unwrap();
        machine.fail_closed();
        assert!(!machine.visible() && !machine.wants_input());
        let now = run(&mut machine, now, 2000, Zone::Hot);
        assert_eq!(machine.phase, Phase::Hidden(HideReason::FailedClosed));
        machine.match_ended();
        machine.tick(now, Ok(()), Zone::Outside);
        assert_eq!(machine.phase, Phase::Tucked);
    }

    #[test]
    fn critical_alert_closes_the_panel_and_locks_the_orb() {
        let (mut machine, now) = revealed_machine();
        machine.click_disc().unwrap();
        machine.lock_for_critical(now, Duration::from_millis(1500));
        assert!(!machine.panel_open() && machine.panel.is_none());
        let later = run(&mut machine, now, 1400, Zone::Hot);
        assert_eq!(machine.phase, Phase::Tucked, "no reveal while locked");
        assert!(machine.click_disc().is_err());
        let _ = run(&mut machine, later, 200, Zone::Hot);
        assert!(
            matches!(machine.phase, Phase::Charging { .. }),
            "reveal resumes after the cue"
        );
    }

    #[test]
    fn poll_rate_is_ten_hertz_idle_and_thirty_when_active() {
        let mut machine = Machine::new(1000);
        machine.tick(Instant::now(), Ok(()), Zone::Outside);
        assert_eq!(machine.poll_interval(), IDLE_POLL);
        machine.tick(Instant::now(), Ok(()), Zone::Hot);
        assert_eq!(machine.poll_interval(), ACTIVE_POLL);
        machine.tick(Instant::now(), Err(HideReason::NotInMatch), Zone::Hot);
        assert_eq!(machine.poll_interval(), IDLE_POLL);
    }

    #[test]
    fn payload_carries_charge_progress_for_the_scale_cue() {
        let t0 = Instant::now();
        let mut machine = Machine::new(1000);
        machine.tick(t0, Ok(()), Zone::Hot);
        let payload = machine.payload(t0 + Duration::from_millis(400), None);
        assert_eq!(payload["phase"], "charging");
        assert_eq!(payload["charge_ms"], 400);
        assert_eq!(payload["dwell_ms"], 1000);
    }

    fn fg(hwnd: isize, exe: &str) -> Foreground {
        Foreground {
            hwnd,
            exe: Some(exe.to_owned()),
        }
    }

    #[test]
    fn guard_keeps_focus_only_when_dota_stays_foreground() {
        let before = fg(10, "dota2.exe");
        assert!(kept_focus(Some(&before), &fg(10, "dota2.exe"), 99));
        assert!(!kept_focus(
            Some(&before),
            &fg(99, "kohbor-desktop.exe"),
            99
        ));
        assert!(!kept_focus(Some(&before), &fg(11, "explorer.exe"), 99));
        assert!(!kept_focus(None, &fg(10, "dota2.exe"), 99));
        assert!(!kept_focus(
            Some(&fg(11, "explorer.exe")),
            &fg(11, "explorer.exe"),
            99
        ));
        let unknown = Foreground { hwnd: 0, exe: None };
        assert!(!kept_focus(Some(&before), &unknown, 99));
    }

    #[test]
    fn prefs_default_off_and_parse_strictly() {
        assert!(!OrbPrefs::default().enabled);
        assert_eq!(OrbPrefs::default().dwell_ms, 1000);
        assert_eq!(OrbPrefs::default().focus_percent, 20);
        assert!(OrbPrefs::default().focus_hotkey.is_none());
        let good = br#"{"schema_version":1,"enabled":true,"dwell_ms":600,"focus_percent":25,"music_exe":"Spotify.exe"}"#;
        let parsed = parse_prefs(good).unwrap();
        assert_eq!(parsed.music_exe.as_deref(), Some("spotify.exe"));
        assert!(parse_prefs(br#"{"schema_version":1,"enabled":true,"dwell_ms":700,"focus_percent":25,"music_exe":null}"#).is_none());
        assert!(parse_prefs(br#"{"schema_version":1,"enabled":true,"dwell_ms":600,"focus_percent":25,"music_exe":"..\\x.exe"}"#).is_none());
        assert!(parse_prefs(br#"{"schema_version":1,"enabled":true,"dwell_ms":600,"focus_percent":25,"music_exe":null,"x":1}"#).is_none());
    }

    #[test]
    fn prefs_patch_accepts_only_known_bounded_fields() {
        let base = OrbPrefs::default();
        let next = patch_prefs(
            &base,
            &json!({"enabled": true, "dwell_ms": 300, "focus_percent": 30}),
        )
        .unwrap();
        assert!(next.enabled && next.dwell_ms == 300 && next.focus_percent == 30);
        assert!(patch_prefs(&base, &json!({"dwell_ms": 500})).is_err());
        assert!(patch_prefs(&base, &json!({"focus_percent": 0})).is_err());
        assert!(patch_prefs(&base, &json!({"music_exe": "x.exe"})).is_err());
        assert!(patch_prefs(&base, &json!({})).is_err());
    }

    #[test]
    fn prefs_store_round_trips() {
        let dir = std::env::temp_dir().join(format!("kohbor-orb-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = PrefsStore::in_data_dir(&dir);
        assert_eq!(store.load(), OrbPrefs::default());
        let prefs = OrbPrefs {
            enabled: true,
            dwell_ms: 600,
            focus_percent: 15,
            music_exe: Some("spotify.exe".into()),
            focus_hotkey: None,
        };
        store.save(&prefs).unwrap();
        assert_eq!(store.load(), prefs);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn orb_window_is_not_a_startup_window_and_has_a_listen_only_capability() {
        let config: Value = serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        assert!(!config["app"]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|window| window["label"] == ORB_LABEL));
        let capability: Value =
            serde_json::from_str(include_str!("../../capabilities/orb.json")).unwrap();
        assert_eq!(capability["windows"], json!([ORB_LABEL]));
        assert_eq!(capability["permissions"], json!(["core:event:default"]));
    }

    #[test]
    fn orb_window_is_mapped_without_activation() {
        let host = include_str!("host.rs");
        assert!(host.contains("crate::show_without_activation(&window)"));
        assert!(
            !host.contains(concat!("window", ".show()")),
            "the activating show is never used"
        );
        assert!(host.contains(".focusable(false)") && host.contains(".focused(false)"));
        assert!(host.contains("WS_EX_NOACTIVATE"));
    }
}

#[cfg(test)]
mod click_through_guard_tests {
    /// Regression (owner test 2026-11-21): toggling click-through through Tauri re-adds `WS_EX_LAYERED` and the
    /// disc stops drawing. The poll loop may only switch `WS_EX_TRANSPARENT` itself; Tauri is only ever asked to
    /// keep the window click-through (`true`), which never removes the layered bit.
    #[test]
    fn the_poll_loop_never_asks_tauri_to_turn_click_through_off() {
        let host = include_str!("host.rs");
        let tauri_calls: Vec<&str> = host
            .match_indices("set_ignore_cursor_events(")
            .map(|(i, _)| &host[i..i + 32])
            .collect();
        assert!(!tauri_calls.is_empty());
        for call in tauri_calls {
            assert!(call.starts_with("set_ignore_cursor_events(true)"), "{call}");
        }
        assert!(host.contains("fn set_click_through(hwnd: HWND, click_through: bool) -> bool"));
    }
}
