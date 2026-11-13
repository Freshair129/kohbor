#![allow(dead_code)]
use kohbor_capture::*;

pub fn controller(epoch: u64) -> CaptureController {
    let mut c = CaptureController::new(CaptureSource::Dxgi, epoch);
    let env = Environment {
        output: OutputGeometry {
            output_id: 7,
            width: 100,
            height: 100,
            dpi: 96,
        },
        monitor_id: 2,
        window_rect: Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        },
        minimap_layout: "left".into(),
    };
    c.calibrator_mut()
        .begin(
            env,
            Rect {
                x: 0,
                y: 0,
                width: 20,
                height: 20,
            },
            None,
        )
        .unwrap();
    for _ in 0..30 {
        c.calibrator_mut().structural_frame(true);
    }
    c.calibrator_mut()
        .confirm(CalibrationMethod::Manual, 0)
        .unwrap();
    c
}
pub fn frame(c: &mut CaptureController, epoch: u64, sequence: u64, at: u64) -> AdmittedFrame {
    let h = FrameHeader {
        source: CaptureSource::Dxgi,
        session_epoch: epoch,
        calibration_generation: c.calibrator().generation(),
        output_id: 7,
        sequence,
        acquired_at_ms: at,
    };
    c.admit(&h, at).unwrap()
}
