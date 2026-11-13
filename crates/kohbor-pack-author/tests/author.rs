use kohbor_pack::{catalog::Catalog, container::stage_package, Limits};
use kohbor_pack_author::{audio::convert_wav, build_package, BuildOptions};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        let index = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = base.join(format!("kohbor-pack-author-{}-{index}", std::process::id()));
        fs::create_dir_all(&path).expect("create test temp dir");
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
    out: PathBuf,
    map: PathBuf,
    catalog: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new();
        let clips = temp.0.join("clips");
        let out = temp.0.join("out");
        fs::create_dir_all(&clips).expect("create clips");
        let map = temp.0.join("mapping.json");
        let catalog = temp.0.join("event-catalog.json");
        fs::write(&catalog, catalog_bytes()).expect("write catalog");
        fs::write(
            &map,
            mapping_bytes(&[binding(
                "multi_kill",
                Some(2),
                Some("x.webm"),
                None,
                None,
                None,
            )]),
        )
        .expect("write mapping");
        fs::write(
            clips.join("x.wav"),
            pcm_wav(48_000, 1, 16, &vec![1024; 480]),
        )
        .expect("write audio");
        fs::write(clips.join("x.webm"), webm(None)).expect("write visual");
        Self {
            temp,
            clips,
            out,
            map,
            catalog,
        }
    }

    fn options(&self) -> BuildOptions {
        BuildOptions {
            clips_dir: self.clips.clone(),
            map_path: self.map.clone(),
            out_dir: self.out.clone(),
            package_id: "synthetic-pack".to_owned(),
            revision: 3,
            display_name: "Synthetic Pack".to_owned(),
            catalog_path: self.catalog.clone(),
            rights_claim: Some("synthetic owner claim".to_owned()),
            publisher_key_id: None,
            require_visual: false,
        }
    }

    fn set_mapping(&self, bindings: &[Value]) {
        fs::write(&self.map, mapping_bytes(bindings)).expect("write mapping");
    }
}

fn catalog_bytes() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "taxonomy_version": "1.1",
        "events": [
            { "id": "multi_kill", "status": "active", "media_mappable": true, "tier_range": { "min": 2, "max": 4 } },
            { "id": "hero_kill", "status": "active", "media_mappable": true },
            { "id": "hero_respawn", "status": "active", "media_mappable": false }
        ]
    }))
    .expect("serialize catalog")
}

fn binding(
    event_id: &str,
    tier: Option<u32>,
    visual: Option<&str>,
    identity_audio: Option<&str>,
    visual_tail_ms: Option<u32>,
    audio: Option<&str>,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("event_id".to_owned(), json!(event_id));
    value.insert("audio".to_owned(), json!(audio.unwrap_or("x.wav")));
    if let Some(tier) = tier {
        value.insert("tier".to_owned(), json!(tier));
    }
    if let Some(visual) = visual {
        value.insert("visual".to_owned(), json!(visual));
    }
    if let Some(identity_audio) = identity_audio {
        value.insert("identity_audio".to_owned(), json!(identity_audio));
    }
    if let Some(visual_tail_ms) = visual_tail_ms {
        value.insert("visual_tail_ms".to_owned(), json!(visual_tail_ms));
    }
    Value::Object(value)
}

fn mapping_bytes(bindings: &[Value]) -> Vec<u8> {
    serde_json::to_vec(&json!({ "schema_version": "1.0.0", "bindings": bindings }))
        .expect("serialize mapping")
}

fn pcm_wav(sample_rate: u32, channels: u16, bits: u16, samples: &[i32]) -> Vec<u8> {
    let bytes_per_sample = usize::from(bits / 8);
    let mut data = Vec::with_capacity(samples.len() * bytes_per_sample);
    for sample in samples {
        match bits {
            8 => data.push(*sample as u8),
            16 => data.extend_from_slice(&(*sample as i16).to_le_bytes()),
            24 => {
                let raw = *sample as u32;
                data.extend_from_slice(&raw.to_le_bytes()[..3]);
            }
            _ => {}
        }
    }
    wav_with_chunks(
        sample_rate,
        channels,
        1,
        bits,
        &[
            (&b"fmt "[..], fmt_data(channels, sample_rate, 1, bits)),
            (&b"data"[..], data),
        ],
    )
}

