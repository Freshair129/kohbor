mod common;
use common::*;
use kohbor_pack::{
    error::RejectReason,
    limits::Limits,
    store::{InstallRequest, Status, Store},
    trust::{AuthorityClass, PublisherClaim},
};

#[test]
fn installs_records_and_preserves_source() {
    for publisher in [false, true] {
        let temp = TempDir::new();
        let (path, bytes) = store_package(&temp.0, "test-pack", 1, publisher);
        let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
        let result = store
            .install(&request(path.clone(), &bytes, "request_1"))
            .unwrap();
        assert_eq!(result.status, Status::Installed);
        assert_eq!(result.reason, None);
        let record = result.record.unwrap();
        assert_eq!(record.install_id, "ins_0000000001");
        assert_eq!(record.package_id, "test-pack");
        assert_eq!(record.revision, 1);
        assert_eq!(record.container_sha256, hash(&bytes));
        assert_eq!(record.authority_class, AuthorityClass::LocalImport);
        assert_eq!(
            record.publisher_claim,
            if publisher {
                PublisherClaim::Unverified
            } else {
                PublisherClaim::None
            }
        );
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        assert!(store.active().is_none());
    }
}

#[test]
fn request_parsing_is_strict() {
    let _temp = TempDir::new();
    assert!(InstallRequest::from_json(&fixture("media-install-request-valid.json")).is_ok());
    assert!(InstallRequest::from_json(&fixture("media-install-request-bad-policy.json")).is_err());
    let original: serde_json::Value =
        serde_json::from_slice(&fixture("media-install-request-valid.json")).unwrap();
    for (key, value) in [
        ("extra", serde_json::json!(1)),
        ("request_id", serde_json::json!("!")),
        ("expected_container_sha256", serde_json::json!("bad")),
    ] {
        let mut document = original.clone();
        document[key] = value;
        assert!(
            InstallRequest::from_json(&serde_json::to_vec(&document).unwrap()).is_err(),
            "{key}"
        );
    }
}

#[test]
fn retry_and_revision_identity_are_idempotent() {
    let temp = TempDir::new();
    let (path, bytes) = store_package(&temp.0, "test-pack", 2, false);
    let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    let req = request(path.clone(), &bytes, "request_1");
    let first = store.install(&req).unwrap();
    let before = tree(&temp.0.join("store"));
    assert_eq!(store.install(&req).unwrap().to_json(), first.to_json());
    assert_eq!(tree(&temp.0.join("store")), before);
    let second = store.install(&request(path, &bytes, "request_2")).unwrap();
    assert_eq!(
        second.status,
        Status::Installed,
        "same identity and digest status"
    );
    assert_eq!(first.record, second.record);
    let (changed, different) = store_package(&temp.0, "test-pack", 2, true);
    let conflict = store
        .install(&request(changed.clone(), &different, "request_3"))
        .unwrap();
    assert_eq!(conflict.reject, Some(RejectReason::RevisionConflict));
    assert_eq!(
        conflict.status,
        Status::Rejected,
        "revision conflict status"
    );
    assert!(conflict.record.is_none(), "revision conflict record");
    let reused = store
        .install(&request(changed, &different, "request_1"))
        .unwrap();
    // S2-4 F6: a reused request id with different content is a request conflict.
    assert_eq!(reused.reject, Some(RejectReason::RequestConflict));
    assert_eq!(reused.status, Status::Rejected, "reused request status");
    assert!(reused.record.is_none(), "reused request record");
    assert_eq!(store.records().len(), 1);
    let (next_path, next_bytes) = store_package(&temp.0, "other-pack", 1, false);
    assert_eq!(
        store
            .install(&request(next_path, &next_bytes, "request_4"))
            .unwrap()
            .record
            .unwrap()
            .install_id,
        "ins_0000000002",
        "retries and rejected conflicts do not consume a counter value"
    );
}

