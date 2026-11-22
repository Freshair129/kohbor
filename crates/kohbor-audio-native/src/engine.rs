use crate::master_volume_percent;
use kohbor_audio::{PlaybackId, SinkError, SinkEvent};
#[cfg(feature = "latency-probe")]
use std::sync::atomic::fence;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
#[cfg(feature = "latency-probe")]
use std::time::Duration;
use std::time::Instant;

pub(super) const COMMAND_QUEUE_CAPACITY: usize = 8;
pub(super) const EVENT_QUEUE_CAPACITY: usize = 16;
const RETIREMENT_QUEUE_CAPACITY: usize = COMMAND_QUEUE_CAPACITY + 1;
const INACTIVE_GENERATION: u64 = 0;
const RAMP_SAMPLE_COUNT: usize = 480;

#[derive(Clone, Copy, Debug)]
struct GainRamp {
    start_gain: f32,
    target_gain: f32,
    current_gain: f32,
    remaining_samples: usize,
}

impl GainRamp {
    fn new(percent: u8) -> Self {
        let gain = f32::from(percent.min(100)) / 100.0;
        Self {
            start_gain: gain,
            target_gain: gain,
            current_gain: gain,
            remaining_samples: 0,
        }
    }

    fn set_percent(&mut self, percent: u8) {
        let target_gain = f32::from(percent.min(100)) / 100.0;
        if target_gain != self.target_gain {
            self.start_gain = self.current_gain;
            self.target_gain = target_gain;
            self.remaining_samples = RAMP_SAMPLE_COUNT;
        }
    }

    fn next_gain(&mut self) -> f32 {
        if self.remaining_samples > 0 {
            let completed = RAMP_SAMPLE_COUNT - self.remaining_samples + 1;
            let progress = completed as f32 / RAMP_SAMPLE_COUNT as f32;
            self.current_gain = if self.remaining_samples == 1 {
                self.target_gain
            } else {
                self.start_gain + (self.target_gain - self.start_gain) * progress
            };
            self.remaining_samples -= 1;
        }
        self.current_gain
    }

    #[cfg(test)]
    fn current(&self) -> f32 {
        self.current_gain
    }

    fn apply(&mut self, samples: &mut [i16], percent: u8) {
        self.set_percent(percent);
        if self.remaining_samples == 0 {
            if self.target_gain == 1.0 {
                return;
            }
            if self.target_gain == 0.0 {
                samples.fill(0);
                return;
            }
            for sample in samples {
                *sample = scale_sample(*sample, self.target_gain);
            }
            return;
        }
        for sample in samples {
            *sample = scale_sample(*sample, self.next_gain());
        }
    }
}

fn scale_sample(sample: i16, gain: f32) -> i16 {
    if gain == 0.0 {
        return 0;
    }
    if gain == 1.0 {
        return sample;
    }
    (f32::from(sample) * gain)
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

fn apply_master_gain(samples: &mut [i16], ramp: &mut GainRamp, percent: u8) {
    ramp.apply(samples, percent);
}

#[cfg(feature = "latency-probe")]
pub(crate) struct FirstFillProbeState {
    base: Instant,
    sequence: AtomicU64,
    first_fill_nanos: AtomicU64,
    playback_id: AtomicU64,
    cleared_seq: AtomicU64,
}

#[cfg(feature = "latency-probe")]
#[derive(Clone)]
pub struct FirstFillProbe {
    pub(crate) inner: Arc<FirstFillProbeState>,
}

#[cfg(feature = "latency-probe")]
impl FirstFillProbe {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(FirstFillProbeState {
                base: Instant::now(),
                sequence: AtomicU64::new(0),
                first_fill_nanos: AtomicU64::new(0),
                playback_id: AtomicU64::new(0),
                cleared_seq: AtomicU64::new(0),
            }),
        }
    }

    /// Return the most recently observed first render fill.
    pub fn latest(&self) -> Option<(PlaybackId, Instant)> {
        for _ in 0..8 {
            let sequence = self.inner.sequence.load(Ordering::Acquire);
            if sequence & 1 != 0 {
                continue;
            }

            let nanos = self.inner.first_fill_nanos.load(Ordering::Relaxed);
            let playback_id = self.inner.playback_id.load(Ordering::Relaxed);
            fence(Ordering::Acquire);
            if self.inner.sequence.load(Ordering::Acquire) != sequence {
                continue;
            }
            if sequence == self.inner.cleared_seq.load(Ordering::Acquire)
                || playback_id == INACTIVE_GENERATION
            {
                return None;
            }

            let instant = self.inner.base.checked_add(Duration::from_nanos(nanos))?;
            return Some((PlaybackId(playback_id), instant));
        }
        None
    }

    /// Clear the latest observation without writing fields owned by the render thread.
    pub fn clear(&self) {
        loop {
            let sequence = self.inner.sequence.load(Ordering::Acquire);
            if sequence & 1 != 0 || self.inner.sequence.load(Ordering::Acquire) != sequence {
                continue;
            }
            self.inner.cleared_seq.store(sequence, Ordering::Release);
            return;
        }
    }
}

#[cfg(feature = "latency-probe")]
impl FirstFillProbeState {
    fn record(&self, id: PlaybackId) {
        let nanos = Instant::now()
            .saturating_duration_since(self.base)
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        self.record_nanos(id, nanos);
    }

    fn record_nanos(&self, id: PlaybackId, nanos: u64) {
        self.sequence.fetch_add(1, Ordering::Release);
        fence(Ordering::Acquire);
        self.first_fill_nanos.store(nanos, Ordering::Relaxed);
        self.playback_id.store(id.0, Ordering::Relaxed);
        self.sequence.fetch_add(1, Ordering::Release);
    }
}

