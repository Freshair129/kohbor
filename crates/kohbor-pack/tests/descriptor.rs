mod common;
use kohbor_pack::{catalog::Catalog, descriptor::parse_descriptor, Limits, RejectReason};
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
    let bytes = br#"{"schema_version":"1.3.0","schema_version":"1.3.0"}"#;
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

fn variants_fixture() -> Value {
    serde_json::from_slice(&common::fixture("media-variants-valid.json")).unwrap()
}

fn assert_reason(value: &Value, expected: &str) {
    assert_eq!(parse(value).unwrap_err().as_str(), expected);
}

fn retain_first_variant_files(value: &mut Value) {
    let variant = value["bindings"][0]["variants"][0].clone();
    let audio_path = variant["mixed_audio_path"].as_str().unwrap().to_owned();
    let visual_path = variant["silent_visual_path"].as_str().unwrap().to_owned();
    value["files"].as_array_mut().unwrap().retain(|file| {
        matches!(file["path"].as_str(), Some(path) if path == audio_path.as_str() || path == visual_path.as_str())
    });
}

#[test]
fn admits_valid_multi_variant_fixture() {
    assert!(parse(&variants_fixture()).is_ok());
}

#[test]
fn admits_proposed_v141_caption_and_keeps_v140_without_caption_valid() {
    let mut with_caption = variants_fixture();
    with_caption["schema_version"] = json!("1.4.1");
    with_caption["bindings"][0]["variants"][0]["caption"] = json!("เสียงสังหารต่อไป");
    let parsed = parse(&with_caption).expect("proposed 1.4.1 caption is accepted");
    assert_eq!(
        parsed.descriptor.bindings[0].variants[0].caption.as_deref(),
        Some("เสียงสังหารต่อไป")
    );
    assert!(
        parse(&variants_fixture()).is_ok(),
        "existing 1.4.0 packs without captions remain valid"
    );
}

#[test]
fn rejects_caption_outside_v141() {
    let mut old_version = variants_fixture();
    old_version["bindings"][0]["variants"][0]["caption"] = json!("legacy must stay caption-free");
    assert_reason(&old_version, "unknown_field");
}

#[test]
fn v141_caption_keeps_original_text_and_counts_unicode_scalars() {
    for caption in ["e".to_owned(), "e\u{301}".to_owned(), "🦋".repeat(200)] {
        let mut value = variants_fixture();
        value["schema_version"] = json!("1.4.1");
        value["bindings"][0]["variants"][0]["caption"] = json!(&caption);
        let parsed = parse(&value).expect("trimmed single-line caption is accepted");
        assert_eq!(
            parsed.descriptor.bindings[0].variants[0].caption.as_deref(),
            Some(caption.as_str())
        );
    }
}

#[test]
fn v141_rejects_empty_untrimmed_overlong_and_multiline_captions() {
    for caption in [
        String::new(),
        " ".to_owned(),
        " leading".to_owned(),
        "trailing\u{a0}".to_owned(),
        "🦋".repeat(201),
        "a\0b".to_owned(),
        "a\u{7f}b".to_owned(),
        "a\t b".to_owned(),
        "line\nbreak".to_owned(),
        "line\rbreak".to_owned(),
        "line\u{85}break".to_owned(),
        "line\u{2028}break".to_owned(),
        "line\u{2029}break".to_owned(),
    ] {
        let mut value = variants_fixture();
        value["schema_version"] = json!("1.4.1");
        value["bindings"][0]["variants"][0]["caption"] = json!(caption);
        assert_reason(&value, "caption_invalid");
    }
}

#[test]
fn admits_one_v14_variant_without_selection() {
    let mut value = variants_fixture();
    value["bindings"][0]["variants"]
        .as_array_mut()
        .unwrap()
        .pop();
    value["bindings"][0]
        .as_object_mut()
        .unwrap()
        .remove("selection");
    retain_first_variant_files(&mut value);
    assert!(parse(&value).is_ok());
}

#[test]
fn maps_each_legacy_binding_to_a_stable_variant() {
    let parsed = parse(&common::descriptor()).unwrap();
    assert_eq!(parsed.descriptor.bindings[0].variants[0].variant_id, "v1");
    assert_eq!(parsed.descriptor.bindings[1].variants[0].variant_id, "v2");
    assert_eq!(parsed.descriptor.bindings[0].selection, None);
}

