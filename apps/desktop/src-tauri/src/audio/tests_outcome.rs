use super::{
    outcome::{
        playback_ended, playback_started, pre_owner_record, preempted_record, record, EndReason,
    },
    resolve::Clip,
};
use kohbor_audio::{AudioClass, CancelReason, Decision, Outcome, PlaybackId};
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::Arc};

const CLASSES: [AudioClass; 5] = [
    AudioClass::Critical,
    AudioClass::Correction,
    AudioClass::Notice,
    AudioClass::Ordinary,
    AudioClass::Voice,
];
const OUTCOMES: [Outcome; 14] = [
    Outcome::Admitted,
    Outcome::Preempted,
    Outcome::Unmapped,
    Outcome::InvalidMedia,
    Outcome::Inert,
    Outcome::NotVoiced,
    Outcome::DetectorUnavailable,
    Outcome::StaleSession,
    Outcome::SupersededRevision,
    Outcome::CriticalActive,
    Outcome::ChannelBusy,
    Outcome::LowerRankSameTick,
    Outcome::Cooldown,
    Outcome::Muted,
];
const ENDS: [(EndReason, &str); 6] = [
    (EndReason::Finished, "finished"),
    (
        EndReason::Cancelled(CancelReason::Preempted),
        "cancelled_preempted",
    ),
    (
        EndReason::Cancelled(CancelReason::SessionFence),
        "cancelled_session",
    ),
    (
        EndReason::Cancelled(CancelReason::RevisionFence),
        "cancelled_revision",
    ),
    (EndReason::Cancelled(CancelReason::Muted), "cancelled_muted"),
    (
        EndReason::Cancelled(CancelReason::DeviceLost),
        "cancelled_device_lost",
    ),
];
fn decision(class: AudioClass, outcome: Outcome) -> Decision {
    Decision {
        instance_id: "ev_g3_s120_0".into(),
        event_type: (class != AudioClass::Voice).then(|| "health_danger".into()),
        session_epoch: 3,
        class,
        outcome,
        playback_id: matches!(outcome, Outcome::Admitted | Outcome::Preempted)
            .then_some(PlaybackId(1)),
        package_revision: (class != AudioClass::Voice).then_some(7),
        decided_at_ms: 1234,
        detail: None,
    }
}
fn clip(tail: u32) -> Clip {
    Clip {
        pcm: Arc::from([1_i16, 2]),
        bound_tier: Some(3),
        audio_ms: 42,
        visual_tail_ms: tail,
        visual_url: Some(Arc::from(format!(
            "kohbor-media://localhost/v1/ins_0000000001/{}",
            "a".repeat(64)
        ))),
    }
}
fn id_ok(s: &str, max: usize) -> bool {
    (8..=max).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn event_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .skip(1)
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
fn keys(v: &Value) -> BTreeSet<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}
fn check_record(v: &Value) {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../../contracts/audio-outcome.schema.json"
    ))
    .unwrap();
    let required: BTreeSet<_> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(keys(v), required, "record keys");
    assert_eq!(
        v["schema_version"],
        schema["properties"]["schema_version"]["const"]
    );
    assert!(id_ok(v["instance_id"].as_str().unwrap(), 96));
    for field in ["outcome", "audio_class"] {
        assert!(
            schema["properties"][field]["enum"]
                .as_array()
                .unwrap()
                .contains(&v[field]),
            "{field}: {v}"
        );
    }
    let max = 9_007_199_254_740_991_u64;
    assert!((1..=max).contains(&v["session_epoch"].as_u64().unwrap()));
    assert!(v["decided_monotonic_ms"].as_u64().is_some_and(|n| n <= max));
    assert!(
        v["package_revision"].is_null()
            || v["package_revision"]
                .as_u64()
                .is_some_and(|n| (1..=max).contains(&n))
    );
    let needs_id = matches!(v["outcome"].as_str().unwrap(), "admitted" | "preempted");
    if needs_id {
        assert!(id_ok(v["playback_id"].as_str().unwrap(), 64));
    } else {
        assert!(v["playback_id"].is_null());
    }
    if v["audio_class"] == "voice" {
        assert!(v["event_type"].is_null());
        assert!(v["package_revision"].is_null());
    } else {
        assert!(event_ok(v["event_type"].as_str().unwrap()));
        if v["outcome"] == "admitted" {
            assert!(v["package_revision"].is_u64());
        }
    }
}
// Models the current renderer parser, including its closed returned shape.
fn renderer_accepts(v: &Value) -> bool {
    let Some(_) = v.as_object() else {
        return false;
    };
    let Some(id) = v["playback_id"].as_str() else {
        return false;
    };
    if !id_ok(id, 64) || v["at_ms"].as_u64().is_none() {
        return false;
    }
    match v["kind"].as_str() {
        Some("ended") => {
            keys(v)
                == ["kind", "playback_id", "reason", "at_ms"]
                    .into_iter()
                    .collect()
                && v["reason"]
                    .as_str()
                    .is_some_and(|s| ENDS.iter().any(|(_, name)| *name == s))
        }
        Some("started") => {
            keys(v)
                == [
                    "kind",
                    "playback_id",
                    "class",
                    "event_type",
                    "tier",
                    "visual_url",
                    "audio_duration_ms",
                    "visual_tail_ms",
                    "at_ms",
                ]
                .into_iter()
                .collect()
                && CLASSES.iter().any(|c| v["class"] == c.as_str())
                && (v["event_type"].is_null() || v["event_type"].as_str().is_some_and(event_ok))
                && (v["tier"].is_null() || v["tier"].as_u64().is_some())
                && (v["visual_url"].is_null()
                    || v["visual_url"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("kohbor-media://")))
                && v["audio_duration_ms"].as_u64().is_some()
                && v["visual_tail_ms"].as_u64().is_some()
        }
        _ => false,
    }
}
#[test]
fn all_decision_outcomes_and_classes_conform() {
    for class in CLASSES {
        for outcome in OUTCOMES {
            let d = decision(class, outcome);
            let v = record(&d).unwrap_or_else(|| panic!("missing {class:?} {outcome:?}"));
            check_record(&v);
            assert_eq!(v["audio_class"], class.as_str(), "{class:?} {outcome:?}");
            assert_eq!(v["outcome"], outcome.as_str(), "{class:?} {outcome:?}");
        }
    }
}
#[test]
fn exact_field_mapping() {
    let d = decision(AudioClass::Critical, Outcome::Admitted);
    let v = record(&d).unwrap();
    check_record(&v);
    assert_eq!(
        v,
        json!({"schema_version":"1.0.0", "instance_id":"ev_g3_s120_0", "event_type":"health_danger", "session_epoch":3, "audio_class":"critical", "outcome":"admitted", "playback_id":"pb_0000000001", "package_revision":7, "decided_monotonic_ms":1234})
    );
}
#[test]
fn larger_playback_id_mapping() {
    let mut d = decision(AudioClass::Ordinary, Outcome::Admitted);
    d.playback_id = Some(PlaybackId(12_345_678_901));
    let v = record(&d).unwrap();
    check_record(&v);
    assert_eq!(v["playback_id"], "pb_12345678901");
}
// The owner normalizes voice. For malformed manually constructed voice decisions the
// stub promises null fields but does not say whether rejection is also allowed.
#[test]
fn normalized_voice_is_valid() {
    let v = record(&decision(AudioClass::Voice, Outcome::Admitted)).unwrap();
    check_record(&v);
    assert!(v["event_type"].is_null());
    assert!(v["package_revision"].is_null());
}
#[test]
fn non_normalized_voice_is_normalized_or_rejected() {
    let mut d = decision(AudioClass::Voice, Outcome::Admitted);
    d.event_type = Some("health_danger".into());
    d.package_revision = Some(7);
    if let Some(v) = record(&d) {
        check_record(&v);
        assert!(v["event_type"].is_null());
        assert!(v["package_revision"].is_null());
    }
}
macro_rules! rejects {
    ($name:ident, $change:expr) => {
        #[test]
        fn $name() {
            let mut d = decision(AudioClass::Ordinary, Outcome::Admitted);
            ($change)(&mut d);
            assert!(record(&d).is_none(), stringify!($name));
        }
    };
}
rejects!(reject_epoch_zero, |d: &mut Decision| d.session_epoch = 0);
rejects!(reject_instance_short, |d: &mut Decision| d.instance_id =
    "a".repeat(7));