const DIAGNOSTIC_SLOT_COUNT: usize = COMMAND_QUEUE_CAPACITY + 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RenderObservation {
    pub(crate) lateness_ns: Option<i64>,
    pub(crate) period_ns: Option<u64>,
    pub(crate) available_frames: u32,
    pub(crate) requested_frames: u32,
    pub(crate) padding_frames: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RenderStatsSnapshot {
    first_fill_latency_ns: Option<u64>,
    frames_available_total: Option<u64>,
    frames_requested_total: Option<u64>,
    late_fills: Option<u64>,
    max_lateness_ns: Option<i64>,
    min_padding_frames: Option<u32>,
}

#[derive(Default)]
struct RenderStats {
    first_fill_latency_ns: Option<u64>,
    frames_available_total: u64,
    frames_requested_total: u64,
    late_fills: u64,
    max_lateness_ns: Option<i64>,
    min_padding_frames: Option<u32>,
    callbacks_seen: bool,
    timeouts_seen: bool,
}

impl RenderStats {
    fn record_callback(&mut self, observation: RenderObservation) {
        self.callbacks_seen = true;
        self.frames_available_total = self
            .frames_available_total
            .saturating_add(u64::from(observation.available_frames));
        self.frames_requested_total = self
            .frames_requested_total
            .saturating_add(u64::from(observation.requested_frames));
        if let Some(lateness_ns) = observation.lateness_ns {
            self.max_lateness_ns = Some(
                self.max_lateness_ns
                    .map_or(lateness_ns, |current| current.max(lateness_ns)),
            );
        }
        self.min_padding_frames = Some(
            self.min_padding_frames
                .map_or(observation.padding_frames, |current| {
                    current.min(observation.padding_frames)
                }),
        );
        let exceeded_period = observation
            .lateness_ns
            .zip(observation.period_ns)
            .is_some_and(|(lateness_ns, period_ns)| {
                lateness_ns > 0
                    && u64::try_from(lateness_ns).is_ok_and(|lateness_ns| lateness_ns > period_ns)
            });
        if observation.padding_frames == 0 || exceeded_period {
            self.late_fills = self.late_fills.saturating_add(1);
        }
    }

    fn record_timeout(&mut self) {
        self.timeouts_seen = true;
        self.late_fills = self.late_fills.saturating_add(1);
    }

    fn record_first_fill(&mut self, latency_ns: u64) {
        if self.first_fill_latency_ns.is_none() {
            self.first_fill_latency_ns = Some(latency_ns);
        }
    }

    fn snapshot(&self) -> RenderStatsSnapshot {
        RenderStatsSnapshot {
            first_fill_latency_ns: self.first_fill_latency_ns,
            frames_available_total: self.callbacks_seen.then_some(self.frames_available_total),
            frames_requested_total: self.callbacks_seen.then_some(self.frames_requested_total),
            late_fills: (self.callbacks_seen || self.timeouts_seen).then_some(self.late_fills),
            max_lateness_ns: self.max_lateness_ns,
            min_padding_frames: self.min_padding_frames,
        }
    }
}

struct PlaybackMetrics {
    playback_id: AtomicU64,
    pcm_frames: AtomicU64,
    cursor_frames: AtomicU64,
    first_fill_latency_ns: AtomicU64,
    first_fill_measured: AtomicBool,
    frames_rendered: AtomicU64,
    rendered_duration_known: AtomicBool,
    snapshot_pending: AtomicBool,
    write_error_count: AtomicU64,
    event_timeout_count: AtomicU64,
    frames_available_total: AtomicU64,
    frames_requested_total: AtomicU64,
    callback_frames_measured: AtomicBool,
    late_fills: AtomicU64,
    late_fills_measured: AtomicBool,
    max_lateness_ns: AtomicI64,
    lateness_measured: AtomicBool,
    min_padding_frames: AtomicU64,
    padding_measured: AtomicBool,
}

impl PlaybackMetrics {
    fn new() -> Self {
        Self {
            playback_id: AtomicU64::new(INACTIVE_GENERATION),
            pcm_frames: AtomicU64::new(0),
            cursor_frames: AtomicU64::new(0),
            first_fill_latency_ns: AtomicU64::new(0),
            first_fill_measured: AtomicBool::new(false),
            frames_rendered: AtomicU64::new(0),
            rendered_duration_known: AtomicBool::new(true),
            snapshot_pending: AtomicBool::new(false),
            write_error_count: AtomicU64::new(0),
            event_timeout_count: AtomicU64::new(0),
            frames_available_total: AtomicU64::new(0),
            frames_requested_total: AtomicU64::new(0),
            callback_frames_measured: AtomicBool::new(false),
            late_fills: AtomicU64::new(0),
            late_fills_measured: AtomicBool::new(false),
            max_lateness_ns: AtomicI64::new(0),
            lateness_measured: AtomicBool::new(false),
            min_padding_frames: AtomicU64::new(0),
            padding_measured: AtomicBool::new(false),
        }
    }

    fn reset(&self, playback_id: u64, pcm_frames: u64) {
        self.playback_id
            .store(INACTIVE_GENERATION, Ordering::Release);
        self.pcm_frames.store(pcm_frames, Ordering::Relaxed);
        self.cursor_frames.store(0, Ordering::Relaxed);
        self.first_fill_latency_ns.store(0, Ordering::Relaxed);
        self.first_fill_measured.store(false, Ordering::Relaxed);
        self.frames_rendered.store(0, Ordering::Relaxed);
        self.rendered_duration_known.store(true, Ordering::Relaxed);
        self.snapshot_pending.store(false, Ordering::Relaxed);
        self.write_error_count.store(0, Ordering::Relaxed);
        self.event_timeout_count.store(0, Ordering::Relaxed);
        self.frames_available_total.store(0, Ordering::Relaxed);
        self.frames_requested_total.store(0, Ordering::Relaxed);
        self.callback_frames_measured
            .store(false, Ordering::Relaxed);
        self.late_fills.store(0, Ordering::Relaxed);
        self.late_fills_measured.store(false, Ordering::Relaxed);
        self.max_lateness_ns.store(0, Ordering::Relaxed);
        self.lateness_measured.store(false, Ordering::Relaxed);
        self.min_padding_frames.store(0, Ordering::Relaxed);
        self.padding_measured.store(false, Ordering::Relaxed);
        self.playback_id.store(playback_id, Ordering::Release);
    }
}

pub(crate) struct PlaybackMetricsSnapshot {
    pub(crate) pcm_frames: u64,
    pub(crate) cursor_frames: u64,
    pub(crate) first_fill_latency_ns: Option<u64>,
    pub(crate) frames_rendered: Option<u64>,
    pub(crate) rendered_duration_known: bool,
    pub(crate) snapshot_pending: bool,
    pub(crate) write_error_count: Option<u64>,
    pub(crate) event_timeout_count: u64,
    pub(crate) frames_available_total: Option<u64>,
    pub(crate) frames_requested_total: Option<u64>,
    pub(crate) late_fills: Option<u64>,
    pub(crate) max_lateness_ns: Option<i64>,
    pub(crate) min_padding_frames: Option<u64>,
}

pub(crate) struct SharedDiagnostics {
    slots: [PlaybackMetrics; DIAGNOSTIC_SLOT_COUNT],
}

impl SharedDiagnostics {
    fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| PlaybackMetrics::new()),
        }
    }

    fn slot(&self, playback_id: u64) -> &PlaybackMetrics {
        let index = (playback_id % DIAGNOSTIC_SLOT_COUNT as u64) as usize;
        &self.slots[index]
    }

    fn begin_playback(&self, playback_id: u64, pcm_frames: u64) {
        self.slot(playback_id).reset(playback_id, pcm_frames);
    }

    fn publish_stats(&self, playback_id: u64, stats: RenderStatsSnapshot) {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) != playback_id {
            return;
        }
        if let Some(value) = stats.first_fill_latency_ns {
            slot.first_fill_latency_ns.store(value, Ordering::Relaxed);
            slot.first_fill_measured.store(true, Ordering::Release);
        }
        if let (Some(available), Some(requested)) =
            (stats.frames_available_total, stats.frames_requested_total)
        {
            slot.frames_available_total
                .store(available, Ordering::Relaxed);
            slot.frames_requested_total
                .store(requested, Ordering::Relaxed);
            slot.callback_frames_measured.store(true, Ordering::Release);
        }
        if let Some(value) = stats.late_fills {
            slot.late_fills.store(value, Ordering::Relaxed);
            slot.late_fills_measured.store(true, Ordering::Release);
        }
        if let Some(value) = stats.max_lateness_ns {
            slot.max_lateness_ns.store(value, Ordering::Relaxed);
            slot.lateness_measured.store(true, Ordering::Release);
        }
        if let Some(value) = stats.min_padding_frames {
            slot.min_padding_frames
                .store(u64::from(value), Ordering::Relaxed);
            slot.padding_measured.store(true, Ordering::Release);
        }
    }

    fn record_rendered_frames(&self, playback_id: u64, frames: usize) {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) == playback_id {
            slot.frames_rendered
                .fetch_add(frames as u64, Ordering::Relaxed);
            slot.rendered_duration_known.store(true, Ordering::Release);
            slot.snapshot_pending.store(false, Ordering::Release);
        }
    }

    fn record_cursor_frames(&self, playback_id: u64, frames: usize) {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) == playback_id {
            slot.cursor_frames.store(frames as u64, Ordering::Release);
        }
    }

    fn record_write_error(&self, playback_id: u64) {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) == playback_id {
            slot.write_error_count.fetch_add(1, Ordering::Relaxed);
            slot.rendered_duration_known.store(true, Ordering::Relaxed);
            slot.snapshot_pending.store(false, Ordering::Release);
        }
    }

    fn record_event_timeout(&self, playback_id: u64) {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) == playback_id {
            slot.event_timeout_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn mark_render_write_pending(&self, playback_id: u64) {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) == playback_id {
            slot.rendered_duration_known.store(false, Ordering::Release);
            slot.snapshot_pending.store(true, Ordering::Release);
        }
    }

    fn complete_render_write_without_samples(&self, playback_id: u64) {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) == playback_id {
            slot.rendered_duration_known.store(true, Ordering::Release);
            slot.snapshot_pending.store(false, Ordering::Release);
        }
    }

    pub(crate) fn snapshot(&self, playback_id: u64) -> Option<PlaybackMetricsSnapshot> {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) != playback_id {
            return None;
        }
        let snapshot_pending = slot.snapshot_pending.load(Ordering::Acquire);
        Some(PlaybackMetricsSnapshot {
            pcm_frames: slot.pcm_frames.load(Ordering::Relaxed),
            cursor_frames: slot.cursor_frames.load(Ordering::Acquire),
            first_fill_latency_ns: slot
                .first_fill_measured
                .load(Ordering::Acquire)
                .then(|| slot.first_fill_latency_ns.load(Ordering::Relaxed)),
            frames_rendered: (!snapshot_pending)
                .then(|| slot.frames_rendered.load(Ordering::Relaxed)),
            rendered_duration_known: slot.rendered_duration_known.load(Ordering::Acquire),
            snapshot_pending,
            write_error_count: (!snapshot_pending)
                .then(|| slot.write_error_count.load(Ordering::Relaxed)),
            event_timeout_count: slot.event_timeout_count.load(Ordering::Relaxed),
            frames_available_total: slot
                .callback_frames_measured
                .load(Ordering::Acquire)
                .then(|| slot.frames_available_total.load(Ordering::Relaxed)),
            frames_requested_total: slot
                .callback_frames_measured
                .load(Ordering::Acquire)
                .then(|| slot.frames_requested_total.load(Ordering::Relaxed)),
            late_fills: slot
                .late_fills_measured
                .load(Ordering::Acquire)
                .then(|| slot.late_fills.load(Ordering::Relaxed)),
            max_lateness_ns: slot
                .lateness_measured
                .load(Ordering::Acquire)
                .then(|| slot.max_lateness_ns.load(Ordering::Relaxed)),
            min_padding_frames: slot
                .padding_measured
                .load(Ordering::Acquire)
                .then(|| slot.min_padding_frames.load(Ordering::Relaxed)),
        })
    }
}

