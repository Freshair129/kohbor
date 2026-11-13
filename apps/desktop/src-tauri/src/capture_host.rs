//! MVP-1 W3 host capture loop, frame transport, and observation/risk event bridge.

use kohbor_capture::{
    AdmittedFrame, CalibrationMethod, CalibrationPhase, CaptureController, CapturePhase,
    CaptureSource, Coverage, CycleOutcome, Environment, FrameHeader, InvalidationReason, Rect,
    Reject,
};
use kohbor_capture_native::{Grab, MinimapGrabber, OpenError, OutputInfo};
use kohbor_minimap_cv::{PixelReport, Team};
use kohbor_risk::{Meter, OwnGsi, RiskEngine, RiskEvent, RiskOutput, TickInput};
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tauri::ipc::{Channel, InvokeResponseBody};

const FRAME_HEADER_LEN: usize = 32;
const FRAME_MAGIC: u32 = 0x4B4D_4150;
const FRAME_VERSION: u16 = 1;
const DISPLAY_INTERVAL: Duration = Duration::from_nanos(33_333_334);
const ANALYSIS_INTERVAL: Duration = Duration::from_millis(100);
const STATUS_INTERVAL: Duration = Duration::from_millis(500);
const RESOURCE_INTERVAL: Duration = Duration::from_secs(1);
const CAPTURE_TIMEOUT_MS: u32 = 15;
const RETRY_INITIAL: Duration = Duration::from_millis(500);
const RETRY_MAX: Duration = Duration::from_secs(5);
const MAX_SEEN_GSI_EVENT_KEYS: usize = 256;
const RISK_HEARTBEAT_MS: u64 = 1000;

static CAPTURE_STATUS: OnceLock<Mutex<Option<Value>>> = OnceLock::new();
static FRAME_SUBSCRIBERS: OnceLock<Mutex<Vec<Channel<InvokeResponseBody>>>> = OnceLock::new();
static FRAME_INVALIDATE_GATE: Mutex<()> = Mutex::new(());
static USER_INVALIDATE: AtomicBool = AtomicBool::new(false);
static HOST_STARTED: AtomicBool = AtomicBool::new(false);

fn capture_status_slot() -> &'static Mutex<Option<Value>> {
    CAPTURE_STATUS.get_or_init(|| Mutex::new(None))
}

fn frame_subscribers() -> &'static Mutex<Vec<Channel<InvokeResponseBody>>> {
    FRAME_SUBSCRIBERS.get_or_init(|| Mutex::new(Vec::new()))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn unknown_capture_status() -> Value {
    json!({
        "phase": "unknown",
        "fps_measured": null,
        "frame_age_ms": null,
        "rect_profile": null,
        "calibration_generation": null,
        "auto_confirmed": true,
    })
}

fn status_payload(
    phase: &str,
    fps_measured: Option<u32>,
    frame_age_ms: Option<u64>,
    rect_profile: Option<&str>,
    calibration_generation: Option<u64>,
) -> Value {
    json!({
        "phase": phase,
        "fps_measured": fps_measured,
        "frame_age_ms": frame_age_ms,
        "rect_profile": rect_profile,
        "calibration_generation": calibration_generation,
        "auto_confirmed": true,
    })
}

fn capture_phase_label(phase: CapturePhase) -> &'static str {
    match phase {
        CapturePhase::Live => "live",
        CapturePhase::Lite | CapturePhase::Unsupported => "unsupported",
        CapturePhase::Stale => "stale",
        CapturePhase::CoverageUnavailable => "coverage_unavailable",
        CapturePhase::Calibrating | CapturePhase::RecalibrationNeeded => "calibrating",
        CapturePhase::Lost => "lost",
    }
}

fn resource_payload(
    sample: Option<ResourceSample>,
    gpu_local_bytes: Option<u64>,
    capture_fps: Option<u32>,
    frame_age_ms: Option<u64>,
) -> Value {
    let cpu_percent = sample
        .and_then(|sample| sample.cpu_percent)
        .filter(|value| value.is_finite());
    let working_set_bytes = sample.and_then(|sample| sample.working_set_bytes);
    json!({
        "working_set_bytes": working_set_bytes,
        "cpu_percent": cpu_percent,
        "gpu_local_bytes": gpu_local_bytes,
        "gpu_group_bytes": null,
        "gpu_util": null,
        "capture_fps": capture_fps,
        "frame_age_ms": frame_age_ms,
    })
}

fn risk_payload(output: &RiskOutput) -> Value {
    let meter = match output.meter {
        Meter::Unknown => json!("unknown"),
        Meter::Level(level) => json!(level),
    };
    let mut sources = Vec::with_capacity(2);
    if output.gsi_source {
        sources.push("gsi");
    }
    if output.vision_source {
        sources.push("vision");
    }
    let events = output
        .events
        .iter()
        .map(|event| match event {
            RiskEvent::EnemyMissing => "enemy_missing",
            RiskEvent::RiskAlert => "risk_alert",
            RiskEvent::RiskRevision => "risk_revision",
            RiskEvent::RiskUnavailable => "risk_unavailable",
        })
        .collect::<Vec<_>>();
    json!({
        "meter": meter,
        "sources": sources,
        "visible": output.visible,
        "baseline": output.baseline,
        "events": events,
    })
}

fn enemy_team(own_team: Option<&str>) -> Option<Team> {
    match own_team {
        Some("radiant") => Some(Team::Dire),
        Some("dire") => Some(Team::Radiant),
        _ => None,
    }
}

fn fresh_enemy_team(snapshot: Option<&GsiSnapshot>) -> Option<Team> {
    let snapshot = snapshot?;
    if snapshot.age_ms.is_none_or(|age_ms| age_ms > 2000) {
        return None;
    }
    enemy_team(snapshot.team.as_deref())
}

fn visible_count(
    report: Option<&PixelReport>,
    enemy: Option<Team>,
    coverage: Coverage,
) -> Option<u8> {
    let report = report?;
    if !report.structural_ok || coverage != Coverage::Valid || enemy.is_none() {
        return None;
    }
    u8::try_from(report.enemy_blobs.len()).ok()
}

fn own_gsi(snapshot: Option<&GsiSnapshot>) -> Option<OwnGsi> {
    let snapshot = snapshot?;
    Some(OwnGsi {
        alive: snapshot.alive?,
        hp_pct: snapshot.hp_pct,
        in_match: snapshot.in_match,
        age_ms: snapshot.age_ms?,
    })
}

trait FrameSource {
    fn open(&mut self) -> Result<(), OpenError>;
    fn output(&self) -> Option<OutputInfo>;
    fn profile_rect(&self, output: &OutputInfo) -> Option<(Rect, String)>;
    fn game_window(&self) -> Option<Rect>;
    fn grab(&mut self, rect: Rect, timeout_ms: u32, out: &mut Vec<u8>) -> Grab;
    fn gpu_local_bytes(&self) -> Option<u64>;
    fn close(&mut self);
}

struct NativeFrameSource {
    grabber: Option<MinimapGrabber>,
}

impl NativeFrameSource {
    fn new() -> Self {
        Self { grabber: None }
    }
}

impl FrameSource for NativeFrameSource {
    fn open(&mut self) -> Result<(), OpenError> {
        self.grabber = Some(MinimapGrabber::open()?);
        Ok(())
    }

    fn output(&self) -> Option<OutputInfo> {
        self.grabber.as_ref().map(MinimapGrabber::output)
    }

    fn profile_rect(&self, output: &OutputInfo) -> Option<(Rect, String)> {
        kohbor_capture_native::profile_rect(output)
            .map(|(rect, profile)| (rect, profile.to_owned()))
    }

    fn game_window(&self) -> Option<Rect> {
        kohbor_capture_native::game_window()
    }

    fn grab(&mut self, rect: Rect, timeout_ms: u32, out: &mut Vec<u8>) -> Grab {
        match self.grabber.as_mut() {
            Some(grabber) => grabber.grab(rect, timeout_ms, out),
            None => Grab::Failed("grabber_not_open".to_owned()),
        }
    }

    fn gpu_local_bytes(&self) -> Option<u64> {
        self.grabber
            .as_ref()
            .and_then(MinimapGrabber::gpu_local_bytes)
    }

    fn close(&mut self) {
        self.grabber = None;
    }
}

trait CaptureAccess: Sized {
    fn new(session_epoch: u64) -> Self;
    fn set_session_epoch(&mut self, epoch: u64);
    fn begin(&mut self, environment: Environment, rect: Rect, profile: String) -> bool;
    fn observe_environment(&mut self, environment: &Environment);
    fn structural_frame(&mut self, passed: bool);
    fn confirm_profile(&mut self, now_ms: u64) -> bool;
    fn generation(&self) -> Option<u64>;
    fn admit(&mut self, header: &FrameHeader, now_ms: u64) -> Result<(), Reject>;
    fn record_cycle(&mut self, now_ms: u64, outcome: CycleOutcome);
    fn phase(&self, now_ms: u64) -> CapturePhase;
    fn coverage(&self, now_ms: u64) -> Coverage;
    fn crop_visible(&self) -> bool;
    fn invalidate_user(&mut self, now_ms: u64);
    fn calibration_phase(&self) -> CalibrationPhase;
}

struct CaptureControllerAccess(CaptureController);

impl CaptureAccess for CaptureControllerAccess {
    fn new(session_epoch: u64) -> Self {
        Self(CaptureController::new(CaptureSource::Dxgi, session_epoch))
    }

    fn set_session_epoch(&mut self, epoch: u64) {
        self.0.set_session_epoch(epoch);
    }

    fn begin(&mut self, environment: Environment, rect: Rect, profile: String) -> bool {
        self.0
            .calibrator_mut()
            .begin(environment, rect, Some(profile))
            .is_ok()
    }

    fn observe_environment(&mut self, environment: &Environment) {
        self.0.calibrator_mut().observe_environment(environment);
    }

    fn structural_frame(&mut self, passed: bool) {
        self.0.calibrator_mut().structural_frame(passed);
    }