rejects!(reject_instance_long, |d: &mut Decision| d.instance_id =
    "a".repeat(97));
rejects!(reject_instance_space, |d: &mut Decision| d.instance_id =
    "bad id 123".into());
rejects!(reject_event_uppercase, |d: &mut Decision| d.event_type =
    Some("Bad".into()));
rejects!(reject_event_long, |d: &mut Decision| d.event_type =
    Some("a".repeat(81)));
rejects!(reject_event_empty, |d: &mut Decision| d.event_type =
    Some(String::new()));
rejects!(reject_zero_revision, |d: &mut Decision| d
    .package_revision =
    Some(0));
rejects!(reject_missing_admitted_revision, |d: &mut Decision| d
    .package_revision =
    None);
#[test]
fn pre_owner_outcomes_are_valid() {
    for outcome in [
        Outcome::Unmapped,
        Outcome::InvalidMedia,
        Outcome::Inert,
        Outcome::NotVoiced,
        Outcome::DetectorUnavailable,
        Outcome::LowerRankSameTick,
        Outcome::StaleSession,
    ] {
        let v = pre_owner_record(
            "ev_g3_s120_0",
            Some("hero_kill"),
            3,
            AudioClass::Ordinary,
            outcome,
            Some(7),
            1234,
        )
        .unwrap();
        check_record(&v);
        assert_eq!(v["outcome"], outcome.as_str(), "{outcome:?}");
        assert!(v["playback_id"].is_null());
    }
}
#[test]
fn pre_owner_rejects_invalid_identity() {
    assert!(pre_owner_record(
        "bad",
        Some("hero_kill"),
        3,
        AudioClass::Ordinary,
        Outcome::Unmapped,
        Some(7),
        0
    )
    .is_none());
}
#[test]
fn preemption_uses_old_identity_and_new_time() {
    let d = decision(AudioClass::Critical, Outcome::Admitted);
    let v = preempted_record(&d, 9999).unwrap();
    check_record(&v);
    assert_eq!(
        v,
        json!({"schema_version":"1.0.0", "instance_id":"ev_g3_s120_0", "event_type":"health_danger", "session_epoch":3, "audio_class":"critical", "outcome":"preempted", "playback_id":"pb_0000000001", "package_revision":7, "decided_monotonic_ms":9999})
    );
}
#[test]
fn preemption_without_playback_is_none() {
    assert!(preempted_record(&decision(AudioClass::Critical, Outcome::Muted), 3).is_none());
}
#[test]
fn end_reason_names() {
    for (reason, name) in ENDS {
        assert_eq!(reason.as_str(), name, "{reason:?}");
    }
}
#[test]
fn started_exact_shape_values_and_tail_clamp() {
    for (tail, expected) in [(3200, 3200), (3201, 3200), (u32::MAX, 3200)] {
        let d = decision(AudioClass::Critical, Outcome::Admitted);
        let v = playback_started(&d, &clip(tail)).unwrap();
        assert_eq!(
            v,
            json!({"kind":"started", "playback_id":"pb_0000000001", "class":"critical", "event_type":"health_danger", "tier":3, "visual_url":format!("kohbor-media://localhost/v1/ins_0000000001/{}", "a".repeat(64)), "audio_duration_ms":42, "visual_tail_ms":expected, "at_ms":1234}),
            "tail {tail}"
        );
        assert!(renderer_accepts(&v), "tail {tail}");
    }
}
#[test]
fn started_without_playback_is_none() {
    assert!(playback_started(&decision(AudioClass::Ordinary, Outcome::Muted), &clip(0)).is_none());
}
#[test]
fn ended_exact_shape_for_each_reason() {
    for (reason, name) in ENDS {
        let v = playback_ended(PlaybackId(1), reason, 1234);
        assert_eq!(
            v,
            json!({"kind":"ended", "playback_id":"pb_0000000001", "reason":name, "at_ms":1234}),
            "{reason:?}"
        );
        assert!(renderer_accepts(&v), "{reason:?}");
    }
}
#[test]
fn started_renderer_accepts_all_classes_and_nulls() {
    for class in CLASSES {
        let d = decision(class, Outcome::Admitted);
        let mut c = clip(0);
        c.visual_url = None;
        c.bound_tier = None;
        let v = playback_started(&d, &c).unwrap();
        assert!(renderer_accepts(&v), "{class:?}: {v}");
    }
}

