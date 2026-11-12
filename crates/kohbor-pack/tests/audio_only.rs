mod common;

use common::*;
use kohbor_pack::{
    container::stage_package,
    descriptor::parse_descriptor,
    error::RejectReason,
    store::{ActivationPolicy, ActivationState, ResolveError, Status, Store},
    Limits,
};
use serde_json::{json, Value};
use std::fs;

fn parse(value: &Value) -> Result<kohbor_pack::descriptor::ParsedDescriptor, RejectReason> {
    parse_descriptor(
        &serde_json::to_vec(value).unwrap(),
        &catalog(),
        &Limits::default(),
    )
}

fn stage_entries(
    entries: &[ZipEntry],
) -> Result<kohbor_pack::container::StagedPackage, RejectReason> {
    let temp = TempDir::new();
    let (package, _) = write_package_entries(&temp.0, "test.zip", entries);
    let staging = temp.0.join("staging");
    fs::create_dir(&staging).unwrap();
    stage_package(&package, &staging, &catalog(), &Limits::default())
}

#[test]
fn descriptor_audio_only_fixture_and_tiered_pair_are_valid() {
    let parsed = parse_descriptor(
        &fixture("media-audio-only-valid.json"),
        &catalog(),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(parsed.descriptor.bindings.len(), 2);
    assert_eq!(parsed.descriptor.bindings[0].event_id, "hero_kill");
    assert_eq!(parsed.descriptor.bindings[0].silent_visual_path, None);
    assert_eq!(parsed.descriptor.bindings[0].visual_tail_ms, 0);
    assert_eq!(parsed.descriptor.bindings[1].event_id, "multi_kill");
    assert_eq!(
        parsed.descriptor.bindings[1].silent_visual_path.as_deref(),
        Some("video/cue2.webm")
    );
    assert_eq!(parsed.descriptor.bindings[1].tier, Some(2));
}

#[test]
fn descriptor_audio_only_tail_is_rejected_at_visual_tail_pointer() {
    assert_eq!(
        parse_descriptor(
            &fixture("media-audio-only-tail.json"),
            &catalog(),
            &Limits::default(),
        )
        .unwrap_err(),
        RejectReason::DescriptorSchema("/bindings/0/visual_tail_ms".to_owned())
    );
}

#[test]
fn descriptor_rejects_unreleased_120_and_still_checks_present_visuals() {
    let mut value = common::descriptor();
    value["schema_version"] = json!("1.2.0");
    assert_eq!(
        parse(&value).unwrap_err(),
        RejectReason::DescriptorSchema("/schema_version".to_owned())
    );

    let mut role_mismatch = common::descriptor();
    role_mismatch["bindings"][0]["silent_visual_path"] = json!("audio/cue1.wav");
    assert_eq!(
        parse(&role_mismatch).unwrap_err(),
        RejectReason::BindingRoleMismatch
    );

    let mut dangling = common::descriptor();
    dangling["bindings"][0]["silent_visual_path"] = json!("video/missing.webm");
    assert_eq!(
        parse(&dangling).unwrap_err(),
        RejectReason::DanglingReference
    );

    let mut unsafe_path = common::descriptor();
    unsafe_path["bindings"][0]["silent_visual_path"] = json!("video/../escape.webm");
    assert_eq!(parse(&unsafe_path).unwrap_err(), RejectReason::UnsafePath);
}

#[test]
fn unused_audio_and_visual_inventory_entries_remain_unpaired_errors() {
    let mut unused_visual = common::descriptor();
    unused_visual["files"].as_array_mut().unwrap().push(json!({
        "path": "video/unused.webm",
        "size_bytes": 1,
        "sha256": "0".repeat(64),
        "role": "silent_visual"
    }));
    assert_eq!(
        parse(&unused_visual).unwrap_err(),
        RejectReason::UnpairedBinding
    );

    let mut unused_audio = common::descriptor();
    unused_audio["files"].as_array_mut().unwrap().push(json!({
        "path": "audio/unused.wav",
        "size_bytes": 1,
        "sha256": "0".repeat(64),
        "role": "mixed_audio"
    }));
    assert_eq!(
        parse(&unused_audio).unwrap_err(),
        RejectReason::UnpairedBinding
    );
}

#[test]
fn audio_only_package_stages_and_mixed_package_verifies_both_binding_kinds() {
    let audio_only = audio_only_entries(wav(48_000), 1_000, None);
    let staged = stage_entries(&audio_only).unwrap();
    assert_eq!(staged.parsed.descriptor.bindings.len(), 1);
    assert_eq!(
        staged.parsed.descriptor.bindings[0].silent_visual_path,
        None
    );

    let mixed = mixed_audio_visual_entries();
    let staged = stage_entries(&mixed).unwrap();
    assert_eq!(staged.parsed.descriptor.bindings.len(), 2);
    assert_eq!(
        staged.parsed.descriptor.bindings[0].silent_visual_path,
        None
    );
    assert!(staged.parsed.descriptor.bindings[1]
        .silent_visual_path
        .is_some());
}

#[test]
fn audio_only_package_staging_rejects_bad_audio_duration_and_identity_length() {
    let invalid_audio = audio_only_entries(b"not a wav".to_vec(), 1_000, None);
    assert_eq!(
        stage_entries(&invalid_audio).unwrap_err(),
        RejectReason::MixedAudioInvalid
    );

    let mut corrupt_audio = audio_only_entries(wav(48_000), 1_000, None);
    let audio = corrupt_audio
        .iter_mut()
        .find(|entry| entry.name == "audio/cue.wav")
        .unwrap();
    audio.bytes[44] ^= 1;
    assert_eq!(
        stage_entries(&corrupt_audio).unwrap_err(),
        RejectReason::DigestMismatch
    );

    let wrong_duration = audio_only_entries(wav(48_000), 900, None);
    assert_eq!(
        stage_entries(&wrong_duration).unwrap_err(),
        RejectReason::AudioDurationMismatch
    );

    let wrong_identity_length = audio_only_entries(wav(48_000), 1_000, Some(wav(24_000)));
    assert_eq!(
        stage_entries(&wrong_identity_length).unwrap_err(),
        RejectReason::IdentityAudioLengthMismatch
    );
}

#[test]
fn store_resolves_audio_only_and_rejects_corruption_per_binding() {
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let entries = mixed_audio_visual_entries();
    let (package, bytes) = write_package_entries(&temp.0, "mixed.zip", &entries);
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let installed = store
        .install(&request(package, &bytes, "audio_only_request"))
        .unwrap();
    assert_eq!(installed.status, Status::Installed);
    let record = installed.record.unwrap();
    assert_eq!(
        store
            .activate(
                "audio_only_request",
                &record.install_id,
                ActivationPolicy::NowIfIdle,
                &ActivationState::default(),
            )
            .unwrap()
            .status,
        Status::Active
    );

    let audio_only = store.resolve("first_blood", None).unwrap();
    assert_eq!(audio_only.silent_visual, None);
    let revision_root = root.join("revisions").join(&record.install_id);
    assert!(audio_only.mixed_audio.starts_with(&revision_root));
    let mut audio_bytes = fs::read(&audio_only.mixed_audio).unwrap();
    audio_bytes[44] ^= 1;
    fs::write(&audio_only.mixed_audio, &audio_bytes).unwrap();
    assert_eq!(
        store.resolve("first_blood", None),
        Err(ResolveError::Invalid(RejectReason::DigestMismatch))
    );

    let paired = store.resolve("multi_kill", Some(2)).unwrap();
    assert!(paired.silent_visual.is_some());
    assert!(paired.mixed_audio.starts_with(&revision_root));
    let audio_entry = entries
        .iter()
        .find(|entry| entry.name == "audio/cue1.wav")
        .unwrap();
    fs::write(&audio_only.mixed_audio, &audio_entry.bytes).unwrap();
    assert!(store.resolve("first_blood", None).is_ok());

    let visual_path = paired.silent_visual.as_ref().unwrap();
    let mut visual_bytes = fs::read(visual_path).unwrap();
    let last = visual_bytes.len().checked_sub(1).unwrap();
    visual_bytes[last] ^= 1;
    fs::write(visual_path, &visual_bytes).unwrap();
    assert_eq!(
        store.resolve("multi_kill", Some(2)),
        Err(ResolveError::Invalid(RejectReason::DigestMismatch))
    );
    assert!(store.resolve("first_blood", None).is_ok());
}