fn float_wav(sample_rate: u32, channels: u16, samples: &[f32]) -> Vec<u8> {
    let mut data = Vec::new();
    for sample in samples {
        data.extend_from_slice(&sample.to_le_bytes());
    }
    wav_with_chunks(
        sample_rate,
        channels,
        3,
        32,
        &[
            (&b"fmt "[..], fmt_data(channels, sample_rate, 3, 32)),
            (&b"data"[..], data),
        ],
    )
}

fn fmt_data(channels: u16, sample_rate: u32, format: u16, bits: u16) -> Vec<u8> {
    let bytes_per_frame = u32::from(channels) * u32::from(bits / 8);
    let byte_rate = sample_rate * bytes_per_frame;
    let block_align = u16::try_from(bytes_per_frame).expect("small align");
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&format.to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&sample_rate.to_le_bytes());
    fmt.extend_from_slice(&byte_rate.to_le_bytes());
    fmt.extend_from_slice(&block_align.to_le_bytes());
    fmt.extend_from_slice(&bits.to_le_bytes());
    fmt
}

fn wav_with_chunks(
    sample_rate: u32,
    channels: u16,
    format: u16,
    bits: u16,
    chunks: &[(&[u8], Vec<u8>)],
) -> Vec<u8> {
    let _ = (sample_rate, channels, format, bits);
    let mut body = b"WAVE".to_vec();
    for (id, bytes) in chunks {
        body.extend_from_slice(id);
        body.extend_from_slice(
            &u32::try_from(bytes.len())
                .expect("chunk size")
                .to_le_bytes(),
        );
        body.extend_from_slice(bytes);
        if bytes.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut result = b"RIFF".to_vec();
    result.extend_from_slice(&u32::try_from(body.len()).expect("riff size").to_le_bytes());
    result.extend_from_slice(&body);
    result
}

fn webm(audio_track: Option<bool>) -> Vec<u8> {
    let mut ebml_header = ebml_element(&[0x42, 0x82], b"webm");
    ebml_header.extend(ebml_uint(&[0x42, 0x86], 1));

    let mut video = ebml_uint(&[0xb0], 640);
    video.extend(ebml_uint(&[0xba], 360));
    let mut video_track = ebml_uint(&[0xd7], 1);
    video_track.extend(ebml_uint(&[0x73, 0xc5], 1));
    video_track.extend(ebml_uint(&[0x83], 1));
    video_track.extend(ebml_element(&[0x86], b"V_VP9"));
    video_track.extend(ebml_element(&[0xe0], &video));
    let mut tracks = ebml_element(&[0xae], &video_track);
    if audio_track == Some(true) {
        let mut audio = ebml_uint(&[0xd7], 2);
        audio.extend(ebml_uint(&[0x73, 0xc5], 2));
        audio.extend(ebml_uint(&[0x83], 2));
        audio.extend(ebml_element(&[0x86], b"A_OPUS"));
        tracks.extend(ebml_element(&[0xae], &audio));
    }
    let segment = ebml_element(&[0x16, 0x54, 0xae, 0x6b], &tracks);
    let mut result = ebml_element(&[0x1a, 0x45, 0xdf, 0xa3], &ebml_header);
    result.extend(ebml_element(&[0x18, 0x53, 0x80, 0x67], &segment));
    result
}

fn ebml_element(id: &[u8], bytes: &[u8]) -> Vec<u8> {
    let mut result = id.to_vec();
    result.extend(ebml_size(
        u64::try_from(bytes.len()).expect("small EBML element"),
    ));
    result.extend_from_slice(bytes);
    result
}

fn ebml_size(size: u64) -> Vec<u8> {
    for width in 1..=8usize {
        let bits = width * 7;
        let maximum = (1u64 << bits) - 2;
        if size <= maximum {
            let mut out = size.to_be_bytes()[8 - width..].to_vec();
            let first = out.first_mut().expect("size encoding has a byte");
            *first |= 1u8 << (8 - width);
            return out;
        }
    }
    Vec::new()
}

fn ebml_uint(id: &[u8], value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap_or(7);
    ebml_element(id, &bytes[first..])
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn output_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).expect("read generated output")
}