enum Command {
    Submit {
        id: PlaybackId,
        generation: u64,
        samples: Arc<[i16]>,
        submitted_at: Instant,
    },
}

pub(crate) struct RenderedBlock {
    pub(crate) playback_id: PlaybackId,
    pub(crate) frames: usize,
}

struct Playback {
    id: PlaybackId,
    generation: u64,
    samples: Arc<[i16]>,
    cursor: usize,
    finished_notified: bool,
    cancelled_by_fade: bool,
    fade_out: Option<FadeOutRamp>,
    submitted_at: Instant,
    stats: RenderStats,
}

#[derive(Clone, Copy)]
struct FadeOutRamp {
    total_samples: usize,
    rendered_samples: usize,
}

impl FadeOutRamp {
    fn gain(&self) -> f32 {
        if self.total_samples <= 1 {
            0.0
        } else {
            1.0 - self.rendered_samples as f32 / (self.total_samples - 1) as f32
        }
    }

    fn remaining_samples(&self) -> usize {
        self.total_samples.saturating_sub(self.rendered_samples)
    }
}

fn begin_fade(playback: &mut Playback, requested_samples: usize) {
    if playback.fade_out.is_some() {
        return;
    }
    let remaining = playback.samples.len().saturating_sub(playback.cursor);
    let total_samples = requested_samples.min(remaining);
    if total_samples == 0 {
        playback.cancelled_by_fade = true;
    } else {
        playback.fade_out = Some(FadeOutRamp {
            total_samples,
            rendered_samples: 0,
        });
    }
}

/// Device-independent render state. `fill` is called only on the audio-owner
/// thread and uses fixed-capacity channels plus the current buffer; it does not
/// wait for the coordinator.
pub(crate) struct Engine {
    commands: Receiver<Command>,
    retired: SyncSender<Arc<[i16]>>,
    events: SyncSender<SinkEvent>,
    current_generation: Arc<AtomicU64>,
    fade_generation: Arc<AtomicU64>,
    fade_samples: Arc<AtomicUsize>,
    active: Option<Playback>,
    fade_tail: Option<Playback>,
    pending_command: Option<Command>,
    diagnostics: Arc<SharedDiagnostics>,
    gain_ramp: GainRamp,
    #[cfg(feature = "latency-probe")]
    first_fill_probe: Option<Arc<FirstFillProbeState>>,
}

/// Coordinator-side half of the bounded hand-off. Callers are serialized by
/// `&mut self`; render-side cancellation observes only the atomic generation.
pub(crate) struct EngineHandle {
    commands: SyncSender<Command>,
    retired: Receiver<Arc<[i16]>>,
    event_sender: SyncSender<SinkEvent>,
    events: Receiver<SinkEvent>,
    current_generation: Arc<AtomicU64>,
    fade_generation: Arc<AtomicU64>,
    fade_samples: Arc<AtomicUsize>,
    device_lost_pending: Arc<AtomicBool>,
    device_stop_requested: Arc<AtomicBool>,
    next_generation: u64,
    current_id: Option<PlaybackId>,
    diagnostics: Arc<SharedDiagnostics>,
}

/// Compact device notifications from WASAPI. Device loss has its own one-slot
/// atomic flag and is returned before queued noncritical events. When the event
/// channel is full, the newest `Finished` or `DeviceChanged` event is dropped.
pub(crate) struct DeviceEventSender {
    events: SyncSender<SinkEvent>,
    device_lost_pending: Arc<AtomicBool>,
    device_stop_requested: Arc<AtomicBool>,
}

impl DeviceEventSender {
    pub(crate) fn clone_for_callback(&self) -> Self {
        Self {
            events: Clone::clone(&self.events),
            device_lost_pending: Arc::clone(&self.device_lost_pending),
            device_stop_requested: Arc::clone(&self.device_stop_requested),
        }
    }

    pub(crate) fn device_lost(&self) {
        self.device_stop_requested.store(true, Ordering::Release);
        self.device_lost_pending.store(true, Ordering::Release);
    }

    pub(crate) fn device_changed(&self) {
        let _ = self.events.try_send(SinkEvent::DeviceChanged);
    }

    pub(crate) fn stop_requested(&self) -> bool {
        self.device_stop_requested.load(Ordering::Acquire)
    }
}

impl Engine {
    pub(crate) fn new() -> (Self, EngineHandle) {
        let (command_sender, commands) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let (retired_sender, retired) = mpsc::sync_channel(RETIREMENT_QUEUE_CAPACITY);
        let (event_sender, events) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let current_generation = Arc::new(AtomicU64::new(INACTIVE_GENERATION));
        let fade_generation = Arc::new(AtomicU64::new(INACTIVE_GENERATION));
        let fade_samples = Arc::new(AtomicUsize::new(0));
        let device_lost_pending = Arc::new(AtomicBool::new(false));
        let device_stop_requested = Arc::new(AtomicBool::new(false));
        let diagnostics = Arc::new(SharedDiagnostics::new());
        let handle = EngineHandle {
            commands: command_sender,
            retired,
            event_sender: Clone::clone(&event_sender),
            events,
            current_generation: Arc::clone(&current_generation),
            fade_generation: Arc::clone(&fade_generation),
            fade_samples: Arc::clone(&fade_samples),
            device_lost_pending: Arc::clone(&device_lost_pending),
            device_stop_requested: Arc::clone(&device_stop_requested),
            next_generation: INACTIVE_GENERATION,
            current_id: None,
            diagnostics: Arc::clone(&diagnostics),
        };
        (
            Self {
                commands,
                retired: retired_sender,
                events: event_sender,
                current_generation,
                fade_generation,
                fade_samples,
                active: None,
                fade_tail: None,
                pending_command: None,
                diagnostics,
                gain_ramp: GainRamp::new(master_volume_percent()),
                #[cfg(feature = "latency-probe")]
                first_fill_probe: None,
            },
            handle,
        )
    }

    #[cfg(feature = "latency-probe")]
    pub(crate) fn set_first_fill_probe(&mut self, probe: Arc<FirstFillProbeState>) {
        self.first_fill_probe = Some(probe);
    }

    /// Fill the render block with the active samples, padding the final block or
    /// an idle/cancelled slot with silence.
    #[cfg(test)]
    pub(crate) fn fill(&mut self, out: &mut [i16]) {
        let _ = self.fill_observed(out, None);
    }

    pub(crate) fn fill_observed(
        &mut self,
        out: &mut [i16],
        observation: Option<RenderObservation>,
    ) -> Option<RenderedBlock> {
        out.fill(0);
        if !self.process_commands() {
            if let Some(observation) = observation {
                self.record_active_callback(observation);
            }
            return None;
        }
        let rendered_active = self.render_active(out, observation);
        let rendered_tail = self.render_fade_tail(out, observation);
        apply_master_gain(out, &mut self.gain_ramp, master_volume_percent());
        self.retire_terminal_playbacks();
        rendered_active.or(rendered_tail)
    }

    pub(crate) fn record_no_space_callback(&mut self, observation: RenderObservation) {
        self.record_active_callback(observation);
    }

    pub(crate) fn record_event_timeout(&mut self) {
        let diagnostics = &self.diagnostics;
        if let Some(active) = self.active.as_mut() {
            if active.generation == self.current_generation.load(Ordering::Acquire) {
                active.stats.record_timeout();
                diagnostics.record_event_timeout(active.id.0);
                diagnostics.publish_stats(active.id.0, active.stats.snapshot());
            }
        }
    }

