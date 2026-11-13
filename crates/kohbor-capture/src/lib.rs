//! Pure capture admission. See docs/design/S4-OBSERVATION-BOUNDARY.md section 4.1.
//! STUB: signatures are the contract for the S4-T1 tests; bodies are `todo!()` until S4-L1.
#![allow(unused_variables, dead_code)]

pub const FRESH_MAX_MS: u64 = 2000;
pub const COVERAGE_MAX_MS: u64 = 5000;
pub const STRUCTURAL_PASS_FRAMES: u32 = 30;
pub const STRUCTURAL_FAIL_FRAMES: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputGeometry {
    pub output_id: u64,
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub output: OutputGeometry,
    pub monitor_id: u64,
    pub window_rect: Rect,
    pub minimap_layout: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryError {
    ZeroOutput,
    EmptyRect,
    OutOfBounds,
    NotSquare,
}

/// Valid: output non-zero, rect non-empty, fully inside [0,width]x[0,height], |width-height| <= 1.
pub fn validate_rect(rect: &Rect, output: &OutputGeometry) -> Result<(), GeometryError> {
    todo!()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationMethod {
    Profile,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationPhase {
    Uncalibrated,
    Calibrating,
    Validated,
    RecalibrationNeeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidationReason {
    OutputChanged,
    DpiChanged,
    MonitorChanged,
    WindowRectChanged,
    LayoutChanged,
    AccessLost,
    StructuralCheckFailed,
    UserAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmError {
    NotCalibrating,
    StructuralCheckIncomplete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibrationRecord {
    pub env: Environment,
    pub rect: Rect,
    pub profile_id: Option<String>,
    pub method: CalibrationMethod,
    pub generation: u64,
    pub validated_at_ms: u64,
}

#[derive(Debug)]
pub struct Calibrator {
    _private: (),
}

impl Calibrator {
    /// Uncalibrated, generation 0, no record.
    pub fn new() -> Self {
        todo!()
    }
    pub fn phase(&self) -> CalibrationPhase {
        todo!()
    }
    pub fn generation(&self) -> u64 {
        todo!()
    }
    pub fn record(&self) -> Option<&CalibrationRecord> {
        todo!()
    }
    /// Validates the rect against env.output, bumps generation, enters Calibrating, clears the
    /// structural count and any old record. On a geometry error nothing changes.
    pub fn begin(
        &mut self,
        env: Environment,
        candidate: Rect,
        profile_id: Option<String>,
    ) -> Result<(), GeometryError> {
        todo!()
    }
    /// While Calibrating: counts consecutive passes (a failure resets to 0). While Validated: 60
    /// consecutive failures invalidate with StructuralCheckFailed (a pass resets the failure count).
    /// Other phases: ignored.
    pub fn structural_frame(&mut self, passed: bool) {
        todo!()
    }
    /// Needs Calibrating and >= 30 consecutive passes. Creates the record (generation = current),
    /// enters Validated.
    pub fn confirm(&mut self, method: CalibrationMethod, now_ms: u64) -> Result<(), ConfirmError> {
        todo!()
    }
    /// While Validated: any difference from record.env invalidates (OutputChanged for
    /// width/height/output_id, DpiChanged, MonitorChanged, WindowRectChanged, LayoutChanged;
    /// checked in that order, first difference wins). Other phases: ignored.
    pub fn observe_environment(&mut self, env: &Environment) {
        todo!()
    }
    /// Bumps generation, drops the record, enters RecalibrationNeeded. Ignored when Uncalibrated.
    pub fn invalidate(&mut self, reason: InvalidationReason) {
        todo!()
    }
    pub fn last_invalidation(&self) -> Option<InvalidationReason> {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSource {
    Dxgi,
    Wgc,
    Obs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleOutcome {
    HealthyTimeout,
    AccessLost,
    Failed,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturePhase {
    Live,
    Lite,
    Stale,
    CoverageUnavailable,
    Calibrating,
    RecalibrationNeeded,
    Lost,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    Valid,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub source: CaptureSource,
    pub session_epoch: u64,
    pub calibration_generation: u64,
    pub output_id: u64,
    pub sequence: u64,
    pub acquired_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    NotValidated,
    StaleGeneration,
    WrongSession,
    WrongOutput,
    SourceNotSelected,
    OutOfOrder,
}

/// Unforgeable: private fields, no public constructor. Only `CaptureController::admit` returns one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedFrame {
    _private: (),
}

impl AdmittedFrame {
    pub fn sequence(&self) -> u64 {
        todo!()
    }
    pub fn generation(&self) -> u64 {
        todo!()
    }
    pub fn session_epoch(&self) -> u64 {
        todo!()
    }
    pub fn acquired_at_ms(&self) -> u64 {
        todo!()
    }
    pub fn source(&self) -> CaptureSource {
        todo!()
    }
}

#[derive(Debug)]
pub struct CaptureController {
    _private: (),
}

impl CaptureController {
    /// Selected critical source is fixed at construction (no setter in this slice). Not lite,
    /// OBS recording off, no cycle yet.
    pub fn new(selected_source: CaptureSource, session_epoch: u64) -> Self {
        todo!()
    }
    pub fn calibrator(&self) -> &Calibrator {
        todo!()
    }
    pub fn calibrator_mut(&mut self) -> &mut Calibrator {
        todo!()
    }
    pub fn selected_source(&self) -> CaptureSource {
        todo!()
    }
    /// A new epoch makes frames of the old epoch WrongSession.
    pub fn set_session_epoch(&mut self, epoch: u64) {
        todo!()
    }
    pub fn set_lite(&mut self, lite: bool) {
        todo!()
    }
    pub fn set_obs_recording(&mut self, on: bool) {
        todo!()
    }
    pub fn obs_recording(&self) -> bool {
        todo!()
    }
    /// HealthyTimeout refreshes the healthy clock at now_ms. AccessLost => phase Lost until the
    /// next admitted frame, and the calibrator is invalidated with AccessLost. Failed never
    /// refreshes. Unsupported => phase Unsupported until the next admitted frame.
    pub fn record_cycle(&mut self, now_ms: u64, outcome: CycleOutcome) {
        todo!()
    }
    /// Check order: source, session, output, validated, generation, ordering (acquired_at_ms <=
    /// now_ms, and sequence greater than the previous admitted sequence). Success refreshes the
    /// healthy clock at header.acquired_at_ms and clears Lost/Unsupported. A rejected frame
    /// changes nothing.
    pub fn admit(&mut self, header: &FrameHeader, now_ms: u64) -> Result<AdmittedFrame, Reject> {
        todo!()
    }
    /// Precedence: Unsupported, Lost, Calibrating (also Uncalibrated), RecalibrationNeeded, then
    /// by age of the healthy clock: age > 5000 => CoverageUnavailable (also when no healthy cycle
    /// was ever recorded), age > 2000 => Stale, else Lite if lite else Live.
    pub fn phase(&self, now_ms: u64) -> CapturePhase {
        todo!()
    }
    /// Valid for Live/Lite, Stale for Stale, Unavailable for everything else.
    pub fn coverage(&self, now_ms: u64) -> Coverage {
        todo!()
    }
    /// True exactly when the calibrator is Validated.
    pub fn crop_visible(&self) -> bool {
        todo!()
    }
}