#[test]
fn resampler_44100_to_48000_preserves_length_and_sine_frequency() {
    let samples = (0..44_100)
        .map(|index| {
            let phase = std::f64::consts::TAU * 440.0 * f64::from(index) / 44_100.0;
            (phase.sin() * 24_000.0).round() as i32
        })
        .collect::<Vec<_>>();
    let converted = convert_wav(&pcm_wav(44_100, 1, 16, &samples)).expect("convert 44.1 kHz WAV");
    assert_eq!(converted.samples, 48_000);
    let data = converted.wav.get(44..).expect("PCM data");
    let values = data
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
        .collect::<Vec<_>>();
    let start = 2_000usize;
    let crossings = values
        .get(start..)
        .expect("bounded sine slice")
        .windows(2)
        .filter(|pair| pair[0] <= 0 && pair[1] > 0)
        .count();
    let measured_hz = crossings as f64 * 48_000.0 / (values.len() - start) as f64;
    assert!(
        (measured_hz - 440.0).abs() < 2.0,
        "measured {measured_hz} Hz"
    );
}

#[test]
fn passthrough_48khz_is_exact_and_stereo_is_averaged() {
    let mono = vec![-32_768, -1, 0, 1, 32_767];
    let converted = convert_wav(&pcm_wav(48_000, 1, 16, &mono)).expect("convert passthrough");
    let source_data = pcm_wav(48_000, 1, 16, &mono);
    assert_eq!(converted.wav.get(44..), source_data.get(44..));

    let stereo = [16_384, 0, -16_384, 0, 8_192, 8_192];
    let mixed = convert_wav(&pcm_wav(48_000, 2, 16, &stereo)).expect("convert stereo");
    let samples = mixed
        .wav
        .get(44..)
        .expect("stereo conversion data")
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
        .collect::<Vec<_>>();
    assert_eq!(samples, vec![8_192, -8_192, 8_192]);
}

#[test]
fn accepts_8_bit_24_bit_and_float_inputs_with_clipping() {
    let pcm8 = pcm_wav(48_000, 1, 8, &[0, 255]);
    let converted8 = convert_wav(&pcm8).expect("convert 8-bit");
    let out8 = converted8.wav.get(44..).expect("8-bit output");
    assert_eq!(i16::from_le_bytes([out8[0], out8[1]]), -32_768);
    assert_eq!(i16::from_le_bytes([out8[2], out8[3]]), 32_512);

    let pcm24 = pcm_wav(48_000, 1, 24, &[-8_388_608, 8_388_607]);
    let converted24 = convert_wav(&pcm24).expect("convert 24-bit");
    let out24 = converted24.wav.get(44..).expect("24-bit output");
    assert_eq!(i16::from_le_bytes([out24[0], out24[1]]), -32_768);
    assert_eq!(i16::from_le_bytes([out24[2], out24[3]]), 32_767);

    let floating = float_wav(48_000, 1, &[-2.0, -1.0, 0.0, 1.0, 2.0]);
    let converted_float = convert_wav(&floating).expect("convert float");
    let values = converted_float
        .wav
        .get(44..)
        .expect("float output")
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
        .collect::<Vec<_>>();
    assert_eq!(values, vec![-32_768, -32_768, 0, 32_767, 32_767]);
}