    pub(crate) fn record_rendered_frames(&self, playback_id: PlaybackId, frames: usize) {
        self.diagnostics
            .record_rendered_frames(playback_id.0, frames);
    }

    pub(crate) fn record_write_error(&self, playback_id: PlaybackId) {
        self.diagnostics.record_write_error(playback_id.0);
    }

    fn record_active_callback(&mut self, observation: RenderObservation) {
        let diagnostics = &self.diagnostics;
        if let Some(active) = self.active.as_mut() {
            if active.generation == self.current_generation.load(Ordering::Acquire) {
                active.stats.record_callback(observation);
                diagnostics.publish_stats(active.id.0, active.stats.snapshot());
            }
        }
    }

    fn process_commands(&mut self) -> bool {
        if !self.retire_terminal_playbacks() {
            return false;
        }

        for _ in 0..COMMAND_QUEUE_CAPACITY {
            let command = match self.pending_command.take() {
                Some(command) => command,
                None => match self.commands.try_recv() {
                    Ok(command) => command,
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                },
            };

            if !self.process_command(command) {
                return false;
            }
            if self.pending_command.is_some() {
                break;
            }
        }
        true
    }

    fn process_command(&mut self, command: Command) -> bool {
        let Command::Submit {
            id,
            generation,
            samples,
            submitted_at,
        } = command;
        if self.current_generation.load(Ordering::Acquire) != generation {
            return match self.retired.try_send(samples) {
                Ok(()) => true,
                Err(TrySendError::Full(samples) | TrySendError::Disconnected(samples)) => {
                    self.pending_command = Some(Command::Submit {
                        id,
                        generation,
                        samples,
                        submitted_at,
                    });
                    false
                }
            };
        }

        if self
            .active
            .as_ref()
            .is_some_and(|active| self.is_fading(active.generation))
        {
            if self.fade_tail.is_some() && !self.retire_fade_tail() {
                self.pending_command = Some(Command::Submit {
                    id,
                    generation,
                    samples,
                    submitted_at,
                });
                return false;
            }
            if let Some(mut previous) = self.active.take() {
                begin_fade(&mut previous, self.fade_samples.load(Ordering::Relaxed));
                self.fade_tail = Some(previous);
            }
        } else if self.active.is_some() && !self.retire_active() {
            self.pending_command = Some(Command::Submit {
                id,
                generation,
                samples,
                submitted_at,
            });
            return false;
        }

        self.active = Some(Playback {
            id,
            generation,
            samples,
            cursor: 0,
            finished_notified: false,
            cancelled_by_fade: false,
            fade_out: None,
            submitted_at,
            stats: RenderStats::default(),
        });
        true
    }

