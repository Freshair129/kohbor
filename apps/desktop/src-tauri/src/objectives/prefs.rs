use kohbor_events::ObjectiveSettings;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

const SCHEMA_VERSION: u8 = 1;
pub(crate) const FILE_NAME: &str = "objective-prefs.json";
const WATER_RUNE: u8 = 1 << 0;
const POWER_RUNE: u8 = 1 << 1;
const BOUNTY_RUNE: u8 = 1 << 2;
const WISDOM_RUNE: u8 = 1 << 3;
const TORMENTOR: u8 = 1 << 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ObjectivePrefs {
    pub(crate) water_rune: bool,
    pub(crate) power_rune: bool,
    pub(crate) bounty_rune: bool,
    pub(crate) wisdom_rune: bool,
    pub(crate) tormentor: bool,
    pub(crate) diagnostic_map_keys: bool,
}

impl Default for ObjectivePrefs {
    fn default() -> Self {
        Self {
            water_rune: true,
            power_rune: true,
            bounty_rune: true,
            wisdom_rune: true,
            tormentor: true,
            diagnostic_map_keys: false,
        }
    }
}

impl ObjectivePrefs {
    fn to_stored(self) -> StoredPrefs {
        StoredPrefs {
            schema_version: SCHEMA_VERSION,
            water_rune: self.water_rune,
            power_rune: self.power_rune,
            bounty_rune: self.bounty_rune,
            wisdom_rune: self.wisdom_rune,
            tormentor: self.tormentor,
            diagnostic_map_keys: self.diagnostic_map_keys,
        }
    }

