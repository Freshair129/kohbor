#![allow(dead_code)]
use kohbor_pack::catalog::{Catalog, CatalogEvent, TaxonomyVersion};
use serde_json::Value;
use std::path::PathBuf;

pub fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .unwrap()
}
pub fn catalog() -> Catalog {
    Catalog {
        taxonomy: TaxonomyVersion { major: 1, minor: 1 },
        events: vec![
            CatalogEvent {
                id: "multi_kill".into(),
                media_mappable: true,
                tier_range: Some((2, 5)),
            },
            CatalogEvent {
                id: "streak".into(),
                media_mappable: true,
                tier_range: Some((3, 10)),
            },
            CatalogEvent {
                id: "first_blood".into(),
                media_mappable: true,
                tier_range: None,
            },
            CatalogEvent {
                id: "non_mappable".into(),
                media_mappable: false,
                tier_range: None,
            },
        ],
    }
}
pub fn load_catalog() -> Catalog {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/event-catalog.json");
    let doc: Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    Catalog::from_json(&doc).unwrap()
}
pub fn descriptor() -> Value {
    serde_json::from_slice(&fixture("media-tiered-bindings-valid.json")).unwrap()
}
pub fn wav(samples: usize) -> Vec<u8> {
    let data_len = u32::try_from(samples * 2).unwrap();
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&48_000u32.to_le_bytes());
    out.extend_from_slice(&96_000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(44 + data_len as usize, 0);
    out
}

