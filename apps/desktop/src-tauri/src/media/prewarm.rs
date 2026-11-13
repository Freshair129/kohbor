//! Off-engine media loading for the immutable audio table and visual allowlist.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use kohbor_audio::pcm::decode_wav;
use kohbor_events::EventId;
use kohbor_pack::catalog::Catalog;
use kohbor_pack::store::{Hook, ResolveError, Store};
use sha2::{Digest, Sha256};

use crate::audio::resolve::{Clip, Entry, PrewarmTable};

/// Verified visual file metadata captured alongside the audio table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified: SystemTime,
}

/// Renderer-visible files from the one revision represented by the table.
#[derive(Debug, Clone, Default)]
pub struct VisualIndex {
    pub install_id: Option<Arc<str>>,
    pub entries: HashMap<String, VisualEntry>,
}

pub type VisualIndexHandle = Arc<Mutex<Arc<VisualIndex>>>;

pub fn new_visual_index_handle() -> VisualIndexHandle {
    Arc::new(Mutex::new(Arc::new(VisualIndex::default())))
}

const MAX_PREWARMED_SAMPLES: u64 = 40_000_000;
const AUDIO_SAMPLE_RATE: u64 = 48_000;

/// Return every voiced host event's tier-less key and catalog-defined tiers.
pub fn key_space(catalog: &Catalog) -> Vec<(EventId, Option<u8>)> {
    let mut keys = Vec::new();
    for id in crate::media::inventory::ALL_EVENTS
        .iter()
        .copied()
        .filter(|id| id.voiced())
    {
        keys.push((id, None));
        if let Some((minimum, maximum)) = catalog
            .event(id.as_str())
            .and_then(|event| event.tier_range)
        {
            // Catalog::from_json bounds tiers at 100. Keep this safe for callers
            // that construct Catalog directly instead of using that validator.
            if minimum == 0 || maximum < minimum || maximum > 100 {
                continue;
            }
            for tier in minimum..=maximum {
                if let Ok(tier) = u8::try_from(tier) {
                    keys.push((id, Some(tier)));
                }
            }
        }
    }
    keys
}

/// Resolve, verify, read and decode all playable keys without touching the engine.
///
/// The table retains at most 40 million decoded i16 samples (80 MB, about 76.3
/// MiB). The product design's future 26-key maximum is 37.44 MB (18.72 million
/// samples, about 35.7 MiB); this wider guard is defensive for future inventories.
/// If a decoded clip would exceed the guard, that key and all remaining keys are
/// marked invalid and are not loaded.
pub fn build_table<H: Hook>(
    store: &Store<H>,
    catalog: &Catalog,
    pause: &mut dyn FnMut(),
) -> (Arc<PrewarmTable>, VisualIndex) {
    let Some(active) = store.active() else {
        return (PrewarmTable::empty(), VisualIndex::default());
    };

    let install_id: Arc<str> = Arc::from(active.install_id.as_str());
    let fence = install_number(&active.install_id);
    let revision_root = store.root().join("revisions").join(&active.install_id);
    let mut visual_index = VisualIndex {
        install_id: Some(install_id.clone()),
        entries: HashMap::new(),
    };
    let keys = key_space(catalog);
    let mut entries = Vec::with_capacity(keys.len());
    let mut decoded_samples = 0_u64;
    let mut over_memory_bound = false;

    for (id, tier) in keys {
        pause();
        if over_memory_bound {
            entries.push((id, tier, Entry::Invalid));
            continue;
        }

        let resolved = match store.resolve(id.as_str(), tier.map(u32::from)) {
            Ok(resolved) if resolved.install_id == active.install_id => resolved,
            Ok(_) | Err(ResolveError::Invalid(_)) => {
                entries.push((id, tier, Entry::Invalid));
                continue;
            }
            Err(ResolveError::Unmapped | ResolveError::Inert) => continue,
        };

        let audio_bytes = match fs::read(&resolved.mixed_audio) {
            Ok(bytes) => bytes,
            Err(_) => {
                entries.push((id, tier, Entry::Invalid));
                continue;
            }
        };
        let Some(pcm) = decode_wav(&audio_bytes) else {
            entries.push((id, tier, Entry::Invalid));
            continue;
        };
        let sample_count = u64::try_from(pcm.len()).unwrap_or(u64::MAX);
        if sample_count > MAX_PREWARMED_SAMPLES.saturating_sub(decoded_samples) {
            over_memory_bound = true;
            entries.push((id, tier, Entry::Invalid));
            continue;
        }

        let visual_url = if let Some(path) = resolved.silent_visual.as_deref() {
            let Some((token, visual)) = visual_entry(&revision_root, path) else {
                entries.push((id, tier, Entry::Invalid));
                continue;
            };
            visual_index.entries.insert(token.clone(), visual);
            Some(Arc::<str>::from(format!(
                "kohbor-media://localhost/v1/{}/{token}",
                active.install_id
            )))
        } else {
            None
        };

        decoded_samples = decoded_samples.saturating_add(sample_count);
        entries.push((
            id,
            tier,
            Entry::Ready(Clip {
                pcm,
                bound_tier: tier,
                audio_ms: audio_duration_ms(sample_count),
                visual_tail_ms: resolved.visual_tail_ms,
                visual_url,
            }),
        ));
    }

    (
        Arc::new(PrewarmTable::new(fence, Some(install_id), entries)),
        visual_index,
    )
}

