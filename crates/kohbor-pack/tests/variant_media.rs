mod common;

use common::{catalog, hash, wav, TempDir, ZipEntry};
use kohbor_pack::{
    container::stage_package,
    error::RejectReason,
    limits::Limits,
    store::{ActivationPolicy, ActivationState, Store},
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn variant_package(
    second_mixed: Vec<u8>,
    second_identity: Vec<u8>,
    second_duration_ms: u32,
) -> (TempDir, PathBuf, PathBuf) {
    let first_mixed = wav(48_000);
    let first_identity = wav(48_000);
    let payloads = vec![
        ("audio/a.wav", "mixed_audio", first_mixed),
        ("audio/b.wav", "mixed_audio", second_mixed),
        ("identity/a.wav", "identity_audio", first_identity),
        ("identity/b.wav", "identity_audio", second_identity),
    ];
    let files = payloads
        .iter()
        .map(|(path, role, bytes)| {
            json!({
                "path": path,
                "size_bytes": bytes.len(),
                "sha256": hash(bytes),
                "role": role
            })
        })
        .collect::<Vec<Value>>();
    let descriptor = json!({
        "schema_version": "1.4.0",
        "package_id": "synthetic-variants-pack",
        "revision": 1,
        "taxonomy_version": "1.1",
        "display_name": "Synthetic media verification",
        "publisher_key_id": null,
        "files": files,
        "bindings": [{
            "event_id": "first_blood",
            "selection": "shuffle",
            "variants": [
                {
                    "variant_id": "a",
                    "mixed_audio_path": "audio/a.wav",
                    "identity_audio_path": "identity/a.wav",
                    "duration_ms": 1000,
                    "visual_tail_ms": 0
                },
                {
                    "variant_id": "b",
                    "mixed_audio_path": "audio/b.wav",
                    "identity_audio_path": "identity/b.wav",
                    "duration_ms": second_duration_ms,
                    "visual_tail_ms": 0
                }
            ]
        }]
    });
    let mut entries = vec![ZipEntry::new(
        "descriptor.json",
        serde_json::to_vec(&descriptor).unwrap(),
        0,
    )];
    entries.extend(
        payloads
            .into_iter()
            .map(|(path, _, bytes)| ZipEntry::new(path, bytes, 0)),
    );
    let archive = common::zip(&entries, &[], &[], &[]);
    let temp = TempDir::new();
    let source = temp.0.join("variants.zip");
    std::fs::write(&source, archive).unwrap();
    let staging = temp.0.join("staging");
    std::fs::create_dir(&staging).unwrap();
    (temp, source, staging)
}

fn verify(source: &Path, staging: &Path) -> Result<(), RejectReason> {
    stage_package(source, staging, &catalog(), &Limits::default()).map(|_| ())
}

#[test]
fn verifies_media_in_the_second_variant() {
    let (_temp, source, staging) = variant_package(b"invalid wav".to_vec(), wav(48_000), 1000);
    assert_eq!(
        verify(&source, &staging).unwrap_err(),
        RejectReason::MixedAudioInvalid
    );
}

#[test]
fn checks_non_first_identity_audio_against_its_own_mixed_audio() {
    let (_temp, source, staging) = variant_package(wav(48_100), wav(48_000), 1002);
    assert_eq!(
        verify(&source, &staging).unwrap_err(),
        RejectReason::IdentityAudioLengthMismatch
    );
}

#[test]
fn resolves_all_variants_while_resolve_keeps_returning_the_first() {
    let (temp, source, _staging) = variant_package(wav(48_100), wav(48_100), 1002);
    let archive = std::fs::read(&source).unwrap();
    let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    let record = store
        .install(&common::request(source, &archive, "request_v14"))
        .unwrap()
        .record
        .unwrap();
    store
        .activate(
            "request_v14",
            &record.install_id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();

    let variants = store.resolve_variants("first_blood", None).unwrap();
    assert_eq!(variants.len(), 2);
    assert_eq!(variants[0].variant_id, "a");
    assert_eq!(variants[1].variant_id, "b");
    assert!(variants
        .iter()
        .all(|variant| variant.selection == Some(kohbor_pack::descriptor::Selection::Shuffle)));
    assert_eq!(store.resolve("first_blood", None).unwrap(), variants[0]);
}
