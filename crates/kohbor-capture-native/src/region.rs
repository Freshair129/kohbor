// Provenance: ported from owner-authored G-Maiden 0.13.6 src-tauri/src/cv/region.rs, owner consent recorded in claim vision-risk__mvp1-capture-native.

use crate::{OutputInfo, Rect};

/// Default 1080p inner-map crop, excluding the grey HUD frame. v2 is an ESTIMATE
/// from the owner's screenshot: x=10, y=811, 260x260. It is UNVERIFIED on a
/// full-screen frame. The profile name selects Inner structural validation in the host;
/// this region intentionally has no outer dark border. The user can nudge it in Live.
pub(crate) fn profile_rect(output: &OutputInfo) -> Option<(Rect, &'static str)> {
    if output.width != 1920 || output.height != 1080 || output.dpi != 96 {
        return None;
    }

    Some((
        Rect {
            x: 10,
            y: 811,
            width: 260,
            height: 260,
        },
        "p1080-default-v2",
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
                    x: 10,
                    y: 811,
                    width: 260,
                    height: 260
                },
                "p1080-default-v2"
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