    fn render_active(
        &mut self,
        out: &mut [i16],
        observation: Option<RenderObservation>,
    ) -> Option<RenderedBlock> {
        let generation = self.current_generation.load(Ordering::Acquire);
        let fade_generation = self.fade_generation.load(Ordering::Acquire);
        let requested_fade = self.fade_samples.load(Ordering::Relaxed);
        let diagnostics = &self.diagnostics;
        let active = self.active.as_mut()?;
        let fade_authorized = fade_generation == active.generation && requested_fade > 0;
        if generation != active.generation && !fade_authorized {
            return None;
        }

        if fade_authorized && active.fade_out.is_none() {
            begin_fade(active, requested_fade);
        }

        let remaining = active.samples.len().saturating_sub(active.cursor);
        let fade_remaining = active
            .fade_out
            .as_ref()
            .map_or(usize::MAX, FadeOutRamp::remaining_samples);
        let count = remaining.min(out.len()).min(fade_remaining);
        let end = active.cursor.checked_add(count)?;
        if count > 0 {
            diagnostics.mark_render_write_pending(active.id.0);
        }
        out[..count].copy_from_slice(&active.samples[active.cursor..end]);
        if let Some(fade) = active.fade_out.as_mut() {
            for sample in &mut out[..count] {
                *sample = scale_sample(*sample, fade.gain());
                fade.rendered_samples += 1;
            }
        }
        if let Some(observation) = observation {
            active.stats.record_callback(observation);
        }
        let latest_generation = self.current_generation.load(Ordering::Acquire);
        let fade_still_authorized =
            self.fade_generation.load(Ordering::Acquire) == active.generation;
        if latest_generation != active.generation && !fade_still_authorized {
            out.fill(0);
            if count > 0 {
                diagnostics.complete_render_write_without_samples(active.id.0);
            }
            diagnostics.publish_stats(active.id.0, active.stats.snapshot());
            return None;
        }
        if active.cursor == 0 && count > 0 {
            active.stats.record_first_fill(
                Instant::now()
                    .saturating_duration_since(active.submitted_at)
                    .as_nanos()
                    .min(u128::from(u64::MAX)) as u64,
            );
        }
        #[cfg(feature = "latency-probe")]
        if active.cursor == 0 && count > 0 {
            if let Some(probe) = self.first_fill_probe.as_ref() {
                probe.record(active.id);
            }
        }
        diagnostics.publish_stats(active.id.0, active.stats.snapshot());
        let rendered = (count > 0).then_some(RenderedBlock {
            playback_id: active.id,
            frames: count,
        });
        active.cursor = end;
        diagnostics.record_cursor_frames(active.id.0, active.cursor);

        let fade_completed = active
            .fade_out
            .as_ref()
            .is_some_and(|fade| fade.remaining_samples() == 0);
        if fade_completed {
            active.cancelled_by_fade = true;
            let _ = self.fade_generation.compare_exchange(
                active.generation,
                INACTIVE_GENERATION,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        if active.cursor == active.samples.len()
            && !active.finished_notified
            && !active.cancelled_by_fade
        {
            active.finished_notified = true;
            let id = active.id;
            let _ = self.events.try_send(SinkEvent::Finished { id });
            let _ = self.current_generation.compare_exchange(
                generation,
                INACTIVE_GENERATION,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        rendered
    }

    fn render_fade_tail(
        &mut self,
        out: &mut [i16],
        observation: Option<RenderObservation>,
    ) -> Option<RenderedBlock> {
        let fade_generation = self.fade_generation.load(Ordering::Acquire);
        let requested_fade = self.fade_samples.load(Ordering::Relaxed);
        let diagnostics = &self.diagnostics;
        let tail = self.fade_tail.as_mut()?;
        if fade_generation != tail.generation || requested_fade == 0 {
            return None;
        }
        if tail.fade_out.is_none() {
            begin_fade(tail, requested_fade);
        }
        if tail.cancelled_by_fade {
            return None;
        }

        let remaining = tail.samples.len().saturating_sub(tail.cursor);
        let fade_remaining = tail
            .fade_out
            .as_ref()
            .map_or(0, FadeOutRamp::remaining_samples);
        let count = remaining.min(out.len()).min(fade_remaining);
        let end = tail.cursor.checked_add(count)?;
        if count > 0 {
            diagnostics.mark_render_write_pending(tail.id.0);
        }
        if let Some(observation) = observation {
            tail.stats.record_callback(observation);
        }

        if let Some(fade) = tail.fade_out.as_mut() {
            for (offset, source) in tail.samples[tail.cursor..end].iter().copied().enumerate() {
                let faded = scale_sample(source, fade.gain());
                out[offset] = out[offset].saturating_add(faded);
                fade.rendered_samples += 1;
            }
        }

        diagnostics.publish_stats(tail.id.0, tail.stats.snapshot());
        let rendered = (count > 0).then_some(RenderedBlock {
            playback_id: tail.id,
            frames: count,
        });
        tail.cursor = end;
        diagnostics.record_cursor_frames(tail.id.0, tail.cursor);

        let fade_completed = tail
            .fade_out
            .as_ref()
            .is_some_and(|fade| fade.remaining_samples() == 0);
        if fade_completed {
            tail.cancelled_by_fade = true;
            let _ = self.fade_generation.compare_exchange(
                tail.generation,
                INACTIVE_GENERATION,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        rendered
    }

    fn is_fading(&self, generation: u64) -> bool {
        self.fade_generation.load(Ordering::Acquire) == generation
            && self.fade_samples.load(Ordering::Relaxed) > 0
    }

    fn retire_terminal_playbacks(&mut self) -> bool {
        let tail_should_retire = self
            .fade_tail
            .as_ref()
            .is_some_and(|tail| tail.cancelled_by_fade || !self.is_fading(tail.generation));
        if tail_should_retire && !self.retire_fade_tail() {
            return false;
        }

        let active_should_retire = self.active.as_ref().is_some_and(|active| {
            active.finished_notified
                || active.cancelled_by_fade
                || (active.generation != self.current_generation.load(Ordering::Acquire)
                    && !self.is_fading(active.generation))
        });
        !active_should_retire || self.retire_active()
    }

    fn retire_active(&mut self) -> bool {
        let Some(active) = self.active.take() else {
            return true;
        };
        match Self::try_retire(&self.retired, active) {
            Ok(()) => true,
            Err(active) => {
                self.active = Some(active);
                false
            }
        }
    }

    fn retire_fade_tail(&mut self) -> bool {
        let Some(tail) = self.fade_tail.take() else {
            return true;
        };
        match Self::try_retire(&self.retired, tail) {
            Ok(()) => true,
            Err(tail) => {
                self.fade_tail = Some(tail);
                false
            }
        }
    }

    fn try_retire(retired: &SyncSender<Arc<[i16]>>, playback: Playback) -> Result<(), Playback> {
        let Playback {
            id,
            generation,
            samples,
            cursor,
            finished_notified,
            cancelled_by_fade,
            fade_out,
            submitted_at,
            stats,
        } = playback;
        match retired.try_send(samples) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(samples) | TrySendError::Disconnected(samples)) => {
                Err(Playback {
                    id,
                    generation,
                    samples,
                    cursor,
                    finished_notified,
                    cancelled_by_fade,
                    fade_out,
                    submitted_at,
                    stats,
                })
            }
        }
    }
}

impl EngineHandle {
    pub(crate) fn diagnostics(&self) -> Arc<SharedDiagnostics> {
        Arc::clone(&self.diagnostics)
    }

    pub(crate) fn submit(&mut self, id: PlaybackId, samples: Arc<[i16]>) -> Result<(), SinkError> {
        self.reclaim_retired();
        let Some(generation) = self.next_generation.checked_add(1) else {
            self.current_generation
                .store(INACTIVE_GENERATION, Ordering::Release);
            self.current_id = None;
            return Err(SinkError);
        };
        self.next_generation = generation;
        self.current_id = Some(id);
        let submitted_at = Instant::now();
        self.diagnostics
            .begin_playback(id.0, u64::try_from(samples.len()).unwrap_or(u64::MAX));
        self.current_generation.store(generation, Ordering::Release);
        let command = Command::Submit {
            id,
            generation,
            samples,
            submitted_at,
        };
        match self.commands.try_send(command) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(command) | TrySendError::Disconnected(command)) => {
                self.current_generation
                    .compare_exchange(
                        generation,
                        INACTIVE_GENERATION,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .ok();
                self.current_id = None;
                drop(command);
                Err(SinkError)
            }
        }
    }

    pub(crate) fn cancel(&mut self, id: PlaybackId) {
        if self.current_id == Some(id) {
            self.current_generation
                .store(INACTIVE_GENERATION, Ordering::Release);
            self.current_id = None;
            self.fade_generation
                .store(INACTIVE_GENERATION, Ordering::Release);
        }
    }

    pub(crate) fn cancel_with_fade(&mut self, id: PlaybackId, fade_samples: usize) {
        if self.current_id != Some(id) {
            return;
        }
        if fade_samples == 0 {
            self.cancel(id);
            return;
        }

        let current_generation = self.current_generation.load(Ordering::Acquire);
        if current_generation != INACTIVE_GENERATION {
            self.fade_samples.store(fade_samples, Ordering::Relaxed);
            self.fade_generation
                .store(current_generation, Ordering::Release);
        }
        self.current_id = None;
    }

    pub(crate) fn poll_event(&mut self) -> Option<SinkEvent> {
        self.reclaim_retired();
        if self.device_lost_pending.swap(false, Ordering::AcqRel) {
            return Some(SinkEvent::DeviceLost);
        }
        self.events.try_recv().ok()
    }

    pub(crate) fn poll_retired(&mut self) -> Option<Arc<[i16]>> {
        self.retired.try_recv().ok()
    }

    pub(crate) fn device_event_sender(&self) -> DeviceEventSender {
        DeviceEventSender {
            events: Clone::clone(&self.event_sender),
            device_lost_pending: Arc::clone(&self.device_lost_pending),
            device_stop_requested: Arc::clone(&self.device_stop_requested),
        }
    }

    fn reclaim_retired(&mut self) {
        while let Some(samples) = self.poll_retired() {
            drop(samples);
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "latency-probe")]
    use super::FirstFillProbe;
    use super::{
        apply_master_gain, scale_sample, Engine, GainRamp, RenderObservation, RenderStats,
        SharedDiagnostics, COMMAND_QUEUE_CAPACITY, EVENT_QUEUE_CAPACITY, RAMP_SAMPLE_COUNT,
    };
    use kohbor_audio::{PlaybackId, SinkError, SinkEvent};
    use std::sync::Arc;
    #[cfg(feature = "latency-probe")]
    use std::time::Instant;

    fn observation(
        lateness_ns: u64,
        period_ns: u64,
        available_frames: u32,
        requested_frames: u32,
        padding_frames: u32,
    ) -> RenderObservation {
        RenderObservation {
            lateness_ns: Some(lateness_ns as i64),
            period_ns: Some(period_ns),
            available_frames,
            requested_frames,
            padding_frames,
        }
    }

    #[test]
    fn settled_zero_volume_is_exact_silence() {
        let mut ramp = GainRamp::new(0);
        let mut samples = [i16::MIN, -1, 0, 1, i16::MAX];

        apply_master_gain(&mut samples, &mut ramp, 0);

        assert_eq!(samples, [0; 5]);
    }

    #[test]
    fn settled_hundred_percent_is_bit_exact_passthrough() {
        let mut ramp = GainRamp::new(100);
        let original = [i16::MIN, -12345, -1, 0, 1, 12345, i16::MAX];
        let mut samples = original;

        apply_master_gain(&mut samples, &mut ramp, 100);

        assert_eq!(samples, original);
    }

    #[test]
    fn settled_sixty_percent_scales_samples() {
        let mut ramp = GainRamp::new(60);
        let mut samples = [10_000, -10_000, 1, -1];

        apply_master_gain(&mut samples, &mut ramp, 60);

        assert_eq!(samples, [6_000, -6_000, 1, -1]);
    }

    #[test]
    fn a_volume_change_scales_the_next_block_of_an_active_clip() {
        let mut ramp = GainRamp::new(100);
        let mut first_block = [10_000; 1];
        apply_master_gain(&mut first_block, &mut ramp, 100);
        assert_eq!(first_block, [10_000]);

        let mut next_block = [10_000; RAMP_SAMPLE_COUNT];
        apply_master_gain(&mut next_block, &mut ramp, 60);

        assert!(next_block[0] < 10_000);
        assert_eq!(next_block[RAMP_SAMPLE_COUNT - 1], 6_000);
    }

    #[test]
    fn gain_sample_conversion_saturates_to_i16() {
        assert_eq!(scale_sample(i16::MAX, 1.5), i16::MAX);
        assert_eq!(scale_sample(i16::MIN, 1.5), i16::MIN);
    }

    #[test]
    fn volume_ramp_is_monotonic_and_ends_exactly_at_target() {
        let mut ramp = GainRamp::new(0);
        ramp.set_percent(100);
        let gains = (0..RAMP_SAMPLE_COUNT)
            .map(|_| ramp.next_gain())
            .collect::<Vec<_>>();

        assert!(gains.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(gains.last().copied(), Some(1.0));
        assert_eq!(ramp.current(), 1.0);

        ramp.set_percent(0);
        let descending = (0..RAMP_SAMPLE_COUNT)
            .map(|_| ramp.next_gain())
            .collect::<Vec<_>>();
        assert!(descending.windows(2).all(|pair| pair[0] >= pair[1]));
        assert_eq!(descending.last().copied(), Some(0.0));
        assert_eq!(ramp.current(), 0.0);
    }

    #[test]
    fn volume_ramp_step_is_bounded_by_ten_milliseconds() {
        let mut ramp = GainRamp::new(0);
        ramp.set_percent(100);
        let gains = (0..RAMP_SAMPLE_COUNT)
            .map(|_| ramp.next_gain())
            .collect::<Vec<_>>();
        let maximum_step = 1.0 / RAMP_SAMPLE_COUNT as f32;

        assert!(gains[0] <= maximum_step + f32::EPSILON);
        assert!(gains
            .windows(2)
            .all(|pair| (pair[1] - pair[0]).abs() <= maximum_step + f32::EPSILON));
    }

    #[test]
    fn render_stats_aggregate_on_time_frames_and_headroom() {
        let mut stats = RenderStats::default();
        stats.record_callback(observation(1_000_000, 10_000_000, 440, 480, 40));
        stats.record_callback(observation(2_000_000, 10_000_000, 460, 480, 20));

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.late_fills, Some(0));
        assert_eq!(snapshot.max_lateness_ns, Some(2_000_000));
        assert_eq!(snapshot.frames_available_total, Some(900));
        assert_eq!(snapshot.frames_requested_total, Some(960));
        assert_eq!(snapshot.min_padding_frames, Some(20));
    }

    #[test]
    fn one_late_callback_counts_once_and_tracks_maximum_lateness() {
        let mut stats = RenderStats::default();
        stats.record_callback(observation(10_000_000, 10_000_000, 480, 480, 0));

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.late_fills, Some(1));
        assert_eq!(snapshot.max_lateness_ns, Some(10_000_000));
    }

    #[test]
    fn empty_padding_fill_counts_as_possible_underrun() {
        let mut stats = RenderStats::default();
        stats.record_callback(observation(0, 10_000_000, 480, 480, 0));

        assert_eq!(stats.snapshot().late_fills, Some(1));
    }

    #[test]
    fn event_timeout_counts_as_late_fill_without_inventing_callback_frames() {
        let mut stats = RenderStats::default();
        stats.record_timeout();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.late_fills, Some(1));
        assert_eq!(snapshot.frames_available_total, None);
        assert_eq!(snapshot.frames_requested_total, None);
        assert_eq!(snapshot.max_lateness_ns, None);
        assert_eq!(snapshot.min_padding_frames, None);
    }

    #[test]
    fn timeout_and_late_callback_are_each_counted_once() {
        let mut stats = RenderStats::default();
        stats.record_timeout();
        stats.record_callback(observation(11_000_000, 10_000_000, 400, 480, 80));

        assert_eq!(stats.snapshot().late_fills, Some(2));
    }

    #[test]
    fn zero_lateness_on_time_callback_is_measured_as_zero_not_unknown() {
        let mut stats = RenderStats::default();
        stats.record_callback(observation(0, 10_000_000, 480, 480, 1));

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.max_lateness_ns, Some(0));
        assert_eq!(snapshot.late_fills, Some(0));
    }

