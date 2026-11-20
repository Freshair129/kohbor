//! Bounded, local persistence for the Packs last-match recap.

use super::outcome::{is_valid_record, PlaybackEnd};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

pub const MATCH_RECAP_SCHEMA_VERSION: &str = "1.0.0";
pub const MAX_MATCHES: usize = 5;
pub const MAX_EVENTS_PER_MATCH: usize = 128;
pub const MAX_FILE_BYTES: usize = 1_048_576;

#[derive(Clone)]
pub struct MatchRecapStore {
    inner: Arc<Mutex<StoreState>>,
}

#[derive(Debug)]
struct StoreState {
    path: PathBuf,
    file: MatchRecapFile,
    write_error_logged: bool,
}

#[derive(Debug)]
enum LoadError {
    Corrupt,
    ReadFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchRecapFile {
    schema_version: String,
    matches: Vec<MatchRecapEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchRecapEntry {
    match_id: String,
    events: Vec<Value>,
}

impl Default for MatchRecapFile {
    fn default() -> Self {
        Self {
            schema_version: MATCH_RECAP_SCHEMA_VERSION.to_owned(),
            matches: Vec::new(),
        }
    }
}

impl MatchRecapStore {
    /// Load once at host startup. Invalid/oversized files become empty and emit one warning.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let file = match load_file(&path) {
            Ok(file) => file,
            Err(LoadError::Corrupt) => {
                crate::log_warn!(
                    "audio",
                    "match_recap_corrupt",
                    "stored match recap is corrupt; starting empty",
                    code = "RECAP_CORRUPT"
                );
                MatchRecapFile::default()
            }
            Err(LoadError::ReadFailed) => {
                crate::log_warn!(
                    "audio",
                    "match_recap_read_failed",
                    "could not read stored match recap; starting empty",
                    code = "RECAP_READ_FAILED"
                );
                MatchRecapFile::default()
            }
        };
        Self {
            inner: Arc::new(Mutex::new(StoreState {
                path,
                file,
                write_error_logged: false,
            })),
        }
    }

    pub fn snapshot(&self) -> Value {
        let state = lock(&self.inner);
        json!({
            "schema_version": MATCH_RECAP_SCHEMA_VERSION,
            "matches": state.file.matches.clone(),
        })
    }

    /// Called on the publisher thread, never on the real-time audio owner.
    pub fn record(&self, record: &Value) {
        if !is_recap_event(record) {
            return;
        }
        let mut state = lock(&self.inner);
        let mut next = state.file.clone();
        upsert_event(&mut next, record.clone());
        let _ = self.commit(&mut state, next);
    }

    /// Update the already-recorded admission outcome when its audio playback ends.
    pub fn finish_playback(&self, playback_id: &str, instance_id: &str, end: PlaybackEnd) {
        let mut state = lock(&self.inner);
        let mut next = state.file.clone();
        let mut changed = false;
        for entry in &mut next.matches {
            for event in &mut entry.events {
                if event.get("playback_id").and_then(Value::as_str) == Some(playback_id) {
                    if event.get("instance_id").and_then(Value::as_str) != Some(instance_id) {
                        continue;
                    }
                    if let Some(object) = event.as_object_mut() {
                        object.insert("playback_end".to_owned(), json!(end.as_str()));
                        changed = true;
                    }
                }
            }
        }
        if changed {
            let _ = self.commit(&mut state, next);
        }
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut state = lock(&self.inner);
        self.commit(&mut state, MatchRecapFile::default())
    }

    fn commit(&self, state: &mut StoreState, mut next: MatchRecapFile) -> Result<(), String> {
        trim_document(&mut next);
        let bytes = serde_json::to_vec(&next).map_err(|_| "RECAP_WRITE_FAILED".to_owned())?;
        if bytes.len() > MAX_FILE_BYTES {
            return self.write_failed(state);
        }
        match atomic_write(&state.path, &bytes) {
            Ok(()) => {
                state.file = next;
                Ok(())
            }
            Err(()) => self.write_failed(state),
        }
    }

    fn write_failed(&self, state: &mut StoreState) -> Result<(), String> {
        if !state.write_error_logged {
            state.write_error_logged = true;
            crate::log_warn!(
                "audio",
                "match_recap_write_failed",
                "could not persist local match recap",
                code = "RECAP_WRITE_FAILED"
            );
        }
        Err("RECAP_WRITE_FAILED".to_owned())
    }
}

