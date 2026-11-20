//! Contract records for audio outcomes (audio-outcome 1.1.0) and the renderer
//! interface I1 payloads (design sections 5 and 7). API STUB until S3-5d.
//! Pure: no I/O, no lock, no panic on any input.

#![allow(dead_code, unused_variables)]

use kohbor_audio::{AudioClass, CancelReason, Decision, Outcome, PlaybackId};
use serde_json::Value;

use super::resolve::Clip;

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub const OUTCOME_SCHEMA_VERSION: &str = "1.1.0";
pub const OUTCOME_KEYS: [&str; 16] = [
    "schema_version",
    "instance_id",
    "event_type",
    "session_epoch",
    "audio_class",
    "outcome",
    "playback_id",
    "package_revision",
    "decided_monotonic_ms",
    "tier",
    "variant_id",
    "clip_id",
    "selection",
    "game_clock_seconds",
    "match_id",
    "playback_end",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackEnd {
    Finished,
    Cut,
    Failed,
}

impl PlaybackEnd {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::Cut => "cut",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutcomeDetails {
    pub tier: Option<u8>,
    pub variant_id: Option<String>,
    pub clip_id: Option<String>,
    pub selection: Option<String>,
    pub game_clock_seconds: Option<u64>,
}

/// Derive an opaque, local match key from the persisted local session epoch.
/// The raw match identifier from GSI is deliberately never copied into history.
pub fn local_match_id(session_epoch: u64) -> Option<String> {
    (session_epoch > 0 && session_epoch <= MAX_SAFE_INTEGER)
        .then(|| format!("local_match_{session_epoch}"))
}

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

fn valid_variant_id(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'_' | b'-'))
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn valid_clip_id(value: &str) -> bool {
    (1..=48).contains(&value.len())
}

fn valid_selection(value: &str) -> bool {
    matches!(value, "random" | "shuffle" | "sequence")
}

fn valid_details(details: &OutcomeDetails) -> bool {
    details.tier.is_none_or(|tier| (1..=10).contains(&tier))
        && details.variant_id.as_deref().is_none_or(valid_variant_id)
        && details.clip_id.as_deref().is_none_or(valid_clip_id)
        && details.selection.as_deref().is_none_or(valid_selection)
        && details
            .game_clock_seconds
            .is_none_or(|clock| clock <= MAX_SAFE_INTEGER)
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
    details: &'a OutcomeDetails,
    playback_end: Option<PlaybackEnd>,
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
        details,
        playback_end,
    } = data;

    if !valid_instance_id(instance_id)
        || session_epoch == 0
        || session_epoch > MAX_SAFE_INTEGER
        || decided_at_ms > MAX_SAFE_INTEGER
        || !valid_details(details)
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
    let local_match_id = if is_voice {
        None
    } else {
        local_match_id(session_epoch)
    };
    if !is_voice && local_match_id.is_none() {
        return None;
    }
    let details = if is_voice {
        OutcomeDetails::default()
    } else {
        details.clone()
    };
    if outcome == Outcome::Preempted && playback_end != Some(PlaybackEnd::Cut) {
        return None;
    }
    if !requires_playback && playback_end.is_some() {
        return None;
    }

    Some(serde_json::json!({
        "schema_version": OUTCOME_SCHEMA_VERSION,
        "instance_id": instance_id,
        "event_type": normalized_event_type,
        "session_epoch": session_epoch,
        "audio_class": class.as_str(),
        "outcome": outcome.as_str(),
        "playback_id": playback_id.map(|id| id.to_contract_id()),
        "package_revision": normalized_revision,
        "decided_monotonic_ms": decided_at_ms,
        "tier": details.tier,
        "variant_id": details.variant_id,
        "clip_id": details.clip_id,
        "selection": details.selection,
        "game_clock_seconds": details.game_clock_seconds,
        "match_id": local_match_id,
        "playback_end": playback_end.map(PlaybackEnd::as_str),
    }))
}

/// audio-outcome 1.1.0 record of an owner `Decision`: `schema_version` "1.1.0",
/// `instance_id`, `event_type`, `session_epoch`, `audio_class` (`class.as_str()`),
/// `outcome` (`outcome.as_str()`), `playback_id` (`pb_0000000001` form or null),
/// `package_revision`, `decided_monotonic_ms` = `decided_at_ms`. `None` when the
/// record would break the contract: `session_epoch == 0`, an `instance_id` outside
/// `^[A-Za-z0-9_-]{8,96}$`, an `event_type` outside `^[a-z][a-z0-9_]{0,79}$`,
/// `package_revision == Some(0)`, or `package_revision` null for a non-voice
/// `Admitted` decision. Voice records carry null match and announcer provenance.
pub fn record(
    decision: &Decision,
    details: &OutcomeDetails,
    playback_end: Option<PlaybackEnd>,
) -> Option<Value> {
    build_record(RecordData {
        instance_id: &decision.instance_id,
        event_type: decision.event_type.as_deref(),
        session_epoch: decision.session_epoch,
        class: decision.class,
        outcome: decision.outcome,
        playback_id: decision.playback_id,
        package_revision: decision.package_revision,
        decided_at_ms: decision.decided_at_ms,
        details,
        playback_end,
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
    details: &OutcomeDetails,
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
        details,
        playback_end: None,
    })
}

