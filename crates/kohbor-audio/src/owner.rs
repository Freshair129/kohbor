//! One audio owner: class priority, preemption, generations, mute, fences.

use crate::pcm::Pcm;
use std::cell::Cell;

/// Duration of a preemption fade at 48 kHz.
pub const PREEMPT_FADE_MS: u64 = 100;
/// Mono samples in [`PREEMPT_FADE_MS`] at the audio sink's 48 kHz sample rate.
pub const PREEMPT_FADE_SAMPLES: usize = (PREEMPT_FADE_MS as usize * 48_000) / 1_000;

/// Audio classes, highest priority first (EVENT-AUDIO-SPEC section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioClass {
    /// C1 critical: `risk_alert`, `health_danger`.
    Critical,
    /// C2 correction: `risk_revision`.
    Correction,
    /// C3 notice: `enemy_missing`.
    Notice,
    /// C4 ordinary announcer events.
    Ordinary,
    /// C5 voice answers and advice speech.
    Voice,
}

impl AudioClass {
    /// `critical`, `correction`, `notice`, `ordinary`, `voice` (audio-outcome `audio_class`).
    pub fn as_str(&self) -> &'static str {
        match self {
            AudioClass::Critical => "critical",
            AudioClass::Correction => "correction",
            AudioClass::Notice => "notice",
            AudioClass::Ordinary => "ordinary",
            AudioClass::Voice => "voice",
        }
    }
}

/// audio-outcome `outcome` values. The owner itself only decides `Admitted`,
/// `Preempted`, `StaleSession`, `SupersededRevision`, `CriticalActive`,
/// `ChannelBusy`, `Cooldown` and `Muted`; the others are decided by the resolver
/// before a request reaches the owner and exist here so one type covers the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    Admitted,
    Preempted,
    Unmapped,
    InvalidMedia,
    Inert,
    NotVoiced,
    DetectorUnavailable,
    StaleSession,
    SupersededRevision,
    CriticalActive,
    ChannelBusy,
    LowerRankSameTick,
    Cooldown,
    Muted,
}

impl Outcome {
    /// Snake-case contract value, for example `critical_active`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Admitted => "admitted",
            Outcome::Preempted => "preempted",
            Outcome::Unmapped => "unmapped",
            Outcome::InvalidMedia => "invalid_media",
            Outcome::Inert => "inert",
            Outcome::NotVoiced => "not_voiced",
            Outcome::DetectorUnavailable => "detector_unavailable",
            Outcome::StaleSession => "stale_session",
            Outcome::SupersededRevision => "superseded_revision",
            Outcome::CriticalActive => "critical_active",
            Outcome::ChannelBusy => "channel_busy",
            Outcome::LowerRankSameTick => "lower_rank_same_tick",
            Outcome::Cooldown => "cooldown",
            Outcome::Muted => "muted",
        }
    }
}

/// Opaque playback identity; each admitted playback gets a new one (its
/// generation), starting at 1 by default and never reused. The host may configure
/// another first id through `AudioOwner::with_first_playback_id`. `to_contract_id`
/// is `pb_` plus 10 decimal digits (`pb_0000000001`), matching the `playback_id`
/// pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlaybackId(pub u64);

impl PlaybackId {
    pub fn to_contract_id(&self) -> String {
        format!("pb_{:010}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkError;

/// Events reported by an output sink to the audio owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkEvent {
    /// The sink finished rendering this playback generation.
    Finished { id: PlaybackId },
    /// The endpoint in use was removed, disabled, or its stream became invalid.
    DeviceLost,
    /// The default render endpoint changed. The sink keeps servicing its existing
    /// stream; recovery or endpoint rebinding is a host responsibility.
    DeviceChanged,
}

/// The output device behind the owner. Implementations must be bounded and
/// non-blocking: neither method may wait for a lock held by another thread,
/// allocate on the audio thread, or panic.
///
/// A sink has one primary playback and at most one fading tail. `submit` starts
/// the new primary immediately; the only mix is a preempted clip's <=100 ms
/// fade-out tail under the new playback. `cancel` hard-cuts both the primary and
/// any tail when `id` is current, and is a no-op for stale or unknown ids. The
/// real device layer keeps one stream open and swaps its source.
///
/// A device can still output the last buffer of the old playback for up to one
/// device period after its deadline. The owner does not model that delay, so its
/// visual can end up to one device period before the sound does.
pub trait Sink {
    /// Atomically replace the current playback with `pcm` (mono 48 kHz) as `id`.
    /// An error means the device did not accept it and has no current playback.
    fn submit(&mut self, id: PlaybackId, pcm: Pcm) -> Result<(), SinkError>;
    /// Stop `id` if it is current; stale or unknown ids are a no-op.
    fn cancel(&mut self, id: PlaybackId);
    /// Fade `id` underneath the next submitted playback. Sinks that do not
    /// support a fade retain their existing hard-cancel behavior.
    fn cancel_with_fade(&mut self, id: PlaybackId, _fade_samples: usize) {
        self.cancel(id);
    }
    /// Return the next sink event, if one is pending. Existing sinks need not
    /// produce events.
    fn poll_event(&mut self) -> Option<SinkEvent> {
        None
    }
}

/// One request to play a verified, prewarmed clip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Per-occurrence identity (event `instance_id`).
    pub instance_id: String,
    /// The owner echoes this for non-voice requests. The host must provide a
    /// contract-valid event type; the owner does not validate it. Voice decisions
    /// always normalize this field to `None`.
    pub event_type: Option<String>,
    pub session_epoch: u64,
    pub class: AudioClass,
    /// Catalog rank for C4 requests. The owner compares this scalar and never
    /// infers event priority from `event_type` text.
    pub ordinary_rank: u32,
    /// Same value for repeated alerts of one critical episode (C1 same-episode drop).
    pub episode: Option<String>,
    /// Active package revision the media came from (`install_id` number). The
    /// host must provide the active revision for non-voice requests; the owner
    /// echoes it without validating the contract range. Voice decisions always
    /// normalize this field to `None`.
    pub package_revision: Option<u64>,
    pub pcm: Pcm,
    /// Visual tail after natural audio end; values above 3,200 ms are clamped.
    pub visual_tail_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// The sink refused `submit`, or playback ids are exhausted; the request was
    /// not admitted.
    SinkFailed,
    /// A queued C2 or C4 request waited longer than the TTL.
    QueueExpired,
    /// A queued C2 or C4 request lost its one-slot queue to a newer request.
    QueueReplaced,
    /// The device is marked unavailable.
    DeviceUnavailable,
    /// The request contains no PCM samples. This is a lead proposal because the
    /// closed outcome contract has no more specific outcome than `channel_busy`.
    EmptyMedia,
}

/// The owner's decision for one request (audio-outcome minus the fixed fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub instance_id: String,
    pub event_type: Option<String>,
    pub session_epoch: u64,
    pub class: AudioClass,
    pub outcome: Outcome,
    /// Set for `Admitted` and `Preempted`.
    pub playback_id: Option<PlaybackId>,
    pub package_revision: Option<u64>,
    pub decided_at_ms: u64,
    pub detail: Option<Detail>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    /// A higher class (or a newer C2 revision) took the output.
    Preempted,
    SessionFence,
    RevisionFence,
    Muted,
    DeviceLost,
}

