use crate::{atomic, secrets};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

const PREFS_SCHEMA: u32 = 1;
const MAX_CHAT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LEDGER_ROWS: usize = 200;
const SYSTEM_NOTE: &str = "สรุปแมตช์.md";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnalyticsPreferences {
    pub(crate) ai_on: bool,
    pub(crate) consent: bool,
    pub(crate) model: String,
    pub(crate) keep_newest: usize,
    pub(crate) max_bytes: u64,
    pub(crate) auto_analyse: bool,
}

impl Default for AnalyticsPreferences {
    fn default() -> Self {
        Self { ai_on: false, consent: false, model: "claude-opus-5-5".into(), keep_newest: 20, max_bytes: 2 * 1024 * 1024 * 1024, auto_analyse: true }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreferencesFile { schema_version: u32, pub(crate) settings: AnalyticsPreferences }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct LedgerRow {
    pub(crate) time_unix_ms: u64,
    pub(crate) model: String,
    pub(crate) bytes_sent: u64,
    pub(crate) bytes_received: u64,
    pub(crate) input_tokens: Option<u64>,
    pub(crate) cache_read_tokens: Option<u64>,
    pub(crate) cache_creation_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) status: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NoteRow { pub(crate) id: String, pub(crate) title: String, pub(crate) ai: bool }

#[derive(Clone)]
pub(crate) struct AnalyticsStore {
    root: PathBuf,
    install_key: Option<[u8; 32]>,
    write_gate: Arc<Mutex<()>>,
}

impl AnalyticsStore {
    pub(crate) fn open(data_dir: &Path, install_key: Option<[u8; 32]>) -> Self {
        Self { root: data_dir.join("analytics"), install_key, write_gate: Arc::new(Mutex::new(())) }
    }

    pub(crate) fn root(&self) -> &Path { &self.root }

    fn ensure_root(&self) -> Result<(), &'static str> { fs::create_dir_all(&self.root).map_err(|_| "storage_unavailable") }

    pub(crate) fn key_status(&self) -> Result<Value, &'static str> {
        self.ensure_root()?;
        let key = secrets::load_or_create(&self.root.join("anthropic-key.dpapi"), || None);
        let last4 = key.as_ref().and_then(|bytes| bytes.get(bytes.len().saturating_sub(4)..)).and_then(|bytes| std::str::from_utf8(bytes).ok()).map(str::to_owned);
        Ok(json!({"set":key.is_some(),"last4":last4}))
    }

    pub(crate) fn read_key(&self) -> Result<Option<Vec<u8>>, &'static str> {
        self.ensure_root()?;
        Ok(secrets::load_or_create(&self.root.join("anthropic-key.dpapi"), || None))
    }