fn install_number(install_id: &str) -> u64 {
    let Some(digits) = install_id.strip_prefix("ins_") else {
        return 0;
    };
    if digits.len() != 10 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return 0;
    }
    digits.parse::<u64>().unwrap_or_default()
}

fn audio_duration_ms(samples: u64) -> u32 {
    let milliseconds = samples
        .saturating_mul(1_000)
        .saturating_add(AUDIO_SAMPLE_RATE.saturating_sub(1))
        / AUDIO_SAMPLE_RATE;
    u32::try_from(milliseconds).unwrap_or(u32::MAX)
}

fn visual_entry(revision_root: &Path, path: &Path) -> Option<(String, VisualEntry)> {
    let relative = path.strip_prefix(revision_root).ok()?;
    let descriptor_path = relative.to_str()?.replace('\\', "/");
    if descriptor_path.is_empty() || descriptor_path.starts_with('/') {
        return None;
    }
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    let modified = metadata.modified().ok()?;
    let digest = Sha256::digest(descriptor_path.as_bytes());
    let token = format!("{digest:x}");
    Some((
        token,
        VisualEntry {
            path: path.to_path_buf(),
            size_bytes: metadata.len(),
            modified,
        },
    ))
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use kohbor_pack::catalog::Catalog;
    use kohbor_pack::store::{
        ActivationPolicy, ActivationState, InstallRequest, InstallResult, Store,
    };
    use kohbor_pack_author::{build_package, BuildOptions};
    use serde_json::{json, Value};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(label: &str) -> Self {
            // CARGO_TARGET_TMPDIR exists only for integration tests, not for lib unit
            // tests: keep the scratch area inside the target directory by walking up
            // from the test executable (target/<profile>/deps/<exe>) to target/.
            let base = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.ancestors().nth(3).map(|target| target.join("tmp")))
                .unwrap_or_else(std::env::temp_dir);
            fs::create_dir_all(&base).expect("create Cargo target temporary root");
            let index = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0);
            let path = base.join(format!(
                "kohbor-s3-5i-{label}-{}-{nanos}-{index}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create unique media test directory");
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Clone, Copy)]
    pub struct BindingSpec {
        pub event_id: &'static str,
        pub tier: Option<u32>,
        pub samples: usize,
        pub visual: bool,
        pub visual_tail_ms: u32,
    }

    pub struct Package {
        pub package_path: PathBuf,
        pub digest: String,
        pub package_id: String,
    }

    pub fn catalog() -> Catalog {
        let value: Value =
            serde_json::from_str(include_str!("../../../../../contracts/event-catalog.json"))
                .expect("repository event catalog parses");
        Catalog::from_json(&value).expect("repository event catalog is valid for package use")
    }

    pub fn package(temp: &TempDir, package_id: &str, bindings: &[BindingSpec]) -> Package {
        let clips = temp.path().join("clips");
        fs::create_dir_all(&clips).expect("create generated input clip directory");
        let mut mapping_bindings = Vec::with_capacity(bindings.len());
        for (index, binding) in bindings.iter().enumerate() {
            let audio_name = format!("clip-{index}.wav");
            fs::write(clips.join(&audio_name), pcm_wav(binding.samples))
                .expect("write generated PCM16 WAV input");
            let mut row = json!({
                "event_id": binding.event_id,
                "audio": audio_name,
                "visual_tail_ms": binding.visual_tail_ms,
            });
            if let Some(tier) = binding.tier {
                row["tier"] = json!(tier);
            }
            if binding.visual {
                let visual_name = format!("visual-{index}.webm");
                fs::write(clips.join(&visual_name), webm())
                    .expect("write generated single-track VP9 WebM input");
                row["visual"] = json!(visual_name);
            }
            mapping_bindings.push(row);
        }
        let mapping = temp.path().join("mapping.json");
        let mapping_bytes = serde_json::to_vec(&json!({
            "schema_version": "1.0.0",
            "bindings": mapping_bindings,
        }))
        .expect("serialize generated mapping");
        fs::write(mapping, mapping_bytes).expect("write generated mapping");

        let output = temp.path().join(format!("out-{package_id}"));
        let options = BuildOptions {
            clips_dir: clips,
            map_path: temp.path().join("mapping.json"),
            out_dir: output,
            package_id: package_id.to_owned(),
            revision: 1,
            display_name: "Synthetic media service fixture".to_owned(),
            catalog_path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../contracts/event-catalog.json"),
            rights_claim: None,
            publisher_key_id: None,
            require_visual: false,
        };
        let report = build_package(&options).expect("author generated fixture package");
        Package {
            package_path: report.package_path,
            digest: report.container_sha256,
            package_id: package_id.to_owned(),
        }
    }

    pub fn install_request_json(package: &Package, request_id: &str, policy: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "schema_version": "1.0.0",
            "request_id": request_id,
            "profile_id": "test-profile",
            "source_path": package.package_path.to_string_lossy(),
            "expected_container_sha256": package.digest,
            "project_binding": null,
            "activation_policy": policy,
        }))
        .expect("serialize valid install request")
    }

    pub fn install(
        store: &mut Store<kohbor_pack::store::NoHook>,
        package: &Package,
        request_id: &str,
    ) -> InstallResult {
        let request_bytes = install_request_json(package, request_id, "never");
        let request =
            InstallRequest::from_json(&request_bytes).expect("parse test install request");
        store.install(&request).expect("install generated package")
    }

    pub fn activate(
        store: &mut Store<kohbor_pack::store::NoHook>,
        request_id: &str,
        install_id: &str,
    ) -> InstallResult {
        store
            .activate(
                request_id,
                install_id,
                ActivationPolicy::NowIfIdle,
                &ActivationState::default(),
            )
            .expect("activate generated package")
    }

    pub fn pcm_wav(samples: usize) -> Vec<u8> {
        let data_len = u32::try_from(samples.saturating_mul(2)).expect("fixture WAV fits u32");
        let riff_len = 36_u32.checked_add(data_len).expect("fixture RIFF fits u32");
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&riff_len.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&48_000_u32.to_le_bytes());
        bytes.extend_from_slice(&96_000_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        let payload = usize::try_from(data_len).expect("fixture data size fits usize");
        bytes.resize(
            44_usize
                .checked_add(payload)
                .expect("fixture WAV fits usize"),
            0,
        );
        bytes
    }

    fn ebml_element(id: &[u8], data: &[u8]) -> Vec<u8> {
        let size = u8::try_from(data.len()).expect("test EBML element is small");
        let mut bytes = id.to_vec();
        bytes.push(0x80 | size);
        bytes.extend_from_slice(data);
        bytes
    }

    fn ebml_uint(id: &[u8], value: u64) -> Vec<u8> {
        let data = value.to_be_bytes();
        let first = data
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(data.len().saturating_sub(1));
        ebml_element(id, &data[first..])
    }

    pub fn webm() -> Vec<u8> {
        let mut header = ebml_element(&[0x42, 0x82], b"webm");
        header.extend(ebml_uint(&[0x42, 0x86], 1));
        let mut video = ebml_uint(&[0xb0], 640);
        video.extend(ebml_uint(&[0xba], 360));
        let mut track = ebml_uint(&[0xd7], 1);
        track.extend(ebml_uint(&[0x73, 0xc5], 1));
        track.extend(ebml_uint(&[0x83], 1));
        track.extend(ebml_element(&[0x86], b"V_VP9"));
        track.extend(ebml_element(&[0xe0], &video));
        let tracks = ebml_element(&[0xae], &track);
        let segment = ebml_element(&[0x16, 0x54, 0xae, 0x6b], &tracks);
        let mut bytes = ebml_element(&[0x1a, 0x45, 0xdf, 0xa3], &header);
        bytes.extend(ebml_element(&[0x18, 0x53, 0x80, 0x67], &segment));
        bytes
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::sync::atomic::Ordering;

    use kohbor_audio::CriticalMedia;
    use kohbor_events::EventId;
    use kohbor_pack::store::{ResolveError, Store};
    use kohbor_pack::Limits;
    use serde_json::Value;

    use super::test_support::{activate, catalog, install, package, BindingSpec, TempDir};
    use super::{build_table, key_space, new_visual_index_handle};
    use crate::audio::lanes::EngineShared;
    use crate::audio::resolve::Lookup;
    use crate::media::inventory::{facts_from_table, media_status, ALL_EVENTS};

    fn open_store(root: &TempDir) -> Store<kohbor_pack::store::NoHook> {
        Store::open(&root.path().join("store"), catalog(), Limits::default())
            .expect("open isolated media store")
    }

    fn audio(event_id: &'static str, samples: usize) -> BindingSpec {
        BindingSpec {
            event_id,
            tier: None,
            samples,
            visual: false,
            visual_tail_ms: 0,
        }
    }

    fn activated_store(
        root: &TempDir,
        package_id: &str,
        bindings: &[BindingSpec],
        request_prefix: &str,
    ) -> (Store<kohbor_pack::store::NoHook>, String) {
        let mut store = open_store(root);
        let pkg = package(root, package_id, bindings);
        let installed = install(&mut store, &pkg, &format!("{request_prefix}-install"));
        let install_id = installed
            .record
            .expect("generated package install returns a record")
            .install_id;
        let activated = activate(&mut store, &format!("{request_prefix}-active"), &install_id);
        assert_eq!(
            activated.status.as_str(),
            "active",
            "generated package activates"
        );
        (store, install_id)
    }

    #[test]
    fn key_space_count_matches_an_independent_catalog_calculation() {
        let catalog_json: Value =
            serde_json::from_str(include_str!("../../../../../contracts/event-catalog.json"))
                .expect("independent event catalog JSON parses");
        let events = catalog_json["events"]
            .as_array()
            .expect("catalog events exist");
        let voiced: Vec<_> = ALL_EVENTS
            .iter()
            .copied()
            .filter(|id| id.voiced())
            .collect();
        let mut expected = voiced.len();
        for id in &voiced {
            if let Some(event) = events.iter().find(|event| event["id"] == id.as_str()) {
                if let Some(range) = event.get("tier_range") {
                    let minimum = range["min"].as_u64().expect("tier minimum is numeric");
                    let maximum = range["max"].as_u64().expect("tier maximum is numeric");
                    expected += usize::try_from(maximum - minimum + 1)
                        .expect("catalog tier count fits usize");
                }
            }
        }
        assert_eq!(
            key_space(&catalog()).len(),
            expected,
            "keys follow the independent catalog count"
        );
        assert_eq!(expected, 23, "the independent count is 23 for this catalog");
    }

    #[test]
    fn key_space_contains_a_tierless_key_for_every_voiced_host_event() {
        let keys = key_space(&catalog());
        let actual: Vec<_> = keys
            .iter()
            .filter_map(|(id, tier)| tier.is_none().then_some(*id))
            .collect();
        let expected: Vec<_> = ALL_EVENTS
            .iter()
            .copied()
            .filter(|id| id.voiced())
            .collect();
        assert_eq!(
            actual, expected,
            "tierless keys preserve host inventory order"
        );
    }

    #[test]
    fn key_space_omits_unvoiced_hero_respawn() {
        assert!(
            !key_space(&catalog())
                .iter()
                .any(|(id, _)| *id == EventId::HeroRespawn),
            "hero_respawn has no prewarm key"
        );
    }

    #[test]
    fn key_space_derives_multikill_tiers_from_the_catalog() {
        let actual: Vec<_> = key_space(&catalog())
            .into_iter()
            .filter_map(|(id, tier)| (id == EventId::MultiKill).then_some(tier))
            .collect();
        assert_eq!(
            actual,
            vec![None, Some(2), Some(3), Some(4), Some(5)],
            "multi_kill range comes from event-catalog.json"
        );
    }

    #[test]
    fn key_space_derives_streak_tiers_from_the_catalog() {
        let actual: Vec<_> = key_space(&catalog())
            .into_iter()
            .filter_map(|(id, tier)| (id == EventId::Streak).then_some(tier))
            .collect();
        assert_eq!(
            actual,
            vec![
                None,
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                Some(7),
                Some(8),
                Some(9),
                Some(10)
            ],
            "streak range comes from event-catalog.json"
        );
    }

    #[test]
    fn key_space_has_no_duplicate_pairs() {
        let keys = key_space(&catalog());
        let unique: HashSet<_> = keys.iter().copied().collect();
        assert_eq!(
            unique.len(),
            keys.len(),
            "derived event and tier pairs are unique"
        );
    }

    #[test]
    fn visual_index_handle_starts_without_install_or_entries() {
        let handle = new_visual_index_handle();
        let index = handle
            .lock()
            .expect("new visual index lock is available")
            .clone();
        assert!(
            index.install_id.is_none(),
            "new visual index has no active install"
        );
        assert!(
            index.entries.is_empty(),
            "new visual index has no visual files"
        );
    }

    #[test]
    fn no_active_store_returns_empty_table_and_visual_index() {
        let temp = TempDir::new("empty-table");
        let store = open_store(&temp);
        let (table, index) = build_table(&store, &catalog(), &mut || {});
        assert_eq!(table.fence, 0, "empty table fence is zero");
        assert!(
            table.install_id.is_none(),
            "empty table has no install identifier"
        );
        assert!(
            index.install_id.is_none(),
            "empty visual index has no install identifier"
        );
        assert!(index.entries.is_empty(), "empty visual index has no paths");
    }

    #[test]
    fn loader_pauses_once_before_every_catalog_key() {
        let temp = TempDir::new("pause-count");
        let (store, _) = activated_store(
            &temp,
            "pause-count-pack",
            &[audio("health_danger", 100)],
            "req-pause",
        );
        let mut calls = 0;
        let _ = build_table(&store, &catalog(), &mut || calls += 1);
        assert_eq!(calls, 23, "pause callback runs before all 23 keys");
    }

    #[test]
    fn audio_only_revision_prewarms_multiple_events_with_install_fence() {
        let temp = TempDir::new("audio-table");
        let (store, install_id) = activated_store(
            &temp,
            "audio-table-pack",
            &[audio("health_danger", 481), audio("hero_kill", 960)],
            "req-audio",
        );
        let (table, index) = build_table(&store, &catalog(), &mut || {});
        assert_eq!(
            table.fence, 1,
            "first generated install number is the table fence"
        );
        assert_eq!(
            table.install_id.as_deref(),
            Some(install_id.as_str()),
            "table uses the active install id"
        );
        assert!(
            index.entries.is_empty(),
            "audio-only revision has no visual entries"
        );
        for (id, samples) in [
            (EventId::HealthDanger, 481_usize),
            (EventId::HeroKill, 960_usize),
        ] {
            match table.lookup(id, None) {
                Lookup::Ready(clip) => {
                    assert_eq!(
                        clip.pcm.len(),
                        samples,
                        "decoded PCM preserves generated sample count"
                    );
                    assert!(
                        clip.visual_url.is_none(),
                        "audio-only clip has no visual URL"
                    );
                }
                Lookup::Invalid | Lookup::Unmapped => {
                    panic!("generated voiced binding should be ready")
                }
            }
        }
    }

    #[test]
    fn audio_duration_uses_ceiling_sample_conversion() {
        let temp = TempDir::new("duration-ceil");
        let (store, _) = activated_store(
            &temp,
            "duration-ceil-pack",
            &[audio("health_danger", 49)],
            "req-duration",
        );
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        match table.lookup(EventId::HealthDanger, None) {
            Lookup::Ready(clip) => {
                assert_eq!(clip.audio_ms, 2, "49 samples round up to two milliseconds")
            }
            Lookup::Invalid | Lookup::Unmapped => {
                panic!("duration fixture binding should be ready")
            }
        }
    }

    #[test]
    fn absent_binding_remains_unmapped_in_the_table() {
        let temp = TempDir::new("unmapped");
        let (store, _) = activated_store(
            &temp,
            "unmapped-pack",
            &[audio("hero_kill", 480)],
            "req-unmapped",
        );
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        assert!(
            matches!(table.lookup(EventId::HealthDanger, None), Lookup::Unmapped),
            "unbound event has no placeholder entry"
        );
    }

    #[test]
    fn corrupt_audio_after_activation_becomes_invalid_and_blocks_readiness() {
        let temp = TempDir::new("corrupt-wav");
        let (store, install_id) = activated_store(
            &temp,
            "corrupt-wav-pack",
            &[audio("health_danger", 480)],
            "req-corrupt",
        );
        let revision = temp
            .path()
            .join("store")
            .join("revisions")
            .join(&install_id);
        let descriptor: Value = serde_json::from_slice(
            &fs::read(revision.join("descriptor.json")).expect("installed descriptor is readable"),
        )
        .expect("installed descriptor is valid JSON");
        let relative = descriptor["bindings"][0]["mixed_audio_path"]
            .as_str()
            .expect("binding has mixed audio path");
        let path = revision.join(relative);
        let mut bytes = fs::read(&path).expect("verified WAV is readable");
        *bytes
            .get_mut(44)
            .expect("WAV sample follows its 44-byte header") ^= 0xff;
        fs::write(path, bytes).expect("flip one installed audio byte");
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        assert!(
            matches!(table.lookup(EventId::HealthDanger, None), Lookup::Invalid),
            "digest recheck rejects modified installed WAV"
        );
        let facts = facts_from_table(&table, &|_| Some(true), true);
        let shared = EngineShared::new();
        shared.heartbeat_ms.store(10, Ordering::Relaxed);
        shared.device_available.store(true, Ordering::Relaxed);
        assert_eq!(
            media_status(&facts, &shared, 10),
            ("not_ready", Some("MEDIA_INVALID")),
            "healthy engine reports invalid critical audio"
        );
    }

    #[test]
    fn audio_only_binding_has_no_visual_index_row_or_url() {
        let temp = TempDir::new("audio-only-index");
        let (store, _) = activated_store(
            &temp,
            "audio-only-index-pack",
            &[audio("health_danger", 480)],
            "req-audioonly",
        );
        let (table, index) = build_table(&store, &catalog(), &mut || {});
        assert!(
            index.entries.is_empty(),
            "audio-only revision has no visual index rows"
        );
        match table.lookup(EventId::HealthDanger, None) {
            Lookup::Ready(clip) => assert!(
                clip.visual_url.is_none(),
                "audio-only clip does not create a logical URL"
            ),
            Lookup::Invalid | Lookup::Unmapped => panic!("audio-only fixture should be ready"),
        }
    }

    #[test]
    fn tiered_rows_support_highest_bound_tier_lookup() {
        let temp = TempDir::new("tier-fallback");
        let bindings = [
            BindingSpec {
                event_id: "multi_kill",
                tier: Some(2),
                samples: 480,
                visual: false,
                visual_tail_ms: 0,
            },
            BindingSpec {
                event_id: "multi_kill",
                tier: Some(3),
                samples: 960,
                visual: false,
                visual_tail_ms: 0,
            },
        ];
        let (store, _) = activated_store(&temp, "tier-fallback-pack", &bindings, "req-tier");
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        match table.lookup(EventId::MultiKill, Some(4)) {
            Lookup::Ready(clip) => {
                assert_eq!(
                    clip.bound_tier,
                    Some(3),
                    "tier four falls back to the highest lower bound"
                );
                assert_eq!(
                    clip.pcm.len(),
                    960,
                    "fallback uses the tier three clip samples"
                );
            }
            Lookup::Invalid | Lookup::Unmapped => {
                panic!("tier three binding should serve tier four")
            }
        }
    }

    #[test]
    fn malformed_install_identifier_parses_defensively_as_zero() {
        for malformed in ["bad-id", "ins_000000000x", "ins_12345678901"] {
            assert_eq!(
                super::install_number(malformed),
                0,
                "malformed install id {malformed} has fence zero"
            );
        }
    }

    #[test]
    fn valid_install_identifier_parses_its_decimal_install_number() {
        assert_eq!(
            super::install_number("ins_0000000007"),
            7,
            "ten decimal suffix digits become fence seven"
        );
    }

    #[test]
    fn resolving_without_an_active_revision_is_unmapped() {
        let temp = TempDir::new("unmapped-resolve");
        let store = open_store(&temp);
        assert_eq!(
            store.resolve("health_danger", None),
            Err(ResolveError::Unmapped),
            "store leaves absent active revision unmapped"
        );
    }

    #[test]
    fn audio_only_visual_index_still_names_active_revision() {
        let temp = TempDir::new("visual-empty-revision");
        let (store, install_id) = activated_store(
            &temp,
            "visual-empty-revision-pack",
            &[audio("health_danger", 480)],
            "req-visual-empty",
        );
        let (_, index) = build_table(&store, &catalog(), &mut || {});
        assert_eq!(
            index.install_id.as_deref(),
            Some(install_id.as_str()),
            "empty visual index remains revision scoped"
        );
    }

    #[test]
    fn audio_only_critical_readiness_skips_the_visual_probe() {
        let temp = TempDir::new("critical-facts");
        let (store, _) = activated_store(
            &temp,
            "critical-facts-pack",
            &[audio("health_danger", 480)],
            "req-critical",
        );
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        let facts = facts_from_table(&table, &|_| Some(false), true);
        assert_eq!(
            facts.critical.first().map(|fact| fact.probe_ok),
            Some(Some(true)),
            "audio-only critical media is probe-safe"
        );
    }

    #[test]
    fn active_prewarmer_facts_include_expected_health_danger_record() {
        let temp = TempDir::new("critical-record");
        let (store, _) = activated_store(
            &temp,
            "critical-record-pack",
            &[audio("health_danger", 480)],
            "req-critical-record",
        );
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        let facts = facts_from_table(&table, &|_| None, true);
        let expected = CriticalMedia {
            event_id: "health_danger".to_owned(),
            mapped: true,
            verified: true,
            prewarmed: true,
            probe_ok: Some(true),
        };
        assert_eq!(
            facts.critical,
            vec![expected],
            "host critical inventory maps to the prewarmed audio clip"
        );
    }

    #[test]
    fn configured_catalog_order_is_reflected_in_every_prepared_entry() {
        let temp = TempDir::new("key-order");
        let (store, _) = activated_store(
            &temp,
            "key-order-pack",
            &[audio("health_danger", 480)],
            "req-key-order",
        );
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        assert!(
            matches!(table.lookup(EventId::HealthDanger, None), Lookup::Ready(_)),
            "mapped key survives complete catalog traversal"
        );
        assert_eq!(
            key_space(&catalog()).first().copied(),
            Some((EventId::MultiKill, None)),
            "prewarm order starts with host inventory declaration order"
        );
    }
}
