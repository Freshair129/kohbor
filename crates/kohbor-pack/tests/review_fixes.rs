mod common;

#[path = "../src/error.rs"]
#[allow(dead_code)]
mod error_helpers;
#[path = "../src/fsutil.rs"]
mod fsutil;

use common::{catalog, descriptor, hash, package_entries, request, store_package, TempDir};
use kohbor_pack::container;
use kohbor_pack::descriptor as descriptor_parser;
use kohbor_pack::error::RejectReason;
use kohbor_pack::store::{InstallRequest, Status, Store, MAX_REQUEST_RESULTS};
use kohbor_pack::Limits;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn write_zip(dir: &Path, name: &str, entries: &[common::ZipEntry]) -> (PathBuf, Vec<u8>) {
    let bytes = common::zip(entries, &[], &[], &[]);
    let path = dir.join(name);
    fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

fn stage_package_error(entries: &[common::ZipEntry]) -> RejectReason {
    let temp = TempDir::new();
    let (source, _) = write_zip(&temp.0, "candidate.zip", entries);
    let staging = temp.0.join("staging");
    fs::create_dir(&staging).unwrap();
    container::stage_package(&source, &staging, &catalog(), &Limits::default()).unwrap_err()
}

fn descriptor_path_error(path: &str, binding_path: Option<&str>) -> RejectReason {
    let mut document = descriptor();
    document["files"][0]["path"] = Value::String(path.to_owned());
    if let Some(binding_path) = binding_path {
        document["bindings"][0]["mixed_audio_path"] = Value::String(binding_path.to_owned());
    } else {
        document["bindings"][0]["mixed_audio_path"] = Value::String(path.to_owned());
    }
    descriptor_parser::parse_descriptor(
        &serde_json::to_vec(&document).unwrap(),
        &catalog(),
        &Limits::default(),
    )
    .unwrap_err()
}

fn set_profile(request: &InstallRequest, profile_id: &str) -> InstallRequest {
    let mut changed = request.clone();
    changed.profile_id = profile_id.to_owned();
    changed
}

#[test]
fn directory_flush_helpers_work_for_existing_and_missing_directories() {
    let temp = TempDir::new();
    let metadata = fs::symlink_metadata(&temp.0).unwrap();
    assert!(!fsutil::is_link_or_reparse_point(&metadata));
    assert!(fsutil::is_directory_reparse_point(&metadata));
    fsutil::flush_dir(&temp.0).unwrap();

    let missing = temp.0.join("missing");
    assert_eq!(
        fsutil::flush_dir(&missing).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn open_recovers_only_the_authorized_store_paths() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("source");
    let root = temp.0.join("store");
    fs::create_dir(&source_dir).unwrap();
    let (package, bytes) = store_package(&source_dir, "test-pack", 1, false);
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let installed = store
        .install(&request(package, &bytes, "request_kept"))
        .unwrap()
        .record
        .unwrap();
    drop(store);

    fs::create_dir(root.join("staging/leftover-dir")).unwrap();
    fs::write(root.join("staging/leftover-dir/payload"), b"stale").unwrap();
    fs::write(root.join("staging/leftover-file"), b"stale").unwrap();
    let orphan = root.join("revisions/ins_9999999999");
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("payload"), b"orphan").unwrap();
    fs::create_dir(root.join("revisions/custom-data")).unwrap();
    fs::write(root.join("revisions/custom-data/keep"), b"keep").unwrap();
    fs::create_dir(root.join("custom")).unwrap();
    fs::write(root.join("custom/user-data.tmp"), b"keep").unwrap();
    fs::write(root.join("other.tmp"), b"keep").unwrap();
    fs::write(root.join("registry.json.tmp"), b"stale").unwrap();
    fs::write(root.join("active.json.tmp"), b"stale").unwrap();

    let reopened = Store::open(&root, catalog(), Limits::default()).unwrap();
    assert!(fs::read_dir(root.join("staging")).unwrap().next().is_none());
    assert!(root.join("revisions").join(&installed.install_id).is_dir());
    assert!(!orphan.exists());
    assert!(root.join("revisions/custom-data/keep").is_file());
    assert!(root.join("custom/user-data.tmp").is_file());
    assert!(root.join("other.tmp").is_file());
    assert!(!root.join("registry.json.tmp").exists());
    assert!(!root.join("active.json.tmp").exists());
    assert_eq!(reopened.records().len(), 1);
}

#[test]
fn container_names_reject_windows_devices_trailing_dots_spaces_colons_and_long_segments() {
    for name in [
        "CON",
        "con",
        "CON.wav",
        "aux.txt",
        "COM1.wav",
        "LPT9",
        "audio/cue.wav.",
        "audio/cue.wav ",
        "audio/cue:stream.wav",
        &format!("audio/{}.wav", "x".repeat(256)),
    ] {
        let mut entries = package_entries();
        entries[1].name = name.to_owned();
        entries[1..].sort_by(|left, right| left.name.cmp(&right.name));
        assert_eq!(
            stage_package_error(&entries),
            RejectReason::UnsafePath,
            "container entry: {name:?}"
        );
    }
}

#[test]
fn descriptor_files_and_binding_paths_reject_windows_unsafe_segments() {
    for path in [
        "CON",
        "con",
        "CON.wav",
        "aux.txt",
        "COM1.wav",
        "LPT9",
        "audio/cue.wav.",
        "audio/cue.wav ",
        &format!("audio/{}.wav", "x".repeat(256)),
    ] {
        assert_eq!(
            descriptor_path_error(path, None),
            RejectReason::UnsafePath,
            "descriptor file path: {path:?}"
        );
    }
    assert_eq!(
        descriptor_path_error("audio/cue.wav", Some("dir/aux.txt")),
        RejectReason::UnsafePath,
        "descriptor binding path"
    );
    assert_eq!(
        descriptor_path_error("audio/cue:stream.wav", None),
        RejectReason::DescriptorSchema("/files/0/path".to_owned()),
        "descriptor colon remains rejected by the existing path pattern"
    );
}

#[test]
fn duplicate_names_use_ascii_case_and_trailing_dot_normalization() {
    assert_eq!(
        fsutil::windows_path_key("folder/a"),
        fsutil::windows_path_key("FOLDER/A.")
    );
    assert!(fsutil::has_unsafe_windows_segment("folder/a."));

    let mut entries = package_entries();
    entries[2].name = "AUDIO/CUE.WAV".to_owned();
    entries[1..].sort_by(|left, right| left.name.cmp(&right.name));
    assert_eq!(stage_package_error(&entries), RejectReason::DuplicateEntry);

    let mut document = descriptor();
    let first = document["files"][0]["path"].as_str().unwrap().to_owned();
    document["files"][1]["path"] = Value::String(first.to_ascii_uppercase());
    assert_eq!(
        descriptor_parser::parse_descriptor(
            &serde_json::to_vec(&document).unwrap(),
            &catalog(),
            &Limits::default(),
        )
        .unwrap_err(),
        RejectReason::DuplicateEntry
    );
}

#[test]
fn request_id_conflicts_cover_digest_profile_and_project_binding_without_mutating_result() {
    let temp = TempDir::new();
    let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let original = request(path, &bytes, "request_same");
    let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    let first = store.install(&original).unwrap();

    let mut changed_digest = original.clone();
    changed_digest.expected_container_sha256 = "f".repeat(64);
    let changed_profile = set_profile(&original, "other_profile");
    let mut changed_binding = original.clone();
    changed_binding.project_binding = Some("project_two".to_owned());

    for changed in [changed_digest, changed_profile.clone(), changed_binding] {
        let conflict = store.install(&changed).unwrap();
        assert_eq!(conflict.status, Status::Rejected);
        assert_eq!(conflict.reject, Some(RejectReason::RequestConflict));
        assert_eq!(conflict.reason.as_deref(), Some("SOURCE_INVALID"));
        assert!(conflict.record.is_none());
    }

    assert_eq!(store.install(&original).unwrap().to_json(), first.to_json());
    assert_eq!(store.records().len(), 1);
    drop(store);

    let mut reopened = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    assert_eq!(
        reopened.install(&changed_profile).unwrap().reject,
        Some(RejectReason::RequestConflict)
    );
}

#[test]
fn registry_fingerprints_persist_and_legacy_entries_remain_digest_only() {
    let temp = TempDir::new();
    let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let original = request(path, &bytes, "request_legacy");
    let root = temp.0.join("store");
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    store.install(&original).unwrap();
    drop(store);

    let registry_path = root.join("registry.json");
    let mut registry: Value = serde_json::from_slice(&fs::read(&registry_path).unwrap()).unwrap();
    assert_eq!(registry["schema_version"], "1.0.0");
    assert_eq!(
        registry["request_results"][0]["fingerprint"]["expected_container_sha256"],
        hash(&bytes)
    );
    assert_eq!(
        registry["request_results"][0]["fingerprint"]["profile_id"],
        "default"
    );
    registry["request_results"][0]
        .as_object_mut()
        .unwrap()
        .remove("fingerprint");
    fs::write(&registry_path, serde_json::to_vec(&registry).unwrap()).unwrap();

    let mut legacy_store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let legacy_retry = legacy_store
        .install(&set_profile(&original, "legacy_other"))
        .unwrap();
    assert_eq!(legacy_retry.status, Status::Installed);
}

#[test]
fn rejection_diagnostic_payloads_are_bounded_and_printable() {
    let raw = format!("a\"\\\n\t\u{7f}é{}", "x".repeat(70));
    for reason in [
        error_helpers::RejectReason::unknown_field(&raw),
        error_helpers::RejectReason::descriptor_schema(&raw),
    ] {
        let payload = match reason {
            error_helpers::RejectReason::UnknownField(payload)
            | error_helpers::RejectReason::DescriptorSchema(payload) => payload,
            other => panic!("unexpected diagnostic variant: {other:?}"),
        };
        assert_eq!(payload.len(), 64);
        assert!(payload.is_ascii());
        assert!(payload
            .bytes()
            .all(|byte| (0x20..=0x7e).contains(&byte) && byte != b'\\' && byte != b'"'));
        assert!(payload.starts_with("a???????"));
    }

    let mut document = descriptor();
    document["bad\"\\\n"] = Value::Bool(true);
    let parsed = descriptor_parser::parse_descriptor(
        &serde_json::to_vec(&document).unwrap(),
        &catalog(),
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(parsed, RejectReason::UnknownField("bad???".to_owned()));
}

#[test]
fn request_result_window_evicts_oldest_without_evicting_records() {
    let temp = TempDir::new();
    let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let root = temp.0.join("store");
    let mut store = Store::open(&root, catalog(), Limits::default()).unwrap();
    let original_record = store
        .install(&request(path.clone(), &bytes, "req_00000000"))
        .unwrap()
        .record
        .unwrap();
    drop(store);

    // Seed valid history so this test exercises the real eviction commit without
    // spending hundreds of filesystem flushes creating prior successful requests.
    let registry_path = root.join("registry.json");
    let mut document: Value = serde_json::from_slice(&fs::read(&registry_path).unwrap()).unwrap();
    let template = document["request_results"][0].clone();
    let mut seeded = Vec::with_capacity(MAX_REQUEST_RESULTS);
    for index in 0..MAX_REQUEST_RESULTS {
        let id = format!("req_{index:08}");
        let mut entry = template.clone();
        entry["request_id"] = Value::String(id.clone());
        entry["result"]["request_id"] = Value::String(id);
        seeded.push(entry);
    }
    document["request_results"] = Value::Array(seeded);
    fs::write(&registry_path, serde_json::to_vec(&document).unwrap()).unwrap();

    let mut reopened = Store::open(&root, catalog(), Limits::default()).unwrap();
    let inserted = reopened
        .install(&request(path.clone(), &bytes, "req_00000512"))
        .unwrap();
    assert_eq!(inserted.status, Status::Installed);
    assert_eq!(inserted.record, Some(original_record.clone()));
    let document: Value = serde_json::from_slice(&fs::read(&registry_path).unwrap()).unwrap();
    let results = document["request_results"].as_array().unwrap();
    assert_eq!(results.len(), MAX_REQUEST_RESULTS);
    assert_eq!(results[0]["request_id"], "req_00000001");
    assert_eq!(
        results[MAX_REQUEST_RESULTS - 1]["request_id"],
        "req_00000512"
    );
    assert_eq!(reopened.records(), vec![original_record.clone()]);
    drop(reopened);

    let mut reopened = Store::open(&root, catalog(), Limits::default()).unwrap();
    let retry = reopened
        .install(&request(path, &bytes, "req_00000000"))
        .unwrap();
    assert_eq!(retry.status, Status::Installed);
    assert_eq!(retry.record, Some(original_record));
    assert_eq!(reopened.records().len(), 1);
    let document: Value =
        serde_json::from_slice(&fs::read(root.join("registry.json")).unwrap()).unwrap();
    let results = document["request_results"].as_array().unwrap();
    assert_eq!(results.len(), MAX_REQUEST_RESULTS);
    assert_eq!(results[0]["request_id"], "req_00000002");
    assert_eq!(
        results[MAX_REQUEST_RESULTS - 1]["request_id"],
        "req_00000000"
    );
}