    #[test]
    fn no_callback_or_timeout_leaves_callback_measurements_unknown() {
        let snapshot = RenderStats::default().snapshot();

        assert_eq!(snapshot.late_fills, None);
        assert_eq!(snapshot.max_lateness_ns, None);
        assert_eq!(snapshot.frames_available_total, None);
        assert_eq!(snapshot.frames_requested_total, None);
        assert_eq!(snapshot.min_padding_frames, None);
    }

    #[test]
    fn first_callback_keeps_maximum_lateness_unmeasured() {
        let mut stats = RenderStats::default();
        stats.record_callback(RenderObservation {
            lateness_ns: None,
            period_ns: Some(10_000_000),
            available_frames: 480,
            requested_frames: 480,
            padding_frames: 20,
        });

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.max_lateness_ns, None);
        assert_eq!(snapshot.late_fills, Some(0));
    }

    #[test]
    fn render_duration_is_unknown_until_the_wasapi_write_completes() {
        let diagnostics = SharedDiagnostics::new();
        diagnostics.begin_playback(7, 1_000);
        diagnostics.mark_render_write_pending(7);

        let pending = diagnostics.snapshot(7).unwrap();
        assert!(!pending.rendered_duration_known);
        assert!(pending.snapshot_pending);
        assert_eq!(pending.pcm_frames, 1_000);
        assert_eq!(pending.frames_rendered, None);
        assert_eq!(pending.write_error_count, None);

        diagnostics.record_rendered_frames(7, 480);
        let completed = diagnostics.snapshot(7).unwrap();
        assert!(completed.rendered_duration_known);
        assert!(!completed.snapshot_pending);
        assert_eq!(completed.frames_rendered, Some(480));
        assert_eq!(completed.write_error_count, Some(0));

        diagnostics.mark_render_write_pending(7);
        diagnostics.record_write_error(7);
        let failed = diagnostics.snapshot(7).unwrap();
        assert!(failed.rendered_duration_known);
        assert!(!failed.snapshot_pending);
        assert_eq!(failed.frames_rendered, Some(480));
        assert_eq!(failed.write_error_count, Some(1));
    }

    #[test]
    fn finish_accounting_keeps_submitted_cursor_and_successful_write_counts_distinct() {
        let diagnostics = SharedDiagnostics::new();
        diagnostics.begin_playback(11, 960);
        diagnostics.mark_render_write_pending(11);
        diagnostics.record_cursor_frames(11, 960);

        let pending = diagnostics.snapshot(11).unwrap();
        assert_eq!(pending.pcm_frames, 960);
        assert_eq!(pending.cursor_frames, 960);
        assert_eq!(pending.frames_rendered, None);
        assert!(pending.snapshot_pending);

        diagnostics.record_rendered_frames(11, 960);
        let written = diagnostics.snapshot(11).unwrap();
        assert_eq!(written.frames_rendered, Some(960));
        assert_eq!(written.pcm_frames, written.cursor_frames);
        assert!(!written.snapshot_pending);
    }

    #[test]
    fn finish_event_is_visible_before_the_final_successful_write_is_recorded() {
        let (mut render, mut coordinator) = pair();
        let diagnostics = coordinator.diagnostics();
        submit(&mut coordinator, 14, &[1, 2, 3, 4]);

        let mut output = [0; 4];
        let rendered = render
            .fill_observed(&mut output, Some(observation(0, 10, 4, 4, 4)))
            .expect("clip frames render");

        assert_eq!(rendered.frames, 4);
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(14) })
        );
        let pending = diagnostics.snapshot(14).unwrap();
        assert_eq!(pending.pcm_frames, 4);
        assert_eq!(pending.cursor_frames, 4);
        assert_eq!(pending.frames_rendered, None);
        assert!(pending.snapshot_pending);

        render.record_rendered_frames(rendered.playback_id, rendered.frames);
        let written = diagnostics.snapshot(14).unwrap();
        assert_eq!(written.frames_rendered, Some(4));
        assert!(!written.snapshot_pending);
    }

    #[test]
    fn a_failed_device_write_is_counted_without_inflating_successful_frames() {
        let diagnostics = SharedDiagnostics::new();
        diagnostics.begin_playback(12, 1_440);
        diagnostics.record_rendered_frames(12, 480);
        diagnostics.mark_render_write_pending(12);
        diagnostics.record_cursor_frames(12, 960);
        diagnostics.record_write_error(12);

        let snapshot = diagnostics.snapshot(12).unwrap();
        assert_eq!(snapshot.pcm_frames, 1_440);
        assert_eq!(snapshot.cursor_frames, 960);
        assert_eq!(snapshot.frames_rendered, Some(480));
        assert_eq!(snapshot.write_error_count, Some(1));
        assert!(!snapshot.snapshot_pending);
    }

    #[test]
    fn event_timeout_count_is_exact_per_playback() {
        let diagnostics = SharedDiagnostics::new();
        diagnostics.begin_playback(13, 480);

        diagnostics.record_event_timeout(13);
        diagnostics.record_event_timeout(13);

        assert_eq!(diagnostics.snapshot(13).unwrap().event_timeout_count, 2);
        assert_eq!(diagnostics.snapshot(13).unwrap().write_error_count, Some(0));
    }

    fn pair() -> (Engine, super::EngineHandle) {
        Engine::new()
    }

    fn samples(values: &[i16]) -> Arc<[i16]> {
        Arc::from(values.to_vec())
    }

    fn submit(engine: &mut super::EngineHandle, id: u64, values: &[i16]) {
        assert_eq!(engine.submit(PlaybackId(id), samples(values)), Ok(()));
    }

    #[test]
    fn a_new_submit_replaces_the_current_buffer_without_mixing() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[1, 2, 3, 4]);
        let mut first = [0; 2];
        render.fill(&mut first);
        assert_eq!(first, [1, 2]);

        submit(&mut coordinator, 2, &[7, 8]);
        let mut replacement = [0; 2];
        render.fill(&mut replacement);

        assert_eq!(replacement, [7, 8]);
        assert_eq!(
            coordinator.poll_retired().as_deref(),
            Some(&[1, 2, 3, 4][..])
        );
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(2) })
        );
    }

    #[test]
    fn cancel_makes_the_next_fill_silent_and_retires_without_finished() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[9, 8, 7]);
        coordinator.cancel(PlaybackId(1));

        let mut output = [4; 5];
        render.fill(&mut output);

        assert_eq!(output, [0; 5]);
        assert_eq!(coordinator.poll_retired().as_deref(), Some(&[9, 8, 7][..]));
        assert_eq!(coordinator.poll_event(), None);
    }

    #[test]
    fn fade_cancel_overlaps_the_new_clip_with_a_sample_exact_tail() {
        crate::set_master_volume_percent(100);
        let (mut render, mut coordinator) = pair();
        let old = vec![16_000; 6_000];
        submit(&mut coordinator, 1, &old);

        let prefix_len = 17;
        let mut prefix = vec![0; prefix_len];
        render.fill(&mut prefix);
        assert_eq!(prefix, old[..prefix_len]);

        coordinator.cancel_with_fade(PlaybackId(1), kohbor_audio::PREEMPT_FADE_SAMPLES);
        let replacement = vec![1_234; kohbor_audio::PREEMPT_FADE_SAMPLES + 2];
        submit(&mut coordinator, 2, &replacement);

        let fade_samples = kohbor_audio::PREEMPT_FADE_SAMPLES;
        let mut mixed = vec![0; fade_samples + 1];
        render.fill(&mut mixed);
        let mut tail_samples = Vec::with_capacity(fade_samples);
        let mut prior_gain = 1.0_f32;
        for (index, actual) in mixed[..fade_samples].iter().copied().enumerate() {
            let gain = 1.0 - index as f32 / (fade_samples - 1) as f32;
            assert!(gain <= prior_gain, "fade gain is monotonic non-increasing");
            prior_gain = gain;
            let faded_tail = scale_sample(old[prefix_len + index], gain);
            tail_samples.push(faded_tail);
            assert_eq!(
                actual,
                replacement[index].saturating_add(faded_tail),
                "sample {index} is the saturating sum of the immediate new clip and linear tail"
            );
        }
        assert_eq!(mixed[0], replacement[0].saturating_add(old[prefix_len]));
        assert_eq!(
            tail_samples[fade_samples - 1],
            0,
            "last tail sample is silent"
        );
        assert_eq!(mixed[fade_samples], replacement[fade_samples]);
        assert_eq!(coordinator.poll_retired().as_deref(), Some(&old[..]));
        assert_eq!(
            coordinator.poll_event(),
            None,
            "faded playback is not Finished"
        );

        let max_adjacent_tail_step = tail_samples
            .windows(2)
            .map(|pair| (pair[0] - pair[1]).abs())
            .max()
            .unwrap();
        assert_eq!(max_adjacent_tail_step, 4);
        assert!(tail_samples.windows(2).all(|pair| pair[1] <= pair[0]));

        crate::set_master_volume_percent(100);
    }

    #[test]
    fn fade_shortens_to_the_old_clip_samples_that_remain_and_overlaps() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[10, 20, 30, 40, 50]);
        let mut prefix = [0; 3];
        render.fill(&mut prefix);
        assert_eq!(prefix, [10, 20, 30]);

        coordinator.cancel_with_fade(PlaybackId(1), kohbor_audio::PREEMPT_FADE_SAMPLES);
        submit(&mut coordinator, 2, &[99, 100, 101]);
        let mut mixed = [0; 3];
        render.fill(&mut mixed);

        assert_eq!(
            mixed,
            [139, 100, 101],
            "remaining source fades over two samples"
        );
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(2) }),
            "only the replacement emits Finished"
        );
    }

    #[test]
    fn a_second_fade_drops_the_older_tail_before_fading_the_active_clip() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[12_000; 8]);
        render.fill(&mut [0; 1]);
        coordinator.cancel_with_fade(PlaybackId(1), 4);
        submit(&mut coordinator, 2, &[2_000; 8]);

        let mut first_mix = [0; 1];
        render.fill(&mut first_mix);
        assert_eq!(first_mix, [14_000]);
        assert_eq!(coordinator.poll_event(), None);

        coordinator.cancel_with_fade(PlaybackId(2), 4);
        submit(&mut coordinator, 3, &[500; 8]);
        let mut second_mix = [0; 1];
        render.fill(&mut second_mix);

        assert_eq!(second_mix, [2_500], "the first tail was hard-dropped");
        assert_eq!(
            coordinator.poll_retired().as_deref(),
            Some(&[12_000; 8][..])
        );
        assert_eq!(coordinator.poll_event(), None);
    }

    #[test]
    fn plain_cancel_silences_the_active_clip_and_its_fading_tail() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[12_000; 8]);
        render.fill(&mut [0; 1]);
        coordinator.cancel_with_fade(PlaybackId(1), 4);
        submit(&mut coordinator, 2, &[2_000; 8]);
        render.fill(&mut [0; 1]);

        coordinator.cancel(PlaybackId(2));
        let mut output = [7; 3];
        render.fill(&mut output);

        assert_eq!(output, [0; 3]);
        assert_eq!(coordinator.poll_event(), None);
    }

    #[test]
    fn overlapping_sources_saturate_instead_of_wrapping() {
        for sample in [i16::MAX, i16::MIN] {
            let (mut render, mut coordinator) = pair();
            submit(&mut coordinator, 1, &[sample; 3]);
            render.fill(&mut [0; 1]);
            coordinator.cancel_with_fade(PlaybackId(1), 4_800);
            submit(&mut coordinator, 2, &[sample; 2]);

            let mut output = [0; 2];
            render.fill(&mut output);

            assert_eq!(output, [sample, sample], "saturation holds for {sample}");
            assert_eq!(
                coordinator.poll_event(),
                Some(SinkEvent::Finished { id: PlaybackId(2) })
            );
        }
    }

    #[test]
    fn cancelling_a_stale_id_does_not_cancel_the_replacement() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[1, 2, 3]);
        let mut prefix = [0; 1];
        render.fill(&mut prefix);
        submit(&mut coordinator, 2, &[6, 5, 4]);
        coordinator.cancel(PlaybackId(1));

        let mut output = [0; 3];
        render.fill(&mut output);

        assert_eq!(output, [6, 5, 4]);
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(2) })
        );
    }

    #[test]
    fn completion_is_emitted_exactly_once_after_the_last_sample() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 17, &[3, 2, 1]);
        let mut first = [0; 2];
        render.fill(&mut first);
        assert_eq!(first, [3, 2]);
        assert_eq!(coordinator.poll_event(), None);

        let mut last = [9; 2];
        render.fill(&mut last);
        assert_eq!(last, [1, 0]);
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(17) })
        );
        render.fill(&mut last);
        assert_eq!(coordinator.poll_event(), None);
    }

    #[test]
    fn zero_length_fill_is_safe_and_does_not_consume_samples() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[5, 4]);
        render.fill(&mut []);

        let mut output = [0; 2];
        render.fill(&mut output);

        assert_eq!(output, [5, 4]);
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(1) })
        );
    }

    #[test]
    fn a_short_final_buffer_is_padded_with_silence_and_retired() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[11, 12, 13]);
        let mut output = [0; 5];

        render.fill(&mut output);

        assert_eq!(output, [11, 12, 13, 0, 0]);
        assert_eq!(
            coordinator.poll_retired().as_deref(),
            Some(&[11, 12, 13][..])
        );
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(1) })
        );
    }

    #[test]
    fn finished_buffers_are_returned_to_the_coordinator() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[21, 22]);
        render.fill(&mut [0; 2]);

        let retired = coordinator.poll_retired();
        assert_eq!(retired.as_deref(), Some(&[21, 22][..]));
    }

    #[test]
    fn a_full_command_queue_returns_sink_error_and_leaves_silence() {
        let (mut render, mut coordinator) = pair();
        for id in 1..=COMMAND_QUEUE_CAPACITY as u64 {
            submit(&mut coordinator, id, &[id as i16]);
        }

        assert_eq!(
            coordinator.submit(PlaybackId(99), samples(&[99])),
            Err(SinkError)
        );
        let mut output = [7; 1];
        render.fill(&mut output);
        assert_eq!(output, [0]);
        let mut retired = 0;
        while coordinator.poll_retired().is_some() {
            retired += 1;
        }
        assert_eq!(retired, COMMAND_QUEUE_CAPACITY);
    }

    #[test]
    fn a_full_event_queue_drops_newest_but_preserves_device_lost() {
        let (mut render, mut coordinator) = pair();
        for id in 1..=EVENT_QUEUE_CAPACITY as u64 + 1 {
            submit(&mut coordinator, id, &[id as i16]);
            render.fill(&mut [0; 1]);
        }
        coordinator.device_event_sender().device_changed();
        coordinator.device_event_sender().device_lost();

        assert_eq!(coordinator.poll_event(), Some(SinkEvent::DeviceLost));
        for id in 1..=EVENT_QUEUE_CAPACITY as u64 {
            assert_eq!(
                coordinator.poll_event(),
                Some(SinkEvent::Finished { id: PlaybackId(id) })
            );
        }
        assert_eq!(coordinator.poll_event(), None);
    }

    #[test]
    fn device_changed_is_queued_as_a_bounded_event() {
        let (render, mut coordinator) = pair();
        coordinator.device_event_sender().device_changed();
        drop(render);

        assert_eq!(coordinator.poll_event(), Some(SinkEvent::DeviceChanged));
    }

    #[test]
    fn deterministic_randomish_sequences_remain_consistent() {
        let (mut render, mut coordinator) = pair();
        let mut seed = 0x6d2b_79f5_u32;
        let mut next_id = 1_u64;
        let mut current: Option<(PlaybackId, Vec<i16>, usize)> = None;
        let mut generation_active = true;

        for _ in 0..10_000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            match seed % 4 {
                0 => {
                    let count = ((seed >> 8) % 5 + 1) as usize;
                    let values: Vec<i16> = (0..count)
                        .map(|index| (next_id as i16).wrapping_add(index as i16))
                        .collect();
                    let id = PlaybackId(next_id);
                    next_id += 1;
                    assert_eq!(coordinator.submit(id, samples(&values)), Ok(()));
                    current = Some((id, values, 0));
                    generation_active = true;
                }
                1 => {
                    if let Some((id, _, _)) = current.as_ref() {
                        let cancel_id = if seed & 0x1000 == 0 {
                            *id
                        } else {
                            PlaybackId(id.0.saturating_add(1_000_000))
                        };
                        coordinator.cancel(cancel_id);
                        if cancel_id == *id {
                            generation_active = false;
                        }
                    } else {
                        coordinator.cancel(PlaybackId(seed as u64));
                    }
                }
                _ => {}
            }

            let output_len = ((seed >> 16) % 7) as usize;
            let mut actual = vec![99_i16; output_len];
            let mut expected = vec![0_i16; output_len];
            let mut clear_current = false;
            let mut expected_finished = None;
            if let Some((id, values, cursor)) = current.as_mut() {
                if !generation_active {
                    clear_current = true;
                } else {
                    let count = output_len.min(values.len().saturating_sub(*cursor));
                    expected[..count].copy_from_slice(&values[*cursor..*cursor + count]);
                    *cursor += count;
                    if *cursor == values.len() {
                        clear_current = true;
                        expected_finished = Some(*id);
                    }
                }
            }
            if clear_current {
                current = None;
            }
            render.fill(&mut actual);
            assert_eq!(actual, expected);
            if let Some(id) = expected_finished {
                assert_eq!(coordinator.poll_event(), Some(SinkEvent::Finished { id }));
            }
            while coordinator.poll_retired().is_some() {}
            while let Some(event) = coordinator.poll_event() {
                assert!(!matches!(event, SinkEvent::Finished { .. }));
            }
        }
    }

    #[test]
    fn render_source_guard_rejects_blocking_allocating_or_io_primitives() {
        let source = include_str!("engine.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        let code_without_comments = strip_comments(production);

        for forbidden in [
            "Mutex",
            "RwLock",
            "Vec::new",
            "Box::new",
            "String",
            "format!",
            ".clone()",
            "println!",
            "std::fs",
            "std::net",
            "TcpStream",
            "UdpSocket",
        ] {
            assert!(
                !code_without_comments.contains(forbidden),
                "engine render source contains forbidden token {forbidden}"
            );
        }
    }

    #[cfg(feature = "latency-probe")]
    #[test]
    fn first_fill_probe_ignores_idle_and_cancelled_fills() {
        let (mut render, mut coordinator) = pair();
        let probe = FirstFillProbe::new();
        render.set_first_fill_probe(Arc::clone(&probe.inner));

        render.fill(&mut [0; 2]);
        assert_eq!(probe.latest(), None);

        submit(&mut coordinator, 1, &[4, 5]);
        coordinator.cancel(PlaybackId(1));
        render.fill(&mut [0; 2]);

        assert_eq!(probe.latest(), None);
    }

    #[cfg(feature = "latency-probe")]
    #[test]
    fn first_fill_probe_keeps_the_first_service_for_a_playback() {
        let (mut render, mut coordinator) = pair();
        let probe = FirstFillProbe::new();
        render.set_first_fill_probe(Arc::clone(&probe.inner));
        submit(&mut coordinator, 17, &[3, 2]);

        let before = Instant::now();
        render.fill(&mut [0; 1]);
        let after = Instant::now();
        let first = probe.latest();
        let (id, timestamp) = first.expect("the first nonempty fill is recorded");
        assert_eq!(id, PlaybackId(17));
        assert!(before <= timestamp && timestamp <= after);

        render.fill(&mut [0; 1]);

        assert_eq!(probe.latest(), first);
        probe.clear();
        assert_eq!(probe.latest(), None);

        submit(&mut coordinator, 18, &[6, 7]);
        render.fill(&mut [0; 1]);
        assert_eq!(probe.latest().map(|(id, _)| id), Some(PlaybackId(18)));
    }

    #[cfg(feature = "latency-probe")]
    #[test]
    fn first_fill_probe_seqlock_keeps_id_and_timestamp_paired_under_stress() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Barrier;
        use std::thread;

        const RECORDS: u64 = 50_000;

        let probe = FirstFillProbe::new();
        let finished = Arc::new(AtomicBool::new(false));
        let start = Arc::new(Barrier::new(3));

        let writer_probe = probe.clone();
        let writer_finished = Arc::clone(&finished);
        let writer_start = Arc::clone(&start);
        let writer = thread::spawn(move || {
            writer_start.wait();
            for id in 1..=RECORDS {
                writer_probe.inner.record_nanos(PlaybackId(id), id);
                if id % 256 == 0 {
                    thread::yield_now();
                }
            }
            writer_finished.store(true, Ordering::Release);
        });

        let reader_probe = probe.clone();
        let reader_finished = Arc::clone(&finished);
        let reader_start = Arc::clone(&start);
        let reader = thread::spawn(move || {
            reader_start.wait();
            let mut observations = 0;
            loop {
                if let Some((id, instant)) = reader_probe.latest() {
                    assert_eq!(
                        instant
                            .saturating_duration_since(reader_probe.inner.base)
                            .as_nanos(),
                        u128::from(id.0)
                    );
                    observations += 1;
                }
                if reader_finished.load(Ordering::Acquire) {
                    break;
                }
                thread::yield_now();
            }
            observations
        });

        start.wait();
        writer.join().expect("probe writer thread");
        assert!(reader.join().expect("probe reader thread") > 0);
    }

    fn strip_comments(source: &str) -> String {
        let mut output = String::with_capacity(source.len());
        let mut characters = source.chars().peekable();
        let mut block_depth = 0_usize;
        while let Some(character) = characters.next() {
            if block_depth > 0 {
                if character == '/' && characters.peek() == Some(&'*') {
                    characters.next();
                    block_depth += 1;
                } else if character == '*' && characters.peek() == Some(&'/') {
                    characters.next();
                    block_depth -= 1;
                } else if character == '\n' {
                    output.push('\n');
                }
            } else if character == '/' && characters.peek() == Some(&'/') {
                characters.next();
                for comment_character in characters.by_ref() {
                    if comment_character == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            } else if character == '/' && characters.peek() == Some(&'*') {
                characters.next();
                block_depth = 1;
            } else {
                output.push(character);
            }
        }
        output
    }
}