    fn confirm_profile(&mut self, now_ms: u64) -> bool {
        self.0
            .calibrator_mut()
            .confirm(CalibrationMethod::Profile, now_ms)
            .is_ok()
    }

    fn generation(&self) -> Option<u64> {
        let calibrator = self.0.calibrator();
        (calibrator.phase() == CalibrationPhase::Validated).then_some(calibrator.generation())
    }

    fn admit(&mut self, header: &FrameHeader, now_ms: u64) -> Result<(), Reject> {
        self.0
            .admit(header, now_ms)
            .map(|_admitted: AdmittedFrame| ())
    }

    fn record_cycle(&mut self, now_ms: u64, outcome: CycleOutcome) {
        self.0.record_cycle(now_ms, outcome);
    }

    fn phase(&self, now_ms: u64) -> CapturePhase {
        self.0.phase(now_ms)
    }

    fn coverage(&self, now_ms: u64) -> Coverage {
        self.0.coverage(now_ms)
    }

    fn crop_visible(&self) -> bool {
        self.0.crop_visible()
    }

    fn invalidate_user(&mut self, now_ms: u64) {
        self.0
            .calibrator_mut()
            .invalidate(InvalidationReason::UserAction);
        self.0.record_cycle(now_ms, CycleOutcome::Unsupported);
    }

    fn calibration_phase(&self) -> CalibrationPhase {
        self.0.calibrator().phase()
    }
}

trait FrameAnalyzer {
    fn analyze(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        enemy: Option<Team>,
    ) -> PixelReport;
}

struct MinimapAnalyzer;

impl FrameAnalyzer for MinimapAnalyzer {
    fn analyze(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        enemy: Option<Team>,
    ) -> PixelReport {
        match enemy {
            Some(enemy) => kohbor_minimap_cv::analyze(bgra, width, height, stride, enemy),
            None => {
                let radiant =
                    kohbor_minimap_cv::analyze(bgra, width, height, stride, Team::Radiant);
                let dire = kohbor_minimap_cv::analyze(bgra, width, height, stride, Team::Dire);
                PixelReport {
                    structural_ok: radiant.structural_ok && dire.structural_ok,
                    enemy_blobs: Vec::new(),
                }
            }
        }
    }
}

trait RiskAccess {
    fn tick(&mut self, input: &TickInput) -> RiskOutput;
}

impl RiskAccess for RiskEngine {
    fn tick(&mut self, input: &TickInput) -> RiskOutput {
        RiskEngine::tick(self, input)
    }
}

trait EventSink {
    fn emit_value(&self, event: &str, value: Value);
}

impl EventSink for tauri::AppHandle {
    fn emit_value(&self, event: &str, value: Value) {
        let _ = self.emit(event, value);
    }
}

trait FrameSink {
    fn send_raw(&mut self, bytes: &[u8]) -> Result<(), ()>;
}

impl FrameSink for Channel<InvokeResponseBody> {
    fn send_raw(&mut self, bytes: &[u8]) -> Result<(), ()> {
        self.send(InvokeResponseBody::Raw(bytes.to_vec()))
            .map_err(|_| ())
    }
}

#[derive(Clone)]
struct CandidateFrame {
    output_id: u64,
    session_epoch: u64,
    sequence: u64,
    acquired_at_ms: u64,
    width: u32,
    height: u32,
    stride: u32,
    bgra: Arc<[u8]>,
    analysis_enemy: Option<Team>,
    report: PixelReport,
}

#[derive(Clone)]
struct AdmittedPixels {
    candidate: CandidateFrame,
    header: FrameHeader,
}

/// Capacity-one overwrite slot shared by the display and analysis consumers.
struct LatestValue<T> {
    value: Mutex<Option<Arc<T>>>,
}

impl<T> LatestValue<T> {
    fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    fn replace(&self, value: T) {
        *lock(&self.value) = Some(Arc::new(value));
    }

    fn read(&self) -> Option<Arc<T>> {
        lock(&self.value).clone()
    }

    fn clear(&self) {
        *lock(&self.value) = None;
    }
}

#[derive(Default)]
struct RateWindow {
    began: Option<Instant>,
    samples: VecDeque<Instant>,
}

impl RateWindow {
    fn start(&mut self, at: Instant) {
        self.began = Some(at);
    }

    fn record(&mut self, at: Instant) {
        if self.began.is_none() {
            self.began = Some(at);
        }
        self.samples.push_back(at);
        self.prune(at);
    }

    fn rate(&mut self, now: Instant) -> Option<u32> {
        if self
            .began
            .is_none_or(|began| now.saturating_duration_since(began) < RESOURCE_INTERVAL)
        {
            return None;
        }
        self.prune(now);
        u32::try_from(self.samples.len()).ok()
    }

    fn prune(&mut self, now: Instant) {
        while self
            .samples
            .front()
            .is_some_and(|sample| now.saturating_duration_since(*sample) > RESOURCE_INTERVAL)
        {
            self.samples.pop_front();
        }
    }
}

#[derive(Default)]
struct DisplayLimiter {
    last_sent: Option<Instant>,
    last_sequence: Option<u64>,
    sent_rate: RateWindow,
}

impl DisplayLimiter {
    fn new(started_at: Instant) -> Self {
        Self {
            last_sent: None,
            last_sequence: None,
            sent_rate: RateWindow {
                began: Some(started_at),
                samples: VecDeque::new(),
            },
        }
    }

    fn can_send(&self, now: Instant, sequence: u64, crop_visible: bool) -> bool {
        crop_visible
            && self.last_sequence != Some(sequence)
            && self
                .last_sent
                .is_none_or(|last| now.saturating_duration_since(last) >= DISPLAY_INTERVAL)
    }

    fn record_sent(&mut self, now: Instant, sequence: u64) {
        self.last_sent = Some(now);
        self.last_sequence = Some(sequence);
        self.sent_rate.record(now);
    }
}

fn frame_message(header: &[u8; FRAME_HEADER_LEN], bgra: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(FRAME_HEADER_LEN.saturating_add(bgra.len()));
    message.extend_from_slice(header);
    message.extend_from_slice(bgra);
    message
}

fn broadcast_frame<S: FrameSink>(subscribers: &mut Vec<S>, message: &[u8]) -> bool {
    let mut sent = false;
    subscribers.retain_mut(|subscriber| match subscriber.send_raw(message) {
        Ok(()) => {
            sent = true;
            true
        }
        Err(()) => false,
    });
    sent
}

fn send_frame<S: FrameSink, H: FrameHeaderCodec>(
    frame: &AdmittedPixels,
    crop_visible: bool,
    now: Instant,
    now_ms: u64,
    header_codec: &H,
    display: &mut DisplayLimiter,
    subscribers: &mut Vec<S>,
) -> bool {
    if subscribers.is_empty() || !display.can_send(now, frame.header.sequence, crop_visible) {
        return false;
    }
    let Some(age_ms) = now_ms.checked_sub(frame.candidate.acquired_at_ms) else {
        return false;
    };
    let Some(header) = header_codec.encode_header(frame, age_ms) else {
        return false;
    };
    let message = frame_message(&header, &frame.candidate.bgra);
    if broadcast_frame(subscribers, &message) {
        display.record_sent(now, frame.header.sequence);
        true
    } else {
        false
    }
}

#[derive(Default)]
struct RetryBackoff {
    next: Option<Duration>,
}

impl RetryBackoff {
    fn delay(&mut self) -> Duration {
        let delay = self.next.unwrap_or(RETRY_INITIAL);
        self.next = Some(delay.saturating_mul(2).min(RETRY_MAX).max(RETRY_INITIAL));
        delay
    }

    fn reset(&mut self) {
        self.next = None;
    }
}

trait FrameHeaderCodec {
    fn encode_header(&self, frame: &AdmittedPixels, age_ms: u64) -> Option<[u8; FRAME_HEADER_LEN]>;
}

struct MvpFrameHeaderCodec;

impl FrameHeaderCodec for MvpFrameHeaderCodec {
    fn encode_header(&self, frame: &AdmittedPixels, age_ms: u64) -> Option<[u8; FRAME_HEADER_LEN]> {
        let width = u16::try_from(frame.candidate.width).ok()?;
        let height = u16::try_from(frame.candidate.height).ok()?;
        if width == 0 || height == 0 {
            return None;
        }
        let tight_stride = frame.candidate.width.checked_mul(4)?;
        if frame.candidate.stride != tight_stride
            || frame.header.session_epoch != frame.candidate.session_epoch
            || frame.header.output_id != frame.candidate.output_id
            || frame.header.sequence != frame.candidate.sequence
        {
            return None;
        }
        let expected_len = usize::try_from(frame.candidate.stride)
            .ok()?
            .checked_mul(usize::try_from(frame.candidate.height).ok()?)?;
        if frame.candidate.bgra.len() != expected_len {
            return None;
        }

        let session_epoch = u32::try_from(frame.header.session_epoch).ok()?;
        let calibration_generation = u32::try_from(frame.header.calibration_generation).ok()?;
        let frame_sequence = u32::try_from(frame.header.sequence).ok()?;
        let age_ms = u32::try_from(age_ms).ok()?;
        let mut header = [0_u8; FRAME_HEADER_LEN];
        header[0..4].copy_from_slice(&FRAME_MAGIC.to_le_bytes());
        header[4..6].copy_from_slice(&FRAME_VERSION.to_le_bytes());
        header[6..8].copy_from_slice(&0_u16.to_le_bytes());
        header[8..10].copy_from_slice(&width.to_le_bytes());
        header[10..12].copy_from_slice(&height.to_le_bytes());
        header[12..16].copy_from_slice(&frame.candidate.stride.to_le_bytes());
        header[16..20].copy_from_slice(&session_epoch.to_le_bytes());
        header[20..24].copy_from_slice(&calibration_generation.to_le_bytes());
        header[24..28].copy_from_slice(&frame_sequence.to_le_bytes());
        header[28..32].copy_from_slice(&age_ms.to_le_bytes());
        Some(header)
    }
}

