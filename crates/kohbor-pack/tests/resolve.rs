mod common;
use common::*;
use kohbor_pack::{
    error::RejectReason,
    limits::Limits,
    store::{ActivationPolicy, ActivationState, ResolveError, Store},
};

#[test]
fn only_active_revision_resolves_and_switch_retains_old() {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    assert_eq!(
        store.resolve("first_blood", None),
        Err(ResolveError::Unmapped)
    );
    let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let old = store
        .install(&request(path, &bytes, "request_1"))
        .unwrap()
        .record
        .unwrap();
    assert_eq!(
        store.resolve("first_blood", None),
        Err(ResolveError::Unmapped)
    );
    store
        .activate(
            "request_1",
            &old.install_id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    let pair = store.resolve("first_blood", None).unwrap();
    assert!(pair
        .mixed_audio
        .starts_with(root.join("revisions").join(&old.install_id)));
    assert!(pair.mixed_audio.is_file());
    assert!(pair
        .silent_visual
        .as_ref()
        .is_some_and(|path| path.is_file()));
    let (path, bytes) = store_package(&temp.0, "test-pack", 2, false);
    let new = store
        .install(&request(path, &bytes, "request_2"))
        .unwrap()
        .record
        .unwrap();
    assert_eq!(
        store.resolve("first_blood", None).unwrap().install_id,
        old.install_id
    );
    store
        .activate(
            "request_2",
            &new.install_id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    assert_eq!(
        store.resolve("first_blood", None).unwrap().install_id,
        new.install_id
    );
    assert!(root.join("revisions").join(old.install_id).is_dir());
}

#[test]
fn damaged_pair_is_never_returned() {
    for damage in [0, 1, 2] {
        let temp = TempDir::new();
        let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
        let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
        let id = store
            .install(&request(path, &bytes, "request_1"))
            .unwrap()
            .record
            .unwrap()
            .install_id;
        store
            .activate(
                "request_1",
                &id,
                ActivationPolicy::NowIfIdle,
                &ActivationState::default(),
            )
            .unwrap();
        let pair = store.resolve("first_blood", None).unwrap();
        match damage {
            0 => {
                let mut data = std::fs::read(&pair.mixed_audio).unwrap();
                data[50] ^= 1;
                std::fs::write(pair.mixed_audio, data).unwrap();
            }
            1 => std::fs::remove_file(pair.silent_visual.unwrap()).unwrap(),
            _ => std::fs::write(pair.mixed_audio, b"short").unwrap(),
        }
        let result = store.resolve("first_blood", None);
        match damage {
            0 => assert_eq!(
                result,
                Err(ResolveError::Invalid(RejectReason::DigestMismatch))
            ),
            2 => assert_eq!(
                result,
                Err(ResolveError::Invalid(RejectReason::SizeMismatch))
            ),
            _ => assert!(matches!(result, Err(ResolveError::Invalid(_)))),
        }
    }
}

fn tier_case(
    event: &str,
    tier: Option<u32>,
    expected_path: Option<&str>,
    error: Option<ResolveError>,
) {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let (path, bytes) = tiered_package(&temp.0);
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let record = store
        .install(&request(path, &bytes, "request_1"))
        .unwrap()
        .record
        .unwrap();
    store
        .activate(
            "request_1",
            &record.install_id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    let result = store.resolve(event, tier);
    if let Some(error) = error {
        assert_eq!(
            result,
            Err(error),
            "{event} {tier:?}: expected resolution error"
        );
    } else {
        let pair = result.unwrap_or_else(|e| panic!("{event} {tier:?}: {e:?}"));
        let revision = root.join("revisions").join(record.install_id);
        assert_eq!(pair.event_id, event, "{event} {tier:?}: event");
        assert_eq!(pair.tier, tier, "{event} {tier:?}: tier");
        assert!(
            pair.mixed_audio.starts_with(&revision),
            "{event} {tier:?}: audio path outside revision"
        );
        let visual = pair.silent_visual.as_ref();
        assert!(
            visual.is_some_and(|path| path.starts_with(&revision)),
            "{event} {tier:?}: visual path outside revision"
        );
        assert!(
            pair.mixed_audio.is_file() && visual.is_some_and(|path| path.is_file()),
            "{event} {tier:?}: missing paired media"
        );
        assert!(
            pair.mixed_audio.ends_with(expected_path.unwrap()),
            "{event} {tier:?}: wrong binding"
        );
        assert!(
            !pair.mixed_audio.ends_with("identity.wav"),
            "{event} {tier:?}: identity audio substituted"
        );
    }
}

// Exact-tier behavior is a lead proposal, pending owner approval.
#[test]
fn resolves_multi_kill_tier_2() {
    tier_case("multi_kill", Some(2), Some("audio/cue1.wav"), None);
}
#[test]
fn resolves_multi_kill_tier_3() {
    tier_case("multi_kill", Some(3), Some("audio/cue2.wav"), None);
}
#[test]
fn unmapped_multi_kill_tier_4() {
    tier_case("multi_kill", Some(4), None, Some(ResolveError::Unmapped));
}
#[test]
fn unmapped_multi_kill_no_tier() {
    tier_case("multi_kill", None, None, Some(ResolveError::Unmapped));
}
#[test]
fn resolves_streak_tierless() {
    tier_case("streak", None, Some("audio/cue3.wav"), None);
}
#[test]
fn streak_tier_does_not_fall_back() {
    tier_case("streak", Some(3), None, Some(ResolveError::Unmapped));
}
#[test]
fn unknown_bound_event_is_inert() {
    tier_case("future_event", None, None, Some(ResolveError::Inert));
}
#[test]
fn known_unbound_event_is_unmapped() {
    tier_case("first_blood", None, None, Some(ResolveError::Unmapped));
}

#[test]
fn valid_identity_audio_is_never_substituted_for_corrupt_mix() {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let (path, bytes) = tiered_package(&temp.0);
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let record = store
        .install(&request(path, &bytes, "request_1"))
        .unwrap()
        .record
        .unwrap();
    store
        .activate(
            "request_1",
            &record.install_id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    let pair = store.resolve("multi_kill", Some(2)).unwrap();
    let identity = root
        .join("revisions")
        .join(record.install_id)
        .join("audio/identity.wav");
    let identity_before = std::fs::read(&identity).unwrap();
    let mut mixed = std::fs::read(&pair.mixed_audio).unwrap();
    mixed[50] ^= 1;
    std::fs::write(&pair.mixed_audio, mixed).unwrap();
    assert_eq!(
        std::fs::read(identity).unwrap(),
        identity_before,
        "identity remains valid"
    );
    assert_eq!(
        store.resolve("multi_kill", Some(2)),
        Err(ResolveError::Invalid(RejectReason::DigestMismatch)),
        "corrupt mix rejects entire pair"
    );
}