#[test]
fn extra_riff_chunks_and_chunk_order_are_supported() {
    let fmt = fmt_data(1, 48_000, 1, 16);
    let data = 1234i16.to_le_bytes().to_vec();
    let riff = wav_with_chunks(
        48_000,
        1,
        1,
        16,
        &[
            (&b"JUNK"[..], vec![1, 2, 3]),
            (&b"data"[..], data),
            (&b"fmt "[..], fmt),
        ],
    );
    let converted = convert_wav(&riff).expect("convert reordered RIFF");
    assert_eq!(converted.samples, 1);
    let value = i16::from_le_bytes([converted.wav[44], converted.wav[45]]);
    assert_eq!(value, 1234);
}

#[test]
fn silence_stays_silent_and_audio_clips_over_15_seconds_are_rejected() {
    let silence = vec![0; 44_100];
    let converted = convert_wav(&pcm_wav(44_100, 1, 16, &silence)).expect("convert silence");
    assert!(converted
        .wav
        .get(44..)
        .expect("silence data")
        .iter()
        .all(|byte| *byte == 0));

    let exact = vec![1; 720_000];
    assert_eq!(
        convert_wav(&pcm_wav(48_000, 1, 16, &exact))
            .expect("exact 15s")
            .samples,
        720_000
    );
    let too_long = vec![1; 720_001];
    assert!(convert_wav(&pcm_wav(48_000, 1, 16, &too_long)).is_err());
}

#[test]
fn duration_is_floored_from_converted_sample_count() {
    let fixture = Fixture::new();
    fs::write(
        fixture.clips.join("x.wav"),
        pcm_wav(48_000, 1, 16, &vec![12; 97]),
    )
    .expect("write fractional-ms audio");
    let report = build_package(&fixture.options()).expect("build package");
    assert_eq!(
        report.written.first().map(|binding| binding.duration_ms),
        Some(2)
    );
}

#[test]
fn deterministic_bytes_round_trip_and_provenance_are_verified() {
    let fixture = Fixture::new();
    let options = fixture.options();
    let first = build_package(&options).expect("first build");
    let first_bytes = output_bytes(&first.package_path);
    let first_provenance = output_bytes(&first.provenance_path);

    let second_out = fixture.temp.0.join("second-out");
    let mut second_options = options.clone();
    second_options.out_dir = second_out;
    let second = build_package(&second_options).expect("second build");
    assert_eq!(first_bytes, output_bytes(&second.package_path));
    assert_eq!(first_provenance, output_bytes(&second.provenance_path));

    let stage_dir = fixture.out.join("round-trip-check");
    fs::create_dir(&stage_dir).expect("create stage dir");
    let catalog_value: Value =
        serde_json::from_slice(&catalog_bytes()).expect("parse test catalog");
    let catalog = Catalog::from_json(&catalog_value).expect("test catalog shape");
    assert!(stage_package(
        &first.package_path,
        &stage_dir,
        &catalog,
        &Limits::default()
    )
    .is_ok());
    fs::remove_dir_all(&stage_dir).expect("remove round-trip dir");

    let provenance: Value = serde_json::from_slice(&first_provenance).expect("parse provenance");
    assert_eq!(
        provenance["output_container"]["sha256"],
        json!(sha256(&first_bytes))
    );
    assert_eq!(
        provenance["output_container"]["size_bytes"],
        json!(first_bytes.len())
    );
    assert_eq!(
        provenance["rights_claim"]["text"],
        json!("synthetic owner claim")
    );
    assert_eq!(
        provenance["rights_claim"]["status"],
        json!("owner_claim_unverified")
    );
    let input_files = provenance["input_files"].as_array().expect("input list");
    for (kind, path, bytes) in [
        (
            "mapping",
            "mapping.json",
            fs::read(&fixture.map).expect("mapping bytes"),
        ),
        (
            "catalog",
            "event-catalog.json",
            fs::read(&fixture.catalog).expect("catalog bytes"),
        ),
        (
            "clip",
            "x.wav",
            fs::read(fixture.clips.join("x.wav")).expect("audio bytes"),
        ),
        (
            "clip",
            "x.webm",
            fs::read(fixture.clips.join("x.webm")).expect("video bytes"),
        ),
    ] {
        let receipt = input_files
            .iter()
            .find(|receipt| receipt["kind"] == kind && receipt["path"] == path)
            .expect("matching provenance receipt");
        assert_eq!(receipt["sha256"], json!(sha256(&bytes)));
        assert_eq!(receipt["size_bytes"], json!(bytes.len()));
    }
    let provenance_text = String::from_utf8(first_provenance).expect("provenance utf8");
    assert!(!provenance_text.contains(&fixture.temp.0.to_string_lossy().to_string()));
    assert!(!provenance_text.contains("F:\\"));
    assert!(!fixture.out.read_dir().expect("out entries").any(|entry| {
        entry
            .expect("out entry")
            .file_name()
            .to_string_lossy()
            .contains("stage-")
    }));
}

