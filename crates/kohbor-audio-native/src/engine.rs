use kohbor_audio::{PlaybackId, SinkError, SinkEvent};
#[cfg(feature = "latency-probe")]
use std::sync::atomic::fence;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
#[cfg(feature = "latency-probe")]
use std::time::Duration;
use std::time::Instant;

pub(super) const COMMAND_QUEUE_CAPACITY: usize = 8;
pub(super) const EVENT_QUEUE_CAPACITY: usize = 16;
const RETIREMENT_QUEUE_CAPACITY: usize = COMMAND_QUEUE_CAPACITY + 1;
const INACTIVE_GENERATION: u64 = 0;

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
    pub(crate) lateness_ns: u64,
    pub(crate) period_ns: u64,
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
    max_lateness_ns: Option<u64>,
    min_padding_frames: Option<u32>,
}

#[derive(Default)]
struct RenderStats {
    first_fill_latency_ns: Option<u64>,
    frames_available_total: u64,
    frames_requested_total: u64,
    late_fills: u64,
    max_lateness_ns: Option<u64>,
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
        self.max_lateness_ns = Some(
            self.max_lateness_ns
                .map_or(observation.lateness_ns, |current| {
                    current.max(observation.lateness_ns)
                }),
        );
        self.min_padding_frames = Some(
            self.min_padding_frames
                .map_or(observation.padding_frames, |current| {
                    current.min(observation.padding_frames)
                }),
        );
        if observation.padding_frames == 0 || observation.lateness_ns > observation.period_ns {
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
    first_fill_latency_ns: AtomicU64,
    first_fill_measured: AtomicBool,
    frames_rendered: AtomicU64,
    rendered_duration_known: AtomicBool,
    frames_available_total: AtomicU64,
    frames_requested_total: AtomicU64,
    callback_frames_measured: AtomicBool,
    late_fills: AtomicU64,
    late_fills_measured: AtomicBool,
    max_lateness_ns: AtomicU64,
    lateness_measured: AtomicBool,
    min_padding_frames: AtomicU64,
    padding_measured: AtomicBool,
}

impl PlaybackMetrics {
    fn new() -> Self {
        Self {
            playback_id: AtomicU64::new(INACTIVE_GENERATION),
            first_fill_latency_ns: AtomicU64::new(0),
            first_fill_measured: AtomicBool::new(false),
            frames_rendered: AtomicU64::new(0),
            rendered_duration_known: AtomicBool::new(true),
            frames_available_total: AtomicU64::new(0),
            frames_requested_total: AtomicU64::new(0),
            callback_frames_measured: AtomicBool::new(false),
            late_fills: AtomicU64::new(0),
            late_fills_measured: AtomicBool::new(false),
            max_lateness_ns: AtomicU64::new(0),
            lateness_measured: AtomicBool::new(false),
            min_padding_frames: AtomicU64::new(0),
            padding_measured: AtomicBool::new(false),
        }
    }

    fn reset(&self, playback_id: u64) {
        self.playback_id
            .store(INACTIVE_GENERATION, Ordering::Release);
        self.first_fill_latency_ns.store(0, Ordering::Relaxed);
        self.first_fill_measured.store(false, Ordering::Relaxed);
        self.frames_rendered.store(0, Ordering::Relaxed);
        self.rendered_duration_known.store(true, Ordering::Relaxed);
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
    pub(crate) first_fill_latency_ns: Option<u64>,
    pub(crate) frames_rendered: Option<u64>,
    pub(crate) rendered_duration_known: bool,
    pub(crate) frames_available_total: Option<u64>,
    pub(crate) frames_requested_total: Option<u64>,
    pub(crate) late_fills: Option<u64>,
    pub(crate) max_lateness_ns: Option<u64>,
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

    fn begin_playback(&self, playback_id: u64) {
        self.slot(playback_id).reset(playback_id);
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
        }
    }

    fn mark_render_write_pending(&self, playback_id: u64) {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) == playback_id {
            slot.rendered_duration_known.store(false, Ordering::Release);
        }
    }

