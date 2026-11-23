//! Native draft statistics: closed requests, bounded persistence, parsing and passive view state.
//!
//! The renderer receives only the view model in `view`; it never selects a URL or makes a request.

mod budget;
mod cache;
pub(crate) mod counters;
mod groups;
pub(crate) mod host;
pub(crate) mod layout;
mod parse;
mod prefs;
mod rank;
mod request;
pub(crate) mod transport;
mod view;

#[cfg(test)]
pub(crate) use cache::{CacheEntry, CACHE_FILE};
pub(crate) use prefs::DraftStatsPrefs;
#[cfg(test)]
pub(crate) use view::Failure;
pub(crate) use view::{DraftColumnView, DraftPhase};

/// On 2026-11-22 the owner reviewed the rule-B groups and chose option B: rule B plus the reviewed
/// Kohbor override list in `position-overrides.json`. That enables the in-game list.
pub(crate) const SUGGESTIONS_LIST_BUILD_ENABLED: bool = true;

/// Counter-pick suggestions (DEC-PICK-STATS revision 3). The owner approved the first 7.42 counter
/// table and the in-game mock on 2026-11-22 ("ตามที่แนะนำทั้งหมด"), so the switch is on. Turning it
/// off makes the view report `build_disabled` with no counter rows.
pub(crate) const COUNTERS_BUILD_ENABLED: bool = true;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Monitor {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Return the right-edge draft column in physical monitor pixels, scaled by height, for a content
/// height in 1080p pixels (capped at `layout::MAX_HEIGHT`). The host still refuses to draw unless
/// the output is exactly 1920x1080 at 100 percent.
pub(crate) fn draft_column_rect(monitor: Monitor, content_height: u32) -> Option<Rect> {
    if monitor.width == 0 || monitor.height == 0 || content_height == 0 {
        return None;
    }
    let scale = f64::from(monitor.height) / 1080.0;
    let x = f64::from(monitor.width) - 192.0 * scale;
    let y = f64::from(layout::TOP) * scale;
    let width = 180.0 * scale;
    let height = f64::from(content_height.min(layout::MAX_HEIGHT)) * scale;
    let (Ok(x), Ok(y), Ok(width), Ok(height)) = (
        i32::try_from(x.round() as i64),
        i32::try_from(y.round() as i64),
        u32::try_from(width.round() as i64),
        u32::try_from(height.round() as i64),
    ) else {
        return None;
    };
    Some(Rect {
        x: monitor.x.saturating_add(x),
        y: monitor.y.saturating_add(y),
        width,
        height,
    })
}

pub(crate) fn supported_output(monitor: Monitor, scale_factor: f64) -> bool {
    monitor.width == 1920
        && monitor.height == 1080
        && scale_factor.is_finite()
        && (scale_factor - 1.0).abs() < f64::EPSILON
}

#[cfg(test)]
mod geometry_tests {
    use super::{draft_column_rect, layout, supported_output, Monitor, Rect};

    const PROTECTED: [Rect; 6] = [
        Rect {
            x: 215,
            y: 0,
            width: 1493,
            height: 117,
        },
        Rect {
            x: 133,
            y: 181,
            width: 1159,
            height: 517,
        },
        Rect {
            x: 1344,
            y: 181,
            width: 375,
            height: 605,
        },
        Rect {
            x: 1345,
            y: 794,
            width: 269,
            height: 57,
        },
        Rect {
            x: 1627,
            y: 794,
            width: 92,
            height: 57,
        },
        Rect {
            x: 838,
            y: 869,
            width: 485,
            height: 193,
        },
    ];

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.x.saturating_add(b.width as i32)
            && b.x < a.x.saturating_add(a.width as i32)
            && a.y < b.y.saturating_add(b.height as i32)
            && b.y < a.y.saturating_add(a.height as i32)
    }

    #[test]
    fn placement_matches_reference_and_clears_protected_rectangles() {
        let monitor = Monitor {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        // Every Option A state height, including the cap, stays clear of the protected areas.
        for content in [56, 571, 685, 693, super::layout::MAX_HEIGHT, 5_000] {
            let rect = draft_column_rect(monitor, content).unwrap();
            assert!(PROTECTED
                .iter()
                .all(|protected| !overlaps(rect, *protected)));
            assert!(
                rect.y + rect.height as i32 <= 860,
                "{content}px ends at {}",
                rect.y + rect.height as i32
            );
        }
        let rect = draft_column_rect(monitor, 692).unwrap();
        assert_eq!(
            rect,
            Rect {
                x: 1728,
                y: 132,
                width: 180,
                height: 692
            }
        );
        assert_eq!(draft_column_rect(monitor, 0), None);
        assert!(PROTECTED
            .iter()
            .all(|protected| !overlaps(rect, *protected)));
        assert_eq!(rect.x + rect.width as i32, 1908);
        assert_eq!(1920 - (rect.x + rect.width as i32), 12);
        assert!(
            rect.x - 1719 >= 8,
            "column keeps at least 8 px after the detail panel"
        );
    }

    #[test]
    fn placement_scales_from_monitor_height_and_support_is_exact() {
        let monitor = Monitor {
            x: 100,
            y: 50,
            width: 2560,
            height: 1440,
        };
        assert_eq!(
            draft_column_rect(monitor, 488),
            Some(Rect {
                x: 2404,
                y: 226,
                width: 240,
                height: 651
            })
        );
        assert!(supported_output(
            Monitor {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            },
            1.0
        ));
        assert!(!supported_output(
            Monitor {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            },
            1.25
        ));
        assert!(!supported_output(
            Monitor {
                x: 0,
                y: 0,
                width: 1919,
                height: 1080
            },
            1.0
        ));
        assert_eq!(
            draft_column_rect(
                Monitor {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 1080
                },
                488
            ),
            None
        );
        // Strategy time and hero selection share the column; its largest rect ends at y 860.
        assert_eq!(layout::TOP + layout::MAX_HEIGHT as i32, 860);
    }
}