#[test]
fn requires_selection_for_multiple_variants() {
    let mut value = variants_fixture();
    value["bindings"][0]
        .as_object_mut()
        .unwrap()
        .remove("selection");
    assert_reason(&value, "selection_required");
}

#[test]
fn rejects_selection_for_single_variant() {
    let mut value = variants_fixture();
    value["bindings"][0]["variants"]
        .as_array_mut()
        .unwrap()
        .pop();
    value["bindings"][0]["selection"] = json!("random");
    assert_reason(&value, "selection_unexpected");
}

#[test]
fn rejects_weight_without_random_selection() {
    let mut value = variants_fixture();
    value["bindings"][0]["selection"] = json!("shuffle");
    assert_reason(&value, "weight_selection_mismatch");
}

#[test]
fn rejects_reserved_when_condition_explicitly() {
    let mut value = variants_fixture();
    value["bindings"][0]["variants"][0]["when"] = json!({"fact":"killer","eq":"ally"});
    assert_reason(&value, "conditions_reserved");
}

#[test]
fn rejects_duplicate_variant_ids_across_binding() {
    let mut value = variants_fixture();
    let mut duplicate_binding = value["bindings"][0].clone();
    duplicate_binding["event_id"] = json!("streak");
    duplicate_binding["tier"] = json!(3);
    value["bindings"]
        .as_array_mut()
        .unwrap()
        .push(duplicate_binding);
    assert_reason(&value, "duplicate_variant_id");
}

#[test]
fn rejects_invalid_clip_id() {
    let mut value = variants_fixture();
    value["bindings"][0]["variants"][1]["clip_id"] = json!("not-a-clip");
    assert_reason(&value, "clip_id_invalid");
}

#[test]
fn rejects_invalid_variant_id() {
    let mut value = variants_fixture();
    value["bindings"][0]["variants"][0]["variant_id"] = json!("UpperCase");
    assert_reason(&value, "variant_id_invalid");
}

#[test]
fn rejects_invalid_variant_weight() {
    let mut value = variants_fixture();
    value["bindings"][0]["variants"][1]["weight"] = json!(101);
    assert_reason(&value, "variant_weight_invalid");
}

#[test]
fn rejects_unknown_selection_value() {
    let mut value = variants_fixture();
    value["bindings"][0]["selection"] = json!("round_robin");
    assert_reason(&value, "selection_invalid");
}

#[test]
fn rejects_empty_variant_list() {
    let mut value = variants_fixture();
    value["bindings"][0]["variants"] = json!([]);
    assert_reason(&value, "variant_count_invalid");
}

#[test]
fn rejects_legacy_media_fields_in_a_v14_binding() {
    let mut value = variants_fixture();
    value["bindings"][0]["mixed_audio_path"] = json!("audio/cue1.wav");
    assert_eq!(
        parse(&value).unwrap_err(),
        RejectReason::UnknownField("mixed_audio_path".into())
    );
}

#[test]
fn rejects_more_than_sixteen_variants_per_binding() {
    let mut value = variants_fixture();
    let original = value["bindings"][0]["variants"][0].clone();
    let variants = (0..17)
        .map(|index| {
            let mut variant = original.clone();
            variant["variant_id"] = json!(format!("v{index}"));
            variant
        })
        .collect::<Vec<_>>();
    value["bindings"][0]["variants"] = json!(variants);
    assert_reason(&value, "too_many_variants_per_binding");
}