#[test]
fn unknown_event_tier_errors_and_duplicate_tiers_are_refused() {
    let fixture = Fixture::new();

    fixture.set_mapping(&[binding("not_known", None, Some("x.webm"), None, None, None)]);
    let unknown = build_package(&fixture.options()).expect_err("unknown event refused");
    assert_eq!(unknown.exit_code, 1);
    assert!(unknown.message.contains("unknown event"));

    fixture.set_mapping(&[binding(
        "hero_kill",
        Some(1),
        Some("x.webm"),
        None,
        None,
        None,
    )]);
    let tier_not_allowed =
        build_package(&fixture.options()).expect_err("tier on un-tiered event refused");
    assert!(tier_not_allowed.message.contains("does not allow a tier"));

    fixture.set_mapping(&[binding(
        "multi_kill",
        Some(5),
        Some("x.webm"),
        None,
        None,
        None,
    )]);
    let tier_outside = build_package(&fixture.options()).expect_err("out-of-range tier refused");
    assert!(tier_outside.message.contains("outside the catalog range"));

    fixture.set_mapping(&[
        binding("multi_kill", Some(2), Some("x.webm"), None, None, None),
        binding("multi_kill", Some(2), Some("x.webm"), None, None, None),
    ]);
    let duplicate = build_package(&fixture.options()).expect_err("duplicate event/tier refused");
    assert!(duplicate.message.contains("duplicate event/tier"));
}

#[test]
fn non_mappable_events_and_duplicate_json_keys_are_refused() {
    let fixture = Fixture::new();
    fixture.set_mapping(&[binding(
        "hero_respawn",
        None,
        Some("x.webm"),
        None,
        None,
        None,
    )]);
    let non_mappable = build_package(&fixture.options()).expect_err("unmappable event refused");
    assert!(non_mappable.message.contains("not media-mappable"));

    fs::write(
        &fixture.map,
        br#"{"schema_version":"1.0.0","schema_version":"1.0.0","bindings":[]}"#,
    )
    .expect("write duplicate key mapping");
    let duplicate_key = build_package(&fixture.options()).expect_err("duplicate JSON key refused");
    assert!(duplicate_key.message.contains("DuplicateKey"));
}