fn load_file(path: &Path) -> Result<MatchRecapFile, LoadError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(MatchRecapFile::default())
        }
        Err(_) => return Err(LoadError::ReadFailed),
    };
    if metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(LoadError::Corrupt);
    }
    let bytes = fs::read(path).map_err(|_| LoadError::ReadFailed)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(LoadError::Corrupt);
    }
    let file: MatchRecapFile = serde_json::from_slice(&bytes).map_err(|_| LoadError::Corrupt)?;
    validate_file(&file)
        .then_some(file)
        .ok_or(LoadError::Corrupt)
}

fn validate_file(file: &MatchRecapFile) -> bool {
    if file.schema_version != MATCH_RECAP_SCHEMA_VERSION || file.matches.len() > MAX_MATCHES {
        return false;
    }
    let mut seen_matches: Vec<String> = Vec::new();
    for entry in &file.matches {
        let Some(epoch) = local_match_epoch(&entry.match_id) else {
            return false;
        };
        if entry.match_id != format!("local_match_{epoch}")
            || seen_matches.contains(&entry.match_id)
            || entry.events.len() > MAX_EVENTS_PER_MATCH
        {
            return false;
        }
        seen_matches.push(entry.match_id.clone());
        let mut seen_events = Vec::new();
        for event in &entry.events {
            if !is_recap_event(event)
                || event.get("match_id").and_then(Value::as_str) != Some(&entry.match_id)
            {
                return false;
            }
            let key = event
                .get("playback_id")
                .and_then(Value::as_str)
                .or_else(|| event.get("instance_id").and_then(Value::as_str));
            let Some(key) = key else {
                return false;
            };
            if seen_events.contains(&key) {
                return false;
            }
            seen_events.push(key);
        }
    }
    true
}

fn is_recap_event(event: &Value) -> bool {
    is_valid_record(event)
        && event.get("audio_class").and_then(Value::as_str) != Some("voice")
        && event.get("event_type").and_then(Value::as_str) != Some("audition")
}

fn upsert_event(file: &mut MatchRecapFile, event: Value) {
    let Some(match_id) = event.get("match_id").and_then(Value::as_str) else {
        return;
    };
    let entry_index = match file
        .matches
        .iter()
        .position(|entry| entry.match_id == match_id)
    {
        Some(index) => index,
        None => {
            let rank = local_match_epoch(match_id).unwrap_or_default();
            let insert_at = file
                .matches
                .iter()
                .position(|entry| local_match_epoch(&entry.match_id).unwrap_or_default() < rank)
                .unwrap_or(file.matches.len());
            file.matches.insert(
                insert_at,
                MatchRecapEntry {
                    match_id: match_id.to_owned(),
                    events: Vec::new(),
                },
            );
            insert_at
        }
    };
    let Some(entry) = file.matches.get_mut(entry_index) else {
        return;
    };
    let playback_id = event.get("playback_id").and_then(Value::as_str);
    let instance_id = event.get("instance_id").and_then(Value::as_str);
    let existing = entry.events.iter().position(|old| {
        playback_id.is_some_and(|id| old.get("playback_id").and_then(Value::as_str) == Some(id))
            || (playback_id.is_none()
                && instance_id
                    .is_some_and(|id| old.get("instance_id").and_then(Value::as_str) == Some(id)))
    });
    if let Some(index) = existing {
        if let Some(old) = entry.events.get_mut(index) {
            *old = event;
        }
    } else {
        entry.events.push(event);
    }
    if entry.events.len() > MAX_EVENTS_PER_MATCH {
        let remove_count = entry.events.len() - MAX_EVENTS_PER_MATCH;
        entry.events.drain(0..remove_count);
    }
}

fn trim_document(file: &mut MatchRecapFile) {
    file.matches.truncate(MAX_MATCHES);
    loop {
        let Ok(bytes) = serde_json::to_vec(file) else {
            return;
        };
        if bytes.len() <= MAX_FILE_BYTES {
            return;
        }
        if file.matches.is_empty() {
            return;
        }
        if file
            .matches
            .last()
            .is_some_and(|oldest| oldest.events.is_empty())
        {
            file.matches.pop();
        } else {
            if let Some(oldest) = file.matches.last_mut() {
                oldest.events.remove(0);
            }
        }
    }
}