struct WorkerServices<A, R, E, H> {
    analyzer: A,
    risk: R,
    events: E,
    header_codec: H,
    gsi: GsiView,
    resources: ResourceView,
    capture_enabled: bool,
}

struct CaptureHostWorker<S, C, A, R, E, H> {
    source: S,
    controller: Option<C>,
    analyzer: A,
    risk: R,
    events: E,
    header_codec: H,
    gsi: GsiView,
    resources: ResourceView,
    started: Instant,
    capture_enabled: bool,
    source_open: bool,
    grab_buffer: Vec<u8>,
    next_sequence: u64,
    next_open_at: Instant,
    retry: RetryBackoff,
    unsupported_output: bool,
    user_invalidated: bool,
    lost: bool,
    session_epoch: Option<u64>,
    output: Option<OutputInfo>,
    profile_rect: Option<Rect>,
    profile_name: Option<String>,
    environment: Option<Environment>,
    latest_admitted: LatestValue<AdmittedPixels>,
    capture_rate: RateWindow,
    display: DisplayLimiter,
    next_analysis: Instant,
    next_status: Instant,
    next_resource: Instant,
    consecutive_passes: u32,
    last_risk_payload: Option<Value>,
    last_risk_emit_ms: Option<u64>,
    seen_gsi_event_keys: HashSet<(&'static str, u64)>,
    gsi_event_order: VecDeque<(&'static str, u64)>,
}

impl<S, C, A, R, E, H> CaptureHostWorker<S, C, A, R, E, H>
where
    S: FrameSource,
    C: CaptureAccess,
    A: FrameAnalyzer,
    R: RiskAccess,
    E: EventSink,
    H: FrameHeaderCodec,
{
    fn new(source: S, services: WorkerServices<A, R, E, H>, now: Instant) -> Self {
        Self {
            source,
            controller: None,
            analyzer: services.analyzer,
            risk: services.risk,
            events: services.events,
            header_codec: services.header_codec,
            gsi: services.gsi,
            resources: services.resources,
            started: now,
            capture_enabled: services.capture_enabled,
            source_open: false,
            grab_buffer: Vec::new(),
            next_sequence: 0,
            next_open_at: now,
            retry: RetryBackoff::default(),
            unsupported_output: false,
            user_invalidated: false,
            lost: false,
            session_epoch: None,
            output: None,
            profile_rect: None,
            profile_name: None,
            environment: None,
            latest_admitted: LatestValue::new(),
            capture_rate: {
                let mut rate = RateWindow::default();
                rate.start(now);
                rate
            },
            display: DisplayLimiter::new(now),
            next_analysis: now,
            next_status: now,
            next_resource: now,
            consecutive_passes: 0,
            last_risk_payload: None,
            last_risk_emit_ms: None,
            seen_gsi_event_keys: HashSet::new(),
            gsi_event_order: VecDeque::new(),
        }
    }

    fn run(&mut self) -> ! {
        loop {
            let now = Instant::now();
            let now_ms = self.elapsed_ms(now);
            if catch_unwind(AssertUnwindSafe(|| self.cycle_at(now, now_ms))).is_err() {
                self.handle_panic(now, now_ms);
            }
            let sleep = if !self.capture_enabled || self.user_invalidated {
                ANALYSIS_INTERVAL
            } else if !self.source_open || self.unsupported_output {
                Duration::from_millis(50)
            } else {
                Duration::from_millis(2)
            };
            thread::sleep(sleep);
        }
    }

    fn elapsed_ms(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.started)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    fn cycle_at(&mut self, now: Instant, now_ms: u64) {
        if USER_INVALIDATE.load(Ordering::Acquire) && !self.user_invalidated {
            self.invalidate_for_user(now_ms);
        }

        let gsi = catch_unwind(AssertUnwindSafe(|| self.gsi.read())).ok();
        if !self.capture_enabled || self.user_invalidated {
            self.unsupported_output = true;
        } else {
            self.ensure_source(now);
            if self.source_open {
                self.refresh_capture_profile(now, now_ms, gsi.as_ref());
                if !self.unsupported_output && self.session_epoch.is_some() {
                    self.capture_one(now, now_ms, gsi.as_ref());
                }
            }
        }

        if now >= self.next_analysis {
            self.analysis_tick(now_ms, gsi.as_ref());
            self.next_analysis = now + ANALYSIS_INTERVAL;
        }
        self.publish_frame(now, now_ms);
        if now >= self.next_status {
            self.publish_status(now, now_ms);
            self.next_status = now + STATUS_INTERVAL;
        }
        if now >= self.next_resource {
            self.publish_resource(now, now_ms);
            self.next_resource = now + RESOURCE_INTERVAL;
        }
    }

    fn ensure_source(&mut self, now: Instant) {
        if self.source_open || now < self.next_open_at {
            return;
        }
        match self.source.open() {
            Ok(()) => {
                self.source_open = true;
                self.output = self.source.output();
            }
            Err(_error) => {
                self.mark_lost(now);
                self.schedule_retry(now);
            }
        }
    }

    fn schedule_retry(&mut self, now: Instant) {
        let delay = self.retry.delay();
        self.next_open_at = now + delay;
    }

    fn mark_lost(&mut self, now: Instant) {
        self.lost = true;
        let now_ms = self.elapsed_ms(now);
        if let Some(controller) = self.controller.as_mut() {
            controller.record_cycle(now_ms, CycleOutcome::Failed);
        }
        self.clear_frames();
    }

    fn refresh_capture_profile(&mut self, now: Instant, now_ms: u64, gsi: Option<&GsiSnapshot>) {
        let Some(output) = self.source.output() else {
            self.mark_lost(now);
            self.source.close();
            self.source_open = false;
            self.schedule_retry(now);
            return;
        };
        let Some((rect, profile)) = self.source.profile_rect(&output) else {
            self.unsupported_output = true;
            self.profile_rect = None;
            self.profile_name = None;
            self.clear_frames();
            if let Some(controller) = self.controller.as_mut() {
                controller.record_cycle(now_ms, CycleOutcome::Unsupported);
            }
            return;
        };
        let Some(window_rect) = self.source.game_window() else {
            self.unsupported_output = true;
            self.profile_rect = None;
            self.profile_name = Some(profile);
            self.clear_frames();
            if let Some(controller) = self.controller.as_mut() {
                controller.record_cycle(now_ms, CycleOutcome::Unsupported);
            }
            return;
        };

        self.unsupported_output = false;
        let environment = Environment {
            output: kohbor_capture::OutputGeometry {
                output_id: output.output_id,
                width: output.width,
                height: output.height,
                dpi: output.dpi,
            },
            monitor_id: output.output_id,
            window_rect,
            minimap_layout: profile.clone(),
        };
        self.output = Some(output);
        let new_rect = self.profile_rect != Some(rect);
        let new_profile = self.profile_name.as_deref() != Some(profile.as_str());
        let new_environment = self.environment.as_ref() != Some(&environment);
        let changed = new_rect || new_profile || new_environment;
        self.profile_rect = Some(rect);
        self.profile_name = Some(profile.clone());

        let next_epoch = gsi.and_then(|snapshot| snapshot.session_epoch);
        if next_epoch != self.session_epoch {
            self.session_epoch = next_epoch;
            self.clear_frames();
            self.consecutive_passes = 0;
            if let (Some(controller), Some(epoch)) = (self.controller.as_mut(), next_epoch) {
                controller.set_session_epoch(epoch);
            }
        }
        let Some(session_epoch) = next_epoch else {
            self.environment = Some(environment);
            self.clear_frames();
            return;
        };

        if self.controller.is_none() {
            self.controller = Some(C::new(session_epoch));
        }
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if changed {
            controller.observe_environment(&environment);
            if !controller.begin(environment.clone(), rect, profile) {
                self.unsupported_output = true;
                self.clear_frames();
                return;
            }
            self.clear_frames();
            self.consecutive_passes = 0;
        } else if matches!(
            controller.calibration_phase(),
            CalibrationPhase::Uncalibrated | CalibrationPhase::RecalibrationNeeded
        ) {
            if !controller.begin(environment.clone(), rect, profile) {
                self.unsupported_output = true;
                self.clear_frames();
                return;
            }
            self.consecutive_passes = 0;
        }
        self.environment = Some(environment);
    }

    fn capture_one(&mut self, now: Instant, now_ms: u64, gsi: Option<&GsiSnapshot>) {
        let (Some(rect), Some(output), Some(session_epoch)) =
            (self.profile_rect, self.output, self.session_epoch)
        else {
            return;
        };
        self.grab_buffer.clear();
        match self
            .source
            .grab(rect, CAPTURE_TIMEOUT_MS, &mut self.grab_buffer)
        {
            Grab::Timeout => {
                if let Some(controller) = self.controller.as_mut() {
                    controller.record_cycle(now_ms, CycleOutcome::HealthyTimeout);
                }
            }
            Grab::AccessLost => {
                if let Some(controller) = self.controller.as_mut() {
                    controller.record_cycle(now_ms, CycleOutcome::AccessLost);
                }
                self.source.close();
                self.source_open = false;
                self.schedule_retry(now);
                self.lost = true;
                self.clear_frames();
            }
            Grab::Failed(_error) => {
                if let Some(controller) = self.controller.as_mut() {
                    controller.record_cycle(now_ms, CycleOutcome::Failed);
                }
                self.source.close();
                self.source_open = false;
                self.schedule_retry(now);
                self.lost = true;
                self.clear_frames();
            }
            Grab::Frame {
                width,
                height,
                stride,
                acquired_us: _,
            } => {
                self.capture_rate.record(now);
                let Some(min_stride) = width.checked_mul(4) else {
                    self.record_malformed_frame(now, now_ms);
                    return;
                };
                let expected_len = usize::try_from(height)
                    .ok()
                    .and_then(|height| height.checked_mul(stride as usize));
                if width == 0
                    || height == 0
                    || stride != min_stride
                    || expected_len != Some(self.grab_buffer.len())
                {
                    self.record_malformed_frame(now, now_ms);
                    return;
                }
                self.retry.reset();
                self.lost = false;
                let enemy = fresh_enemy_team(gsi);
                let report = self.analyzer.analyze(
                    &self.grab_buffer,
                    width,
                    height,
                    stride,
                    enemy,
                );
                let was_crop_visible = self
                    .controller
                    .as_ref()
                    .is_some_and(CaptureAccess::crop_visible);
                if let Some(controller) = self.controller.as_mut() {
                    controller.structural_frame(report.structural_ok);
                }
                if report.structural_ok {
                    self.consecutive_passes = self.consecutive_passes.saturating_add(1);
                } else {
                    self.consecutive_passes = 0;
                }
                let mut crop_visible = self
                    .controller
                    .as_ref()
                    .is_some_and(CaptureAccess::crop_visible);
                if was_crop_visible && !crop_visible {
                    self.latest_admitted.clear();
                    self.consecutive_passes = 0;
                }
                if !crop_visible && self.consecutive_passes >= 30 {
                    let confirmed = self
                        .controller
                        .as_mut()
                        .is_some_and(|controller| controller.confirm_profile(now_ms));
                    crop_visible = confirmed
                        || self
                            .controller
                            .as_ref()
                            .is_some_and(CaptureAccess::crop_visible);
                }
                let Some(sequence) = self.next_sequence.checked_add(1) else {
                    self.mark_lost(now);
                    self.source.close();
                    self.source_open = false;
                    self.schedule_retry(now);
                    return;
                };
                self.next_sequence = sequence;
                let candidate = CandidateFrame {
                    output_id: output.output_id,
                    session_epoch,
                    sequence,
                    acquired_at_ms: now_ms,
                    width,
                    height,
                    stride,
                    bgra: Arc::from(self.grab_buffer.as_slice()),
                    analysis_enemy: enemy,
                    report,
                };
                if crop_visible {
                    self.admit_candidate(candidate, now_ms);
                }
            }
        }
    }

    fn record_malformed_frame(&mut self, now: Instant, now_ms: u64) {
        if let Some(controller) = self.controller.as_mut() {
            controller.structural_frame(false);
            controller.record_cycle(now_ms, CycleOutcome::Failed);
        }
        self.consecutive_passes = 0;
        self.source.close();
        self.source_open = false;
        self.schedule_retry(now);
        self.lost = true;
        self.clear_frames();
    }

    fn admit_candidate(&mut self, candidate: CandidateFrame, now_ms: u64) -> bool {
        let _invalidate_gate = lock(&FRAME_INVALIDATE_GATE);
        if USER_INVALIDATE.load(Ordering::Acquire) || self.user_invalidated {
            return false;
        }
        let Some(controller) = self.controller.as_mut() else {
            return false;
        };
        let Some(calibration_generation) = controller.generation() else {
            return false;
        };
        let header = FrameHeader {
            source: CaptureSource::Dxgi,
            session_epoch: candidate.session_epoch,
            calibration_generation,
            output_id: candidate.output_id,
            sequence: candidate.sequence,
            acquired_at_ms: candidate.acquired_at_ms,
        };
        if controller.admit(&header, now_ms).is_err() {
            return false;
        }
        self.lost = false;
        self.latest_admitted
            .replace(AdmittedPixels { candidate, header });
        true
    }

    fn analysis_tick(&mut self, now_ms: u64, gsi: Option<&GsiSnapshot>) {
        let externally_invalidated = USER_INVALIDATE.load(Ordering::Acquire);
        let enemy = fresh_enemy_team(gsi);
        let crop_visible = self
            .controller
            .as_ref()
            .is_some_and(CaptureAccess::crop_visible)
            && !externally_invalidated
            && !self.user_invalidated;
        let latest_admitted = crop_visible.then(|| self.latest_admitted.read()).flatten();

        let coverage = if externally_invalidated || self.user_invalidated {
            Coverage::Unavailable
        } else {
            self.controller
                .as_ref()
                .map_or(Coverage::Unavailable, |controller| {
                    controller.coverage(now_ms)
                })
        };
        let visible = latest_admitted.as_ref().and_then(|frame| {
            (frame.candidate.analysis_enemy == enemy)
                .then(|| {
                    visible_count(
                        Some(&frame.candidate.report),
                        enemy,
                        coverage,
                    )
                })
                .flatten()
        });
        let gsi_events = self.new_gsi_events(gsi);
        let input = TickInput {
            now_ms,
            coverage,
            visible,
            own: own_gsi(gsi),
            gsi_events,
        };
        let output = self.risk.tick(&input);
        let payload = risk_payload(&output);
        let changed = self.last_risk_payload.as_ref() != Some(&payload);
        let heartbeat_due = self
            .last_risk_emit_ms
            .and_then(|last_ms| now_ms.checked_sub(last_ms))
            .is_some_and(|elapsed_ms| elapsed_ms >= RISK_HEARTBEAT_MS);
        if changed || heartbeat_due {
            self.events.emit_value("risk://state", payload.clone());
            self.last_risk_payload = Some(payload);
            self.last_risk_emit_ms = Some(now_ms);
        }
    }

    fn new_gsi_events(&mut self, gsi: Option<&GsiSnapshot>) -> Vec<&'static str> {
        let mut new_events = Vec::new();
        let Some(snapshot) = gsi else {
            return new_events;
        };
        for event in &snapshot.recent_events {
            if !matches!(event.id, "health_low" | "health_danger") {
                continue;
            }
            let key = (event.id, event.at_ms);
            if self.seen_gsi_event_keys.insert(key) {
                self.gsi_event_order.push_back(key);
                new_events.push(event.id);
                while self.gsi_event_order.len() > MAX_SEEN_GSI_EVENT_KEYS {
                    if let Some(expired) = self.gsi_event_order.pop_front() {
                        self.seen_gsi_event_keys.remove(&expired);
                    }
                }
            }
        }
        new_events
    }

    fn invalidate_for_user(&mut self, now_ms: u64) {
        self.user_invalidated = true;
        if let Some(controller) = self.controller.as_mut() {
            controller.invalidate_user(now_ms);
        }
        self.clear_frames();
    }

    fn clear_frames(&mut self) {
        self.latest_admitted.clear();
        self.consecutive_passes = 0;
    }

    fn phase(&self, now_ms: u64) -> &'static str {
        if !self.capture_enabled || self.user_invalidated || self.unsupported_output {
            return "unsupported";
        }
        if self.lost {
            return "lost";
        }
        if self.session_epoch.is_none() {
            return "unknown";
        }
        self.controller.as_ref().map_or("unknown", |controller| {
            capture_phase_label(controller.phase(now_ms))
        })
    }

