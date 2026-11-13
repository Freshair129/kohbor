// Provenance: ported from owner-authored G-Maiden 0.13.6 src-tauri/src/cv/region.rs, owner consent recorded in claim vision-risk__mvp1-capture-native.

use crate::{OutputInfo, Rect};

/// Default 1080p crop from the owner-authored G-Maiden cv/region.rs measurement:
/// x 8-290, y 790-1070; the 292-pixel candidate keeps a small margin around it.
pub(crate) fn profile_rect(output: &OutputInfo) -> Option<(Rect, &'static str)> {
    if output.width != 1920 || output.height != 1080 || output.dpi != 96 {
        return None;
    }

    Some((
        Rect {
            x: 0,
            y: 788,
            width: 292,
            height: 292,
        },
        "p1080-default-v0",
    ))
}

#[cfg(test)]
mod tests {
    use crate::{profile_rect, OutputInfo, Rect};

    #[test]
    fn profile_rect_table_is_limited_to_the_measured_1080p_cell() {
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
                    y: 788,
                    width: 292,
                    height: 292
                },
                "p1080-default-v0"
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