    pub(crate) fn set_key(&self, key: &str) -> Result<(), &'static str> {
        if !(16..=512).contains(&key.len()) || !key.is_ascii() || key.bytes().any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace()) { return Err("key_invalid"); }
        self.ensure_root()?;
        secrets::store(&self.root.join("anthropic-key.dpapi"), key.as_bytes()).ok_or("key_storage_unavailable")
    }

    pub(crate) fn delete_key(&self) -> Result<(), &'static str> {
        self.ensure_root()?;
        match fs::remove_file(self.root.join("anthropic-key.dpapi")) {
            Ok(()) => Ok(()), Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()), Err(_) => Err("key_storage_unavailable"),
        }
    }

    pub(crate) fn preferences(&self) -> Result<AnalyticsPreferences, &'static str> {
        self.ensure_root()?;
        let path = self.root.join("settings.json");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let defaults = AnalyticsPreferences::default();
                self.write_preferences(&defaults)?;
                return Ok(defaults);
            }
            Err(_) => return Err("storage_unavailable"),
        };
        let file: PreferencesFile = serde_json::from_slice(&bytes).map_err(|_| "settings_invalid")?;
        if file.schema_version != PREFS_SCHEMA { return Err("settings_version_unsupported"); }
        validate_preferences(&file.settings)?;
        Ok(file.settings)
    }

    pub(crate) fn write_preferences(&self, settings: &AnalyticsPreferences) -> Result<(), &'static str> {
        validate_preferences(settings)?; self.ensure_root()?;
        let file = PreferencesFile { schema_version: PREFS_SCHEMA, settings: settings.clone() };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|_| "settings_write_failed")?;
        atomic::write(&self.root.join("settings.json"), &bytes).map_err(|_| "settings_write_failed")
    }

    pub(crate) fn ledger(&self) -> Result<Vec<LedgerRow>, &'static str> {
        self.ensure_root()?;
        match fs::read(self.root.join("ledger.json")) {
            Ok(bytes) if bytes.len() <= 128 * 1024 => serde_json::from_slice(&bytes).map_err(|_| "ledger_invalid"),
            Ok(_) => Err("ledger_invalid"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(_) => Err("storage_unavailable"),
        }
    }

    pub(crate) fn add_ledger(&self, row: LedgerRow) -> Result<(), &'static str> {
        let mut rows = self.ledger()?; rows.insert(0, row); rows.truncate(MAX_LEDGER_ROWS);
        let bytes = serde_json::to_vec_pretty(&rows).map_err(|_| "ledger_write_failed")?;
        atomic::write(&self.root.join("ledger.json"), &bytes).map_err(|_| "ledger_write_failed")
    }

    pub(crate) fn match_key(&self, match_id: &str) -> Result<String, &'static str> {
        let Some(key) = self.install_key.as_ref() else { return Err("install_key_unavailable"); };
        let id = match_id.parse::<u64>().ok().filter(|id| *id > 0).ok_or("invalid_match_id")?.to_string();
        Ok(hmac_match_key(key, id.as_bytes()))
    }

    fn match_dir(&self, match_id: &str) -> Result<PathBuf, &'static str> { Ok(self.root.join(self.match_key(match_id)?)) }

    pub(crate) fn set_own_hero(&self, match_id: &str, hero: &str) -> Result<(), &'static str> {
        if !valid_hero(hero) { return Err("invalid_hero"); }
        let directory = self.match_dir(match_id)?; fs::create_dir_all(&directory).map_err(|_| "storage_unavailable")?;
        let path = directory.join("state.json");
        let mut state: Value = match fs::read(&path) { Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "state_invalid")?, Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}), Err(_) => return Err("storage_unavailable") };
        state["own_hero"] = json!(hero);
        let bytes = serde_json::to_vec_pretty(&state).map_err(|_| "storage_unavailable")?;
        atomic::write(&path, &bytes).map_err(|_| "storage_unavailable")
    }

    pub(crate) fn own_hero(&self, match_id: &str) -> Option<String> {
        let path = self.match_dir(match_id).ok()?.join("state.json");
        let bytes = fs::read(path).ok()?; let state: Value = serde_json::from_slice(&bytes).ok()?;
        let hero = state.get("own_hero")?.as_str()?; valid_hero(hero).then(|| hero.to_owned())
    }

    pub(crate) fn append_chat(&self, match_id: &str, message: Value) -> Result<Vec<Value>, &'static str> {
        let _guard = lock(&self.write_gate);
        let directory = self.match_dir(match_id)?; fs::create_dir_all(&directory).map_err(|_| "storage_unavailable")?;
        let path = directory.join("chat.json");
        let mut messages: Vec<Value> = match fs::metadata(&path) {
            Ok(metadata) if metadata.len() > MAX_CHAT_BYTES => return Err("chat_too_large"),
            Ok(_) => serde_json::from_slice(&fs::read(&path).map_err(|_| "storage_unavailable")?).map_err(|_| "chat_invalid")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(), Err(_) => return Err("storage_unavailable"),
        };
        messages.push(message);
        let bytes = serde_json::to_vec(&messages).map_err(|_| "storage_unavailable")?;
        if bytes.len() as u64 > MAX_CHAT_BYTES { return Err("chat_too_large"); }
        atomic::write(&path, &bytes).map_err(|_| "storage_unavailable")?;
        Ok(messages)
    }

    pub(crate) fn chat(&self, match_id: &str) -> Result<Vec<Value>, &'static str> {
        let path = self.match_dir(match_id)?.join("chat.json");
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() > MAX_CHAT_BYTES => Err("chat_too_large"),
            Ok(_) => serde_json::from_slice(&fs::read(path).map_err(|_| "storage_unavailable")?).map_err(|_| "chat_invalid"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()), Err(_) => Err("storage_unavailable"),
        }
    }

    pub(crate) fn list_notes(&self, match_id: &str, system_markdown: &str) -> Result<Vec<NoteRow>, &'static str> {
        let _guard = lock(&self.write_gate);
        let directory = self.match_dir(match_id)?.join("notes"); fs::create_dir_all(&directory).map_err(|_| "storage_unavailable")?;
        atomic::write(&directory.join(SYSTEM_NOTE), system_markdown.as_bytes()).map_err(|_| "note_write_failed")?;
        let mut rows = vec![NoteRow { id: SYSTEM_NOTE.into(), title: SYSTEM_NOTE.into(), ai: false }];
        for item in fs::read_dir(&directory).map_err(|_| "storage_unavailable")? {
            let item = item.map_err(|_| "storage_unavailable")?; let path = item.path();
            if !path.is_file() || path.file_name().and_then(|name| name.to_str()) == Some(SYSTEM_NOTE) || path.extension().and_then(|ext| ext.to_str()) != Some("md") { continue; }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue; };
            rows.push(NoteRow { id: name.to_owned(), title: name.to_owned(), ai: true });
        }
        rows[1..].sort_by(|left, right| left.title.cmp(&right.title)); Ok(rows)
    }

    pub(crate) fn read_note(&self, match_id: &str, note_id: &str) -> Result<String, &'static str> {
        if note_id != SYSTEM_NOTE && !valid_note_id(note_id) { return Err("note_not_found"); }
        let path = self.match_dir(match_id)?.join("notes").join(note_id);
        let metadata = fs::metadata(&path).map_err(|_| "note_not_found")?;
        if !metadata.is_file() || metadata.len() > 64 * 1024 { return Err("note_invalid"); }
        String::from_utf8(fs::read(path).map_err(|_| "note_not_found")?).map_err(|_| "note_invalid")
    }

    pub(crate) fn write_note(&self, match_id: &str, title: &str, markdown: &str) -> Result<String, &'static str> {
        let _guard = lock(&self.write_gate);
        if markdown.len() > 64 * 1024 { return Err("note_too_large"); }
        let base = safe_note_stem(title)?; let directory = self.match_dir(match_id)?.join("notes");
        fs::create_dir_all(&directory).map_err(|_| "note_write_failed")?;
        for suffix in 1_u32..=10_000 {
            let file = if suffix == 1 { format!("{base}.md") } else { format!("{base} ({suffix}).md") };
            if file == SYSTEM_NOTE { continue; }
            let path = directory.join(&file);
            if path.exists() { continue; }
            atomic::write(&path, markdown.as_bytes()).map_err(|_| "note_write_failed")?;
            return Ok(file);
        }
        Err("note_name_exhausted")
    }

    pub(crate) fn delete_note(&self, match_id: &str, note_id: &str) -> Result<(), &'static str> {
        let _guard = lock(&self.write_gate);
        if note_id == SYSTEM_NOTE || !valid_note_id(note_id) { return Err("system_note_read_only"); }
        let path = self.match_dir(match_id)?.join("notes").join(note_id);
        fs::remove_file(path).map_err(|_| "note_not_found")
    }

    pub(crate) fn open_note_folder(&self, match_id: &str) -> Result<PathBuf, &'static str> {
        let directory = self.match_dir(match_id)?.join("notes"); fs::create_dir_all(&directory).map_err(|_| "storage_unavailable")?; Ok(directory)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> { mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner) }

