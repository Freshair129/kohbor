// Provenance: ported from owner-authored G-Maiden 0.13.6 src-tauri/src/capture_frame.rs, owner consent recorded in claim vision-risk__mvp1-capture-native.

use kohbor_capture::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CropLayout {
    pub(crate) row_bytes: usize,
    pub(crate) stride: u32,
    pub(crate) byte_len: usize,
}

pub(crate) fn rect_fits_output(rect: Rect, output_width: u32, output_height: u32) -> bool {
    if rect.x < 0 || rect.y < 0 || rect.width == 0 || rect.height == 0 {
        return false;
    }

    let Some(right) = i64::from(rect.x).checked_add(i64::from(rect.width)) else {
        return false;
    };
    let Some(bottom) = i64::from(rect.y).checked_add(i64::from(rect.height)) else {
        return false;
    };

    right <= i64::from(output_width) && bottom <= i64::from(output_height)
}

pub(crate) fn crop_layout(rect: Rect) -> Option<CropLayout> {
    if rect.width == 0 || rect.height == 0 {
        return None;
    }

    let row_bytes = usize::try_from(rect.width).ok()?.checked_mul(4)?;
    let stride = u32::try_from(row_bytes).ok()?;
    let byte_len = row_bytes.checked_mul(usize::try_from(rect.height).ok()?)?;

    Some(CropLayout {
        row_bytes,
        stride,
        byte_len,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_bounds_accept_exact_edges_and_reject_invalid_geometry() {
        assert!(rect_fits_output(
            Rect {
                x: 1900,
                y: 1060,
                width: 20,
                height: 20
            },
            1920,
            1080,
        ));
        assert!(!rect_fits_output(
            Rect {
                x: -1,
                y: 0,
                width: 1,
                height: 1
            },
            1920,
            1080,
        ));
        assert!(!rect_fits_output(
            Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 1
            },
            1920,
            1080,
        ));
        assert!(!rect_fits_output(
            Rect {
                x: 1919,
                y: 0,
                width: 2,
                height: 1
            },
            1920,
            1080,
        ));
        assert!(!rect_fits_output(
            Rect {
                x: i32::MAX,
                y: 0,
                width: u32::MAX,
                height: 1
            },
            u32::MAX,
            1080,
        ));
    }

    #[test]
    fn crop_layout_uses_checked_tight_bgra_stride() {
        assert_eq!(
            crop_layout(Rect {
                x: 0,
                y: 0,
                width: 292,
                height: 292,
            }),
            Some(CropLayout {
                row_bytes: 1168,
                stride: 1168,
                byte_len: 341_056,
            }),
        );
        assert_eq!(
            crop_layout(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 1,
            }),
            None,
        );
    }
}
