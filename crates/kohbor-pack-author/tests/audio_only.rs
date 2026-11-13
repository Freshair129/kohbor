use kohbor_pack::{catalog::Catalog, container::stage_package, Limits};
use kohbor_pack_author::{build_package, parse_cli_args, BuildOptions};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        let index = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = base.join(format!(
            "kohbor-pack-author-audio-only-{}-{index}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create isolated test directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    temp: TempDir,
    clips: PathBuf,
    output: PathBuf,
    mapping: PathBuf,
    catalog: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new();
        let clips = temp.0.join("clips");
        let output = temp.0.join("output");
        fs::create_dir_all(&clips).expect("create clip directory");
        let mapping = temp.0.join("mapping.json");
        let catalog = temp.0.join("event-catalog.json");
        fs::write(&catalog, catalog_bytes()).expect("write catalog");
        fs::write(clips.join("cue.wav"), pcm_wav(480)).expect("write WAV input");
        fs::write(clips.join("banner.webm"), webm(false)).expect("write silent visual");
        Self {
            temp,
            clips,
            output,
            mapping,
            catalog,
        }
    }

    fn set_mapping(&self, bindings: &[Value]) {
        fs::write(&self.mapping, mapping_bytes(bindings)).expect("write mapping");
    }

    fn options(&self, out_dir: &Path) -> BuildOptions {
        BuildOptions {
            clips_dir: self.clips.clone(),
            map_path: self.mapping.clone(),
            out_dir: out_dir.to_path_buf(),
            package_id: "audio-only-pack".to_owned(),
            revision: 1,
            display_name: "Audio Only Pack".to_owned(),
            catalog_path: self.catalog.clone(),
            rights_claim: None,
            publisher_key_id: None,
            require_visual: false,
        }
    }

    fn cli_args(&self, out_dir: &Path, require_visual: bool) -> Vec<OsString> {
        let mut args = Vec::new();
        if require_visual {
            args.push(OsString::from("--require-visual"));
        }
        for (flag, value) in [
            ("--clips", self.clips.as_path()),
            ("--map", self.mapping.as_path()),
            ("--out", out_dir),
            ("--package-id", Path::new("audio-only-pack")),
            ("--revision", Path::new("1")),
            ("--display-name", Path::new("Audio Only Pack")),
            ("--catalog", self.catalog.as_path()),
        ] {
            args.push(OsString::from(flag));
            args.push(value.as_os_str().to_owned());
        }
        args
    }
}

fn pcm_wav(samples: usize) -> Vec<u8> {
    let data_len = u32::try_from(samples.saturating_mul(2)).expect("small test WAV");
    let riff_len = 36u32.checked_add(data_len).expect("small test WAV");
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&riff_len.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&96_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes.resize(44 + usize::try_from(data_len).expect("small test WAV"), 0);
    bytes
}

fn ebml_element(id: &[u8], data: &[u8]) -> Vec<u8> {
    let mut bytes = id.to_vec();
    let size = u8::try_from(data.len()).expect("small EBML test element");
    bytes.push(0x80 | size);
    bytes.extend_from_slice(data);
    bytes
}

fn ebml_uint(id: &[u8], value: u64) -> Vec<u8> {
    let data = value.to_be_bytes();
    let first = data
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(data.len() - 1);
    ebml_element(id, &data[first..])
}

fn webm(with_audio: bool) -> Vec<u8> {
    let mut header = ebml_element(&[0x42, 0x82], b"webm");
    header.extend(ebml_uint(&[0x42, 0x86], 1));
    let mut video = ebml_uint(&[0xb0], 640);
    video.extend(ebml_uint(&[0xba], 360));
    let mut video_track = ebml_uint(&[0xd7], 1);
    video_track.extend(ebml_uint(&[0x73, 0xc5], 1));
    video_track.extend(ebml_uint(&[0x83], 1));
    video_track.extend(ebml_element(&[0x86], b"V_VP9"));
    video_track.extend(ebml_element(&[0xe0], &video));
    let mut tracks = ebml_element(&[0xae], &video_track);
    if with_audio {
        let mut audio_track = ebml_uint(&[0xd7], 2);
        audio_track.extend(ebml_uint(&[0x73, 0xc5], 2));
        audio_track.extend(ebml_uint(&[0x83], 2));
        audio_track.extend(ebml_element(&[0x86], b"A_OPUS"));
        tracks.extend(ebml_element(&[0xae], &audio_track));
    }
    let segment = ebml_element(&[0x16, 0x54, 0xae, 0x6b], &tracks);
    let mut bytes = ebml_element(&[0x1a, 0x45, 0xdf, 0xa3], &header);
    bytes.extend(ebml_element(&[0x18, 0x53, 0x80, 0x67], &segment));
    bytes
}

fn catalog_bytes() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "taxonomy_version": "1.1",
        "events": [
            {"id":"first_blood","status":"active","media_mappable":true},
            {"id":"hero_kill","status":"active","media_mappable":true},
            {"id":"multi_kill","status":"active","media_mappable":true,"tier_range":{"min":2,"max":5}}
        ]
    }))
    .unwrap()
}

fn mapping_bytes(bindings: &[Value]) -> Vec<u8> {
    serde_json::to_vec(&json!({ "schema_version": "1.0.0", "bindings": bindings })).unwrap()
}

