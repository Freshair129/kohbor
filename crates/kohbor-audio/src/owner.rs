//! One audio owner: class priority, preemption, generations, mute, fences.

use crate::pcm::Pcm;

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
        todo!()
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
        todo!()
    }
}

/// Opaque playback identity; each admitted playback gets a new one (its
/// generation), starting at 1 and never reused. `to_contract_id` is `pb_` plus 10
/// decimal digits (`pb_0000000001`), matching the `playback_id` pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlaybackId(pub u64);

impl PlaybackId {
    pub fn to_contract_id(&self) -> String {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkError;

/// The output device behind the owner. Implemented by the real device layer and by
/// a fake in tests. The owner never blocks on it and never takes a lock another
/// thread needs.
pub trait Sink {
    /// Start `pcm` (mono 48 kHz) as `id`. An error means the device did not accept it.
    fn submit(&mut self, id: PlaybackId, pcm: Pcm) -> Result<(), SinkError>;
    /// Stop `id` now and drop its audio (no-op for an unknown or finished id).
    fn cancel(&mut self, id: PlaybackId);
}

/// One request to play a verified, prewarmed clip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Per-occurrence identity (event `instance_id`).
    pub instance_id: String,
    pub event_type: Option<String>,
    pub session_epoch: u64,
    pub class: AudioClass,
    /// Same value for repeated alerts of one critical episode (C1 same-episode drop).
    pub episode: Option<String>,
    /// Active package revision the media came from (`install_id` number). `None`
    /// only for voice (C5), which has no package (audio-outcome: voice records
    /// have a null `event_type` and a null `package_revision`).
    pub package_revision: Option<u64>,
    pub pcm: Pcm,
    /// Visual tail after natural audio end, 0..=3,200 ms.
    pub visual_tail_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// The sink refused `submit`; the request was not admitted.
    SinkFailed,
    /// A queued C2 request waited longer than the TTL while critical audio played.
    QueueExpired,
    /// A queued C2 request was replaced in the queue by a newer C2 request.
    QueueReplaced,
    /// The device is marked unavailable.
    DeviceUnavailable,
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
    },
    /// Audio (and the visual tail) of `id` ended naturally.
    Finished { id: PlaybackId },
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
    /// Minimum gap between two admitted C3 notices (default 10,000 ms).
    pub notice_cooldown_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            correction_queue_ttl_ms: 3_000,
            notice_cooldown_ms: 10_000,
        }
    }
}

pub struct AudioOwner<S: Sink> {
    _sink: S,
    _config: Config,
}

impl<S: Sink> AudioOwner<S> {
    pub fn new(sink: S, config: Config) -> AudioOwner<S> {
        todo!()
    }

    /// Decide one request at `at_ms`. Check order (first match wins):
    /// 1. mute on -> `Muted`;
    /// 2. `session_epoch` differs from the current session -> `StaleSession`;
    /// 3. for classes other than voice: `package_revision` is `None` or differs from
    ///    the current revision -> `SupersededRevision` (voice never has a revision
    ///    and skips this check);
    /// 4. device marked unavailable -> `ChannelBusy` with `Detail::DeviceUnavailable`;
    /// 5. the priority matrix of EVENT-AUDIO-SPEC section 4 against the class that
    ///    holds the audio channel at `at_ms` (a playback holds it from admission
    ///    until `admission + duration_ms(pcm)`; a visual tail does not hold the
    ///    audio channel):
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
    ///    - C4: C1 or C2 active -> `CriticalActive`; C3 or C4 active -> `ChannelBusy`;
    ///      C5 active -> preempt; idle -> play.
    ///    - C5: any of C1..C4 active -> `ChannelBusy` (the host waits and
    ///      re-submits; the owner never queues voice); C5 active -> preempt
    ///      (supersede); idle -> play.
    /// 6. On play or preempt: when preempting, `sink.cancel(old)` first and queue
    ///    `Notice::Cancelled{Preempted}`; then `sink.submit`. A `SinkError` gives
    ///    `ChannelBusy` with `Detail::SinkFailed` and nothing becomes active.
    ///
    /// Returns `Some(Decision)` for every request except a queued C2 (decided
    /// later through `Notice::Decided`). A request that plays, on an idle channel
    /// or by preempting, is `Admitted`; the `preempted` outcome of the contract
    /// belongs to the OLD playback and is derived by the host from the
    /// `Notice::Cancelled { reason: Preempted }` the owner emits for it.
    pub fn request(&mut self, at_ms: u64, request: Request) -> Option<Decision> {
        todo!()
    }

    /// Advance time: audio that ended is finished (`Notice::Finished` once the
    /// audio and the declared visual tail are over); a queued C2 starts when the
    /// critical audio ended (`Decided(Admitted)`); a queued C2 older than the TTL
    /// expires (`Decided(CriticalActive, QueueExpired)`).
    pub fn tick(&mut self, at_ms: u64) {
        todo!()
    }

    /// The sink reports that `id` finished (device callback). Ignored for an id
    /// that is not the current playback (late completion of a cancelled one).
    pub fn on_sink_finished(&mut self, at_ms: u64, id: PlaybackId) {
        todo!()
    }

    /// Take the pending notices in order of occurrence.
    pub fn drain_notices(&mut self) -> Vec<Notice> {
        todo!()
    }

    /// Global mute: on cancels the active playback (`Muted`) and the queue; later
    /// requests get `Muted`. Off restores nothing.
    pub fn set_muted(&mut self, at_ms: u64, muted: bool) {
        todo!()
    }

    /// New current session: cancels the active playback and the queue when their
    /// epoch differs (`SessionFence`). The initial session is 0 (no request with
    /// epoch 0 is stale before `set_session` is called).
    pub fn set_session(&mut self, at_ms: u64, session_epoch: u64) {
        todo!()
    }

    /// New current package revision: cancels the active non-voice playback and the
    /// queue when their revision differs (`RevisionFence`); a voice playback is not
    /// affected. The initial revision is 0.
    pub fn set_revision(&mut self, at_ms: u64, package_revision: u64) {
        todo!()
    }

    /// The output device disappeared or came back. `false` cancels the active
    /// playback (`DeviceLost`) and the queue; requests then get `ChannelBusy` with
    /// `Detail::DeviceUnavailable` until it is `true` again. Initially available.
    pub fn set_device_available(&mut self, at_ms: u64, available: bool) {
        todo!()
    }

    /// The playback that holds the audio channel at `at_ms`, if any.
    pub fn active(&self, at_ms: u64) -> Option<PlaybackId> {
        todo!()
    }

    /// Class of the playback that holds the audio channel at `at_ms`, if any.
    pub fn active_class(&self, at_ms: u64) -> Option<AudioClass> {
        todo!()
    }

    /// The visual that must be on screen at `at_ms` (audio phase or tail), if any.
    /// Cancellation of audio cancels this at the same instant.
    pub fn visual(&self, at_ms: u64) -> Option<VisualState> {
        todo!()
    }

    pub fn sink(&self) -> &S {
        todo!()
    }
}