fn local_match_epoch(value: &str) -> Option<u64> {
    let epoch = value.strip_prefix("local_match_")?.parse::<u64>().ok()?;
    (epoch > 0).then_some(epoch)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ()> {
    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().ok_or(())?;
    fs::create_dir_all(parent).map_err(|_| ())?;
    let file_name = path.file_name().ok_or(())?.to_string_lossy();
    let mut temp_path = None;
    let mut temp_file: Option<File> = None;
    for _ in 0..8 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{file_name}.tmp-{}-{sequence}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temp_path = Some(candidate);
                temp_file = Some(file);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(()),
        }
    }
    let Some(temp_path) = temp_path else {
        return Err(());
    };
    let Some(mut file) = temp_file else {
        return Err(());
    };
    let result = (|| {
        file.write_all(bytes).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        drop(file);
        fs::rename(&temp_path, path).map_err(|_| ())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::outcome::{OutcomeDetails, OUTCOME_SCHEMA_VERSION};
    use kohbor_audio::{AudioClass, Decision, Outcome, PlaybackId};
    use serde_json::json;

    fn temp_path(label: &str) -> PathBuf {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "kohbor-p5-recap-{label}-{}-{id}.json",
            std::process::id()
        ))
    }

    fn record(epoch: u64, sequence: u64, outcome: Outcome) -> Value {
        let playback = matches!(outcome, Outcome::Admitted | Outcome::Preempted);
        let decision = Decision {
            instance_id: format!("event_{epoch}_{sequence}"),
            event_type: Some("hero_kill".to_owned()),
            session_epoch: epoch,
            class: AudioClass::Ordinary,
            outcome,
            playback_id: playback.then_some(PlaybackId(sequence)),
            package_revision: Some(1),
            decided_at_ms: sequence,
            detail: None,
        };
        let details = OutcomeDetails {
            tier: Some(3),
            variant_id: Some("variant_1".to_owned()),
            clip_id: Some("clip-1".to_owned()),
            selection: Some("shuffle".to_owned()),
            game_clock_seconds: Some(75),
        };
        let end = (outcome == Outcome::Preempted).then_some(PlaybackEnd::Cut);
        crate::audio::outcome::record(&decision, &details, end).unwrap()
    }

    #[test]
    fn rotates_to_the_last_five_local_matches() {
        let path = temp_path("rotation");
        let store = MatchRecapStore::open(&path);
        for epoch in 1..=6 {
            store.record(&record(epoch, epoch, Outcome::Admitted));
        }
        let snapshot = store.snapshot();
        let matches = snapshot["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 5);
        assert_eq!(matches[0]["match_id"], "local_match_6");
        assert_eq!(matches[4]["match_id"], "local_match_2");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn corrupt_file_recovers_as_empty_and_clear_persists_empty_document() {
        let path = temp_path("corrupt");
        fs::write(&path, b"{broken").unwrap();
        let store = MatchRecapStore::open(&path);
        assert_eq!(store.snapshot()["matches"], json!([]));
        assert!(store.clear().is_ok());
        let reopened = MatchRecapStore::open(&path);
        assert_eq!(
            reopened.snapshot()["schema_version"],
            MATCH_RECAP_SCHEMA_VERSION
        );
        assert_eq!(reopened.snapshot()["matches"], json!([]));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn playback_end_updates_the_existing_event_and_dropped_outcomes_keep_null_end() {
        let path = temp_path("end");
        let store = MatchRecapStore::open(&path);
        let played = record(9, 1, Outcome::Admitted);
        let older_match = record(8, 1, Outcome::Admitted);
        store.record(&played);
        store.record(&older_match);
        store.finish_playback("pb_0000000001", "event_9_1", PlaybackEnd::Finished);
        let dropped = record(9, 2, Outcome::ChannelBusy);
        store.record(&dropped);
        let events = store.snapshot()["matches"][0]["events"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["playback_end"], "finished");
        assert!(events[1]["playback_end"].is_null());
        assert_eq!(events[1]["schema_version"], OUTCOME_SCHEMA_VERSION);
        let snapshot = store.snapshot();
        assert_eq!(
            snapshot["matches"][1]["events"][0]["playback_end"],
            Value::Null
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn saved_event_clock_is_the_known_game_clock_value() {
        let value = record(10, 1, Outcome::Admitted);
        assert_eq!(value["game_clock_seconds"], json!(75));
    }
}