// The schema bounds all integers to 2^53-1; the stub's example list is not exhaustive.
rejects!(reject_epoch_over_schema_max, |d: &mut Decision| d
    .session_epoch =
    9_007_199_254_740_992);
rejects!(reject_revision_over_schema_max, |d: &mut Decision| d
    .package_revision =
    Some(9_007_199_254_740_992));
rejects!(reject_time_over_schema_max, |d: &mut Decision| d
    .decided_at_ms =
    9_007_199_254_740_992);
rejects!(reject_missing_playback_for_admitted, |d: &mut Decision| d
    .playback_id =
    None);
rejects!(reject_missing_non_voice_event, |d: &mut Decision| d
    .event_type =
    None);
#[test]
fn reject_playback_for_non_admitted_outcome() {
    let mut d = decision(AudioClass::Ordinary, Outcome::Muted);
    d.playback_id = Some(PlaybackId(1));
    assert!(record(&d).is_none());
}
#[test]
fn pre_owner_rejects_invalid_fields() {
    for (id, event, epoch, revision) in [
        ("ev_g3_s120_0", Some("hero_kill"), 0, Some(7)),
        ("ev_g3_s120_0", Some("Bad"), 1, Some(7)),
        ("ev_g3_s120_0", None, 1, Some(7)),
        ("ev_g3_s120_0", Some("hero_kill"), 1, Some(0)),
    ] {
        assert!(
            pre_owner_record(
                id,
                event,
                epoch,
                AudioClass::Ordinary,
                Outcome::Unmapped,
                revision,
                0
            )
            .is_none(),
            "{id} {event:?} {epoch} {revision:?}"
        );
    }
}
#[test]
fn renderer_event_type_pattern_currently_rejects_uppercase_and_long_ids() {
    // playback_started accepts arbitrary Decision.event_type; the renderer currently
    // rejects uppercase and >80 characters. S3-5k owns widening its event_type rule.
    let mut v = json!({"kind":"started", "playback_id":"pb_0000000001", "class":"ordinary", "event_type":"Bad", "tier":null, "visual_url":null, "audio_duration_ms":0, "visual_tail_ms":0, "at_ms":0});
    assert!(!renderer_accepts(&v));
    v["event_type"] = json!("a".repeat(81));
    assert!(!renderer_accepts(&v));
}