#[test]
fn missing_visuals_build_audio_only_and_invalid_visuals_are_skipped() {
    let fixture = Fixture::new();
    fixture.set_mapping(&[
        binding("hero_kill", None, None, None, None, None),
        binding("multi_kill", Some(2), Some("x.webm"), None, None, None),
    ]);
    let report = build_package(&fixture.options()).expect("build valid remaining binding");
    assert_eq!(report.written.len(), 2);
    assert_eq!(
        report
            .written
            .first()
            .and_then(|binding| binding.visual.as_deref()),
        None
    );

    fixture.set_mapping(&[
        binding("hero_kill", None, Some("bad.webm"), None, None, None),
        binding("multi_kill", Some(2), Some("x.webm"), None, None, None),
    ]);
    let mut options = fixture.options();
    options.out_dir = fixture.temp.0.join("bad-visual-out");
    fs::write(fixture.clips.join("bad.webm"), b"not an EBML webm").expect("write invalid visual");
    let report = build_package(&options).expect("build valid binding after bad visual");
    assert_eq!(report.skipped.len(), 1);
    assert!(report.skipped[0]
        .reason
        .contains("visual failed silent WebM inspection"));

    fixture.set_mapping(&[binding(
        "hero_kill",
        None,
        Some("bad.webm"),
        None,
        None,
        None,
    )]);
    let mut all_skipped_options = fixture.options();
    all_skipped_options.out_dir = fixture.temp.0.join("all-skipped-out");
    let error = build_package(&all_skipped_options).expect_err("all skipped fails");
    assert_eq!(error.exit_code, 2);
    assert!(error
        .message
        .contains("visual failed silent WebM inspection"));
    assert!(!all_skipped_options.out_dir.exists());
}

#[test]
fn visual_with_audio_track_is_skipped_and_oversized_clip_is_skipped() {
    let fixture = Fixture::new();
    fs::write(fixture.clips.join("bad.webm"), webm(Some(true))).expect("write WebM with audio");
    fixture.set_mapping(&[
        binding("hero_kill", None, Some("bad.webm"), None, None, None),
        binding("multi_kill", Some(2), Some("huge.webm"), None, None, None),
        binding("multi_kill", Some(3), Some("x.webm"), None, None, None),
    ]);
    let huge_path = fixture.clips.join("huge.webm");
    let huge = File::create(huge_path).expect("create sparse oversized visual");
    huge.set_len(64 * 1024 * 1024 + 1)
        .expect("set oversized length");
    let report = build_package(&fixture.options()).expect("remaining valid visual builds");
    assert_eq!(report.written.len(), 1);
    assert_eq!(report.skipped.len(), 2);
    assert!(report
        .skipped
        .iter()
        .any(|item| item.reason.contains("visual_has_audio_track")));
    assert!(report
        .skipped
        .iter()
        .any(|item| item.reason.contains("exceeds 64 MiB")));
}

#[test]
fn too_long_audio_and_identity_length_mismatch_fail_the_binding() {
    let fixture = Fixture::new();
    fs::write(
        fixture.clips.join("x.wav"),
        pcm_wav(48_000, 1, 16, &vec![1; 720_001]),
    )
    .expect("write overlong audio");
    let too_long = build_package(&fixture.options()).expect_err("audio over 15 seconds rejected");
    assert_eq!(too_long.exit_code, 1);
    assert!(too_long.message.contains("exceeds 15000 ms"));

    fs::write(
        fixture.clips.join("x.wav"),
        pcm_wav(48_000, 1, 16, &vec![1; 480]),
    )
    .expect("restore mixed audio");
    fs::write(
        fixture.clips.join("identity.wav"),
        pcm_wav(48_000, 1, 16, &vec![1; 481]),
    )
    .expect("write mismatched original");
    fixture.set_mapping(&[binding(
        "multi_kill",
        Some(2),
        Some("x.webm"),
        Some("identity.wav"),
        None,
        None,
    )]);
    let mismatch =
        build_package(&fixture.options()).expect_err("identity length mismatch rejected");
    assert!(mismatch.message.contains("sample count differs"));
}

