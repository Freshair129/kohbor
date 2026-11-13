// Provenance: ported from owner-authored G-Maiden 0.13.6 src-tauri/src/cv/region.rs, owner consent recorded in claim vision-risk__mvp1-capture-native.

use crate::{OutputInfo, Rect};

/// Default 1080p crop: the whole grey Dota minimap frame, nothing beyond it. v3 is measured by
/// the lead from the owner's fix1 screenshot (frame left edge at screen x 0, right edge about
/// x 280, top edge about y 801, bottom edge flush with the screen bottom at y 1080), rounded to
/// the square rule: x=0, y=800, 280x280. UNVERIFIED on a real full-screen frame (the game was not
/// running at measurement time); the Live tab lets the user nudge it. History: v0 G-Maiden box,
/// v1 estimate 280x280 at y 797, v2 inner map 260x260 at (10,811), which the owner found too tight.
pub(crate) fn profile_rect(output: &OutputInfo) -> Option<(Rect, &'static str)> {
    if output.width != 1920 || output.height != 1080 || output.dpi != 96 {
        return None;
    }

    Some((
        Rect {
            x: 0,
            y: 800,
            width: 280,
            height: 280,
        },
        "p1080-default-v3",
    ))
}

#[cfg(test)]
mod tests {
    use crate::{profile_rect, OutputInfo, Rect};

    #[test]
    fn profile_rect_table_is_limited_to_the_supported_1080p_cell() {
        let supported = OutputInfo {
            output_id: 1,
            width: 1920,
            height: 1080,
            dpi: 96,
        };
        assert_eq!(
            profile_rect(&supported),
            Some((
                Rect {
                    x: 0,
                    y: 800,
                    width: 280,
                    height: 280
                },
                "p1080-default-v3"
            )),
        );

        for unsupported in [
            OutputInfo {
                width: 1919,
                ..supported
            },
            OutputInfo {
                height: 1079,
                ..supported
            },
            OutputInfo {
                dpi: 120,
                ..supported
            },
        ] {
            assert_eq!(profile_rect(&unsupported), None, "{unsupported:?}");
        }
    }
}
