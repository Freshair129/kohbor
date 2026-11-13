//! Pure capture admission. See docs/design/S4-OBSERVATION-BOUNDARY.md section 4.1.

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_STREAM_ID: AtomicU64 = AtomicU64::new(1);

fn next_stream_id() -> u64 {
    NEXT_STREAM_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("capture stream id exhausted")
}

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

/// Valid when the output is non-zero, the rectangle is inside it, and its sides differ by at most
/// one pixel.
pub fn validate_rect(rect: &Rect, output: &OutputGeometry) -> Result<(), GeometryError> {
    if output.width == 0 || output.height == 0 {
        return Err(GeometryError::ZeroOutput);
    }
    if rect.width == 0 || rect.height == 0 {
        return Err(GeometryError::EmptyRect);
    }

    let left = i64::from(rect.x);
    let top = i64::from(rect.y);
    let right = left + i64::from(rect.width);
    let bottom = top + i64::from(rect.height);
    if left < 0 || top < 0 || right > i64::from(output.width) || bottom > i64::from(output.height) {
        return Err(GeometryError::OutOfBounds);
    }
    if rect.width.abs_diff(rect.height) > 1 {
        return Err(GeometryError::NotSquare);
    }
    Ok(())
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
struct CalibrationCandidate {
    env: Environment,
    rect: Rect,
    profile_id: Option<String>,
}

#[derive(Debug)]
pub struct Calibrator {
    phase: CalibrationPhase,
    generation: u64,
    candidate: Option<CalibrationCandidate>,
    record: Option<CalibrationRecord>,
    consecutive_passes: u32,
    consecutive_failures: u32,
    last_invalidation: Option<InvalidationReason>,
}

impl Calibrator {
    /// Starts uncalibrated at generation zero with no record.
    pub fn new() -> Self {
        Self {
            phase: CalibrationPhase::Uncalibrated,
            generation: 0,
            candidate: None,
            record: None,
            consecutive_passes: 0,
            consecutive_failures: 0,
            last_invalidation: None,
        }
    }

    pub fn phase(&self) -> CalibrationPhase {
        self.phase
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn record(&self) -> Option<&CalibrationRecord> {
        self.record.as_ref()
    }

    /// Validates before changing state, then fences older frames and starts a new candidate.
    pub fn begin(
        &mut self,
        env: Environment,
        candidate: Rect,
        profile_id: Option<String>,
    ) -> Result<(), GeometryError> {
        validate_rect(&candidate, &env.output)?;

        self.bump_generation();
        self.phase = CalibrationPhase::Calibrating;
        self.candidate = Some(CalibrationCandidate {
            env,
            rect: candidate,
            profile_id,
        });
        self.record = None;
        self.consecutive_passes = 0;
        self.consecutive_failures = 0;
        Ok(())
    }

    /// Counts consecutive structural passes during calibration and failures while validated.
    pub fn structural_frame(&mut self, passed: bool) {
        match self.phase {
            CalibrationPhase::Calibrating => {
                if passed {
                    self.consecutive_passes = self.consecutive_passes.saturating_add(1);
                } else {
                    self.consecutive_passes = 0;
                }
            }
            CalibrationPhase::Validated => {
                if passed {
                    self.consecutive_failures = 0;
                } else {
                    self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                    if self.consecutive_failures >= STRUCTURAL_FAIL_FRAMES {
                        self.invalidate(InvalidationReason::StructuralCheckFailed);
                    }
                }
            }
            CalibrationPhase::Uncalibrated | CalibrationPhase::RecalibrationNeeded => {}
        }
    }

    /// Confirms a candidate after the required consecutive structural passes.
    pub fn confirm(&mut self, method: CalibrationMethod, now_ms: u64) -> Result<(), ConfirmError> {
        if self.phase != CalibrationPhase::Calibrating {
            return Err(ConfirmError::NotCalibrating);
        }
        if self.consecutive_passes < STRUCTURAL_PASS_FRAMES {
            return Err(ConfirmError::StructuralCheckIncomplete);
        }

        let candidate = self.candidate.take().ok_or(ConfirmError::NotCalibrating)?;
        self.record = Some(CalibrationRecord {
            env: candidate.env,
            rect: candidate.rect,
            profile_id: candidate.profile_id,
            method,
            generation: self.generation,
            validated_at_ms: now_ms,
        });
        self.phase = CalibrationPhase::Validated;
        self.consecutive_failures = 0;
        Ok(())
    }

    /// Invalidates when any calibrated environment field changes, using the first difference.
    pub fn observe_environment(&mut self, env: &Environment) {
        if self.phase != CalibrationPhase::Validated {
            return;
        }

        let Some(record) = self.record.as_ref() else {
            return;
        };
        let old = &record.env;
        let reason = if old.output.output_id != env.output.output_id
            || old.output.width != env.output.width
            || old.output.height != env.output.height
        {
            Some(InvalidationReason::OutputChanged)
        } else if old.output.dpi != env.output.dpi {
            Some(InvalidationReason::DpiChanged)
        } else if old.monitor_id != env.monitor_id {
            Some(InvalidationReason::MonitorChanged)
        } else if old.window_rect != env.window_rect {
            Some(InvalidationReason::WindowRectChanged)
        } else if old.minimap_layout != env.minimap_layout {
            Some(InvalidationReason::LayoutChanged)
        } else {
            None
        };

        if let Some(reason) = reason {
            self.invalidate(reason);
        }
    }

    /// Advances the generation fence and drops calibration state for every invalidation.
    pub fn invalidate(&mut self, reason: InvalidationReason) {
        self.bump_generation();
        self.phase = CalibrationPhase::RecalibrationNeeded;
        self.candidate = None;
        self.record = None;
        self.consecutive_passes = 0;
        self.consecutive_failures = 0;
        self.last_invalidation = Some(reason);
    }

    pub fn last_invalidation(&self) -> Option<InvalidationReason> {
        self.last_invalidation
    }

    fn bump_generation(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("calibration generation exhausted");
    }
}

impl Default for Calibrator {
    fn default() -> Self {
        Self::new()
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

/// Only `CaptureController::admit` can create this validated frame token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedFrame {
    header: FrameHeader,
    stream_id: u64,
    admission_order: u64,
}

impl AdmittedFrame {
    pub fn sequence(&self) -> u64 {
        self.header.sequence
    }

    pub fn generation(&self) -> u64 {
        self.header.calibration_generation
    }

    pub fn session_epoch(&self) -> u64 {
        self.header.session_epoch
    }

    pub fn acquired_at_ms(&self) -> u64 {
        self.header.acquired_at_ms
    }

    pub fn source(&self) -> CaptureSource {
        self.header.source
    }

    /// Identifies the controller stream that issued this non-forgeable token.
    pub fn stream_id(&self) -> u64 {
        self.stream_id
    }

    /// Increases for every successful admission from this controller.
    pub fn admission_order(&self) -> u64 {
        self.admission_order
    }
}

#[derive(Debug)]
pub struct CaptureController {
    calibrator: Calibrator,
    selected_source: CaptureSource,
    session_epoch: u64,
    stream_id: u64,
    admission_order: u64,
    lite: bool,
    obs_recording: bool,
    last_healthy_cycle_ms: Option<u64>,
    last_admitted_sequence: Option<u64>,
    last_admitted_at_ms: Option<u64>,
    lost: bool,
    unsupported: bool,
}

impl CaptureController {
    /// Fixes the selected source and session for this controller. Only DXGI is admitted as a
    /// critical source in this slice; OBS recording remains independent of that selection.
    pub fn new(selected_source: CaptureSource, session_epoch: u64) -> Self {
        Self {
            calibrator: Calibrator::new(),
            selected_source,
            session_epoch,
            stream_id: next_stream_id(),
            admission_order: 0,
            lite: false,
            obs_recording: false,
            last_healthy_cycle_ms: None,
            last_admitted_sequence: None,
            last_admitted_at_ms: None,
            lost: false,
            unsupported: false,
        }
    }

    pub fn calibrator(&self) -> &Calibrator {
        &self.calibrator
    }

    pub fn calibrator_mut(&mut self) -> &mut Calibrator {
        &mut self.calibrator
    }

    pub fn selected_source(&self) -> CaptureSource {
        self.selected_source
    }

    /// Starts a new session epoch, resets sequence ordering, and keeps the monotonic time fence.
    pub fn set_session_epoch(&mut self, epoch: u64) {
        if self.session_epoch != epoch {
            self.session_epoch = epoch;
            self.last_healthy_cycle_ms = None;
            self.last_admitted_sequence = None;
        }
    }

    pub fn set_lite(&mut self, lite: bool) {
        self.lite = lite;
    }

    pub fn set_obs_recording(&mut self, on: bool) {
        self.obs_recording = on;
    }

    pub fn obs_recording(&self) -> bool {
        self.obs_recording
    }

    /// Healthy timeouts refresh capture freshness. Lost and unsupported states persist until an
    /// admitted frame recovers the stream; failed cycles never refresh freshness.
    pub fn record_cycle(&mut self, now_ms: u64, outcome: CycleOutcome) {
        match outcome {
            CycleOutcome::HealthyTimeout => self.refresh_healthy_cycle(now_ms),
            CycleOutcome::AccessLost => {
                self.last_healthy_cycle_ms = None;
                self.lost = true;
                self.calibrator.invalidate(InvalidationReason::AccessLost);
            }
            CycleOutcome::Failed => {}
            CycleOutcome::Unsupported => {
                self.last_healthy_cycle_ms = None;
                self.unsupported = true;
            }
        }
    }

    /// Checks source, session, calibration, output, generation, then timestamp and sequence order.
    pub fn admit(&mut self, header: &FrameHeader, now_ms: u64) -> Result<AdmittedFrame, Reject> {
        if header.source != self.selected_source || self.selected_source != CaptureSource::Dxgi {
            return Err(Reject::SourceNotSelected);
        }
        if header.session_epoch != self.session_epoch {
            return Err(Reject::WrongSession);
        }
        if self.calibrator.phase() != CalibrationPhase::Validated {
            return Err(Reject::NotValidated);
        }
        let expected_output = self
            .calibrator
            .record()
            .map(|record| record.env.output.output_id);
        if expected_output != Some(header.output_id) {
            return Err(Reject::WrongOutput);
        }
        if header.calibration_generation != self.calibrator.generation() {
            return Err(Reject::StaleGeneration);
        }

        if header.acquired_at_ms > now_ms
            || self
                .last_admitted_sequence
                .is_some_and(|sequence| header.sequence <= sequence)
            || self
                .last_admitted_at_ms
                .is_some_and(|acquired_at_ms| header.acquired_at_ms < acquired_at_ms)
        {
            return Err(Reject::OutOfOrder);
        }

        let admission_order = self
            .admission_order
            .checked_add(1)
            .expect("capture admission order exhausted");
        self.admission_order = admission_order;
        self.last_admitted_sequence = Some(header.sequence);
        self.last_admitted_at_ms = Some(header.acquired_at_ms);
        self.refresh_healthy_cycle(header.acquired_at_ms);
        self.lost = false;
        self.unsupported = false;
        Ok(AdmittedFrame {
            header: *header,
            stream_id: self.stream_id,
            admission_order,
        })
    }

    /// Derives status in the specified precedence order from state and the injected clock.
    pub fn phase(&self, now_ms: u64) -> CapturePhase {
        if self.unsupported || self.selected_source != CaptureSource::Dxgi {
            return CapturePhase::Unsupported;
        }
        if self.lost {
            return CapturePhase::Lost;
        }
        match self.calibrator.phase() {
            CalibrationPhase::Uncalibrated | CalibrationPhase::Calibrating => {
                return CapturePhase::Calibrating;
            }
            CalibrationPhase::RecalibrationNeeded => return CapturePhase::RecalibrationNeeded,
            CalibrationPhase::Validated => {}
        }

        let Some(last_healthy_ms) = self.last_healthy_cycle_ms else {
            return CapturePhase::CoverageUnavailable;
        };
        let age = now_ms.saturating_sub(last_healthy_ms);
        if age > COVERAGE_MAX_MS {
            CapturePhase::CoverageUnavailable
        } else if age > FRESH_MAX_MS {
            CapturePhase::Stale
        } else if self.lite {
            CapturePhase::Lite
        } else {
            CapturePhase::Live
        }
    }

    /// Reports valid coverage for live/lite, stale coverage for stale, and unavailable otherwise.
    pub fn coverage(&self, now_ms: u64) -> Coverage {
        match self.phase(now_ms) {
            CapturePhase::Live | CapturePhase::Lite => Coverage::Valid,
            CapturePhase::Stale => Coverage::Stale,
            CapturePhase::CoverageUnavailable
            | CapturePhase::Calibrating
            | CapturePhase::RecalibrationNeeded
            | CapturePhase::Lost
            | CapturePhase::Unsupported => Coverage::Unavailable,
        }
    }

    pub fn crop_visible(&self) -> bool {
        self.calibrator.phase() == CalibrationPhase::Validated
    }

    fn refresh_healthy_cycle(&mut self, at_ms: u64) {
        self.last_healthy_cycle_ms = Some(
            self.last_healthy_cycle_ms
                .map_or(at_ms, |previous_ms| previous_ms.max(at_ms)),
        );
    }
}
