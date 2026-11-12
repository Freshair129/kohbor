//! Critical media readiness (MP-08, ALERT-007). Never a vacuous "ready".

/// One available critical (C1/C2) event of the active revision and what is known
/// about its media.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticalMedia {
    pub event_id: String,
    /// A binding exists in the active revision.
    pub mapped: bool,
    /// The pair passed verification (size, SHA-256, profile).
    pub verified: bool,
    /// The audio is decoded to PCM in memory.
    pub prewarmed: bool,
    /// The renderer decode probe of the visual succeeded (EVENT-AUDIO-SPEC section 6).
    pub probe_ok: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessInput {
    /// Critical events whose detector is available in this runtime AND that the
    /// catalog marks `critical_media_required`. Empty means none exists.
    pub available_critical: Vec<CriticalMedia>,
    pub muted: bool,
    /// The output device exists.
    pub device_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessState {
    NotReady,
    Ready,
    Unavailable,
}

/// `state` and `reason` as in runtime-status `media`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Readiness {
    pub state: ReadinessState,
    /// Contract reason code, `None` only for `Ready`.
    pub reason: Option<&'static str>,
}

impl ReadinessState {
    /// `not_ready`, `ready`, `unavailable`.
    pub fn as_str(&self) -> &'static str {
        todo!()
    }
}

/// First matching rule wins:
/// 1. `muted` -> `Unavailable`, `AUDIO_MUTED`;
/// 2. `!device_available` -> `Unavailable`, `CAPABILITY_UNAVAILABLE`;
/// 3. no available critical event -> `NotReady`, `MEDIA_UNMAPPED` (never vacuous ready);
/// 4. any event not `mapped` -> `NotReady`, `MEDIA_UNMAPPED`;
/// 5. any event mapped but not `verified` -> `NotReady`, `MEDIA_INVALID`;
/// 6. any verified event not `prewarmed` -> `NotReady`, `MEDIA_INVALID`;
/// 7. any event whose `probe_ok` is `None` or `Some(false)` -> `NotReady`,
///    `MEDIA_PROBE_FAILED`;
/// 8. otherwise `Ready`, reason `None`.
///
/// Rules 4 to 7 are evaluated over all events in that order (a missing mapping
/// anywhere beats an invalid file elsewhere). Duplicate event ids are judged
/// independently (all must pass).
pub fn assess(input: &ReadinessInput) -> Readiness {
    todo!()
}
