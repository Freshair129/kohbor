mod common;
use kohbor_pack::{descriptor::parse_descriptor, Limits, RejectReason};
use serde_json::{json, Value};
fn parse(v: &Value) -> Result<kohbor_pack::descriptor::ParsedDescriptor, RejectReason> {
    parse_descriptor(
        &serde_json::to_vec(v).unwrap(),
        &common::catalog(),
        &Limits::default(),
    )
}
macro_rules! fixture_reject {
    ($name:ident, $file:literal, $reason:expr) => {
        #[test]
        fn $name() {
            assert_eq!(
                parse_descriptor(
                    &common::fixture($file),
                    &common::catalog(),
                    &Limits::default()
                )
                .unwrap_err(),
                $reason
            );
        }
    };
}
fixture_reject!(
    self_trust,
    "media-self-trust-field.json",
    RejectReason::UnknownField("trusted_public_key".into())
);
// Lead decision: schema-invalid path patterns yield DescriptorSchema; paths that pass
// the pattern but have unsafe segments yield UnsafePath. ZIP entry names use UnsafePath.
fixture_reject!(
    traversal,
    "media-traversal-semantic.json",
    RejectReason::UnsafePath
);
fixture_reject!(
    role_mismatch,
    "media-binding-role-mismatch.json",
    RejectReason::BindingRoleMismatch
);
fixture_reject!(
    tier_out_of_range,
    "media-tier-out-of-range.json",
    RejectReason::TierOutOfRange
);
fixture_reject!(
    tier_untiered,
    "media-tier-untiered-event.json",
    RejectReason::TierNotAllowed
);
fixture_reject!(
    tier_duplicate,
    "media-tier-duplicate.json",
    RejectReason::TierDuplicate
);
#[test]
fn valid_fixture() {
    assert!(parse_descriptor(
        &common::fixture("media-tiered-bindings-valid.json"),
        &common::catalog(),
        &Limits::default()
    )
    .is_ok());
}
#[test]
fn unknown_id_inert() {
    let mut v = common::descriptor();
    v["bindings"][0]["event_id"] = json!("future_event");
    assert!(parse(&v)
        .unwrap()
        .inert_event_ids
        .contains(&"future_event".to_string()));
}
#[test]
fn non_mappable_inert() {
    let mut v = common::descriptor();
    v["bindings"][0]["event_id"] = json!("non_mappable");
    assert!(parse(&v)
        .unwrap()
        .inert_event_ids
        .contains(&"non_mappable".to_string()));
}
#[test]
fn future_minor_inert() {
    let mut v = common::descriptor();
    v["taxonomy_version"] = json!("1.99");
    v["bindings"][0]["event_id"] = json!("future_event");
    assert!(parse(&v)
        .unwrap()
        .inert_event_ids
        .contains(&"future_event".to_string()));
}
#[test]
fn older_minor_accepted() {
    let mut v = common::descriptor();
    v["taxonomy_version"] = json!("1.0");
    assert!(parse(&v).is_ok());
}
#[test]
fn duplicate_json_keys() {
    let bytes = br#"{"schema_version":"1.2.0","schema_version":"1.2.0"}"#;
    assert_eq!(
        parse_descriptor(bytes, &common::catalog(), &Limits::default()).unwrap_err(),
        RejectReason::DuplicateJsonKey
    );
}
#[test]
fn nested_duplicate_json_keys() {
    let bytes = br#"{"nested":{"x":1,"x":2}}"#;
    assert_eq!(
        parse_descriptor(bytes, &common::catalog(), &Limits::default()).unwrap_err(),
        RejectReason::DuplicateJsonKey
    );
}
#[test]
fn bad_json() {
    for bytes in [&b"{"[..], &b"\xff"[..]] {
        assert_eq!(
            parse_descriptor(bytes, &common::catalog(), &Limits::default()).unwrap_err(),
            RejectReason::DescriptorSyntax
        );
    }
}
#[test]
fn descriptor_too_large() {
    let limits = Limits {
        descriptor_bytes: 2,
        ..Limits::default()
    };
    assert_eq!(
        parse_descriptor(b"{} ", &common::catalog(), &limits).unwrap_err(),
        RejectReason::EntryTooLarge
    );
}
macro_rules! modified {
    ($name:ident, $body:expr, $reason:expr) => {
        #[test]
        fn $name() {
            let mut v = common::descriptor();
            ($body)(&mut v);
            assert_eq!(parse(&v).unwrap_err(), $reason);
        }
    };
}
modified!(
    unknown_file_field,
    |v: &mut Value| {
        v["files"][0]["extra"] = json!(1);
    },
    RejectReason::UnknownField("extra".into())
);
modified!(
    unknown_binding_field,
    |v: &mut Value| {
        v["bindings"][0]["extra"] = json!(1);
    },
    RejectReason::UnknownField("extra".into())
);
modified!(
    major_mismatch,
    |v: &mut Value| {
        v["taxonomy_version"] = json!("2.0");
    },
    RejectReason::TaxonomyMajorUnsupported
);
modified!(
    malformed_taxonomy,
    |v: &mut Value| {
        v["taxonomy_version"] = json!("01.1");
    },
    RejectReason::TaxonomyMalformed
);
modified!(
    duplicate_path,
    |v: &mut Value| {
        v["files"][1]["path"] = json!("AUDIO/CUE1.WAV");
    },
    RejectReason::DuplicateEntry
);
modified!(
    dangling_binding,
    |v: &mut Value| {
        v["bindings"][0]["mixed_audio_path"] = json!("audio/missing.wav");
    },
    RejectReason::DanglingReference
);
modified!(
    identity_role_mismatch,
    |v: &mut Value| {
        v["bindings"][0]["identity_audio_path"] = json!("audio/cue1.wav");
    },
    RejectReason::BindingRoleMismatch
);
modified!(
    unpaired_audio,
    |v: &mut Value| {
        v["bindings"].as_array_mut().unwrap().remove(0);
    },
    RejectReason::UnpairedBinding
);

