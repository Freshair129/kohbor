//! W0 scaffold: pins the frozen public API shape. W6 adds real tests beside this file.
use kohbor_minimap_cv::*;

#[test]
fn api_shape_is_frozen() {
    let _f: fn(&[u8], u32, u32, u32, Team) -> PixelReport = analyze;
}