#[test]
fn lower_revision_and_other_package_keep_active_selection() {
    use kohbor_pack::store::{ActivationPolicy, ActivationState};
    let temp = TempDir::new();
    let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    let (path, bytes) = store_package(&temp.0, "test-pack", 2, false);
    let first = store
        .install(&request(path, &bytes, "request_1"))
        .unwrap()
        .record
        .unwrap();
    store
        .activate(
            "request_1",
            &first.install_id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    for (package, revision, id) in [
        ("test-pack", 1, "request_2"),
        ("other-pack", 1, "request_3"),
    ] {
        let (path, bytes) = store_package(&temp.0, package, revision, false);
        assert_eq!(
            store.install(&request(path, &bytes, id)).unwrap().status,
            Status::Installed
        );
        assert_eq!(store.active().unwrap().install_id, first.install_id);
        assert!(store.resolve("first_blood", None).is_ok());
    }
    assert_eq!(
        store
            .records()
            .iter()
            .map(|r| r.install_id.as_str())
            .collect::<Vec<_>>(),
        vec!["ins_0000000001", "ins_0000000002", "ins_0000000003"]
    );
}

#[test]
fn replaced_source_is_checked_when_copied() {
    let temp = TempDir::new();
    let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
    let req = request(path.clone(), &bytes, "request_1");
    std::fs::write(&path, b"replacement").unwrap();
    let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    let result = store.install(&req).unwrap();
    assert_eq!(result.reject, Some(RejectReason::ContainerDigestMismatch));
    assert_eq!(std::fs::read(path).unwrap(), b"replacement");
    assert!(store.records().is_empty());
}

#[test]
fn rejected_sources_do_not_commit() {
    for case in 0..3 {
        let temp = TempDir::new();
        let (path, bytes) = store_package(&temp.0, "test-pack", 1, false);
        let mut req = request(path.clone(), &bytes, "request_1");
        let expected = match case {
            0 => {
                req.expected_container_sha256 = "0".repeat(64);
                RejectReason::ContainerDigestMismatch
            }
            1 => {
                std::fs::remove_file(path).unwrap();
                RejectReason::SourceUnavailable
            }
            _ => {
                std::fs::write(&path, b"not zip").unwrap();
                req.expected_container_sha256 = hash(b"not zip");
                RejectReason::NotAZip
            }
        };
        let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
        let result = store.install(&req).unwrap();
        assert_eq!(result.status, Status::Rejected);
        assert_eq!(result.record, None);
        assert_eq!(result.reject, Some(expected.clone()));
        assert_eq!(result.reason.as_deref(), Some(expected.contract_reason()));
        assert!(store.records().is_empty());
        assert!(tree(&temp.0.join("store/staging")).is_empty());
    }
}

#[test]
fn serialized_results_obey_result_schema() {
    let _temp = TempDir::new();
    use kohbor_pack::store::{InstallResult, Record};
    let schema: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../contracts/media-install-result.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        schema["properties"]["request_id"]["pattern"], "^[A-Za-z0-9_-]{8,64}$",
        "request_id schema pattern changed"
    );
    let record_schema = &schema["properties"]["record"]["anyOf"][0]["properties"];
    assert_eq!(
        record_schema["install_id"]["pattern"], "^[A-Za-z0-9_-]{8,64}$",
        "install_id schema pattern changed"
    );
    assert_eq!(
        record_schema["package_id"]["pattern"], "^[a-z0-9][a-z0-9_.-]{0,79}$",
        "package_id schema pattern changed"
    );
    assert_eq!(
        record_schema["container_sha256"]["pattern"], "^[a-f0-9]{64}$",
        "digest schema pattern changed"
    );
    let record = Record {
        install_id: "ins_0000000001".into(),
        package_id: "test-pack".into(),
        revision: 1,
        container_sha256: "a".repeat(64),
        authority_class: AuthorityClass::LocalImport,
        publisher_claim: PublisherClaim::None,
    };
    for (status, reason, has_record) in [
        (Status::Installed, None, true),
        (Status::Active, None, true),
        (Status::Deferred, Some("ACTIVATION_DEFERRED"), true),
        (Status::ActivationError, Some("DISK_PRESSURE"), true),
        (Status::Rejected, Some("SOURCE_INVALID"), false),
    ] {
        let doc = InstallResult {
            request_id: "request_1".into(),
            status,
            reason: reason.map(str::to_owned),
            record: has_record.then(|| record.clone()),
            reject: None,
        }
        .to_json();
        let obj = doc.as_object().unwrap();
        let required = schema["required"].as_array().unwrap();
        assert_eq!(obj.len(), required.len());
        for field in required {
            assert!(obj.contains_key(field.as_str().unwrap()));
        }
        assert_eq!(
            doc["schema_version"],
            schema["properties"]["schema_version"]["const"]
        );
        assert!(schema["properties"]["status"]["enum"]
            .as_array()
            .unwrap()
            .contains(&doc["status"]));
        assert_eq!(
            doc["status"],
            status.as_str(),
            "status serialization: {status:?}"
        );
        let id = doc["request_id"].as_str().unwrap();
        assert!(
            (8..=64).contains(&id.len())
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        );
        if let Some(code) = doc["reason"].as_str() {
            assert!(schema["properties"]["reason"]["anyOf"][0]["enum"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!(code)));
        } else {
            assert!(doc["reason"].is_null(), "reason type: {status:?}");
        }
        assert_eq!(doc["reason"].as_str(), reason, "reason value: {status:?}");
        if has_record {
            let item = doc["record"].as_object().unwrap();
            let definition = &schema["properties"]["record"]["anyOf"][0];
            let keys = definition["required"].as_array().unwrap();
            assert_eq!(item.len(), keys.len());
            for key in keys {
                assert!(item.contains_key(key.as_str().unwrap()));
            }
            let install_id = doc["record"]["install_id"].as_str().unwrap();
            assert!(
                (8..=64).contains(&install_id.len())
                    && install_id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
                "install_id pattern: {status:?}"
            );
            let package_id = doc["record"]["package_id"].as_str().unwrap();
            assert!(
                package_id
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
                "package_id first character: {status:?}"
            );
            assert!(
                (1..=80).contains(&package_id.len())
                    && package_id.bytes().all(|b| b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || b"_.-".contains(&b)),
                "package_id pattern: {status:?}"
            );
            assert_eq!(
                doc["record"]["authority_class"],
                definition["properties"]["authority_class"]["const"]
            );
            assert!(definition["properties"]["publisher_claim"]["enum"]
                .as_array()
                .unwrap()
                .contains(&doc["record"]["publisher_claim"]));
            let digest = doc["record"]["container_sha256"].as_str().unwrap();
            assert!(
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            );
            assert!(
                (1..=9_007_199_254_740_991).contains(&doc["record"]["revision"].as_u64().unwrap()),
                "revision bounds: {status:?}"
            );
        } else {
            assert!(doc["record"].is_null());
        }
    }
    let mut unverified = record;
    unverified.publisher_claim = PublisherClaim::Unverified;
    let json = InstallResult {
        request_id: "request_2".into(),
        status: Status::Installed,
        reason: None,
        record: Some(unverified),
        reject: None,
    }
    .to_json();
    assert_eq!(
        json["record"]["publisher_claim"], "unverified",
        "unverified publisher claim serialization"
    );
    assert!(
        record_schema["publisher_claim"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json["record"]["publisher_claim"]),
        "unverified publisher claim enum"
    );
}