// The API does not specify whether a missing field points to its parent or intended
// location; these cases assume the intended location pending an explicit decision.
macro_rules! schema_case {
    ($name:ident,$body:expr,$pointer:literal) => {
        modified!(
            $name,
            $body,
            RejectReason::DescriptorSchema($pointer.into())
        );
    };
}
schema_case!(
    schema_version_wrong,
    |v: &mut Value| {
        v["schema_version"] = json!("1.0.0");
    },
    "/schema_version"
);
schema_case!(
    taxonomy_missing,
    |v: &mut Value| {
        v.as_object_mut().unwrap().remove("taxonomy_version");
    },
    "/taxonomy_version"
);
schema_case!(
    taxonomy_null,
    |v: &mut Value| {
        v["taxonomy_version"] = Value::Null;
    },
    "/taxonomy_version"
);
schema_case!(
    taxonomy_type,
    |v: &mut Value| {
        v["taxonomy_version"] = json!(1);
    },
    "/taxonomy_version"
);
schema_case!(
    path_absolute,
    |v: &mut Value| {
        v["files"][0]["path"] = json!("/a.wav");
    },
    "/files/0/path"
);
schema_case!(
    path_drive,
    |v: &mut Value| {
        v["files"][0]["path"] = json!("C:/a.wav");
    },
    "/files/0/path"
);
schema_case!(
    path_backslash,
    |v: &mut Value| {
        v["files"][0]["path"] = json!("audio\\cue.wav");
    },
    "/files/0/path"
);
schema_case!(
    path_non_ascii,
    |v: &mut Value| {
        v["files"][0]["path"] = json!("audio/เพลง.wav");
    },
    "/files/0/path"
);
schema_case!(
    size_zero,
    |v: &mut Value| {
        v["files"][0]["size_bytes"] = json!(0);
    },
    "/files/0/size_bytes"
);
schema_case!(
    size_above_maximum,
    |v: &mut Value| {
        v["files"][0]["size_bytes"] = json!(33_554_433);
    },
    "/files/0/size_bytes"
);
schema_case!(
    hash_short,
    |v: &mut Value| {
        v["files"][0]["sha256"] = json!("123");
    },
    "/files/0/sha256"
);
schema_case!(
    hash_uppercase,
    |v: &mut Value| {
        v["files"][0]["sha256"] = json!("A".repeat(64));
    },
    "/files/0/sha256"
);
schema_case!(
    duration_zero,
    |v: &mut Value| {
        v["bindings"][0]["duration_ms"] = json!(0);
    },
    "/bindings/0/duration_ms"
);
schema_case!(
    duration_above_maximum,
    |v: &mut Value| {
        v["bindings"][0]["duration_ms"] = json!(15001);
    },
    "/bindings/0/duration_ms"
);
schema_case!(
    tail_above_maximum,
    |v: &mut Value| {
        v["bindings"][0]["visual_tail_ms"] = json!(3201);
    },
    "/bindings/0/visual_tail_ms"
);
schema_case!(
    revision_zero,
    |v: &mut Value| {
        v["revision"] = json!(0);
    },
    "/revision"
);
schema_case!(
    revision_above_maximum,
    |v: &mut Value| {
        v["revision"] = json!(9_007_199_254_740_992u64);
    },
    "/revision"
);
schema_case!(
    package_id_invalid,
    |v: &mut Value| {
        v["package_id"] = json!("Bad ID");
    },
    "/package_id"
);
modified!(
    path_dot,
    |v: &mut Value| {
        v["files"][0]["path"] = json!("audio/./x.wav");
    },
    RejectReason::UnsafePath
);
modified!(
    path_dotdot,
    |v: &mut Value| {
        v["files"][0]["path"] = json!("audio/../x.wav");
    },
    RejectReason::UnsafePath
);
modified!(
    path_empty_segment,
    |v: &mut Value| {
        v["files"][0]["path"] = json!("audio//x.wav");
    },
    RejectReason::UnsafePath
);
modified!(
    path_trailing_slash,
    |v: &mut Value| {
        v["files"][0]["path"] = json!("audio/x/");
    },
    RejectReason::UnsafePath
);
modified!(
    path_dot_leading_segment,
    |v: &mut Value| {
        v["files"][0]["path"] = json!("audio/.secret");
    },
    RejectReason::UnsafePath
);
modified!(
    tier_below,
    |v: &mut Value| {
        v["bindings"][0]["tier"] = json!(1);
    },
    RejectReason::TierOutOfRange
);
modified!(
    duplicate_tierless,
    |v: &mut Value| {
        v["bindings"][1]["event_id"] = json!("multi_kill");
        v["bindings"][0].as_object_mut().unwrap().remove("tier");
        v["bindings"][1].as_object_mut().unwrap().remove("tier");
    },
    RejectReason::TierDuplicate
);
#[test]
fn too_many_bindings() {
    let v = common::descriptor();
    let l = Limits {
        bindings: 3,
        ..Limits::default()
    };
    assert_eq!(
        parse_descriptor(&serde_json::to_vec(&v).unwrap(), &common::catalog(), &l).unwrap_err(),
        RejectReason::DescriptorSchema("/bindings".into())
    );
}
#[test]
fn too_many_files() {
    let v = common::descriptor();
    let l = Limits {
        payload_count: 7,
        ..Limits::default()
    };
    assert_eq!(
        parse_descriptor(&serde_json::to_vec(&v).unwrap(), &common::catalog(), &l).unwrap_err(),
        RejectReason::DescriptorSchema("/files".into())
    );
}
modified!(
    unpaired_visual,
    |v: &mut Value| {
        v["files"][1]["role"] = json!("source_asset");
        v["files"].as_array_mut().unwrap().push(json!({"path":"video/unused.webm","size_bytes":1,"sha256":"0".repeat(64),"role":"silent_visual"}));
        v["bindings"][0]["silent_visual_path"] = json!("video/cue2.webm");
    },
    RejectReason::UnpairedBinding
);
schema_case!(
    hash_non_hex,
    |v: &mut Value| {
        v["files"][0]["sha256"] = json!("g".repeat(64));
    },
    "/files/0/sha256"
);
modified!(
    taxonomy_patch,
    |v: &mut Value| {
        v["taxonomy_version"] = json!("1.1.0");
    },
    RejectReason::TaxonomyMalformed
);
modified!(
    taxonomy_overflow,
    |v: &mut Value| {
        v["taxonomy_version"] = json!("4294967296.0");
    },
    RejectReason::TaxonomyMalformed
);
modified!(
    taxonomy_sign,
    |v: &mut Value| {
        v["taxonomy_version"] = json!("+1.1");
    },
    RejectReason::TaxonomyMalformed
);
schema_case!(
    path_colon,
    |v: &mut Value| {
        v["files"][0]["path"] = json!("audio/a:b.wav");
    },
    "/files/0/path"
);
schema_case!(
    package_id_empty,
    |v: &mut Value| {
        v["package_id"] = json!("");
    },
    "/package_id"
);
schema_case!(
    package_id_long,
    |v: &mut Value| {
        v["package_id"] = json!("a".repeat(81));
    },
    "/package_id"
);