/// Asynchronous things that happen to playbacks, in order of occurrence.
///
/// The host keeps the `Decision` for each admitted playback keyed by
/// `PlaybackId` until its `Finished` or `Cancelled` notice arrives. For a
/// `Preempted` cancellation, it emits the old playback's `preempted` outcome
/// record using that decision's identity fields and this notice's `at_ms` as
/// `decided_monotonic_ms`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// A request that was queued earlier got its decision now: played after the
    /// critical audio ended (`Admitted`), expired (`CriticalActive` with
    /// `QueueExpired`) or replaced (`CriticalActive` with `QueueReplaced`).
    Decided(Decision),
    /// Audio and the paired visual tail of `id` were cancelled together.
    Cancelled {
        id: PlaybackId,
        reason: CancelReason,
        /// Monotonic instant at which cancellation occurred.
        at_ms: u64,
    },
    /// Audio (and the visual tail) of `id` ended naturally.
    Finished {
        id: PlaybackId,
        /// Monotonic instant at which the visual lifecycle ended.
        at_ms: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualPhase {
    /// Audio is playing; the visual plays with it.
    Audio,
    /// Audio ended; only the declared visual tail remains.
    Tail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualState {
    pub id: PlaybackId,
    pub phase: VisualPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// C2 queue time-to-live (default 3,000 ms).
    pub correction_queue_ttl_ms: u64,
    /// C4 ordinary queue time-to-live (default 3,000 ms).
    pub ordinary_queue_ttl_ms: u64,
    /// Minimum gap between two admitted C3 notices (default 10,000 ms).
    pub notice_cooldown_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            correction_queue_ttl_ms: 3_000,
            ordinary_queue_ttl_ms: 3_000,
            notice_cooldown_ms: 10_000,
        }
    }
}

struct Playback {
    id: PlaybackId,
    class: AudioClass,
    ordinary_rank: u32,
    episode: Option<String>,
    session_epoch: u64,
    package_revision: Option<u64>,
    /// `None` means the checked deadline overflowed; only a callback, cancel or
    /// fence can then end this playback.
    audio_ends_at_ms: Option<u64>,
    visual_tail_ms: u32,
}

struct VisualTail {
    id: PlaybackId,
    class: AudioClass,
    session_epoch: u64,
    package_revision: Option<u64>,
    started_at_ms: u64,
    /// `None` means the checked tail deadline overflowed and only cancellation
    /// or a fence can end this visual tail.
    ends_at_ms: Option<u64>,
}

struct QueuedCorrection {
    request: Request,
    queued_at_ms: u64,
}

struct QueuedOrdinary {
    request: Request,
    queued_at_ms: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QueueAdvance {
    All,
    CorrectionOnly,
    ExpireOnly,
    Fence,
}

/// Pure, single-owner arbitration state. Every timestamp-bearing call uses
/// `max(at_ms, last_at_ms)`; owner time never moves backwards.
pub struct AudioOwner<S: Sink> {
    sink: S,
    config: Config,
    muted: bool,
    device_available: bool,
    session_epoch: u64,
    package_revision: u64,
    next_playback_id: u64,
    playback_ids_exhausted: bool,
    active: Option<Playback>,
    visual_tail: Option<VisualTail>,
    queued_correction: Option<QueuedCorrection>,
    queued_ordinary: Option<QueuedOrdinary>,
    last_notice_admitted_at_ms: Option<u64>,
    notices: Vec<Notice>,
    dropped_notices: u64,
    last_at_ms: Cell<u64>,
}

impl<S: Sink> AudioOwner<S> {
    pub fn new(sink: S, config: Config) -> AudioOwner<S> {
        Self::with_first_playback_id(sink, config, 1)
    }

    /// Create an owner using `first_playback_id` for the first playback id.
    ///
    /// A value of 0 is treated as 1. `u64::MAX` remains a valid final id; after
    /// it is used, the counter stays at `u64::MAX` and later playback attempts
    /// return `ChannelBusy` with `Detail::SinkFailed` rather than wrapping.
    pub fn with_first_playback_id(
        sink: S,
        config: Config,
        first_playback_id: u64,
    ) -> AudioOwner<S> {
        AudioOwner {
            sink,
            config,
            muted: false,
            device_available: true,
            session_epoch: 0,
            package_revision: 0,
            next_playback_id: first_playback_id.max(1),
            playback_ids_exhausted: false,
            active: None,
            visual_tail: None,
            queued_correction: None,
            queued_ordinary: None,
            last_notice_admitted_at_ms: None,
            notices: Vec::new(),
            dropped_notices: 0,
            last_at_ms: Cell::new(0),
        }
    }