#[test]
fn original_identity_audio_is_converted_and_included() {
    let fixture = Fixture::new();
    let original = vec![2_048; 441];
    fs::write(
        fixture.clips.join("x-original.wav"),
        pcm_wav(44_100, 1, 16, &original),
    )
    .expect("write original audio");
    fixture.set_mapping(&[binding(
        "multi_kill",
        Some(2),
        Some("x.webm"),
        Some("x-original.wav"),
        None,
        None,
    )]);

    let report = build_package(&fixture.options()).expect("build with original identity audio");
    let stage_dir = fixture.out.join("identity-stage");
    fs::create_dir(&stage_dir).expect("create identity stage");
    let catalog_value: Value = serde_json::from_slice(&catalog_bytes()).expect("parse catalog");
    let catalog = Catalog::from_json(&catalog_value).expect("catalog shape");
    let staged = match stage_package(
        &report.package_path,
        &stage_dir,
        &catalog,
        &Limits::default(),
    ) {
        Ok(staged) => staged,
        Err(reason) => panic!("package rejected in round trip: {}", reason.as_str()),
    };
    let descriptor_binding = staged
        .parsed
        .descriptor
        .bindings
        .first()
        .expect("one parsed binding");
    assert_eq!(
        descriptor_binding.identity_audio_path.as_deref(),
        Some("audio/multi_kill-t2-identity.wav")
    );
    assert_eq!(
        descriptor_binding.mixed_audio_path,
        "audio/multi_kill-t2.wav"
    );
    fs::remove_dir_all(stage_dir).expect("remove identity stage");
}

#[test]
fn output_inside_git_worktree_is_refused_with_code_four() {
    let fixture = Fixture::new();
    let repository = fixture.temp.0.join("fake-repository");
    fs::create_dir_all(repository.join(".git")).expect("create git marker");
    let mut options = fixture.options();
    options.out_dir = repository.join("build-output");
    let error = build_package(&options).expect_err("repository output refused");
    assert_eq!(error.exit_code, 4);
    assert!(!repository.join("build-output").exists());
}

#[test]
fn output_is_confined_to_out_and_staging_is_removed() {
    let fixture = Fixture::new();
    let before = fs::read_dir(&fixture.temp.0)
        .expect("test input root")
        .map(|entry| entry.expect("entry").file_name())
        .collect::<Vec<_>>();
    let report = build_package(&fixture.options()).expect("build package");
    let canonical_out = fs::canonicalize(&fixture.out).expect("canonical output directory");
    assert!(report.package_path.starts_with(&canonical_out));
    assert!(report.provenance_path.starts_with(&canonical_out));
    let after = fs::read_dir(&fixture.temp.0)
        .expect("test input root after build")
        .map(|entry| entry.expect("entry").file_name())
        .collect::<Vec<_>>();
    let after_without_out = after
        .into_iter()
        .filter(|name| name != "out")
        .collect::<Vec<_>>();
    assert_eq!(before, after_without_out);
    let output_names = fs::read_dir(&fixture.out)
        .expect("output directory")
        .map(|entry| {
            entry
                .expect("output entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(output_names.len(), 2);
    assert!(output_names.iter().all(|name| !name.contains("stage-")));
}

#[test]
fn binary_builds_package_through_process_command() {
    let fixture = Fixture::new();
    let result = Command::new(env!("CARGO_BIN_EXE_kohbor-pack-build"))
        .arg("--clips")
        .arg(&fixture.clips)
        .arg("--map")
        .arg(&fixture.map)
        .arg("--out")
        .arg(&fixture.out)
        .arg("--package-id")
        .arg("synthetic-pack")
        .arg("--revision")
        .arg("3")
        .arg("--display-name")
        .arg("Synthetic Pack")
        .arg("--catalog")
        .arg(&fixture.catalog)
        .arg("--rights-claim")
        .arg("synthetic claim")
        .output()
        .expect("run compiled package builder");
    assert_eq!(result.status.code(), Some(0));
    let stdout = String::from_utf8(result.stdout).expect("CLI JSON output");
    assert!(stdout.contains("synthetic-pack-r3.zip"));
    assert!(fixture
        .out
        .join("synthetic-pack-r3.provenance.json")
        .is_file());
}
