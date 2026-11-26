use crate::atomic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

const FILE_NAME: &str = "lane-history.json";
const CALIBRATION_FILE_NAME: &str = "lane-calibration-v1.json";
const TOKEN_FILE_NAME: &str = "lane-history-seen.json";
const PENDING_FILE_NAME: &str = "lane-history-pending.json";
const HISTORY_SCHEMA: u32 = 2;
const TOKEN_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct LaneHeroRecord {
    pub(crate) hero: String,
    pub(crate) role: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct LaneHistoryRecord {
    pub(crate) patch: Option<String>,
    pub(crate) game_mode: Option<u64>,
    pub(crate) lane: String,
    pub(crate) shape: String,
    pub(crate) radiant: Vec<LaneHeroRecord>,
    pub(crate) dire: Vec<LaneHeroRecord>,
    pub(crate) score: Option<f64>,
    pub(crate) score_reason: Option<String>,
    pub(crate) constants_version: u32,
    pub(crate) own_lane: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct LaneHistoryFile {
    schema: u32,
    records: Vec<LaneHistoryRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SeenTokensFile {
    schema: u32,
    tokens: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct PendingBundleFile {
    schema: u32,
    history: LaneHistoryFile,
    token: String,
}

#[derive(Clone)]
pub(crate) struct LaneHistoryStore {
    path: PathBuf,
    tokens_path: PathBuf,
    pending_path: PathBuf,
    install_key: Option<[u8; 32]>,
    write_gate: Arc<Mutex<()>>,
}

impl LaneHistoryStore {
    pub(crate) fn open(analytics_root: &Path, install_key: Option<[u8; 32]>) -> Self {
        Self {
            path: analytics_root.join(FILE_NAME),
            tokens_path: analytics_root.join(TOKEN_FILE_NAME),
            pending_path: analytics_root.join(PENDING_FILE_NAME),
            install_key,
            write_gate: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn records(&self) -> Result<Vec<LaneHistoryRecord>, &'static str> {
        let _guard = lock(&self.write_gate);
        self.current_file_locked().map(|file| file.records)
    }

    pub(crate) fn append(&self, records: &[LaneHistoryRecord]) -> Result<(), &'static str> {
        if records.is_empty() {
            return Ok(());
        }
        let _guard = lock(&self.write_gate);
        let mut all = self.current_file_locked()?.records;
        all.extend_from_slice(records);
        let maximum = super::lane_expectation::config_integer("/lane_store_max_records");
        if all.len() > maximum {
            let excess = all.len() - maximum;
            all.drain(..excess);
        }
        self.write_history_file(&LaneHistoryFile {
            schema: HISTORY_SCHEMA,
            records: all,
        })
    }

    pub(crate) fn append_bundle(
        &self,
        bundle_hash: &str,
        records: &[LaneHistoryRecord],
    ) -> Result<bool, &'static str> {
        if records.is_empty() {
            return Ok(false);
        }
        if !valid_sha256(bundle_hash) {
            return Err("bundle_hash_invalid");
        }
        let key = self.install_key.as_ref().ok_or("install_key_unavailable")?;
        let token = hmac_bundle_token(key, bundle_hash.as_bytes());
        let _guard = lock(&self.write_gate);
        let mut history = self.current_file_locked()?;
        let seen = self.read_tokens_file()?;
        if seen.tokens.iter().any(|previous| previous == &token) {
            return Ok(false);
        }

        history.records.extend_from_slice(records);
        let maximum = super::lane_expectation::config_integer("/lane_store_max_records");
        if history.records.len() > maximum {
            let excess = history.records.len() - maximum;
            history.records.drain(..excess);
        }
        let pending = PendingBundleFile {
            schema: 1,
            history,
            token,
        };
        self.write_json_file(&self.pending_path, &pending)?;
        self.recover_pending_locked()?;
        Ok(true)
    }

    pub(crate) fn clear(&self) -> Result<(), &'static str> {
        let _guard = lock(&self.write_gate);
        for path in [
            self.path.clone(),
            self.report_path(),
            self.tokens_path.clone(),
            self.pending_path.clone(),
        ] {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("lane_history_delete_failed"),
            }
        }
        Ok(())
    }

    pub(crate) fn calibration_report(&self) -> Result<Option<Value>, &'static str> {
        let bytes = match fs::read(self.report_path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("lane_calibration_read_failed"),
        };
        let value = serde_json::from_slice(&bytes).map_err(|_| "lane_calibration_corrupt")?;
        Ok(Some(value))
    }

    pub(crate) fn write_calibration_report(&self, bytes: &[u8]) -> Result<(), &'static str> {
        let _guard = lock(&self.write_gate);
        let path = self.report_path();
        if path.exists() {
            return Err("lane_calibration_already_exists");
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| "lane_calibration_write_failed")?;
        }
        atomic::write(&path, bytes).map_err(|_| "lane_calibration_write_failed")
    }

    fn report_path(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(CALIBRATION_FILE_NAME)
    }

    fn current_file_locked(&self) -> Result<LaneHistoryFile, &'static str> {
        if matches!(read_file(&self.path), Ok(LaneHistoryFile { schema: 1, .. })) {
            self.write_history_file(&LaneHistoryFile {
                schema: HISTORY_SCHEMA,
                records: Vec::new(),
            })?;
            self.remove_if_exists(&self.tokens_path)?;
            self.remove_if_exists(&self.pending_path)?;
            return Ok(LaneHistoryFile {
                schema: HISTORY_SCHEMA,
                records: Vec::new(),
            });
        }
        self.recover_pending_locked()?;
        match read_file(&self.path) {
            Ok(file) => Ok(file),
            Err("lane_history_corrupt") => {
                crate::log_warn!(
                    "analytics",
                    "lane_history_corrupt",
                    "lane history file is invalid; treating its records as unavailable"
                );
                Ok(LaneHistoryFile {
                    schema: HISTORY_SCHEMA,
                    records: Vec::new(),
                })
            }
            Err(error) => Err(error),
        }
    }

    fn read_tokens_file(&self) -> Result<SeenTokensFile, &'static str> {
        let bytes = match fs::read(&self.tokens_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(SeenTokensFile {
                    schema: TOKEN_SCHEMA,
                    tokens: Vec::new(),
                });
            }
            Err(_) => return Err("lane_history_read_failed"),
        };
        let file: SeenTokensFile =
            serde_json::from_slice(&bytes).map_err(|_| "lane_history_corrupt")?;
        validate_tokens_file(&file)?;
        Ok(file)
    }

    fn write_history_file(&self, file: &LaneHistoryFile) -> Result<(), &'static str> {
        self.write_json_file(&self.path, file)
    }

    fn write_json_file<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), &'static str> {
        let bytes = serde_json::to_vec(value).map_err(|_| "lane_history_write_failed")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| "lane_history_write_failed")?;
        }
        atomic::write(path, &bytes).map_err(|_| "lane_history_write_failed")
    }

    fn recover_pending_locked(&self) -> Result<(), &'static str> {
        let bytes = match fs::read(&self.pending_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err("lane_history_read_failed"),
        };
        let pending: PendingBundleFile =
            serde_json::from_slice(&bytes).map_err(|_| "lane_history_corrupt")?;
        if pending.schema != 1
            || pending.history.schema != HISTORY_SCHEMA
            || validate_history_file(&pending.history).is_err()
        {
            return Err("lane_history_corrupt");
        }
        if !valid_sha256(&pending.token) {
            return Err("lane_history_corrupt");
        }
        let mut seen = self.read_tokens_file()?;
        if !seen.tokens.iter().any(|token| token == &pending.token) {
            seen.tokens.push(pending.token);
            let maximum =
                (super::lane_expectation::config_integer("/lane_store_max_records") / 3).max(1);
            if seen.tokens.len() > maximum {
                let excess = seen.tokens.len() - maximum;
                seen.tokens.drain(..excess);
            }
        }
        self.write_history_file(&pending.history)?;
        self.write_json_file(&self.tokens_path, &seen)?;
        self.remove_if_exists(&self.pending_path)
    }

    fn remove_if_exists(&self, path: &Path) -> Result<(), &'static str> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("lane_history_delete_failed"),
        }
    }

    #[cfg(test)]
    fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(test)]
    fn tokens_path(&self) -> &Path {
        &self.tokens_path
    }

    #[cfg(test)]
    fn pending_path(&self) -> &Path {
        &self.pending_path
    }

    #[cfg(test)]
    fn seen_tokens(&self) -> Result<Vec<String>, &'static str> {
        Ok(self.read_tokens_file()?.tokens)
    }
}

