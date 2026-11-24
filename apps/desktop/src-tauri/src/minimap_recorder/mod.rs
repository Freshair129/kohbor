#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    use std::sync::atomic::AtomicU64;
    use std::sync::mpsc::sync_channel;
    use std::time::Instant;

    fn root(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "kohbor-minimap-recorder-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    pub(super) fn sample(epoch: u64) -> FrameSample {
        FrameSample {
            pixels: Arc::from([0_u8, 0, 255, 255, 0, 255, 0, 255]),
            width: 2,
            height: 1,
            stride: 8,
            metadata: FrameMetadata {
                session_epoch: epoch,
                game_clock_seconds: Some(120),
                phase: "live".to_owned(),
                capture_age_ms: 12,
                crop_rect: RectData {
                    x: 0,
                    y: 800,
                    width: 2,
                    height: 1,
                },
                capture_size: Dimensions {
                    width: 1920,
                    height: 1080,
                },
                stale: false,
                crop_confirmed: true,
                game_build: None,
                resolution: Some(Dimensions {
                    width: 1920,
                    height: 1080,
                }),
                dpi: Some(96),
                capture_profile: Some("p1080-default-v3".to_owned()),
                dota_minimap_settings: DotaMinimapSettings {
                    show_hero_icons: Some(true),
                    hero_size: Some(600),
                },
                team_side: Some("radiant".to_owned()),
                match_hash: None,
                ally_hero_ids: Some((0..5).map(|index| format!("npc_dota_hero_ally_{index}")).collect()),
                enemy_hero_ids: Some((0..5).map(|index| format!("npc_dota_hero_enemy_{index}")).collect()),
            },
        }
    }

    fn enabled_config(total_cap_bytes: u64) -> RecorderConfig {
        RecorderConfig {
            enabled: true,
            sampling_rate_fps: 2,
            total_cap_bytes,
        }
    }

    #[test]
    fn disabled_recording_writes_no_files() {
        let directory = root("off");
        let store = RecordingStore::new(directory.clone(), b"synthetic-install-key".to_vec());

        let result = store
            .record_frame(&RecorderConfig::default(), &sample(7), 0)
            .unwrap();

        assert_eq!(result, RecordOutcome::Ignored);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_frame_is_eligible_only_when_live_fresh_and_crop_confirmed() {
        let directory = root("eligibility");
        let store = RecordingStore::new(directory.clone(), b"synthetic-install-key".to_vec());
        let config = enabled_config(1_000_000);
        let frame = sample(7);
        assert!(frame.eligible());
        assert_eq!(
            store.record_frame(&config, &frame, 0).unwrap(),
            RecordOutcome::Recorded
        );

        let mut invalid_cases = Vec::new();
        let mut not_live = sample(8);
        not_live.metadata.phase = "pregame".to_owned();
        invalid_cases.push(not_live);
        let mut stale = sample(9);
        stale.metadata.stale = true;
        invalid_cases.push(stale);
        let mut old = sample(10);
        old.metadata.capture_age_ms = 2001;
        invalid_cases.push(old);
        let mut unconfirmed = sample(11);
        unconfirmed.metadata.crop_confirmed = false;
        invalid_cases.push(unconfirmed);

        for invalid in invalid_cases {
            assert!(!invalid.eligible());
            assert_eq!(
                store.record_frame(&config, &invalid, 0).unwrap(),
                RecordOutcome::Ignored
            );
        }
        let matches = store.recordings().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["frame_count"], 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn manifest_and_frame_index_keep_only_the_keyed_match_hash_and_approved_fields() {
        let directory = root("privacy");
        let store = RecordingStore::new(directory.clone(), b"synthetic-install-key".to_vec());
        assert_eq!(
            store
                .record_frame(&enabled_config(1_000_000), &sample(7), 2)
                .unwrap(),
            RecordOutcome::Recorded
        );

        let match_hash = install_keyed_match_hash(b"synthetic-install-key", 7);
        let match_dir = directory.join(&match_hash);
        let manifest = fs::read_to_string(match_dir.join("manifest.json")).unwrap();
        let index = fs::read_to_string(match_dir.join("frames.jsonl")).unwrap();
        assert!(manifest.contains(&match_hash));
        assert!(!manifest.contains("raw-match-id"));
        assert!(!manifest.to_ascii_lowercase().contains("player_name"));
        assert!(!manifest.to_ascii_lowercase().contains("account_id"));
        let manifest_value: Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(
            manifest_value["enemy_hero_ids"],
            json!((0..5).map(|index| format!("npc_dota_hero_enemy_{index}")).collect::<Vec<_>>())
        );
        assert_eq!(manifest_value["match_hash_source"], "session_epoch_fallback");
        assert_eq!(manifest_value["dropped_frames"], 2);
        assert!(index.contains("\"game_clock_seconds\":120"));
        assert!(index.contains("\"session_epoch\":7"));
        assert!(index.contains("\"stale\":false"));
        assert!(!index.contains("raw-match-id"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn known_install_keyed_match_hash_is_preserved_in_the_test_manifest() {
        let directory = root("known-match-hash");
        let match_hash = "a".repeat(64);
        let mut frame = sample(7);
        frame.metadata.match_hash = Some(match_hash.clone());
        let store = RecordingStore::new(directory.clone(), b"synthetic-install-key".to_vec());

        assert_eq!(
            store
                .record_frame(&enabled_config(1_000_000), &frame, 0)
                .unwrap(),
            RecordOutcome::Recorded
        );
        let manifest_path = directory.join(&match_hash).join("manifest.json");
        let manifest = fs::read_to_string(&manifest_path).unwrap();
        let value: Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(value["match_hash"], match_hash);
        assert_eq!(value["match_hash_source"], "match_id_hmac");
        assert_eq!(value["ally_hero_ids"][0], "npc_dota_hero_ally_0");
        assert_eq!(value["enemy_hero_ids"][4], "npc_dota_hero_enemy_4");
        assert!(!manifest.contains("8000000001"));
        eprintln!("sample manifest produced by test: {manifest}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn total_storage_cap_stops_the_match_before_a_frame_is_written() {
        let directory = root("cap");
        let store = RecordingStore::new(directory.clone(), b"synthetic-install-key".to_vec());

        let result = store
            .record_frame(&enabled_config(1), &sample(7), 0)
            .unwrap();

        assert_eq!(result, RecordOutcome::CapReached);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn per_match_frame_cap_stops_before_a_png_is_written() {
        let directory = root("match-cap");
        let store = RecordingStore::with_match_frame_cap(
            directory.clone(),
            b"synthetic-install-key".to_vec(),
            1,
        );

        let result = store
            .record_frame(&enabled_config(1_000_000), &sample(7), 0)
            .unwrap();

        assert_eq!(result, RecordOutcome::CapReached);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn per_match_cap_is_recorded_in_the_manifest_without_removing_prior_frames() {
        let directory = root("match-stop");
        let sample = sample(7);
        let one_frame_png =
            encode_png_bgra(sample.width, sample.height, sample.stride, &sample.pixels).unwrap();
        let store = RecordingStore::with_match_frame_cap(
            directory.clone(),
            b"synthetic-install-key".to_vec(),
            one_frame_png.len() as u64,
        );
        let config = enabled_config(1_000_000);
        assert_eq!(
            store.record_frame(&config, &sample, 0).unwrap(),
            RecordOutcome::Recorded
        );
        assert_eq!(
            store.record_frame(&config, &sample, 0).unwrap(),
            RecordOutcome::CapReached
        );

        let hash = install_keyed_match_hash(b"synthetic-install-key", 7);
        let match_dir = directory.join(hash);
        let manifest: Value =
            serde_json::from_slice(&fs::read(match_dir.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["frame_count"], 1);
        assert_eq!(manifest["stop_reason"], "cap_reached");
        assert_eq!(
            directory_png_bytes(&match_dir).unwrap(),
            one_frame_png.len() as u64
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_full_frame_queue_drops_without_waiting_for_the_receiver() {
        let (sender, _receiver) = sync_channel(1);
        let drops = AtomicU64::new(0);
        assert!(try_send_frame(&sender, FrameJob::test_job(1), &drops));

        let started = Instant::now();
        assert!(!try_send_frame(&sender, FrameJob::test_job(2), &drops));

        assert_eq!(drops.load(Ordering::Relaxed), 1);
        assert!(started.elapsed().as_millis() < 100);
    }

    fn test_dota_minimap_settings() -> DotaMinimapSettings {
        DotaMinimapSettings::default()
    }

    #[test]
    fn a_drop_after_the_last_accepted_frame_is_persisted_while_worker_is_idle() {
        let directory = root("final-drop");
        let install_key = b"synthetic-install-key".to_vec();
        let config = enabled_config(1_000_000);
        let control = Arc::new(RecorderControl::new(config));
        control.begin_epoch(7);
        let (sender, receiver) = sync_channel(FRAME_QUEUE_CAPACITY);
        let worker_directory = directory.clone();
        let worker_key = install_key.clone();
        let worker_control = Arc::clone(&control);
        let worker = std::thread::spawn(move || {
            recorder_worker(
                worker_directory,
                worker_key,
                receiver,
                worker_control,
                test_dota_minimap_settings,
            )
        });
        sender
            .send(FrameJob {
                sample: sample(7),
                dropped_frames: 0,
            })
            .unwrap();

        let match_hash = install_keyed_match_hash(&install_key, 7);
        let manifest_path = directory.join(&match_hash).join("manifest.json");
        for _ in 0..100 {
            if fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .is_some_and(|manifest| manifest["frame_count"] == 1)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(manifest_path.exists(), "worker records the accepted frame");

        control.dropped_frames.store(1, Ordering::Release);
        let mut manifest_drops = 0;
        for _ in 0..100 {
            manifest_drops = fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .and_then(|manifest| manifest["dropped_frames"].as_u64())
                .unwrap_or(0);
            if manifest_drops == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(sender);
        worker.join().unwrap();

        assert_eq!(manifest_drops, 1);
        let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["dropped_frames"], 1);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn png_encoding_round_trips_a_synthetic_bgra_frame_and_reports_encode_time() {
        let width = 256_u32;
        let height = 256_u32;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        let mut expected_rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let blue = x as u8;
                let green = y as u8;
                let red = (x.wrapping_add(y)) as u8;
                pixels.extend_from_slice(&[blue, green, red, 255]);
                expected_rgba.extend_from_slice(&[red, green, blue, 255]);
            }
        }
        let started = Instant::now();
        let encoded = encode_png_bgra(width, height, width * 4, &pixels).unwrap();
        let elapsed = started.elapsed();
        let (width, height, rgba) = decode_test_png(&encoded);
        assert_eq!((width, height), (256, 256));
        assert!(
            rgba == expected_rgba,
            "decoded synthetic frame pixels match encoder input"
        );
        eprintln!(
            "synthetic 256x256 PNG encode time: {} us",
            elapsed.as_micros()
        );
    }

    #[test]
    fn deleting_one_match_preserves_other_recordings() {
        let directory = root("delete");
        let store = RecordingStore::new(directory.clone(), b"synthetic-install-key".to_vec());
        let config = enabled_config(1_000_000);
        assert_eq!(
            store.record_frame(&config, &sample(7), 0).unwrap(),
            RecordOutcome::Recorded
        );
        assert_eq!(
            store.record_frame(&config, &sample(8), 0).unwrap(),
            RecordOutcome::Recorded
        );
        let keep = install_keyed_match_hash(b"synthetic-install-key", 8);
        let remove = install_keyed_match_hash(b"synthetic-install-key", 7);

        store.delete_match(&remove).unwrap();

        assert!(!directory.join(remove).exists());
        assert!(directory.join(keep).join("manifest.json").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn dota_settings_parser_keeps_missing_values_unknown() {
        let parsed = parse_dota_minimap_settings(
            "\"dota_minimap_show_hero_icon\" \"1\"\n\"dota_minimap_hero_size\" \"600\"\n",
        );
        assert_eq!(parsed.show_hero_icons, Some(true));
        assert_eq!(parsed.hero_size, Some(600));

        let unknown = parse_dota_minimap_settings("\"unrelated_setting\" \"1\"\n");
        assert_eq!(unknown, DotaMinimapSettings::default());
    }

    fn decode_test_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        assert!(bytes.starts_with(PNG_SIGNATURE));
        let mut cursor = 8;
        let mut width = 0;
        let mut height = 0;
        let mut compressed = Vec::new();
        while cursor < bytes.len() {
            let length = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
            let kind = &bytes[cursor + 4..cursor + 8];
            let data = &bytes[cursor + 8..cursor + 8 + length];
            if kind == b"IHDR" {
                width = u32::from_be_bytes(data[0..4].try_into().unwrap());
                height = u32::from_be_bytes(data[4..8].try_into().unwrap());
            } else if kind == b"IDAT" {
                compressed.extend_from_slice(data);
            }
            cursor += 12 + length;
            if kind == b"IEND" {
                break;
            }
        }
        let mut raw = Vec::new();
        ZlibDecoder::new(compressed.as_slice())
            .read_to_end(&mut raw)
            .unwrap();
        let row_width = width as usize * 4;
        let encoded_row_width = row_width + 1;
        assert_eq!(
            raw.len(),
            encoded_row_width * height as usize,
            "encoded rows have the expected filter prefix and pixel payload"
        );
        let mut rgba = Vec::with_capacity(row_width * height as usize);
        for row in raw.chunks_exact(encoded_row_width) {
            assert_eq!(row[0], 0, "the encoder uses lossless filter type 0");
            rgba.extend_from_slice(&row[1..]);
        }
        (width, height, rgba)
    }
}
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_TOTAL_CAP_BYTES: u64 = 5_000_000_000;
const PER_MATCH_FRAME_CAP_BYTES: u64 = 150_000_000;
const FRAME_QUEUE_CAPACITY: usize = 8;
const FRESH_CAPTURE_MAX_AGE_MS: u64 = 2_000;
const RECORDING_INDICATOR_FRESH_MS: u64 = 1_500;
const MANIFEST_UPDATE_HEADROOM_BYTES: u64 = 64;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecorderConfig {
    pub enabled: bool,
    pub sampling_rate_fps: u8,
    pub total_cap_bytes: u64,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sampling_rate_fps: 2,
            total_cap_bytes: DEFAULT_TOTAL_CAP_BYTES,
        }
    }
}

impl RecorderConfig {
    fn validate(&self) -> Result<(), String> {
        if !(1..=5).contains(&self.sampling_rate_fps) || self.total_cap_bytes == 0 {
            return Err("MINIMAP_RECORDER_CONFIG_INVALID".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct RectData {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DotaMinimapSettings {
    pub show_hero_icons: Option<bool>,
    pub hero_size: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchHashSource {
    MatchIdHmac,
    SessionEpochFallback,
}

/// Minimal copy of admitted intake fields needed to label a minimap recording. It carries no
/// raw match ID, player ID, or name and is produced only from fresh verified live GSI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecorderIntakeSnapshot {
    pub session_epoch: u64,
    pub match_hash: Option<String>,
    pub game_clock_seconds: Option<i64>,
    pub team_side: Option<crate::hero_stats::counters::Side>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrameMetadata {
    pub session_epoch: u64,
    /// Existing install-keyed match-history hash when GSI has a match ID; never the raw ID.
    pub match_hash: Option<String>,
    pub game_clock_seconds: Option<i64>,
    pub phase: String,
    pub capture_age_ms: u64,
    pub crop_rect: RectData,
    pub capture_size: Dimensions,
    pub stale: bool,
    pub crop_confirmed: bool,
    pub game_build: Option<String>,
    pub resolution: Option<Dimensions>,
    pub dpi: Option<u32>,
    pub capture_profile: Option<String>,
    pub dota_minimap_settings: DotaMinimapSettings,
    pub team_side: Option<String>,
    pub ally_hero_ids: Option<Vec<String>>,
    pub enemy_hero_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct FrameSample {
    /// BGRA bytes from the capture host's already-cropped minimap region.
    pub pixels: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub metadata: FrameMetadata,
}

impl FrameSample {
    pub fn eligible(&self) -> bool {
        self.metadata.phase == "live"
            && !self.metadata.stale
            && self.metadata.capture_age_ms <= FRESH_CAPTURE_MAX_AGE_MS
            && self.metadata.crop_confirmed
            && self.metadata.session_epoch > 0
            && self.width > 0
            && self.height > 0
            && self.stride >= self.width.saturating_mul(4)
            && usize::try_from(self.height)
                .ok()
                .and_then(|height| height.checked_mul(self.stride as usize))
                == Some(self.pixels.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    Ignored,
    Recorded,
    CapReached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchManifest {
    schema_version: u32,
    match_hash: String,
    match_hash_source: MatchHashSource,
    game_build: Option<String>,
    resolution: Option<Dimensions>,
    dpi: Option<u32>,
    capture_profile: Option<String>,
    crop_confirmed: bool,
    dota_minimap_settings: DotaMinimapSettings,
    team_side: Option<String>,
    ally_hero_ids: Option<Vec<String>>,
    enemy_hero_ids: Option<Vec<String>>,
    sampling_rate_fps: u8,
    frame_count: u64,
    frame_bytes: u64,
    dropped_frames: u64,
    stop_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct FrameIndexEntry<'a> {
    frame_file: &'a str,
    game_clock_seconds: Option<i64>,
    session_epoch: u64,
    phase: &'a str,
    capture_age_ms: u64,
    crop_rect: RectData,
    capture_size: Dimensions,
    stale: bool,
}

#[derive(Debug, Serialize)]
struct RecordingSummary {
    match_hash: String,
    frame_count: u64,
    frame_bytes: u64,
    total_bytes: u64,
    ally_hero_ids: Option<Vec<String>>,
    enemy_hero_ids: Option<Vec<String>>,
}

pub struct RecordingStore {
    root: PathBuf,
    install_key: Vec<u8>,
    per_match_frame_cap_bytes: u64,
}

impl RecordingStore {
    pub fn new(root: PathBuf, install_key: Vec<u8>) -> Self {
        Self {
            root,
            install_key,
            per_match_frame_cap_bytes: PER_MATCH_FRAME_CAP_BYTES,
        }
    }

    #[cfg(test)]
    fn with_match_frame_cap(root: PathBuf, install_key: Vec<u8>, cap_bytes: u64) -> Self {
        Self {
            root,
            install_key,
            per_match_frame_cap_bytes: cap_bytes,
        }
    }

    pub fn record_frame(
        &self,
        config: &RecorderConfig,
        sample: &FrameSample,
        dropped_frames: u64,
    ) -> Result<RecordOutcome, String> {
        config.validate()?;
        if !config.enabled || !sample.eligible() {
            return Ok(RecordOutcome::Ignored);
        }
        ensure_recording_root(&self.root)?;
        let (match_hash, match_hash_source) = match sample.metadata.match_hash.as_deref() {
            Some(match_hash) if valid_match_hash(match_hash) => {
                (match_hash.to_owned(), MatchHashSource::MatchIdHmac)
            }
            Some(_) => return Err("MINIMAP_RECORDING_MATCH_INVALID".to_owned()),
            None => (
                install_keyed_match_hash(&self.install_key, sample.metadata.session_epoch),
                MatchHashSource::SessionEpochFallback,
            ),
        };
        let png = encode_png_bgra(sample.width, sample.height, sample.stride, &sample.pixels)?;
        let match_dir = self.root.join(&match_hash);
        let existing_frames = directory_png_bytes(&match_dir)?;
        if existing_frames.saturating_add(png.len() as u64) > self.per_match_frame_cap_bytes {
            self.mark_stopped(&match_dir, config.total_cap_bytes);
            return Ok(RecordOutcome::CapReached);
        }
        let _ = safe_regular_file_if_present(&match_dir.join("manifest.json"))?;
        let mut manifest = load_or_create_manifest(
            &match_dir,
            &match_hash,
            match_hash_source,
            config,
            &sample.metadata,
        )?;
        if manifest.match_hash_source != match_hash_source {
            return Err("MINIMAP_RECORDING_MATCH_INVALID".to_owned());
        }
        let existing_total = directory_bytes(&self.root)?;
        let mut frame_number = manifest.frame_count.saturating_add(1);
        let (frame_name, frame_path) = loop {
            let name = format!("frame-{frame_number:06}.png");
            let path = match_dir.join(&name);
            if !path.exists() {
                break (name, path);
            }
            frame_number = frame_number.saturating_add(1);
        };
        let entry = FrameIndexEntry {
            frame_file: &frame_name,
            game_clock_seconds: sample.metadata.game_clock_seconds,
            session_epoch: sample.metadata.session_epoch,
            phase: &sample.metadata.phase,
            capture_age_ms: sample.metadata.capture_age_ms,
            crop_rect: sample.metadata.crop_rect,
            capture_size: sample.metadata.capture_size,
            stale: sample.metadata.stale,
        };
        let index_path = match_dir.join("frames.jsonl");
        let _ = safe_regular_file_if_present(&index_path)?;
        let mut index_line =
            serde_json::to_vec(&entry).map_err(|_| "MINIMAP_RECORDING_WRITE_FAILED".to_owned())?;
        index_line.push(b'\n');
        manifest.frame_count = manifest.frame_count.saturating_add(1);
        manifest.frame_bytes = existing_frames.saturating_add(png.len() as u64);
        manifest.dropped_frames = manifest.dropped_frames.max(dropped_frames);
        manifest.sampling_rate_fps = config.sampling_rate_fps;
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|_| "MINIMAP_RECORDING_WRITE_FAILED".to_owned())?;
        // Atomic manifest replacement briefly keeps the old manifest and its replacement
        // temp file together; include the incoming PNG, JSONL row and replacement bytes in
        // the cap check without reserving an arbitrary amount for every future frame.
        if existing_total
            .saturating_add(png.len() as u64)
            .saturating_add(index_line.len() as u64)
            .saturating_add(manifest_bytes.len() as u64)
            .saturating_add(MANIFEST_UPDATE_HEADROOM_BYTES)
            > config.total_cap_bytes
        {
            self.mark_stopped(&match_dir, config.total_cap_bytes);
            return Ok(RecordOutcome::CapReached);
        }

        ensure_match_directory(&match_dir)?;
        let mut frame_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&frame_path)
            .map_err(|_| "MINIMAP_RECORDING_WRITE_FAILED".to_owned())?;
        if frame_file.write_all(&png).is_err() {
            let _ = fs::remove_file(&frame_path);
            return Err("MINIMAP_RECORDING_WRITE_FAILED".to_owned());
        }
        drop(frame_file);

        let old_index_len = fs::metadata(&index_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let mut index = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&index_path)
            .map_err(|_| "MINIMAP_RECORDING_WRITE_FAILED".to_owned())?;
        if index.write_all(&index_line).is_err() {
            let _ = index.set_len(old_index_len);
            let _ = fs::remove_file(&frame_path);
            return Err("MINIMAP_RECORDING_WRITE_FAILED".to_owned());
        }
        drop(index);

        if crate::atomic::write(&match_dir.join("manifest.json"), &manifest_bytes).is_err() {
            if let Ok(index) = OpenOptions::new().write(true).open(&index_path) {
                let _ = index.set_len(old_index_len);
            }
            let _ = fs::remove_file(&frame_path);
            return Err("MINIMAP_RECORDING_WRITE_FAILED".to_owned());
        }
        Ok(RecordOutcome::Recorded)
    }

    fn mark_stopped(&self, match_dir: &Path, total_cap_bytes: u64) {
        if !safe_directory(match_dir) {
            return;
        }
        let path = match_dir.join("manifest.json");
        if !safe_regular_file_if_present(&path).unwrap_or(false) {
            return;
        }
        let Ok(bytes) = fs::read(&path) else { return };
        let Ok(mut manifest) = serde_json::from_slice::<MatchManifest>(&bytes) else {
            return;
        };
        manifest.stop_reason = Some("cap_reached".to_owned());
        let Ok(updated) = serde_json::to_vec_pretty(&manifest) else {
            return;
        };
        let Ok(total) = directory_bytes(&self.root) else {
            return;
        };
        if total.saturating_add(updated.len() as u64) > total_cap_bytes {
            return;
        }
        let _ = crate::atomic::write(&path, &updated);
    }

    fn update_dropped_frames(
        &self,
        match_hash: &str,
        dropped_frames: u64,
        total_cap_bytes: u64,
    ) -> Result<bool, String> {
        if !valid_match_hash(match_hash) {
            return Err("MINIMAP_RECORDING_MATCH_INVALID".to_owned());
        }
        let match_dir = self.root.join(match_hash);
        if !safe_directory_if_present(&match_dir)? {
            return Ok(false);
        }
        let path = match_dir.join("manifest.json");
        if !safe_regular_file_if_present(&path)? {
            return Ok(false);
        }
        let bytes = fs::read(&path).map_err(|_| "MINIMAP_RECORDING_READ_FAILED".to_owned())?;
        let mut manifest: MatchManifest = serde_json::from_slice(&bytes)
            .map_err(|_| "MINIMAP_RECORDING_MANIFEST_INVALID".to_owned())?;
        if manifest.dropped_frames >= dropped_frames {
            return Ok(true);
        }
        manifest.dropped_frames = dropped_frames;
        let updated = serde_json::to_vec_pretty(&manifest)
            .map_err(|_| "MINIMAP_RECORDING_WRITE_FAILED".to_owned())?;
        let current_total = directory_bytes(&self.root)?;
        if current_total
            .saturating_add(updated.len() as u64)
            .saturating_add(MANIFEST_UPDATE_HEADROOM_BYTES)
            > total_cap_bytes
        {
            return Err("MINIMAP_RECORDING_STORAGE_CAP".to_owned());
        }
        crate::atomic::write(&path, &updated)
            .map_err(|_| "MINIMAP_RECORDING_WRITE_FAILED".to_owned())?;
        Ok(true)
    }
    pub fn recordings(&self) -> Result<Vec<Value>, String> {
        let mut results = Vec::new();
        if !safe_directory_if_present(&self.root)? {
            return Ok(results);
        }
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Ok(results);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !valid_match_hash(&name) || !safe_directory(&path) {
                continue;
            }
            let manifest_path = path.join("manifest.json");
            if !safe_regular_file_if_present(&manifest_path)? {
                continue;
            }
            let Ok(bytes) = fs::read(&manifest_path) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_slice::<MatchManifest>(&bytes) else {
                continue;
            };
            let total_bytes = directory_bytes(&path)?;
            results.push(json!(RecordingSummary {
                match_hash: name,
                frame_count: manifest.frame_count,
                frame_bytes: manifest.frame_bytes,
                total_bytes,
                ally_hero_ids: manifest.ally_hero_ids,
                enemy_hero_ids: manifest.enemy_hero_ids,
            }));
        }
        results.sort_by(|left, right| {
            left["match_hash"]
                .as_str()
                .cmp(&right["match_hash"].as_str())
        });
        Ok(results)
    }

    pub fn total_bytes(&self) -> Result<u64, String> {
        directory_bytes(&self.root)
    }

    pub fn delete_match(&self, match_hash: &str) -> Result<(), String> {
        if !valid_match_hash(match_hash) {
            return Err("MINIMAP_RECORDING_MATCH_INVALID".to_owned());
        }
        if !safe_directory_if_present(&self.root)? {
            return Err("MINIMAP_RECORDING_NOT_FOUND".to_owned());
        }
        let path = self.root.join(match_hash);
        if !path.exists() {
            return Err("MINIMAP_RECORDING_NOT_FOUND".to_owned());
        }
        if !safe_directory(&path) {
            return Err("MINIMAP_RECORDING_PATH_INVALID".to_owned());
        }
        fs::remove_dir_all(&path).map_err(|_| "MINIMAP_RECORDING_DELETE_FAILED".to_owned())
    }
}

fn load_or_create_manifest(
    match_dir: &Path,
    match_hash: &str,
    match_hash_source: MatchHashSource,
    config: &RecorderConfig,
    metadata: &FrameMetadata,
) -> Result<MatchManifest, String> {
    let path = match_dir.join("manifest.json");
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "MINIMAP_RECORDING_MANIFEST_INVALID".to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(MatchManifest {
            schema_version: 1,
            match_hash: match_hash.to_owned(),
            match_hash_source,
            game_build: metadata.game_build.clone(),
            resolution: metadata.resolution,
            dpi: metadata.dpi,
            capture_profile: metadata.capture_profile.clone(),
            crop_confirmed: metadata.crop_confirmed,
            dota_minimap_settings: metadata.dota_minimap_settings.clone(),
            team_side: metadata.team_side.clone(),
            ally_hero_ids: metadata.ally_hero_ids.clone(),
            enemy_hero_ids: metadata.enemy_hero_ids.clone(),
            sampling_rate_fps: config.sampling_rate_fps,
            frame_count: 0,
            frame_bytes: 0,
            dropped_frames: 0,
            stop_reason: None,
        }),
        Err(_) => Err("MINIMAP_RECORDING_READ_FAILED".to_owned()),
    }
}

fn ensure_recording_root(root: &Path) -> Result<(), String> {
    if safe_directory_if_present(root)? == false {
        fs::create_dir_all(root).map_err(|_| "MINIMAP_RECORDING_WRITE_FAILED".to_owned())?;
    }
    if !safe_directory(root) {
        return Err("MINIMAP_RECORDING_PATH_INVALID".to_owned());
    }
    Ok(())
}

fn ensure_match_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        if !safe_directory(path) {
            return Err("MINIMAP_RECORDING_PATH_INVALID".to_owned());
        }
        return Ok(());
    }
    fs::create_dir(path).map_err(|_| "MINIMAP_RECORDING_WRITE_FAILED".to_owned())
}

fn safe_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_dir())
        .unwrap_or(false)
}

fn safe_directory_if_present(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(true),
        Ok(_) => Err("MINIMAP_RECORDING_PATH_INVALID".to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("MINIMAP_RECORDING_READ_FAILED".to_owned()),
    }
}

fn safe_regular_file_if_present(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err("MINIMAP_RECORDING_PATH_INVALID".to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("MINIMAP_RECORDING_READ_FAILED".to_owned()),
    }
}

fn valid_match_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn directory_bytes(path: &Path) -> Result<u64, String> {
    if !safe_directory_if_present(path)? {
        return Ok(0);
    }
    let Ok(entries) = fs::read_dir(path) else {
        return Ok(0);
    };
    let mut total = 0_u64;
    for entry in entries {
        let entry = entry.map_err(|_| "MINIMAP_RECORDING_READ_FAILED".to_owned())?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "MINIMAP_RECORDING_READ_FAILED".to_owned())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            total = total.saturating_add(directory_bytes(&path)?);
        }
    }
    Ok(total)
}

fn directory_png_bytes(path: &Path) -> Result<u64, String> {
    if !safe_directory_if_present(path)? {
        return Ok(0);
    }
    let Ok(entries) = fs::read_dir(path) else {
        return Ok(0);
    };
    let mut total = 0_u64;
    for entry in entries {
        let entry = entry.map_err(|_| "MINIMAP_RECORDING_READ_FAILED".to_owned())?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "png")
            && fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_file())
        {
            total = total.saturating_add(
                fs::metadata(path)
                    .map_err(|_| "MINIMAP_RECORDING_READ_FAILED".to_owned())?
                    .len(),
            );
        }
    }
    Ok(total)
}

pub fn install_keyed_match_hash(install_key: &[u8], session_epoch: u64) -> String {
    // The native admission layer has already reduced match identity to a session epoch. Key it
    // with the existing installation secret so recordings never receive or store a raw match ID.
    let mut key = [0_u8; 64];
    if install_key.len() > key.len() {
        key[..32].copy_from_slice(&Sha256::digest(install_key));
    } else {
        key[..install_key.len()].copy_from_slice(install_key);
    }
    let mut inner = [0x36_u8; 64];
    let mut outer = [0x5c_u8; 64];
    for index in 0..64 {
        inner[index] ^= key[index];
        outer[index] ^= key[index];
    }
    let mut digest = Sha256::new();
    digest.update(inner);
    digest.update(b"kohbor-lab-minimap-recording-v1\0");
    digest.update(session_epoch.to_be_bytes());
    let inner_digest = digest.finalize();
    let mut digest = Sha256::new();
    digest.update(outer);
    digest.update(inner_digest);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct FrameJob {
    sample: FrameSample,
    dropped_frames: u64,
}

impl FrameJob {
    #[cfg(test)]
    fn test_job(session_epoch: u64) -> Self {
        Self {
            sample: tests::sample(session_epoch),
            dropped_frames: 0,
        }
    }
}

fn try_send_frame(
    sender: &SyncSender<FrameJob>,
    job: FrameJob,
    dropped_frames: &AtomicU64,
) -> bool {
    match sender.try_send(job) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            dropped_frames.fetch_add(1, Ordering::Relaxed);
            false
        }
        Err(TrySendError::Disconnected(_)) => false,
    }
}

struct RecorderControl {
    config: RwLock<RecorderConfig>,
    enabled: AtomicBool,
    sampling_rate_fps: AtomicU8,
    current_epoch: AtomicU64,
    last_sample_at_ms: AtomicU64,
    dropped_frames: AtomicU64,
    last_submitted_at_unix_ms: AtomicU64,
    stopped_epoch: AtomicU64,
    stop_reason: Mutex<Option<String>>,
    current_match_hash: Mutex<Option<String>>,
}

impl RecorderControl {
    fn new(config: RecorderConfig) -> Self {
        Self {
            enabled: AtomicBool::new(config.enabled),
            sampling_rate_fps: AtomicU8::new(config.sampling_rate_fps),
            config: RwLock::new(config),
            current_epoch: AtomicU64::new(0),
            last_sample_at_ms: AtomicU64::new(0),
            dropped_frames: AtomicU64::new(0),
            last_submitted_at_unix_ms: AtomicU64::new(0),
            stopped_epoch: AtomicU64::new(0),
            stop_reason: Mutex::new(None),
            current_match_hash: Mutex::new(None),
        }
    }

    fn config(&self) -> RecorderConfig {
        self.config
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn begin_epoch(&self, epoch: u64) {
        if self.current_epoch.swap(epoch, Ordering::AcqRel) != epoch {
            self.last_sample_at_ms.store(0, Ordering::Release);
            self.dropped_frames.store(0, Ordering::Release);
            self.stopped_epoch.store(0, Ordering::Release);
        }
    }

    fn take_sample_slot(&self, sampled_at_ms: u64) -> bool {
        let fps = self.sampling_rate_fps.load(Ordering::Acquire).clamp(1, 5);
        let interval_ms = (1000_u64 + u64::from(fps) - 1) / u64::from(fps);
        loop {
            let last = self.last_sample_at_ms.load(Ordering::Acquire);
            if last != 0 && sampled_at_ms.saturating_sub(last) < interval_ms {
                return false;
            }
            if self
                .last_sample_at_ms
                .compare_exchange(last, sampled_at_ms, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
    }

    fn indicator_active(&self) -> bool {
        if !self.enabled.load(Ordering::Acquire)
            || self.stopped_epoch.load(Ordering::Acquire)
                == self.current_epoch.load(Ordering::Acquire)
                && self.current_epoch.load(Ordering::Acquire) != 0
        {
            return false;
        }
        let last = self.last_submitted_at_unix_ms.load(Ordering::Acquire);
        last != 0 && unix_time_ms().saturating_sub(last) <= RECORDING_INDICATOR_FRESH_MS
    }
}

pub struct RecorderService {
    root: PathBuf,
    config_path: PathBuf,
    install_key: Vec<u8>,
    sender: SyncSender<FrameJob>,
    control: Arc<RecorderControl>,
}

impl RecorderService {
    fn new(data_dir: PathBuf, install_key: Vec<u8>) -> Result<Self, String> {
        let root = data_dir.join("lab").join("minimap-recordings");
        let config_path = data_dir.join("lab").join("minimap-recorder.json");
        let config = fs::read(&config_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<RecorderConfig>(&bytes).ok())
            .filter(|config| config.validate().is_ok())
            .unwrap_or_default();
        let control = Arc::new(RecorderControl::new(config));
        let (sender, receiver) = sync_channel(FRAME_QUEUE_CAPACITY);
        let worker_root = root.clone();
        let worker_key = install_key.clone();
        let worker_control = Arc::clone(&control);
        thread::Builder::new()
            .name("kohbor-lab-minimap-recorder".to_owned())
            .spawn(move || {
                recorder_worker(
                    worker_root,
                    worker_key,
                    receiver,
                    worker_control,
                    read_dota_minimap_settings,
                )
            })
            .map_err(|_| "MINIMAP_RECORDER_WORKER_UNAVAILABLE".to_owned())?;
        Ok(Self {
            root,
            config_path,
            install_key,
            sender,
            control,
        })
    }

    pub fn config(&self) -> RecorderConfig {
        self.control.config()
    }

    pub fn set_config(&self, config: RecorderConfig) -> Result<(), String> {
        config.validate()?;
        let was_enabled = self.control.enabled.load(Ordering::Acquire);
        if !config.enabled {
            // Stop accepting work before the UI command persists the opt-out.
            self.control.enabled.store(false, Ordering::Release);
        }
        let bytes = serde_json::to_vec_pretty(&config)
            .map_err(|_| "MINIMAP_RECORDER_CONFIG_WRITE_FAILED".to_owned())?;
        if crate::atomic::write(&self.config_path, &bytes).is_err() {
            self.control.enabled.store(was_enabled, Ordering::Release);
            return Err("MINIMAP_RECORDER_CONFIG_WRITE_FAILED".to_owned());
        }
        *self
            .control
            .config
            .write()
            .unwrap_or_else(|error| error.into_inner()) = config.clone();
        self.control
            .enabled
            .store(config.enabled, Ordering::Release);
        self.control
            .sampling_rate_fps
            .store(config.sampling_rate_fps, Ordering::Release);
        Ok(())
    }

    pub fn try_submit(&self, sample: FrameSample, sampled_at_ms: u64) -> bool {
        if !self.control.enabled.load(Ordering::Acquire) || !sample.eligible() {
            return false;
        }
        let epoch = sample.metadata.session_epoch;
        self.control.begin_epoch(epoch);
        if self.control.stopped_epoch.load(Ordering::Acquire) == epoch
            || !self.control.take_sample_slot(sampled_at_ms)
        {
            return false;
        }
        let dropped = self.control.dropped_frames.load(Ordering::Acquire);
        let job = FrameJob {
            sample,
            dropped_frames: dropped,
        };
        let accepted = try_send_frame(&self.sender, job, &self.control.dropped_frames);
        if accepted {
            self.control
                .last_submitted_at_unix_ms
                .store(unix_time_ms(), Ordering::Release);
        }
        accepted
    }

    pub fn recording_indicator(&self) -> bool {
        self.control.indicator_active()
    }

    pub fn status_value(&self) -> Result<Value, String> {
        let store = RecordingStore::new(self.root.clone(), self.install_key.clone());
        let matches = store.recordings()?;
        let total_bytes = store.total_bytes()?;
        let config = self.config();
        let stop_reason = self
            .control
            .stop_reason
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        Ok(json!({
            "available": true,
            "enabled": config.enabled,
            "sampling_rate_fps": config.sampling_rate_fps,
            "total_cap_bytes": config.total_cap_bytes,
            "per_match_frame_cap_bytes": PER_MATCH_FRAME_CAP_BYTES,
            "recording": self.recording_indicator(),
            "recordings_count": matches.len(),
            "total_bytes": total_bytes,
            "dropped_frames": self.control.dropped_frames.load(Ordering::Acquire),
            "stop_reason": stop_reason,
            "matches": matches,
        }))
    }

    pub fn delete_match(&self, match_hash: &str) -> Result<(), String> {
        if self.recording_indicator()
            && self
                .control
                .current_match_hash
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_deref()
                == Some(match_hash)
        {
            return Err("MINIMAP_RECORDING_ACTIVE".to_owned());
        }
        RecordingStore::new(self.root.clone(), self.install_key.clone()).delete_match(match_hash)
    }

    pub fn folder(&self) -> &Path {
        &self.root
    }

    pub fn ensure_folder(&self) -> Result<(), String> {
        ensure_recording_root(&self.root)
    }
}

fn persist_pending_drops(
    store: &RecordingStore,
    control: &RecorderControl,
    epoch: u64,
    persisted_drops: &mut u64,
) {
    if control.current_epoch.load(Ordering::Acquire) != epoch {
        return;
    }
    let dropped_frames = control.dropped_frames.load(Ordering::Acquire);
    if dropped_frames <= *persisted_drops {
        return;
    }
    let Some(match_hash) = control
        .current_match_hash
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
    else {
        return;
    };
    match store.update_dropped_frames(
        &match_hash,
        dropped_frames,
        control.config().total_cap_bytes,
    ) {
        Ok(true) => *persisted_drops = dropped_frames,
        Ok(false) => {}
        Err(code) => {
            control.stopped_epoch.store(epoch, Ordering::Release);
            *control
                .stop_reason
                .lock()
                .unwrap_or_else(|error| error.into_inner()) =
                Some(if code == "MINIMAP_RECORDING_STORAGE_CAP" {
                    "cap_reached".to_owned()
                } else {
                    code
                });
        }
    }
}

fn recorder_worker(
    root: PathBuf,
    install_key: Vec<u8>,
    receiver: Receiver<FrameJob>,
    control: Arc<RecorderControl>,
    read_settings: fn() -> DotaMinimapSettings,
) {
    let store = RecordingStore::new(root, install_key);
    let mut loaded_settings_epoch = None;
    let mut previous_epoch = None;
    let mut persisted_dropped_frames = 0_u64;
    let mut settings = DotaMinimapSettings::default();
    loop {
        let mut job = match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(job) => job,
            Err(RecvTimeoutError::Timeout) => {
                if let Some(epoch) = previous_epoch {
                    persist_pending_drops(&store, &control, epoch, &mut persisted_dropped_frames);
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => {
                if let Some(epoch) = previous_epoch {
                    persist_pending_drops(&store, &control, epoch, &mut persisted_dropped_frames);
                }
                break;
            }
        };
        if !control.enabled.load(Ordering::Acquire) {
            continue;
        }
        let epoch = job.sample.metadata.session_epoch;
        if previous_epoch != Some(epoch) {
            *control
                .stop_reason
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = None;
            *control
                .current_match_hash
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = None;
            previous_epoch = Some(epoch);
            persisted_dropped_frames = 0;
        }
        if loaded_settings_epoch != Some(epoch) {
            settings = read_settings();
            loaded_settings_epoch = Some(epoch);
        }
        job.sample.metadata.dota_minimap_settings = settings.clone();
        let config = control.config();
        match store.record_frame(&config, &job.sample, job.dropped_frames) {
            Ok(RecordOutcome::Recorded) => {
                let hash = job.sample.metadata.match_hash.clone().unwrap_or_else(|| {
                    install_keyed_match_hash(&store.install_key, epoch)
                });
                *control
                    .current_match_hash
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(hash);
                *control
                    .stop_reason
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = None;
                persisted_dropped_frames = persisted_dropped_frames.max(job.dropped_frames);
            }
            Ok(RecordOutcome::CapReached) => {
                control.stopped_epoch.store(epoch, Ordering::Release);
                *control
                    .stop_reason
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some("cap_reached".to_owned());
            }
            Ok(RecordOutcome::Ignored) => {}
            Err(code) => {
                control.stopped_epoch.store(epoch, Ordering::Release);
                *control
                    .stop_reason
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(code);
            }
        }
        persist_pending_drops(&store, &control, epoch, &mut persisted_dropped_frames);
    }
}

static RECORDER_SERVICE: OnceLock<Arc<RecorderService>> = OnceLock::new();
static RECORDER_INIT_LOCK: Mutex<()> = Mutex::new(());

pub fn initialize(data_dir: PathBuf, install_key: Vec<u8>) -> Result<Arc<RecorderService>, String> {
    if let Some(service) = RECORDER_SERVICE.get() {
        return Ok(Arc::clone(service));
    }
    let _guard = RECORDER_INIT_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(service) = RECORDER_SERVICE.get() {
        return Ok(Arc::clone(service));
    }
    let service = Arc::new(RecorderService::new(data_dir, install_key)?);
    let _ = RECORDER_SERVICE.set(Arc::clone(&service));
    Ok(service)
}

pub fn service() -> Option<&'static Arc<RecorderService>> {
    RECORDER_SERVICE.get()
}

pub fn submit_from_capture_host(sample: FrameSample, sampled_at_ms: u64) -> bool {
    service().is_some_and(|service| service.try_submit(sample, sampled_at_ms))
}

pub fn recording_enabled() -> bool {
    service().is_some_and(|service| service.control.enabled.load(Ordering::Acquire))
}

pub fn recording_indicator() -> bool {
    service().is_some_and(|service| service.recording_indicator())
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn read_dota_minimap_settings() -> DotaMinimapSettings {
    let Some(path) = dota_user_convars_path() else {
        return DotaMinimapSettings::default();
    };
    fs::read_to_string(path)
        .ok()
        .map(|contents| parse_dota_minimap_settings(&contents))
        .unwrap_or_default()
}

fn parse_dota_minimap_settings(contents: &str) -> DotaMinimapSettings {
    let mut result = DotaMinimapSettings::default();
    for line in contents.lines() {
        let mut quoted = line.match_indices('"').map(|(index, _)| index);
        let Some(first) = quoted.next() else { continue };
        let Some(second) = quoted.next() else {
            continue;
        };
        let Some(third) = quoted.next() else { continue };
        let Some(fourth) = quoted.next() else {
            continue;
        };
        let key = &line[first + 1..second];
        let value = &line[third + 1..fourth];
        match key {
            "dota_minimap_show_hero_icon" => {
                result.show_hero_icons = parse_bool_setting(value);
            }
            "dota_minimap_hero_size" => {
                result.hero_size = value.parse::<u32>().ok();
            }
            _ => {}
        }
    }
    result
}

fn parse_bool_setting(value: &str) -> Option<bool> {
    match value.trim() {
        "1" | "true" | "True" => Some(true),
        "0" | "false" | "False" => Some(false),
        _ => None,
    }
}

#[cfg(windows)]
fn dota_user_convars_path() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let active = current_user
        .open_subkey(r"Software\Valve\Steam\ActiveProcess")
        .ok()?;
    let active_user: u32 = active.get_value("ActiveUser").ok()?;
    let steam_key = current_user.open_subkey(r"Software\Valve\Steam").ok()?;
    let steam_path: String = steam_key.get_value("SteamPath").ok()?;
    Some(
        PathBuf::from(steam_path)
            .join("userdata")
            .join(active_user.to_string())
            .join("570")
            .join("local")
            .join("cfg")
            .join("user_convars_0_slot0.vcfg"),
    )
}

#[cfg(not(windows))]
fn dota_user_convars_path() -> Option<PathBuf> {
    None
}

fn encode_png_bgra(width: u32, height: u32, stride: u32, bgra: &[u8]) -> Result<Vec<u8>, String> {
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || stride < width.saturating_mul(4)
    {
        return Err("MINIMAP_RECORDING_FRAME_INVALID".to_owned());
    }
    let expected = usize::try_from(height)
        .ok()
        .and_then(|height| height.checked_mul(stride as usize))
        .ok_or_else(|| "MINIMAP_RECORDING_FRAME_INVALID".to_owned())?;
    if bgra.len() != expected {
        return Err("MINIMAP_RECORDING_FRAME_INVALID".to_owned());
    }
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| "MINIMAP_RECORDING_FRAME_INVALID".to_owned())?;
    let raw_capacity = height as usize * (row_bytes + 1);
    let mut raw = Vec::with_capacity(raw_capacity);
    for row in 0..height as usize {
        raw.push(0); // PNG filter 0: lossless, bounded, and simple to decode.
        let source = row * stride as usize;
        for pixel in bgra[source..source + row_bytes].chunks_exact(4) {
            raw.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(&raw)
        .map_err(|_| "MINIMAP_RECORDING_ENCODE_FAILED".to_owned())?;
    let compressed = encoder
        .finish()
        .map_err(|_| "MINIMAP_RECORDING_ENCODE_FAILED".to_owned())?;

    let mut png = PNG_SIGNATURE.to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    push_png_chunk(&mut png, b"IHDR", &header)?;
    push_png_chunk(&mut png, b"IDAT", &compressed)?;
    push_png_chunk(&mut png, b"IEND", &[])?;
    Ok(png)
}

fn push_png_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Result<(), String> {
    let length =
        u32::try_from(data.len()).map_err(|_| "MINIMAP_RECORDING_ENCODE_FAILED".to_owned())?;
    png.extend_from_slice(&length.to_be_bytes());
    let start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let crc = png_crc32(&png[start..]);
    png.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

fn png_crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