    fn frame_age_ms(&self, now_ms: u64) -> Option<u64> {
        self.latest_admitted
            .read()
            .and_then(|frame| now_ms.checked_sub(frame.candidate.acquired_at_ms))
    }

    fn publish_frame(&mut self, now: Instant, now_ms: u64) {
        let mut subscribers = lock(frame_subscribers());
        self.publish_frame_to(now, now_ms, &mut subscribers);
    }

    fn publish_frame_to<Sink: FrameSink>(
        &mut self,
        now: Instant,
        now_ms: u64,
        subscribers: &mut Vec<Sink>,
    ) {
        let _invalidate_gate = lock(&FRAME_INVALIDATE_GATE);
        if USER_INVALIDATE.load(Ordering::Acquire) || self.user_invalidated {
            return;
        }
        let Some(frame) = self.latest_admitted.read() else {
            return;
        };
        let crop_visible = self
            .controller
            .as_ref()
            .is_some_and(CaptureAccess::crop_visible);
        send_frame(
            &frame,
            crop_visible,
            now,
            now_ms,
            &self.header_codec,
            &mut self.display,
            subscribers,
        );
    }

    fn publish_status(&mut self, now: Instant, now_ms: u64) {
        let payload = status_payload(
            self.phase(now_ms),
            self.display.sent_rate.rate(now),
            self.frame_age_ms(now_ms),
            self.profile_name.as_deref(),
            self.controller.as_ref().and_then(CaptureAccess::generation),
        );
        *lock(capture_status_slot()) = Some(payload.clone());
        self.events.emit_value("capture://status", payload);
    }

    fn publish_resource(&mut self, now: Instant, now_ms: u64) {
        let sample = catch_unwind(AssertUnwindSafe(|| self.resources.read())).ok();
        let gpu_local_bytes = if self.source_open {
            catch_unwind(AssertUnwindSafe(|| self.source.gpu_local_bytes()))
                .ok()
                .flatten()
        } else {
            None
        };
        let payload = resource_payload(
            sample,
            gpu_local_bytes,
            self.capture_rate.rate(now),
            self.frame_age_ms(now_ms),
        );
        self.events.emit_value("resource://sample", payload);
    }

    fn handle_panic(&mut self, now: Instant, now_ms: u64) {
        self.lost = true;
        self.clear_frames();
        let _ = catch_unwind(AssertUnwindSafe(|| self.source.close()));
        self.source_open = false;
        self.schedule_retry(now);
        let payload = status_payload(
            "lost",
            self.display.sent_rate.rate(now),
            self.frame_age_ms(now_ms),
            self.profile_name.as_deref(),
            self.controller.as_ref().and_then(CaptureAccess::generation),
        );
        *lock(capture_status_slot()) = Some(payload.clone());
    }
}

fn run_capture_host(app: tauri::AppHandle, gsi: GsiView, resources: ResourceView) {
    let capture_enabled = std::env::var("KOHBOR_CAPTURE").ok().as_deref() != Some("0");
    let now = Instant::now();
    let services = WorkerServices {
        analyzer: MinimapAnalyzer,
        risk: RiskEngine::new(),
        events: app,
        header_codec: MvpFrameHeaderCodec,
        gsi,
        resources,
        capture_enabled,
    };
    let mut worker = CaptureHostWorker::<
        NativeFrameSource,
        CaptureControllerAccess,
        MinimapAnalyzer,
        RiskEngine,
        tauri::AppHandle,
        MvpFrameHeaderCodec,
    >::new(NativeFrameSource::new(), services, now);
    worker.run();
}