fn rejection_preserves_active(case: &str) {
    use kohbor_pack::store::{ActivationPolicy, ActivationState};
    let temp = TempDir::new();
    let root = temp.0.join("store");
    let (old_path, old_bytes) = store_package(&temp.0, "test-pack", 1, false);
    let limits = Limits {
        container_bytes: 1024 * 1024,
        ..Limits::default()
    };
    let mut store = Store::open(&root, catalog(), limits).unwrap();
    let old = store
        .install(&request(old_path, &old_bytes, "request_old"))
        .unwrap()
        .record
        .unwrap();
    store
        .activate(
            "request_old",
            &old.install_id,
            ActivationPolicy::NowIfIdle,
            &ActivationState::default(),
        )
        .unwrap();
    let registry = std::fs::read(root.join("registry.json")).unwrap();
    let active = store.active();
    let (path, bytes) = store_package(&temp.0, "test-pack", 2, false);
    let mut req = request(path.clone(), &bytes, "request_bad");
    let (reason, code) = match case {
        "digest" => {
            req.expected_container_sha256 = "0".repeat(64);
            (RejectReason::ContainerDigestMismatch, "SOURCE_INVALID")
        }
        "missing" => {
            std::fs::remove_file(&path).unwrap();
            (RejectReason::SourceUnavailable, "SOURCE_UNAVAILABLE")
        }
        "hostile" => {
            let mut entries = package_entries();
            // Right after descriptor.json so that the unsafe name, not the entry
            // order, is the first violation the reader meets.
            entries.insert(1, ZipEntry::new("../evil", vec![1], 0));
            let data = zip(&entries, &[], &[], &[]);
            std::fs::write(&path, &data).unwrap();
            req.expected_container_sha256 = hash(&data);
            (RejectReason::UnsafePath, "MEDIA_INVALID")
        }
        _ => unreachable!(),
    };
    let result = store.install(&req).unwrap();
    assert_eq!(result.status, Status::Rejected, "{case}: status");
    assert_eq!(result.record, None, "{case}: no record");
    assert_eq!(result.reject, Some(reason), "{case}: specific reject");
    assert_eq!(
        result.reason.as_deref(),
        Some(code),
        "{case}: contract reason"
    );
    assert_eq!(
        std::fs::read(root.join("registry.json")).unwrap(),
        registry,
        "{case}: registry unchanged"
    );
    assert_eq!(
        store.records(),
        vec![old.clone()],
        "{case}: records unchanged"
    );
    assert_eq!(store.active(), active, "{case}: active unchanged");
    assert_eq!(
        store.resolve("first_blood", None).unwrap().install_id,
        old.install_id,
        "{case}: active resolves"
    );
    assert!(
        tree(&root.join("staging")).is_empty(),
        "{case}: staging empty"
    );
    let (path, bytes) = store_package(&temp.0, "test-pack", 3, false);
    assert_eq!(
        store
            .install(&request(path, &bytes, "request_next"))
            .unwrap()
            .record
            .unwrap()
            .install_id,
        "ins_0000000002",
        "{case}: counter unchanged"
    );
    assert!(!temp.0.join("evil").exists(), "{case}: no traversal write");
}
#[test]
fn digest_rejection_preserves_active() {
    rejection_preserves_active("digest");
}
#[test]
fn missing_source_preserves_active() {
    rejection_preserves_active("missing");
}
#[test]
fn hostile_archive_preserves_active() {
    rejection_preserves_active("hostile");
}