    pub(crate) fn to_value(self) -> Value {
        json!({
            "schema_version": SCHEMA_VERSION,
            "water_rune": self.water_rune,
            "power_rune": self.power_rune,
            "bounty_rune": self.bounty_rune,
            "wisdom_rune": self.wisdom_rune,
            "tormentor": self.tormentor,
            "diagnostic_map_keys": self.diagnostic_map_keys,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredPrefs {
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
            water_rune: value.water_rune,
            power_rune: value.power_rune,
            bounty_rune: value.bounty_rune,
            wisdom_rune: value.wisdom_rune,
            tormentor: value.tormentor,
            diagnostic_map_keys: value.diagnostic_map_keys,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ObjectivePrefsPatch {
    water_rune: Option<bool>,
    power_rune: Option<bool>,
    bounty_rune: Option<bool>,
    wisdom_rune: Option<bool>,
    tormentor: Option<bool>,
    diagnostic_map_keys: Option<bool>,
}

impl ObjectivePrefsPatch {
    fn apply(self, current: ObjectivePrefs) -> ObjectivePrefs {
        ObjectivePrefs {
            water_rune: self.water_rune.unwrap_or(current.water_rune),
            power_rune: self.power_rune.unwrap_or(current.power_rune),
            bounty_rune: self.bounty_rune.unwrap_or(current.bounty_rune),
            wisdom_rune: self.wisdom_rune.unwrap_or(current.wisdom_rune),
            tormentor: self.tormentor.unwrap_or(current.tormentor),
            diagnostic_map_keys: self
                .diagnostic_map_keys
                .unwrap_or(current.diagnostic_map_keys),
        }
    }
}

pub(crate) trait ObjectivePrefsStore: Send + Sync {
    fn load(&self) -> ObjectivePrefs;
    fn save(&self, prefs: ObjectivePrefs) -> Result<(), String>;
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
        fs::read(&self.path)
            .map(|bytes| parse_or_default(&bytes))
            .unwrap_or_default()
    }

    fn save(&self, prefs: ObjectivePrefs) -> Result<(), String> {
        let bytes =
            serde_json::to_vec(&prefs.to_stored()).map_err(|_| "OBJECTIVE_PREFS_WRITE_FAILED")?;
        crate::atomic::write(&self.path, &bytes)
            .map_err(|_| "OBJECTIVE_PREFS_WRITE_FAILED".to_owned())
    }
}

pub(crate) struct ObjectiveRuntimeFlags {
    enabled: AtomicU8,
    diagnostic_enabled: Arc<AtomicBool>,
}

impl ObjectiveRuntimeFlags {
    fn new(prefs: ObjectivePrefs) -> Self {
        let mut enabled = 0;
        enabled |= if prefs.water_rune { WATER_RUNE } else { 0 };
        enabled |= if prefs.power_rune { POWER_RUNE } else { 0 };
        enabled |= if prefs.bounty_rune { BOUNTY_RUNE } else { 0 };
        enabled |= if prefs.wisdom_rune { WISDOM_RUNE } else { 0 };
        enabled |= if prefs.tormentor { TORMENTOR } else { 0 };
        Self {
            enabled: AtomicU8::new(enabled),
            diagnostic_enabled: Arc::new(AtomicBool::new(prefs.diagnostic_map_keys)),
        }
    }

    pub(crate) fn objective_settings(&self) -> ObjectiveSettings {
        let enabled = self.enabled.load(Ordering::Acquire);
        ObjectiveSettings {
            water_rune: enabled & WATER_RUNE != 0,
            power_rune: enabled & POWER_RUNE != 0,
            bounty_rune: enabled & BOUNTY_RUNE != 0,
            wisdom_rune: enabled & WISDOM_RUNE != 0,
            tormentor: enabled & TORMENTOR != 0,
        }
    }

    pub(crate) fn diagnostic_enabled(&self) -> Arc<AtomicBool> {
        // The diagnostic worker keeps a clone of the same atomic switch so an
        // already queued record is dropped immediately after opt-out.
        self.diagnostic_enabled.clone()
    }

    fn update(&self, prefs: ObjectivePrefs) {
        let next = Self::new(prefs);
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
        Self::new(ObjectivePrefs::default())
    }
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
            current: Mutex::new(current),
            flags: Arc::new(ObjectiveRuntimeFlags::new(current)),
        }
    }

    pub(crate) fn current(&self) -> ObjectivePrefs {
        *lock(&self.current)
    }

    pub(crate) fn flags(&self) -> Arc<ObjectiveRuntimeFlags> {
        self.flags.clone()
    }

    pub(crate) fn patch(&self, request: &Value) -> Result<Value, String> {
        let request_object = request
            .as_object()
            .ok_or_else(|| "OBJECTIVE_PREFS_INVALID".to_owned())?;
        const PATCH_KEYS: [&str; 6] = [
            "water_rune",
            "power_rune",
            "bounty_rune",
            "wisdom_rune",
            "tormentor",
            "diagnostic_map_keys",
        ];
        if request_object
            .iter()
            .any(|(key, value)| !PATCH_KEYS.contains(&key.as_str()) || !value.is_boolean())
        {
            return Err("OBJECTIVE_PREFS_INVALID".to_owned());
        }
        let patch = serde_json::from_value::<ObjectivePrefsPatch>(request.clone())
            .map_err(|_| "OBJECTIVE_PREFS_INVALID".to_owned())?;
        let mut current = lock(&self.current);
        let updated = patch.apply(*current);
        if updated != *current {
            self.store.save(updated)?;
            self.flags.update(updated);
            *current = updated;
        }
        Ok(updated.to_value())
    }
}

fn parse_or_default(bytes: &[u8]) -> ObjectivePrefs {
    let Ok(stored) = serde_json::from_slice::<StoredPrefs>(bytes) else {
        return ObjectivePrefs::default();
    };
    if stored.schema_version != SCHEMA_VERSION {
        return ObjectivePrefs::default();
    }
    stored.into()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{parse_or_default, ObjectivePrefs, ObjectivePrefsService, ObjectivePrefsStore};
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    struct MemoryStore(Mutex<ObjectivePrefs>);

    impl ObjectivePrefsStore for MemoryStore {
        fn load(&self) -> ObjectivePrefs {
            *self.0.lock().unwrap()
        }

        fn save(&self, prefs: ObjectivePrefs) -> Result<(), String> {
            *self.0.lock().unwrap() = prefs;
            Ok(())
        }
    }

    fn service() -> (ObjectivePrefsService, Arc<MemoryStore>) {
        let store = Arc::new(MemoryStore(Mutex::new(ObjectivePrefs::default())));
        let service = ObjectivePrefsService::with_store(store.clone());
        (service, store)
    }

    #[test]
    fn strict_parse_rejects_missing_wrong_typed_unknown_and_wrong_version_data() {
        for invalid in [
            br#"{"schema_version":1,"water_rune":true}"#.as_slice(),
            br#"{"schema_version":1,"water_rune":"yes","power_rune":true,"bounty_rune":true,"wisdom_rune":true,"tormentor":true,"diagnostic_map_keys":false}"#,
            br#"{"schema_version":1,"water_rune":true,"power_rune":true,"bounty_rune":true,"wisdom_rune":true,"tormentor":true,"diagnostic_map_keys":false,"extra":0}"#,
            br#"{"schema_version":2,"water_rune":true,"power_rune":true,"bounty_rune":true,"wisdom_rune":true,"tormentor":true,"diagnostic_map_keys":false}"#,
        ] {
            assert_eq!(parse_or_default(invalid), ObjectivePrefs::default());
        }
    }

    #[test]
    fn defaults_enable_each_objective_and_leave_diagnostics_off() {
        let prefs = ObjectivePrefs::default();
        assert!(prefs.water_rune && prefs.power_rune && prefs.bounty_rune);
        assert!(prefs.wisdom_rune && prefs.tormentor);
        assert!(!prefs.diagnostic_map_keys);
    }

    #[test]
    fn strict_patch_changes_only_named_fields_and_rejects_unknown_or_wrong_types() {
        let (service, store) = service();
        let saved = service
            .patch(&json!({"water_rune": false, "diagnostic_map_keys": true}))
            .unwrap();
        assert_eq!(saved["water_rune"], false);
        assert_eq!(saved["power_rune"], true);
        assert_eq!(saved["diagnostic_map_keys"], true);
        assert!(!store.0.lock().unwrap().water_rune);
        assert!(service.patch(&json!({"water_rune": "off"})).is_err());
        assert!(service.patch(&json!({"water_rune": null})).is_err());
        assert!(service.patch(&json!([])).is_err());
        assert!(service
            .patch(&json!({"water_rune": true, "extra": false}))
            .is_err());
        assert!(
            !service.current().water_rune,
            "invalid patches do not partially apply"
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