/// An intake health event with its original observation timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GsiEvent {
    pub id: &'static str,
    pub at_ms: u64,
}

/// Own-hero facts read from the intake snapshot. Unknown fields remain unknown unless observed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GsiSnapshot {
    /// "radiant" or "dire".
    pub team: Option<String>,
    /// The admitted match/session generation, if one is present in the read model.
    pub session_epoch: Option<u64>,
    pub alive: Option<bool>,
    pub hp_pct: Option<u8>,
    /// Set by the intake producer only while its phase is Live.
    pub in_match: bool,
    pub age_ms: Option<u64>,
    /// Health events detected in the recent GSI window. Defaults to empty.
    pub recent_events: Vec<GsiEvent>,
}

impl GsiSnapshot {
    /// Parses an own-snapshot 2.0.0 value. Missing or invalid fields stay `None`.
    pub fn from_snapshot(snapshot: &Value) -> Self {
        let own = &snapshot["own"];
        let value = |name: &str| &own["fields"][name]["value"];
        let hp = value("hp").as_u64();
        let max_hp = value("max_hp").as_u64();
        let hp_pct = match (hp, max_hp) {
            (Some(hp), Some(max)) if max > 0 => {
                Some(((u128::from(hp) * 100) / u128::from(max)).min(100) as u8)
            }
            _ => None,
        };
        Self {
            team: own["team"].as_str().map(str::to_owned),
            session_epoch: snapshot["session_epoch"].as_u64(),
            alive: value("alive").as_bool(),
            hp_pct,
            in_match: false,
            age_ms: snapshot["source"]["age_ms"].as_u64(),
            recent_events: Vec::new(),
        }
    }
}

/// Read-only accessor for the own-hero facts.
#[derive(Clone)]
pub struct GsiView(Arc<dyn Fn() -> GsiSnapshot + Send + Sync>);

impl GsiView {
    pub fn new(read: impl Fn() -> GsiSnapshot + Send + Sync + 'static) -> Self {
        Self(Arc::new(read))
    }
    pub fn read(&self) -> GsiSnapshot {
        (self.0)()
    }
}

/// App-group resource sample (the same one `runtime_get_status` uses). `None` is unknown, never 0.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResourceSample {
    pub cpu_percent: Option<f64>,
    pub working_set_bytes: Option<u64>,
}

/// Read-only accessor. Monitoring only: no code path may use it to admit, throttle or refuse anything.
#[derive(Clone)]
pub struct ResourceView(Arc<dyn Fn() -> ResourceSample + Send + Sync>);

impl ResourceView {
    pub fn new(read: impl Fn() -> ResourceSample + Send + Sync + 'static) -> Self {
        Self(Arc::new(read))
    }
    pub fn read(&self) -> ResourceSample {
        (self.0)()
    }
}

/// Starts the capture host on a dedicated, below-normal-priority thread.
pub fn start(app: tauri::AppHandle, gsi: GsiView, resources: ResourceView) {
    if HOST_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let worker_app = app.clone();
    if let Err(error) = thread::Builder::new()
        .name("capture-host".to_owned())
        .spawn(move || {
            crate::host::set_current_thread_priority(crate::host::ThreadPriority::BelowNormal);
            run_capture_host(worker_app, gsi, resources);
        })
    {
        let status = status_payload("lost", None, None, None, None);
        *lock(capture_status_slot()) = Some(status.clone());
        app.emit_value("capture://status", status);
        eprintln!("capture-host unavailable: {error}");
    }
}

#[tauri::command(async)]
pub(crate) fn minimap_subscribe(on_frame: Channel<InvokeResponseBody>) -> Result<(), String> {
    lock(frame_subscribers()).push(on_frame);
    Ok(())
}

#[tauri::command(async)]
pub(crate) fn capture_status() -> Value {
    lock(capture_status_slot())
        .clone()
        .unwrap_or_else(unknown_capture_status)
}