fn invalid_request_field(key: &str, value: serde_json::Value) {
    let _temp = TempDir::new();
    let mut document: serde_json::Value =
        serde_json::from_slice(&fixture("media-install-request-valid.json")).unwrap();
    document[key] = value;
    assert!(
        InstallRequest::from_json(&serde_json::to_vec(&document).unwrap()).is_err(),
        "invalid {key}"
    );
}
#[test]
fn request_rejects_short_id() {
    invalid_request_field("request_id", serde_json::json!("short"));
}
#[test]
fn request_rejects_invalid_profile() {
    invalid_request_field("profile_id", serde_json::json!("bad/profile"));
}
#[test]
fn request_rejects_invalid_project_binding() {
    invalid_request_field("project_binding", serde_json::json!("bad binding"));
}
#[test]
fn request_rejects_uppercase_hash() {
    invalid_request_field(
        "expected_container_sha256",
        serde_json::json!("A".repeat(64)),
    );
}
#[test]
fn request_rejects_short_path() {
    invalid_request_field("source_path", serde_json::json!("x"));
}

#[test]
fn install_writes_only_under_store_root() {
    let temp = TempDir::new();
    let source_dir = temp.0.join("source");
    let sibling = temp.0.join("sibling");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();
    std::fs::write(sibling.join("sentinel"), b"untouched").unwrap();
    let (path, bytes) = store_package(&source_dir, "test-pack", 1, false);
    let before_source = tree(&source_dir);
    let before_sibling = tree(&sibling);
    let mut store = Store::open(&temp.0.join("store"), catalog(), Limits::default()).unwrap();
    store
        .install(&request(path.clone(), &bytes, "request_1"))
        .unwrap();
    assert_eq!(tree(&source_dir), before_source, "source directory entries");
    assert_eq!(std::fs::read(path).unwrap(), bytes, "source file contents");
    assert_eq!(tree(&sibling), before_sibling, "sibling directory entries");
    assert_eq!(
        std::fs::read(sibling.join("sentinel")).unwrap(),
        b"untouched",
        "sibling contents"
    );
    assert_eq!(
        tree(&temp.0)
            .iter()
            .filter(|p| p.components().count() == 1)
            .count(),
        3,
        "unexpected sibling created"
    );
}