    /// Decide one request at `at_ms`. Time is normalized to
    /// `max(at_ms, last_at_ms)`, so owner time never moves backwards. Check order
    /// (first match wins):
    /// 1. mute on -> `Muted`;
    /// 2. `session_epoch` differs from the current session -> `StaleSession`;
    /// 3. for classes other than voice: `package_revision` is `None` or differs from
    ///    the current revision -> `SupersededRevision` (voice never has a revision
    ///    and skips this check);
    /// 4. device marked unavailable -> `ChannelBusy` with `Detail::DeviceUnavailable`;
    /// 5. the priority matrix of EVENT-AUDIO-SPEC section 4 against the class that
    ///    holds the audio channel at `at_ms` (a playback holds it from admission
    ///    until `admission + ceil(samples * 1000 / 48_000)`; the short preemption
    ///    tail does not extend the new playback's channel hold):
    ///    - C1: idle -> play; C1 active with the same `episode` (both `Some` and
    ///      equal) -> `Cooldown`; C1 active with a different episode -> preempt;
    ///      C2..C5 active -> preempt.
    ///    - C2: idle -> play; C1 active -> queue (latest only: a newer queued C2
    ///      replaces the older one, which gets `Notice::Decided(CriticalActive,
    ///      QueueReplaced)`); C2 active -> preempt (newer revision); C3, C4, C5
    ///      active -> preempt.
    ///    - C3: within `notice_cooldown_ms` of the last admitted C3 -> `Cooldown`
    ///      (checked before the channel); C1 or C2 active -> `CriticalActive`;
    ///      C3 active -> `ChannelBusy`; C4 or C5 active -> preempt; idle -> play.
    ///    - C4: C1 or C2 active -> queue by rank (latest wins ties); C3 active
    ///      -> `ChannelBusy`; higher-ranked C4 active -> preempt with a 100 ms
    ///      fade; equal/lower-ranked C4 active -> `ChannelBusy`; C5 active ->
    ///      preempt; idle -> play. C1, C2, and C3 preempting C4 also use that
    ///      fade; their other preemptions remain hard cuts.
    ///    - C5: any of C1..C4 active -> `ChannelBusy` (the host waits and
    ///      re-submits; the owner never queues voice); C5 active -> preempt
    ///      (supersede); idle -> play.
    /// 6. On play or preempt: when preempting, cancel the old playback first
    ///    (using a fade only when C1/C2/C3 or higher-ranked C4 preempts C4) and queue
    ///    `Notice::Cancelled{Preempted}`; then `sink.submit`. A `SinkError` gives
    ///    `ChannelBusy` with `Detail::SinkFailed` and nothing becomes active.
    ///
    /// Returns `Some(Decision)` for every request except a queued C2 or C4 (decided
    /// later through `Notice::Decided`). A request that plays, on an idle channel
    /// or by preempting, is `Admitted`; the `preempted` outcome of the contract
    /// belongs to the OLD playback and is derived by the host from the
    /// `Notice::Cancelled { reason: Preempted }` the owner emits for it.
    pub fn request(&mut self, at_ms: u64, request: Request) -> Option<Decision> {
        let at_ms = self.advance_for_request(at_ms, request.class);

        if self.muted {
            return Some(Self::decision(request, at_ms, Outcome::Muted, None, None));
        }
        if request.session_epoch != self.session_epoch {
            return Some(Self::decision(
                request,
                at_ms,
                Outcome::StaleSession,
                None,
                None,
            ));
        }
        if request.class != AudioClass::Voice
            && request.package_revision != Some(self.package_revision)
        {
            return Some(Self::decision(
                request,
                at_ms,
                Outcome::SupersededRevision,
                None,
                None,
            ));
        }
        if !self.device_available {
            return Some(Self::decision(
                request,
                at_ms,
                Outcome::ChannelBusy,
                None,
                Some(Detail::DeviceUnavailable),
            ));
        }

        if request.pcm.is_empty() {
            return Some(Self::decision(
                request,
                at_ms,
                Outcome::ChannelBusy,
                None,
                Some(Detail::EmptyMedia),
            ));
        }

        if request.class == AudioClass::Notice {
            if let Some(last) = self.last_notice_admitted_at_ms {
                if at_ms.saturating_sub(last) < self.config.notice_cooldown_ms {
                    return Some(Self::decision(
                        request,
                        at_ms,
                        Outcome::Cooldown,
                        None,
                        None,
                    ));
                }
            }
        }

        let active_class = self.active_at(at_ms).map(|playback| playback.class);
        match (request.class, active_class) {
            (AudioClass::Critical, Some(AudioClass::Critical)) => {
                let same_episode = self
                    .active_at(at_ms)
                    .and_then(|playback| playback.episode.as_deref())
                    .zip(request.episode.as_deref())
                    .is_some_and(|(left, right)| left == right);
                if same_episode {
                    return Some(Self::decision(
                        request,
                        at_ms,
                        Outcome::Cooldown,
                        None,
                        None,
                    ));
                }
                Some(self.start(request, at_ms, true))
            }
            (AudioClass::Critical, Some(AudioClass::Ordinary)) => {
                Some(self.start_with_fade(request, at_ms))
            }
            (AudioClass::Critical, Some(_)) => Some(self.start(request, at_ms, true)),
            (AudioClass::Critical, None) => Some(self.start(request, at_ms, false)),

            (AudioClass::Correction, Some(AudioClass::Critical)) => {
                self.queue_correction(at_ms, request);
                None
            }
            (AudioClass::Correction, Some(AudioClass::Ordinary)) => {
                Some(self.start_with_fade(request, at_ms))
            }
            (AudioClass::Correction, Some(_)) => Some(self.start(request, at_ms, true)),
            (AudioClass::Correction, None) => Some(self.start(request, at_ms, false)),

            (AudioClass::Notice, Some(AudioClass::Critical | AudioClass::Correction)) => Some(
                Self::decision(request, at_ms, Outcome::CriticalActive, None, None),
            ),
            (AudioClass::Notice, Some(AudioClass::Notice)) => Some(Self::decision(
                request,
                at_ms,
                Outcome::ChannelBusy,
                None,
                None,
            )),
            (AudioClass::Notice, Some(AudioClass::Ordinary)) => {
                Some(self.start_with_fade(request, at_ms))
            }
            (AudioClass::Notice, Some(AudioClass::Voice)) => Some(self.start(request, at_ms, true)),
            (AudioClass::Notice, None) => Some(self.start(request, at_ms, false)),

            (AudioClass::Ordinary, Some(AudioClass::Critical | AudioClass::Correction)) => {
                self.queue_ordinary(at_ms, request)
            }
            (AudioClass::Ordinary, Some(AudioClass::Notice)) => Some(Self::decision(
                request,
                at_ms,
                Outcome::ChannelBusy,
                None,
                None,
            )),
            (AudioClass::Ordinary, Some(AudioClass::Ordinary)) => {
                if self
                    .active_at(at_ms)
                    .is_some_and(|playback| request.ordinary_rank > playback.ordinary_rank)
                {
                    Some(self.start_with_fade(request, at_ms))
                } else {
                    Some(Self::decision(
                        request,
                        at_ms,
                        Outcome::ChannelBusy,
                        None,
                        None,
                    ))
                }
            }
            (AudioClass::Ordinary, Some(AudioClass::Voice)) => {
                Some(self.start(request, at_ms, true))
            }
            (AudioClass::Ordinary, None) => Some(self.start(request, at_ms, false)),

            (AudioClass::Voice, Some(AudioClass::Voice)) => Some(self.start(request, at_ms, true)),
            (AudioClass::Voice, Some(_)) => Some(Self::decision(
                request,
                at_ms,
                Outcome::ChannelBusy,
                None,
                None,
            )),
            (AudioClass::Voice, None) => Some(self.start(request, at_ms, false)),
        }
    }