// Each contract combination has its own test so one failure does not mask the rest.
macro_rules! decision_case {
    ($name:ident, $class:ident, $outcome:ident) => {
        #[test]
        fn $name() {
            let d = decision(AudioClass::$class, Outcome::$outcome);
            let v = record(&d).expect(stringify!($name));
            check_record(&v);
            assert_eq!(v["audio_class"], d.class.as_str(), stringify!($name));
            assert_eq!(v["outcome"], d.outcome.as_str(), stringify!($name));
        }
    };
}
decision_case!(record_critical_admitted, Critical, Admitted);
decision_case!(record_critical_preempted, Critical, Preempted);
decision_case!(record_critical_unmapped, Critical, Unmapped);
decision_case!(record_critical_invalid_media, Critical, InvalidMedia);
decision_case!(record_critical_inert, Critical, Inert);
decision_case!(record_critical_not_voiced, Critical, NotVoiced);
decision_case!(
    record_critical_detector_unavailable,
    Critical,
    DetectorUnavailable
);
decision_case!(record_critical_stale_session, Critical, StaleSession);
decision_case!(
    record_critical_superseded_revision,
    Critical,
    SupersededRevision
);
decision_case!(record_critical_critical_active, Critical, CriticalActive);
decision_case!(record_critical_channel_busy, Critical, ChannelBusy);
decision_case!(
    record_critical_lower_rank_same_tick,
    Critical,
    LowerRankSameTick
);
decision_case!(record_critical_cooldown, Critical, Cooldown);
decision_case!(record_critical_muted, Critical, Muted);
decision_case!(record_correction_admitted, Correction, Admitted);
decision_case!(record_correction_preempted, Correction, Preempted);
decision_case!(record_correction_unmapped, Correction, Unmapped);
decision_case!(record_correction_invalid_media, Correction, InvalidMedia);
decision_case!(record_correction_inert, Correction, Inert);
decision_case!(record_correction_not_voiced, Correction, NotVoiced);
decision_case!(
    record_correction_detector_unavailable,
    Correction,
    DetectorUnavailable
);
decision_case!(record_correction_stale_session, Correction, StaleSession);
decision_case!(
    record_correction_superseded_revision,
    Correction,
    SupersededRevision
);
decision_case!(
    record_correction_critical_active,
    Correction,
    CriticalActive
);
decision_case!(record_correction_channel_busy, Correction, ChannelBusy);
decision_case!(
    record_correction_lower_rank_same_tick,
    Correction,
    LowerRankSameTick
);
decision_case!(record_correction_cooldown, Correction, Cooldown);
decision_case!(record_correction_muted, Correction, Muted);
decision_case!(record_notice_admitted, Notice, Admitted);
decision_case!(record_notice_preempted, Notice, Preempted);
decision_case!(record_notice_unmapped, Notice, Unmapped);
decision_case!(record_notice_invalid_media, Notice, InvalidMedia);
decision_case!(record_notice_inert, Notice, Inert);
decision_case!(record_notice_not_voiced, Notice, NotVoiced);
decision_case!(
    record_notice_detector_unavailable,
    Notice,
    DetectorUnavailable
);
decision_case!(record_notice_stale_session, Notice, StaleSession);
decision_case!(
    record_notice_superseded_revision,
    Notice,
    SupersededRevision
);
decision_case!(record_notice_critical_active, Notice, CriticalActive);
decision_case!(record_notice_channel_busy, Notice, ChannelBusy);
decision_case!(
    record_notice_lower_rank_same_tick,
    Notice,
    LowerRankSameTick
);
decision_case!(record_notice_cooldown, Notice, Cooldown);
decision_case!(record_notice_muted, Notice, Muted);
decision_case!(record_ordinary_admitted, Ordinary, Admitted);
decision_case!(record_ordinary_preempted, Ordinary, Preempted);
decision_case!(record_ordinary_unmapped, Ordinary, Unmapped);
decision_case!(record_ordinary_invalid_media, Ordinary, InvalidMedia);
decision_case!(record_ordinary_inert, Ordinary, Inert);
decision_case!(record_ordinary_not_voiced, Ordinary, NotVoiced);
decision_case!(
    record_ordinary_detector_unavailable,
    Ordinary,
    DetectorUnavailable
);
decision_case!(record_ordinary_stale_session, Ordinary, StaleSession);
decision_case!(
    record_ordinary_superseded_revision,
    Ordinary,
    SupersededRevision
);
decision_case!(record_ordinary_critical_active, Ordinary, CriticalActive);
decision_case!(record_ordinary_channel_busy, Ordinary, ChannelBusy);
decision_case!(
    record_ordinary_lower_rank_same_tick,
    Ordinary,
    LowerRankSameTick
);
decision_case!(record_ordinary_cooldown, Ordinary, Cooldown);
decision_case!(record_ordinary_muted, Ordinary, Muted);
decision_case!(record_voice_admitted, Voice, Admitted);
decision_case!(record_voice_preempted, Voice, Preempted);
decision_case!(record_voice_unmapped, Voice, Unmapped);
decision_case!(record_voice_invalid_media, Voice, InvalidMedia);
decision_case!(record_voice_inert, Voice, Inert);
decision_case!(record_voice_not_voiced, Voice, NotVoiced);
decision_case!(
    record_voice_detector_unavailable,
    Voice,
    DetectorUnavailable
);
decision_case!(record_voice_stale_session, Voice, StaleSession);
decision_case!(record_voice_superseded_revision, Voice, SupersededRevision);
decision_case!(record_voice_critical_active, Voice, CriticalActive);
decision_case!(record_voice_channel_busy, Voice, ChannelBusy);
decision_case!(record_voice_lower_rank_same_tick, Voice, LowerRankSameTick);
decision_case!(record_voice_cooldown, Voice, Cooldown);
decision_case!(record_voice_muted, Voice, Muted);
