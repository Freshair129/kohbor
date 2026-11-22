use kohbor_events::{ObjectiveId, ObjectiveSettings};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

const SCHEMA_VERSION: u8 = 2;
const FILE_NAME: &str = "objective-prefs.json";
const WATER_RUNE: u8 = 1 << 0;
const POWER_RUNE: u8 = 1 << 1;
const BOUNTY_RUNE: u8 = 1 << 2;
const WISDOM_RUNE: u8 = 1 << 3;
const TORMENTOR: u8 = 1 << 4;
const LEAD_BITS_PER_OBJECTIVE: u32 = 12;
const LEAD_SLOT_BITS: u32 = 6;
const LEAD_SLOT_MASK: u64 = 0x3f;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObjectivePreference {
    pub(crate) enabled: bool,
    pub(crate) lead_s: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObjectivePrefs {
    pub(crate) water_rune: ObjectivePreference,
    pub(crate) power_rune: ObjectivePreference,
    pub(crate) bounty_rune: ObjectivePreference,
    pub(crate) wisdom_rune: ObjectivePreference,
    pub(crate) tormentor: ObjectivePreference,
    pub(crate) diagnostic_map_keys: bool,
}

impl Default for ObjectivePrefs {
    fn default() -> Self {
        Self {
            water_rune: ObjectivePreference {
                enabled: true,
                lead_s: vec![30, 15],
            },
            power_rune: ObjectivePreference {
                enabled: true,
                lead_s: vec![30, 15],
            },
            bounty_rune: ObjectivePreference {
                enabled: true,
                lead_s: Vec::new(),
            },
            wisdom_rune: ObjectivePreference {
                enabled: true,
                lead_s: Vec::new(),
            },
            tormentor: ObjectivePreference {
                enabled: true,
                lead_s: Vec::new(),
            },
            diagnostic_map_keys: false,
        }
    }
}

impl ObjectivePrefs {
    fn preference(&self, objective: ObjectiveId) -> &ObjectivePreference {
        match objective {
            ObjectiveId::WaterRune => &self.water_rune,
            ObjectiveId::PowerRune => &self.power_rune,
            ObjectiveId::BountyRune => &self.bounty_rune,
            ObjectiveId::WisdomRune => &self.wisdom_rune,
            ObjectiveId::Tormentor => &self.tormentor,
        }
    }

    fn to_stored(&self) -> StoredPrefs {
        StoredPrefs {
            schema_version: SCHEMA_VERSION,
            objectives: StoredObjectives {
                water_rune: self.water_rune.clone(),
                power_rune: self.power_rune.clone(),
                bounty_rune: self.bounty_rune.clone(),
                wisdom_rune: self.wisdom_rune.clone(),
                tormentor: self.tormentor.clone(),
            },
            diagnostic_map_keys: self.diagnostic_map_keys,
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        json!(self.to_stored())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPrefs {
    schema_version: u8,
    objectives: StoredObjectives,
    diagnostic_map_keys: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredObjectives {
    water_rune: ObjectivePreference,
    power_rune: ObjectivePreference,
    bounty_rune: ObjectivePreference,
    wisdom_rune: ObjectivePreference,
    tormentor: ObjectivePreference,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPrefsV1 {
    schema_version: u8,
    water_rune: bool,
    power_rune: bool,
    bounty_rune: bool,
    wisdom_rune: bool,
    tormentor: bool,
    diagnostic_map_keys: bool,
}

impl From<StoredPrefs> for ObjectivePrefs {
    fn from(value: StoredPrefs) -> Self {
        Self {
            water_rune: value.objectives.water_rune,
            power_rune: value.objectives.power_rune,
            bounty_rune: value.objectives.bounty_rune,
            wisdom_rune: value.objectives.wisdom_rune,
            tormentor: value.objectives.tormentor,
            diagnostic_map_keys: value.diagnostic_map_keys,
        }
    }
}

impl StoredPrefsV1 {
    fn migrate(self) -> ObjectivePrefs {
        let mut prefs = ObjectivePrefs::default();
        prefs.water_rune.enabled = self.water_rune;
        prefs.power_rune.enabled = self.power_rune;
        prefs.bounty_rune.enabled = self.bounty_rune;
        prefs.wisdom_rune.enabled = self.wisdom_rune;
        prefs.tormentor.enabled = self.tormentor;
        prefs.diagnostic_map_keys = self.diagnostic_map_keys;
        prefs
    }
}

pub(crate) trait ObjectivePrefsStore: Send + Sync {
    fn load(&self) -> ObjectivePrefs;
    fn save(&self, prefs: &ObjectivePrefs) -> Result<(), String>;
}

struct JsonObjectivePrefsStore {
    path: PathBuf,
}

impl JsonObjectivePrefsStore {
    fn in_data_dir(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(FILE_NAME),
        }
    }
}

impl ObjectivePrefsStore for JsonObjectivePrefsStore {
    fn load(&self) -> ObjectivePrefs {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return ObjectivePrefs::default()
            }
            Err(_) => {
                crate::log_warn!(
                    "objectives",
                    "prefs_unavailable",
                    "objective preferences could not be read"
                );
                return ObjectivePrefs::default();
            }
        };
        match parse_prefs(&bytes) {
            Ok((prefs, migrated)) => {
                if migrated && self.save(&prefs).is_err() {
                    crate::log_warn!(
                        "objectives",
                        "prefs_migration_write_failed",
                        "objective preferences were migrated in memory but could not be saved"
                    );
                }
                prefs
            }
            Err(_) => {
                crate::log_warn!(
                    "objectives",
                    "prefs_invalid",
                    "invalid objective preferences; using defaults"
                );
                ObjectivePrefs::default()
            }
        }
    }

    fn save(&self, prefs: &ObjectivePrefs) -> Result<(), String> {
        let bytes =
            serde_json::to_vec(&prefs.to_stored()).map_err(|_| "OBJECTIVE_PREFS_WRITE_FAILED")?;
        crate::atomic::write(&self.path, &bytes)
            .map_err(|_| "OBJECTIVE_PREFS_WRITE_FAILED".to_owned())
    }
}

pub(crate) struct ObjectiveRuntimeFlags {
    enabled: AtomicU8,
    packed_lead_times: AtomicU64,
    diagnostic_enabled: Arc<AtomicBool>,
}

impl ObjectiveRuntimeFlags {
    fn new(prefs: &ObjectivePrefs) -> Self {
        let mut enabled = 0;
        enabled |= if prefs.water_rune.enabled {
            WATER_RUNE
        } else {
            0
        };
        enabled |= if prefs.power_rune.enabled {
            POWER_RUNE
        } else {
            0
        };
        enabled |= if prefs.bounty_rune.enabled {
            BOUNTY_RUNE
        } else {
            0
        };
        enabled |= if prefs.wisdom_rune.enabled {
            WISDOM_RUNE
        } else {
            0
        };
        enabled |= if prefs.tormentor.enabled {
            TORMENTOR
        } else {
            0
        };
        Self {
            enabled: AtomicU8::new(enabled),
            packed_lead_times: AtomicU64::new(pack_lead_times(prefs)),
            diagnostic_enabled: Arc::new(AtomicBool::new(prefs.diagnostic_map_keys)),
        }
    }

    pub(crate) fn objective_settings(&self) -> ObjectiveSettings {
        let enabled = self.enabled.load(Ordering::Acquire);
        let packed = self.packed_lead_times.load(Ordering::Acquire);
        let mut lead_times = [[None; 2]; 5];
        for objective_index in 0..5 {
            for slot in 0..2 {
                let shift =
                    objective_index as u32 * LEAD_BITS_PER_OBJECTIVE + slot as u32 * LEAD_SLOT_BITS;
                let encoded = ((packed >> shift) & LEAD_SLOT_MASK) as u8;
                lead_times[objective_index][slot] = (encoded != 0).then_some(encoded + 4);
            }
        }
        ObjectiveSettings {
            water_rune: enabled & WATER_RUNE != 0,
            power_rune: enabled & POWER_RUNE != 0,
            bounty_rune: enabled & BOUNTY_RUNE != 0,
            wisdom_rune: enabled & WISDOM_RUNE != 0,
            tormentor: enabled & TORMENTOR != 0,
            lead_times,
        }
    }

    pub(crate) fn diagnostic_enabled(&self) -> Arc<AtomicBool> {
        self.diagnostic_enabled.clone()
    }

    fn update(&self, prefs: &ObjectivePrefs) {
        let next = Self::new(prefs);
        self.packed_lead_times.store(
            next.packed_lead_times.load(Ordering::Relaxed),
            Ordering::Release,
        );
        self.enabled
            .store(next.enabled.load(Ordering::Relaxed), Ordering::Release);
        self.diagnostic_enabled.store(
            next.diagnostic_enabled.load(Ordering::Relaxed),
            Ordering::Release,
        );
    }
}

impl Default for ObjectiveRuntimeFlags {
    fn default() -> Self {
        Self::new(&ObjectivePrefs::default())
    }
}

fn pack_lead_times(prefs: &ObjectivePrefs) -> u64 {
    let mut packed = 0_u64;
    for (index, objective) in [
        ObjectiveId::WaterRune,
        ObjectiveId::PowerRune,
        ObjectiveId::BountyRune,
        ObjectiveId::WisdomRune,
        ObjectiveId::Tormentor,
    ]
    .into_iter()
    .enumerate()
    {
        for (slot, lead_s) in prefs
            .preference(objective)
            .lead_s
            .iter()
            .take(2)
            .enumerate()
        {
            let encoded = u64::from(lead_s - 4);
            let shift = index as u32 * LEAD_BITS_PER_OBJECTIVE + slot as u32 * LEAD_SLOT_BITS;
            packed |= encoded << shift;
        }
    }
    packed
}

pub(crate) struct ObjectivePrefsService {
    store: Arc<dyn ObjectivePrefsStore>,
    current: Mutex<ObjectivePrefs>,
    flags: Arc<ObjectiveRuntimeFlags>,
}

impl ObjectivePrefsService {
    pub(crate) fn open(data_dir: &Path) -> Self {
        Self::with_store(Arc::new(JsonObjectivePrefsStore::in_data_dir(data_dir)))
    }

    fn with_store(store: Arc<dyn ObjectivePrefsStore>) -> Self {
        let current = store.load();
        Self {
            store,
            current: Mutex::new(current.clone()),
            flags: Arc::new(ObjectiveRuntimeFlags::new(&current)),
        }
    }

    pub(crate) fn current(&self) -> ObjectivePrefs {
        lock(&self.current).clone()
    }

    pub(crate) fn flags(&self) -> Arc<ObjectiveRuntimeFlags> {
        self.flags.clone()
    }

    pub(crate) fn patch(&self, request: &Value) -> Result<Value, String> {
        let patch = parse_patch(request)?;
        let mut current = lock(&self.current);
        let mut updated = current.clone();
        let mut changed_keys = Vec::new();

        for objective in [
            ObjectiveId::WaterRune,
            ObjectiveId::PowerRune,
            ObjectiveId::BountyRune,
            ObjectiveId::WisdomRune,
            ObjectiveId::Tormentor,
        ] {
            let Some(change) = patch.objective(objective) else {
                continue;
            };
            let preference = preference_mut(&mut updated, objective);
            if let Some(enabled) = change.enabled {
                if preference.enabled != enabled {
                    preference.enabled = enabled;
                    changed_keys.push(format!("objectives.{}.enabled", objective.as_str()));
                }
            }
            if let Some(lead_s) = &change.lead_s {
                if preference.lead_s != *lead_s {
                    preference.lead_s = lead_s.clone();
                    changed_keys.push(format!("objectives.{}.lead_s", objective.as_str()));
                }
            }
        }
        if let Some(diagnostic_map_keys) = patch.diagnostic_map_keys {
            if updated.diagnostic_map_keys != diagnostic_map_keys {
                updated.diagnostic_map_keys = diagnostic_map_keys;
                changed_keys.push("diagnostic_map_keys".to_owned());
            }
        }

        if updated != *current {
            self.store.save(&updated)?;
            self.flags.update(&updated);
            *current = updated.clone();
            crate::log_info!(
                "objectives",
                "objective_prefs_changed",
                "objective preferences changed",
                changed_keys = changed_keys.join(",")
            );
        }
        Ok(updated.to_value())
    }
}

#[derive(Default)]
struct ObjectivePrefsPatch {
    water_rune: Option<ObjectivePreferencePatch>,
    power_rune: Option<ObjectivePreferencePatch>,
    bounty_rune: Option<ObjectivePreferencePatch>,
    wisdom_rune: Option<ObjectivePreferencePatch>,
    tormentor: Option<ObjectivePreferencePatch>,
    diagnostic_map_keys: Option<bool>,
}

#[derive(Default)]
struct ObjectivePreferencePatch {
    enabled: Option<bool>,
    lead_s: Option<Vec<u8>>,
}

impl ObjectivePrefsPatch {
    fn objective(&self, id: ObjectiveId) -> Option<&ObjectivePreferencePatch> {
        match id {
            ObjectiveId::WaterRune => self.water_rune.as_ref(),
            ObjectiveId::PowerRune => self.power_rune.as_ref(),
            ObjectiveId::BountyRune => self.bounty_rune.as_ref(),
            ObjectiveId::WisdomRune => self.wisdom_rune.as_ref(),
            ObjectiveId::Tormentor => self.tormentor.as_ref(),
        }
    }
}

fn preference_mut(prefs: &mut ObjectivePrefs, id: ObjectiveId) -> &mut ObjectivePreference {
    match id {
        ObjectiveId::WaterRune => &mut prefs.water_rune,
        ObjectiveId::PowerRune => &mut prefs.power_rune,
        ObjectiveId::BountyRune => &mut prefs.bounty_rune,
        ObjectiveId::WisdomRune => &mut prefs.wisdom_rune,
        ObjectiveId::Tormentor => &mut prefs.tormentor,
    }
}

fn parse_patch(value: &Value) -> Result<ObjectivePrefsPatch, String> {
    let invalid = || "OBJECTIVE_PREFS_INVALID".to_owned();
    let object = value.as_object().ok_or_else(invalid)?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "objectives" | "diagnostic_map_keys"))
    {
        return Err(invalid());
    }
    let diagnostic_map_keys = match object.get("diagnostic_map_keys") {
        Some(value) => Some(value.as_bool().ok_or_else(invalid)?),
        None => None,
    };
    let mut patch = ObjectivePrefsPatch {
        diagnostic_map_keys,
        ..ObjectivePrefsPatch::default()
    };
    if let Some(value) = object.get("objectives") {
        let objectives = value.as_object().ok_or_else(invalid)?;
        for (id, value) in objectives {
            let parsed = parse_objective_patch(value)?;
            match id.as_str() {
                "water_rune" => patch.water_rune = Some(parsed),
                "power_rune" => patch.power_rune = Some(parsed),
                "bounty_rune" => patch.bounty_rune = Some(parsed),
                "wisdom_rune" => patch.wisdom_rune = Some(parsed),
                "tormentor" => patch.tormentor = Some(parsed),
                _ => return Err(invalid()),
            }
        }
    }
    if patch.is_empty() {
        return Err(invalid());
    }
    Ok(patch)
}

impl ObjectivePrefsPatch {
    fn is_empty(&self) -> bool {
        self.water_rune.is_none()
            && self.power_rune.is_none()
            && self.bounty_rune.is_none()
            && self.wisdom_rune.is_none()
            && self.tormentor.is_none()
            && self.diagnostic_map_keys.is_none()
    }
}

fn parse_objective_patch(value: &Value) -> Result<ObjectivePreferencePatch, String> {
    let invalid = || "OBJECTIVE_PREFS_INVALID".to_owned();
    let object = value.as_object().ok_or_else(invalid)?;
    if object.is_empty()
        || object
            .keys()
            .any(|key| !matches!(key.as_str(), "enabled" | "lead_s"))
    {
        return Err(invalid());
    }
    let enabled = object
        .get("enabled")
        .map(|value| value.as_bool().ok_or_else(invalid))
        .transpose()?;
    let lead_s = object.get("lead_s").map(parse_lead_times).transpose()?;
    Ok(ObjectivePreferencePatch { enabled, lead_s })
}

fn parse_lead_times(value: &Value) -> Result<Vec<u8>, String> {
    let invalid = || "OBJECTIVE_PREFS_INVALID".to_owned();
    let values = value.as_array().ok_or_else(invalid)?;
    if values.len() > 2 {
        return Err(invalid());
    }
    let mut lead_s = Vec::with_capacity(values.len());
    for value in values {
        let seconds = value
            .as_u64()
            .and_then(|seconds| u8::try_from(seconds).ok())
            .filter(|seconds| (5..=60).contains(seconds))
            .ok_or_else(invalid)?;
        if lead_s.contains(&seconds) {
            return Err(invalid());
        }
        lead_s.push(seconds);
    }
    Ok(lead_s)
}

fn parse_prefs(bytes: &[u8]) -> Result<(ObjectivePrefs, bool), &'static str> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "OBJECTIVE_PREFS_INVALID")?;
    match value.get("schema_version").and_then(Value::as_u64) {
        Some(1) => {
            let stored: StoredPrefsV1 =
                serde_json::from_value(value).map_err(|_| "OBJECTIVE_PREFS_INVALID")?;
            if stored.schema_version != 1 {
                return Err("OBJECTIVE_PREFS_INVALID");
            }
            Ok((stored.migrate(), true))
        }
        Some(2) => {
            let stored: StoredPrefs =
                serde_json::from_value(value).map_err(|_| "OBJECTIVE_PREFS_INVALID")?;
            if stored.schema_version != SCHEMA_VERSION
                || [
                    &stored.objectives.water_rune,
                    &stored.objectives.power_rune,
                    &stored.objectives.bounty_rune,
                    &stored.objectives.wisdom_rune,
                    &stored.objectives.tormentor,
                ]
                .iter()
                .any(|prefs| !valid_lead_times(&prefs.lead_s))
            {
                return Err("OBJECTIVE_PREFS_INVALID");
            }
            Ok((stored.into(), false))
        }
        _ => Err("OBJECTIVE_PREFS_INVALID"),
    }
}