pub(crate) fn records_from_digest(
    digest: &Value,
    own_hero: Option<&str>,
) -> Vec<LaneHistoryRecord> {
    let patch = digest
        .pointer("/meta/patch")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let game_mode = digest.pointer("/meta/game_mode").and_then(Value::as_u64);
    let Some(lanes) = digest.get("lanes").and_then(Value::as_array) else {
        return Vec::new();
    };
    lanes
        .iter()
        .filter_map(|lane| {
            let lane_name = lane.get("lane")?.as_str()?;
            let radiant = hero_records(lane.get("radiant"));
            let dire = hero_records(lane.get("dire"));
            let own_lane = own_hero.is_some_and(|own| {
                radiant
                    .iter()
                    .chain(dire.iter())
                    .any(|hero| hero.hero == own)
            });
            let score = lane
                .pointer("/score/value")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite());
            let constants_version = lane
                .pointer("/constants/version")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0);
            Some(LaneHistoryRecord {
                patch: patch.clone(),
                game_mode,
                lane: lane_name.to_owned(),
                shape: lane
                    .get("shape")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned(),
                radiant,
                dire,
                score,
                score_reason: lane
                    .pointer("/score/reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                constants_version,
                own_lane,
            })
        })
        .collect()
}

fn hero_records(value: Option<&Value>) -> Vec<LaneHeroRecord> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|hero| {
            Some(LaneHeroRecord {
                hero: hero.get("hero")?.as_str()?.to_owned(),
                role: hero.get("role").and_then(Value::as_str).map(str::to_owned),
            })
        })
        .collect()
}

