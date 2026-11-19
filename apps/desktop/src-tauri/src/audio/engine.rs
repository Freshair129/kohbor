//! The audio engine: one thread that owns the audio owner and the prewarm table
//! (design docs/design/S3-5-HOST-CRITICAL-PATH.md sections 1, 2, 5, 7 and 8).
//! Admission-path code: no file, network, lock, blocking call or panic source.

#![allow(dead_code, unused_variables)]

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use kohbor_audio::{
    AudioClass, AudioOwner, CancelReason, Config, Decision, Notice, Outcome, PlaybackId, Request,
    Sink,
};
use kohbor_events::{arbitrate, Event, EventId};

use super::lanes::{
    AuditionRefusal, AuditionRequest, Control, EngineOut, EngineRx, EngineShared, EngineTx, TickMsg,
};
use super::outcome::{
    playback_ended, playback_started, pre_owner_record, preempted_record, record, EndReason,
};
use super::resolve::{class_of, instance_id, Clip, Lookup, PrewarmTable};
use super::slot_sink::SlotSink;
use crate::clock::MonoClock;

const CONTROL_BUDGET: usize = 32;
const ORDINARY_BUDGET: usize = 16;
const REBUILD_WINDOW_MS: u64 = 60_000;
const MAX_REBUILDS_IN_WINDOW: usize = 3;
const AUDITION_GENERATION: u64 = u64::MAX - 1;
// Outside the contract's match epoch range. The owner can cancel the audition
// and its visual tail through the normal session fence.
const AUDITION_EPOCH: u64 = u64::MAX;

struct LivePlayback {
    id: PlaybackId,
    decision: Decision,
    audio_ends_at_ms: Option<u64>,
    expected_frames: Option<u64>,
}

pub struct Engine {
    clock: MonoClock,
    shared: Arc<EngineShared>,
    critical: mpsc::Receiver<TickMsg>,
    ordinary: mpsc::Receiver<TickMsg>,
    audition: mpsc::Receiver<AuditionRequest>,
    control: mpsc::Receiver<Control>,
    out: SyncSender<EngineOut>,
    owner: AudioOwner<SlotSink>,
    config: Config,
    table: Arc<PrewarmTable>,
    latency: bool,
    session_epoch: u64,
    audition_session: bool,
    audition_match_reported: bool,
    muted: bool,
    last_issued_id: u64,
    live: Vec<LivePlayback>,
    rebuild_instants: VecDeque<u64>,
    need_sink_requested: bool,
    replacement_pending: bool,
    pending_sink: Option<Box<dyn Sink + Send>>,
    shutdown: bool,
}

impl Engine {
    /// Build an engine that drains `rx` and writes to `out`. `sink` is the first sink
    /// (`None`: the owner starts with the device marked unavailable and the engine emits
    /// `EngineOut::NeedSink` on the first step). `latency` turns on `EngineOut::Latency`.
    /// The initial table is `PrewarmTable::empty()`, the initial mute state false, the
    /// initial session 0, the first playback id 1.
    pub fn new(
        clock: MonoClock,
        shared: Arc<EngineShared>,
        rx: EngineRx,
        sink: Option<Box<dyn Sink + Send>>,
        out: SyncSender<EngineOut>,
        latency: bool,
    ) -> Engine {
        let device_available = sink.is_some();
        let mut slot = SlotSink::empty();
        if let Some(sink) = sink {
            let _ = slot.install(sink);
        }

        let config = Config::default();
        let mut owner = AudioOwner::with_first_playback_id(slot, config, 1);
        owner.set_device_available(clock.now_ms(), device_available);

        Engine {
            clock,
            shared,
            critical: rx.critical,
            ordinary: rx.ordinary,
            audition: rx.audition,
            control: rx.control,
            out,
            owner,
            config,
            table: PrewarmTable::empty(),
            latency,
            session_epoch: 0,
            audition_session: false,
            audition_match_reported: false,
            muted: false,
            last_issued_id: 0,
            live: Vec::with_capacity(2),
            rebuild_instants: VecDeque::new(),
            need_sink_requested: false,
            replacement_pending: false,
            pending_sink: None,
            shutdown: false,
        }
    }