fn replace_with_reused_variant_bindings(value: &mut Value, binding_count: usize) {
    let mut audio = None;
    let mut visual = None;
    for file in value["files"].as_array().unwrap() {
        match file["role"].as_str().unwrap() {
            "mixed_audio" if audio.is_none() => audio = file["path"].as_str().map(str::to_owned),
            "silent_visual" if visual.is_none() => {
                visual = file["path"].as_str().map(str::to_owned)
            }
            _ => {}
        }
    }
    let audio = audio.unwrap();
    let visual = visual.unwrap();
    let retained_files = value["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|file| {
            file["path"].as_str() == Some(audio.as_str())
                || file["path"].as_str() == Some(visual.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    value["files"] = json!(retained_files);
    let bindings = (0..binding_count)
        .map(|binding_index| {
            let variants = (0..16)
                .map(|variant_index| {
                    json!({
                        "variant_id": format!("v{binding_index}_{variant_index}"),
                        "mixed_audio_path": audio.clone(),
                        "silent_visual_path": visual.clone(),
                        "duration_ms": 1000,
                        "visual_tail_ms": 0
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "event_id": format!("future_event_{binding_index}"),
                "selection": "shuffle",
                "variants": variants
            })
        })
        .collect::<Vec<_>>();
    value["bindings"] = json!(bindings);
}

#[test]
fn rejects_more_than_512_variants_per_pack() {
    let mut value = variants_fixture();
    replace_with_reused_variant_bindings(&mut value, 33);
    assert_reason(&value, "too_many_variants_per_pack");
}

#[test]
fn rejects_multiple_variants_for_critical_media() {
    let mut value = variants_fixture();
    value["bindings"][0]["event_id"] = json!("health_danger");
    let result = parse_descriptor(
        &serde_json::to_vec(&value).unwrap(),
        &common::load_catalog(),
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(result.as_str(), "critical_media_variant_count");
}

#[test]
fn rejects_multiple_variants_for_legacy_critical_category() {
    let catalog = Catalog::from_json(&json!({
        "taxonomy_version": "1.1",
        "events": [{
            "id": "legacy_critical",
            "status": "active",
            "media_mappable": true,
            "category": "critical"
        }]
    }))
    .expect("legacy catalog is accepted");
    let mut value = variants_fixture();
    value["bindings"][0]["event_id"] = json!("legacy_critical");
    assert_eq!(
        parse_descriptor(
            &serde_json::to_vec(&value).unwrap(),
            &catalog,
            &Limits::default()
        )
        .unwrap_err(),
        RejectReason::CriticalMediaVariantCount
    );
}

#[test]
fn admits_one_variant_for_critical_media() {
    let mut value = variants_fixture();
    value["bindings"][0]["event_id"] = json!("health_danger");
    value["bindings"][0]["variants"]
        .as_array_mut()
        .unwrap()
        .pop();
    value["bindings"][0]
        .as_object_mut()
        .unwrap()
        .remove("selection");
    retain_first_variant_files(&mut value);
    assert!(parse(&value).is_ok());
}

#[test]
fn admits_exactly_512_variants_per_pack() {
    let mut value = variants_fixture();
    replace_with_reused_variant_bindings(&mut value, 32);
    assert!(parse(&value).is_ok());
}

#[test]
fn unpaired_files_are_checked_across_variants() {
    let mut value = variants_fixture();
    value["bindings"][0]["variants"][1]["mixed_audio_path"] =
        value["bindings"][0]["variants"][0]["mixed_audio_path"].clone();
    value["bindings"][0]["variants"][1]["silent_visual_path"] =
        value["bindings"][0]["variants"][0]["silent_visual_path"].clone();
    assert_eq!(parse(&value).unwrap_err(), RejectReason::UnpairedBinding);
}

#[test]
fn checks_v14_variant_paths_for_traversal() {
    let mut value = variants_fixture();
    value["bindings"][0]["variants"][0]["mixed_audio_path"] = json!("audio/../escape.wav");
    assert_eq!(parse(&value).unwrap_err(), RejectReason::UnsafePath);
}

#[test]
fn checks_v14_variant_references_and_roles() {
    let mut dangling = variants_fixture();
    dangling["bindings"][0]["variants"][0]["mixed_audio_path"] = json!("audio/missing.wav");
    assert_eq!(
        parse(&dangling).unwrap_err(),
        RejectReason::DanglingReference
    );

    let mut wrong_role = variants_fixture();
    for file in wrong_role["files"].as_array_mut().unwrap() {
        if file["path"].as_str() == Some("audio/cue1.wav") {
            file["role"] = json!("identity_audio");
            break;
        }
    }
    assert_eq!(
        parse(&wrong_role).unwrap_err(),
        RejectReason::BindingRoleMismatch
    );
}

fixture_reject!(
    rejects_v14_audio_only_variant_with_visual_tail,
    "media-variant-audio-only-tail-invalid.json",
    RejectReason::DescriptorSchema("/bindings/0/variants/1/visual_tail_ms".into())
);
