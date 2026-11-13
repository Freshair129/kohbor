//! W0 scaffold: pins the frozen public API shape. W2 adds real tests beside this file.
use kohbor_capture::Rect;
use kohbor_capture_native::*;

#[test]
fn api_shape_is_frozen() {
    let _open: fn() -> Result<MinimapGrabber, OpenError> = MinimapGrabber::open;
    let _output: fn(&MinimapGrabber) -> OutputInfo = MinimapGrabber::output;
    let _grab: fn(&mut MinimapGrabber, Rect, u32, &mut Vec<u8>) -> Grab = MinimapGrabber::grab;
    let _gpu: fn(&MinimapGrabber) -> Option<u64> = MinimapGrabber::gpu_local_bytes;
    let _rect: fn(&OutputInfo) -> Option<(Rect, &'static str)> = profile_rect;
    let _win: fn() -> Option<Rect> = game_window;
}