    /// One iteration at `now_ms` (the caller supplies the time; tests never sleep).
    /// Order: (1) drain the whole critical lane; (2) drain up to 32 control messages;
    /// (3) drain up to 16 ordinary ticks; (4) `owner.pump_sink_events(now)` and
    /// `owner.tick(now)`, convert `drain_notices()` into outputs; (5) consider one
    /// audition only after game audio; (6) store the atomics of
    /// `EngineShared` (heartbeat_ms = now_ms, muted, device_available, lease_active, fence).
    /// Returns how long the caller may park: 10 ms while the owner has an active playback or
    /// a visual, otherwise 250 ms.
    ///
    /// `step` never panics: the body runs under `catch_unwind`; on a caught panic the engine
    /// increments `shared.rebuilds`, hands the old sink to `EngineOut::ReleaseSink`, rebuilds
    /// the owner with `AudioOwner::with_first_playback_id(SlotSink::empty(), config,
    /// last_issued_id + 1)`, re-applies mute, session and table, emits `EngineOut::NeedSink`,
    /// emits `ended cancelled_device_lost` for the playback that was live, and returns.
    /// More than 3 rebuilds within 60,000 ms sets `shared.degraded`: from then on every request
    /// is recorded `channel_busy` without touching a sink, until the process restarts.
    pub fn step(&mut self, now_ms: u64) -> Duration {
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.step_body(now_ms)));
        if result.is_err() {
            self.rebuild_after_panic(now_ms);
        }

        self.publish_shared(now_ms);
        self.wait_duration(now_ms)
    }

    fn step_body(&mut self, now_ms: u64) {
        if !self.shutdown {
            let match_active = self.shared.audition_match_active();
            if match_active {
                self.stop_audition(now_ms, false);
            }
            if match_active != self.audition_match_reported
                && self.try_output(EngineOut::AuditionMatchChanged(match_active))
            {
                self.audition_match_reported = match_active;
            }
            let mut real_event_seen = false;
            if !self.is_degraded()
                && (self.owner.sink().is_empty()
                    || self.replacement_pending
                    || !self.owner.device_available())
            {
                self.request_sink_if_needed();
            }

            while let Ok(message) = self.critical.try_recv() {
                real_event_seen |= !message.events.is_empty();
                self.handle_tick(message, now_ms);
            }

            for _ in 0..CONTROL_BUDGET {
                match self.control.try_recv() {
                    Ok(message) => {
                        if self.handle_control(message, now_ms) {
                            break;
                        }
                    }
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                }
            }

            if !self.shutdown {
                for _ in 0..ORDINARY_BUDGET {
                    match self.ordinary.try_recv() {
                        Ok(message) => {
                            real_event_seen |= !message.events.is_empty();
                            self.handle_tick(message, now_ms);
                        }
                        Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                    }
                }
            }

            if !self.shutdown && !self.is_degraded() {
                let was_available = self.owner.device_available();
                self.owner.pump_sink_events(now_ms);
                let device_changed = self.owner.sink_mut().take_changed();
                let is_available = self.owner.device_available();

                if was_available && !is_available {
                    let _ = self.try_output(EngineOut::DeviceLost);
                    self.drain_notices();
                    self.request_sink(true);
                } else {
                    self.drain_notices();
                }

                if device_changed {
                    self.replacement_pending = true;
                    self.request_sink_if_needed();
                }

                self.owner.tick(now_ms);
                self.drain_notices();
                self.install_pending_if_idle(now_ms);
            }

            if !self.shutdown {
                if let Ok(request) = self.audition.try_recv() {
                    let result = self.admit_audition(&request, now_ms, real_event_seen);
                    let _ = request.reply.try_send(result);
                }
            }
        }

        if self.shutdown {
            self.finish_shutdown(now_ms);
        }
    }

    fn handle_control(&mut self, message: Control, now_ms: u64) -> bool {
        match message {
            Control::Session { epoch } => {
                if epoch > self.session_epoch {
                    self.session_epoch = epoch;
                    self.owner.set_session(now_ms, epoch);
                    self.audition_session = false;
                    self.drain_notices();
                }
            }
            Control::SetMuted { muted, ack } => {
                self.muted = muted;
                self.owner.set_muted(now_ms, muted);
                self.drain_notices();
                let _ = ack.try_send(muted);
            }
            Control::SwapTable { table } => {
                self.owner.set_revision(now_ms, table.fence);
                self.drain_notices();
                let old = std::mem::replace(&mut self.table, table);
                self.shared.fence.store(self.table.fence, Ordering::Relaxed);
                let _ = self.try_output(EngineOut::ReleaseTable(old));
            }
            Control::SinkReady(sink) => {
                if self.is_degraded() {
                    self.need_sink_requested = true;
                    let _ = self.try_output(EngineOut::ReleaseSink(sink));
                } else {
                    self.need_sink_requested = true;
                    if self.owner.active(now_ms).is_some() {
                        if let Some(previous) = self.pending_sink.replace(sink) {
                            let _ = self.try_output(EngineOut::ReleaseSink(previous));
                        }
                    } else {
                        self.install_sink(sink, now_ms);
                    }
                }
            }
            Control::Shutdown => {
                self.shutdown = true;
                return true;
            }
        }
        false
    }

    fn handle_tick(&mut self, message: TickMsg, now_ms: u64) {
        if !message.events.is_empty() {
            self.stop_audition(now_ms, true);
        }
        if message.session_epoch > self.session_epoch {
            self.session_epoch = message.session_epoch;
            self.owner.set_session(now_ms, message.session_epoch);
            self.drain_notices();
        }

        let mut critical = Vec::new();
        let mut ordinary = Vec::new();

        for (position, event) in message.events.iter().copied().enumerate() {
            let key = instance_id(message.generation, message.sequence, position);
            if crate::intake::EVENT_UNAVAILABLE.contains(&event.id) {
                self.record_pre_owner(
                    &message,
                    event.id,
                    &key,
                    Outcome::DetectorUnavailable,
                    now_ms,
                );
            } else if !event.id.voiced() {
                self.record_pre_owner(&message, event.id, &key, Outcome::NotVoiced, now_ms);
            } else if event.id.is_critical() {
                critical.push((event, key));
            } else {
                ordinary.push((event, key));
            }
        }

        for (event, key) in critical {
            self.handle_voiced_event(&message, event, key, now_ms);
        }

        let arbitration_inputs: Vec<Event> = ordinary.iter().map(|(event, _)| *event).collect();
        let arbitration = arbitrate(&arbitration_inputs);
        let mut used = vec![false; ordinary.len()];
        let winner_position = arbitration
            .winner
            .and_then(|winner| Self::find_event_position(&ordinary, &used, winner));
        if let Some(position) = winner_position {
            if let Some(is_used) = used.get_mut(position) {
                *is_used = true;
            }
        }

        for dropped in arbitration.dropped {
            if let Some(position) = Self::find_event_position(&ordinary, &used, dropped.event) {
                if let Some(is_used) = used.get_mut(position) {
                    *is_used = true;
                }
                if let Some((event, key)) = ordinary.get(position) {
                    self.record_pre_owner(
                        &message,
                        event.id,
                        key,
                        Outcome::LowerRankSameTick,
                        now_ms,
                    );
                }
            }
        }

        if let Some(position) = winner_position {
            if let Some((event, key)) = ordinary.get(position) {
                self.handle_voiced_event(&message, *event, key.clone(), now_ms);
            }
        }
    }

    fn find_event_position(
        events: &[(Event, String)],
        used: &[bool],
        event: Event,
    ) -> Option<usize> {
        events
            .iter()
            .enumerate()
            .find_map(|(position, (candidate, _))| {
                let already_used = match used.get(position) {
                    Some(value) => *value,
                    None => true,
                };
                if !already_used && *candidate == event {
                    Some(position)
                } else {
                    None
                }
            })
    }

    fn handle_voiced_event(&mut self, message: &TickMsg, event: Event, key: String, now_ms: u64) {
        let event_type = event.id.as_str();
        let class = class_of(event.id);
        let clip = match self.resolve_clip(event) {
            Ok(clip) => clip,
            Err(outcome) => {
                self.record_pre_owner(message, event.id, &key, outcome, now_ms);
                return;
            }
        };

        if self.is_degraded() {
            self.record_pre_owner(message, event.id, &key, Outcome::ChannelBusy, now_ms);
            return;
        }

        let request = Request {
            instance_id: key.clone(),
            event_type: Some(event_type.to_owned()),
            session_epoch: message.session_epoch,
            class,
            episode: if class == AudioClass::Critical {
                Some(key)
            } else {
                None
            },
            package_revision: Some(self.table.fence),
            pcm: clip.pcm.clone(),
            visual_tail_ms: clip.visual_tail_ms,
        };

        let t2 = Instant::now();
        let t2_us = Self::offset_us(message.arrival, t2);
        let decision = self.request_playback(now_ms, request);
        let t3 = Instant::now();
        let t3p_us = Self::offset_us(message.arrival, t3);

        self.drain_notices();
        if let Some(decision) = decision {
            let admitted = decision.outcome == Outcome::Admitted;
            let id = decision.playback_id;
            let decision_instance_id = decision.instance_id.clone();
            self.handle_decision(decision, Some(&clip));
            if admitted && self.latency {
                let _ = self.try_output(EngineOut::Latency {
                    instance_id: decision_instance_id,
                    event_type: Some(event_type.to_owned()),
                    t0_ms: self.clock.ms_at(message.arrival),
                    t1_us: Self::offset_us(message.arrival, message.detected),
                    t2_us,
                    t3p_us,
                });
            }
            if let Some(id) = id {
                self.last_issued_id = self.last_issued_id.max(id.0);
            }
        }
    }

    fn resolve_clip(&self, event: Event) -> Result<Clip, Outcome> {
        match self.table.lookup(event.id, event.tier) {
            Lookup::Ready(clip) => Ok(clip.clone()),
            Lookup::Unmapped => Err(Outcome::Unmapped),
            Lookup::Invalid => Err(Outcome::InvalidMedia),
        }
    }

    fn request_playback(&mut self, now_ms: u64, request: Request) -> Option<Decision> {
        self.last_issued_id = self.last_issued_id.saturating_add(1);
        self.owner.request(now_ms, request)
    }

    fn admit_audition(
        &mut self,
        audition: &AuditionRequest,
        now_ms: u64,
        real_event_seen: bool,
    ) -> Result<(), AuditionRefusal> {
        if self.shared.audition_match_active() {
            return Err(AuditionRefusal::MatchActive);
        }
        if real_event_seen
            || self.is_degraded()
            || self.owner.active(now_ms).is_some()
            || self.owner.visual(now_ms).is_some()
        {
            return Err(AuditionRefusal::Busy);
        }
        if self.table.install_id.as_deref() != Some(audition.install_id.as_str()) {
            return Err(AuditionRefusal::MediaStateUnknown);
        }
        let clip = self
            .resolve_clip(audition.event)
            .map_err(|outcome| match outcome {
                Outcome::Unmapped => AuditionRefusal::Unmapped,
                _ => AuditionRefusal::MediaStateUnknown,
            })?;
        let request = Request {
            instance_id: instance_id(AUDITION_GENERATION, audition.sequence, 0),
            event_type: Some("audition".to_owned()),
            session_epoch: AUDITION_EPOCH,
            class: AudioClass::Ordinary,
            episode: None,
            package_revision: Some(self.table.fence),
            pcm: clip.pcm.clone(),
            visual_tail_ms: clip.visual_tail_ms,
        };
        if Instant::now() >= audition.expires_at {
            return Err(AuditionRefusal::Busy);
        }
        if self.shared.audition_match_active() {
            return Err(AuditionRefusal::MatchActive);
        }
        // A tick can arrive while the sink is polled after the lane drains.
        let pending_tick = self
            .critical
            .try_recv()
            .or_else(|_| self.ordinary.try_recv());
        if let Ok(message) = pending_tick {
            self.handle_tick(message, now_ms);
            return Err(AuditionRefusal::Busy);
        }
        self.owner.set_session(now_ms, AUDITION_EPOCH);
        self.audition_session = true;
        let decision = self.request_playback(now_ms, request);
        self.drain_notices();
        match decision {
            Some(mut decision) if decision.outcome == Outcome::Admitted => {
                // The owner fence is local. The outcome record uses the host's
                // positive epoch and identifies the playback as an audition.
                decision.session_epoch = audition.session_epoch;
                self.handle_decision(decision, Some(&clip));
                Ok(())
            }
            decision => {
                self.stop_audition(now_ms, false);
                if decision.is_some_and(|decision| decision.outcome == Outcome::Muted) {
                    Err(AuditionRefusal::Muted)
                } else {
                    Err(AuditionRefusal::Busy)
                }
            }
        }
    }

    fn stop_audition(&mut self, now_ms: u64, preempted: bool) {
        if self.audition_session {
            self.audition_session = false;
            self.owner.set_session(now_ms, self.session_epoch);
            self.publish_notices(preempted);
        }
    }

    fn record_pre_owner(
        &mut self,
        message: &TickMsg,
        event_id: EventId,
        key: &str,
        outcome: Outcome,
        now_ms: u64,
    ) {
        let revision = if self.table.fence == 0 {
            None
        } else {
            Some(self.table.fence)
        };
        if let Some(value) = pre_owner_record(
            key,
            Some(event_id.as_str()),
            message.session_epoch,
            class_of(event_id),
            outcome,
            revision,
            now_ms,
        ) {
            let _ = self.try_output(EngineOut::Outcome(value));
        } else {
            Self::increment_u64(&self.shared.out_dropped);
        }
    }

    fn handle_decision(&mut self, decision: Decision, clip: Option<&Clip>) {
        if decision.outcome == Outcome::Admitted {
            if let Some(id) = decision.playback_id {
                let audio_ends_at_ms = clip
                    .and_then(|clip| decision.decided_at_ms.checked_add(u64::from(clip.audio_ms)));
                self.remember_live(
                    id,
                    decision.clone(),
                    audio_ends_at_ms,
                    clip.map(|clip| clip.pcm.len() as u64),
                );
                self.last_issued_id = self.last_issued_id.max(id.0);
            }
        }

        match record(&decision) {
            Some(value) => {
                let _ = self.try_output(EngineOut::Outcome(value));
            }
            None => Self::increment_u64(&self.shared.out_dropped),
        }
        if decision.outcome == Outcome::Admitted {
            if let Some(clip) = clip {
                if let Some(value) = playback_started(&decision, clip) {
                    let _ = self.try_output(EngineOut::Playback(value));
                } else {
                    Self::increment_u64(&self.shared.out_dropped);
                }
            }
        }
    }

    fn remember_live(
        &mut self,
        id: PlaybackId,
        decision: Decision,
        audio_ends_at_ms: Option<u64>,
        expected_frames: Option<u64>,
    ) {
        self.remove_live(id);
        if self.live.len() < 2 {
            self.live.push(LivePlayback {
                id,
                decision,
                audio_ends_at_ms,
                expected_frames,
            });
        }
    }

    fn remove_live(&mut self, id: PlaybackId) -> Option<LivePlayback> {
        let mut removed = None;
        self.live.retain(|live| {
            if removed.is_none() && live.id == id {
                removed = Some(LivePlayback {
                    id: live.id,
                    decision: live.decision.clone(),
                    audio_ends_at_ms: live.audio_ends_at_ms,
                    expected_frames: live.expected_frames,
                });
                false
            } else {
                true
            }
        });
        removed
    }

    fn drain_notices(&mut self) {
        self.publish_notices(false);
    }

    fn publish_notices(&mut self, preempting_audition: bool) {
        let notices = self.owner.drain_notices();
        for notice in notices {
            match notice {
                Notice::Decided(decision) => self.handle_decision(decision, None),
                Notice::Finished { id, at_ms } => {
                    let live = self.remove_live(id);
                    let _ = self.try_output(EngineOut::Playback(playback_ended(
                        id,
                        EndReason::Finished,
                        at_ms,
                    )));
                    if let Some(live) = live.as_ref() {
                        self.emit_clip_finished(live, "finished");
                    }
                }
                Notice::Cancelled { id, reason, at_ms } => {
                    let live = self.remove_live(id);
                    let reason = if preempting_audition
                        && reason == CancelReason::SessionFence
                        && live
                            .as_ref()
                            .and_then(|live| live.decision.event_type.as_deref())
                            == Some("audition")
                    {
                        CancelReason::Preempted
                    } else {
                        reason
                    };
                    let _ = self.try_output(EngineOut::Playback(playback_ended(
                        id,
                        EndReason::Cancelled(reason),
                        at_ms,
                    )));
                    let finished_reason = match reason {
                        CancelReason::Preempted => "interrupted",
                        CancelReason::SessionFence => "cancelled_session",
                        CancelReason::RevisionFence => "cancelled_revision",
                        CancelReason::Muted => "cancelled_muted",
                        CancelReason::DeviceLost => "cancelled_device_lost",
                    };
                    if reason == CancelReason::Preempted {
                        if let Some(live) = live.as_ref() {
                            let before_audio_end = live
                                .audio_ends_at_ms
                                .is_none_or(|audio_ends_at_ms| at_ms < audio_ends_at_ms);
                            if before_audio_end {
                                if let Some(value) = preempted_record(&live.decision, at_ms) {
                                    let _ = self.try_output(EngineOut::Outcome(value));
                                } else {
                                    Self::increment_u64(&self.shared.out_dropped);
                                }
                            }
                        }
                    }
                    if let Some(live) = live.as_ref() {
                        self.emit_clip_finished(live, finished_reason);
                    }
                }
            }
        }
    }

    fn emit_clip_finished(&mut self, live: &LivePlayback, reason: &str) {
        let Some(event_id) = live.decision.event_type.as_deref() else {
            return;
        };
        let _ = self.try_output(EngineOut::ClipFinished {
            event_id: event_id.to_owned(),
            reason: reason.to_owned(),
            expected_frames: live.expected_frames,
            native_snapshot: super::diagnostics::native_playback_snapshot_json(live.id.0),
            render_thread_priority: super::diagnostics::render_thread_priority(),
        });
    }

    fn install_sink(&mut self, sink: Box<dyn Sink + Send>, now_ms: u64) {
        if let Some(previous) = self.owner.sink_mut().install(sink) {
            let _ = self.try_output(EngineOut::ReleaseSink(previous));
        }
        self.owner.set_device_available(now_ms, true);
        self.drain_notices();
        self.need_sink_requested = false;
        self.replacement_pending = false;
    }

    fn install_pending_if_idle(&mut self, now_ms: u64) {
        if self.is_degraded() || self.owner.active(now_ms).is_some() {
            return;
        }
        if let Some(sink) = self.pending_sink.take() {
            self.install_sink(sink, now_ms);
        }
    }

    fn request_sink_if_needed(&mut self) {
        if !self.need_sink_requested {
            self.request_sink(false);
        }
    }

    fn request_sink(&mut self, force: bool) {
        if force || !self.need_sink_requested {
            self.need_sink_requested = self.try_output(EngineOut::NeedSink);
        }
    }

    fn finish_shutdown(&mut self, now_ms: u64) {
        let replacement = AudioOwner::with_first_playback_id(
            SlotSink::empty(),
            self.config,
            self.last_issued_id.saturating_add(1).max(1),
        );
        let mut old_owner = std::mem::replace(&mut self.owner, replacement);
        let current_sink = old_owner.sink_mut().take();
        drop(old_owner);
        if let Some(sink) = current_sink {
            let _ = self.try_output(EngineOut::ReleaseSink(sink));
        }
        if let Some(sink) = self.pending_sink.take() {
            let _ = self.try_output(EngineOut::ReleaseSink(sink));
        }
        self.owner.set_device_available(now_ms, false);
        self.live.clear();
    }

    fn rebuild_after_panic(&mut self, now_ms: u64) {
        self.audition_session = false;
        let _ =
            self.shared
                .rebuilds
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    Some(current.saturating_add(1))
                });

        while self
            .rebuild_instants
            .front()
            .is_some_and(|instant| now_ms.saturating_sub(*instant) > REBUILD_WINDOW_MS)
        {
            let _ = self.rebuild_instants.pop_front();
        }
        self.rebuild_instants.push_back(now_ms);
        if self.rebuild_instants.len() > MAX_REBUILDS_IN_WINDOW {
            self.shared.degraded.store(true, Ordering::Relaxed);
        }

        let live = std::mem::take(&mut self.live);
        let next_id = match self.last_issued_id.checked_add(1) {
            Some(value) => value,
            None => {
                self.shared.degraded.store(true, Ordering::Relaxed);
                u64::MAX
            }
        };

        let replacement =
            AudioOwner::with_first_playback_id(SlotSink::empty(), self.config, next_id);
        let mut old_owner = std::mem::replace(&mut self.owner, replacement);
        let old_sink = old_owner.sink_mut().take();
        drop(old_owner);
        if let Some(sink) = old_sink {
            let _ = self.try_output(EngineOut::ReleaseSink(sink));
        }
        if let Some(sink) = self.pending_sink.take() {
            let _ = self.try_output(EngineOut::ReleaseSink(sink));
        }

        self.owner.set_muted(now_ms, self.muted);
        self.owner.set_session(now_ms, self.session_epoch);
        self.owner.set_revision(now_ms, self.table.fence);
        self.owner.set_device_available(now_ms, false);
        let _ = self.owner.drain_notices();
        self.need_sink_requested = false;
        self.replacement_pending = false;
        self.request_sink(true);
        for playback in live {
            let _ = self.try_output(EngineOut::Playback(playback_ended(
                playback.id,
                EndReason::Cancelled(CancelReason::DeviceLost),
                now_ms,
            )));
        }
    }

    fn publish_shared(&self, now_ms: u64) {
        self.shared.heartbeat_ms.store(now_ms, Ordering::Relaxed);
        self.shared
            .muted
            .store(self.owner.is_muted(), Ordering::Relaxed);
        self.shared
            .device_available
            .store(self.owner.device_available(), Ordering::Relaxed);
        let lease_active =
            self.owner.active(now_ms).is_some() || self.owner.visual(now_ms).is_some();
        self.shared
            .lease_active
            .store(lease_active, Ordering::Relaxed);
        self.shared.fence.store(self.table.fence, Ordering::Relaxed);
    }

    fn wait_duration(&self, now_ms: u64) -> Duration {
        if !self.shutdown
            && (self.owner.active(now_ms).is_some() || self.owner.visual(now_ms).is_some())
        {
            Duration::from_millis(10)
        } else {
            Duration::from_millis(250)
        }
    }

    fn is_degraded(&self) -> bool {
        self.shared.degraded.load(Ordering::Relaxed)
    }

    fn offset_us(origin: Instant, instant: Instant) -> u64 {
        match instant.checked_duration_since(origin) {
            Some(duration) => {
                let micros = duration.as_micros();
                if micros > u128::from(u64::MAX) {
                    u64::MAX
                } else {
                    // The checked bound makes the narrowing conversion lossless.
                    micros as u64
                }
            }
            None => 0,
        }
    }

    fn try_output(&mut self, output: EngineOut) -> bool {
        match self.out.try_send(output) {
            Ok(()) => true,
            Err(TrySendError::Full(output)) => {
                Self::increment_u64(&self.shared.out_dropped);
                Self::release_resource_off_thread(output);
                false
            }
            Err(TrySendError::Disconnected(output)) => {
                Self::increment_u64(&self.shared.out_dropped);
                Self::release_resource_off_thread(output);
                false
            }
        }
    }

    fn release_resource_off_thread(output: EngineOut) {
        if matches!(
            &output,
            EngineOut::ReleaseSink(_) | EngineOut::ReleaseTable(_)
        ) {
            let retained = std::mem::ManuallyDrop::new(output);
            let _ = thread::Builder::new()
                .name(String::from("audio-resource-release"))
                .spawn(move || drop(std::mem::ManuallyDrop::into_inner(retained)));
        }
    }

    fn increment_u64(counter: &AtomicU64) {
        let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some(current.saturating_add(1))
        });
    }

    /// Spawn the engine thread (name `audio-engine`): inside the thread it creates the lanes
    /// with its own `Thread` handle, sends the `EngineTx` back, then loops
    /// `step(clock.now_ms())` + `park_timeout(returned wait)` until `Control::Shutdown`.
    /// The sink is `None` at spawn: the supervisor answers `EngineOut::NeedSink` with
    /// `Control::SinkReady`.
    pub fn spawn(
        clock: MonoClock,
        shared: Arc<EngineShared>,
        out: SyncSender<EngineOut>,
        latency: bool,
    ) -> io::Result<(EngineTx, JoinHandle<()>)> {
        let (tx_sender, tx_receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker = thread::Builder::new()
            .name(String::from("audio-engine"))
            .spawn(move || {
                let (tx, rx) = super::lanes::lanes(thread::current(), shared.clone());
                let mut engine = Engine::new(clock, shared, rx, None, out, latency);
                if tx_sender.try_send(tx).is_err() {
                    return;
                }

                loop {
                    if worker_cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    if engine.shutdown {
                        break;
                    } else {
                        let wait = engine.step(clock.now_ms());
                        if !engine.shutdown {
                            thread::park_timeout(wait);
                        }
                    }
                }
            })?;

        match tx_receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(tx) => Ok((tx, worker)),
            Err(error) => {
                let kind = match error {
                    mpsc::RecvTimeoutError::Timeout => io::ErrorKind::TimedOut,
                    mpsc::RecvTimeoutError::Disconnected => io::ErrorKind::BrokenPipe,
                };
                cancelled.store(true, Ordering::Relaxed);
                worker.thread().unpark();
                let _ = worker.join();
                Err(io::Error::new(kind, error))
            }
        }
    }
}