fn read_file(path: &Path) -> Result<LaneHistoryFile, &'static str> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LaneHistoryFile {
                schema: HISTORY_SCHEMA,
                records: Vec::new(),
            });
        }
        Err(_) => return Err("lane_history_read_failed"),
    };
    let file: LaneHistoryFile =
        serde_json::from_slice(&bytes).map_err(|_| "lane_history_corrupt")?;
    validate_history_file(&file)?;
    Ok(file)
}

fn validate_history_file(file: &LaneHistoryFile) -> Result<(), &'static str> {
    if !matches!(file.schema, 1 | HISTORY_SCHEMA)
        || file.records.len() > super::lane_expectation::config_integer("/lane_store_max_records")
        || file.records.iter().any(|record| {
            !matches!(record.lane.as_str(), "top" | "mid" | "bot")
                || !record.score.is_none_or(f64::is_finite)
                || record.radiant.iter().chain(record.dire.iter()).any(|hero| {
                    !valid_hero_id(&hero.hero)
                        || hero.role.as_deref().is_some_and(|role| !valid_role(role))
                })
        })
    {
        return Err("lane_history_corrupt");
    }
    Ok(())
}

fn validate_tokens_file(file: &SeenTokensFile) -> Result<(), &'static str> {
    let maximum = (super::lane_expectation::config_integer("/lane_store_max_records") / 3).max(1);
    let mut unique = std::collections::BTreeSet::new();
    if file.schema != TOKEN_SCHEMA
        || file.tokens.len() > maximum
        || file
            .tokens
            .iter()
            .any(|token| !valid_sha256(token) || !unique.insert(token))
    {
        return Err("lane_history_corrupt");
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hmac_bundle_token(key: &[u8; 32], bundle_hash: &[u8]) -> String {
    let mut inner_pad = [0x36_u8; 64];
    let mut outer_pad = [0x5c_u8; 64];
    for (index, byte) in key.iter().enumerate() {
        inner_pad[index] ^= *byte;
        outer_pad[index] ^= *byte;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(bundle_hash);
    let digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(digest);
    outer
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn valid_hero_id(value: &str) -> bool {
    value.starts_with("npc_dota_hero_")
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_role(value: &str) -> bool {
    matches!(
        value,
        "carry" | "mid" | "offlane" | "soft_support" | "hard_support"
    )
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_store() -> LaneHistoryStore {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("kohbor-lane-history-{suffix}"));
        LaneHistoryStore::open(&root, Some([29; 32]))
    }

    fn record(index: usize, own: bool) -> LaneHistoryRecord {
        LaneHistoryRecord {
            patch: Some("7.42".into()),
            game_mode: Some(22),
            lane: ["top", "mid", "bot"][index % 3].into(),
            shape: "2v2".into(),
            radiant: vec![LaneHeroRecord {
                hero: "npc_dota_hero_axe".into(),
                role: Some("offlane".into()),
            }],
            dire: vec![LaneHeroRecord {
                hero: "npc_dota_hero_lina".into(),
                role: Some("offlane".into()),
            }],
            score: Some(index as f64),
            score_reason: None,
            constants_version: 0,
            own_lane: own,
        }
    }

    #[test]
    fn history_is_bounded_atomic_and_contains_no_player_or_match_identifiers() {
        let store = temp_store();
        let path = store.path().to_path_buf();
        let maximum = super::super::lane_expectation::config_integer("/lane_store_max_records");
        let rows = (0..=maximum)
            .map(|index| record(index, false))
            .collect::<Vec<_>>();
        store.append(&rows).unwrap();
        let saved = store.records().unwrap();
        assert_eq!(saved.len(), maximum);
        assert_eq!(saved[0].score, Some(1.0));
        let bytes = fs::read_to_string(&path).unwrap();
        for forbidden in [
            "match_id",
            "match_key",
            "player_name",
            "account_id",
            "steamid",
            "bundle_path",
        ] {
            assert!(!bytes.contains(forbidden));
        }
        assert!(!bytes.contains("private-name"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_history_is_treated_as_empty_and_recovered_atomically() {
        let store = temp_store();
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(store.path(), b"not-json").unwrap();
        assert!(store.records().unwrap().is_empty());
        store.append(&[record(0, false)]).unwrap();
        assert_eq!(store.records().unwrap().len(), 1);
        let _ = fs::remove_dir_all(store.path().parent().unwrap());
    }

    #[test]
    fn clear_history_deletes_the_lane_store() {
        let store = temp_store();
        store.append(&[record(0, false)]).unwrap();
        let path = store.path().to_path_buf();
        let report_path = store.report_path();
        store.write_calibration_report(b"{}").unwrap();
        store.clear().unwrap();
        assert!(!path.exists());
        assert!(!report_path.exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn bundle_deduplication_is_hmac_tokenized_and_survives_store_restart() {
        let first = temp_store();
        let root = first.path().parent().unwrap().to_path_buf();
        let bundle_hash = "ab".repeat(32);
        let install_key = [29; 32];

        assert!(first
            .append_bundle(&bundle_hash, &[record(0, true)])
            .unwrap());
        drop(first);

        let restarted = LaneHistoryStore::open(&root, Some(install_key));
        assert!(!restarted
            .append_bundle(&bundle_hash, &[record(0, true)])
            .unwrap());
        assert_eq!(restarted.records().unwrap().len(), 1);
        let token_bytes = fs::read_to_string(restarted.tokens_path()).unwrap();
        assert!(!token_bytes.contains(&bundle_hash));
        assert_eq!(
            restarted.seen_tokens().unwrap(),
            vec![hmac_bundle_token(&install_key, bundle_hash.as_bytes())]
        );
        let history_bytes = fs::read_to_string(restarted.path()).unwrap();
        assert!(!history_bytes.contains(&bundle_hash));
        assert!(!history_bytes.contains("match_id"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clear_history_deletes_the_separate_deduplication_token_file() {
        let store = temp_store();
        store
            .append_bundle(&"cd".repeat(32), &[record(0, true)])
            .unwrap();
        let token_path = store.tokens_path().to_path_buf();
        assert!(token_path.exists());

        store.clear().unwrap();

        assert!(!token_path.exists());
        assert!(!store.pending_path().exists());
        let _ = fs::remove_dir_all(store.path().parent().unwrap());
    }

    #[test]
    fn legacy_lane_store_is_cleared_once_by_the_versioned_migration() {
        let store = temp_store();
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(
            store.path(),
            serde_json::to_vec(&LaneHistoryFile {
                schema: 1,
                records: vec![record(0, true)],
            })
            .unwrap(),
        )
        .unwrap();

        assert!(store.records().unwrap().is_empty());
        let migrated: LaneHistoryFile =
            serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
        assert_eq!(migrated.schema, 2);
        store.append(&[record(1, false)]).unwrap();
        assert_eq!(store.records().unwrap().len(), 1);
        let _ = fs::remove_dir_all(store.path().parent().unwrap());
    }

    #[test]
    fn records_capture_all_three_lanes_and_mark_only_the_own_lane() {
        let digest = json!({"meta":{"patch":"7.42","game_mode":22},"lanes":[
            {"lane":"top","shape":"2v2","radiant":[{"hero":"npc_dota_hero_axe","role":"offlane"}],"dire":[],"score":{"value":1,"reason":null},"constants":{"version":0}},
            {"lane":"mid","shape":"1v1","radiant":[],"dire":[{"hero":"npc_dota_hero_lina","role":"mid"}],"score":{"value":null,"reason":"missing"},"constants":{"version":0}},
            {"lane":"bot","shape":"2v2","radiant":[],"dire":[],"score":{"value":2,"reason":null},"constants":{"version":0}}
        ]});
        let rows = records_from_digest(&digest, Some("npc_dota_hero_axe"));
        assert_eq!(rows.len(), 3);
        assert!(rows[0].own_lane);
        assert!(!rows[1].own_lane && !rows[2].own_lane);
        assert_eq!(rows[1].score, None);
    }
}
