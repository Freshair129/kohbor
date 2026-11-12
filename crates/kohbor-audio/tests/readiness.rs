mod common;
use kohbor_audio::{assess, CriticalMedia, Readiness, ReadinessInput, ReadinessState::*};
fn event() -> CriticalMedia {
    CriticalMedia {
        event_id: "event".into(),
        mapped: true,
        verified: true,
        prewarmed: true,
        probe_ok: Some(true),
    }
}
fn input() -> ReadinessInput {
    ReadinessInput {
        available_critical: vec![event()],
        muted: false,
        device_available: true,
    }
}
fn check(i: ReadinessInput, state: kohbor_audio::ReadinessState, reason: Option<&'static str>) {
    assert_eq!(assess(&i), Readiness { state, reason });
}
#[test]
fn rule_1_muted() {
    let mut i = input();
    i.muted = true;
    check(i, Unavailable, Some("AUDIO_MUTED"));
}
#[test]
fn rule_2_device() {
    let mut i = input();
    i.device_available = false;
    check(i, Unavailable, Some("CAPABILITY_UNAVAILABLE"));
}
#[test]
fn rule_3_empty() {
    let mut i = input();
    i.available_critical.clear();
    check(i, NotReady, Some("MEDIA_UNMAPPED"));
}
#[test]
fn rule_4_unmapped() {
    let mut i = input();
    i.available_critical[0].mapped = false;
    check(i, NotReady, Some("MEDIA_UNMAPPED"));
}
#[test]
fn rule_5_invalid() {
    let mut i = input();
    i.available_critical[0].verified = false;
    check(i, NotReady, Some("MEDIA_INVALID"));
}
#[test]
fn rule_6_not_prewarmed() {
    let mut i = input();
    i.available_critical[0].prewarmed = false;
    check(i, NotReady, Some("MEDIA_INVALID"));
}
#[test]
fn rule_7_probe_false() {
    let mut i = input();
    i.available_critical[0].probe_ok = Some(false);
    check(i, NotReady, Some("MEDIA_PROBE_FAILED"));
}
#[test]
fn rule_7_probe_missing() {
    let mut i = input();
    i.available_critical[0].probe_ok = None;
    check(i, NotReady, Some("MEDIA_PROBE_FAILED"));
}
#[test]
fn rule_8_ready_two_events() {
    let mut i = input();
    let mut e = event();
    e.event_id = "other".into();
    i.available_critical.push(e);
    check(i, Ready, None);
}
#[test]
fn muted_before_device() {
    let mut i = input();
    i.muted = true;
    i.device_available = false;
    check(i, Unavailable, Some("AUDIO_MUTED"));
}
#[test]
fn device_before_empty() {
    let mut i = input();
    i.device_available = false;
    i.available_critical.clear();
    check(i, Unavailable, Some("CAPABILITY_UNAVAILABLE"));
}
#[test]
fn unmapped_before_invalid_elsewhere() {
    let mut i = input();
    i.available_critical[0].verified = false;
    let mut e = event();
    e.mapped = false;
    i.available_critical.push(e);
    check(i, NotReady, Some("MEDIA_UNMAPPED"));
}
#[test]
fn invalid_before_prewarm_and_probe() {
    let mut i = input();
    i.available_critical[0].verified = false;
    i.available_critical[0].prewarmed = false;
    i.available_critical[0].probe_ok = None;
    check(i, NotReady, Some("MEDIA_INVALID"));
}
#[test]
fn prewarm_before_probe() {
    let mut i = input();
    i.available_critical[0].prewarmed = false;
    i.available_critical[0].probe_ok = None;
    check(i, NotReady, Some("MEDIA_INVALID"));
}
#[test]
fn duplicate_ids_checked_independently() {
    let mut i = input();
    let mut e = event();
    e.mapped = false;
    i.available_critical.push(e);
    check(i, NotReady, Some("MEDIA_UNMAPPED"));
}
#[test]
fn state_strings() {
    assert_eq!(
        [NotReady.as_str(), Ready.as_str(), Unavailable.as_str()],
        ["not_ready", "ready", "unavailable"]
    );
}
#[test]
fn returned_reasons_belong_to_media_enum() {
    let schema = common::schema("runtime-status.schema.json");
    let media = &schema[schema.find("\"media\": {").unwrap()..];
    let allowed = common::enum_values(media, "reason");
    for kind in 0..5 {
        let mut i = input();
        match kind {
            0 => i.muted = true,
            1 => i.device_available = false,
            2 => i.available_critical.clear(),
            3 => i.available_critical[0].verified = false,
            _ => i.available_critical[0].probe_ok = None,
        };
        let r = assess(&i);
        assert!(
            allowed.contains(&r.reason.unwrap().to_string()),
            "reason {:?}",
            r.reason
        );
    }
}
