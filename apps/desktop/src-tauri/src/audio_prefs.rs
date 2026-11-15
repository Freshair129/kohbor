use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

const SCHEMA_VERSION: u8 = 1;
pub(crate) const FILE_NAME: &str = "audio-prefs.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct AudioPrefs {
    pub(crate) master_volume_percent: u8,
    pub(crate) hotkeys_enabled: bool,
}

impl Default for AudioPrefs {
    fn default() -> Self {
        Self {
            master_volume_percent: 60,
            hotkeys_enabled: true,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredAudioPrefs {
    schema_version: u8,
    master_volume_percent: u8,
    hotkeys_enabled: bool,
}

pub(crate) trait AudioPrefsStore: Send + Sync {
    fn load(&self) -> AudioPrefs;
    fn save(&self, prefs: &AudioPrefs) -> Result<(), String>;
}

pub(crate) struct JsonAudioPrefsStore {
    path: PathBuf,
}

impl JsonAudioPrefsStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn in_data_dir(data_dir: &Path) -> Self {
        Self::new(data_dir.join(FILE_NAME))
    }
}

impl AudioPrefsStore for JsonAudioPrefsStore {
    fn load(&self) -> AudioPrefs {
        fs::read(&self.path)
            .map(|bytes| parse_or_default(&bytes))
            .unwrap_or_default()
    }

    fn save(&self, prefs: &AudioPrefs) -> Result<(), String> {
        let value = StoredAudioPrefs {
            schema_version: SCHEMA_VERSION,
            master_volume_percent: prefs.master_volume_percent.min(100),
            hotkeys_enabled: prefs.hotkeys_enabled,
        };
        let bytes = serde_json::to_vec(&value).map_err(|_| "AUDIO_PREFS_WRITE_FAILED")?;
        crate::atomic::write(&self.path, &bytes).map_err(|_| "AUDIO_PREFS_WRITE_FAILED".to_owned())
    }
}

fn parse_or_default(bytes: &[u8]) -> AudioPrefs {
    let Ok(stored) = serde_json::from_slice::<StoredAudioPrefs>(bytes) else {
        return AudioPrefs::default();
    };
    if stored.schema_version != SCHEMA_VERSION || stored.master_volume_percent > 100 {
        return AudioPrefs::default();
    }
    AudioPrefs {
        master_volume_percent: stored.master_volume_percent,
        hotkeys_enabled: stored.hotkeys_enabled,
    }
}

pub(crate) struct AudioPrefsService {
    store: Arc<dyn AudioPrefsStore>,
    current: Mutex<AudioPrefs>,
}

impl AudioPrefsService {
    pub(crate) fn open(data_dir: &Path) -> Self {
        Self::with_store(Arc::new(JsonAudioPrefsStore::in_data_dir(data_dir)))
    }

    pub(crate) fn with_store(store: Arc<dyn AudioPrefsStore>) -> Self {
        let current = store.load();
        kohbor_audio_native::set_master_volume_percent(current.master_volume_percent);
        Self {
            store,
            current: Mutex::new(current),
        }
    }

    pub(crate) fn current(&self) -> AudioPrefs {
        *lock(&self.current)
    }

    pub(crate) fn set_volume_percent(&self, percent: u8) -> Result<(AudioPrefs, bool), String> {
        let percent = percent.min(100);
        self.update(|current| AudioPrefs {
            master_volume_percent: percent,
            ..current
        })
    }

    pub(crate) fn step_volume(
        &self,
        step: crate::hotkeys::VolumeStep,
    ) -> Result<(AudioPrefs, bool), String> {
        self.update(|current| AudioPrefs {
            master_volume_percent: crate::hotkeys::step_volume(current.master_volume_percent, step),
            ..current
        })
    }

    pub(crate) fn set_hotkeys_enabled(&self, enabled: bool) -> Result<(AudioPrefs, bool), String> {
        self.update(|current| AudioPrefs {
            hotkeys_enabled: enabled,
            ..current
        })
    }

    fn update(
        &self,
        update: impl FnOnce(AudioPrefs) -> AudioPrefs,
    ) -> Result<(AudioPrefs, bool), String> {
        let mut current = lock(&self.current);
        let updated = update(*current);
        if updated == *current {
            return Ok((updated, false));
        }
        self.store.save(&updated)?;
        if updated.master_volume_percent != current.master_volume_percent {
            kohbor_audio_native::set_master_volume_percent(updated.master_volume_percent);
        }
        *current = updated;
        Ok((updated, true))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{parse_or_default, AudioPrefs, AudioPrefsStore, JsonAudioPrefsStore};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "kohbor-audio-prefs-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn valid_preferences() -> &'static [u8] {
        br#"{"schema_version":1,"master_volume_percent":37,"hotkeys_enabled":false}"#
    }

    #[test]
    fn defaults_are_sixty_percent_with_hotkeys_enabled() {
        assert_eq!(AudioPrefs::default().master_volume_percent, 60);
        assert!(AudioPrefs::default().hotkeys_enabled);
    }

    #[test]
    fn a_missing_file_uses_defaults() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let store = JsonAudioPrefsStore::new(dir.join("audio-prefs.json"));

        assert_eq!(store.load(), AudioPrefs::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn strict_parse_uses_defaults_for_missing_fields() {
        assert_eq!(
            parse_or_default(br#"{"schema_version":1,"master_volume_percent":50}"#),
            AudioPrefs::default()
        );
    }

    #[test]
    fn strict_parse_uses_defaults_for_unknown_keys() {
        assert_eq!(
            parse_or_default(
                br#"{"schema_version":1,"master_volume_percent":50,"hotkeys_enabled":true,"extra":1}"#
            ),
            AudioPrefs::default()
        );
    }

    #[test]
    fn strict_parse_uses_defaults_for_wrong_types() {
        assert_eq!(
            parse_or_default(
                br#"{"schema_version":1,"master_volume_percent":"50","hotkeys_enabled":true}"#
            ),
            AudioPrefs::default()
        );
        assert_eq!(
            parse_or_default(
                br#"{"schema_version":1,"master_volume_percent":50,"hotkeys_enabled":1}"#
            ),
            AudioPrefs::default()
        );
    }

    #[test]
    fn strict_parse_uses_defaults_for_out_of_range_volume() {
        assert_eq!(
            parse_or_default(
                br#"{"schema_version":1,"master_volume_percent":101,"hotkeys_enabled":true}"#
            ),
            AudioPrefs::default()
        );
    }

    #[test]
    fn strict_parse_uses_defaults_for_unsupported_schema_version() {
        assert_eq!(
            parse_or_default(
                br#"{"schema_version":2,"master_volume_percent":50,"hotkeys_enabled":true}"#
            ),
            AudioPrefs::default()
        );
    }

    #[test]
    fn strict_parse_accepts_a_valid_preferences_file() {
        assert_eq!(
            parse_or_default(valid_preferences()),
            AudioPrefs {
                master_volume_percent: 37,
                hotkeys_enabled: false,
            }
        );
    }

    #[test]
    fn write_read_round_trip_uses_audio_prefs_file() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("audio-prefs.json");
        let store = JsonAudioPrefsStore::new(path.clone());
        let prefs = AudioPrefs {
            master_volume_percent: 37,
            hotkeys_enabled: false,
        };

        store.save(&prefs).unwrap();

        assert_eq!(path.file_name().unwrap(), "audio-prefs.json");
        assert_eq!(store.load(), prefs);
        let bytes = fs::read(path).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("\"schema_version\":1"));
        let _ = fs::remove_dir_all(dir);
    }
}