    /// Advance time: audio that ended is finished (`Notice::Finished` once the
    /// audio and the declared visual tail are over); a queued C2 or C4 starts when
    /// the channel frees (`Decided(Admitted)`), using that audio deadline as its
    /// decision time if still eligible at the call time. Queued requests expire
    /// at `queued_at + ttl + 1` ms.
    pub fn tick(&mut self, at_ms: u64) {
        self.advance(at_ms, QueueAdvance::All);
    }

    /// The sink reports that `id` finished (device callback). Ignored for an id
    /// that is not the current playback (late completion of a cancelled one),
    /// without advancing time or changing any owner state. For the current id,
    /// an early callback ends audio at the effective callback time.
    pub fn on_sink_finished(&mut self, at_ms: u64, id: PlaybackId) {
        if self
            .active
            .as_ref()
            .is_none_or(|playback| playback.id != id)
        {
            return;
        }
        let at_ms = self.advance(at_ms, QueueAdvance::All);
        if self
            .active
            .as_ref()
            .is_some_and(|playback| playback.id == id)
        {
            self.finish_audio(at_ms);
            self.advance_to(at_ms, QueueAdvance::All);
        }
    }

    /// Drain all pending sink events at `at_ms`.
    ///
    /// A default-device change does not alter owner state: the sink continues
    /// servicing the endpoint stream it already owns. Device recovery and any
    /// endpoint rebinding are handled by the host.
    pub fn pump_sink_events(&mut self, at_ms: u64) {
        while let Some(event) = self.sink.poll_event() {
            match event {
                SinkEvent::Finished { id } => self.on_sink_finished(at_ms, id),
                SinkEvent::DeviceLost => self.set_device_available(at_ms, false),
                SinkEvent::DeviceChanged => {}
            }
        }
    }

    /// Take the pending notices in order of occurrence. Hosts must drain notices
    /// on every tick; the queue retains at most 256 entries.
    pub fn drain_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    /// Number of notices dropped because the bounded notice queue was full.
    /// Draining notices does not reset this counter.
    pub fn dropped_notices(&self) -> u64 {
        self.dropped_notices
    }

    /// Global mute: on cancels the active playback (`Muted`) and the queue; later
    /// requests get `Muted`. Off restores nothing.
    pub fn set_muted(&mut self, at_ms: u64, muted: bool) {
        let at_ms = self.advance_for_fence(at_ms, muted);
        self.muted = muted;
        if muted {
            self.cancel_active(CancelReason::Muted, at_ms);
            self.cancel_tail(CancelReason::Muted, at_ms);
            self.clear_queued(at_ms, Outcome::Muted, None);
        }
    }

    /// New current session: cancels the active playback and the queue when their
    /// epoch differs (`SessionFence`). The initial session is 0 (no request with
    /// epoch 0 is stale before `set_session` is called).
    pub fn set_session(&mut self, at_ms: u64, session_epoch: u64) {
        let at_ms = self.advance_for_fence(at_ms, session_epoch != self.session_epoch);
        self.session_epoch = session_epoch;
        if self
            .active
            .as_ref()
            .is_some_and(|playback| playback.session_epoch != session_epoch)
        {
            self.cancel_active(CancelReason::SessionFence, at_ms);
        }
        if self
            .visual_tail
            .as_ref()
            .is_some_and(|tail| tail.session_epoch != session_epoch)
        {
            self.cancel_tail(CancelReason::SessionFence, at_ms);
        }
        if self
            .queued_correction
            .as_ref()
            .is_some_and(|queued| queued.request.session_epoch != session_epoch)
            || self
                .queued_ordinary
                .as_ref()
                .is_some_and(|queued| queued.request.session_epoch != session_epoch)
        {
            self.clear_queued(at_ms, Outcome::StaleSession, None);
        }
    }

    /// New current package revision: cancels the active non-voice playback and the
    /// queue when their revision differs (`RevisionFence`); a voice playback is not
    /// affected. The initial revision is 0.
    pub fn set_revision(&mut self, at_ms: u64, package_revision: u64) {
        let at_ms = self.advance_for_fence(at_ms, package_revision != self.package_revision);
        self.package_revision = package_revision;
        if self.active.as_ref().is_some_and(|playback| {
            playback.class != AudioClass::Voice
                && playback.package_revision != Some(package_revision)
        }) {
            self.cancel_active(CancelReason::RevisionFence, at_ms);
        }
        if self.visual_tail.as_ref().is_some_and(|tail| {
            tail.class != AudioClass::Voice && tail.package_revision != Some(package_revision)
        }) {
            self.cancel_tail(CancelReason::RevisionFence, at_ms);
        }
        if self
            .queued_correction
            .as_ref()
            .is_some_and(|queued| queued.request.package_revision != Some(package_revision))
            || self
                .queued_ordinary
                .as_ref()
                .is_some_and(|queued| queued.request.package_revision != Some(package_revision))
        {
            self.clear_queued(at_ms, Outcome::SupersededRevision, None);
        }
    }

    /// The output device disappeared or came back. `false` cancels the active
    /// playback (`DeviceLost`) and the queue; requests then get `ChannelBusy` with
    /// `Detail::DeviceUnavailable` until it is `true` again. Initially available.
    pub fn set_device_available(&mut self, at_ms: u64, available: bool) {
        let at_ms = self.advance_for_fence(at_ms, !available);
        self.device_available = available;
        if !available {
            self.cancel_active(CancelReason::DeviceLost, at_ms);
            self.cancel_tail(CancelReason::DeviceLost, at_ms);
            self.clear_queued(at_ms, Outcome::ChannelBusy, Some(Detail::DeviceUnavailable));
        }
    }

    /// Clear queued requests when the owning engine is stopping. The closed
    /// outcome vocabulary has no shutdown reason, so queued work is reported as
    /// `ChannelBusy` without implying mute or device loss.
    pub fn stop(&mut self, at_ms: u64) {
        let at_ms = self.effective_at(at_ms);
        self.last_at_ms.set(at_ms);
        self.clear_queued(at_ms, Outcome::ChannelBusy, None);
    }

    /// The playback that holds the audio channel at `at_ms`, if any.
    pub fn active(&self, at_ms: u64) -> Option<PlaybackId> {
        self.active_at(self.effective_at(at_ms))
            .map(|playback| playback.id)
    }

