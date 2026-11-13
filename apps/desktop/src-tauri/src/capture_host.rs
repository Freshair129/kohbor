//! MVP-1 W3 host capture loop, frame transport, and observation/risk event bridge.

use kohbor_capture::{
    AdmittedFrame, CalibrationMethod, CalibrationPhase, CaptureController, CapturePhase,
    CaptureSource, Coverage, CycleOutcome, Environment, FrameHeader, InvalidationReason, Rect,
    Reject, STRUCTURAL_PASS_FRAMES,
};
use kohbor_capture_native::{Grab, MinimapGrabber, OpenError, OutputInfo};
use kohbor_minimap_cv::{FrameProposal, PixelReport, StructuralMode, Team};
use kohbor_risk::{Meter, OwnGsi, RiskEngine, RiskEvent, RiskOutput, TickInput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use std::{fs, path::PathBuf};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::Emitter;

const FRAME_HEADER_LEN: usize = 32;
const FRAME_MAGIC: u32 = 0x4B4D_4150;
const FRAME_VERSION: u16 = 1;
const DISPLAY_INTERVAL: Duration = Duration::from_nanos(33_333_334);
const DEFAULT_DISPLAY_FPS: u8 = 30;
const ANALYSIS_INTERVAL: Duration = Duration::from_millis(100);
const STATUS_INTERVAL: Duration = Duration::from_millis(500);
const RESOURCE_INTERVAL: Duration = Duration::from_secs(1);
const CAPTURE_TIMEOUT_MS: u32 = 15;
const RETRY_INITIAL: Duration = Duration::from_millis(500);
const RETRY_MAX: Duration = Duration::from_secs(5);
const CROP_SEARCH_INFLATION: i32 = 48;
const CROP_SEARCH_MAX_FRAMES: u8 = 5;
const CROP_PROPOSAL_MIN_CONFIDENCE: u16 = 600;
const PACER_SPIN_TAIL: Duration = Duration::from_millis(2);
const CAPTURE_CONFIG_FILE: &str = "capture-config.json";
const CAPTURE_CONFIG_INVALID: &str = "CAPTURE_CONFIG_INVALID";
const CAPTURE_CONFIG_WRITE_FAILED: &str = "CAPTURE_CONFIG_WRITE_FAILED";
const MAX_SEEN_GSI_EVENT_KEYS: usize = 256;
const RISK_HEARTBEAT_MS: u64 = 1000;

static CAPTURE_STATUS: OnceLock<Mutex<Option<Value>>> = OnceLock::new();
static FRAME_SUBSCRIBERS: OnceLock<Mutex<Vec<Channel<InvokeResponseBody>>>> = OnceLock::new();
static CAPTURE_CONFIG: OnceLock<Arc<CaptureConfigStore>> = OnceLock::new();
static FRAME_INVALIDATE_GATE: Mutex<()> = Mutex::new(());
static HOST_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CropOffsets {
    pub dx: i32,
    pub dy: i32,
    pub dw: i32,
    pub dh: i32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    None,
    Searching,
    Found,
    NotFound,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl From<Rect> for ScreenRect {
    fn from(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

impl From<ScreenRect> for Rect {
    fn from(rect: ScreenRect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CaptureConfig {
    pub schema_version: u32,
    pub crop: CropOffsets,
    pub display_fps: u8,
    pub paused: bool,
    pub profile_rect: Option<ScreenRect>,
    pub effective_rect: Option<ScreenRect>,
    pub proposal: Option<ScreenRect>,
    pub proposal_status: ProposalStatus,
    pub crop_confirmed: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            crop: CropOffsets::default(),
            display_fps: DEFAULT_DISPLAY_FPS,
            paused: false,
            profile_rect: None,
            effective_rect: None,
            proposal: None,
            proposal_status: ProposalStatus::None,
            crop_confirmed: false,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CaptureConfigPatch {
    pub crop: Option<CropOffsets>,
    pub display_fps: Option<u8>,
    pub paused: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedCaptureConfig {
    schema_version: u32,
    #[serde(default)]
    profile: Option<String>,
    crop: CropOffsets,
    display_fps: u8,
    crop_confirmed: bool,
}

#[derive(Debug)]
struct CaptureConfigState {
    config: CaptureConfig,
    output: Option<OutputInfo>,
    proposal_search_pending: bool,
    revision: u64,
    retry_generation: u64,
    invalidation: InvalidationState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct InvalidationState {
    invalidated: bool,
    generation: u64,
}

struct CaptureConfigStore {
    data_dir: PathBuf,
    state: Mutex<CaptureConfigState>,
    command_generation: AtomicU64,
}

impl CaptureConfigStore {
    fn open(data_dir: PathBuf) -> Self {
        let persisted = fs::read(data_dir.join(CAPTURE_CONFIG_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PersistedCaptureConfig>(&bytes).ok())
            .filter(|config| {
                config.schema_version == 1 && display_interval(config.display_fps).is_some()
            });
        let config = persisted.map_or_else(CaptureConfig::default, |persisted| {
            let matching_crop = persisted.profile.as_deref() == Some(DEFAULT_PROFILE_NAME)
                && validate_crop_offsets(persisted.crop)
                && persisted.crop.dw.abs_diff(persisted.crop.dh) <= 1
                && [persisted.crop.dw, persisted.crop.dh]
                    .into_iter()
                    .all(|delta| {
                        (64..=400).contains(&(i64::from(DEFAULT_PROFILE_SIDE) + i64::from(delta)))
                    });
            CaptureConfig {
                crop: if matching_crop {
                    persisted.crop
                } else {
                    CropOffsets::default()
                },
                display_fps: persisted.display_fps,
                crop_confirmed: matching_crop && persisted.crop_confirmed,
                ..CaptureConfig::default()
            }
        });
        Self {
            data_dir,
            state: Mutex::new(CaptureConfigState {
                config,
                output: None,
                proposal_search_pending: false,
                revision: 0,
                retry_generation: 0,
                invalidation: InvalidationState::default(),
            }),
            command_generation: AtomicU64::new(0),
        }
    }

    fn get(&self) -> CaptureConfig {
        lock(&self.state).config.clone()
    }

    fn snapshot(&self) -> (CaptureConfig, u64, u64, InvalidationState) {
        let state = lock(&self.state);
        (
            state.config.clone(),
            state.revision,
            state.retry_generation,
            state.invalidation,
        )
    }

    fn invalidation_state(&self) -> InvalidationState {
        lock(&self.state).invalidation
    }

    fn invalidate_user(&self) {
        let mut state = lock(&self.state);
        let _gate = lock(&FRAME_INVALIDATE_GATE);
        state.invalidation.invalidated = true;
        self.advance_command_generation(&mut state);
    }

    // Called while both state and the frame gate are held. The release store is the
    // command's frame-fencing point; readers can compare it without taking state.
    fn advance_command_generation(&self, state: &mut CaptureConfigState) {
        state.invalidation.generation = state.invalidation.generation.saturating_add(1);
        self.command_generation
            .store(state.invalidation.generation, Ordering::Release);
    }

    fn begin_retry(&self, state: &mut CaptureConfigState) {
        let _gate = lock(&FRAME_INVALIDATE_GATE);
        state.retry_generation = state.retry_generation.saturating_add(1);
        state.invalidation.invalidated = false;
        self.advance_command_generation(state);
    }

    fn retry(&self) -> CaptureConfig {
        let mut state = lock(&self.state);
        self.begin_retry(&mut state);
        state.config.clone()
    }

    fn set(&self, patch: CaptureConfigPatch) -> Result<CaptureConfig, String> {
        let layout_action = patch.crop.is_some();
        let mut state = lock(&self.state);
        let mut next = state.config.clone();
        if let Some(crop) = patch.crop {
            next.crop = crop;
            if crop != state.config.crop {
                next.crop_confirmed = false;
                next.proposal = None;
                next.proposal_status = ProposalStatus::None;
            }
        }
        if let Some(display_fps) = patch.display_fps {
            next.display_fps = display_fps;
        }
        if let Some(paused) = patch.paused {
            next.paused = paused;
        }
        if !validate_crop_offsets(next.crop) || display_interval(next.display_fps).is_none() {
            return Err(CAPTURE_CONFIG_INVALID.to_owned());
        }
        if let (Some(profile), Some(output)) = (next.profile_rect, state.output) {
            next.effective_rect = Some(
                effective_rect(profile.into(), next.crop, output)
                    .ok_or_else(|| CAPTURE_CONFIG_INVALID.to_owned())?
                    .into(),
            );
        } else {
            // The only supported profile is square (260 px, p1080-default-v2), even before its
            // output is observed; keep the resulting side inside the contract bounds.
            let side = |delta: i32| i64::from(DEFAULT_PROFILE_SIDE) + i64::from(delta);
            if next.crop.dw.abs_diff(next.crop.dh) > 1
                || !(64..=400).contains(&side(next.crop.dw))
                || !(64..=400).contains(&side(next.crop.dh))
            {
                return Err(CAPTURE_CONFIG_INVALID.to_owned());
            }
        }
        if next == state.config {
            // Nothing changed: do not write (an ignored corrupt file stays untouched).
            if layout_action {
                self.begin_retry(&mut state);
            }
            return Ok(next);
        }
        self.persist(&next)?;
        if next.crop != state.config.crop {
            state.proposal_search_pending = false;
        }
        state.config = next;
        state.revision = state.revision.saturating_add(1);
        if layout_action {
            self.begin_retry(&mut state);
        }
        Ok(state.config.clone())
    }

    fn reset_crop(&self) -> Result<CaptureConfig, String> {
        let mut state = lock(&self.state);
        let mut next = state.config.clone();
        next.crop = CropOffsets::default();
        next.crop_confirmed = false;
        next.proposal = None;
        next.proposal_status = ProposalStatus::None;
        if let (Some(profile), Some(output)) = (next.profile_rect, state.output) {
            next.effective_rect = Some(
                effective_rect(profile.into(), next.crop, output)
                    .ok_or_else(|| CAPTURE_CONFIG_INVALID.to_owned())?
                    .into(),
            );
        }
        self.persist(&next)?;
        state.config = next;
        state.proposal_search_pending = false;
        state.revision = state.revision.saturating_add(1);
        self.begin_retry(&mut state);
        Ok(state.config.clone())
    }

    fn confirm_crop(&self) -> Result<CaptureConfig, String> {
        let mut state = lock(&self.state);
        let mut next = state.config.clone();
        next.crop_confirmed = true;
        self.persist(&next)?;
        state.config = next;
        state.revision = state.revision.saturating_add(1);
        self.begin_retry(&mut state);
        Ok(state.config.clone())
    }

    fn set_profile(&self, profile: Rect, output: OutputInfo) -> (CaptureConfig, bool) {
        let mut state = lock(&self.state);
        let effective = effective_rect(profile, state.config.crop, output);
        let changed = state.config.profile_rect != Some(profile.into())
            || state.config.effective_rect != effective.map(Into::into);
        state.config.profile_rect = Some(profile.into());
        state.config.effective_rect = effective.map(Into::into);
        state.output = Some(output);
        (state.config.clone(), changed)
    }

    fn clear_profile(&self) {
        let mut state = lock(&self.state);
        state.config.profile_rect = None;
        state.config.effective_rect = None;
        state.output = None;
    }

    fn request_proposal(&self) -> CaptureConfig {
        let mut state = lock(&self.state);
        state.config.proposal = None;
        state.config.proposal_status = ProposalStatus::Searching;
        state.proposal_search_pending = true;
        state.revision = state.revision.saturating_add(1);
        state.config.clone()
    }

    fn take_proposal_request(&self) -> bool {
        let mut state = lock(&self.state);
        std::mem::take(&mut state.proposal_search_pending)
    }

    fn set_proposal(&self, proposal: Option<Rect>, status: ProposalStatus) {
        let mut state = lock(&self.state);
        state.config.proposal = proposal.map(Into::into);
        state.config.proposal_status = status;
        state.proposal_search_pending = false;
        state.revision = state.revision.saturating_add(1);
    }

    fn accept_proposal(&self) -> Result<CaptureConfig, String> {
        let mut state = lock(&self.state);
        let proposal = state
            .config
            .proposal
            .filter(|_| state.config.proposal_status == ProposalStatus::Found)
            .ok_or_else(|| CAPTURE_CONFIG_INVALID.to_owned())?;
        let profile = state
            .config
            .profile_rect
            .ok_or_else(|| CAPTURE_CONFIG_INVALID.to_owned())?;
        let output = state
            .output
            .ok_or_else(|| CAPTURE_CONFIG_INVALID.to_owned())?;
        let crop = CropOffsets {
            dx: proposal
                .x
                .checked_sub(profile.x)
                .ok_or_else(|| CAPTURE_CONFIG_INVALID.to_owned())?,
            dy: proposal
                .y
                .checked_sub(profile.y)
                .ok_or_else(|| CAPTURE_CONFIG_INVALID.to_owned())?,
            dw: i32::try_from(proposal.width)
                .ok()
                .and_then(|width| width.checked_sub(i32::try_from(profile.width).ok()?))
                .ok_or_else(|| CAPTURE_CONFIG_INVALID.to_owned())?,
            dh: i32::try_from(proposal.height)
                .ok()
                .and_then(|height| height.checked_sub(i32::try_from(profile.height).ok()?))
                .ok_or_else(|| CAPTURE_CONFIG_INVALID.to_owned())?,
        };
        if !validate_crop_offsets(crop) || effective_rect(profile.into(), crop, output).is_none() {
            return Err(CAPTURE_CONFIG_INVALID.to_owned());
        }
        let mut next = state.config.clone();
        next.crop = crop;
        next.crop_confirmed = true;
        next.effective_rect = effective_rect(profile.into(), crop, output).map(Into::into);
        next.proposal = None;
        next.proposal_status = ProposalStatus::None;
        self.persist(&next)?;
        state.config = next;
        state.proposal_search_pending = false;
        state.revision = state.revision.saturating_add(1);
        self.begin_retry(&mut state);
        Ok(state.config.clone())
    }

    fn persist(&self, config: &CaptureConfig) -> Result<(), String> {
        let persisted = PersistedCaptureConfig {
            schema_version: 1,
            profile: Some(DEFAULT_PROFILE_NAME.to_owned()),
            crop: config.crop,
            display_fps: config.display_fps,
            crop_confirmed: config.crop_confirmed,
        };
        let bytes = serde_json::to_vec_pretty(&persisted)
            .map_err(|_| CAPTURE_CONFIG_WRITE_FAILED.to_owned())?;
        fs::create_dir_all(&self.data_dir).map_err(|_| CAPTURE_CONFIG_WRITE_FAILED.to_owned())?;
        crate::atomic::write(&self.data_dir.join(CAPTURE_CONFIG_FILE), &bytes)
            .map_err(|_| CAPTURE_CONFIG_WRITE_FAILED.to_owned())
    }
}

fn validate_crop_offsets(crop: CropOffsets) -> bool {
    [crop.dx, crop.dy, crop.dw, crop.dh]
        .into_iter()
        .all(|offset| offset.unsigned_abs() <= 200)
}

fn effective_rect(profile: Rect, crop: CropOffsets, output: OutputInfo) -> Option<Rect> {
    if !validate_crop_offsets(crop) {
        return None;
    }
    let x = i64::from(profile.x).checked_add(i64::from(crop.dx))?;
    let y = i64::from(profile.y).checked_add(i64::from(crop.dy))?;
    let width = i64::from(profile.width).checked_add(i64::from(crop.dw))?;
    let height = i64::from(profile.height).checked_add(i64::from(crop.dh))?;
    if !(64..=400).contains(&width)
        || !(64..=400).contains(&height)
        || width.abs_diff(height) > 1
        || x < 0
        || y < 0
        || x.checked_add(width)? > i64::from(output.width)
        || y.checked_add(height)? > i64::from(output.height)
    {
        return None;
    }
    Some(Rect {
        x: i32::try_from(x).ok()?,
        y: i32::try_from(y).ok()?,
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
    })
}

fn inflated_search_rect(profile: Rect, output: OutputInfo) -> Option<Rect> {
    let left = (i64::from(profile.x) - i64::from(CROP_SEARCH_INFLATION)).max(0);
    let top = (i64::from(profile.y) - i64::from(CROP_SEARCH_INFLATION)).max(0);
    let right = (i64::from(profile.x)
        .checked_add(i64::from(profile.width))?
        .checked_add(i64::from(CROP_SEARCH_INFLATION))?)
    .min(i64::from(output.width));
    let bottom = (i64::from(profile.y)
        .checked_add(i64::from(profile.height))?
        .checked_add(i64::from(CROP_SEARCH_INFLATION))?)
    .min(i64::from(output.height));
    if right <= left || bottom <= top {
        return None;
    }
    Some(Rect {
        x: i32::try_from(left).ok()?,
        y: i32::try_from(top).ok()?,
        width: u32::try_from(right - left).ok()?,
        height: u32::try_from(bottom - top).ok()?,
    })
}

fn absolute_proposal(
    search: Rect,
    output: OutputInfo,
    mut proposal: FrameProposal,
) -> Option<Rect> {
    // S4 permits at most a one-pixel side difference. Larger differences are squared by
    // shrinking to the shorter side, preserving the proposed top-left and adding no pixels.
    if proposal.width.abs_diff(proposal.height) > 1 {
        let side = proposal.width.min(proposal.height);
        proposal.width = side;
        proposal.height = side;
    }
    if !(64..=400).contains(&proposal.width) || !(64..=400).contains(&proposal.height) {
        return None;
    }
    let x = i64::from(search.x).checked_add(i64::from(proposal.x))?;
    let y = i64::from(search.y).checked_add(i64::from(proposal.y))?;
    let right = x.checked_add(i64::from(proposal.width))?;
    let bottom = y.checked_add(i64::from(proposal.height))?;
    let search_right = i64::from(search.x).checked_add(i64::from(search.width))?;
    let search_bottom = i64::from(search.y).checked_add(i64::from(search.height))?;
    if proposal.x < 0
        || proposal.y < 0
        || right > search_right
        || bottom > search_bottom
        || x < 0
        || y < 0
        || right > i64::from(output.width)
        || bottom > i64::from(output.height)
    {
        return None;
    }
    Some(Rect {
        x: i32::try_from(x).ok()?,
        y: i32::try_from(y).ok()?,
        width: proposal.width,
        height: proposal.height,
    })
}

fn display_interval(fps: u8) -> Option<Duration> {
    match fps {
        10 => Some(Duration::from_millis(100)),
        15 => Some(Duration::from_nanos(66_666_667)),
        30 => Some(Duration::from_nanos(33_333_334)),
        _ => None,
    }
}

fn parse_config_patch(value: Value) -> Result<CaptureConfigPatch, String> {
    let Some(object) = value.as_object() else {
        return Err(CAPTURE_CONFIG_INVALID.to_owned());
    };
    if ["crop", "display_fps", "paused"]
        .into_iter()
        .any(|key| object.get(key).is_some_and(Value::is_null))
    {
        return Err(CAPTURE_CONFIG_INVALID.to_owned());
    }
    serde_json::from_value(value).map_err(|_| CAPTURE_CONFIG_INVALID.to_owned())
}

fn display_ready(last_sent: Option<Instant>, now: Instant, interval: Duration) -> bool {
    last_sent.is_none_or(|last| {
        now.saturating_duration_since(last)
            .as_nanos()
            .saturating_mul(10)
            >= interval.as_nanos().saturating_mul(9)
    })
}

fn next_deadline(previous: Instant, now: Instant, period: Duration) -> Instant {
    if period.is_zero() {
        return now;
    }
    let Some(candidate) = previous.checked_add(period) else {
        return now;
    };
    if candidate > now {
        return candidate;
    }
    let period_nanos = period.as_nanos();
    let elapsed_nanos = now.saturating_duration_since(previous).as_nanos();
    let steps = elapsed_nanos / period_nanos + 1;
    let advanced_nanos = steps.saturating_mul(period_nanos);
    let seconds = u64::try_from(advanced_nanos / 1_000_000_000).unwrap_or(u64::MAX);
    let subsecond = (advanced_nanos % 1_000_000_000) as u32;
    previous
        .checked_add(Duration::new(seconds, subsecond))
        .unwrap_or(now)
}

struct DeadlineWaiter {
    #[cfg(windows)]
    timer: Option<windows::Win32::Foundation::HANDLE>,
}

impl DeadlineWaiter {
    fn new() -> Self {
        #[cfg(windows)]
        {
            use windows::Win32::System::Threading::{
                CreateWaitableTimerExW, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS,
            };
            let timer = unsafe {
                CreateWaitableTimerExW(
                    None,
                    windows::core::PCWSTR::null(),
                    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                    TIMER_ALL_ACCESS.0,
                )
            }
            .ok();
            Self { timer }
        }
        #[cfg(not(windows))]
        {
            Self {}
        }
    }

    fn wait_until(&mut self, deadline: Instant) {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        let remaining = deadline.saturating_duration_since(now);
        #[cfg(windows)]
        if let Some(timer) = self.timer {
            use windows::Win32::System::Threading::{
                SetWaitableTimer, WaitForSingleObject, INFINITE,
            };
            if remaining > PACER_SPIN_TAIL {
                let wait = remaining.saturating_sub(PACER_SPIN_TAIL);
                let due_ticks = wait.as_nanos().saturating_add(99) / 100;
                if let Ok(due_ticks) = i64::try_from(due_ticks) {
                    let due_time = -due_ticks;
                    if unsafe { SetWaitableTimer(timer, &due_time, 0, None, None, false) }.is_ok() {
                        let _ = unsafe { WaitForSingleObject(timer, INFINITE) };
                        spin_until(deadline);
                        return;
                    }
                }
            }
        }
        if remaining > PACER_SPIN_TAIL {
            thread::sleep(remaining.saturating_sub(PACER_SPIN_TAIL));
        }
        spin_until(deadline);
    }
}

#[cfg(windows)]
impl Drop for DeadlineWaiter {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.take() {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(timer) };
        }
    }
}

fn spin_until(deadline: Instant) {
    while Instant::now() < deadline {
        std::hint::spin_loop();
    }
}

#[derive(Default)]
struct DurationWindow {
    samples: VecDeque<(Instant, Duration)>,
}

impl DurationWindow {
    fn record(&mut self, at: Instant, duration: Duration) {
        self.samples.push_back((at, duration));
        self.prune(at);
    }

    fn mean_ms(&mut self, now: Instant) -> Option<f64> {
        self.prune(now);
        if self.samples.is_empty() {
            return None;
        }
        let total_ms = self
            .samples
            .iter()
            .map(|(_, duration)| duration.as_secs_f64() * 1000.0)
            .sum::<f64>();
        Some(total_ms / self.samples.len() as f64)
    }

    fn prune(&mut self, now: Instant) {
        while self
            .samples
            .front()
            .is_some_and(|(at, _)| now.saturating_duration_since(*at) > Duration::from_secs(1))
        {
            self.samples.pop_front();
        }
    }
}

fn test_config_store() -> Result<Arc<CaptureConfigStore>, String> {
    CAPTURE_CONFIG
        .get()
        .cloned()
        .ok_or_else(|| "CAPTURE_CONFIG_UNAVAILABLE".to_owned())
}

fn capture_status_slot() -> &'static Mutex<Option<Value>> {
    CAPTURE_STATUS.get_or_init(|| Mutex::new(None))
}

const MAX_FRAME_SUBSCRIBERS: usize = 8;
/// Side of the only supported profile rect (kohbor-capture-native p1080-default-v2).
const DEFAULT_PROFILE_SIDE: u32 = 260;
const DEFAULT_PROFILE_NAME: &str = "p1080-default-v2";

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
        "grab_fps": null,
        "sent_total": null,
        "grab_total": null,
        "frame_age_ms": null,
        "rect_profile": null,
        "calibration_generation": null,
        "auto_confirmed": true,
        "crop_confirmed": false,
        "display_fps": DEFAULT_DISPLAY_FPS,
        "paused": false,
        "phase_reason": null,
        "phase_detail": null,
        "grab_ms_mean": null,
        "analysis_ms_mean": null,
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
        "grab_fps": null,
        "sent_total": null,
        "grab_total": null,
        "frame_age_ms": frame_age_ms,
        "rect_profile": rect_profile,
        "calibration_generation": calibration_generation,
        "auto_confirmed": true,
        "crop_confirmed": false,
        "display_fps": DEFAULT_DISPLAY_FPS,
        "paused": false,
        "phase_reason": null,
        "phase_detail": null,
        "grab_ms_mean": null,
        "analysis_ms_mean": null,
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
    fn structural_check(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        mode: StructuralMode,
    ) -> bool;

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
    fn structural_check(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        mode: StructuralMode,
    ) -> bool {
        kohbor_minimap_cv::structural_check_mode(bgra, width, height, stride, mode)
    }

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
            None => PixelReport {
                structural_ok: kohbor_minimap_cv::structural_check(bgra, width, height, stride),
                enemy_blobs: Vec::new(),
            },
        }
    }
}

/// The CV crate is kept behind this seam so all host behavior can be exercised with a fake
/// proposer while the implementation in that crate is still being completed.
trait FrameProposer: Send {
    fn propose(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) -> Option<FrameProposal>;
}

struct CvFrameProposer;

impl FrameProposer for CvFrameProposer {
    fn propose(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) -> Option<FrameProposal> {
        kohbor_minimap_cv::propose_frame(bgra, width, height, stride)
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
    command_generation: u64,
    output_id: u64,
    session_epoch: u64,
    sequence: u64,
    acquired_at_ms: u64,
    width: u32,
    height: u32,
    stride: u32,
    bgra: Arc<[u8]>,
    structural_ok: bool,
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
    interval: Duration,
    max_per_second: u32,
    next_send_at: Option<Instant>,
    last_sequence: Option<u64>,
    sent_rate: RateWindow,
    sent_total: u64,
}

impl DisplayLimiter {
    fn new(started_at: Instant) -> Self {
        Self {
            // The limiter's test/default interval is 30 fps. Runtime cycles set the configured
            // interval from CaptureConfig before publishing.
            interval: DISPLAY_INTERVAL,
            max_per_second: 30,
            next_send_at: None,
            last_sequence: None,
            sent_total: 0,
            sent_rate: RateWindow {
                began: Some(started_at),
                samples: VecDeque::new(),
            },
        }
    }

    fn can_send(&mut self, now: Instant, sequence: u64, crop_visible: bool) -> bool {
        self.sent_rate.prune(now);
        crop_visible
            && self.last_sequence != Some(sequence)
            && self.sent_rate.samples.len() < self.max_per_second as usize
            && display_ready(
                self.next_send_at
                    .and_then(|due| due.checked_sub(self.interval)),
                now,
                self.interval,
            )
    }

    fn set_interval(&mut self, interval: Duration) {
        if interval != self.interval {
            self.next_send_at = None;
        }
        self.interval = interval;
        let interval_nanos = interval.as_nanos().max(1);
        self.max_per_second =
            u32::try_from(1_000_000_000_u128.saturating_add(interval_nanos - 1) / interval_nanos)
                .unwrap_or(u32::MAX)
                .max(1);
    }

    fn record_sent(&mut self, now: Instant, sequence: u64) {
        // Preserve the send schedule across late wakes. Measuring from the last actual
        // send would reject the following on-time frame after a coarse Windows wake.
        self.next_send_at = Some(self.next_send_at.map_or(now + self.interval, |due| {
            next_deadline(due, now + self.interval / 10, self.interval)
        }));
        self.last_sequence = Some(sequence);
        self.sent_total = self.sent_total.saturating_add(1);
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
    config: Arc<CaptureConfigStore>,
    proposer: Box<dyn FrameProposer>,
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
    config: Arc<CaptureConfigStore>,
    proposer: Box<dyn FrameProposer>,
    started: Instant,
    capture_enabled: bool,
    source_open: bool,
    grab_buffer: Vec<u8>,
    proposal_buffer: Vec<u8>,
    proposal_search_rect: Option<Rect>,
    proposal_frames_seen: u8,
    proposal_best: Option<(Rect, u16)>,
    next_sequence: u64,
    next_open_at: Instant,
    retry: RetryBackoff,
    unsupported_reason: Option<&'static str>,
    user_invalidated: bool,
    lost: bool,
    phase_reason: Option<&'static str>,
    phase_detail: Option<String>,
    paused: bool,
    active_crop: CropOffsets,
    config_revision: u64,
    retry_generation: u64,
    command_generation: u64,
    session_epoch: Option<u64>,
    output: Option<OutputInfo>,
    profile_rect: Option<Rect>,
    profile_name: Option<String>,
    environment: Option<Environment>,
    latest_admitted: LatestValue<AdmittedPixels>,
    capture_rate: RateWindow,
    grab_total: u64,
    grab_duration: DurationWindow,
    analysis_duration: DurationWindow,
    display: DisplayLimiter,
    next_analysis: Instant,
    next_status: Instant,
    next_resource: Instant,
    consecutive_passes: u32,
    last_risk_payload: Option<Value>,
    last_risk_emit_ms: Option<u64>,
    seen_gsi_event_keys: HashSet<(&'static str, u64)>,
    seen_gsi_epoch: Option<u64>,
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
        let (config, config_revision, retry_generation, commands) = services.config.snapshot();
        Self {
            source,
            controller: None,
            analyzer: services.analyzer,
            risk: services.risk,
            events: services.events,
            header_codec: services.header_codec,
            gsi: services.gsi,
            resources: services.resources,
            config: services.config,
            proposer: services.proposer,
            started: now,
            capture_enabled: services.capture_enabled,
            source_open: false,
            grab_buffer: Vec::new(),
            proposal_buffer: Vec::new(),
            proposal_search_rect: None,
            proposal_frames_seen: 0,
            proposal_best: None,
            next_sequence: 0,
            next_open_at: now,
            retry: RetryBackoff::default(),
            unsupported_reason: None,
            user_invalidated: false,
            lost: false,
            phase_reason: None,
            phase_detail: None,
            paused: false,
            active_crop: config.crop,
            config_revision,
            retry_generation,
            command_generation: commands.generation,
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
            grab_total: 0,
            grab_duration: DurationWindow::default(),
            analysis_duration: DurationWindow::default(),
            display: DisplayLimiter::new(now),
            next_analysis: now,
            next_status: now,
            next_resource: now,
            consecutive_passes: 0,
            last_risk_payload: None,
            last_risk_emit_ms: None,
            seen_gsi_event_keys: HashSet::new(),
            gsi_event_order: VecDeque::new(),
            seen_gsi_epoch: None,
        }
    }

    fn run(&mut self) -> ! {
        let mut deadline = Instant::now();
        let mut waiter = DeadlineWaiter::new();
        loop {
            let now = Instant::now();
            let now_ms = self.elapsed_ms(now);
            if catch_unwind(AssertUnwindSafe(|| self.cycle_at(now, now_ms))).is_err() {
                self.handle_panic(now, now_ms);
            }
            let config = self.config.get();
            let period = if config.paused
                || !self.capture_enabled
                || self.user_invalidated
                || !self.source_open
                || self.unsupported_reason.is_some()
            {
                ANALYSIS_INTERVAL
            } else {
                display_interval(config.display_fps).unwrap_or(ANALYSIS_INTERVAL)
            };
            deadline = next_deadline(deadline, Instant::now(), period);
            waiter.wait_until(deadline);
        }
    }

    fn elapsed_ms(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.started)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    fn cycle_at(&mut self, now: Instant, now_ms: u64) {
        let config = self.invalidate_for_user(now_ms);
        if let Some(interval) = display_interval(config.display_fps) {
            self.display.set_interval(interval);
        }
        self.paused = config.paused;
        let gsi = catch_unwind(AssertUnwindSafe(|| self.gsi.read())).ok();
        if self.paused {
            self.phase_reason = Some("paused");
            self.clear_frames();
        } else if !self.capture_enabled || self.user_invalidated {
            self.clear_frames();
        } else {
            self.ensure_source(now);
            if self.source_open {
                self.refresh_capture_profile(now, now_ms, gsi.as_ref());
                if self.source_open {
                    self.start_proposal_if_requested();
                    let proposal_grab_due = self.proposal_search_rect.is_some();
                    if proposal_grab_due {
                        self.capture_proposal_frame(now, now_ms);
                    }
                    if !proposal_grab_due
                        && self.unsupported_reason.is_none()
                        && self.session_epoch.is_some()
                    {
                        self.capture_one(now, now_ms, gsi.as_ref());
                    }
                }
            }
        }

        if now >= self.next_analysis {
            self.analysis_tick(now_ms, gsi.as_ref());
            self.next_analysis = next_deadline(self.next_analysis, now, ANALYSIS_INTERVAL);
        }
        self.publish_frame(now, now_ms);
        if now >= self.next_status {
            self.publish_status(now, now_ms);
            self.next_status = next_deadline(self.next_status, now, STATUS_INTERVAL);
        }
        if now >= self.next_resource {
            self.publish_resource(now, now_ms);
            self.next_resource = next_deadline(self.next_resource, now, RESOURCE_INTERVAL);
        }
    }

    /// Consume one coherent command snapshot; never write an observed invalidation
    /// back into the store, since a newer retry may already have cleared it.
    fn invalidate_for_user(&mut self, now_ms: u64) -> CaptureConfig {
        let (config, revision, retry_generation, commands) = self.config.snapshot();
        if retry_generation != self.retry_generation {
            self.retry_generation = retry_generation;
            self.user_invalidated = false;
            self.unsupported_reason = None;
            self.profile_rect = None;
            self.clear_frames();
            if let Some(controller) = self.controller.as_mut() {
                controller.invalidate_user(now_ms);
            }
            self.lost = false;
            self.phase_reason = Some("calibrating");
            self.phase_detail = None;
            self.next_open_at = self
                .started
                .checked_add(Duration::from_millis(now_ms))
                .unwrap_or(self.started);
            self.proposal_search_rect = None;
            self.proposal_best = None;
            self.proposal_frames_seen = 0;
            if config.proposal_status == ProposalStatus::Searching {
                self.config.set_proposal(None, ProposalStatus::None);
            }
        }
        if commands.invalidated
            && (!self.user_invalidated || commands.generation != self.command_generation)
        {
            if let Some(controller) = self.controller.as_mut() {
                controller.invalidate_user(now_ms);
            }
            self.clear_frames();
        }
        self.user_invalidated = commands.invalidated;
        self.command_generation = commands.generation;

        if revision != self.config_revision {
            if config.crop != self.active_crop {
                self.profile_rect = None;
                self.clear_frames();
                self.proposal_search_rect = None;
                self.proposal_frames_seen = 0;
                self.proposal_best = None;
                if config.proposal_status == ProposalStatus::Searching {
                    self.config.set_proposal(None, ProposalStatus::None);
                }
            }
            self.active_crop = config.crop;
            self.config_revision = revision;
        }
        self.paused = config.paused;
        config
    }

    fn ensure_source(&mut self, now: Instant) {
        if self.source_open || now < self.next_open_at {
            return;
        }
        self.phase_reason = Some("reopening");
        self.phase_detail = None;
        match self.source.open() {
            Ok(()) => {
                self.source_open = true;
                self.output = self.source.output();
                self.phase_reason = Some("calibrating");
            }
            Err(error) => {
                self.mark_lost(now);
                self.unsupported_reason = match error {
                    OpenError::NoGameWindow => Some("no_game_window"),
                    OpenError::Unsupported(message) => {
                        self.phase_detail = Some(message);
                        Some("capture_unavailable")
                    }
                    OpenError::Os(_) => None,
                };
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
        self.phase_reason = Some("reopening");
        let now_ms = self.elapsed_ms(now);
        if let Some(controller) = self.controller.as_mut() {
            controller.record_cycle(now_ms, CycleOutcome::AccessLost);
        }
        self.clear_frames();
    }

    fn refresh_capture_profile(&mut self, now: Instant, now_ms: u64, gsi: Option<&GsiSnapshot>) {
        self.invalidate_for_user(now_ms);
        let Some(output) = self.source.output() else {
            self.mark_lost(now);
            self.source.close();
            self.source_open = false;
            self.schedule_retry(now);
            return;
        };
        self.phase_detail = None;
        let Some((rect, profile)) = self.source.profile_rect(&output) else {
            self.unsupported_reason = Some("unsupported_resolution");
            self.profile_rect = None;
            self.profile_name = None;
            self.config.clear_profile();
            self.clear_frames();
            // OutputInfo is cached by the native grabber. Reopen after backoff so a
            // restored resolution/DPI can be observed without restarting the app.
            self.source.close();
            self.source_open = false;
            self.schedule_retry(now);
            return;
        };
        let (config, geometry_changed) = self.config.set_profile(rect, output);
        if geometry_changed && self.proposal_search_rect.is_some() {
            self.proposal_search_rect = None;
            self.proposal_frames_seen = 0;
            self.proposal_best = None;
            self.config.set_proposal(None, ProposalStatus::NotFound);
        }
        let Some(effective_rect) = config.effective_rect.map(Rect::from) else {
            self.unsupported_reason = Some("crop_out_of_bounds");
            self.profile_rect = None;
            self.profile_name = Some(profile);
            self.clear_frames();
            return;
        };
        let was_unsupported = self.unsupported_reason.is_some();
        let Some(window_rect) = self.source.game_window() else {
            self.unsupported_reason = Some("no_game_window");
            self.profile_rect = Some(effective_rect);
            self.profile_name = Some(profile);
            self.clear_frames();
            return;
        };

        self.unsupported_reason = None;
        if was_unsupported {
            self.phase_reason = Some("calibrating");
        }
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
        let new_rect = self.profile_rect != Some(effective_rect);
        let new_profile = self.profile_name.as_deref() != Some(profile.as_str());
        let new_environment = self.environment.as_ref() != Some(&environment);
        let changed = was_unsupported || new_rect || new_profile || new_environment;
        self.profile_rect = Some(effective_rect);
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
            if !controller.begin(environment.clone(), effective_rect, profile) {
                self.unsupported_reason = Some("crop_out_of_bounds");
                self.clear_frames();
                return;
            }
            self.clear_frames();
            self.consecutive_passes = 0;
        } else if matches!(
            controller.calibration_phase(),
            CalibrationPhase::Uncalibrated | CalibrationPhase::RecalibrationNeeded
        ) {
            if !controller.begin(environment.clone(), effective_rect, profile) {
                self.unsupported_reason = Some("crop_out_of_bounds");
                self.clear_frames();
                return;
            }
            self.consecutive_passes = 0;
        }
        self.environment = Some(environment);
    }

    fn start_proposal_if_requested(&mut self) {
        if !self.config.take_proposal_request() {
            return;
        }
        let config = self.config.get();
        let Some(profile) = config.profile_rect.map(Rect::from) else {
            self.config.set_proposal(None, ProposalStatus::NotFound);
            return;
        };
        let Some(output) = self.output.or_else(|| self.source.output()) else {
            self.config.set_proposal(None, ProposalStatus::NotFound);
            return;
        };
        let Some(search) = inflated_search_rect(profile, output) else {
            self.config.set_proposal(None, ProposalStatus::NotFound);
            return;
        };
        self.proposal_search_rect = Some(search);
        self.proposal_frames_seen = 0;
        self.proposal_best = None;
    }

    fn capture_proposal_frame(&mut self, now: Instant, now_ms: u64) {
        let command_generation = self.config.command_generation.load(Ordering::Acquire);
        if self.paused || self.user_invalidated || command_generation != self.command_generation {
            self.clear_frames();
            return;
        }
        let (Some(search_rect), Some(output)) = (self.proposal_search_rect, self.output) else {
            return;
        };
        if self.proposal_frames_seen >= CROP_SEARCH_MAX_FRAMES {
            self.finish_proposal_search();
            return;
        }
        self.proposal_buffer.clear();
        self.proposal_frames_seen = self.proposal_frames_seen.saturating_add(1);
        let grab_started = Instant::now();
        let result = self
            .source
            .grab(search_rect, CAPTURE_TIMEOUT_MS, &mut self.proposal_buffer);
        let grab_finished = Instant::now();
        self.grab_duration.record(
            grab_finished,
            grab_finished.saturating_duration_since(grab_started),
        );
        if self.config.command_generation.load(Ordering::Acquire) != command_generation {
            self.clear_frames();
            return;
        }
        match result {
            Grab::Frame {
                width,
                height,
                stride,
                ..
            } => {
                let expected_len = usize::try_from(height)
                    .ok()
                    .and_then(|height| height.checked_mul(stride as usize));
                if width == search_rect.width
                    && height == search_rect.height
                    && stride == width.saturating_mul(4)
                    && expected_len == Some(self.proposal_buffer.len())
                {
                    self.record_grab(now);
                    self.retry.reset();
                    if let Some(proposal) =
                        self.proposer
                            .propose(&self.proposal_buffer, width, height, stride)
                    {
                        if proposal.confidence_milli > CROP_PROPOSAL_MIN_CONFIDENCE {
                            if let Some(absolute) = absolute_proposal(search_rect, output, proposal)
                            {
                                let replace = self.proposal_best.is_none_or(|(_, confidence)| {
                                    confidence < proposal.confidence_milli
                                });
                                if replace {
                                    self.proposal_best =
                                        Some((absolute, proposal.confidence_milli));
                                }
                            }
                        }
                    }
                }
            }
            Grab::Timeout => {
                if let Some(controller) = self.controller.as_mut() {
                    controller.record_cycle(now_ms, CycleOutcome::HealthyTimeout);
                }
            }
            Grab::AccessLost | Grab::Failed(_) => {
                self.config.set_proposal(None, ProposalStatus::NotFound);
                self.proposal_search_rect = None;
                if let Some(controller) = self.controller.as_mut() {
                    controller.record_cycle(now_ms, CycleOutcome::AccessLost);
                }
                self.source.close();
                self.source_open = false;
                self.schedule_retry(now);
                self.lost = true;
                self.phase_reason = Some("access_lost");
                self.clear_frames();
                return;
            }
        }
        if self.proposal_frames_seen >= CROP_SEARCH_MAX_FRAMES {
            self.finish_proposal_search();
        }
    }

    fn finish_proposal_search(&mut self) {
        let proposal = self.proposal_best.take().map(|(rect, _)| rect);
        let status = if proposal.is_some() {
            ProposalStatus::Found
        } else {
            ProposalStatus::NotFound
        };
        self.config.set_proposal(proposal, status);
        self.proposal_search_rect = None;
        self.proposal_frames_seen = 0;
    }

    fn record_grab(&mut self, now: Instant) {
        self.capture_rate.record(now);
        self.grab_total = self.grab_total.saturating_add(1);
    }

    fn capture_one(&mut self, now: Instant, now_ms: u64, _gsi: Option<&GsiSnapshot>) {
        let command_generation = self.config.command_generation.load(Ordering::Acquire);
        if self.paused || self.user_invalidated || command_generation != self.command_generation {
            self.clear_frames();
            return;
        }
        let (Some(rect), Some(output), Some(session_epoch)) =
            (self.profile_rect, self.output, self.session_epoch)
        else {
            return;
        };
        self.grab_buffer.clear();
        let grab_started = Instant::now();
        let grab = self
            .source
            .grab(rect, CAPTURE_TIMEOUT_MS, &mut self.grab_buffer);
        let grab_finished = Instant::now();
        self.grab_duration.record(
            grab_finished,
            grab_finished.saturating_duration_since(grab_started),
        );
        match grab {
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
                self.phase_reason = Some("access_lost");
                self.clear_frames();
            }
            Grab::Failed(_error) => {
                if let Some(controller) = self.controller.as_mut() {
                    controller.record_cycle(now_ms, CycleOutcome::AccessLost);
                }
                self.source.close();
                self.source_open = false;
                self.schedule_retry(now);
                self.lost = true;
                self.phase_reason = Some("reopening");
                self.clear_frames();
            }
            Grab::Frame {
                width,
                height,
                stride,
                acquired_us: _,
            } => {
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
                self.record_grab(now);
                if self.config.command_generation.load(Ordering::Acquire) != command_generation {
                    self.clear_frames();
                    return;
                }
                self.retry.reset();
                self.lost = false;
                self.phase_reason = Some("calibrating");
                // Profile IDs carry crop semantics: v2 excludes the HUD frame. Other
                // profiles retain Framed validation, including future framed profiles.
                let mode = if self.profile_name.as_deref() == Some(DEFAULT_PROFILE_NAME) {
                    StructuralMode::Inner
                } else {
                    StructuralMode::Framed
                };
                let structural_ok =
                    self.analyzer
                        .structural_check(&self.grab_buffer, width, height, stride, mode);
                let was_crop_visible = self
                    .controller
                    .as_ref()
                    .is_some_and(CaptureAccess::crop_visible);
                let calibration_phase = self
                    .controller
                    .as_ref()
                    .map(CaptureAccess::calibration_phase);
                if let Some(controller) = self.controller.as_mut() {
                    controller.structural_frame(structural_ok);
                }
                if calibration_phase == Some(CalibrationPhase::Calibrating) {
                    if structural_ok {
                        self.consecutive_passes = self
                            .consecutive_passes
                            .saturating_add(1)
                            .min(STRUCTURAL_PASS_FRAMES);
                    } else {
                        self.consecutive_passes = 0;
                    }
                }
                let mut crop_visible = self
                    .controller
                    .as_ref()
                    .is_some_and(CaptureAccess::crop_visible);
                if was_crop_visible && !crop_visible {
                    self.latest_admitted.clear();
                    self.consecutive_passes = 0;
                }
                if !crop_visible && self.consecutive_passes >= STRUCTURAL_PASS_FRAMES {
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
                if crop_visible {
                    self.phase_reason = None;
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
                    command_generation,
                    output_id: output.output_id,
                    session_epoch,
                    sequence,
                    acquired_at_ms: now_ms,
                    width,
                    height,
                    stride,
                    bgra: Arc::from(self.grab_buffer.as_slice()),
                    structural_ok,
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
            controller.record_cycle(now_ms, CycleOutcome::AccessLost);
        }
        self.consecutive_passes = 0;
        self.source.close();
        self.source_open = false;
        self.schedule_retry(now);
        self.lost = true;
        self.phase_reason = Some("reopening");
        self.clear_frames();
    }

    fn admit_candidate(&mut self, candidate: CandidateFrame, now_ms: u64) -> bool {
        let _invalidate_gate = lock(&FRAME_INVALIDATE_GATE);
        if self.user_invalidated
            || candidate.command_generation
                != self.config.command_generation.load(Ordering::Acquire)
        {
            self.latest_admitted.clear();
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
        let commands = self.config.invalidation_state();
        let externally_invalidated =
            commands.invalidated || commands.generation != self.command_generation;
        let enemy = fresh_enemy_team(gsi);
        let crop_visible = self
            .controller
            .as_ref()
            .is_some_and(CaptureAccess::crop_visible)
            && !externally_invalidated
            && !self.user_invalidated;
        let latest_admitted = crop_visible.then(|| self.latest_admitted.read()).flatten();

        let mut coverage = if self.paused
            || externally_invalidated
            || self.user_invalidated
            || !self.capture_enabled
            || self.unsupported_reason.is_some()
        {
            Coverage::Unavailable
        } else {
            self.controller
                .as_ref()
                .map_or(Coverage::Unavailable, |controller| {
                    controller.coverage(now_ms)
                })
        };
        let visible = latest_admitted.as_ref().and_then(|frame| {
            if coverage != Coverage::Valid
                || !frame.candidate.structural_ok
                || frame.candidate.command_generation != commands.generation
            {
                return None;
            }
            let pixels = &frame.candidate;
            let started = Instant::now();
            let mut report = self.analyzer.analyze(
                &pixels.bgra,
                pixels.width,
                pixels.height,
                pixels.stride,
                enemy,
            );
            // Inner crops have no HUD border. Their admitted structural state, rather
            // than analyze()'s Framed diagnostic, authorizes counts for this valid tick.
            if self.profile_name.as_deref() == Some(DEFAULT_PROFILE_NAME) {
                report.structural_ok = pixels.structural_ok;
            }
            let finished = Instant::now();
            self.analysis_duration
                .record(finished, finished.duration_since(started));
            if self.config.command_generation.load(Ordering::Acquire) != pixels.command_generation {
                return None;
            }
            visible_count(Some(&report), enemy, coverage)
        });
        if self.config.command_generation.load(Ordering::Acquire) != commands.generation {
            coverage = Coverage::Unavailable;
        }
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
        // A different match (session epoch) starts a fresh key space.
        if snapshot.session_epoch != self.seen_gsi_epoch {
            self.seen_gsi_epoch = snapshot.session_epoch;
            self.seen_gsi_event_keys.clear();
            self.gsi_event_order.clear();
        }
        // Stale GSI is unknown: do not consume events the risk engine would ignore.
        if snapshot.age_ms.is_none_or(|age| age > 2000) {
            return new_events;
        }
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

    fn clear_frames(&mut self) {
        self.latest_admitted.clear();
        self.consecutive_passes = 0;
    }

    fn phase(&self, now_ms: u64) -> &'static str {
        let commands = self.config.invalidation_state();
        if self.paused {
            return "paused";
        }
        if !self.capture_enabled || commands.invalidated {
            return "unsupported";
        }
        if commands.generation != self.command_generation {
            return "calibrating";
        }
        if self.unsupported_reason.is_some() {
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

    fn phase_reason(&self, now_ms: u64) -> Option<&'static str> {
        let commands = self.config.invalidation_state();
        if self.paused {
            return Some("paused");
        }
        if !self.capture_enabled {
            return Some("capture_disabled");
        }
        if commands.invalidated {
            return Some("user_invalidated");
        }
        if commands.generation != self.command_generation {
            return Some("calibrating");
        }
        if let Some(reason) = self.unsupported_reason {
            return Some(reason);
        }
        if self.lost {
            return Some(self.phase_reason.unwrap_or("reopening"));
        }
        if self
            .controller
            .as_ref()
            .is_none_or(|controller| controller.phase(now_ms) == CapturePhase::Calibrating)
        {
            return Some("calibrating");
        }
        self.phase_reason
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
        if self.paused {
            return;
        }
        let _invalidate_gate = lock(&FRAME_INVALIDATE_GATE);
        if self.user_invalidated {
            return;
        }
        let Some(frame) = self.latest_admitted.read() else {
            return;
        };
        if frame.candidate.command_generation
            != self.config.command_generation.load(Ordering::Acquire)
        {
            return;
        }
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
        let config = self.config.get();
        let mut payload = status_payload(
            self.phase(now_ms),
            self.display.sent_rate.rate(now),
            self.frame_age_ms(now_ms),
            self.profile_name.as_deref(),
            self.controller.as_ref().and_then(CaptureAccess::generation),
        );
        payload["crop_confirmed"] = json!(config.crop_confirmed);
        payload["grab_fps"] = json!(self.capture_rate.rate(now));
        payload["sent_total"] = json!(self.display.sent_total);
        payload["grab_total"] = json!(self.grab_total);
        payload["display_fps"] = json!(config.display_fps);
        payload["paused"] = json!(config.paused);
        payload["phase_reason"] = json!(self.phase_reason(now_ms));
        payload["phase_detail"] = json!(self.phase_detail.as_deref());
        payload["grab_ms_mean"] = json!(self.grab_duration.mean_ms(now));
        payload["analysis_ms_mean"] = json!(self.analysis_duration.mean_ms(now));
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
        self.phase_detail = None;
        self.lost = true;
        self.phase_reason = Some("reopening");
        self.clear_frames();
        let _ = catch_unwind(AssertUnwindSafe(|| self.source.close()));
        self.source_open = false;
        self.schedule_retry(now);
        let config = self.config.get();
        let mut payload = status_payload(
            "lost",
            self.display.sent_rate.rate(now),
            self.frame_age_ms(now_ms),
            self.profile_name.as_deref(),
            self.controller.as_ref().and_then(CaptureAccess::generation),
        );
        payload["crop_confirmed"] = json!(config.crop_confirmed);
        payload["grab_fps"] = json!(self.capture_rate.rate(now));
        payload["sent_total"] = json!(self.display.sent_total);
        payload["grab_total"] = json!(self.grab_total);
        payload["display_fps"] = json!(config.display_fps);
        payload["paused"] = json!(config.paused);
        payload["phase_reason"] = json!(self.phase_reason(now_ms));
        payload["phase_detail"] = json!(self.phase_detail.as_deref());
        payload["grab_ms_mean"] = json!(self.grab_duration.mean_ms(now));
        payload["analysis_ms_mean"] = json!(self.analysis_duration.mean_ms(now));
        *lock(capture_status_slot()) = Some(payload.clone());
    }
}

fn run_capture_host(
    app: tauri::AppHandle,
    gsi: GsiView,
    resources: ResourceView,
    config: Arc<CaptureConfigStore>,
) {
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
        config,
        proposer: Box::new(CvFrameProposer),
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
pub fn start(app: tauri::AppHandle, gsi: GsiView, resources: ResourceView, data_dir: PathBuf) {
    if HOST_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let config = CAPTURE_CONFIG
        .get_or_init(|| Arc::new(CaptureConfigStore::open(data_dir)))
        .clone();
    let worker_app = app.clone();
    if let Err(error) = thread::Builder::new()
        .name("capture-host".to_owned())
        .spawn(move || {
            crate::host::set_current_thread_priority(crate::host::ThreadPriority::BelowNormal);
            run_capture_host(worker_app, gsi, resources, config);
        })
    {
        let status = status_payload("lost", None, None, None, None);
        *lock(capture_status_slot()) = Some(status.clone());
        app.emit_value("capture://status", status);
        eprintln!("capture-host unavailable: {error}");
    }
}

#[tauri::command(async)]
pub(crate) async fn capture_get_config() -> CaptureConfig {
    CAPTURE_CONFIG
        .get()
        .map_or_else(CaptureConfig::default, |store| store.get())
}

#[tauri::command(async)]
pub(crate) async fn capture_set_config(patch: Value) -> Result<CaptureConfig, String> {
    test_config_store()?.set(parse_config_patch(patch)?)
}

#[tauri::command(async)]
pub(crate) async fn capture_reset_crop() -> Result<CaptureConfig, String> {
    test_config_store()?.reset_crop()
}

#[tauri::command(async)]
pub(crate) async fn capture_propose_crop() -> Result<CaptureConfig, String> {
    Ok(test_config_store()?.request_proposal())
}

#[tauri::command(async)]
pub(crate) async fn capture_accept_proposal() -> Result<CaptureConfig, String> {
    test_config_store()?.accept_proposal()
}

#[tauri::command(async)]
pub(crate) async fn capture_confirm_crop() -> Result<CaptureConfig, String> {
    test_config_store()?.confirm_crop()
}

#[tauri::command(async)]
pub(crate) async fn capture_retry() -> Result<CaptureConfig, String> {
    Ok(test_config_store()?.retry())
}

#[tauri::command(async)]
pub(crate) fn minimap_subscribe(on_frame: Channel<InvokeResponseBody>) -> Result<(), String> {
    let mut subscribers = lock(frame_subscribers());
    subscribers.push(on_frame);
    // No unsubscribe path exists: keep only the newest subscribers so a remounting
    // renderer cannot grow this list without bound while no frames are sent.
    while subscribers.len() > MAX_FRAME_SUBSCRIBERS {
        subscribers.remove(0);
    }
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
    test_config_store()?.invalidate_user();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kohbor_capture::STRUCTURAL_FAIL_FRAMES;
    use kohbor_minimap_cv::Blob;
    use serde_json::json;

    static STATUS_TEST_GATE: Mutex<()> = Mutex::new(());

    fn test_config_dir(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!(
            "kohbor-capture-config-{}-{name}-{nonce}",
            std::process::id()
        ))
    }

    #[derive(Clone)]
    struct FakeFrameSource {
        output: Option<OutputInfo>,
        profile: Option<(Rect, String)>,
        window: Option<Rect>,
        gpu_local: Option<u64>,
        open_error: Option<OpenError>,
        opened: bool,
        open_calls: usize,
        grabs: VecDeque<(Grab, Vec<u8>)>,
        grabbed_rects: Vec<Rect>,
        grab_buffer_addresses: Vec<usize>,
        grab_buffer_capacities_on_entry: Vec<usize>,
        after_grab: Option<Arc<dyn Fn() + Send + Sync>>,
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
                open_calls: 0,
                grabs: VecDeque::new(),
                grabbed_rects: Vec::new(),
                grab_buffer_addresses: Vec::new(),
                grab_buffer_capacities_on_entry: Vec::new(),
                after_grab: None,
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

        fn queue_sized_frame(&mut self, width: u32, height: u32, sequence: u64) {
            let stride = width.saturating_mul(4);
            let len = usize::try_from(height)
                .ok()
                .and_then(|height| height.checked_mul(stride as usize))
                .unwrap_or_default();
            self.grabs.push_back((
                Grab::Frame {
                    width,
                    height,
                    stride,
                    acquired_us: sequence.saturating_mul(1000),
                },
                vec![sequence as u8; len],
            ));
        }
    }

    impl FrameSource for FakeFrameSource {
        fn open(&mut self) -> Result<(), OpenError> {
            self.open_calls += 1;
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

        fn grab(&mut self, rect: Rect, _timeout_ms: u32, out: &mut Vec<u8>) -> Grab {
            self.grabbed_rects.push(rect);
            let Some((result, pixels)) = self.grabs.pop_front() else {
                return Grab::Timeout;
            };
            if matches!(result, Grab::Frame { .. }) {
                self.grab_buffer_capacities_on_entry.push(out.capacity());
                out.extend_from_slice(&pixels);
                self.grab_buffer_addresses.push(out.as_ptr() as usize);
            }
            if let Some(hook) = self.after_grab.take() {
                hook();
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
        reject_begin: bool,
        epoch: u64,
        generation: u64,
        validated: bool,
        invalidated: bool,
        unsupported: bool,
        lost: bool,
        structural_passes: u32,
        structural_failures: u32,
        last_healthy_ms: Option<u64>,
    }

    impl FakeCaptureAccess {
        fn unvalidated(epoch: u64) -> Self {
            Self {
                reject_begin: false,
                epoch,
                generation: 0,
                validated: false,
                invalidated: false,
                unsupported: false,
                lost: false,
                structural_passes: 0,
                structural_failures: 0,
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
            if self.reject_begin {
                return false;
            }
            self.generation += 1;
            self.validated = false;
            self.invalidated = false;
            self.unsupported = false;
            self.structural_passes = 0;
            self.structural_failures = 0;
            true
        }

        fn observe_environment(&mut self, _environment: &Environment) {}

        fn structural_frame(&mut self, passed: bool) {
            if self.validated {
                if passed {
                    self.structural_failures = 0;
                } else {
                    self.structural_failures = self.structural_failures.saturating_add(1);
                    if self.structural_failures >= STRUCTURAL_FAIL_FRAMES {
                        self.validated = false;
                        self.lost = true;
                        self.generation += 1;
                    }
                }
            } else if passed {
                self.structural_passes = self
                    .structural_passes
                    .saturating_add(1)
                    .min(STRUCTURAL_PASS_FRAMES);
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
                    self.invalidated = true;
                    self.generation += 1;
                    self.last_healthy_ms = None;
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
            } else if self.invalidated {
                CalibrationPhase::RecalibrationNeeded
            } else {
                CalibrationPhase::Calibrating
            }
        }
    }

    #[derive(Default)]
    struct FakeAnalyzer {
        calls: usize,
        input_sizes: Vec<(u32, u32, u32)>,
        input_first_bytes: Vec<Option<u8>>,
        structural_calls: usize,
        structural_input_sizes: Vec<(u32, u32, u32)>,
        structural_modes: Vec<StructuralMode>,
        structural_results: VecDeque<bool>,
    }

    impl FrameAnalyzer for FakeAnalyzer {
        fn structural_check(
            &mut self,
            _bgra: &[u8],
            width: u32,
            height: u32,
            stride: u32,
            mode: StructuralMode,
        ) -> bool {
            self.structural_calls += 1;
            self.structural_input_sizes.push((width, height, stride));
            self.structural_modes.push(mode);
            self.structural_results.pop_front().unwrap_or(true)
        }

        fn analyze(
            &mut self,
            bgra: &[u8],
            width: u32,
            height: u32,
            stride: u32,
            _enemy: Option<Team>,
        ) -> PixelReport {
            self.calls += 1;
            self.input_sizes.push((width, height, stride));
            self.input_first_bytes.push(bgra.first().copied());
            PixelReport {
                structural_ok: true,
                enemy_blobs: vec![Blob {
                    x_milli: 500,
                    y_milli: 500,
                    area: 4,
                }],
            }
        }
    }

    struct FakeFrameProposer {
        proposals: Arc<Mutex<VecDeque<Option<FrameProposal>>>>,
        inputs: Arc<Mutex<Vec<(u32, u32, u32)>>>,
    }

    impl Default for FakeFrameProposer {
        fn default() -> Self {
            Self {
                proposals: Arc::new(Mutex::new(VecDeque::new())),
                inputs: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl FrameProposer for FakeFrameProposer {
        fn propose(
            &mut self,
            _bgra: &[u8],
            width: u32,
            height: u32,
            stride: u32,
        ) -> Option<FrameProposal> {
            lock(&self.inputs).push((width, height, stride));
            lock(&self.proposals).pop_front().flatten()
        }
    }

    #[derive(Clone)]
    struct FakeRisk {
        seen_visible: Arc<Mutex<Vec<Option<u8>>>>,
        seen_gsi_events: Arc<Mutex<Vec<Vec<&'static str>>>>,
        seen_coverage: Arc<Mutex<Vec<Coverage>>>,
    }

    impl RiskAccess for FakeRisk {
        fn tick(&mut self, input: &TickInput) -> RiskOutput {
            lock(&self.seen_visible).push(input.visible);
            lock(&self.seen_gsi_events).push(input.gsi_events.clone());
            lock(&self.seen_coverage).push(input.coverage);
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
                    seen_coverage: Arc::new(Mutex::new(Vec::new())),
                },
                events,
                header_codec: MvpFrameHeaderCodec,
                gsi: GsiView::new(GsiSnapshot::default),
                resources: ResourceView::new(ResourceSample::default),
                capture_enabled,
                config: Arc::new(CaptureConfigStore::open(test_config_dir("worker"))),
                proposer: Box::new(FakeFrameProposer::default()),
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

    fn worker_with_profile(
        now: Instant,
        profile: Rect,
    ) -> CaptureHostWorker<
        FakeFrameSource,
        FakeCaptureAccess,
        FakeAnalyzer,
        FakeRisk,
        FakeEvents,
        MvpFrameHeaderCodec,
    > {
        let mut source = FakeFrameSource::supported();
        source.profile = Some((profile, "fake-profile".to_owned()));
        source.opened = true;
        let mut worker = fake_worker(source, true, now, events());
        worker.source_open = true;
        worker.output = Some(OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        });
        let gsi = GsiSnapshot {
            session_epoch: Some(7),
            age_ms: Some(0),
            ..GsiSnapshot::default()
        };
        worker.refresh_capture_profile(now, 0, Some(&gsi));
        worker
    }

    fn events() -> FakeEvents {
        FakeEvents(Arc::new(Mutex::new(Vec::new())))
    }

    fn admitted_pixels(sequence: u64, at_ms: u64) -> AdmittedPixels {
        let candidate = CandidateFrame {
            command_generation: 0,
            output_id: 4,
            session_epoch: 7,
            sequence,
            acquired_at_ms: at_ms,
            width: 1,
            height: 1,
            stride: 4,
            bgra: Arc::from([1_u8, 2, 3, 255]),
            structural_ok: true,
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

        worker.config.invalidate_user();
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

        worker.config.invalidate_user();
        worker.invalidate_for_user(10);

        assert!(worker.user_invalidated);
        assert_eq!(worker.phase(10), "unsupported");
        assert!(worker.latest_admitted.read().is_none());
        assert!(!worker.controller.as_ref().unwrap().crop_visible());
    }

    #[test]
    fn user_invalidation_prevents_later_frame_publication() {
        let _status_gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fake_worker(FakeFrameSource::supported(), true, start, events());
        let mut controller = FakeCaptureAccess::unvalidated(7);
        controller.validated = true;
        controller.last_healthy_ms = Some(0);
        worker.controller = Some(controller);
        worker.latest_admitted.replace(admitted_pixels(1, 0));
        worker.config.invalidate_user();
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

        assert!(worker
            .controller
            .as_ref()
            .is_some_and(CaptureAccess::crop_visible));
        assert!(worker.latest_admitted.read().is_some());
        assert_eq!(worker.analyzer.structural_calls, 30);
        assert_eq!(worker.analyzer.calls, 0);
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

        assert_eq!(worker.analyzer.structural_calls, 30);
        assert_eq!(worker.analyzer.calls, 0);
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
        assert!(records
            .iter()
            .any(|(event, _)| event == "resource://sample"));
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
            age_ms: Some(0),
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
            age_ms: Some(0),
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
    fn stale_gsi_does_not_consume_events_and_a_new_epoch_readmits_a_key() {
        let start = Instant::now();
        let mut worker = fake_worker(FakeFrameSource::supported(), false, start, events());
        let batches = worker.risk.seen_gsi_events.clone();
        let event = GsiEvent {
            id: "health_low",
            at_ms: 90,
        };
        let stale = GsiSnapshot {
            recent_events: vec![event],
            age_ms: Some(2001),
            session_epoch: Some(1),
            ..GsiSnapshot::default()
        };
        let fresh = GsiSnapshot {
            age_ms: Some(0),
            ..stale.clone()
        };
        let next_match = GsiSnapshot {
            session_epoch: Some(2),
            ..fresh.clone()
        };

        worker.analysis_tick(100, Some(&stale));
        worker.analysis_tick(200, Some(&fresh));
        worker.analysis_tick(300, Some(&fresh));
        worker.analysis_tick(400, Some(&next_match));

        assert_eq!(
            &*lock(&batches),
            &[vec![], vec!["health_low"], vec![], vec!["health_low"]]
        );
    }

    #[test]
    fn frame_subscribers_are_capped() {
        assert_eq!(MAX_FRAME_SUBSCRIBERS, 8);
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
            age_ms: Some(0),
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
            age_ms: Some(0),
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
        worker.profile_rect = Some(Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        });
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
        worker.profile_rect = Some(Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        });
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

    #[test]
    fn capture_config_defaults_keep_unknown_geometry_and_proposal_unknown() {
        let config = CaptureConfig::default();
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.crop, CropOffsets::default());
        assert_eq!(config.display_fps, 30);
        assert!(!config.paused);
        assert!(config.profile_rect.is_none());
        assert!(config.effective_rect.is_none());
        assert!(config.proposal.is_none());
        assert_eq!(config.proposal_status, ProposalStatus::None);
        assert!(!config.crop_confirmed);
    }

    #[test]
    fn crop_offset_limits_are_inclusive_and_reject_outside_values() {
        assert!(validate_crop_offsets(CropOffsets {
            dx: 200,
            dy: -200,
            dw: 200,
            dh: -200,
        }));
        assert!(!validate_crop_offsets(CropOffsets {
            dx: 201,
            ..CropOffsets::default()
        }));
        assert!(!validate_crop_offsets(CropOffsets {
            dh: -201,
            ..CropOffsets::default()
        }));
    }

    #[test]
    fn display_fps_accepts_only_the_three_contract_values() {
        assert_eq!(display_interval(10), Some(Duration::from_millis(100)));
        assert_eq!(display_interval(15), Some(Duration::from_nanos(66_666_667)));
        assert_eq!(display_interval(30), Some(Duration::from_nanos(33_333_334)));
        assert_eq!(display_interval(0), None);
        assert_eq!(display_interval(60), None);
    }

    #[test]
    fn effective_crop_applies_offsets_to_profile_rect() {
        let profile = Rect {
            x: 10,
            y: 811,
            width: 260,
            height: 260,
        };
        let output = OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        };
        assert_eq!(
            effective_rect(
                profile,
                CropOffsets {
                    dx: 0,
                    dy: -1,
                    dw: -4,
                    dh: -4,
                },
                output,
            ),
            Some(Rect {
                x: 10,
                y: 810,
                width: 256,
                height: 256,
            })
        );
    }

    #[test]
    fn effective_crop_rejects_small_large_and_out_of_output_rectangles() {
        let profile = Rect {
            x: 1800,
            y: 900,
            width: 292,
            height: 292,
        };
        let output = OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        };
        assert_eq!(
            effective_rect(
                profile,
                CropOffsets {
                    dw: -229,
                    dh: -229,
                    ..CropOffsets::default()
                },
                output
            ),
            None
        );
        assert_eq!(
            effective_rect(
                profile,
                CropOffsets {
                    dw: 109,
                    dh: 109,
                    ..CropOffsets::default()
                },
                output
            ),
            None
        );
        assert_eq!(
            effective_rect(profile, CropOffsets::default(), output),
            None
        );
    }

    #[test]
    fn effective_crop_accepts_square_boundary_sizes_with_one_pixel_tolerance() {
        let profile = Rect {
            x: 100,
            y: 100,
            width: 200,
            height: 200,
        };
        let output = OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        };
        assert_eq!(
            effective_rect(
                profile,
                CropOffsets {
                    dw: -136,
                    dh: -135,
                    ..CropOffsets::default()
                },
                output,
            ),
            Some(Rect {
                x: 100,
                y: 100,
                width: 64,
                height: 65,
            })
        );
        assert_eq!(
            effective_rect(
                profile,
                CropOffsets {
                    dw: 200,
                    dh: 199,
                    ..CropOffsets::default()
                },
                output,
            ),
            Some(Rect {
                x: 100,
                y: 100,
                width: 400,
                height: 399,
            })
        );
    }

    #[test]
    fn config_patch_parser_rejects_null_unknown_and_wrongly_typed_fields() {
        for patch in [
            json!([]),
            json!({"paused": null}),
            json!({"display_fps": "15"}),
            json!({"crop": {"dx": 0, "dy": 0, "dw": 0, "dh": 0, "extra": 1}}),
            json!({"unknown": true}),
        ] {
            assert_eq!(
                parse_config_patch(patch),
                Err(CAPTURE_CONFIG_INVALID.to_owned())
            );
        }
        assert_eq!(
            parse_config_patch(json!({})),
            Ok(CaptureConfigPatch::default())
        );
    }

    #[test]
    fn config_store_round_trip_persists_only_user_settings() {
        let dir = test_config_dir("round-trip");
        let store = CaptureConfigStore::open(dir.clone());
        let configured = store
            .set(CaptureConfigPatch {
                crop: Some(CropOffsets {
                    dy: 12,
                    dw: -12,
                    dh: -12,
                    ..CropOffsets::default()
                }),
                display_fps: Some(30),
                paused: Some(true),
            })
            .unwrap();
        store.set_proposal(
            Some(Rect {
                x: 4,
                y: 5,
                width: 280,
                height: 280,
            }),
            ProposalStatus::Found,
        );

        let loaded = CaptureConfigStore::open(dir.clone()).get();
        assert_eq!(loaded.crop, configured.crop);
        assert_eq!(loaded.display_fps, 30);
        assert!(!loaded.paused);
        assert!(loaded.proposal.is_none());
        assert_eq!(loaded.proposal_status, ProposalStatus::None);
        assert!(!loaded.crop_confirmed);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_config_is_ignored_without_rewriting_the_file() {
        let dir = test_config_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CAPTURE_CONFIG_FILE);
        std::fs::write(&path, b"not json").unwrap();
        let loaded = CaptureConfigStore::open(dir.clone()).get();
        assert_eq!(loaded, CaptureConfig::default());
        assert_eq!(std::fs::read(&path).unwrap(), b"not json");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unsupported_config_schema_is_ignored_without_rewriting_the_file() {
        let dir = test_config_dir("unsupported-schema");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CAPTURE_CONFIG_FILE);
        let bytes = br#"{"schema_version":2,"display_fps":30}"#;
        std::fs::write(&path, bytes).unwrap();
        let loaded = CaptureConfigStore::open(dir.clone()).get();
        assert_eq!(loaded, CaptureConfig::default());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn config_store_rejects_invalid_fps_without_persisting_it() {
        let dir = test_config_dir("invalid-fps");
        let store = CaptureConfigStore::open(dir.clone());
        assert_eq!(
            store.set(CaptureConfigPatch {
                display_fps: Some(60),
                ..CaptureConfigPatch::default()
            }),
            Err(CAPTURE_CONFIG_INVALID.to_owned())
        );
        assert!(!dir.join(CAPTURE_CONFIG_FILE).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn config_store_bounds_the_effective_size_before_the_output_is_known() {
        let dir = test_config_dir("bounds-unknown-output");
        let store = CaptureConfigStore::open(dir.clone());
        for (dw, dh) in [(200, 200), (-217, -217), (-217, -216)] {
            assert_eq!(
                store.set(CaptureConfigPatch {
                    crop: Some(CropOffsets {
                        dx: 0,
                        dy: 0,
                        dw,
                        dh
                    }),
                    ..CaptureConfigPatch::default()
                }),
                Err(CAPTURE_CONFIG_INVALID.to_owned()),
                "dw={dw} dh={dh}"
            );
        }
        assert!(!dir.join(CAPTURE_CONFIG_FILE).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn config_store_empty_patch_does_not_overwrite_an_ignored_file() {
        let dir = test_config_dir("empty-patch");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(CAPTURE_CONFIG_FILE), b"not json").unwrap();
        let store = CaptureConfigStore::open(dir.clone());
        assert!(store.set(CaptureConfigPatch::default()).is_ok());
        assert_eq!(
            std::fs::read(dir.join(CAPTURE_CONFIG_FILE)).unwrap(),
            b"not json"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn config_store_clears_crop_confirmation_when_crop_changes() {
        let dir = test_config_dir("confirmation-reset");
        let store = CaptureConfigStore::open(dir.clone());
        store.confirm_crop().unwrap();
        let changed = store
            .set(CaptureConfigPatch {
                crop: Some(CropOffsets {
                    dy: 1,
                    ..CropOffsets::default()
                }),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        assert!(!changed.crop_confirmed);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn config_snapshot_returns_settings_and_revision_under_one_state_lock() {
        let dir = test_config_dir("snapshot");
        let store = CaptureConfigStore::open(dir.clone());
        let (_, initial_revision, _, _) = store.snapshot();
        let changed = store
            .set(CaptureConfigPatch {
                display_fps: Some(15),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        let (snapshot, revision, _, _) = store.snapshot();
        assert_eq!(snapshot, changed);
        assert_eq!(snapshot.display_fps, 15);
        assert_eq!(revision, initial_revision + 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn next_deadline_keeps_the_previous_phase_without_drift() {
        let start = Instant::now();
        let period = Duration::from_nanos(33_333_334);
        assert_eq!(
            next_deadline(start, start + Duration::from_millis(30), period),
            start + period
        );
    }

    #[test]
    fn next_deadline_skips_expired_slots_to_the_first_future_tick() {
        let start = Instant::now();
        let period = Duration::from_millis(10);
        assert_eq!(
            next_deadline(start, start + Duration::from_millis(35), period),
            start + Duration::from_millis(40)
        );
    }

    #[test]
    fn display_limiter_accepts_ninety_percent_of_the_selected_interval() {
        let start = Instant::now();
        let interval = Duration::from_millis(100);
        assert!(!display_ready(
            Some(start),
            start + Duration::from_millis(89),
            interval
        ));
        assert!(display_ready(
            Some(start),
            start + Duration::from_millis(90),
            interval
        ));
    }

    #[test]
    fn duration_window_reports_only_observed_samples_in_the_last_second() {
        let start = Instant::now();
        let mut window = DurationWindow::default();
        assert_eq!(window.mean_ms(start), None);
        window.record(start, Duration::from_millis(3));
        window.record(start + Duration::from_millis(300), Duration::from_millis(7));
        assert_eq!(
            window.mean_ms(start + Duration::from_millis(500)),
            Some(5.0)
        );
        assert_eq!(window.mean_ms(start + Duration::from_secs(2)), None);
    }

    #[test]
    fn crop_override_is_used_for_grab_analysis_and_transport_dimensions() {
        let start = Instant::now();
        let base = Rect {
            x: 100,
            y: 200,
            width: 100,
            height: 100,
        };
        let mut worker = worker_with_profile(start, base);
        worker
            .config
            .set(CaptureConfigPatch {
                crop: Some(CropOffsets {
                    dx: 20,
                    dy: 30,
                    dw: -10,
                    dh: -10,
                }),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        let gsi = GsiSnapshot {
            session_epoch: Some(7),
            age_ms: Some(0),
            ..GsiSnapshot::default()
        };
        worker.refresh_capture_profile(start, 0, Some(&gsi));
        let effective = Rect {
            x: 120,
            y: 230,
            width: 90,
            height: 90,
        };
        for sequence in 1..=STRUCTURAL_PASS_FRAMES {
            worker
                .source
                .queue_sized_frame(effective.width, effective.height, u64::from(sequence));
            let elapsed_ms = u64::from(sequence) * 33;
            worker.capture_one(
                start + Duration::from_millis(elapsed_ms),
                elapsed_ms,
                Some(&gsi),
            );
        }

        assert_eq!(
            worker.source.grabbed_rects.len(),
            STRUCTURAL_PASS_FRAMES as usize
        );
        assert!(worker
            .source
            .grabbed_rects
            .iter()
            .all(|rect| *rect == effective));
        assert_eq!(
            worker.analyzer.structural_input_sizes.last(),
            Some(&(90, 90, 360))
        );
        assert_eq!(worker.analyzer.calls, 0);
        worker.analysis_tick(1000, Some(&gsi));
        assert_eq!(worker.analyzer.input_sizes.last(), Some(&(90, 90, 360)));
        let admitted = worker
            .latest_admitted
            .read()
            .expect("validated crop admitted");
        let header = worker
            .header_codec
            .encode_header(&admitted, 0)
            .expect("effective dimensions fit transport header");
        assert_eq!(u16::from_le_bytes([header[8], header[9]]), 90);
        assert_eq!(u16::from_le_bytes([header[10], header[11]]), 90);
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn changing_crop_starts_a_new_generation_and_withholds_frames_until_validation() {
        let start = Instant::now();
        let profile = Rect {
            x: 100,
            y: 100,
            width: 64,
            height: 64,
        };
        let mut worker = worker_with_profile(start, profile);
        let gsi = GsiSnapshot {
            session_epoch: Some(7),
            age_ms: Some(0),
            ..GsiSnapshot::default()
        };
        for sequence in 1..=STRUCTURAL_PASS_FRAMES {
            worker.source.queue_sized_frame(64, 64, u64::from(sequence));
            let elapsed_ms = u64::from(sequence) * 33;
            worker.capture_one(
                start + Duration::from_millis(elapsed_ms),
                elapsed_ms,
                Some(&gsi),
            );
        }
        let old_generation = worker.controller.as_ref().unwrap().generation().unwrap();
        assert!(worker.latest_admitted.read().is_some());

        worker
            .config
            .set(CaptureConfigPatch {
                crop: Some(CropOffsets {
                    dx: 10,
                    ..CropOffsets::default()
                }),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        worker.refresh_capture_profile(start + Duration::from_secs(2), 2000, Some(&gsi));
        let new_generation = worker.controller.as_ref().unwrap().generation().unwrap();
        assert!(new_generation > old_generation);
        assert!(worker.latest_admitted.read().is_none());
        for sequence in 1..STRUCTURAL_PASS_FRAMES {
            worker
                .source
                .queue_sized_frame(64, 64, u64::from(sequence + 30));
            let elapsed_ms = 2000 + u64::from(sequence) * 33;
            worker.capture_one(
                start + Duration::from_millis(elapsed_ms),
                elapsed_ms,
                Some(&gsi),
            );
        }
        assert!(!worker.controller.as_ref().unwrap().crop_visible());
        assert!(worker.latest_admitted.read().is_none());

        worker.source.queue_sized_frame(64, 64, 100);
        worker.capture_one(start + Duration::from_millis(3000), 3000, Some(&gsi));

        assert!(worker.controller.as_ref().unwrap().crop_visible());
        assert!(worker.latest_admitted.read().is_some());
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn one_bad_structural_frame_does_not_reset_live_crop_or_confirm_streak() {
        let start = Instant::now();
        let rect = Rect {
            x: 20,
            y: 30,
            width: 64,
            height: 64,
        };
        let mut worker = worker_with_profile(start, rect);
        let controller = worker.controller.as_mut().unwrap();
        controller.validated = true;
        controller.generation = 3;
        controller.last_healthy_ms = Some(0);
        worker.consecutive_passes = STRUCTURAL_PASS_FRAMES;
        worker.analyzer.structural_results = (0..30).map(|index| index != 14).collect();
        for sequence in 1_u64..=30 {
            worker.source.queue_sized_frame(64, 64, sequence);
            let elapsed_ms = sequence * 33;
            worker.capture_one(start + Duration::from_millis(elapsed_ms), elapsed_ms, None);
        }

        assert!(worker.controller.as_ref().unwrap().crop_visible());
        assert_eq!(worker.consecutive_passes, STRUCTURAL_PASS_FRAMES);
        assert_eq!(worker.phase(990), "live");
        assert_eq!(worker.controller.as_ref().unwrap().structural_failures, 0);
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn sixty_one_bad_structural_frames_invalidate_the_live_crop() {
        let start = Instant::now();
        let rect = Rect {
            x: 20,
            y: 30,
            width: 64,
            height: 64,
        };
        let mut worker = worker_with_profile(start, rect);
        let controller = worker.controller.as_mut().unwrap();
        controller.validated = true;
        controller.generation = 3;
        controller.last_healthy_ms = Some(0);
        worker.analyzer.structural_results = std::iter::repeat_n(false, 61).collect();
        for sequence in 1_u64..=61 {
            worker.source.queue_sized_frame(64, 64, sequence);
            let elapsed_ms = sequence * 33;
            worker.capture_one(start + Duration::from_millis(elapsed_ms), elapsed_ms, None);
        }

        assert_eq!(STRUCTURAL_FAIL_FRAMES, 60);
        assert!(!worker.controller.as_ref().unwrap().crop_visible());
        assert!(worker.latest_admitted.read().is_none());
        assert_eq!(worker.phase(2013), "lost");
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn access_lost_recovery_stays_unavailable_until_thirty_passes() {
        let start = Instant::now();
        let rect = Rect {
            x: 20,
            y: 30,
            width: 64,
            height: 64,
        };
        let mut worker = worker_with_profile(start, rect);
        let controller = worker.controller.as_mut().unwrap();
        controller.validated = true;
        controller.generation = 3;
        controller.last_healthy_ms = Some(0);
        worker
            .source
            .grabs
            .push_back((Grab::AccessLost, Vec::new()));
        worker.capture_one(start, 10, None);
        assert_eq!(worker.phase_reason(10), Some("access_lost"));
        assert!(!worker.source_open);

        let reopened_at = start + Duration::from_millis(500);
        worker.ensure_source(reopened_at);
        let gsi = GsiSnapshot {
            session_epoch: Some(7),
            age_ms: Some(0),
            ..GsiSnapshot::default()
        };
        worker.refresh_capture_profile(reopened_at, 500, Some(&gsi));
        for sequence in 1..STRUCTURAL_PASS_FRAMES {
            worker.source.queue_sized_frame(64, 64, u64::from(sequence));
            let elapsed_ms = 500 + u64::from(sequence) * 33;
            worker.capture_one(
                start + Duration::from_millis(elapsed_ms),
                elapsed_ms,
                Some(&gsi),
            );
        }
        assert!(!worker.controller.as_ref().unwrap().crop_visible());
        assert!(worker.latest_admitted.read().is_none());

        worker.source.queue_sized_frame(64, 64, 100);
        worker.capture_one(start + Duration::from_millis(1600), 1600, Some(&gsi));
        assert!(worker.controller.as_ref().unwrap().crop_visible());
        assert!(worker.latest_admitted.read().is_some());
        assert_eq!(worker.phase(1600), "live");
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn missing_game_window_recovers_to_calibrating_when_it_returns() {
        let start = Instant::now();
        let rect = Rect {
            x: 20,
            y: 30,
            width: 64,
            height: 64,
        };
        let mut worker = worker_with_profile(start, rect);
        worker.source.window = None;
        let gsi = GsiSnapshot {
            session_epoch: Some(7),
            age_ms: Some(0),
            ..GsiSnapshot::default()
        };
        worker.refresh_capture_profile(start + Duration::from_millis(10), 10, Some(&gsi));
        assert_eq!(worker.phase(10), "unsupported");
        assert_eq!(worker.phase_reason(10), Some("no_game_window"));

        worker.source.window = Some(Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });
        worker.refresh_capture_profile(start + Duration::from_millis(20), 20, Some(&gsi));

        assert_eq!(worker.phase(20), "calibrating");
        assert_eq!(worker.phase_reason(20), Some("calibrating"));
        assert!(!worker.controller.as_ref().unwrap().crop_visible());
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn open_errors_back_off_and_a_good_frame_resets_the_next_failure_delay() {
        let start = Instant::now();
        let mut source = FakeFrameSource::supported();
        source.profile = Some((
            Rect {
                x: 20,
                y: 30,
                width: 64,
                height: 64,
            },
            "fake-profile".to_owned(),
        ));
        source.open_error = Some(OpenError::Os("fake open error".to_owned()));
        let mut worker = fake_worker(source, true, start, events());
        worker.ensure_source(start);
        assert_eq!(worker.next_open_at, start + Duration::from_millis(500));
        worker.ensure_source(start + Duration::from_millis(499));
        assert_eq!(worker.source.open_calls, 1);
        worker.ensure_source(start + Duration::from_millis(500));
        assert_eq!(worker.next_open_at, start + Duration::from_millis(1500));
        assert_eq!(worker.retry.next, Some(Duration::from_secs(2)));

        worker.source.open_error = None;
        let reopened_at = start + Duration::from_millis(1500);
        worker.ensure_source(reopened_at);
        let gsi = GsiSnapshot {
            session_epoch: Some(7),
            age_ms: Some(0),
            ..GsiSnapshot::default()
        };
        worker.refresh_capture_profile(reopened_at, 1500, Some(&gsi));
        worker.source.queue_sized_frame(64, 64, 1);
        worker.capture_one(reopened_at, 1500, Some(&gsi));
        assert_eq!(worker.retry.next, None);

        worker
            .source
            .grabs
            .push_back((Grab::AccessLost, Vec::new()));
        worker.capture_one(reopened_at + Duration::from_millis(10), 1510, Some(&gsi));
        assert_eq!(
            worker.next_open_at,
            reopened_at + Duration::from_millis(510)
        );
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn crop_proposal_uses_highest_confidence_above_threshold_without_applying_it() {
        let start = Instant::now();
        let profile = Rect {
            x: 100,
            y: 200,
            width: 64,
            height: 64,
        };
        let mut worker = worker_with_profile(start, profile);
        let old_crop = worker.config.get().crop;
        worker.config.request_proposal();
        worker.start_proposal_if_requested();
        let search = Rect {
            x: 52,
            y: 152,
            width: 160,
            height: 160,
        };
        let proposals = Arc::new(Mutex::new(VecDeque::from([
            Some(FrameProposal {
                x: 5,
                y: 10,
                width: 64,
                height: 64,
                confidence_milli: 650,
            }),
            Some(FrameProposal {
                x: 10,
                y: 20,
                width: 70,
                height: 64,
                confidence_milli: 850,
            }),
            Some(FrameProposal {
                x: 1,
                y: 1,
                width: 64,
                height: 64,
                confidence_milli: 600,
            }),
            Some(FrameProposal {
                x: 20,
                y: 20,
                width: 64,
                height: 64,
                confidence_milli: 810,
            }),
            None,
        ])));
        let inputs = Arc::new(Mutex::new(Vec::new()));
        worker.proposer = Box::new(FakeFrameProposer {
            proposals,
            inputs: inputs.clone(),
        });
        for sequence in 1..=CROP_SEARCH_MAX_FRAMES {
            worker
                .source
                .queue_sized_frame(160, 160, u64::from(sequence));
            worker.capture_proposal_frame(
                start + Duration::from_millis(u64::from(sequence) * 10),
                u64::from(sequence) * 10,
            );
        }

        let config = worker.config.get();
        assert_eq!(config.proposal_status, ProposalStatus::Found);
        assert_eq!(
            config.proposal,
            Some(ScreenRect {
                x: 62,
                y: 172,
                width: 64,
                height: 64,
            })
        );
        assert_eq!(config.crop, old_crop);
        assert_eq!(worker.grab_total, 5);
        assert_eq!(
            worker.source.grabbed_rects,
            vec![search; CROP_SEARCH_MAX_FRAMES as usize]
        );
        assert_eq!(
            lock(&inputs).as_slice(),
            &vec![(160, 160, 640); CROP_SEARCH_MAX_FRAMES as usize]
        );
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn crop_proposal_reports_not_found_after_five_low_confidence_frames() {
        let start = Instant::now();
        let profile = Rect {
            x: 100,
            y: 200,
            width: 64,
            height: 64,
        };
        let mut worker = worker_with_profile(start, profile);
        worker.config.request_proposal();
        worker.start_proposal_if_requested();
        let proposals = Arc::new(Mutex::new(VecDeque::from(vec![
            Some(FrameProposal {
                x: 10,
                y: 10,
                width: 64,
                height: 64,
                confidence_milli: 600,
            });
            CROP_SEARCH_MAX_FRAMES
                as usize
        ])));
        worker.proposer = Box::new(FakeFrameProposer {
            proposals,
            inputs: Arc::new(Mutex::new(Vec::new())),
        });
        for sequence in 1..=CROP_SEARCH_MAX_FRAMES {
            worker
                .source
                .queue_sized_frame(160, 160, u64::from(sequence));
            worker.capture_proposal_frame(start, u64::from(sequence));
        }

        let config = worker.config.get();
        assert_eq!(config.proposal_status, ProposalStatus::NotFound);
        assert!(config.proposal.is_none());
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn accepting_crop_proposal_applies_offsets_and_confirms_crop() {
        let dir = test_config_dir("accept-proposal");
        let store = CaptureConfigStore::open(dir.clone());
        let profile = Rect {
            x: 100,
            y: 200,
            width: 64,
            height: 64,
        };
        let output = OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        };
        store.set_profile(profile, output);
        store.set_proposal(
            Some(Rect {
                x: 62,
                y: 172,
                width: 64,
                height: 64,
            }),
            ProposalStatus::Found,
        );

        let accepted = store.accept_proposal().unwrap();

        assert_eq!(
            accepted.crop,
            CropOffsets {
                dx: -38,
                dy: -28,
                dw: 0,
                dh: 0
            }
        );
        assert_eq!(
            accepted.effective_rect,
            Some(ScreenRect {
                x: 62,
                y: 172,
                width: 64,
                height: 64,
            })
        );
        assert!(accepted.crop_confirmed);
        assert!(accepted.proposal.is_none());
        assert_eq!(accepted.proposal_status, ProposalStatus::None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn proposal_search_rectangle_is_inflated_and_clamped_to_output() {
        let profile = Rect {
            x: 0,
            y: 1000,
            width: 64,
            height: 64,
        };
        let output = OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        };
        assert_eq!(
            inflated_search_rect(profile, output),
            Some(Rect {
                x: 0,
                y: 952,
                width: 112,
                height: 128,
            })
        );
    }

    #[test]
    fn paused_capture_sends_no_frames_reports_unavailable_and_keeps_resource_samples() {
        let start = Instant::now();
        let rect = Rect {
            x: 20,
            y: 30,
            width: 64,
            height: 64,
        };
        let mut worker = worker_with_profile(start, rect);
        worker
            .config
            .set(CaptureConfigPatch {
                display_fps: Some(10),
                paused: Some(true),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        worker.source.queue_sized_frame(64, 64, 1);
        let records = worker.events.0.clone();

        worker.cycle_at(start, 0);
        let mut sinks = vec![FakeFrameSink::default()];
        worker.publish_frame_to(start, 0, &mut sinks);

        assert!(worker.source.grabbed_rects.is_empty());
        assert!(sinks[0].messages.is_empty());
        assert_eq!(
            lock(&worker.risk.seen_coverage).as_slice(),
            &[Coverage::Unavailable]
        );
        let records = lock(&records);
        assert!(records
            .iter()
            .any(|(event, _)| event == "resource://sample"));
        let status = records
            .iter()
            .find(|(event, _)| event == "capture://status")
            .map(|(_, payload)| payload)
            .expect("paused status is emitted");
        assert_eq!(status["display_fps"], 10);
        assert_eq!(status["paused"], true);
        assert_eq!(status["phase_reason"], "paused");
        assert!(status["grab_ms_mean"].is_null());
        assert!(status["analysis_ms_mean"].is_null());
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn unknown_status_keeps_capture_measurements_null() {
        let status = unknown_capture_status();
        assert_eq!(status["crop_confirmed"], false);
        assert_eq!(status["display_fps"], 30);
        assert_eq!(status["paused"], false);
        assert!(status["phase_reason"].is_null());
        assert!(status["grab_ms_mean"].is_null());
        assert!(status["analysis_ms_mean"].is_null());
        assert!(status["grab_fps"].is_null());
        assert!(status["sent_total"].is_null());
        assert!(status["grab_total"].is_null());
    }

    #[test]
    fn resetting_crop_clears_confirmation_even_when_offsets_are_already_zero() {
        let dir = test_config_dir("reset-crop");
        let store = CaptureConfigStore::open(dir.clone());
        store.confirm_crop().unwrap();
        let reset = store.reset_crop().unwrap();
        assert_eq!(reset.crop, CropOffsets::default());
        assert!(!reset.crop_confirmed);
        assert_eq!(reset.proposal_status, ProposalStatus::None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn preview_pacing_sustains_twenty_eight_fps_with_windows_timer_jitter() {
        // Pure simulation of the fallback waiter: sleep rounds up to a 15.6 ms quantum,
        // then the final <= 2 ms tail spins. Grab work cycles through 2..=16 ms.
        let start = Instant::now();
        let period = display_interval(30).unwrap();
        let quantum_ns = Duration::from_micros(15_600).as_nanos();
        let mut now = start;
        let mut deadline = start;
        let mut limiter = DisplayLimiter::new(start);
        let mut grabs = 0_u64;
        let mut sent = 0_u64;
        while now.duration_since(start) < Duration::from_secs(10) {
            grabs += 1;
            if limiter.can_send(now, grabs, true) {
                limiter.record_sent(now, grabs);
                sent += 1;
            }
            let finished = now + Duration::from_millis(2 + (grabs * 7) % 15);
            deadline = next_deadline(deadline, finished, period);
            let sleep_ns = deadline
                .saturating_duration_since(finished)
                .saturating_sub(PACER_SPIN_TAIL)
                .as_nanos();
            let rounded_sleep_ns = sleep_ns.div_ceil(quantum_ns) * quantum_ns;
            let wake = finished + Duration::from_nanos(rounded_sleep_ns as u64);
            now = wake.max(deadline);
        }
        eprintln!(
            "preview simulation: grabs={grabs}, sent={sent}, fps={:.1}, seconds=10",
            sent as f64 / 10.0
        );
        assert!(
            sent >= 280,
            "only {sent} frames sent in 10 simulated seconds"
        );
    }

    #[test]
    fn non_square_config_is_rejected_before_and_after_output_is_known() {
        let dir = test_config_dir("non-square");
        let store = CaptureConfigStore::open(dir.clone());
        let patch = CaptureConfigPatch {
            crop: Some(CropOffsets {
                dw: -12,
                dh: -18,
                ..CropOffsets::default()
            }),
            ..CaptureConfigPatch::default()
        };
        assert_eq!(
            store.set(patch.clone()),
            Err(CAPTURE_CONFIG_INVALID.to_owned())
        );
        let output = OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        };
        let (profile, name) = kohbor_capture_native::profile_rect(&output).unwrap();
        assert_eq!(name, "p1080-default-v2");
        assert_eq!(
            profile,
            Rect {
                x: 10,
                y: 811,
                width: 260,
                height: 260
            }
        );
        store.set_profile(profile, output);
        store.request_proposal();
        let before = store.snapshot();
        assert_eq!(store.set(patch), Err(CAPTURE_CONFIG_INVALID.to_owned()));
        assert_eq!(store.snapshot(), before);
        assert!(store.take_proposal_request());
        let valid = store
            .set(CaptureConfigPatch {
                crop: Some(CropOffsets {
                    dw: -12,
                    dh: -11,
                    ..CropOffsets::default()
                }),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        assert_eq!(valid.effective_rect.unwrap().width, 248);
        assert_eq!(valid.effective_rect.unwrap().height, 249);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn non_square_persisted_crop_is_ignored_without_rewriting() {
        let dir = test_config_dir("non-square-persisted");
        std::fs::create_dir_all(&dir).unwrap();
        let bytes = br#"{"schema_version":1,"crop":{"dx":0,"dy":0,"dw":-12,"dh":-18},"display_fps":30,"crop_confirmed":true}"#;
        let path = dir.join(CAPTURE_CONFIG_FILE);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(
            CaptureConfigStore::open(dir.clone()).get(),
            CaptureConfig::default()
        );
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn proposal_squaring_shrinks_to_shorter_side_and_preserves_top_left() {
        let search = Rect {
            x: 0,
            y: 749,
            width: 328,
            height: 328,
        };
        let output = OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        };
        for (width, height, expected_width, expected_height) in [
            (280, 274, 274, 274),
            (274, 280, 274, 274),
            (280, 279, 280, 279),
        ] {
            assert_eq!(
                absolute_proposal(
                    search,
                    output,
                    FrameProposal {
                        x: 2,
                        y: 48,
                        width,
                        height,
                        confidence_milli: 900,
                    }
                ),
                Some(Rect {
                    x: 2,
                    y: 797,
                    width: expected_width,
                    height: expected_height
                })
            );
        }
    }

    #[test]
    fn full_analysis_runs_at_ten_hz_on_latest_admitted_pixels() {
        let _status_gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = worker_with_profile(
            start,
            Rect {
                x: 100,
                y: 200,
                width: 64,
                height: 64,
            },
        );
        let controller = worker.controller.as_mut().unwrap();
        controller.validated = true;
        controller.last_healthy_ms = Some(0);
        worker.gsi = GsiView::new(|| GsiSnapshot {
            team: Some("radiant".to_owned()),
            session_epoch: Some(7),
            age_ms: Some(0),
            ..GsiSnapshot::default()
        });
        let period = display_interval(30).unwrap();
        for index in 0..30_u32 {
            worker
                .source
                .queue_sized_frame(64, 64, u64::from(index + 1));
            let at = start + period * index;
            worker.cycle_at(at, at.duration_since(start).as_millis() as u64);
        }
        assert_eq!(worker.analyzer.structural_calls, 30);
        assert_eq!(worker.analyzer.calls, 10);
        assert_eq!(
            worker.analyzer.input_first_bytes,
            (0..10).map(|index| Some(1 + index * 3)).collect::<Vec<_>>()
        );
        assert_eq!(worker.grab_total, 30);
        worker.paused = true;
        worker.analysis_tick(1000, None);
        assert_eq!(worker.analyzer.calls, 10);
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn timeout_produces_no_new_grab_or_send_and_totals_survive_frame_reset() {
        let _status_gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = worker_with_profile(
            start,
            Rect {
                x: 100,
                y: 200,
                width: 64,
                height: 64,
            },
        );
        let controller = worker.controller.as_mut().unwrap();
        controller.validated = true;
        controller.last_healthy_ms = Some(0);
        let mut sinks = vec![FakeFrameSink::default()];
        worker.source.queue_sized_frame(64, 64, 1);
        worker.capture_one(start, 0, None);
        worker.publish_frame_to(start, 0, &mut sinks);
        worker.capture_one(start + DISPLAY_INTERVAL, 33, None);
        worker.publish_frame_to(start + DISPLAY_INTERVAL, 33, &mut sinks);
        worker.publish_status(start + Duration::from_secs(1), 1000);
        let status = lock(&worker.events.0).last().unwrap().1.clone();
        assert_eq!(status["grab_fps"], 1);
        assert_eq!(status["fps_measured"], 1);
        assert_eq!(status["grab_total"], 1);
        assert_eq!(status["sent_total"], 1);
        worker.clear_frames();
        worker.source.queue_sized_frame(64, 64, 2);
        worker.capture_one(start + Duration::from_millis(1100), 1100, None);
        worker.publish_frame_to(start + Duration::from_millis(1100), 1100, &mut sinks);
        worker.publish_status(start + Duration::from_millis(2100), 2100);
        let status = lock(&worker.events.0).last().unwrap().1.clone();
        assert_eq!(status["grab_total"], 2);
        assert_eq!(status["sent_total"], 2);
        assert_eq!(sinks[0].messages.len(), 2);
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    type Fix2Worker = CaptureHostWorker<
        FakeFrameSource,
        FakeCaptureAccess,
        FakeAnalyzer,
        FakeRisk,
        FakeEvents,
        MvpFrameHeaderCodec,
    >;

    fn fix2_worker(start: Instant) -> Fix2Worker {
        let output = OutputInfo {
            output_id: 4,
            width: 1920,
            height: 1080,
            dpi: 96,
        };
        let (rect, name) = kohbor_capture_native::profile_rect(&output).unwrap();
        let mut worker = worker_with_profile(start, rect);
        worker.source.profile.as_mut().unwrap().1 = name.to_owned();
        worker.gsi = GsiView::new(|| GsiSnapshot {
            team: Some("radiant".to_owned()),
            session_epoch: Some(7),
            age_ms: Some(0),
            ..GsiSnapshot::default()
        });
        worker
    }

    fn fix2_assert_revalidated(
        worker: &mut Fix2Worker,
        start: Instant,
        at_ms: u64,
        old_generation: u64,
    ) {
        let mut sinks = vec![FakeFrameSink::default()];
        for index in 0..30_u64 {
            let profile = worker.source.profile.as_ref().unwrap().0;
            worker
                .source
                .queue_sized_frame(profile.width, profile.height, 100 + index);
            let ms = at_ms + index * 34;
            worker.cycle_at(start + Duration::from_millis(ms), ms);
            worker.publish_frame_to(start + Duration::from_millis(ms), ms, &mut sinks);
            if index < 29 {
                assert_ne!(worker.phase(ms), "live");
                assert!(worker.latest_admitted.read().is_none());
                assert!(sinks[0].messages.is_empty());
            }
        }
        assert_eq!(worker.phase(at_ms + 29 * 34), "live");
        assert!(worker.controller.as_ref().unwrap().generation().unwrap() > old_generation);
        assert_eq!(sinks[0].messages.len(), 1);
    }

    #[test]
    fn fix2_owner_invalidate_nudge_confirm_recovers_after_thirty_passes() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        fix2_assert_revalidated(&mut worker, start, 0, 0);
        let generation = worker.controller.as_ref().unwrap().generation().unwrap();
        worker.config.invalidate_user();
        worker.invalidate_for_user(1100);
        worker.cycle_at(start + Duration::from_millis(1100), 1100);
        assert_eq!(worker.phase(1100), "unsupported");
        assert_eq!(worker.phase_reason(1100), Some("user_invalidated"));
        worker
            .config
            .set(CaptureConfigPatch {
                crop: Some(CropOffsets {
                    dx: 1,
                    ..CropOffsets::default()
                }),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        worker.config.confirm_crop().unwrap();
        fix2_assert_revalidated(&mut worker, start, 1200, generation);
    }

    #[test]
    fn fix2_no_game_window_has_distinct_reason_and_recovers() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        let window = worker.source.window.take();
        worker.cycle_at(start, 0);
        assert_eq!(worker.phase(0), "unsupported");
        assert_eq!(worker.phase_reason(0), Some("no_game_window"));
        let generation = worker.controller.as_ref().unwrap().generation().unwrap();
        worker.source.window = window;
        fix2_assert_revalidated(&mut worker, start, 100, generation);
    }

    #[test]
    fn fix2_invalid_crop_for_output_has_distinct_reason_and_reset_recovers() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        worker.config.clear_profile();
        worker
            .config
            .set(CaptureConfigPatch {
                crop: Some(CropOffsets {
                    dx: -100,
                    ..CropOffsets::default()
                }),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        worker.cycle_at(start, 0);
        assert_eq!(worker.phase(0), "unsupported");
        assert_eq!(worker.phase_reason(0), Some("crop_out_of_bounds"));
        let generation = worker.controller.as_ref().unwrap().generation().unwrap();
        worker.config.reset_crop().unwrap();
        fix2_assert_revalidated(&mut worker, start, 100, generation);
    }

    #[test]
    fn fix2_1440p_has_distinct_reason_and_resolution_return_recovers() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        let output = worker.source.output;
        let profile = worker.source.profile.take();
        worker.source.output = Some(OutputInfo {
            width: 2560,
            height: 1440,
            ..output.unwrap()
        });
        worker.cycle_at(start, 0);
        assert_eq!(worker.phase(0), "unsupported");
        assert_eq!(worker.phase_reason(0), Some("unsupported_resolution"));
        let generation = worker.controller.as_ref().unwrap().generation().unwrap();
        worker.source.output = output;
        worker.source.profile = profile;
        fix2_assert_revalidated(&mut worker, start, 600, generation);
        assert!(worker.source.open_calls > 0);
    }

    #[test]
    fn fix2_controller_rejection_has_crop_reason_and_recovers() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        worker.controller.as_mut().unwrap().reject_begin = true;
        worker.profile_rect = None;
        worker.cycle_at(start, 0);
        assert_eq!(worker.phase_reason(0), Some("crop_out_of_bounds"));
        worker.controller.as_mut().unwrap().reject_begin = false;
        let generation = worker.controller.as_ref().unwrap().generation().unwrap();
        fix2_assert_revalidated(&mut worker, start, 100, generation);
    }

    #[test]
    fn fix2_disabled_capture_has_distinct_reason() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        worker.capture_enabled = false;
        worker.cycle_at(start, 0);
        assert_eq!(worker.phase(0), "unsupported");
        assert_eq!(worker.phase_reason(0), Some("capture_disabled"));
        assert!(worker.source.grabbed_rects.is_empty());
    }

    #[test]
    fn fix2_each_layout_action_retries_even_when_crop_is_unchanged() {
        let _gate = lock(&STATUS_TEST_GATE);
        for action in ["retry", "same_crop", "reset", "accept", "confirm"] {
            let start = Instant::now();
            let mut worker = fix2_worker(start);
            worker.config.confirm_crop().unwrap();
            fix2_assert_revalidated(&mut worker, start, 0, 0);
            let generation = worker.controller.as_ref().unwrap().generation().unwrap();
            worker.config.invalidate_user();
            worker.cycle_at(start + Duration::from_millis(1100), 1100);
            assert_eq!(
                worker.phase_reason(1100),
                Some("user_invalidated"),
                "{action}"
            );
            match action {
                "retry" => {
                    worker.config.retry();
                }
                "same_crop" => {
                    worker
                        .config
                        .set(CaptureConfigPatch {
                            crop: Some(CropOffsets::default()),
                            ..CaptureConfigPatch::default()
                        })
                        .unwrap();
                }
                "reset" => {
                    worker.config.reset_crop().unwrap();
                }
                "accept" => {
                    let profile = worker.source.profile.as_ref().unwrap().0;
                    worker
                        .config
                        .set_proposal(Some(profile), ProposalStatus::Found);
                    worker.config.accept_proposal().unwrap();
                }
                "confirm" => {
                    worker.config.confirm_crop().unwrap();
                }
                _ => unreachable!(),
            }
            assert_eq!(worker.config.get().crop, CropOffsets::default());
            fix2_assert_revalidated(&mut worker, start, 1200, generation);
            let _ = std::fs::remove_dir_all(&worker.config.data_dir);
        }
    }

    #[test]
    fn fix2_fps_pause_and_rejected_patch_do_not_clear_user_invalidation() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        worker.config.invalidate_user();
        worker
            .config
            .set(CaptureConfigPatch {
                display_fps: Some(15),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        worker.cycle_at(start, 0);
        assert_eq!(worker.phase_reason(0), Some("user_invalidated"));
        worker
            .config
            .set(CaptureConfigPatch {
                paused: Some(true),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        worker.cycle_at(start + Duration::from_millis(100), 100);
        assert_eq!(worker.phase_reason(100), Some("paused"));
        worker
            .config
            .set(CaptureConfigPatch {
                paused: Some(false),
                ..CaptureConfigPatch::default()
            })
            .unwrap();
        assert!(worker
            .config
            .set(CaptureConfigPatch {
                crop: Some(CropOffsets {
                    dx: -200,
                    ..CropOffsets::default()
                }),
                ..CaptureConfigPatch::default()
            })
            .is_err());
        worker.cycle_at(start + Duration::from_millis(200), 200);
        assert_eq!(worker.phase_reason(200), Some("user_invalidated"));
        assert!(worker.source.grabbed_rects.is_empty());
        let _ = std::fs::remove_dir_all(&worker.config.data_dir);
    }

    #[test]
    fn fix2_open_errors_have_distinct_reasons_and_recover_after_backoff() {
        let _gate = lock(&STATUS_TEST_GATE);
        for (error, reason) in [
            (OpenError::NoGameWindow, "no_game_window"),
            (
                OpenError::Unsupported("2560x1440".to_owned()),
                "capture_unavailable",
            ),
        ] {
            let start = Instant::now();
            let mut worker = fix2_worker(start);
            worker.source.close();
            worker.source_open = false;
            worker.source.open_error = Some(error);
            worker.cycle_at(start, 0);
            assert_eq!(worker.phase(0), "unsupported");
            assert_eq!(worker.phase_reason(0), Some(reason));
            let generation = worker.controller.as_ref().unwrap().generation().unwrap();
            worker.source.open_error = None;
            fix2_assert_revalidated(&mut worker, start, 600, generation);
        }
    }

    #[test]
    fn fix2_retry_keeps_capture_disabled() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        worker.capture_enabled = false;
        worker.config.invalidate_user();
        worker.config.retry();
        worker.cycle_at(start, 0);
        assert_eq!(worker.phase(0), "unsupported");
        assert_eq!(worker.phase_reason(0), Some("capture_disabled"));
        assert!(worker.source.grabbed_rects.is_empty());
    }

    #[test]
    fn fix2_profile_name_selects_inner_or_framed_checks() {
        let _gate = lock(&STATUS_TEST_GATE);
        for (name, mode) in [
            ("p1080-default-v2", StructuralMode::Inner),
            ("future-framed", StructuralMode::Framed),
        ] {
            let start = Instant::now();
            let mut worker = fix2_worker(start);
            worker.source.profile.as_mut().unwrap().1 = name.to_owned();
            let rect = worker.source.profile.as_ref().unwrap().0;
            worker.source.queue_sized_frame(rect.width, rect.height, 1);
            worker.cycle_at(start, 0);
            assert_eq!(worker.analyzer.structural_modes, [mode]);
        }
    }

    #[test]
    fn fix2_inner_crop_goes_live_even_when_full_analyze_reports_no_frame() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let fake = fix2_worker(start);
        let services = WorkerServices {
            analyzer: MinimapAnalyzer,
            risk: fake.risk,
            events: fake.events,
            header_codec: fake.header_codec,
            gsi: fake.gsi,
            resources: fake.resources,
            capture_enabled: true,
            config: fake.config,
            proposer: fake.proposer,
        };
        let mut worker = CaptureHostWorker::<_, CaptureControllerAccess, _, _, _, _>::new(
            fake.source,
            services,
            start,
        );
        let side = DEFAULT_PROFILE_SIDE;
        let mut pixels = Vec::new();
        for y in 0..side {
            for x in 0..side {
                let value = if x < 3 || y < 3 || x >= side - 3 || y >= side - 3 {
                    180
                } else if (x + y) % 2 == 0 {
                    60
                } else {
                    100
                };
                pixels.extend_from_slice(&[value, value, value, 255]);
            }
        }
        assert!(
            !kohbor_minimap_cv::analyze(&pixels, side, side, side * 4, Team::Dire).structural_ok
        );
        for index in 0..30_u64 {
            worker.source.grabs.push_back((
                Grab::Frame {
                    width: side,
                    height: side,
                    stride: side * 4,
                    acquired_us: index * 1000,
                },
                pixels.clone(),
            ));
            worker.cycle_at(start + Duration::from_millis(index * 34), index * 34);
            if index < 29 {
                assert_ne!(worker.phase(index * 34), "live");
            }
        }
        let gsi = worker.gsi.read();
        worker.analysis_tick(1000, Some(&gsi));
        assert_eq!(worker.phase(1000), "live");
        assert!(worker.latest_admitted.read().is_some());
    }

    #[test]
    fn fix2_pending_invalidation_blocks_status_and_frames_before_the_next_cycle() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        fix2_assert_revalidated(&mut worker, start, 0, 0);
        let generation = worker.controller.as_ref().unwrap().generation().unwrap();
        worker.config.invalidate_user();
        let mut sinks = vec![FakeFrameSink::default()];
        worker.publish_frame_to(start + Duration::from_millis(1100), 1100, &mut sinks);
        assert!(sinks[0].messages.is_empty());
        worker.publish_status(start + Duration::from_millis(1100), 1100);
        let status = lock(&worker.events.0).last().unwrap().1.clone();
        assert_eq!(status["phase_reason"], "user_invalidated");
        assert_eq!(status["phase"], "unsupported");
        // Even an invalidate/retry pair arriving between cycles must fence the old frame.
        worker.config.retry();
        fix2_assert_revalidated(&mut worker, start, 1200, generation);
    }

    #[test]
    fn fix2r2_admitted_inner_rings_reach_risk_as_a_known_count() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let fake = fix2_worker(start);
        let services = WorkerServices {
            analyzer: MinimapAnalyzer,
            risk: fake.risk,
            events: fake.events,
            header_codec: fake.header_codec,
            gsi: fake.gsi,
            resources: fake.resources,
            capture_enabled: true,
            config: fake.config,
            proposer: fake.proposer,
        };
        let mut worker = CaptureHostWorker::<_, CaptureControllerAccess, _, _, _, _>::new(
            fake.source,
            services,
            start,
        );
        let side = DEFAULT_PROFILE_SIDE;
        let mut pixels = Vec::new();
        for y in 0..side {
            for x in 0..side {
                let color = if x < 3 || y < 3 || x >= side - 3 || y >= side - 3 {
                    [50, 90, 40, 255]
                } else if (x / 32 + y / 32) % 2 == 0 {
                    [18, 24, 14, 255]
                } else {
                    [35, 44, 29, 255]
                };
                pixels.extend_from_slice(&color);
            }
        }
        for (cx, cy) in [(50_i32, 50_i32), (110, 150)] {
            for dy in -9_i32..=9 {
                for dx in -9_i32..=9 {
                    if (25..=81).contains(&(dx * dx + dy * dy)) {
                        let offset = (((cy + dy) as u32 * side + (cx + dx) as u32) * 4) as usize;
                        pixels[offset..offset + 4].copy_from_slice(&[41, 41, 219, 255]);
                    }
                }
            }
        }
        let report = kohbor_minimap_cv::analyze(&pixels, side, side, side * 4, Team::Dire);
        assert!(!report.structural_ok);
        assert_eq!(report.enemy_blobs.len(), 2);
        for index in 0..30_u64 {
            worker.source.grabs.push_back((
                Grab::Frame {
                    width: side,
                    height: side,
                    stride: side * 4,
                    acquired_us: index * 1000,
                },
                pixels.clone(),
            ));
            worker.cycle_at(start + Duration::from_millis(index * 34), index * 34);
        }
        let gsi = worker.gsi.read();
        worker.analysis_tick(1000, Some(&gsi));
        assert_eq!(worker.phase(1000), "live");
        assert_eq!(lock(&worker.risk.seen_visible).last(), Some(&Some(2)));
    }

    #[test]
    fn fix2r2_commands_during_grab_drop_pixels_and_require_revalidation() {
        let _gate = lock(&STATUS_TEST_GATE);
        for action in [
            "invalidate_retry",
            "retry",
            "same_crop",
            "reset",
            "confirm",
            "accept",
        ] {
            let start = Instant::now();
            let mut worker = fix2_worker(start);
            fix2_assert_revalidated(&mut worker, start, 0, 0);
            let generation = worker.controller.as_ref().unwrap().generation().unwrap();
            let config = worker.config.clone();
            worker.source.after_grab = Some(Arc::new(move || match action {
                "invalidate_retry" => {
                    config.invalidate_user();
                    config.retry();
                }
                "retry" => {
                    config.retry();
                }
                "same_crop" => {
                    config
                        .set(CaptureConfigPatch {
                            crop: Some(CropOffsets::default()),
                            ..CaptureConfigPatch::default()
                        })
                        .unwrap();
                }
                "reset" => {
                    config.reset_crop().unwrap();
                }
                "confirm" => {
                    config.confirm_crop().unwrap();
                }
                "accept" => {
                    config.set_proposal(
                        config.get().profile_rect.map(Into::into),
                        ProposalStatus::Found,
                    );
                    config.accept_proposal().unwrap();
                }
                _ => unreachable!(),
            }));
            worker
                .source
                .queue_sized_frame(DEFAULT_PROFILE_SIDE, DEFAULT_PROFILE_SIDE, 200);
            worker.cycle_at(start + Duration::from_millis(1100), 1100);
            let mut sinks = vec![FakeFrameSink::default()];
            worker.publish_frame_to(start + Duration::from_millis(1100), 1100, &mut sinks);
            assert!(
                worker.latest_admitted.read().is_none(),
                "stale pixels admitted after {action}"
            );
            assert!(
                sinks[0].messages.is_empty(),
                "stale pixels published after {action}"
            );
            fix2_assert_revalidated(&mut worker, start, 1200, generation);
            let _ = std::fs::remove_dir_all(&worker.config.data_dir);
        }
    }

    #[test]
    fn fix2r2_invalidate_during_grab_drops_old_and_new_pixels() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        fix2_assert_revalidated(&mut worker, start, 0, 0);
        let config = worker.config.clone();
        worker.source.after_grab = Some(Arc::new(move || config.invalidate_user()));
        worker
            .source
            .queue_sized_frame(DEFAULT_PROFILE_SIDE, DEFAULT_PROFILE_SIDE, 200);
        worker.cycle_at(start + Duration::from_millis(1100), 1100);
        assert!(worker.latest_admitted.read().is_none());
        assert_eq!(worker.phase_reason(1100), Some("user_invalidated"));
    }

    #[test]
    fn fix2r2_retry_between_admit_and_publish_suppresses_old_frame() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        fix2_assert_revalidated(&mut worker, start, 0, 0);
        worker
            .source
            .queue_sized_frame(DEFAULT_PROFILE_SIDE, DEFAULT_PROFILE_SIDE, 200);
        worker.capture_one(start + Duration::from_millis(1100), 1100, None);
        worker.config.retry();
        let mut sinks = vec![FakeFrameSink::default()];
        worker.publish_frame_to(start + Duration::from_millis(1101), 1101, &mut sinks);
        assert!(sinks[0].messages.is_empty());
    }

    #[test]
    fn fix2r2_deferred_invalidation_does_not_reassert_after_retry() {
        let _gate = lock(&STATUS_TEST_GATE);
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        worker.config.invalidate_user();
        worker.config.retry();
        // Model the worker consuming an earlier invalidation after the retry command.
        worker.invalidate_for_user(0);
        assert_ne!(worker.phase_reason(0), Some("user_invalidated"));
        fix2_assert_revalidated(&mut worker, start, 100, 0);
    }

    #[test]
    fn fix2r2_legacy_or_other_profile_keeps_fps_but_resets_crop_without_rewriting() {
        for profile in [None, Some("p1080-default-v1"), Some("future-other-profile")] {
            let dir = test_config_dir("profile-migration");
            std::fs::create_dir_all(&dir).unwrap();
            let mut saved = json!({"schema_version": 1, "crop": {"dx": 3, "dy": -1, "dw": -2, "dh": -2}, "display_fps": 15, "crop_confirmed": true});
            if let Some(profile) = profile {
                saved["profile"] = json!(profile);
            }
            let bytes = serde_json::to_vec(&saved).unwrap();
            let path = dir.join(CAPTURE_CONFIG_FILE);
            std::fs::write(&path, &bytes).unwrap();
            let store = CaptureConfigStore::open(dir.clone());
            assert_eq!(store.get().display_fps, 15, "{profile:?}");
            assert_eq!(store.get().crop, CropOffsets::default(), "{profile:?}");
            assert!(!store.get().crop_confirmed);
            store.retry();
            store.set(CaptureConfigPatch::default()).unwrap();
            store
                .set(CaptureConfigPatch {
                    crop: Some(CropOffsets::default()),
                    ..CaptureConfigPatch::default()
                })
                .unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            store
                .set(CaptureConfigPatch {
                    display_fps: Some(30),
                    ..CaptureConfigPatch::default()
                })
                .unwrap();
            let rewritten: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(rewritten["profile"], "p1080-default-v2");
            assert_eq!(rewritten["schema_version"], 1);
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn fix2r2_matching_profile_round_trip_keeps_crop_and_confirmation() {
        let dir = test_config_dir("profile-round-trip");
        let store = CaptureConfigStore::open(dir.clone());
        let crop = CropOffsets {
            dx: 3,
            dy: -1,
            dw: -2,
            dh: -2,
        };
        store
            .set(CaptureConfigPatch {
                crop: Some(crop),
                display_fps: Some(15),
                paused: None,
            })
            .unwrap();
        store.confirm_crop().unwrap();
        let bytes = std::fs::read(dir.join(CAPTURE_CONFIG_FILE)).unwrap();
        let saved: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(saved["profile"], "p1080-default-v2");
        let loaded = CaptureConfigStore::open(dir.clone()).get();
        assert_eq!(loaded.crop, crop);
        assert!(loaded.crop_confirmed);
        assert_eq!(loaded.display_fps, 15);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn fix2r2_native_unsupported_reports_unavailable_with_detail_and_recovers() {
        let _gate = lock(&STATUS_TEST_GATE);
        for message in [
            "DXGI format is not BGRA",
            "duplication API unavailable",
            "2560x1440 unsupported",
        ] {
            let start = Instant::now();
            let mut worker = fix2_worker(start);
            worker.source.close();
            worker.source_open = false;
            worker.source.open_error = Some(OpenError::Unsupported(message.to_owned()));
            worker.cycle_at(start, 0);
            worker.publish_status(start, 0);
            let status = lock(&worker.events.0).last().unwrap().1.clone();
            assert_eq!(status["phase"], "unsupported");
            assert_eq!(status["phase_reason"], "capture_unavailable");
            assert_eq!(status["phase_detail"], message);
            let generation = worker.controller.as_ref().unwrap().generation().unwrap();
            worker.source.open_error = None;
            fix2_assert_revalidated(&mut worker, start, 600, generation);
            worker.publish_status(start + Duration::from_millis(1600), 1600);
            assert!(lock(&worker.events.0).last().unwrap().1["phase_detail"].is_null());
        }
    }

    #[test]
    fn fix2r2_resolution_reason_requires_observed_geometry_and_detail_is_nullable() {
        let _gate = lock(&STATUS_TEST_GATE);
        let unknown = unknown_capture_status();
        assert!(unknown.as_object().unwrap().contains_key("phase_detail"));
        assert!(unknown["phase_detail"].is_null());
        let start = Instant::now();
        let mut worker = fix2_worker(start);
        worker.source.output.as_mut().unwrap().width = 2560;
        worker.source.output.as_mut().unwrap().height = 1440;
        worker.source.profile = None;
        worker.cycle_at(start, 0);
        worker.publish_status(start, 0);
        let status = lock(&worker.events.0).last().unwrap().1.clone();
        assert_eq!(status["phase_reason"], "unsupported_resolution");
        assert!(status.as_object().unwrap().contains_key("phase_detail"));
        assert!(status["phase_detail"].is_null());
    }
}
