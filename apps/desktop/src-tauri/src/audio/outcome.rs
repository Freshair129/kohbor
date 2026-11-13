//! Contract records for audio outcomes (audio-outcome 1.0.0) and the renderer
//! interface I1 payloads (design sections 5 and 7). API STUB until S3-5d.
//! Pure: no I/O, no lock, no panic on any input.

#![allow(dead_code, unused_variables)]

use kohbor_audio::{AudioClass, CancelReason, Decision, Outcome, PlaybackId};
use serde_json::Value;

use super::resolve::Clip;

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

fn valid_instance_id(value: &str) -> bool {
    (8..=96).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn valid_event_type(value: &str) -> bool {
    if value.is_empty() || value.len() > 80 {
        return false;
    }

    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_revision(value: Option<u64>) -> bool {
    value.is_none_or(|revision| (1..=MAX_SAFE_INTEGER).contains(&revision))
}

struct RecordData<'a> {
    instance_id: &'a str,
    event_type: Option<&'a str>,
    session_epoch: u64,
    class: AudioClass,
    outcome: Outcome,
    playback_id: Option<PlaybackId>,
    package_revision: Option<u64>,
    decided_at_ms: u64,
}

fn build_record(data: RecordData<'_>) -> Option<Value> {
    let RecordData {
        instance_id,
        event_type,
        session_epoch,
        class,
        outcome,
        playback_id,
        package_revision,
        decided_at_ms,
    } = data;

    if !valid_instance_id(instance_id)
        || session_epoch == 0
        || session_epoch > MAX_SAFE_INTEGER
        || decided_at_ms > MAX_SAFE_INTEGER
    {
        return None;
    }

    let is_voice = class == AudioClass::Voice;
    if !is_voice
        && (!event_type.is_some_and(valid_event_type)
            || !valid_revision(package_revision)
            || (outcome == Outcome::Admitted && package_revision.is_none()))
    {
        return None;
    }

    let requires_playback = matches!(outcome, Outcome::Admitted | Outcome::Preempted);
    if requires_playback != playback_id.is_some() {
        return None;
    }

    let normalized_event_type = if is_voice { None } else { event_type };
    let normalized_revision = if is_voice { None } else { package_revision };

    Some(serde_json::json!({
        "schema_version": "1.0.0",
        "instance_id": instance_id,
        "event_type": normalized_event_type,
        "session_epoch": session_epoch,
        "audio_class": class.as_str(),
        "outcome": outcome.as_str(),
        "playback_id": playback_id.map(|id| id.to_contract_id()),
        "package_revision": normalized_revision,
        "decided_monotonic_ms": decided_at_ms,
    }))
}

/// audio-outcome 1.0.0 record of an owner `Decision`: `schema_version` "1.0.0",
/// `instance_id`, `event_type`, `session_epoch`, `audio_class` (`class.as_str()`),
/// `outcome` (`outcome.as_str()`), `playback_id` (`pb_0000000001` form or null),
/// `package_revision`, `decided_monotonic_ms` = `decided_at_ms`. `None` when the
/// record would break the contract: `session_epoch == 0`, an `instance_id` outside
/// `^[A-Za-z0-9_-]{8,96}$`, an `event_type` outside `^[a-z][a-z0-9_]{0,79}$`,
/// `package_revision == Some(0)`, or `package_revision` null for a non-voice
/// `Admitted` decision. Voice records carry null `event_type` and null `package_revision`.
pub fn record(decision: &Decision) -> Option<Value> {
    build_record(RecordData {
        instance_id: &decision.instance_id,
        event_type: decision.event_type.as_deref(),
        session_epoch: decision.session_epoch,
        class: decision.class,
        outcome: decision.outcome,
        playback_id: decision.playback_id,
        package_revision: decision.package_revision,
        decided_at_ms: decision.decided_at_ms,
    })
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
    build_record(RecordData {
        instance_id,
        event_type,
        session_epoch,
        class,
        outcome,
        playback_id: None,
        package_revision,
        decided_at_ms,
    })
}

/// The second record of a preemption: identity fields of the OLD playback's stored
/// decision, `outcome` "preempted", its `playback_id`, `decided_monotonic_ms` =
/// `at_ms` (the cancellation instant). `None` under the same validity rules.
pub fn preempted_record(old: &Decision, at_ms: u64) -> Option<Value> {
    build_record(RecordData {
        instance_id: &old.instance_id,
        event_type: old.event_type.as_deref(),
        session_epoch: old.session_epoch,
        class: old.class,
        outcome: Outcome::Preempted,
        playback_id: old.playback_id,
        package_revision: old.package_revision,
        decided_at_ms: at_ms,
    })
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
        match self {
            Self::Finished => "finished",
            Self::Cancelled(CancelReason::Preempted) => "cancelled_preempted",
            Self::Cancelled(CancelReason::SessionFence) => "cancelled_session",
            Self::Cancelled(CancelReason::RevisionFence) => "cancelled_revision",
            Self::Cancelled(CancelReason::Muted) => "cancelled_muted",
            Self::Cancelled(CancelReason::DeviceLost) => "cancelled_device_lost",
        }
    }
}

/// I1 `started`: `{ kind: "started", playback_id, class, event_type, tier, visual_url,
/// audio_duration_ms, visual_tail_ms, at_ms }`. `tier` = `clip.bound_tier`,
/// `visual_url` = `clip.visual_url`, `audio_duration_ms` = `clip.audio_ms`,
/// `visual_tail_ms` = `min(clip.visual_tail_ms, 3200)`, `at_ms` = `decision.decided_at_ms`,
/// `playback_id` = the decision's id in contract form. `None` when the decision has no
/// playback id.
pub fn playback_started(decision: &Decision, clip: &Clip) -> Option<Value> {
    let playback_id = decision.playback_id?;
    let is_voice = decision.class == AudioClass::Voice;
    let event_type = if is_voice {
        None
    } else {
        decision.event_type.as_deref()
    };
    if event_type.is_some_and(|value| !valid_event_type(value)) {
        return None;
    }

    Some(serde_json::json!({
        "kind": "started",
        "playback_id": playback_id.to_contract_id(),
        "class": decision.class.as_str(),
        "event_type": event_type,
        "tier": clip.bound_tier.map(u64::from),
        "visual_url": clip.visual_url.as_deref(),
        "audio_duration_ms": u64::from(clip.audio_ms),
        "visual_tail_ms": u64::from(clip.visual_tail_ms.min(3200)),
        "at_ms": decision.decided_at_ms,
    }))
}

/// I1 `ended`: `{ kind: "ended", playback_id, reason, at_ms }`.
pub fn playback_ended(id: PlaybackId, reason: EndReason, at_ms: u64) -> Value {
    serde_json::json!({
        "kind": "ended",
        "playback_id": id.to_contract_id(),
        "reason": reason.as_str(),
        "at_ms": at_ms,
    })
}