    /// Class of the playback that holds the audio channel at `at_ms`, if any.
    pub fn active_class(&self, at_ms: u64) -> Option<AudioClass> {
        self.active_at(self.effective_at(at_ms))
            .map(|playback| playback.class)
    }

    /// The visual that must be on screen at `at_ms` (audio phase or tail), if any.
    /// Cancellation of audio cancels this at the same instant.
    pub fn visual(&self, at_ms: u64) -> Option<VisualState> {
        let at_ms = self.effective_at(at_ms);
        if let Some(playback) = self.active.as_ref() {
            if playback
                .audio_ends_at_ms
                .is_none_or(|ends_at_ms| at_ms < ends_at_ms)
            {
                return Some(VisualState {
                    id: playback.id,
                    phase: VisualPhase::Audio,
                });
            }
            let tail_ends_at_ms = playback
                .audio_ends_at_ms
                .and_then(|ends_at_ms| ends_at_ms.checked_add(u64::from(playback.visual_tail_ms)));
            if playback.visual_tail_ms > 0
                && tail_ends_at_ms.is_none_or(|ends_at_ms| at_ms < ends_at_ms)
            {
                return Some(VisualState {
                    id: playback.id,
                    phase: VisualPhase::Tail,
                });
            }
        }
        self.visual_tail
            .as_ref()
            .filter(|tail| {
                at_ms >= tail.started_at_ms
                    && tail.ends_at_ms.is_none_or(|ends_at_ms| at_ms < ends_at_ms)
            })
            .map(|tail| VisualState {
                id: tail.id,
                phase: VisualPhase::Tail,
            })
    }

    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// Mutable access for host device recovery, such as replacing a lost sink.
    /// The caller must not submit or cancel through this reference; the owner
    /// remains responsible for playback admission and cancellation.
    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    /// Whether global mute is enabled.
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// Whether the output device is currently marked available.
    pub fn device_available(&self) -> bool {
        self.device_available
    }

    fn effective_at(&self, at_ms: u64) -> u64 {
        let effective_at_ms = at_ms.max(self.last_at_ms.get());
        self.last_at_ms.set(effective_at_ms);
        effective_at_ms
    }

    fn active_at(&self, at_ms: u64) -> Option<&Playback> {
        self.active.as_ref().filter(|playback| {
            playback
                .audio_ends_at_ms
                .is_none_or(|ends_at_ms| at_ms < ends_at_ms)
        })
    }

    fn decision(
        request: Request,
        at_ms: u64,
        outcome: Outcome,
        playback_id: Option<PlaybackId>,
        detail: Option<Detail>,
    ) -> Decision {
        let is_voice = request.class == AudioClass::Voice;
        Decision {
            instance_id: request.instance_id,
            event_type: if is_voice { None } else { request.event_type },
            session_epoch: request.session_epoch,
            class: request.class,
            outcome,
            playback_id,
            package_revision: if is_voice {
                None
            } else {
                request.package_revision
            },
            decided_at_ms: at_ms,
            detail,
        }
    }

    fn start(&mut self, request: Request, at_ms: u64, preempt: bool) -> Decision {
        self.start_internal(request, at_ms, preempt, false)
    }

    fn start_with_fade(&mut self, request: Request, at_ms: u64) -> Decision {
        self.start_internal(request, at_ms, true, true)
    }

    fn start_internal(
        &mut self,
        request: Request,
        at_ms: u64,
        preempt: bool,
        fade_preempt: bool,
    ) -> Decision {
        if self.playback_ids_exhausted {
            return Self::decision(
                request,
                at_ms,
                Outcome::ChannelBusy,
                None,
                Some(Detail::SinkFailed),
            );
        }
        let id_value = self.next_playback_id;
        let id = PlaybackId(id_value);
        if let Some(next_id) = id_value.checked_add(1) {
            self.next_playback_id = next_id;
        } else {
            // Keep the counter saturated at MAX. The exhausted flag prevents
            // reissuing MAX and preserves strictly increasing playback ids.
            self.playback_ids_exhausted = true;
        }

        if preempt {
            if fade_preempt {
                self.cancel_active_with_fade(
                    CancelReason::Preempted,
                    at_ms,
                    Some(PREEMPT_FADE_SAMPLES),
                );
            } else {
                self.cancel_active(CancelReason::Preempted, at_ms);
            }
        }

        let Request {
            instance_id,
            event_type,
            session_epoch,
            class,
            ordinary_rank,
            episode,
            package_revision,
            pcm,
            visual_tail_ms,
            ..
        } = request;
        let event_type = if class == AudioClass::Voice {
            None
        } else {
            event_type
        };
        let package_revision = if class == AudioClass::Voice {
            None
        } else {
            package_revision
        };
        let visual_tail_ms = visual_tail_ms.min(3_200);
        let audio_ends_at_ms = playback_deadline(at_ms, pcm.len());

        if self.sink.submit(id, pcm).is_err() {
            return Decision {
                instance_id,
                event_type,
                session_epoch,
                class,
                outcome: Outcome::ChannelBusy,
                playback_id: None,
                package_revision,
                decided_at_ms: at_ms,
                detail: Some(Detail::SinkFailed),
            };
        }

        self.cancel_tail(CancelReason::Preempted, at_ms);
        self.active = Some(Playback {
            id,
            class,
            ordinary_rank,
            episode,
            session_epoch,
            package_revision,
            audio_ends_at_ms,
            visual_tail_ms,
        });
        if class == AudioClass::Notice {
            self.last_notice_admitted_at_ms = Some(at_ms);
        }

        Decision {
            instance_id,
            event_type,
            session_epoch,
            class,
            outcome: Outcome::Admitted,
            playback_id: Some(id),
            package_revision,
            decided_at_ms: at_ms,
            detail: None,
        }
    }

    fn queue_correction(&mut self, at_ms: u64, request: Request) {
        if let Some(previous) = self.queued_correction.replace(QueuedCorrection {
            request,
            queued_at_ms: at_ms,
        }) {
            self.push_notice(Notice::Decided(Self::decision(
                previous.request,
                at_ms,
                Outcome::CriticalActive,
                None,
                Some(Detail::QueueReplaced),
            )));
        }
    }