use flate2::{write::DeflateEncoder, Compression, Crc};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
pub struct TempDir(pub PathBuf);
impl TempDir {
    pub fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        loop {
            let name = format!(
                "kohbor-pack-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            );
            let p = root.join(name);
            match std::fs::create_dir(&p) {
                Ok(()) => return Self(p),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("temporary directory: {e}"),
            }
        }
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[derive(Clone)]
pub struct ZipEntry {
    pub name: String,
    pub bytes: Vec<u8>,
    pub method: u16,
    pub flags: u16,
    pub local_name: Option<String>,
    pub local_method: Option<u16>,
    pub local_flags: Option<u16>,
    pub local_crc: Option<u32>,
    pub local_size: Option<u32>,
    pub declared_size: Option<u32>,
    pub time: u16,
    pub date: u16,
    pub mode: u32,
    pub extra: Vec<u8>,
    pub comment: Vec<u8>,
    pub gap: Vec<u8>,
}
impl ZipEntry {
    pub fn new(name: &str, bytes: Vec<u8>, method: u16) -> Self {
        Self {
            name: name.into(),
            bytes,
            method,
            flags: 0,
            local_name: None,
            local_method: None,
            local_flags: None,
            local_crc: None,
            local_size: None,
            declared_size: None,
            time: 0,
            date: 33,
            mode: 0o100644,
            extra: vec![],
            comment: vec![],
            gap: vec![],
        }
    }
}
fn le16(out: &mut Vec<u8>, n: u16) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn le32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn u16len(n: usize) -> u16 {
    u16::try_from(n).unwrap()
}
fn u32len(n: usize) -> u32 {
    u32::try_from(n).unwrap()
}
/// Indexed ZIP with explicit metadata knobs; deflate is raw DEFLATE at level six.
pub fn zip(entries: &[ZipEntry], prefix: &[u8], archive_comment: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut out = prefix.to_vec();
    let mut central = Vec::new();
    for e in entries {
        let offset = u32len(out.len());
        let mut crc = Crc::new();
        crc.update(&e.bytes);
        let crc = crc.sum();
        let compressed = if e.method == 8 {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(6));
            encoder.write_all(&e.bytes).unwrap();
            encoder.finish().unwrap()
        } else {
            e.bytes.clone()
        };
        let csize = u32len(compressed.len());
        let usize = e.declared_size.unwrap_or_else(|| u32len(e.bytes.len()));
        let local_name = e.local_name.as_deref().unwrap_or(&e.name);
        le32(&mut out, 0x04034b50);
        le16(&mut out, 20);
        le16(&mut out, e.local_flags.unwrap_or(e.flags));
        le16(&mut out, e.local_method.unwrap_or(e.method));
        le16(&mut out, e.time);
        le16(&mut out, e.date);
        le32(&mut out, e.local_crc.unwrap_or(crc));
        le32(&mut out, csize);
        le32(&mut out, e.local_size.unwrap_or(usize));
        le16(&mut out, u16len(local_name.len()));
        le16(&mut out, u16len(e.extra.len()));
        out.extend_from_slice(local_name.as_bytes());
        out.extend_from_slice(&e.extra);
        out.extend_from_slice(&compressed);
        out.extend_from_slice(&e.gap);
        le32(&mut central, 0x02014b50);
        le16(&mut central, 0x0314);
        le16(&mut central, 20);
        le16(&mut central, e.flags);
        le16(&mut central, e.method);
        le16(&mut central, e.time);
        le16(&mut central, e.date);
        le32(&mut central, crc);
        le32(&mut central, csize);
        le32(&mut central, usize);
        le16(&mut central, u16len(e.name.len()));
        le16(&mut central, u16len(e.extra.len()));
        le16(&mut central, u16len(e.comment.len()));
        le16(&mut central, 0);
        le16(&mut central, 0);
        le32(&mut central, e.mode << 16);
        le32(&mut central, offset);
        central.extend_from_slice(e.name.as_bytes());
        central.extend_from_slice(&e.extra);
        central.extend_from_slice(&e.comment);
    }
    let offset = u32len(out.len());
    out.extend_from_slice(&central);
    le32(&mut out, 0x06054b50);
    le16(&mut out, 0);
    le16(&mut out, 0);
    le16(&mut out, u16len(entries.len()));
    le16(&mut out, u16len(entries.len()));
    le32(&mut out, u32len(central.len()));
    le32(&mut out, offset);
    le16(&mut out, u16len(archive_comment.len()));
    out.extend_from_slice(archive_comment);
    out.extend_from_slice(suffix);
    out
}
fn element(id: &[u8], data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut out = id.to_vec();
    if n < 127 {
        out.push(0x80 | n as u8);
    } else {
        assert!(n < 16383);
        out.push(0x40 | ((n >> 8) as u8));
        out.push(n as u8);
    }
    out.extend_from_slice(data);
    out
}
fn uint(id: &[u8], n: u64) -> Vec<u8> {
    let b = n.to_be_bytes();
    let first = b.iter().position(|x| *x != 0).unwrap_or(7);
    element(id, &b[first..])
}
/// Minimal EBML document with controllable tracks and declared duration.
pub fn webm(
    codec: &str,
    width: u32,
    height: u32,
    extra_track: Option<u8>,
    duration: Option<f64>,
    default_duration: Option<u64>,
) -> Vec<u8> {
    let mut header = element(&[0x42, 0x82], b"webm");
    header.extend(uint(&[0x42, 0x86], 1));
    let mut video = uint(&[0xb0], u64::from(width));
    video.extend(uint(&[0xba], u64::from(height)));
    let mut track = uint(&[0xd7], 1);
    track.extend(uint(&[0x73, 0xc5], 1));
    track.extend(uint(&[0x83], 1));
    track.extend(element(&[0x86], codec.as_bytes()));
    track.extend(element(&[0xe0], &video));
    if let Some(n) = default_duration {
        track.extend(uint(&[0x23, 0xe3, 0x83], n));
    }
    let mut tracks = element(&[0xae], &track);
    if let Some(t) = extra_track {
        let mut extra = uint(&[0xd7], 2);
        extra.extend(uint(&[0x73, 0xc5], 2));
        extra.extend(uint(&[0x83], u64::from(t)));
        extra.extend(element(
            &[0x86],
            if t == 2 { b"A_OPUS" } else { b"S_TEXT/UTF8" },
        ));
        tracks.extend(element(&[0xae], &extra));
    }
    let mut segment = Vec::new();
    if let Some(d) = duration {
        let mut info = uint(&[0x2a, 0xd7, 0xb1], 1_000_000);
        info.extend(element(&[0x44, 0x89], &d.to_be_bytes()));
        segment.extend(element(&[0x15, 0x49, 0xa9, 0x66], &info));
    }
    segment.extend(element(&[0x16, 0x54, 0xae, 0x6b], &tracks));
    let mut out = element(&[0x1a, 0x45, 0xdf, 0xa3], &header);
    out.extend(element(&[0x18, 0x53, 0x80, 0x67], &segment));
    out
}
/// One second of silent audio and a silent VP9 track, with matching inventory.
pub fn package_entries() -> Vec<ZipEntry> {
    let audio = wav(48_000);
    let video = webm("V_VP9", 640, 360, None, Some(1000.0), None);
    let descriptor = serde_json::json!({"schema_version":"1.2.0","package_id":"test-pack","revision":1,"taxonomy_version":"1.1","display_name":"Test","publisher_key_id":null,"files":[{"path":"audio/cue.wav","size_bytes":audio.len(),"sha256":hash(&audio),"role":"mixed_audio"},{"path":"video/cue.webm","size_bytes":video.len(),"sha256":hash(&video),"role":"silent_visual"}],"bindings":[{"event_id":"first_blood","mixed_audio_path":"audio/cue.wav","silent_visual_path":"video/cue.webm","duration_ms":1000,"visual_tail_ms":0}]});
    vec![
        ZipEntry::new(
            "descriptor.json",
            serde_json::to_vec(&descriptor).unwrap(),
            8,
        ),
        ZipEntry::new("audio/cue.wav", audio, 8),
        ZipEntry::new("video/cue.webm", video, 0),
    ]
}

pub fn store_package(
    dir: &std::path::Path,
    name: &str,
    revision: u64,
    publisher: bool,
) -> (PathBuf, Vec<u8>) {
    let mut entries = package_entries();
    let mut descriptor: Value = serde_json::from_slice(&entries[0].bytes).unwrap();
    descriptor["package_id"] = name.into();
    descriptor["revision"] = revision.into();
    if publisher {
        descriptor["publisher_key_id"] = "publisher-key".into();
    }
    entries[0].bytes = serde_json::to_vec(&descriptor).unwrap();
    let bytes = zip(&entries, &[], &[], &[]);
    let path = dir.join(format!("{name}-{revision}.zip"));
    std::fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

pub fn request(path: PathBuf, bytes: &[u8], id: &str) -> kohbor_pack::store::InstallRequest {
    kohbor_pack::store::InstallRequest {
        request_id: id.into(),
        profile_id: "default".into(),
        source_path: path,
        expected_container_sha256: hash(bytes),
        project_binding: None,
        activation_policy: kohbor_pack::store::ActivationPolicy::Never,
    }
}

pub fn tree(root: &std::path::Path) -> Vec<PathBuf> {
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        if !dir.exists() {
            return;
        }
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            out.push(path.strip_prefix(root).unwrap().to_path_buf());
            if path.is_dir() {
                walk(root, &path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

pub fn tiered_package(dir: &std::path::Path) -> (PathBuf, Vec<u8>) {
    let mut entries = package_entries();
    let mut descriptor: Value = serde_json::from_slice(&entries[0].bytes).unwrap();
    let audio = entries[1].bytes.clone();
    let video = entries[2].bytes.clone();
    let mut files = Vec::new();
    let mut bindings = Vec::new();
    for (index, event, tier) in [
        (1, "multi_kill", Some(2)),
        (2, "multi_kill", Some(3)),
        (3, "streak", None),
        (4, "future_event", None),
    ] {
        let audio_path = format!("audio/cue{index}.wav");
        let video_path = format!("video/cue{index}.webm");
        files.push(serde_json::json!({"path":audio_path,"size_bytes":audio.len(),"sha256":hash(&audio),"role":"mixed_audio"}));
        files.push(serde_json::json!({"path":video_path,"size_bytes":video.len(),"sha256":hash(&video),"role":"silent_visual"}));
        let mut binding = serde_json::json!({"event_id":event,"mixed_audio_path":audio_path,"silent_visual_path":video_path,"duration_ms":1000,"visual_tail_ms":0});
        if let Some(tier) = tier {
            binding["tier"] = tier.into();
        }
        if index == 1 {
            binding["identity_audio_path"] = "audio/identity.wav".into();
        }
        bindings.push(binding);
        entries.push(ZipEntry::new(&audio_path, audio.clone(), 8));
        entries.push(ZipEntry::new(&video_path, video.clone(), 0));
    }
    files.push(serde_json::json!({"path":"audio/identity.wav","size_bytes":audio.len(),"sha256":hash(&audio),"role":"identity_audio"}));
    entries.push(ZipEntry::new("audio/identity.wav", audio, 8));
    entries.remove(2);
    entries.remove(1);
    descriptor["files"] = files.into();
    descriptor["bindings"] = bindings.into();
    entries[0].bytes = serde_json::to_vec(&descriptor).unwrap();
    let bytes = zip(&entries, &[], &[], &[]);
    let path = dir.join("tiered.zip");
    std::fs::write(&path, &bytes).unwrap();
    (path, bytes)
}
