mod common;

use kohbor_pack::{container::sha256_file, container::sha256_file_limited, RejectReason};

#[test]
fn limited_digest_hashes_a_file_exactly_at_the_limit() {
    let tmp = common::TempDir::new();
    let path = tmp.0.join("exact-limit.bin");
    std::fs::write(&path, b"hello").unwrap();

    assert_eq!(
        sha256_file_limited(&path, 5).unwrap(),
        (
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_owned(),
            5,
        )
    );
}

#[test]
fn limited_digest_rejects_a_file_over_the_limit() {
    let tmp = common::TempDir::new();
    let path = tmp.0.join("over-limit.bin");
    std::fs::write(&path, b"hello!").unwrap();

    assert_eq!(
        sha256_file_limited(&path, 5).unwrap_err(),
        RejectReason::SourceUnavailable
    );
}

#[test]
fn limited_digest_matches_unlimited_digest_for_a_small_file() {
    let tmp = common::TempDir::new();
    let path = tmp.0.join("small.bin");
    std::fs::write(&path, b"hello").unwrap();

    let (limited_digest, bytes_hashed) = sha256_file_limited(&path, 64).unwrap();

    assert_eq!(limited_digest, sha256_file(&path).unwrap());
    assert_eq!(bytes_hashed, 5);
}

#[test]
fn limited_digest_rejects_a_missing_file() {
    let tmp = common::TempDir::new();

    assert_eq!(
        sha256_file_limited(&tmp.0.join("missing.bin"), 5).unwrap_err(),
        RejectReason::SourceUnavailable
    );
}