    fn queue_ordinary(&mut self, at_ms: u64, request: Request) -> Option<Decision> {
        if self
            .queued_ordinary
            .as_ref()
            .is_some_and(|queued| request.ordinary_rank < queued.request.ordinary_rank)
        {
            return Some(Self::decision(
                request,
                at_ms,
                Outcome::CriticalActive,
                None,
                Some(Detail::QueueReplaced),
            ));
        }

        if let Some(previous) = self.queued_ordinary.replace(QueuedOrdinary {
            request,
            queued_at_ms: at_ms,
        }) {
            self.push_notice(Notice::Decided(Self::decision(
                previous.request,
                at_ms,
                Outcome::CriticalActive,
                None,
                Some(Detail::QueueReplaced),
            )));
        }
        None
    }

    fn advance(&mut self, at_ms: u64, queue: QueueAdvance) -> u64 {
        let effective_at_ms = self.effective_at(at_ms);
        self.advance_to(effective_at_ms, queue);
        effective_at_ms
    }

    fn advance_for_request(&mut self, at_ms: u64, incoming_class: AudioClass) -> u64 {
        // C1 admission must never wait on a queued sink submit.
        let queue = match incoming_class {
            AudioClass::Critical => QueueAdvance::ExpireOnly,
            AudioClass::Correction | AudioClass::Notice => QueueAdvance::CorrectionOnly,
            AudioClass::Ordinary | AudioClass::Voice => QueueAdvance::All,
        };
        self.advance(at_ms, queue)
    }

    fn advance_for_fence(&mut self, at_ms: u64, fencing: bool) -> u64 {
        // Finish elapsed playback, then let the caller decide the queue with
        // its fence outcome.
        let queue = if fencing {
            QueueAdvance::Fence
        } else {
            QueueAdvance::All
        };
        self.advance(at_ms, queue)
    }

    fn advance_to(&mut self, at_ms: u64, queue: QueueAdvance) {
        let channel_was_idle = self.active.is_none();
        loop {
            let audio_end = self
                .active
                .as_ref()
                .and_then(|playback| playback.audio_ends_at_ms)
                .filter(|ends_at_ms| *ends_at_ms <= at_ms);
            let tail_end = self
                .visual_tail
                .as_ref()
                .and_then(|tail| tail.ends_at_ms)
                .filter(|ends_at_ms| *ends_at_ms <= at_ms);
            let (correction_expiry, ordinary_expiry) = if queue == QueueAdvance::Fence {
                (None, None)
            } else {
                (
                    self.correction_expiry_at()
                        .filter(|expiry| *expiry <= at_ms),
                    self.ordinary_expiry_at().filter(|expiry| *expiry <= at_ms),
                )
            };
            let next_event = [audio_end, tail_end, correction_expiry, ordinary_expiry]
                .into_iter()
                .flatten()
                .min();
            let Some(event_at_ms) = next_event else {
                break;
            };

            // A tail belongs to an earlier playback than audio that ends at the
            // same instant, so finish it first.
            if tail_end == Some(event_at_ms) {
                if let Some(tail) = self.visual_tail.take() {
                    self.push_notice(Notice::Finished {
                        id: tail.id,
                        at_ms: event_at_ms,
                    });
                }
            }
            if audio_end == Some(event_at_ms) {
                self.finish_audio_from_expected_end(event_at_ms);
            }
            if correction_expiry == Some(event_at_ms) {
                self.clear_correction_queued(
                    event_at_ms,
                    Outcome::CriticalActive,
                    Some(Detail::QueueExpired),
                );
            }
            if ordinary_expiry == Some(event_at_ms) {
                self.clear_ordinary_queued(
                    event_at_ms,
                    Outcome::CriticalActive,
                    Some(Detail::QueueExpired),
                );
            }
            if !channel_was_idle && audio_end == Some(event_at_ms) && self.active.is_none() {
                // C2 takes the freed slot first. A waiting C4 stays queued until
                // C2 ends or the C4 TTL expires.
                self.start_queued(event_at_ms, at_ms, queue);
            }
        }

        // If a prior submit failed while the channel was idle, reconsider the
        // highest-priority surviving queue on this call.
        if channel_was_idle && self.active.is_none() {
            self.start_queued(at_ms, at_ms, queue);
        }
    }

    fn correction_expiry_at(&self) -> Option<u64> {
        self.queued_correction.as_ref().and_then(|queued| {
            queued
                .queued_at_ms
                .checked_add(self.config.correction_queue_ttl_ms)?
                .checked_add(1)
        })
    }

    fn ordinary_expiry_at(&self) -> Option<u64> {
        self.queued_ordinary.as_ref().and_then(|queued| {
            queued
                .queued_at_ms
                .checked_add(self.config.ordinary_queue_ttl_ms)?
                .checked_add(1)
        })
    }

    fn finish_audio_from_expected_end(&mut self, at_ms: u64) {
        if self
            .active
            .as_ref()
            .and_then(|playback| playback.audio_ends_at_ms)
            != Some(at_ms)
        {
            return;
        }
        if let Some(playback) = self.active.take() {
            self.set_tail_or_finish(playback, at_ms);
        }
    }

    fn finish_audio(&mut self, at_ms: u64) {
        let Some(playback) = self.active.take() else {
            return;
        };
        self.set_tail_or_finish(playback, at_ms);
    }

    fn set_tail_or_finish(&mut self, playback: Playback, audio_ended_at_ms: u64) {
        if playback.visual_tail_ms == 0 {
            self.push_notice(Notice::Finished {
                id: playback.id,
                at_ms: audio_ended_at_ms,
            });
            return;
        }
        let ends_at_ms = audio_ended_at_ms.checked_add(u64::from(playback.visual_tail_ms));
        if let Some(previous) = self.visual_tail.replace(VisualTail {
            id: playback.id,
            class: playback.class,
            session_epoch: playback.session_epoch,
            package_revision: playback.package_revision,
            started_at_ms: audio_ended_at_ms,
            ends_at_ms,
        }) {
            self.push_notice(Notice::Finished {
                id: previous.id,
                at_ms: previous.ends_at_ms.unwrap_or(audio_ended_at_ms),
            });
        }
    }

    fn cancel_active(&mut self, reason: CancelReason, at_ms: u64) {
        self.cancel_active_with_fade(reason, at_ms, None);
    }

    fn cancel_active_with_fade(
        &mut self,
        reason: CancelReason,
        at_ms: u64,
        fade_samples: Option<usize>,
    ) {
        if let Some(playback) = self.active.take() {
            if let Some(fade_samples) = fade_samples {
                self.sink.cancel_with_fade(playback.id, fade_samples);
            } else {
                self.sink.cancel(playback.id);
            }
            self.push_notice(Notice::Cancelled {
                id: playback.id,
                reason,
                at_ms,
            });
        }
    }