fn validate_preferences(settings: &AnalyticsPreferences) -> Result<(), &'static str> {
    if !matches!(settings.model.as_str(), "claude-opus-5-5" | "claude-sonnet-5-5" | "claude-haiku-5-5") { return Err("model_not_allowed"); }
    if !(5..=100).contains(&settings.keep_newest) || settings.max_bytes == 0 { return Err("invalid_library_limits"); }
    Ok(())
}

fn valid_hero(hero: &str) -> bool {
    hero.len() <= 64 && hero.starts_with("npc_dota_hero_") && hero.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn safe_note_stem(title: &str) -> Result<String, &'static str> {
    let trimmed = title.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 120 || trimmed.contains("..") || trimmed.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0')) { return Err("note_name_invalid"); }
    let stem = trimmed.chars().map(|character| if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ' | '(' | ')') { character } else { '-' })
        .collect::<String>().trim().trim_matches('.').to_owned();
    if stem.is_empty() || stem.chars().count() > 100 { return Err("note_name_invalid"); }
    let windows_reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];
    if windows_reserved.iter().any(|reserved| stem.eq_ignore_ascii_case(reserved)) { return Err("note_name_invalid"); }
    Ok(stem)
}

fn valid_note_id(id: &str) -> bool {
    if id.len() < 4 || id.len() > 128 || id == SYSTEM_NOTE || !id.ends_with(".md") || id.contains("..") { return false; }
    id.chars().all(|character| character.is_alphanumeric() || matches!(character, '-' | '_' | ' ' | '(' | ')' | '.'))
}