fn valid_lead_times(lead_s: &[u8]) -> bool {
    lead_s.len() <= 2
        && lead_s.iter().all(|seconds| (5..=60).contains(seconds))
        && lead_s
            .iter()
            .enumerate()
            .all(|(index, seconds)| !lead_s[..index].contains(seconds))
}

fn parse_or_default(bytes: &[u8]) -> ObjectivePrefs {
    parse_prefs(bytes)
        .map(|(prefs, _)| prefs)
        .unwrap_or_default()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_or_default, parse_prefs, ObjectivePrefs, ObjectivePrefsService, ObjectivePrefsStore,
        ObjectiveRuntimeFlags,
    };
    use kohbor_events::ObjectiveId;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    struct MemoryStore(Mutex<ObjectivePrefs>);

    impl ObjectivePrefsStore for MemoryStore {
        fn load(&self) -> ObjectivePrefs {
            self.0.lock().unwrap().clone()
        }

        fn save(&self, prefs: &ObjectivePrefs) -> Result<(), String> {
            *self.0.lock().unwrap() = prefs.clone();
            Ok(())
        }
    }

    fn service() -> (ObjectivePrefsService, Arc<MemoryStore>) {
        let store = Arc::new(MemoryStore(Mutex::new(ObjectivePrefs::default())));
        let service = ObjectivePrefsService::with_store(store.clone());
        (service, store)
    }

    fn v1(water_rune: bool) -> Vec<u8> {
        json!({
            "schema_version": 1,
            "water_rune": water_rune,
            "power_rune": true,
            "bounty_rune": false,
            "wisdom_rune": true,
            "tormentor": false,
            "diagnostic_map_keys": true
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn schema_one_migrates_switches_and_diagnostic_to_v2_defaults() {
        let (prefs, migrated) = parse_prefs(&v1(false)).unwrap();
        assert!(migrated);
        assert!(!prefs.water_rune.enabled);
        assert_eq!(prefs.water_rune.lead_s, vec![30, 15]);
        assert_eq!(prefs.power_rune.lead_s, vec![30, 15]);
        assert!(prefs.bounty_rune.lead_s.is_empty());
        assert!(prefs.diagnostic_map_keys);
        assert_eq!(prefs.to_value()["schema_version"], 2);
    }

    #[test]
    fn reading_a_schema_one_file_rewrites_it_as_schema_two() {
        use super::JsonObjectivePrefsStore;
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "kohbor-objective-prefs-migration-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let store = JsonObjectivePrefsStore::in_data_dir(&dir);
        fs::write(&store.path, v1(false)).unwrap();

        let prefs = store.load();
        let migrated = fs::read(&store.path).unwrap();
        assert!(!parse_prefs(&migrated).unwrap().1);
        assert_eq!(prefs.water_rune.lead_s, vec![30, 15]);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&migrated).unwrap()["schema_version"],
            2
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn schema_two_parse_is_closed_and_validates_every_lead_time() {
        let valid = ObjectivePrefs::default().to_value();
        assert_eq!(
            parse_prefs(&valid.to_string().into_bytes()).unwrap().0,
            ObjectivePrefs::default()
        );
        for invalid in [
            json!({"schema_version": 2, "objectives": {}, "diagnostic_map_keys": false}),
            json!({"schema_version": 2, "objectives": {"water_rune": {"enabled": true, "lead_s": [30, 15], "extra": true}}, "diagnostic_map_keys": false}),
            json!({"schema_version": 2, "objectives": {"unknown": {"enabled": true, "lead_s": []}}, "diagnostic_map_keys": false}),
            json!({"schema_version": 2, "objectives": {"water_rune": {"enabled": true, "lead_s": [4]}}, "diagnostic_map_keys": false}),
            json!({"schema_version": 2, "objectives": {"water_rune": {"enabled": true, "lead_s": [61]}}, "diagnostic_map_keys": false}),
            json!({"schema_version": 2, "objectives": {"water_rune": {"enabled": true, "lead_s": [15, 15]}}, "diagnostic_map_keys": false}),
            json!({"schema_version": 2, "objectives": {"water_rune": {"enabled": true, "lead_s": [5, 10, 15]}}, "diagnostic_map_keys": false}),
            json!({"schema_version": 2, "objectives": {"water_rune": {"enabled": "yes", "lead_s": []}}, "diagnostic_map_keys": false}),
        ] {
            assert!(parse_prefs(&invalid.to_string().into_bytes()).is_err());
            assert_eq!(
                parse_or_default(&invalid.to_string().into_bytes()),
                ObjectivePrefs::default()
            );
        }
    }

    #[test]
    fn defaults_enable_each_objective_and_leave_diagnostics_off() {
        let prefs = ObjectivePrefs::default();
        assert!(prefs.water_rune.enabled && prefs.power_rune.enabled && prefs.bounty_rune.enabled);
        assert!(prefs.wisdom_rune.enabled && prefs.tormentor.enabled);
        assert!(!prefs.diagnostic_map_keys);
    }

    #[test]
    fn strict_patch_updates_named_values_and_invalid_requests_keep_the_stored_prefs() {
        let (service, store) = service();
        let saved = service
            .patch(&json!({"objectives": {"water_rune": {"enabled": false, "lead_s": [25, 8]}}, "diagnostic_map_keys": true}))
            .unwrap();
        assert_eq!(saved["schema_version"], 2);
        assert_eq!(saved["objectives"]["water_rune"]["enabled"], false);
        assert_eq!(saved["objectives"]["water_rune"]["lead_s"], json!([25, 8]));
        assert_eq!(saved["objectives"]["power_rune"]["lead_s"], json!([30, 15]));
        assert_eq!(saved["diagnostic_map_keys"], true);

        let before = service.current();
        for invalid in [
            json!({"objectives": {"water_rune": {"enabled": "off"}}}),
            json!({"objectives": {"water_rune": {"lead_s": [4]}}}),
            json!({"objectives": {"water_rune": {"lead_s": [61]}}}),
            json!({"objectives": {"water_rune": {"lead_s": [15, 15]}}}),
            json!({"objectives": {"water_rune": {"lead_s": [5, 10, 15]}}}),
            json!({"objectives": {"water_rune": {"lead_s": [5.5]}}}),
            json!({"objectives": {"water_rune": {"lead_s": null}}}),
            json!({"objectives": {"water_rune": {"unexpected": true}}}),
            json!({"objectives": {"unknown": {"enabled": true}}}),
            json!({"water_rune": false}),
            json!({"diagnostic_map_keys": 1}),
            json!({}),
        ] {
            assert!(service.patch(&invalid).is_err());
            assert_eq!(service.current(), before);
            assert_eq!(*store.0.lock().unwrap(), before);
        }
    }

    #[test]
    fn runtime_snapshot_carries_per_objective_enabled_state_and_leads_without_locks() {
        let (service, _) = service();
        service
            .patch(&json!({"objectives": {"water_rune": {"enabled": false, "lead_s": [60, 5]}, "tormentor": {"lead_s": [9]}}}))
            .unwrap();
        let settings = service.flags().objective_settings();
        assert!(!settings.water_rune);
        assert_eq!(
            settings.lead_times[ObjectiveId::WaterRune.index()],
            [Some(60), Some(5)]
        );
        assert_eq!(
            settings.lead_times[ObjectiveId::Tormentor.index()],
            [Some(9), None]
        );
        assert_eq!(
            ObjectiveRuntimeFlags::default()
                .objective_settings()
                .lead_times[ObjectiveId::PowerRune.index()],
            [Some(30), Some(15)]
        );
    }

    #[test]
    fn data_file_name_is_fixed_under_the_app_data_directory() {
        assert_eq!(
            super::JsonObjectivePrefsStore::in_data_dir(&PathBuf::from("data")).path,
            PathBuf::from("data").join("objective-prefs.json")
        );
    }
}