#[tauri::command(async)]
pub(crate) fn capture_invalidate() -> Result<(), String> {
    let _invalidate_gate = lock(&FRAME_INVALIDATE_GATE);
    USER_INVALIDATE.store(true, Ordering::Release);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kohbor_minimap_cv::Blob;
    use serde_json::json;

    static STATUS_TEST_GATE: Mutex<()> = Mutex::new(());

    #[derive(Clone)]
    struct FakeFrameSource {
        output: Option<OutputInfo>,
        profile: Option<(Rect, String)>,
        window: Option<Rect>,
        gpu_local: Option<u64>,
        open_error: Option<OpenError>,
        opened: bool,
        grabs: VecDeque<(Grab, Vec<u8>)>,
        grab_buffer_addresses: Vec<usize>,
        grab_buffer_capacities_on_entry: Vec<usize>,
    }

    impl FakeFrameSource {
        fn supported() -> Self {
            Self {
                output: Some(OutputInfo {
                    output_id: 4,
                    width: 1920,
                    height: 1080,
                    dpi: 96,
                }),
                profile: Some((
                    Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    "fake-profile".to_owned(),
                )),
                window: Some(Rect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                }),
                gpu_local: None,
                open_error: None,
                opened: false,
                grabs: VecDeque::new(),
                grab_buffer_addresses: Vec::new(),
                grab_buffer_capacities_on_entry: Vec::new(),
            }
        }

        fn queue_frame(&mut self, sequence: u64) {
            self.grabs.push_back((
                Grab::Frame {
                    width: 1,
                    height: 1,
                    stride: 4,
                    acquired_us: sequence * 1000,
                },
                vec![sequence as u8, 2, 3, 255],
            ));
        }
    }

    impl FrameSource for FakeFrameSource {
        fn open(&mut self) -> Result<(), OpenError> {
            if let Some(error) = self.open_error.clone() {
                return Err(error);
            }
            self.opened = true;
            Ok(())
        }

        fn output(&self) -> Option<OutputInfo> {
            self.opened.then_some(self.output).flatten()
        }

        fn profile_rect(&self, _output: &OutputInfo) -> Option<(Rect, String)> {
            self.profile.clone()
        }

        fn game_window(&self) -> Option<Rect> {
            self.window
        }

        fn grab(&mut self, _rect: Rect, _timeout_ms: u32, out: &mut Vec<u8>) -> Grab {
            let Some((result, pixels)) = self.grabs.pop_front() else {
                return Grab::Timeout;
            };
            if matches!(result, Grab::Frame { .. }) {
                self.grab_buffer_capacities_on_entry.push(out.capacity());
                out.extend_from_slice(&pixels);
                self.grab_buffer_addresses.push(out.as_ptr() as usize);
            }
            result
        }

        fn gpu_local_bytes(&self) -> Option<u64> {
            self.gpu_local
        }

        fn close(&mut self) {
            self.opened = false;
        }
    }

    struct FakeCaptureAccess {
        epoch: u64,
        generation: u64,
        validated: bool,
        invalidated: bool,
        unsupported: bool,
        lost: bool,
        structural_passes: u32,
        last_healthy_ms: Option<u64>,
    }

    impl FakeCaptureAccess {
        fn unvalidated(epoch: u64) -> Self {
            Self {
                epoch,
                generation: 0,
                validated: false,
                invalidated: false,
                unsupported: false,
                lost: false,
                structural_passes: 0,
                last_healthy_ms: None,
            }
        }
    }

    impl CaptureAccess for FakeCaptureAccess {
        fn new(session_epoch: u64) -> Self {
            Self::unvalidated(session_epoch)
        }

        fn set_session_epoch(&mut self, epoch: u64) {
            self.epoch = epoch;
        }

        fn begin(&mut self, _environment: Environment, _rect: Rect, _profile: String) -> bool {
            self.generation += 1;
            self.validated = false;
            self.unsupported = false;
            self.structural_passes = 0;
            true
        }

        fn observe_environment(&mut self, _environment: &Environment) {}

        fn structural_frame(&mut self, passed: bool) {
            if passed {
                self.structural_passes += 1;
            } else {
                self.structural_passes = 0;
            }
        }

        fn confirm_profile(&mut self, _now_ms: u64) -> bool {
            if self.structural_passes < 30 || self.invalidated {
                return false;
            }
            self.validated = true;
            true
        }

        fn generation(&self) -> Option<u64> {
            (self.generation != 0).then_some(self.generation)
        }

        fn admit(&mut self, header: &FrameHeader, now_ms: u64) -> Result<(), Reject> {
            if header.session_epoch != self.epoch {
                return Err(Reject::WrongSession);
            }
            if !self.validated || self.invalidated || self.unsupported {
                return Err(Reject::NotValidated);
            }
            if header.acquired_at_ms > now_ms {
                return Err(Reject::OutOfOrder);
            }
            self.lost = false;
            self.last_healthy_ms = Some(header.acquired_at_ms);
            Ok(())
        }

        fn record_cycle(&mut self, now_ms: u64, outcome: CycleOutcome) {
            match outcome {
                CycleOutcome::HealthyTimeout => self.last_healthy_ms = Some(now_ms),
                CycleOutcome::AccessLost => {
                    self.lost = true;
                    self.validated = false;
                    self.generation += 1;
                }
                CycleOutcome::Failed => {}
                CycleOutcome::Unsupported => self.unsupported = true,
            }
        }

        fn phase(&self, now_ms: u64) -> CapturePhase {
            if self.unsupported || self.invalidated {
                return CapturePhase::Unsupported;
            }
            if self.lost {
                return CapturePhase::Lost;
            }
            if !self.validated {
                return CapturePhase::Calibrating;
            }
            match self.last_healthy_ms.and_then(|at| now_ms.checked_sub(at)) {
                None => CapturePhase::CoverageUnavailable,
                Some(age) if age > 5000 => CapturePhase::CoverageUnavailable,
                Some(age) if age > 2000 => CapturePhase::Stale,
                Some(_) => CapturePhase::Live,
            }
        }

        fn coverage(&self, now_ms: u64) -> Coverage {
            match self.phase(now_ms) {
                CapturePhase::Live | CapturePhase::Lite => Coverage::Valid,
                CapturePhase::Stale => Coverage::Stale,
                _ => Coverage::Unavailable,
            }
        }

        fn crop_visible(&self) -> bool {
            self.validated && !self.invalidated && !self.unsupported
        }

        fn invalidate_user(&mut self, _now_ms: u64) {
            self.invalidated = true;
            self.unsupported = true;
            self.validated = false;
            self.generation += 1;
        }

        fn calibration_phase(&self) -> CalibrationPhase {
            if self.validated {
                CalibrationPhase::Validated
            } else if self.invalidated || self.lost {
                CalibrationPhase::RecalibrationNeeded
            } else {
                CalibrationPhase::Calibrating
            }
        }
    }

    #[derive(Default)]
    struct FakeAnalyzer {
        calls: usize,
        structural_results: VecDeque<bool>,
    }

    impl FrameAnalyzer for FakeAnalyzer {
        fn analyze(
            &mut self,
            _bgra: &[u8],
            _width: u32,
            _height: u32,
            _stride: u32,
            _enemy: Option<Team>,
        ) -> PixelReport {
            self.calls += 1;
            PixelReport {
                structural_ok: self.structural_results.pop_front().unwrap_or(true),
                enemy_blobs: vec![Blob {
                    x_milli: 500,
                    y_milli: 500,
                    area: 4,
                }],
            }
        }
    }

    #[derive(Clone)]
    struct FakeRisk {
        seen_visible: Arc<Mutex<Vec<Option<u8>>>>,
        seen_gsi_events: Arc<Mutex<Vec<Vec<&'static str>>>>,
    }

    impl RiskAccess for FakeRisk {
        fn tick(&mut self, input: &TickInput) -> RiskOutput {
            lock(&self.seen_visible).push(input.visible);
            lock(&self.seen_gsi_events).push(input.gsi_events.clone());
            RiskOutput {
                meter: if input.own.is_some() {
                    Meter::Level(1)
                } else {
                    Meter::Unknown
                },
                gsi_source: input.own.is_some(),
                vision_source: input.visible.is_some(),
                visible: input.visible,
                baseline: None,
                events: Vec::new(),
            }
        }
    }

    #[derive(Clone)]
    struct FakeEvents(Arc<Mutex<Vec<(String, Value)>>>);

    impl EventSink for FakeEvents {
        fn emit_value(&self, event: &str, value: Value) {
            lock(&self.0).push((event.to_owned(), value));
        }
    }

    #[derive(Default)]
    struct FakeFrameSink {
        messages: Vec<Vec<u8>>,
        fail: bool,
    }

    impl FrameSink for FakeFrameSink {
        fn send_raw(&mut self, bytes: &[u8]) -> Result<(), ()> {
            if self.fail {
                return Err(());
            }
            self.messages.push(bytes.to_vec());
            Ok(())
        }
    }

    fn fake_worker(
        source: FakeFrameSource,
        capture_enabled: bool,
        now: Instant,
        events: FakeEvents,
    ) -> CaptureHostWorker<
        FakeFrameSource,
        FakeCaptureAccess,
        FakeAnalyzer,
        FakeRisk,
        FakeEvents,
        MvpFrameHeaderCodec,
    > {
        CaptureHostWorker::new(
            source,
            WorkerServices {
                analyzer: FakeAnalyzer::default(),
                risk: FakeRisk {
                    seen_visible: Arc::new(Mutex::new(Vec::new())),
                    seen_gsi_events: Arc::new(Mutex::new(Vec::new())),
                },
                events,
                header_codec: MvpFrameHeaderCodec,
                gsi: GsiView::new(GsiSnapshot::default),
                resources: ResourceView::new(ResourceSample::default),
                capture_enabled,
            },
            now,
        )
    }

    fn calibrating_worker(
        now: Instant,
        frame_count: usize,
        structural_results: impl IntoIterator<Item = bool>,
    ) -> CaptureHostWorker<
        FakeFrameSource,
        FakeCaptureAccess,
        FakeAnalyzer,
        FakeRisk,
        FakeEvents,
        MvpFrameHeaderCodec,
    > {
        let mut source = FakeFrameSource::supported();
        for sequence in 1..=frame_count {
            source.queue_frame(sequence as u64);
        }
        source.opened = true;
        let mut worker = fake_worker(source, true, now, events());
        worker.source_open = true;
        worker.output = Some(OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        });
        let rect = Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        worker.profile_rect = Some(rect);
        worker.session_epoch = Some(7);
        let mut controller = FakeCaptureAccess::unvalidated(7);
        controller.begin(
            Environment {
                output: kohbor_capture::OutputGeometry {
                    output_id: 4,
                    width: 1920,
                    height: 1080,
                    dpi: 96,
                },
                monitor_id: 4,
                window_rect: Rect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
                minimap_layout: "fake-profile".to_owned(),
            },
            rect,
            "fake-profile".to_owned(),
        );
        worker.controller = Some(controller);
        worker.analyzer.structural_results = structural_results.into_iter().collect();
        worker
    }

    fn events() -> FakeEvents {
        FakeEvents(Arc::new(Mutex::new(Vec::new())))
    }

    fn admitted_pixels(sequence: u64, at_ms: u64) -> AdmittedPixels {
        let candidate = CandidateFrame {
            output_id: 4,
            session_epoch: 7,
            sequence,
            acquired_at_ms: at_ms,
            width: 1,
            height: 1,
            stride: 4,
            bgra: Arc::from([1_u8, 2, 3, 255]),
            analysis_enemy: Some(Team::Dire),
            report: PixelReport {
                structural_ok: true,
                enemy_blobs: vec![Blob {
                    x_milli: 500,
                    y_milli: 500,
                    area: 4,
                }],
            },
        };
        AdmittedPixels {
            candidate,
            header: FrameHeader {
                source: CaptureSource::Dxgi,
                session_epoch: 7,
                calibration_generation: 1,
                output_id: 4,
                sequence,
                acquired_at_ms: at_ms,
            },
        }
    }

    #[test]
    fn unknown_snapshot_gives_unknown_fields() {
        let s = GsiSnapshot::from_snapshot(&json!({"own": null, "source": {"age_ms": null}}));
        assert_eq!(s, GsiSnapshot::default());
    }

    #[test]
    fn snapshot_fields_are_read_and_hp_percent_is_derived() {
        let s = GsiSnapshot::from_snapshot(&json!({
            "own": {"team": "dire", "fields": {
                "hp": {"value": 150, "availability": "fresh"},
                "max_hp": {"value": 600, "availability": "fresh"},
                "alive": {"value": true, "availability": "fresh"}}},
            "source": {"age_ms": 120},
            "session_epoch": 8
        }));
        assert_eq!(s.team.as_deref(), Some("dire"));
        assert_eq!(s.session_epoch, Some(8));
        assert_eq!(s.hp_pct, Some(25));
        assert_eq!(s.alive, Some(true));
        assert!(!s.in_match);
        assert_eq!(s.age_ms, Some(120));
        assert!(s.recent_events.is_empty());
    }

    #[test]
    fn zero_max_hp_is_unknown_not_zero_percent() {
        let s = GsiSnapshot::from_snapshot(&json!({
            "own": {"fields": {"hp": {"value": 0}, "max_hp": {"value": 0}}}, "source": {}
        }));
        assert_eq!(s.hp_pct, None);
    }

    #[test]
    fn capture_status_is_unknown_until_the_worker_publishes_a_sample() {
        let _status_gate = lock(&STATUS_TEST_GATE);
        *lock(capture_status_slot()) = None;
        let status = super::capture_status();

        assert_eq!(status["phase"], "unknown");
        assert!(status["fps_measured"].is_null());
        assert!(status["frame_age_ms"].is_null());
    }

    #[test]
    fn capture_phase_labels_match_the_renderer_contract() {
        let allowed = [
            "unsupported",
            "calibrating",
            "live",
            "stale",
            "coverage_unavailable",
            "lost",
            "unknown",
        ];
        let labels = [
            CapturePhase::Unsupported,
            CapturePhase::Calibrating,
            CapturePhase::RecalibrationNeeded,
            CapturePhase::Live,
            CapturePhase::Lite,
            CapturePhase::Stale,
            CapturePhase::CoverageUnavailable,
            CapturePhase::Lost,
        ]
        .map(capture_phase_label);

        assert!(labels.iter().all(|label| allowed.contains(label)));
        assert!(allowed.contains(&"unknown"));
    }

    #[test]
    fn frame_message_preserves_the_32_byte_header_and_appends_bgra_rows() {
        let header = [0xA5; FRAME_HEADER_LEN];
        let bgra = [1, 2, 3, 4, 5, 6, 7, 8];
        let message = frame_message(&header, &bgra);

        assert_eq!(message.len(), FRAME_HEADER_LEN + bgra.len());
        assert_eq!(&message[..FRAME_HEADER_LEN], &header);
        assert_eq!(&message[FRAME_HEADER_LEN..], &bgra);
    }

    #[test]
    fn production_header_matches_the_authoritative_little_endian_layout() {
        let mut frame = admitted_pixels(0x99AA_BBCC, 0);
        frame.candidate.width = 2;
        frame.candidate.height = 3;
        frame.candidate.stride = 8;
        frame.candidate.session_epoch = 0x1122_3344;
        frame.candidate.bgra = Arc::from(vec![0xAB; 24]);
        frame.header.session_epoch = 0x1122_3344;
        frame.header.calibration_generation = 0x5566_7788;
        let header = MvpFrameHeaderCodec
            .encode_header(&frame, 0xDDEE_FF00)
            .unwrap();
        let expected = [
            0x50, 0x41, 0x4D, 0x4B, // magic u32
            0x01, 0x00, // version u16
            0x00, 0x00, // flags u16
            0x02, 0x00, // width u16
            0x03, 0x00, // height u16
            0x08, 0x00, 0x00, 0x00, // stride u32
            0x44, 0x33, 0x22, 0x11, // session_epoch u32
            0x88, 0x77, 0x66, 0x55, // calibration_generation u32
            0xCC, 0xBB, 0xAA, 0x99, // frame_seq u32
            0x00, 0xFF, 0xEE, 0xDD, // age_ms u32
        ];
        assert_eq!(header, expected);
        let message = frame_message(&header, &frame.candidate.bgra);
        assert_eq!(message.len(), FRAME_HEADER_LEN + 24);
        let start = Instant::now();
        let mut limiter = DisplayLimiter::new(start);
        let mut sinks = vec![FakeFrameSink::default()];
        assert!(send_frame(
            &frame,
            true,
            start,
            0xDDEE_FF00,
            &MvpFrameHeaderCodec,
            &mut limiter,
            &mut sinks,
        ));
        assert_eq!(&sinks[0].messages[0][..FRAME_HEADER_LEN], &expected);
        assert_eq!(sinks[0].messages[0].len(), FRAME_HEADER_LEN + 24);
        assert_eq!(&sinks[0].messages[0][FRAME_HEADER_LEN..], &[0xAB; 24]);
    }

    #[test]
    fn display_limiter_never_sends_above_30_fps() {
        let start = Instant::now();
        let mut limiter = DisplayLimiter::new(start);
        let codec = MvpFrameHeaderCodec;
        let mut sinks = vec![FakeFrameSink::default()];
        for millisecond in 0..1000_u64 {
            let now = start + Duration::from_millis(millisecond);
            let frame = admitted_pixels(millisecond + 1, millisecond);
            send_frame(
                &frame,
                true,
                now,
                millisecond,
                &codec,
                &mut limiter,
                &mut sinks,
            );
        }

        assert_eq!(sinks[0].messages.len(), 30);
        assert_eq!(
            limiter.sent_rate.rate(start + Duration::from_secs(1)),
            Some(30)
        );
    }

    #[test]
    fn frame_sender_withholds_frames_until_crop_is_validated_and_after_invalidation() {
        let start = Instant::now();
        let mut worker = fake_worker(FakeFrameSource::supported(), true, start, events());
        worker.controller = Some(FakeCaptureAccess::unvalidated(7));
        worker.latest_admitted.replace(admitted_pixels(1, 0));
        let mut sinks = vec![FakeFrameSink::default()];

        worker.publish_frame_to(start, 0, &mut sinks);
        assert!(sinks[0].messages.is_empty());

        let controller = worker.controller.as_mut().unwrap();
        controller.validated = true;
        controller.last_healthy_ms = Some(0);
        worker.publish_frame_to(start + DISPLAY_INTERVAL, 33, &mut sinks);
        assert_eq!(sinks[0].messages.len(), 1);

        worker.invalidate_for_user(33);
        worker.publish_frame_to(start + DISPLAY_INTERVAL + DISPLAY_INTERVAL, 66, &mut sinks);
        assert_eq!(sinks[0].messages.len(), 1);
    }

    #[test]
    fn user_invalidation_clears_admitted_pixels_and_stays_unsupported() {
        let start = Instant::now();
        let mut worker = fake_worker(FakeFrameSource::supported(), true, start, events());
        let mut controller = FakeCaptureAccess::unvalidated(7);
        controller.validated = true;
        controller.last_healthy_ms = Some(0);
        worker.controller = Some(controller);
        worker.latest_admitted.replace(admitted_pixels(1, 0));

        worker.invalidate_for_user(10);

        assert!(worker.user_invalidated);
        assert_eq!(worker.phase(10), "unsupported");
        assert!(worker.latest_admitted.read().is_none());
        assert!(!worker.controller.as_ref().unwrap().crop_visible());
    }

    #[test]
    fn user_invalidation_prevents_later_frame_publication() {
        struct ResetInvalidation;
        impl Drop for ResetInvalidation {
            fn drop(&mut self) {
                let _gate = lock(&FRAME_INVALIDATE_GATE);
                USER_INVALIDATE.store(false, Ordering::Release);
            }
        }

        let _status_gate = lock(&STATUS_TEST_GATE);
        USER_INVALIDATE.store(false, Ordering::Release);
        let _reset = ResetInvalidation;
        let start = Instant::now();
        let mut worker = fake_worker(FakeFrameSource::supported(), true, start, events());
        let mut controller = FakeCaptureAccess::unvalidated(7);
        controller.validated = true;
        controller.last_healthy_ms = Some(0);
        worker.controller = Some(controller);
        worker.latest_admitted.replace(admitted_pixels(1, 0));
        capture_invalidate().unwrap();
        worker.cycle_at(start, 10);
        let mut sinks = vec![FakeFrameSink::default()];

        worker.publish_frame_to(start, 10, &mut sinks);

        assert!(sinks[0].messages.is_empty());
        assert!(worker.user_invalidated);
        assert_eq!(worker.phase(10), "unsupported");
        assert!(worker.latest_admitted.read().is_none());
    }

    #[test]
    fn timeout_refreshes_health_then_stale_and_unavailable_boundaries_apply() {
        let mut controller = FakeCaptureAccess::unvalidated(7);
        controller.validated = true;
        controller.last_healthy_ms = Some(100);
        controller.record_cycle(2100, CycleOutcome::HealthyTimeout);

        assert_eq!(controller.phase(4100), CapturePhase::Live);
        assert_eq!(controller.phase(4101), CapturePhase::Stale);
        assert_eq!(controller.phase(7101), CapturePhase::CoverageUnavailable);
    }

    #[test]
    fn fake_frame_source_timeout_keeps_live_phase_until_stale_thresholds() {
        let start = Instant::now();
        let mut source = FakeFrameSource::supported();
        source.opened = true;
        let mut worker = fake_worker(source, true, start, events());
        worker.source_open = true;
        worker.output = Some(OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        });
        worker.profile_rect = Some(Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        });
        worker.session_epoch = Some(7);
        let mut controller = FakeCaptureAccess::unvalidated(7);
        controller.validated = true;
        controller.last_healthy_ms = Some(100);
        worker.controller = Some(controller);

        worker.capture_one(start + Duration::from_millis(2100), 2100, None);

        let controller = worker.controller.as_ref().unwrap();
        assert_eq!(controller.phase(4100), CapturePhase::Live);
        assert_eq!(controller.phase(4101), CapturePhase::Stale);
        assert_eq!(controller.phase(7101), CapturePhase::CoverageUnavailable);
    }

    #[test]
    fn thirty_fake_structural_passes_admit_the_first_frame() {
        let start = Instant::now();
        let mut worker = calibrating_worker(start, 30, std::iter::repeat_n(true, 30));
        let gsi = GsiSnapshot {
            team: Some("radiant".to_owned()),
            session_epoch: Some(7),
            ..GsiSnapshot::default()
        };

        for index in 0..30_u64 {
            let at = start + Duration::from_millis(index * 33);
            worker.capture_one(at, index * 33, Some(&gsi));
            if index % 3 == 0 {
                worker.analysis_tick(index * 100, Some(&gsi));
            }
        }

        assert!(
            worker
                .controller
                .as_ref()
                .is_some_and(CaptureAccess::crop_visible)
        );
        assert!(worker.latest_admitted.read().is_some());
        assert_eq!(worker.analyzer.calls, 30);
    }

    #[test]
    fn a_structural_failure_resets_calibration_on_that_frame() {
        let start = Instant::now();
        let mut results = vec![true; 29];
        results.push(false);
        let mut worker = calibrating_worker(start, 30, results);
        let gsi = GsiSnapshot {
            team: Some("radiant".to_owned()),
            session_epoch: Some(7),
            ..GsiSnapshot::default()
        };

        for index in 0..30_u64 {
            let at_ms = index * 33;
            worker.capture_one(start + Duration::from_millis(at_ms), at_ms, Some(&gsi));
            if index % 3 == 0 {
                worker.analysis_tick(at_ms, Some(&gsi));
            }
        }

        assert_eq!(worker.analyzer.calls, 30);
        assert_eq!(worker.controller.as_ref().unwrap().structural_passes, 0);
        assert!(!worker.controller.as_ref().unwrap().crop_visible());
        assert!(worker.latest_admitted.read().is_none());
    }

    #[test]
    fn unknown_team_keeps_visible_count_unknown() {
        let report = PixelReport {
            structural_ok: true,
            enemy_blobs: vec![Blob {
                x_milli: 400,
                y_milli: 600,
                area: 5,
            }],
        };
        assert_eq!(enemy_team(None), None);
        assert_eq!(visible_count(Some(&report), None, Coverage::Valid), None);
    }

    #[test]
    fn unknown_team_reaches_risk_engine_as_unknown_visibility() {
        let start = Instant::now();
        let mut worker = fake_worker(FakeFrameSource::supported(), true, start, events());
        let mut controller = FakeCaptureAccess::unvalidated(7);
        controller.validated = true;
        controller.last_healthy_ms = Some(0);
        worker.controller = Some(controller);
        worker.latest_admitted.replace(admitted_pixels(1, 0));
        let visible_samples = worker.risk.seen_visible.clone();

        worker.analysis_tick(0, Some(&GsiSnapshot::default()));

        assert_eq!(&*lock(&visible_samples), &[None]);
    }

    #[test]
    fn stale_gsi_keeps_enemy_visibility_unknown() {
        let start = Instant::now();
        let mut worker = fake_worker(FakeFrameSource::supported(), true, start, events());
        let mut controller = FakeCaptureAccess::unvalidated(7);
        controller.validated = true;
        controller.last_healthy_ms = Some(0);
        worker.controller = Some(controller);
        worker.latest_admitted.replace(admitted_pixels(1, 0));
        let visible_samples = worker.risk.seen_visible.clone();

        worker.analysis_tick(
            0,
            Some(&GsiSnapshot {
                team: Some("radiant".to_owned()),
                age_ms: Some(2000),
                ..GsiSnapshot::default()
            }),
        );
        worker.analysis_tick(
            100,
            Some(&GsiSnapshot {
                team: Some("radiant".to_owned()),
                age_ms: None,
                ..GsiSnapshot::default()
            }),
        );
        worker.analysis_tick(
            200,
            Some(&GsiSnapshot {
                team: Some("radiant".to_owned()),
                age_ms: Some(2001),
                ..GsiSnapshot::default()
            }),
        );

        assert_eq!(&*lock(&visible_samples), &[Some(1), None, None]);
    }

    #[test]
    fn missing_resource_inputs_serialize_as_null() {
        let sample = resource_payload(None, None, None, None);

        for field in [
            "working_set_bytes",
            "cpu_percent",
            "gpu_local_bytes",
            "gpu_group_bytes",
            "gpu_util",
            "capture_fps",
            "frame_age_ms",
        ] {
            assert!(sample[field].is_null(), "{field} must remain unknown");
        }
    }

    #[test]
    fn unsupported_output_still_emits_resource_samples() {
        let _status_gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut source = FakeFrameSource::supported();
        source.profile = None;
        source.opened = true;
        let captured_events = events();
        let event_records = captured_events.0.clone();
        let mut worker = fake_worker(source, true, start, captured_events);
        worker.source_open = true;
        worker.refresh_capture_profile(start, 0, None);
        assert_eq!(worker.phase(0), "unsupported");
        let mut sinks = vec![FakeFrameSink::default()];
        worker.publish_frame_to(start + STATUS_INTERVAL, 500, &mut sinks);
        assert!(sinks[0].messages.is_empty());
        worker.publish_status(start + STATUS_INTERVAL, STATUS_INTERVAL.as_millis() as u64);
        worker.publish_resource(start + RESOURCE_INTERVAL, 1000);

        let records = lock(&event_records);
        let status = records
            .iter()
            .find(|(event, _)| event == "capture://status")
            .map(|(_, payload)| payload)
            .expect("unsupported capture still publishes status");
        assert_eq!(status["phase"], "unsupported");
        let resource = records
            .iter()
            .find(|(event, _)| event == "resource://sample")
            .map(|(_, payload)| payload)
            .expect("unsupported capture still publishes resource state");
        assert!(resource["working_set_bytes"].is_null());
        assert!(resource["cpu_percent"].is_null());
    }

    #[test]
    fn disabled_capture_stays_unsupported_without_opening_the_source() {
        let _status_gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let captured_events = events();
        let event_records = captured_events.0.clone();
        let mut worker = fake_worker(FakeFrameSource::supported(), false, start, captured_events);

        worker.cycle_at(start, 0);

        assert_eq!(worker.phase(0), "unsupported");
        assert!(!worker.source_open);
        assert!(!worker.source.opened);
        let records = lock(&event_records);
        assert!(records.iter().any(|(event, _)| event == "capture://status"));
        assert!(
            records
                .iter()
                .any(|(event, _)| event == "resource://sample")
        );
    }

    #[test]
    fn risk_state_is_emitted_on_change_and_each_second_while_unsupported() {
        let start = Instant::now();
        let captured_events = events();
        let event_records = captured_events.0.clone();
        let mut worker = fake_worker(FakeFrameSource::supported(), false, start, captured_events);

        worker.analysis_tick(0, Some(&GsiSnapshot::default()));
        worker.analysis_tick(
            100,
            Some(&GsiSnapshot {
                alive: Some(true),
                age_ms: Some(0),
                in_match: true,
                ..GsiSnapshot::default()
            }),
        );
        worker.analysis_tick(
            200,
            Some(&GsiSnapshot {
                alive: Some(true),
                age_ms: Some(0),
                ..GsiSnapshot::default()
            }),
        );
        worker.analysis_tick(
            1099,
            Some(&GsiSnapshot {
                alive: Some(true),
                age_ms: Some(0),
                ..GsiSnapshot::default()
            }),
        );
        worker.analysis_tick(
            1100,
            Some(&GsiSnapshot {
                alive: Some(true),
                age_ms: Some(0),
                in_match: true,
                ..GsiSnapshot::default()
            }),
        );

        let records = lock(&event_records);
        let risk_records = records
            .iter()
            .filter(|(event, _)| event == "risk://state")
            .collect::<Vec<_>>();
        assert_eq!(risk_records.len(), 3);
        assert_eq!(risk_records[1].1["meter"], 1);
        assert_eq!(risk_records[1].1["sources"], json!(["gsi"]));
    }

    #[test]
    fn gsi_events_are_passed_once_per_key_without_refreshing_duplicates() {
        let start = Instant::now();
        let mut worker = fake_worker(FakeFrameSource::supported(), false, start, events());
        let gsi_event_batches = worker.risk.seen_gsi_events.clone();
        let original = GsiSnapshot {
            recent_events: vec![
                GsiEvent {
                    id: "health_low",
                    at_ms: 90,
                },
                GsiEvent {
                    id: "health_danger",
                    at_ms: 95,
                },
            ],
            ..GsiSnapshot::default()
        };

        worker.analysis_tick(100, Some(&original));
        worker.analysis_tick(4000, Some(&original));
        let with_new_occurrence = GsiSnapshot {
            recent_events: vec![
                GsiEvent {
                    id: "health_low",
                    at_ms: 90,
                },
                GsiEvent {
                    id: "health_danger",
                    at_ms: 95,
                },
                GsiEvent {
                    id: "health_low",
                    at_ms: 3990,
                },
            ],
            ..GsiSnapshot::default()
        };
        worker.analysis_tick(4000, Some(&with_new_occurrence));

        assert_eq!(
            &*lock(&gsi_event_batches),
            &[
                vec!["health_low", "health_danger"],
                vec![],
                vec!["health_low"],
            ]
        );
    }

    #[test]
    fn gsi_event_dedupe_set_is_bounded() {
        let start = Instant::now();
        let mut worker = fake_worker(FakeFrameSource::supported(), false, start, events());
        let recent_events = (700..1000)
            .map(|at_ms| GsiEvent {
                id: "health_low",
                at_ms,
            })
            .collect();
        let snapshot = GsiSnapshot {
            recent_events,
            ..GsiSnapshot::default()
        };

        worker.analysis_tick(1000, Some(&snapshot));

        assert_eq!(worker.seen_gsi_event_keys.len(), MAX_SEEN_GSI_EVENT_KEYS);
    }

    #[test]
    fn only_supported_health_events_are_passed_to_risk() {
        let start = Instant::now();
        let mut worker = fake_worker(FakeFrameSource::supported(), false, start, events());
        let gsi_event_batches = worker.risk.seen_gsi_events.clone();
        let snapshot = GsiSnapshot {
            recent_events: vec![
                GsiEvent {
                    id: "health_low",
                    at_ms: 100,
                },
                GsiEvent {
                    id: "health_danger",
                    at_ms: 200,
                },
                GsiEvent {
                    id: "other",
                    at_ms: 300,
                },
            ],
            ..GsiSnapshot::default()
        };

        worker.analysis_tick(500, Some(&snapshot));

        assert_eq!(
            &*lock(&gsi_event_batches),
            &[vec!["health_low", "health_danger"]]
        );
    }

    #[test]
    fn open_success_does_not_reset_retry_until_a_frame_grab_succeeds() {
        let start = Instant::now();
        let mut source = FakeFrameSource::supported();
        source.queue_frame(1);
        let mut worker = fake_worker(source, true, start, events());
        worker.retry.next = Some(Duration::from_secs(2));

        worker.ensure_source(start);

        assert_eq!(worker.retry.next, Some(Duration::from_secs(2)));
        worker.profile_rect = Some(Rect { x: 0, y: 0, width: 1, height: 1 });
        worker.session_epoch = Some(7);
        worker.retry.next = Some(Duration::from_secs(4));
        worker.capture_one(start, 0, None);
        assert_eq!(worker.retry.next, None);
    }

    #[test]
    fn capture_loop_reuses_the_bgra_grab_buffer() {
        let start = Instant::now();
        let mut source = FakeFrameSource::supported();
        source.queue_frame(1);
        source.queue_frame(2);
        let mut worker = fake_worker(source, true, start, events());
        worker.ensure_source(start);
        worker.profile_rect = Some(Rect { x: 0, y: 0, width: 1, height: 1 });
        worker.session_epoch = Some(7);

        worker.capture_one(start, 0, None);
        worker.capture_one(start + Duration::from_millis(33), 33, None);

        assert_eq!(worker.source.grab_buffer_addresses.len(), 2);
        assert_eq!(worker.source.grab_buffer_capacities_on_entry[0], 0);
        assert!(worker.source.grab_buffer_capacities_on_entry[1] >= 4);
        assert_eq!(
            worker.source.grab_buffer_addresses[0],
            worker.source.grab_buffer_addresses[1]
        );
    }

    #[test]
    fn dead_frame_subscribers_are_removed() {
        let mut subscribers = vec![
            FakeFrameSink {
                messages: Vec::new(),
                fail: true,
            },
            FakeFrameSink::default(),
        ];

        assert!(broadcast_frame(&mut subscribers, &[1, 2, 3]));
        assert_eq!(subscribers.len(), 1);
        assert_eq!(subscribers[0].messages, [vec![1, 2, 3]]);
    }

    #[test]
    fn retry_backoff_doubles_and_caps_at_five_seconds() {
        let mut backoff = RetryBackoff::default();
        let delays = (0..6).map(|_| backoff.delay()).collect::<Vec<_>>();

        assert_eq!(
            delays,
            [
                Duration::from_millis(500),
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(5),
                Duration::from_secs(5),
            ]
        );
        backoff.reset();
        assert_eq!(backoff.delay(), Duration::from_millis(500));
    }
}