    fn complete_render_write_without_samples(&self, playback_id: u64) {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) == playback_id {
            slot.rendered_duration_known.store(true, Ordering::Release);
        }
    }

    fn mark_rendered_duration_unknown(&self, playback_id: u64) {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) == playback_id {
            slot.rendered_duration_known.store(false, Ordering::Release);
        }
    }

    pub(crate) fn snapshot(&self, playback_id: u64) -> Option<PlaybackMetricsSnapshot> {
        let slot = self.slot(playback_id);
        if slot.playback_id.load(Ordering::Acquire) != playback_id {
            return None;
        }
        Some(PlaybackMetricsSnapshot {
            first_fill_latency_ns: slot
                .first_fill_measured
                .load(Ordering::Acquire)
                .then(|| slot.first_fill_latency_ns.load(Ordering::Relaxed)),
            frames_rendered: Some(slot.frames_rendered.load(Ordering::Relaxed)),
            rendered_duration_known: slot.rendered_duration_known.load(Ordering::Acquire),
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
    submitted_at: Instant,
    stats: RenderStats,
}

/// Device-independent render state. `fill` is called only on the audio-owner
/// thread and uses fixed-capacity channels plus the current buffer; it does not
/// wait for the coordinator.
pub(crate) struct Engine {
    commands: Receiver<Command>,
    retired: SyncSender<Arc<[i16]>>,
    events: SyncSender<SinkEvent>,
    current_generation: Arc<AtomicU64>,
    active: Option<Playback>,
    pending_command: Option<Command>,
    diagnostics: Arc<SharedDiagnostics>,
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
        let device_lost_pending = Arc::new(AtomicBool::new(false));
        let device_stop_requested = Arc::new(AtomicBool::new(false));
        let diagnostics = Arc::new(SharedDiagnostics::new());
        let handle = EngineHandle {
            commands: command_sender,
            retired,
            event_sender: Clone::clone(&event_sender),
            events,
            current_generation: Arc::clone(&current_generation),
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
                active: None,
                pending_command: None,
                diagnostics,
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
        let rendered = self.render_active(out, observation);
        self.retire_terminal_active();
        rendered
    }

    pub(crate) fn record_no_space_callback(&mut self, observation: RenderObservation) {
        self.record_active_callback(observation);
    }

    pub(crate) fn record_event_timeout(&mut self) {
        let diagnostics = &self.diagnostics;
        if let Some(active) = self.active.as_mut() {
            if active.generation == self.current_generation.load(Ordering::Acquire) {
                active.stats.record_timeout();
                diagnostics.publish_stats(active.id.0, active.stats.snapshot());
            }
        }
    }

    pub(crate) fn record_rendered_frames(&self, playback_id: PlaybackId, frames: usize) {
        self.diagnostics
            .record_rendered_frames(playback_id.0, frames);
    }

    pub(crate) fn mark_rendered_duration_unknown(&self, playback_id: PlaybackId) {
        self.diagnostics
            .mark_rendered_duration_unknown(playback_id.0);
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
        if !self.retire_terminal_active() {
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

        if self.active.is_some() && !self.retire_active() {
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
        let diagnostics = &self.diagnostics;
        let active = self.active.as_mut()?;
        if generation != active.generation {
            return None;
        }

        let remaining = active.samples.len().saturating_sub(active.cursor);
        let count = remaining.min(out.len());
        let end = active.cursor.checked_add(count)?;
        if count > 0 {
            diagnostics.mark_render_write_pending(active.id.0);
        }
        out[..count].copy_from_slice(&active.samples[active.cursor..end]);
        if let Some(observation) = observation {
            active.stats.record_callback(observation);
        }
        if self.current_generation.load(Ordering::Acquire) != generation {
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
        if active.cursor == active.samples.len() && !active.finished_notified {
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

    fn retire_terminal_active(&mut self) -> bool {
        let should_retire = self.active.as_ref().is_some_and(|active| {
            active.finished_notified
                || active.generation != self.current_generation.load(Ordering::Acquire)
        });
        !should_retire || self.retire_active()
    }

    fn retire_active(&mut self) -> bool {
        let Some(active) = self.active.take() else {
            return true;
        };
        let Playback {
            id,
            generation,
            samples,
            cursor,
            finished_notified,
            submitted_at,
            stats,
        } = active;
        match self.retired.try_send(samples) {
            Ok(()) => true,
            Err(TrySendError::Full(samples) | TrySendError::Disconnected(samples)) => {
                self.active = Some(Playback {
                    id,
                    generation,
                    samples,
                    cursor,
                    finished_notified,
                    submitted_at,
                    stats,
                });
                false
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
        self.diagnostics.begin_playback(id.0);
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
        }
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
        Engine, RenderObservation, RenderStats, SharedDiagnostics, COMMAND_QUEUE_CAPACITY,
        EVENT_QUEUE_CAPACITY,
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
            lateness_ns,
            period_ns,
            available_frames,
            requested_frames,
            padding_frames,
        }
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
    fn render_duration_is_unknown_until_the_wasapi_write_completes() {
        let diagnostics = SharedDiagnostics::new();
        diagnostics.begin_playback(7);
        diagnostics.mark_render_write_pending(7);

        let pending = diagnostics.snapshot(7).unwrap();
        assert!(!pending.rendered_duration_known);
        assert_eq!(pending.frames_rendered, Some(0));

        diagnostics.record_rendered_frames(7, 480);
        let completed = diagnostics.snapshot(7).unwrap();
        assert!(completed.rendered_duration_known);
        assert_eq!(completed.frames_rendered, Some(480));

        diagnostics.mark_render_write_pending(7);
        diagnostics.mark_rendered_duration_unknown(7);
        assert!(!diagnostics.snapshot(7).unwrap().rendered_duration_known);
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