fn binding(event: &str, tier: Option<u32>, visual: Option<&str>, tail: Option<u32>) -> Value {
    let mut object = json!({ "event_id": event, "audio": "cue.wav" });
    if let Some(tier) = tier {
        object["tier"] = json!(tier);
    }
    if let Some(visual) = visual {
        object["visual"] = json!(visual);
    }
    if let Some(tail) = tail {
        object["visual_tail_ms"] = json!(tail);
    }
    object
}

fn stage_reported_package(path: &Path, temp: &Path) -> kohbor_pack::container::StagedPackage {
    let catalog_value: Value = serde_json::from_slice(&catalog_bytes()).unwrap();
    let catalog = Catalog::from_json(&catalog_value).unwrap();
    let index = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let stage_dir = temp.join(format!("staging-{index}"));
    fs::create_dir(&stage_dir).unwrap();
    stage_package(path, &stage_dir, &catalog, &Limits::default()).unwrap()
}

#[test]
fn audio_only_build_round_trips_deterministically_and_records_null_visual() {
    let fixture = Fixture::new();
    fixture.set_mapping(&[binding("hero_kill", None, None, None)]);
    let options = fixture.options(&fixture.output);
    let first = build_package(&options).expect("build audio-only package");
    let second_dir = fixture.temp.0.join("second-output");
    let second = build_package(&fixture.options(&second_dir)).expect("repeat deterministic build");
    assert_eq!(
        fs::read(&first.package_path).unwrap(),
        fs::read(&second.package_path).unwrap()
    );
    assert_eq!(
        fs::read(&first.provenance_path).unwrap(),
        fs::read(&second.provenance_path).unwrap()
    );
    assert_eq!(first.written.len(), 1);
    assert_eq!(first.written[0].visual, None);

    let staged = stage_reported_package(&first.package_path, &fixture.temp.0);
    assert_eq!(staged.parsed.descriptor.bindings.len(), 1);
    assert_eq!(
        staged.parsed.descriptor.bindings[0].silent_visual_path,
        None
    );
    assert!(staged
        .files
        .iter()
        .all(|file| !file.path.starts_with("video/")));
    let provenance: Value =
        serde_json::from_slice(&fs::read(&first.provenance_path).unwrap()).unwrap();
    assert_eq!(
        provenance["build_report"]["written"][0]["visual"],
        Value::Null
    );
}

#[test]
fn require_visual_skips_audio_only_mapping_and_reports_why() {
    let fixture = Fixture::new();
    fixture.set_mapping(&[
        binding("hero_kill", None, None, None),
        binding("multi_kill", Some(2), Some("banner.webm"), None),
    ]);
    let parsed = parse_cli_args(fixture.cli_args(&fixture.output, true)).unwrap();
    assert!(parsed.require_visual);
    let report = build_package(&parsed).expect("paired visual binding remains");
    assert_eq!(report.written.len(), 1);
    assert_eq!(report.skipped.len(), 1);
    assert!(report.skipped[0]
        .reason
        .contains("visual path is missing (--require-visual)"));
}

#[test]
fn invalid_given_visual_is_skipped_without_downgrading_and_tail_requires_visual() {
    let fixture = Fixture::new();
    fs::write(fixture.clips.join("bad.webm"), b"invalid visual").unwrap();
    fixture.set_mapping(&[
        binding("hero_kill", None, None, None),
        binding("multi_kill", Some(2), Some("bad.webm"), None),
    ]);
    let report = build_package(&fixture.options(&fixture.output)).unwrap();
    assert_eq!(report.written.len(), 1);
    assert_eq!(report.written[0].visual, None);
    assert_eq!(report.skipped.len(), 1);
    assert!(report.skipped[0]
        .reason
        .contains("visual failed silent WebM inspection"));
    let staged = stage_reported_package(&report.package_path, &fixture.temp.0);
    assert_eq!(staged.parsed.descriptor.bindings.len(), 1);
    assert_eq!(
        staged.parsed.descriptor.bindings[0].silent_visual_path,
        None
    );

    let tail_output = fixture.temp.0.join("tail-output");
    fixture.set_mapping(&[
        binding("hero_kill", None, None, Some(10)),
        binding("multi_kill", Some(2), Some("banner.webm"), None),
    ]);
    let tail_report = build_package(&fixture.options(&tail_output)).unwrap();
    assert_eq!(tail_report.written.len(), 1);
    assert_eq!(tail_report.skipped.len(), 1);
    assert!(tail_report.skipped[0]
        .reason
        .contains("audio-only binding must have visual_tail_ms 0"));
}

#[test]
fn binary_builds_audio_only_mapping_and_stages_output() {
    let fixture = Fixture::new();
    fixture.set_mapping(&[binding("hero_kill", None, None, None)]);
    let output = Command::new(env!("CARGO_BIN_EXE_kohbor-pack-build"))
        .args(fixture.cli_args(&fixture.output, false))
        .output()
        .expect("run package builder binary");
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["written"][0]["visual"], Value::Null);
    let package_path = fixture.output.join("audio-only-pack-r1.zip");
    let staged = stage_reported_package(&package_path, &fixture.temp.0);
    assert_eq!(
        staged.parsed.descriptor.bindings[0].silent_visual_path,
        None
    );
    assert!(staged
        .files
        .iter()
        .all(|file| !file.path.starts_with("video/")));
}
