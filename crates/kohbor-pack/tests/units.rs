mod common;
use kohbor_pack::{
    catalog::{Catalog, TaxonomyVersion},
    limits::*,
    RejectReason,
};
use serde_json::json;

#[test]
fn taxonomy_parse_table() {
    for (s, expected) in [
        ("1.1", Some((1, 1))),
        ("0.0", Some((0, 0))),
        ("4294967295.0", Some((u32::MAX, 0))),
        ("01.1", None),
        ("1.01", None),
        ("+1.1", None),
        ("1.1.0", None),
        ("1.1-beta", None),
        (" 1.1", None),
        ("1.1 ", None),
        ("", None),
        ("1", None),
        (".1", None),
        ("1.", None),
        ("4294967296.0", None),
        ("-1.0", None),
    ] {
        assert_eq!(
            TaxonomyVersion::parse(s).map(|v| (v.major, v.minor)),
            expected,
            "{s}"
        );
    }
}
#[test]
fn real_catalog_matches_source() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/event-catalog.json");
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let catalog = Catalog::from_json(&doc).unwrap();
    assert_eq!(catalog.taxonomy, TaxonomyVersion { major: 1, minor: 1 });
    assert_eq!(
        catalog.event("multi_kill").unwrap().tier_range,
        Some((2, 5))
    );
    assert_eq!(catalog.event("streak").unwrap().tier_range, Some((3, 10)));
    let active: Vec<_> = doc["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["status"] == "active")
        .collect();
    assert_eq!(catalog.events.len(), active.len());
    for e in active {
        let item = catalog.event(e["id"].as_str().unwrap()).unwrap();
        assert_eq!(item.media_mappable, e["media_mappable"].as_bool().unwrap());
    }
    for id in doc["retired_ids"].as_array().unwrap() {
        assert!(catalog.event(id.as_str().unwrap()).is_none());
    }
}
#[test]
fn malformed_catalogs_rejected() {
    for doc in [
        json!(null),
        json!({}),
        json!({"taxonomy_version":"1.1","events":[{}]}),
    ] {
        assert!(Catalog::from_json(&doc).is_none());
    }
}
#[test]
fn limits_defaults() {
    let l = Limits::default();
    assert_eq!(
        (
            l.container_bytes,
            l.expanded_bytes,
            l.payload_bytes,
            l.descriptor_bytes,
            l.payload_count,
            l.bindings
        ),
        (512 * MIB, 512 * MIB, 32 * MIB, 2 * MIB, 2048, 128)
    );
}
#[test]
fn limits_clamp() {
    let l = Limits {
        container_bytes: u64::MAX,
        expanded_bytes: u64::MAX,
        payload_bytes: u64::MAX,
        descriptor_bytes: u64::MAX,
        payload_count: usize::MAX,
        bindings: usize::MAX,
    }
    .clamped();
    assert_eq!(l, Limits::default());
}
#[test]
fn rejection_identifiers_and_contract_mapping() {
    use RejectReason::*;
    let variants = [
        SourceUnavailable,
        NotAZip,
        ContainerTooLarge,
        TooManyEntries,
        DuplicateEntry,
        UnsafePath,
        EncryptedEntry,
        UnsupportedCompression,
        UnexpectedExtraField,
        ArchiveComment,
        HeaderMismatch,
        HiddenBytes,
        DescriptorNotFirst,
        EntryTooLarge,
        ExpansionLimit,
        DuplicateJsonKey,
        DescriptorSyntax,
        DescriptorSchema("/x".into()),
        UnknownField("x".into()),
        TaxonomyMalformed,
        TaxonomyMajorUnsupported,
        TierNotAllowed,
        TierOutOfRange,
        TierDuplicate,
        BindingRoleMismatch,
        UnpairedBinding,
        DanglingReference,
        UndeclaredPayload,
        SizeMismatch,
        DigestMismatch,
        ContainerDigestMismatch,
        MixedAudioInvalid,
        IdentityAudioInvalid,
        IdentityAudioLengthMismatch,
        AudioDurationMismatch,
        VisualInvalid,
        VisualHasAudioTrack,
        VisualDurationMismatch,
        RevisionConflict,
        UnknownInstall,
    ];
    let names = [
        "source_unavailable",
        "not_a_zip",
        "container_too_large",
        "too_many_entries",
        "duplicate_entry",
        "unsafe_path",
        "encrypted_entry",
        "unsupported_compression",
        "unexpected_extra_field",
        "archive_comment",
        "header_mismatch",
        "hidden_bytes",
        "descriptor_not_first",
        "entry_too_large",
        "expansion_limit",
        "duplicate_json_key",
        "descriptor_syntax",
        "descriptor_schema",
        "unknown_field",
        "taxonomy_malformed",
        "taxonomy_major_unsupported",
        "tier_not_allowed",
        "tier_out_of_range",
        "tier_duplicate",
        "binding_role_mismatch",
        "unpaired_binding",
        "dangling_reference",
        "undeclared_payload",
        "size_mismatch",
        "digest_mismatch",
        "container_digest_mismatch",
        "mixed_audio_invalid",
        "identity_audio_invalid",
        "identity_audio_length_mismatch",
        "audio_duration_mismatch",
        "visual_invalid",
        "visual_has_audio_track",
        "visual_duration_mismatch",
        "revision_conflict",
        "unknown_install",
    ];
    let mut seen = std::collections::HashSet::new();
    for (v, name) in variants.into_iter().zip(names) {
        let s = v.as_str();
        assert_eq!(s, name);
        assert!(seen.insert(s));
        assert!(s.bytes().all(|c| c.is_ascii_lowercase() || c == b'_'));
        assert!(!s.starts_with('_') && !s.ends_with('_') && !s.contains("__"));
        let expected = match v {
            SourceUnavailable => "SOURCE_UNAVAILABLE",
            ContainerDigestMismatch => "SOURCE_INVALID",
            TaxonomyMajorUnsupported => "VERSION_UNSUPPORTED",
            _ => "MEDIA_INVALID",
        };
        assert_eq!(v.contract_reason(), expected);
    }
}
