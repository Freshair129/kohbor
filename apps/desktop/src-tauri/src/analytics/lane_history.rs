use crate::atomic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

const FILE_NAME: &str = "lane-history.json";
const CALIBRATION_FILE_NAME: &str = "lane-calibration-v1.json";

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

#[derive(Clone)]
pub(crate) struct LaneHistoryStore {
    path: PathBuf,
    write_gate: Arc<Mutex<()>>,
}

impl LaneHistoryStore {
    pub(crate) fn open(analytics_root: &Path) -> Self {
        Self {
            path: analytics_root.join(FILE_NAME),
            write_gate: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn records(&self) -> Result<Vec<LaneHistoryRecord>, &'static str> {
        match read_file(&self.path) {
            Ok(file) => Ok(file.records),
            Err("lane_history_corrupt") => {
                crate::log_warn!(
                    "analytics",
                    "lane_history_corrupt",
                    "lane history file is invalid; treating its records as unavailable"
                );
                Ok(Vec::new())
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn append(&self, records: &[LaneHistoryRecord]) -> Result<(), &'static str> {
        if records.is_empty() {
            return Ok(());
        }
        let _guard = lock(&self.write_gate);
        let mut all = match read_file(&self.path) {
            Ok(file) => file.records,
            Err("lane_history_corrupt") => {
                crate::log_warn!(
                    "analytics",
                    "lane_history_corrupt",
                    "lane history file is invalid; recovering from new lane records"
                );
                Vec::new()
            }
            Err(error) => return Err(error),
        };
        all.extend_from_slice(records);
        let maximum = super::lane_expectation::config_integer("/lane_store_max_records");
        if all.len() > maximum {
            let excess = all.len() - maximum;
            all.drain(..excess);
        }
        let next = LaneHistoryFile {
            schema: 1,
            records: all,
        };
        let bytes = serde_json::to_vec(&next).map_err(|_| "lane_history_write_failed")?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|_| "lane_history_write_failed")?;
        }
        atomic::write(&self.path, &bytes).map_err(|_| "lane_history_write_failed")
    }

    pub(crate) fn clear(&self) -> Result<(), &'static str> {
        let _guard = lock(&self.write_gate);
        for path in [self.path.clone(), self.report_path()] {
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

    #[cfg(test)]
    fn path(&self) -> &Path {
        &self.path
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
                schema: 1,
                records: Vec::new(),
            });
        }
        Err(_) => return Err("lane_history_read_failed"),
    };
    let file: LaneHistoryFile =
        serde_json::from_slice(&bytes).map_err(|_| "lane_history_corrupt")?;
    if file.schema != 1
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
    Ok(file)
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
        LaneHistoryStore::open(&root)
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