    fn cancel_tail(&mut self, reason: CancelReason, at_ms: u64) {
        if let Some(tail) = self.visual_tail.take() {
            self.push_notice(Notice::Cancelled {
                id: tail.id,
                reason,
                at_ms,
            });
        }
    }

    fn clear_queued(&mut self, at_ms: u64, outcome: Outcome, detail: Option<Detail>) {
        self.clear_correction_queued(at_ms, outcome, detail);
        self.clear_ordinary_queued(at_ms, outcome, detail);
    }

    fn clear_correction_queued(&mut self, at_ms: u64, outcome: Outcome, detail: Option<Detail>) {
        if let Some(queued) = self.queued_correction.take() {
            self.push_notice(Notice::Decided(Self::decision(
                queued.request,
                at_ms,
                outcome,
                None,
                detail,
            )));
        }
    }

    fn clear_ordinary_queued(&mut self, at_ms: u64, outcome: Outcome, detail: Option<Detail>) {
        if let Some(queued) = self.queued_ordinary.take() {
            self.push_notice(Notice::Decided(Self::decision(
                queued.request,
                at_ms,
                outcome,
                None,
                detail,
            )));
        }
    }

    fn start_queued(&mut self, at_ms: u64, call_at_ms: u64, queue: QueueAdvance) {
        if matches!(queue, QueueAdvance::ExpireOnly | QueueAdvance::Fence) {
            return;
        }
        // The audio end may be historical. Eligibility is checked at the time
        // of the call because the sink submit happens now.
        let request = if self.queued_correction.is_some()
            && self
                .correction_expiry_at()
                .is_none_or(|expiry| call_at_ms < expiry)
        {
            self.queued_correction.take().map(|queued| queued.request)
        } else if queue == QueueAdvance::All
            && self
                .ordinary_expiry_at()
                .is_none_or(|expiry| call_at_ms < expiry)
        {
            self.queued_ordinary.take().map(|queued| queued.request)
        } else {
            None
        };
        let Some(request) = request else {
            return;
        };
        let decision = if self.muted {
            Self::decision(request, at_ms, Outcome::Muted, None, None)
        } else if request.session_epoch != self.session_epoch {
            Self::decision(request, at_ms, Outcome::StaleSession, None, None)
        } else if request.package_revision != Some(self.package_revision) {
            Self::decision(request, at_ms, Outcome::SupersededRevision, None, None)
        } else if !self.device_available {
            Self::decision(
                request,
                at_ms,
                Outcome::ChannelBusy,
                None,
                Some(Detail::DeviceUnavailable),
            )
        } else {
            self.start(request, at_ms, false)
        };
        self.push_notice(Notice::Decided(decision));
    }

    fn push_notice(&mut self, notice: Notice) {
        const NOTICE_CAPACITY: usize = 256;
        if self.notices.len() == NOTICE_CAPACITY {
            self.notices.remove(0);
            self.dropped_notices = self.dropped_notices.saturating_add(1);
        }
        self.notices.push(notice);
    }
}

fn playback_deadline(admitted_at_ms: u64, samples: usize) -> Option<u64> {
    let samples = u64::try_from(samples).ok()?;
    let duration_numerator = samples.checked_mul(1_000)?;
    let rounded_numerator = duration_numerator.checked_add(47_999)?;
    let duration_ms = rounded_numerator.checked_div(48_000)?;
    admitted_at_ms.checked_add(duration_ms)
}

#[cfg(test)]
mod tests {
    use super::{
        AudioClass, AudioOwner, CancelReason, Config, Notice, Outcome, PlaybackId, Request, Sink,
        SinkError, PREEMPT_FADE_MS, PREEMPT_FADE_SAMPLES,
    };
    use crate::pcm::Pcm;

    #[derive(Debug, PartialEq, Eq)]
    enum Call {
        Submit(PlaybackId),
        Cancel(PlaybackId),
        CancelWithFade(PlaybackId, usize),
    }

    #[derive(Default)]
    struct TestSink {
        calls: Vec<Call>,
    }

    impl Sink for TestSink {
        fn submit(&mut self, id: PlaybackId, _pcm: Pcm) -> Result<(), SinkError> {
            self.calls.push(Call::Submit(id));
            Ok(())
        }

        fn cancel(&mut self, id: PlaybackId) {
            self.calls.push(Call::Cancel(id));
        }

        fn cancel_with_fade(&mut self, id: PlaybackId, fade_samples: usize) {
            self.calls.push(Call::CancelWithFade(id, fade_samples));
        }
    }

    fn request(class: AudioClass, rank: u32, samples: usize) -> Request {
        Request {
            instance_id: format!("test_{rank}"),
            event_type: Some("hero_kill".to_owned()),
            session_epoch: 0,
            class,
            ordinary_rank: rank,
            episode: (class == AudioClass::Critical).then(|| "episode".to_owned()),
            package_revision: (class != AudioClass::Voice).then_some(0),
            pcm: Pcm::from(vec![1_000; samples].into_boxed_slice()),
            visual_tail_ms: 0,
        }
    }

    fn owner() -> AudioOwner<TestSink> {
        AudioOwner::new(TestSink::default(), Config::default())
    }

    #[test]
    fn higher_rank_ordinary_preempts_with_fade_and_holds_for_the_new_clip_only() {
        let mut owner = owner();
        assert_eq!(
            owner
                .request(0, request(AudioClass::Ordinary, 60, 48_000))
                .unwrap()
                .outcome,
            Outcome::Admitted
        );

        let admitted = owner
            .request(400, request(AudioClass::Ordinary, 90, 480))
            .unwrap();
        assert_eq!(admitted.outcome, Outcome::Admitted);
        assert_eq!(admitted.playback_id, Some(PlaybackId(2)));
        assert_eq!(PREEMPT_FADE_MS, 100);
        assert_eq!(PREEMPT_FADE_SAMPLES, 4_800);
        assert_eq!(
            owner.sink().calls,
            [
                Call::Submit(PlaybackId(1)),
                Call::CancelWithFade(PlaybackId(1), 4_800),
                Call::Submit(PlaybackId(2)),
            ]
        );
        assert_eq!(
            owner.drain_notices(),
            [Notice::Cancelled {
                id: PlaybackId(1),
                reason: CancelReason::Preempted,
                at_ms: 400,
            }]
        );
        assert_eq!(owner.active(409), Some(PlaybackId(2)));
        assert_eq!(owner.active(410), None);
    }