fn hmac_match_key(key: &[u8; 32], id: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut inner_pad = [0x36_u8; 64]; let mut outer_pad = [0x5c_u8; 64];
    for (index, byte) in key.iter().enumerate() { inner_pad[index] ^= *byte; outer_pad[index] ^= *byte; }
    let mut inner = Sha256::new(); inner.update(inner_pad); inner.update(b"match\0"); inner.update(id); let digest = inner.finalize();
    let mut outer = Sha256::new(); outer.update(outer_pad); outer.update(digest);
    outer.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn root() -> PathBuf { let path = std::env::temp_dir().join(format!("analytics-store-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))); fs::create_dir_all(&path).unwrap(); path }
    fn store(dir: &Path) -> AnalyticsStore { AnalyticsStore::open(dir, Some([7; 32])) }

    #[test]
    fn key_status_never_returns_key_and_settings_default_off() {
        let dir = root(); let store = store(&dir);
        assert_eq!(store.preferences().unwrap(), AnalyticsPreferences::default());
        assert_eq!(store.key_status().unwrap(), json!({"set":false,"last4":null}));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn note_name_safety_clash_system_read_only_and_limits() {
        for invalid in ["..", "a/b", "a\\b", "C:secret", "CON", "NUL", "", "a".repeat(121).as_str()] { assert!(safe_note_stem(invalid).is_err(), "{invalid}"); }
        let dir = root(); let store = store(&dir);
        let one = store.write_note("123", "Plan", "# one").unwrap(); let two = store.write_note("123", "Plan", "# two").unwrap();
        assert_eq!(one, "Plan.md"); assert_eq!(two, "Plan (2).md");
        assert_ne!(store.write_note("123", "สรุปแมตช์", "AI").unwrap(), SYSTEM_NOTE);
        assert_eq!(store.delete_note("123", SYSTEM_NOTE), Err("system_note_read_only"));
        assert_eq!(store.write_note("123", "large", &"x".repeat(65 * 1024)), Err("note_too_large"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn install_key_paths_and_own_hero_are_hashed_and_survive_replay_removal() {
        let dir = root(); let store = store(&dir);
        store.set_own_hero("12345", "npc_dota_hero_axe").unwrap();
        assert_eq!(store.own_hero("12345").as_deref(), Some("npc_dota_hero_axe"));
        let hash = store.match_key("12345").unwrap(); assert_eq!(hash.len(),64); assert!(!hash.contains("12345"));
        let notes = store.list_notes("12345", "# system").unwrap(); assert_eq!(notes.len(),1); assert!(!notes[0].ai);
        assert_eq!(store.read_note("12345", SYSTEM_NOTE).unwrap(), "# system");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ledger_contains_metadata_only() {
        let dir = root(); let store = store(&dir);
        store.add_ledger(LedgerRow { time_unix_ms:1, model:"claude-opus-5-5".into(), bytes_sent:20, bytes_received:30, input_tokens:Some(4), cache_read_tokens:Some(2), cache_creation_tokens:Some(1), output_tokens:Some(3), status:"success".into() }).unwrap();
        let bytes = fs::read(store.root().join("ledger.json")).unwrap(); let raw = String::from_utf8(bytes).unwrap();
        for secret in ["sk-ant", "question", "markdown", "prompt", "match_id"] { assert!(!raw.contains(secret)); }
        assert_eq!(store.ledger().unwrap().len(), 1); let _ = fs::remove_dir_all(dir);
    }
}
