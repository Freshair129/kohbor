use kohbor_capture::*;

fn env() -> Environment {
    Environment {
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
    }
}
fn rect() -> Rect {
    Rect {
        x: 10,
        y: 10,
        width: 20,
        height: 20,
    }
}
fn begin(c: &mut Calibrator) {
    c.begin(env(), rect(), Some("profile".into())).unwrap();
}
fn validate(c: &mut Calibrator) {
    begin(c);
    for _ in 0..30 {
        c.structural_frame(true);
    }
    c.confirm(CalibrationMethod::Manual, 50).unwrap();
}
fn controller() -> CaptureController {
    let mut c = CaptureController::new(CaptureSource::Dxgi, 3);
    validate(c.calibrator_mut());
    c
}
fn header(c: &CaptureController, seq: u64, at: u64) -> FrameHeader {
    FrameHeader {
        source: CaptureSource::Dxgi,
        session_epoch: 3,
        calibration_generation: c.calibrator().generation(),
        output_id: 7,
        sequence: seq,
        acquired_at_ms: at,
    }
}
macro_rules! geometry {
    ($name:ident, $r:expr, $o:expr, $result:expr) => {
        #[test]
        fn $name() {
            assert_eq!(validate_rect(&$r, &$o), $result);
        }
    };
}
geometry!(
    vision_001_geometry_empty_width,
    Rect { width: 0, ..rect() },
    env().output,
    Err(GeometryError::EmptyRect)
);
geometry!(
    vision_001_geometry_empty_height,
    Rect {
        height: 0,
        ..rect()
    },
    env().output,
    Err(GeometryError::EmptyRect)
);
geometry!(
    vision_001_geometry_left,
    Rect { x: -1, ..rect() },
    env().output,
    Err(GeometryError::OutOfBounds)
);
geometry!(
    vision_001_geometry_top,
    Rect { y: -1, ..rect() },
    env().output,
    Err(GeometryError::OutOfBounds)
);
geometry!(
    vision_001_geometry_right,
    Rect { x: 81, ..rect() },
    env().output,
    Err(GeometryError::OutOfBounds)
);
geometry!(
    vision_001_geometry_bottom,
    Rect { y: 81, ..rect() },
    env().output,
    Err(GeometryError::OutOfBounds)
);
geometry!(
    vision_001_geometry_exact_edges,
    Rect {
        x: 80,
        y: 80,
        ..rect()
    },
    env().output,
    Ok(())
);
geometry!(
    vision_001_geometry_one_pixel_difference,
    Rect {
        width: 21,
        ..rect()
    },
    env().output,
    Ok(())
);
geometry!(
    vision_001_geometry_two_pixel_difference,
    Rect {
        width: 22,
        ..rect()
    },
    env().output,
    Err(GeometryError::NotSquare)
);
geometry!(
    vision_001_geometry_zero_output_width,
    rect(),
    OutputGeometry {
        width: 0,
        ..env().output
    },
    Err(GeometryError::ZeroOutput)
);
geometry!(
    vision_001_geometry_zero_output_height,
    rect(),
    OutputGeometry {
        height: 0,
        ..env().output
    },
    Err(GeometryError::ZeroOutput)
);
#[test]
fn vision_001_calibration_begin_bumps_generation() {
    let mut c = Calibrator::new();
    assert_eq!(c.generation(), 0);
    begin(&mut c);
    assert_eq!(c.generation(), 1);
    assert_eq!(c.phase(), CalibrationPhase::Calibrating);
}
#[test]
fn vision_001_calibration_29_refused() {
    let mut c = Calibrator::new();
    begin(&mut c);
    for _ in 0..29 {
        c.structural_frame(true);
    }
    assert_eq!(
        c.confirm(CalibrationMethod::Manual, 50),
        Err(ConfirmError::StructuralCheckIncomplete)
    );
}
#[test]
fn vision_001_calibration_30_accepted() {
    let mut c = Calibrator::new();
    validate(&mut c);
    assert_eq!(c.phase(), CalibrationPhase::Validated);
}
#[test]
fn vision_001_calibration_failed_pass_resets_count() {
    let mut c = Calibrator::new();
    begin(&mut c);
    for _ in 0..29 {
        c.structural_frame(true);
    }
    c.structural_frame(false);
    for _ in 0..29 {
        c.structural_frame(true);
    }
    assert_eq!(
        c.confirm(CalibrationMethod::Manual, 0),
        Err(ConfirmError::StructuralCheckIncomplete)
    );
    c.structural_frame(true);
    assert_eq!(c.confirm(CalibrationMethod::Manual, 0), Ok(()));
}
#[test]
fn vision_001_calibration_record_contents() {
    let mut c = Calibrator::new();
    validate(&mut c);
    assert_eq!(
        c.record(),
        Some(&CalibrationRecord {
            env: env(),
            rect: rect(),
            profile_id: Some("profile".into()),
            method: CalibrationMethod::Manual,
            generation: 1,
            validated_at_ms: 50
        })
    );
}
macro_rules! env_change {
    ($name:ident, $edit:expr, $reason:expr) => {
        #[test]
        fn $name() {
            let mut c = Calibrator::new();
            validate(&mut c);
            let old = c.generation();
            let mut e = env();
            ($edit)(&mut e);
            c.observe_environment(&e);
            assert_eq!(c.generation(), old + 1);
            assert_eq!(c.phase(), CalibrationPhase::RecalibrationNeeded);
            assert_eq!(c.last_invalidation(), Some($reason));
            assert!(c.record().is_none());
        }
    };
}
env_change!(
    vision_001_calibration_output_change,
    |e: &mut Environment| e.output.width += 1,
    InvalidationReason::OutputChanged
);
env_change!(
    vision_001_calibration_dpi_change,
    |e: &mut Environment| e.output.dpi += 1,
    InvalidationReason::DpiChanged
);
env_change!(
    vision_001_calibration_monitor_change,
    |e: &mut Environment| e.monitor_id += 1,
    InvalidationReason::MonitorChanged
);
env_change!(
    vision_001_calibration_window_change,
    |e: &mut Environment| e.window_rect.x += 1,
    InvalidationReason::WindowRectChanged
);
env_change!(
    vision_001_calibration_layout_change,
    |e: &mut Environment| e.minimap_layout = "right".into(),
    InvalidationReason::LayoutChanged
);
macro_rules! invalidate {
    ($name:ident, $reason:expr) => {
        #[test]
        fn $name() {
            let mut c = Calibrator::new();
            validate(&mut c);
            let g = c.generation();
            c.invalidate($reason);
            assert_eq!(c.generation(), g + 1);
            assert_eq!(c.phase(), CalibrationPhase::RecalibrationNeeded);
            assert_eq!(c.last_invalidation(), Some($reason));
            assert!(c.record().is_none());
        }
    };
}
invalidate!(
    vision_001_calibration_access_lost,
    InvalidationReason::AccessLost
);
invalidate!(
    vision_001_calibration_user_action,
    InvalidationReason::UserAction
);
#[test]
fn vision_001_calibration_59_failures_retained() {
    let mut c = Calibrator::new();
    validate(&mut c);
    for _ in 0..59 {
        c.structural_frame(false);
    }
    assert_eq!(c.phase(), CalibrationPhase::Validated);
}
#[test]
fn vision_001_calibration_60_failures_invalidates() {
    let mut c = Calibrator::new();
    validate(&mut c);
    let g = c.generation();
    for _ in 0..60 {
        c.structural_frame(false);
    }
    assert_eq!(c.generation(), g + 1);
    assert_eq!(c.phase(), CalibrationPhase::RecalibrationNeeded);
    assert_eq!(
        c.last_invalidation(),
        Some(InvalidationReason::StructuralCheckFailed)
    );
}
#[test]
fn vision_001_calibration_crop_only_validated() {
    let mut c = CaptureController::new(CaptureSource::Dxgi, 3);
    assert!(!c.crop_visible());
    begin(c.calibrator_mut());
    assert!(!c.crop_visible());
    for _ in 0..30 {
        c.calibrator_mut().structural_frame(true);
    }
    c.calibrator_mut()
        .confirm(CalibrationMethod::Manual, 0)
        .unwrap();
    assert!(c.crop_visible());
    c.calibrator_mut()
        .invalidate(InvalidationReason::UserAction);
    assert!(!c.crop_visible());
}
macro_rules! freshness {
    ($name:ident, $age:expr, $phase:expr, $coverage:expr) => {
        #[test]
        fn $name() {
            let mut c = controller();
            c.record_cycle(100, CycleOutcome::HealthyTimeout);
            assert_eq!(c.phase(100 + $age), $phase);
            assert_eq!(c.coverage(100 + $age), $coverage);
        }
    };
}
freshness!(
    vision_001_freshness_age_zero,
    0,
    CapturePhase::Live,
    Coverage::Valid
);
freshness!(
    vision_001_freshness_age_2000,
    2000,
    CapturePhase::Live,
    Coverage::Valid
);
freshness!(
    vision_001_freshness_age_2001,
    2001,
    CapturePhase::Stale,
    Coverage::Stale
);
freshness!(
    vision_002_freshness_age_5000,
    5000,
    CapturePhase::Stale,
    Coverage::Stale
);
freshness!(
    vision_002_freshness_age_5001,
    5001,
    CapturePhase::CoverageUnavailable,
    Coverage::Unavailable
);
#[test]
fn vision_001_freshness_timeout_refreshes_paused_game() {
    let mut c = controller();
    c.record_cycle(0, CycleOutcome::HealthyTimeout);
    c.record_cycle(5000, CycleOutcome::HealthyTimeout);
    assert_eq!(c.phase(5000), CapturePhase::Live);
}
#[test]
fn vision_001_freshness_failed_does_not_refresh() {
    let mut c = controller();
    c.record_cycle(0, CycleOutcome::HealthyTimeout);
    c.record_cycle(5000, CycleOutcome::Failed);
    assert_eq!(c.phase(5001), CapturePhase::CoverageUnavailable);
}
#[test]
fn vision_001_freshness_access_lost_overrides_and_invalidates() {
    let mut c = controller();
    c.record_cycle(0, CycleOutcome::HealthyTimeout);
    c.record_cycle(5000, CycleOutcome::AccessLost);
    assert_eq!(c.phase(5000), CapturePhase::Lost);
    assert_eq!(c.coverage(5000), Coverage::Unavailable);
    assert_eq!(
        c.calibrator().last_invalidation(),
        Some(InvalidationReason::AccessLost)
    );
}
#[test]
fn vision_001_freshness_lite() {
    let mut c = controller();
    c.record_cycle(0, CycleOutcome::HealthyTimeout);
    c.set_lite(true);
    assert_eq!(c.phase(0), CapturePhase::Lite);
    assert_eq!(c.coverage(0), Coverage::Valid);
}
#[test]
fn vision_001_freshness_unsupported_precedes_calibrating() {
    let mut c = CaptureController::new(CaptureSource::Dxgi, 3);
    c.record_cycle(0, CycleOutcome::Unsupported);
    assert_eq!(c.phase(0), CapturePhase::Unsupported);
}
#[test]
fn vision_001_freshness_lost_precedes_recalibration() {
    let mut c = controller();
    c.record_cycle(0, CycleOutcome::AccessLost);
    assert_eq!(c.phase(0), CapturePhase::Lost);
}
#[test]
fn vision_001_freshness_calibrating() {
    let mut c = CaptureController::new(CaptureSource::Dxgi, 3);
    begin(c.calibrator_mut());
    assert_eq!(c.phase(0), CapturePhase::Calibrating);
}
#[test]
fn vision_001_freshness_new_frame_recovers_unsupported() {
    let mut c = controller();
    c.record_cycle(0, CycleOutcome::Unsupported);
    let h = header(&c, 1, 10);
    c.admit(&h, 10).unwrap();
    assert_eq!(c.phase(10), CapturePhase::Live);
}
#[test]
fn vision_001_freshness_access_lost_does_not_refresh_after_recalibration() {
    let mut c = controller();
    c.record_cycle(0, CycleOutcome::HealthyTimeout);
    c.record_cycle(4000, CycleOutcome::AccessLost);
    validate(c.calibrator_mut());
    let h = header(&c, 1, 0);
    c.admit(&h, 5001).unwrap();
    assert_eq!(c.phase(5001), CapturePhase::CoverageUnavailable);
}
macro_rules! rejection {
    ($name:ident, $change:expr, $error:expr) => {
        #[test]
        fn $name() {
            let mut c = controller();
            let mut h = header(&c, 1, 100);
            ($change)(&mut h);
            assert_eq!(c.admit(&h, 100), Err($error));
        }
    };
}
rejection!(
    vision_001_admission_wrong_session,
    |h: &mut FrameHeader| h.session_epoch += 1,
    Reject::WrongSession
);
rejection!(
    vision_001_admission_wrong_output,
    |h: &mut FrameHeader| h.output_id += 1,
    Reject::WrongOutput
);
rejection!(
    vision_001_admission_old_generation,
    |h: &mut FrameHeader| h.calibration_generation -= 1,
    Reject::StaleGeneration
);
rejection!(
    vision_001_admission_future_time,
    |h: &mut FrameHeader| h.acquired_at_ms += 1,
    Reject::OutOfOrder
);
#[test]
fn vision_001_admission_not_validated() {
    let mut c = CaptureController::new(CaptureSource::Dxgi, 3);
    let h = header(&c, 1, 0);
    assert_eq!(c.admit(&h, 0), Err(Reject::NotValidated));
}
#[test]
fn vision_001_admission_old_after_invalidation_and_revalidation() {
    let mut c = controller();
    let h = header(&c, 1, 0);
    c.calibrator_mut()
        .invalidate(InvalidationReason::UserAction);
    validate(c.calibrator_mut());
    assert_eq!(c.admit(&h, 0), Err(Reject::StaleGeneration));
}
#[test]
fn vision_001_admission_duplicate_sequence() {
    let mut c = controller();
    let h = header(&c, 1, 10);
    c.admit(&h, 10).unwrap();
    assert_eq!(c.admit(&h, 10), Err(Reject::OutOfOrder));
}
#[test]
fn vision_001_admission_lower_sequence() {
    let mut c = controller();
    let h = header(&c, 2, 10);
    c.admit(&h, 10).unwrap();
    let h = header(&c, 1, 11);
    assert_eq!(c.admit(&h, 11), Err(Reject::OutOfOrder));
}
#[test]
fn vision_001_admission_sequence_exposed() {
    let mut c = controller();
    let h = header(&c, 4, 10);
    assert_eq!(c.admit(&h, 10).unwrap().sequence(), 4);
}
#[test]
fn vision_001_admission_generation_exposed() {
    let mut c = controller();
    let h = header(&c, 4, 10);
    assert_eq!(
        c.admit(&h, 10).unwrap().generation(),
        h.calibration_generation
    );
}
#[test]
fn vision_001_admission_session_exposed() {
    let mut c = controller();
    let h = header(&c, 4, 10);
    assert_eq!(c.admit(&h, 10).unwrap().session_epoch(), 3);
}
#[test]
fn vision_001_admission_time_exposed() {
    let mut c = controller();
    let h = header(&c, 4, 10);
    assert_eq!(c.admit(&h, 10).unwrap().acquired_at_ms(), 10);
}
#[test]
fn vision_001_admission_rejection_does_not_refresh() {
    let mut c = controller();
    c.record_cycle(0, CycleOutcome::HealthyTimeout);
    let mut h = header(&c, 1, 5000);
    h.output_id += 1;
    assert_eq!(c.admit(&h, 5000), Err(Reject::WrongOutput));
    assert_eq!(c.phase(5001), CapturePhase::CoverageUnavailable);
}
#[test]
fn vision_001_admission_new_session_rejects_old() {
    let mut c = controller();
    let h = header(&c, 1, 0);
    c.set_session_epoch(4);
    assert_eq!(c.admit(&h, 0), Err(Reject::WrongSession));
}
#[test]
fn vision_004_source_selected_dxgi() {
    assert_eq!(controller().selected_source(), CaptureSource::Dxgi);
}
rejection!(
    vision_004_source_obs_rejected,
    |h: &mut FrameHeader| h.source = CaptureSource::Obs,
    Reject::SourceNotSelected
);
rejection!(
    vision_004_source_wgc_rejected,
    |h: &mut FrameHeader| h.source = CaptureSource::Wgc,
    Reject::SourceNotSelected
);
#[test]
fn vision_004_source_recording_on_keeps_dxgi() {
    let mut c = controller();
    c.set_obs_recording(true);
    assert!(c.obs_recording());
    assert_eq!(c.selected_source(), CaptureSource::Dxgi);
}
#[test]
fn vision_004_source_recording_off_keeps_dxgi() {
    let mut c = controller();
    c.set_obs_recording(true);
    c.set_obs_recording(false);
    assert!(!c.obs_recording());
    assert_eq!(c.selected_source(), CaptureSource::Dxgi);
}
#[test]
fn vision_004_source_dxgi_admitted_recording_on() {
    let mut c = controller();
    c.set_obs_recording(true);
    let h = header(&c, 1, 0);
    assert_eq!(c.admit(&h, 0).unwrap().source(), CaptureSource::Dxgi);
}
#[test]
fn vision_004_source_dxgi_admitted_recording_off() {
    let mut c = controller();
    c.set_obs_recording(true);
    c.set_obs_recording(false);
    let h = header(&c, 1, 0);
    assert_eq!(c.admit(&h, 0).unwrap().source(), CaptureSource::Dxgi);
}
#[test]
fn vision_004_source_obs_rejected_with_recording_on() {
    let mut c = controller();
    c.set_obs_recording(true);
    let mut h = header(&c, 1, 0);
    h.source = CaptureSource::Obs;
    assert_eq!(c.admit(&h, 0), Err(Reject::SourceNotSelected));
}
