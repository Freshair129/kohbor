use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(crate) const PREFS_FILE: &str = "draft-stats-prefs.json";
const SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DraftStatsPrefs {
    pub(crate) hero_stats_consent: bool,
    pub(crate) draft_bar_enabled: bool,
    pub(crate) position: Option<u8>,
    pub(crate) bracket: Option<u8>,
}

impl Default for DraftStatsPrefs {
    fn default() -> Self {
        Self {
            hero_stats_consent: false,
            draft_bar_enabled: true,
            position: None,
            bracket: None,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredPrefs {
    schema_version: u8,
    hero_stats_consent: bool,
    draft_bar_enabled: bool,
    #[serde(deserialize_with = "required_option")]
    position: Option<u8>,
    #[serde(deserialize_with = "required_option")]
    bracket: Option<u8>,
}

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

impl TryFrom<StoredPrefs> for DraftStatsPrefs {
    type Error = ();

    fn try_from(stored: StoredPrefs) -> Result<Self, Self::Error> {
        if stored.schema_version != SCHEMA_VERSION
            || stored
                .position
                .is_some_and(|position| !(1..=5).contains(&position))
            || stored
                .bracket
                .is_some_and(|bracket| !(1..=8).contains(&bracket))
        {
            return Err(());
        }
        Ok(Self {
            hero_stats_consent: stored.hero_stats_consent,
            draft_bar_enabled: stored.draft_bar_enabled,
            position: stored.position,
            bracket: stored.bracket,
        })
    }
}

pub(crate) fn parse_prefs(bytes: &[u8]) -> Option<DraftStatsPrefs> {
    let stored = serde_json::from_slice::<StoredPrefs>(bytes).ok()?;
    DraftStatsPrefs::try_from(stored).ok()
}

/// A strict command patch. Unknown keys, empty patches and out-of-range values fail closed.
pub(crate) fn patch_prefs(
    current: &DraftStatsPrefs,
    patch: &Value,
) -> Result<DraftStatsPrefs, String> {
    let object = patch.as_object().ok_or("DRAFT_STATS_PREFS_INVALID")?;
    if object.is_empty() {
        return Err("DRAFT_STATS_PREFS_INVALID".to_owned());
    }
    let mut next = current.clone();
    for (key, value) in object {
        match key.as_str() {
            "hero_stats_consent" => {
                next.hero_stats_consent = value.as_bool().ok_or("DRAFT_STATS_PREFS_INVALID")?;
            }
            "draft_bar_enabled" => {
                next.draft_bar_enabled = value.as_bool().ok_or("DRAFT_STATS_PREFS_INVALID")?;
            }
            "position" => next.position = optional_bounded(value, 1, 5)?,
            "bracket" => next.bracket = optional_bounded(value, 1, 8)?,
            _ => return Err("DRAFT_STATS_PREFS_INVALID".to_owned()),
        }
    }
    Ok(next)
}

fn optional_bounded(value: &Value, min: u64, max: u64) -> Result<Option<u8>, String> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_u64()
        .filter(|value| (min..=max).contains(value))
        .map(|value| Some(value as u8))
        .ok_or_else(|| "DRAFT_STATS_PREFS_INVALID".to_owned())
}

pub(crate) struct PrefsStore {
    path: PathBuf,
}

impl PrefsStore {
    pub(crate) fn in_data_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(PREFS_FILE),
        }
    }

    pub(crate) fn load(&self) -> DraftStatsPrefs {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| parse_prefs(&bytes))
            .unwrap_or_default()
    }

    pub(crate) fn save(&self, prefs: &DraftStatsPrefs) -> Result<(), String> {
        let bytes = serde_json::to_vec(&StoredPrefs {
            schema_version: SCHEMA_VERSION,
            hero_stats_consent: prefs.hero_stats_consent,
            draft_bar_enabled: prefs.draft_bar_enabled,
            position: prefs.position,
            bracket: prefs.bracket,
        })
        .map_err(|_| "DRAFT_STATS_PREFS_WRITE_FAILED".to_owned())?;
        crate::atomic::write(&self.path, &bytes)
            .map_err(|_| "DRAFT_STATS_PREFS_WRITE_FAILED".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_prefs, patch_prefs, DraftStatsPrefs, PrefsStore};
    use serde_json::json;

    #[test]
    fn preferences_default_off_and_corrupt_or_incomplete_files_use_defaults() {
        assert_eq!(
            DraftStatsPrefs::default(),
            DraftStatsPrefs {
                hero_stats_consent: false,
                draft_bar_enabled: true,
                position: None,
                bracket: None,
            }
        );
        assert!(parse_prefs(b"{ truncated").is_none());
        assert!(parse_prefs(
            br#"{"schema_version":1,"hero_stats_consent":true,"draft_bar_enabled":true}"#
        )
        .is_none());
        let dir = std::env::temp_dir().join(format!("kohbor-draft-prefs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = PrefsStore::in_data_dir(&dir);
        assert_eq!(store.load(), DraftStatsPrefs::default());
        std::fs::write(dir.join("draft-stats-prefs.json"), b"not json").unwrap();
        assert_eq!(store.load(), DraftStatsPrefs::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn preferences_are_strict_and_validate_the_selected_ranges() {
        assert!(parse_prefs(br#"{"schema_version":1,"hero_stats_consent":true,"draft_bar_enabled":true,"position":6,"bracket":null}"#).is_none());
        assert!(parse_prefs(br#"{"schema_version":1,"hero_stats_consent":true,"draft_bar_enabled":true,"position":null,"bracket":9}"#).is_none());
        assert!(parse_prefs(br#"{"schema_version":1,"hero_stats_consent":true,"draft_bar_enabled":true,"position":null,"bracket":null,"extra":0}"#).is_none());
        let good = br#"{"schema_version":1,"hero_stats_consent":true,"draft_bar_enabled":false,"position":5,"bracket":8}"#;
        assert_eq!(
            parse_prefs(good),
            Some(DraftStatsPrefs {
                hero_stats_consent: true,
                draft_bar_enabled: false,
                position: Some(5),
                bracket: Some(8),
            })
        );
    }

    #[test]
    fn preferences_patch_only_accepts_known_typed_values() {
        let current = DraftStatsPrefs::default();
        let next = patch_prefs(
            &current,
            &json!({"hero_stats_consent":true,"position":3,"bracket":null}),
        )
        .unwrap();
        assert!(next.hero_stats_consent);
        assert_eq!(next.position, Some(3));
        assert_eq!(next.bracket, None);
        for invalid in [
            json!({"extra": true}),
            json!({"position": 0}),
            json!({"bracket": 9}),
            json!({"draft_bar_enabled": 1}),
            json!({}),
        ] {
            assert!(patch_prefs(&current, &invalid).is_err());
        }
    }
}
