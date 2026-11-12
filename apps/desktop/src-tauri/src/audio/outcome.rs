//! Contract records for audio outcomes (audio-outcome 1.0.0) and the renderer
//! interface I1 payloads (design sections 5 and 7). API STUB until S3-5d.
//! Pure: no I/O, no lock, no panic on any input.

#![allow(dead_code, unused_variables)]

use kohbor_audio::{AudioClass, CancelReason, Decision, Outcome, PlaybackId};
use serde_json::Value;

use super::resolve::Clip;

/// audio-outcome 1.0.0 record of an owner `Decision`: `schema_version` "1.0.0",
/// `instance_id`, `event_type`, `session_epoch`, `audio_class` (`class.as_str()`),
/// `outcome` (`outcome.as_str()`), `playback_id` (`pb_0000000001` form or null),
/// `package_revision`, `decided_monotonic_ms` = `decided_at_ms`. `None` when the
/// record would break the contract: `session_epoch == 0`, an `instance_id` outside
/// `^[A-Za-z0-9_-]{8,96}$`, an `event_type` outside `^[a-z][a-z0-9_]{0,79}$`,
/// `package_revision == Some(0)`, or `package_revision` null for a non-voice
/// `Admitted` decision. Voice records carry null `event_type` and null `package_revision`.
pub fn record(decision: &Decision) -> Option<Value> {
    todo!()
}

/// The same contract record for an outcome decided BEFORE the owner (unmapped,
/// invalid_media, inert, not_voiced, detector_unavailable, lower_rank_same_tick,
/// stale_session ...): `playback_id` is null; the same validity rules as `record`.
pub fn pre_owner_record(
    instance_id: &str,
    event_type: Option<&str>,
    session_epoch: u64,
    class: AudioClass,
    outcome: Outcome,
    package_revision: Option<u64>,
    decided_at_ms: u64,
) -> Option<Value> {
    todo!()
}

/// The second record of a preemption: identity fields of the OLD playback's stored
/// decision, `outcome` "preempted", its `playback_id`, `decided_monotonic_ms` =
/// `at_ms` (the cancellation instant). `None` under the same validity rules.
pub fn preempted_record(old: &Decision, at_ms: u64) -> Option<Value> {
    todo!()
}

/// Why a renderer visual ends (interface I1 `ended.reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Finished,
    Cancelled(CancelReason),
}

impl EndReason {
    /// `finished`, `cancelled_preempted`, `cancelled_session`, `cancelled_revision`,
    /// `cancelled_muted`, `cancelled_device_lost`.
    pub fn as_str(&self) -> &'static str {
        todo!()
    }
}

/// I1 `started`: `{ kind: "started", playback_id, class, event_type, tier, visual_url,
/// audio_duration_ms, visual_tail_ms, at_ms }`. `tier` = `clip.bound_tier`,
/// `visual_url` = `clip.visual_url`, `audio_duration_ms` = `clip.audio_ms`,
/// `visual_tail_ms` = `min(clip.visual_tail_ms, 3200)`, `at_ms` = `decision.decided_at_ms`,
/// `playback_id` = the decision's id in contract form. `None` when the decision has no
/// playback id.
pub fn playback_started(decision: &Decision, clip: &Clip) -> Option<Value> {
    todo!()
}

/// I1 `ended`: `{ kind: "ended", playback_id, reason, at_ms }`.
pub fn playback_ended(id: PlaybackId, reason: EndReason, at_ms: u64) -> Value {
    todo!()
}