    #[test]
    fn equal_or_lower_rank_ordinary_requests_remain_channel_busy() {
        for incoming_rank in [60, 20] {
            let mut owner = owner();
            owner.request(0, request(AudioClass::Ordinary, 60, 48_000));

            let decision = owner
                .request(1, request(AudioClass::Ordinary, incoming_rank, 480))
                .unwrap();
            assert_eq!(decision.outcome, Outcome::ChannelBusy);
            assert_eq!(owner.sink().calls, [Call::Submit(PlaybackId(1))]);
            assert!(owner.drain_notices().is_empty());
        }
    }

    #[test]
    fn ordinary_requests_keep_their_c1_c2_queue_and_c3_busy_rules() {
        for active_class in [
            AudioClass::Critical,
            AudioClass::Correction,
            AudioClass::Notice,
        ] {
            let mut owner = owner();
            assert_eq!(
                owner
                    .request(0, request(active_class, 0, 48_000))
                    .unwrap()
                    .outcome,
                Outcome::Admitted
            );

            let incoming = owner.request(1, request(AudioClass::Ordinary, 90, 480));
            if matches!(active_class, AudioClass::Critical | AudioClass::Correction) {
                assert!(
                    incoming.is_none(),
                    "C4 stays queued behind {active_class:?}"
                );
            } else {
                assert_eq!(incoming.unwrap().outcome, Outcome::ChannelBusy);
            }
            assert_eq!(owner.sink().calls, [Call::Submit(PlaybackId(1))]);
        }
    }

    #[test]
    fn critical_request_fades_an_active_ordinary_and_starts_immediately() {
        let mut owner = owner();
        owner.request(0, request(AudioClass::Ordinary, 60, 48_000));
        assert_eq!(
            owner
                .request(1, request(AudioClass::Ordinary, 90, 48_000))
                .unwrap()
                .outcome,
            Outcome::Admitted
        );
        owner.drain_notices();

        let critical = owner
            .request(2, request(AudioClass::Critical, 0, 480))
            .unwrap();
        assert_eq!(critical.outcome, Outcome::Admitted);
        assert_eq!(critical.playback_id, Some(PlaybackId(3)));
        assert_eq!(owner.active(2), Some(PlaybackId(3)));
        assert_eq!(
            owner.sink().calls,
            [
                Call::Submit(PlaybackId(1)),
                Call::CancelWithFade(PlaybackId(1), PREEMPT_FADE_SAMPLES),
                Call::Submit(PlaybackId(2)),
                Call::CancelWithFade(PlaybackId(2), PREEMPT_FADE_SAMPLES),
                Call::Submit(PlaybackId(3)),
            ]
        );
        assert_eq!(
            owner.drain_notices(),
            [Notice::Cancelled {
                id: PlaybackId(2),
                reason: CancelReason::Preempted,
                at_ms: 2,
            }]
        );
    }

    #[test]
    fn critical_correction_and_notice_preempt_ordinary_with_the_same_immediate_fade() {
        for incoming_class in [
            AudioClass::Critical,
            AudioClass::Correction,
            AudioClass::Notice,
        ] {
            let mut owner = owner();
            owner.request(0, request(AudioClass::Ordinary, 60, 48_000));

            let admitted = owner.request(2, request(incoming_class, 0, 480)).unwrap();
            assert_eq!(admitted.outcome, Outcome::Admitted);
            assert_eq!(admitted.playback_id, Some(PlaybackId(2)));
            assert_eq!(owner.active(2), Some(PlaybackId(2)));
            assert_eq!(owner.active(11), Some(PlaybackId(2)));
            assert_eq!(owner.active(12), None);
            assert_eq!(
                owner.sink().calls,
                [
                    Call::Submit(PlaybackId(1)),
                    Call::CancelWithFade(PlaybackId(1), PREEMPT_FADE_SAMPLES),
                    Call::Submit(PlaybackId(2)),
                ],
                "{incoming_class:?} over C4 uses one immediate-start fade"
            );
        }
    }

    #[test]
    fn critical_over_a_different_critical_episode_keeps_the_hard_cut() {
        let mut owner = owner();
        owner.request(0, request(AudioClass::Critical, 0, 48_000));
        let mut incoming = request(AudioClass::Critical, 0, 480);
        incoming.episode = Some("another-episode".to_owned());

        assert_eq!(
            owner.request(2, incoming).unwrap().outcome,
            Outcome::Admitted
        );
        assert_eq!(
            owner.sink().calls,
            [
                Call::Submit(PlaybackId(1)),
                Call::Cancel(PlaybackId(1)),
                Call::Submit(PlaybackId(2)),
            ]
        );
    }

    #[test]
    fn preemptions_of_voice_remain_hard_cuts() {
        for incoming_class in [
            AudioClass::Critical,
            AudioClass::Correction,
            AudioClass::Notice,
            AudioClass::Ordinary,
            AudioClass::Voice,
        ] {
            let mut owner = owner();
            owner.request(0, request(AudioClass::Voice, 0, 48_000));

            assert_eq!(
                owner
                    .request(1, request(incoming_class, 90, 480))
                    .unwrap()
                    .outcome,
                Outcome::Admitted,
                "{incoming_class:?} preempts C5"
            );
            assert_eq!(
                owner.sink().calls,
                [
                    Call::Submit(PlaybackId(1)),
                    Call::Cancel(PlaybackId(1)),
                    Call::Submit(PlaybackId(2)),
                ],
                "{incoming_class:?} over C5 remains a hard cut"
            );
        }
    }

    #[test]
    fn voice_priority_keeps_its_existing_hard_cancel_rules() {
        let mut ordinary_active = owner();
        ordinary_active.request(0, request(AudioClass::Ordinary, 60, 48_000));
        assert_eq!(
            ordinary_active
                .request(1, request(AudioClass::Voice, 0, 480))
                .unwrap()
                .outcome,
            Outcome::ChannelBusy
        );
        assert_eq!(ordinary_active.sink().calls, [Call::Submit(PlaybackId(1))]);

        let mut voice_active = owner();
        voice_active.request(0, request(AudioClass::Voice, 0, 48_000));
        assert_eq!(
            voice_active
                .request(1, request(AudioClass::Ordinary, 60, 480))
                .unwrap()
                .outcome,
            Outcome::Admitted
        );
        assert_eq!(
            voice_active.sink().calls,
            [
                Call::Submit(PlaybackId(1)),
                Call::Cancel(PlaybackId(1)),
                Call::Submit(PlaybackId(2))
            ]
        );
    }
}
