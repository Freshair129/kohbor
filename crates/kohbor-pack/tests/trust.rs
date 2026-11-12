mod common;
use kohbor_pack::{
    catalog::TaxonomyVersion,
    descriptor::Descriptor,
    trust::{assess, AuthorityClass, PublisherClaim, SignatureStatus},
};
fn descriptor(id: &str, name: &str, key: Option<&str>) -> Descriptor {
    Descriptor {
        package_id: id.into(),
        revision: 1,
        taxonomy: TaxonomyVersion { major: 1, minor: 1 },
        display_name: name.into(),
        publisher_key_id: key.map(str::to_owned),
        files: vec![],
        bindings: vec![],
    }
}
#[test]
fn local_claims_cannot_grant_authority() {
    for (id, name, key) in [
        ("store", "official", Some("key")),
        ("system", "official", None),
        ("ordinary", "ordinary", None),
    ] {
        let a = assess(&descriptor(id, name, key));
        assert_eq!(a.authority_class, AuthorityClass::LocalImport);
        assert_eq!(a.signature, SignatureStatus::Unavailable);
        assert_eq!(
            a.publisher_claim,
            if key.is_some() {
                PublisherClaim::Unverified
            } else {
                PublisherClaim::None
            }
        );
        assert!(!a.publisher_trusted());
        assert!(!a.purchase_entitled());
    }
}
#[test]
fn assessment_helpers_deny_trust() {
    let a = kohbor_pack::trust::TrustAssessment {
        authority_class: AuthorityClass::LocalImport,
        publisher_claim: PublisherClaim::Unverified,
        signature: SignatureStatus::Unavailable,
    };
    assert!(!a.publisher_trusted());
    assert!(!a.purchase_entitled());
}
#[test]
fn trust_strings_match_schema() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/media-install-result.schema.json");
    let schema: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        AuthorityClass::LocalImport.as_str(),
        schema["properties"]["record"]["anyOf"][0]["properties"]["authority_class"]["const"]
            .as_str()
            .unwrap()
    );
    let values = schema["properties"]["record"]["anyOf"][0]["properties"]["publisher_claim"]
        ["enum"]
        .as_array()
        .unwrap();
    for p in [PublisherClaim::None, PublisherClaim::Unverified] {
        assert!(values.contains(&serde_json::json!(p.as_str())));
    }
}
#[test]
fn staged_descriptor_cannot_grant_trust() {
    let tmp = common::TempDir::new();
    let source = tmp.0.join("source.zip");
    let stage = tmp.0.join("stage");
    let entries = common::package_entries();
    std::fs::write(&source, common::zip(&entries, &[], &[], &[])).unwrap();
    std::fs::create_dir(&stage).unwrap();
    let staged = kohbor_pack::container::stage_package(
        &source,
        &stage,
        &common::catalog(),
        &kohbor_pack::Limits::default(),
    )
    .unwrap();
    let result = assess(&staged.parsed.descriptor);
    assert_eq!(result.authority_class, AuthorityClass::LocalImport);
    assert_eq!(result.signature, SignatureStatus::Unavailable);
    assert!(!result.publisher_trusted());
    assert!(!result.purchase_entitled());
}
#[test]
fn package_authority_field_is_rejected() {
    for key in [
        "trusted_public_key",
        "authority_class",
        "publisher_trusted",
        "purchase_entitled",
    ] {
        let mut v = common::descriptor();
        v[key] = serde_json::json!("local_import");
        assert_eq!(
            kohbor_pack::descriptor::parse_descriptor(
                &serde_json::to_vec(&v).unwrap(),
                &common::catalog(),
                &kohbor_pack::Limits::default()
            )
            .unwrap_err(),
            kohbor_pack::RejectReason::UnknownField(key.into())
        );
    }
}
