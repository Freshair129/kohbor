mod common;
use kohbor_pack::{
    container::{sha256_file, stage_package},
    Limits, RejectReason,
};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
#[test]
fn missing_source_is_unavailable() {
    let tmp = common::TempDir::new();
    let missing = tmp.0.join("missing.zip");
    let stage = tmp.0.join("stage");
    std::fs::create_dir(&stage).unwrap();
    assert_eq!(
        stage_package(&missing, &stage, &common::catalog(), &Limits::default()).unwrap_err(),
        RejectReason::SourceUnavailable
    );
}
#[test]
fn missing_file_digest_is_unavailable() {
    let tmp = common::TempDir::new();
    assert_eq!(
        sha256_file(&tmp.0.join("missing")).unwrap_err(),
        RejectReason::SourceUnavailable
    );
}
#[test]
fn file_digest_matches_independent_sha256() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/media-tiered-bindings-valid.json");
    let expected = format!("{:x}", Sha256::digest(std::fs::read(&path).unwrap()));
    assert_eq!(sha256_file(&path).unwrap(), expected);
}

use common::{package_entries, zip, ZipEntry};
fn check_archive(bytes: Vec<u8>, limits: Limits, expected: RejectReason) {
    let tmp = common::TempDir::new();
    let source = tmp.0.join("source.zip");
    let staging = tmp.0.join("stage");
    std::fs::write(&source, bytes).unwrap();
    std::fs::create_dir(&staging).unwrap();
    let result = stage_package(&source, &staging, &common::catalog(), &limits);
    let mut names: Vec<_> = std::fs::read_dir(&tmp.0)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            std::ffi::OsString::from("source.zip"),
            std::ffi::OsString::from("stage")
        ]
    );
    fn check_sizes(dir: &std::path::Path, root: &std::path::Path, max: u64, descriptor_max: u64) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let meta = std::fs::symlink_metadata(entry.path()).unwrap();
            assert!(!meta.file_type().is_symlink());
            if meta.is_dir() {
                check_sizes(&entry.path(), root, max, descriptor_max)
            } else {
                let limit = if entry.path() == root.join("descriptor.json") {
                    descriptor_max
                } else {
                    max
                };
                assert!(meta.len() <= limit);
            }
        }
    }
    check_sizes(
        &staging,
        &staging,
        limits.payload_bytes,
        limits.descriptor_bytes,
    );
    assert_eq!(result.unwrap_err(), expected);
}
fn archive(entries: &[ZipEntry]) -> Vec<u8> {
    zip(entries, &[], &[], &[])
}
#[test]
fn valid_package_stages() {
    let tmp = common::TempDir::new();
    let source = tmp.0.join("source.zip");
    let stage = tmp.0.join("stage");
    let entries = package_entries();
    let bytes = archive(&entries);
    std::fs::write(&source, &bytes).unwrap();
    std::fs::create_dir(&stage).unwrap();
    let result = stage_package(&source, &stage, &common::catalog(), &Limits::default()).unwrap();
    assert_eq!(result.container_sha256, common::hash(&bytes));
    assert_eq!(result.parsed.descriptor.package_id, "test-pack");
    assert_eq!(result.files.len(), 2);
    assert!(stage.join("descriptor.json").is_file());
    for e in &entries[1..] {
        let f = result.files.iter().find(|f| f.path == e.name).unwrap();
        assert_eq!(f.size_bytes, e.bytes.len() as u64);
        assert_eq!(f.sha256, common::hash(&e.bytes));
        assert_eq!(std::fs::read(stage.join(&e.name)).unwrap(), e.bytes);
    }
}
macro_rules! zip_mutation {
    ($name:ident,$change:expr,$reason:expr) => {
        #[test]
        fn $name() {
            let mut e = package_entries();
            ($change)(&mut e);
            check_archive(archive(&e), Limits::default(), $reason);
        }
    };
}
macro_rules! bad_bytes {
    ($name:ident,$bytes:expr,$reason:expr) => {
        #[test]
        fn $name() {
            check_archive($bytes, Limits::default(), $reason);
        }
    };
}
bad_bytes!(
    random_not_zip,
    b"random data".to_vec(),
    RejectReason::NotAZip
);
bad_bytes!(empty_not_zip, Vec::new(), RejectReason::NotAZip);
bad_bytes!(
    zip64_not_supported,
    b"PK\x06\x06\x00\x00\x00\x00".to_vec(),
    RejectReason::NotAZip
);
#[test]
fn missing_end_record() {
    let mut b = archive(&package_entries());
    b.truncate(b.len() - 22);
    check_archive(b, Limits::default(), RejectReason::NotAZip);
}
#[test]
fn truncated_zip() {
    let b = archive(&package_entries());
    check_archive(
        b[..b.len() / 2].to_vec(),
        Limits::default(),
        RejectReason::NotAZip,
    );
}
#[test]
fn hidden_prefix() {
    check_archive(
        zip(&package_entries(), b"x", &[], &[]),
        Limits::default(),
        RejectReason::HiddenBytes,
    );
}
#[test]
fn hidden_suffix() {
    check_archive(
        zip(&package_entries(), &[], &[], b"x"),
        Limits::default(),
        RejectReason::HiddenBytes,
    );
}
#[test]
fn archive_comment() {
    check_archive(
        zip(&package_entries(), &[], b"x", &[]),
        Limits::default(),
        RejectReason::ArchiveComment,
    );
}
zip_mutation!(
    hidden_gap,
    |e: &mut Vec<ZipEntry>| {
        e[0].gap = vec![1];
    },
    RejectReason::HiddenBytes
);
zip_mutation!(
    entry_comment,
    |e: &mut Vec<ZipEntry>| {
        e[0].comment = vec![1];
    },
    RejectReason::UnexpectedExtraField
);
zip_mutation!(
    entry_extra,
    |e: &mut Vec<ZipEntry>| {
        e[0].extra = vec![1, 0, 0, 0];
    },
    RejectReason::UnexpectedExtraField
);
zip_mutation!(
    encrypted,
    |e: &mut Vec<ZipEntry>| {
        e[0].flags = 1;
    },
    RejectReason::EncryptedEntry
);
zip_mutation!(
    strong_encrypted,
    |e: &mut Vec<ZipEntry>| {
        e[0].flags = 0x40;
    },
    RejectReason::EncryptedEntry
);
zip_mutation!(
    compression_bzip2,
    |e: &mut Vec<ZipEntry>| {
        e[0].method = 12;
    },
    RejectReason::UnsupportedCompression
);
zip_mutation!(
    compression_lzma,
    |e: &mut Vec<ZipEntry>| {
        e[0].method = 14;
    },
    RejectReason::UnsupportedCompression
);
zip_mutation!(
    duplicate_name,
    |e: &mut Vec<ZipEntry>| {
        e[2].name = e[1].name.clone();
    },
    RejectReason::DuplicateEntry
);
zip_mutation!(
    duplicate_name_case,
    |e: &mut Vec<ZipEntry>| {
        e[2].name = e[1].name.to_uppercase();
    },
    RejectReason::DuplicateEntry
);
zip_mutation!(
    unsafe_traversal,
    |e: &mut Vec<ZipEntry>| {
        e[1].name = "../evil".into();
    },
    RejectReason::UnsafePath
);
zip_mutation!(
    unsafe_absolute,
    |e: &mut Vec<ZipEntry>| {
        e[1].name = "/evil".into();
    },
    RejectReason::UnsafePath
);
zip_mutation!(
    unsafe_unc,
    |e: &mut Vec<ZipEntry>| {
        e[1].name = "\\\\host\\evil".into();
    },
    RejectReason::UnsafePath
);
zip_mutation!(
    unsafe_backslash,
    |e: &mut Vec<ZipEntry>| {
        e[1].name = "audio\\evil".into();
    },
    RejectReason::UnsafePath
);
zip_mutation!(
    unsafe_directory,
    |e: &mut Vec<ZipEntry>| {
        e[1].name = "audio/".into();
    },
    RejectReason::UnsafePath
);
zip_mutation!(
    unsafe_drive,
    |e: &mut Vec<ZipEntry>| {
        e[1].name = "C:/evil".into();
    },
    RejectReason::UnsafePath
);
zip_mutation!(
    descriptor_not_first,
    |e: &mut Vec<ZipEntry>| {
        e.swap(0, 1);
    },
    RejectReason::DescriptorNotFirst
);
zip_mutation!(
    wrong_order,
    |e: &mut Vec<ZipEntry>| {
        e.swap(1, 2);
    },
    RejectReason::HeaderMismatch
);
zip_mutation!(
    wrong_timestamp,
    |e: &mut Vec<ZipEntry>| {
        e[0].date = 34;
    },
    RejectReason::HeaderMismatch
);
zip_mutation!(
    wrong_permissions,
    |e: &mut Vec<ZipEntry>| {
        e[0].mode = 0o100600;
    },
    RejectReason::HeaderMismatch
);
zip_mutation!(
    local_name_disagrees,
    |e: &mut Vec<ZipEntry>| {
        e[0].local_name = Some("descriptox.json".into());
    },
    RejectReason::HeaderMismatch
);
zip_mutation!(
    local_method_disagrees,
    |e: &mut Vec<ZipEntry>| {
        e[0].local_method = Some(0);
    },
    RejectReason::HeaderMismatch
);
zip_mutation!(
    local_crc_disagrees,
    |e: &mut Vec<ZipEntry>| {
        e[0].local_crc = Some(0);
    },
    RejectReason::HeaderMismatch
);
zip_mutation!(
    local_size_disagrees,
    |e: &mut Vec<ZipEntry>| {
        e[0].local_size = Some(1);
    },
    RejectReason::HeaderMismatch
);
zip_mutation!(
    local_flags_disagree,
    |e: &mut Vec<ZipEntry>| {
        e[0].local_flags = Some(2);
    },
    RejectReason::HeaderMismatch
);
#[test]
fn container_too_large() {
    let b = archive(&package_entries());
    let l = Limits {
        container_bytes: (b.len() - 1) as u64,
        ..Limits::default()
    };
    check_archive(b, l, RejectReason::ContainerTooLarge);
}
#[test]
fn entry_count_limit() {
    let e = package_entries();
    let l = Limits {
        payload_count: 1,
        ..Limits::default()
    };
    check_archive(archive(&e), l, RejectReason::TooManyEntries);
}
#[test]
fn entry_size_limit() {
    let e = package_entries();
    let l = Limits {
        payload_bytes: 128,
        ..Limits::default()
    };
    check_archive(archive(&e), l, RejectReason::EntryTooLarge);
}
#[test]
fn aggregate_expansion_limit() {
    let e = package_entries();
    let l = Limits {
        expanded_bytes: 4096,
        ..Limits::default()
    };
    check_archive(archive(&e), l, RejectReason::ExpansionLimit);
}
zip_mutation!(
    expansion_bomb,
    |e: &mut Vec<ZipEntry>| {
        e[1].declared_size = Some(1);
    },
    RejectReason::ExpansionLimit
);
fn set_descriptor(e: &mut [ZipEntry], f: impl FnOnce(&mut serde_json::Value)) {
    let mut v: serde_json::Value = serde_json::from_slice(&e[0].bytes).unwrap();
    f(&mut v);
    e[0].bytes = serde_json::to_vec(&v).unwrap();
}
fn replace_payload(e: &mut [ZipEntry], index: usize, bytes: Vec<u8>) {
    let size = bytes.len();
    let digest = common::hash(&bytes);
    e[index].bytes = bytes;
    set_descriptor(e, |v| {
        v["files"][index - 1]["size_bytes"] = serde_json::json!(size);
        v["files"][index - 1]["sha256"] = serde_json::json!(digest);
    });
}
#[test]
fn undeclared_payload() {
    let mut e = package_entries();
    e.push(ZipEntry::new("z-extra.txt", b"x".to_vec(), 8));
    check_archive(
        archive(&e),
        Limits::default(),
        RejectReason::UndeclaredPayload,
    );
}
#[test]
fn missing_inventory_payload() {
    let mut e = package_entries();
    e.pop();
    check_archive(
        archive(&e),
        Limits::default(),
        RejectReason::DanglingReference,
    );
}
#[test]
fn inventory_size_mismatch() {
    let mut e = package_entries();
    set_descriptor(&mut e, |v| {
        v["files"][0]["size_bytes"] = serde_json::json!(1)
    });
    check_archive(archive(&e), Limits::default(), RejectReason::SizeMismatch);
}
#[test]
fn inventory_digest_mismatch() {
    let mut e = package_entries();
    e[1].bytes[50] ^= 1;
    check_archive(archive(&e), Limits::default(), RejectReason::DigestMismatch);
}
#[test]
fn invalid_mixed_with_valid_identity() {
    let mut e = package_entries();
    replace_payload(&mut e, 1, b"bad wav".to_vec());
    let identity = common::wav(48_000);
    let size = identity.len();
    let digest = common::hash(&identity);
    set_descriptor(&mut e, |v| {
        v["files"].as_array_mut().unwrap().push(serde_json::json!({"path":"audio/identity.wav","size_bytes":size,"sha256":digest,"role":"identity_audio"}));
        v["bindings"][0]["identity_audio_path"] = serde_json::json!("audio/identity.wav");
    });
    e.insert(2, ZipEntry::new("audio/identity.wav", identity, 8));
    check_archive(
        archive(&e),
        Limits::default(),
        RejectReason::MixedAudioInvalid,
    );
}
#[test]
fn identity_length_mismatch() {
    let mut e = package_entries();
    let identity = common::wav(47_999);
    let size = identity.len();
    let digest = common::hash(&identity);
    set_descriptor(&mut e, |v| {
        v["files"].as_array_mut().unwrap().push(serde_json::json!({"path":"audio/identity.wav","size_bytes":size,"sha256":digest,"role":"identity_audio"}));
        v["bindings"][0]["identity_audio_path"] = serde_json::json!("audio/identity.wav");
    });
    e.insert(2, ZipEntry::new("audio/identity.wav", identity, 8));
    check_archive(
        archive(&e),
        Limits::default(),
        RejectReason::IdentityAudioLengthMismatch,
    );
}
#[test]
fn audio_duration_mismatch() {
    let mut e = package_entries();
    set_descriptor(&mut e, |v| {
        v["bindings"][0]["duration_ms"] = serde_json::json!(999)
    });
    check_archive(
        archive(&e),
        Limits::default(),
        RejectReason::AudioDurationMismatch,
    );
}
#[test]
fn visual_with_audio() {
    let mut e = package_entries();
    replace_payload(
        &mut e,
        2,
        common::webm("V_VP9", 640, 360, Some(2), Some(1000.0), None),
    );
    check_archive(
        archive(&e),
        Limits::default(),
        RejectReason::VisualHasAudioTrack,
    );
}
#[test]
fn visual_duration_too_short() {
    let mut e = package_entries();
    replace_payload(
        &mut e,
        2,
        common::webm("V_VP9", 640, 360, None, Some(965.0), None),
    );
    check_archive(
        archive(&e),
        Limits::default(),
        RejectReason::VisualDurationMismatch,
    );
}
#[test]
fn visual_duration_too_long() {
    let mut e = package_entries();
    replace_payload(
        &mut e,
        2,
        common::webm("V_VP9", 640, 360, None, Some(1035.0), None),
    );
    check_archive(
        archive(&e),
        Limits::default(),
        RejectReason::VisualDurationMismatch,
    );
}
#[test]
fn visual_duration_inside_tolerance() {
    let mut e = package_entries();
    replace_payload(
        &mut e,
        2,
        common::webm("V_VP9", 640, 360, None, Some(1034.0), None),
    );
    let tmp = common::TempDir::new();
    let source = tmp.0.join("source.zip");
    let stage = tmp.0.join("stage");
    std::fs::write(&source, archive(&e)).unwrap();
    std::fs::create_dir(&stage).unwrap();
    assert!(stage_package(&source, &stage, &common::catalog(), &Limits::default()).is_ok());
}
#[test]
fn inert_binding_still_verified() {
    let mut e = package_entries();
    set_descriptor(&mut e, |v| {
        v["bindings"][0]["event_id"] = serde_json::json!("future_event")
    });
    replace_payload(&mut e, 1, b"bad".to_vec());
    check_archive(
        archive(&e),
        Limits::default(),
        RejectReason::MixedAudioInvalid,
    );
}
#[test]
fn zip_builder_self_test() {
    use std::io::Read;
    let data = b"hello world";
    let b = archive(&[ZipEntry::new("descriptor.json", data.to_vec(), 8)]);
    assert_eq!(&b[..4], b"PK\x03\x04");
    let n = u16::from_le_bytes([b[26], b[27]]) as usize;
    let size = u32::from_le_bytes(b[18..22].try_into().unwrap()) as usize;
    let start = 30 + n;
    let mut decoded = Vec::new();
    flate2::read::DeflateDecoder::new(&b[start..start + size])
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, data);
    assert_eq!(&b[b.len() - 22..b.len() - 18], b"PK\x05\x06");
}