/// The second record of a preemption: identity fields of the OLD playback's stored
/// decision, `outcome` "preempted", its `playback_id`, `decided_monotonic_ms` =
/// `at_ms` (the cancellation instant). `None` under the same validity rules.
pub fn preempted_record(old: &Decision, at_ms: u64, details: &OutcomeDetails) -> Option<Value> {
    build_record(RecordData {
        instance_id: &old.instance_id,
        event_type: old.event_type.as_deref(),
        session_epoch: old.session_epoch,
        class: old.class,
        outcome: Outcome::Preempted,
        playback_id: old.playback_id,
        package_revision: old.package_revision,
        decided_at_ms: at_ms,
        details,
        playback_end: Some(PlaybackEnd::Cut),
    })
}

/// Strict 1.1.0 shape validation used before exposing history or retaining it on disk.
pub fn is_valid_record(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.len() != OUTCOME_KEYS.len()
        || !OUTCOME_KEYS.iter().all(|key| object.contains_key(*key))
        || object.get("schema_version").and_then(Value::as_str) != Some(OUTCOME_SCHEMA_VERSION)
    {
        return false;
    }

    let Some(instance_id) = object.get("instance_id").and_then(Value::as_str) else {
        return false;
    };
    let Some(session_epoch) = object.get("session_epoch").and_then(Value::as_u64) else {
        return false;
    };
    let Some(class) = object.get("audio_class").and_then(Value::as_str) else {
        return false;
    };
    let Some(outcome) = object.get("outcome").and_then(Value::as_str) else {
        return false;
    };
    let Some(decided_at_ms) = object.get("decided_monotonic_ms").and_then(Value::as_u64) else {
        return false;
    };
    let null = Value::Null;
    let event_type = object.get("event_type").unwrap_or(&null);
    let playback_id = object.get("playback_id").unwrap_or(&null);
    let package_revision = object.get("package_revision").unwrap_or(&null);
    let tier = object.get("tier").unwrap_or(&null);
    let variant_id = object.get("variant_id").unwrap_or(&null);
    let clip_id = object.get("clip_id").unwrap_or(&null);
    let selection = object.get("selection").unwrap_or(&null);
    let game_clock = object.get("game_clock_seconds").unwrap_or(&null);
    let match_id = object.get("match_id").unwrap_or(&null);
    let playback_end = object.get("playback_end").unwrap_or(&null);

    if !valid_instance_id(instance_id)
        || session_epoch == 0
        || session_epoch > MAX_SAFE_INTEGER
        || decided_at_ms > MAX_SAFE_INTEGER
        || !matches!(
            class,
            "critical" | "correction" | "notice" | "ordinary" | "voice"
        )
        || !matches!(
            outcome,
            "admitted"
                | "preempted"
                | "unmapped"
                | "invalid_media"
                | "inert"
                | "not_voiced"
                | "detector_unavailable"
                | "stale_session"
                | "superseded_revision"
                | "critical_active"
                | "channel_busy"
                | "lower_rank_same_tick"
                | "cooldown"
                | "muted"
        )
    {
        return false;
    }

    let playing = matches!(outcome, "admitted" | "preempted");
    let valid_playback_id = playback_id.as_str().is_some_and(|id| {
        (8..=64).contains(&id.len())
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    });
    if (playing && !valid_playback_id) || (!playing && !playback_id.is_null()) {
        return false;
    }
    if !package_revision.is_null()
        && package_revision
            .as_u64()
            .is_none_or(|revision| !(1..=MAX_SAFE_INTEGER).contains(&revision))
    {
        return false;
    }
    if outcome == "admitted" && class != "voice" && package_revision.is_null() {
        return false;
    }
    if let Some(value) = event_type.as_str() {
        if !valid_event_type(value) || class == "voice" {
            return false;
        }
    } else if !event_type.is_null() || class != "voice" {
        return false;
    }
    if class == "voice" {
        if !package_revision.is_null()
            || !match_id.is_null()
            || !tier.is_null()
            || !variant_id.is_null()
            || !clip_id.is_null()
            || !selection.is_null()
            || !game_clock.is_null()
        {
            return false;
        }
    } else {
        let Some(value) = match_id.as_str() else {
            return false;
        };
        if local_match_id(session_epoch).as_deref() != Some(value) {
            return false;
        }
    }
    if !tier.is_null() && tier.as_u64().is_none_or(|value| !(1..=10).contains(&value)) {
        return false;
    }
    if !variant_id.is_null()
        && variant_id
            .as_str()
            .is_none_or(|value| !valid_variant_id(value))
    {
        return false;
    }
    if !clip_id.is_null() && clip_id.as_str().is_none_or(|value| !valid_clip_id(value)) {
        return false;
    }
    if !selection.is_null()
        && selection
            .as_str()
            .is_none_or(|value| !valid_selection(value))
    {
        return false;
    }
    if !game_clock.is_null()
        && game_clock
            .as_u64()
            .is_none_or(|value| value > MAX_SAFE_INTEGER)
    {
        return false;
    }
    if !playback_end.is_null()
        && !matches!(playback_end.as_str(), Some("finished" | "cut" | "failed"))
    {
        return false;
    }
    if outcome == "preempted" && playback_end.as_str() != Some("cut") {
        return false;
    }
    if !playing && !playback_end.is_null() {
        return false;
    }
    true
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
