//! Opt-in own-match history from OpenDota (DEC-OVERVIEW revision 4).
//!
//! The renderer can enable or revoke consent, but cannot supply an account ID, URL, host, path,
//! or request fields. The host persists only the 32-bit account ID while consent is enabled. Match
//! IDs are reduced in memory to the same per-install HMAC used by GSI before cache serialization.

#[cfg(windows)]
use crate::hero_stats::transport::WinHttpTransport;
use crate::hero_stats::transport::{Cancellation, Endpoint, HttpResponse, TransportError};
use crate::match_history::{MAX_ENTRIES, MatchEnd, MatchEntry, MatchResult, MatchType};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const SETTINGS_FILE: &str = "opendota-settings.json";
pub(crate) const CACHE_FILE: &str = "opendota-history-cache.json";
pub(crate) const LEDGER_FILE: &str = "opendota-request-ledger.json";
pub(crate) const HISTORY_LIMIT: usize = 150;
pub(crate) const REQUEST_BUDGET: usize = 4;
pub(crate) const BUDGET_WINDOW_MS: u64 = 24 * 60 * 60 * 1000;
pub(crate) const BODY_LIMIT_BYTES: usize = 512 * 1024;
const SCHEMA_VERSION: u8 = 1;
const CACHE_FILE_LIMIT_BYTES: usize = 1_048_576;
// Offline assumption: OpenDota's `project` query accepts this comma-delimited field list.
const PROJECT_FIELDS: &str = "match_id,hero_id,player_slot,radiant_win,kills,deaths,assists,duration,start_time,lobby_type,game_mode";

const HISTORY_ENDPOINT: Endpoint = Endpoint {
    host: "api.opendota.com",
    body_limit: BODY_LIMIT_BYTES,
    log_module: "opendota",
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum State {
    Disabled,
    Idle,
    WaitingForAccount,
    Loading,
    Merged,
    Empty,
    PrivateProfile,
    NotFound,
    RateLimited,
    BudgetExhausted,
    Offline,
    Unavailable,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    schema_version: u8,
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_id: Option<u32>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            enabled: false,
            account_id: None,
        }
    }
}

impl Preferences {
    fn valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
            && self.account_id.map_or(true, |id| self.enabled && id > 0)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CachedHistory {
    schema_version: u8,
    fetched_at: u64,
    entries: Vec<MatchEntry>,
}

impl CachedHistory {
    fn valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
            && self.entries.len() <= MAX_ENTRIES
            && self.entries.iter().all(|entry| {
                entry.source == "opendota"
                    && entry.match_key.len() == 64
                    && entry
                        .match_key
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    && entry.started_at.is_some()
                    && entry.hero_id.is_some_and(|id| id > 0)
                    && entry.hero.as_deref().is_some_and(|hero| {
                        entry
                            .hero_id
                            .is_some_and(|id| hero == format!("opendota_hero_id_{id}"))
                    })
            })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredLedger {
    schema_version: u8,
    requests_ms: Vec<u64>,
}

impl StoredLedger {
    fn load(path: &Path, now_ms: u64) -> Self {
        let parsed = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok());
        let mut ledger = match parsed {
            Some(value)
                if value.schema_version == SCHEMA_VERSION && value.requests_ms.len() <= 64 =>
            {
                value
            }
            Some(_) => Self {
                schema_version: SCHEMA_VERSION,
                requests_ms: vec![now_ms; REQUEST_BUDGET],
            },
            None if !path.exists() => Self {
                schema_version: SCHEMA_VERSION,
                requests_ms: Vec::new(),
            },
            None => Self {
                schema_version: SCHEMA_VERSION,
                requests_ms: vec![now_ms; REQUEST_BUDGET],
            },
        };
        ledger.prune(now_ms);
        ledger
    }

    fn prune(&mut self, now_ms: u64) {
        self.requests_ms
            .retain(|at| *at > now_ms || now_ms - *at < BUDGET_WINDOW_MS);
    }

    fn can_request(&self, now_ms: u64) -> bool {
        self.requests_ms
            .iter()
            .filter(|at| **at > now_ms || now_ms - **at < BUDGET_WINDOW_MS)
            .count()
            < REQUEST_BUDGET
    }
}

pub(crate) trait HistoryTransport: Send + Sync {
    fn get(&self, path: &str, cancellation: &Cancellation) -> Result<HttpResponse, TransportError>;
    fn cancel_current(&self) {}
}

#[cfg(windows)]
struct NativeTransport(WinHttpTransport);

#[cfg(windows)]
impl HistoryTransport for NativeTransport {
    fn get(&self, path: &str, cancellation: &Cancellation) -> Result<HttpResponse, TransportError> {
        self.0.get_from(HISTORY_ENDPOINT, path, cancellation)
    }

    fn cancel_current(&self) {
        crate::hero_stats::transport::Transport::cancel_current(&self.0);
    }
}

#[cfg(not(windows))]
struct NativeTransport;

#[cfg(not(windows))]
impl HistoryTransport for NativeTransport {
    fn get(
        &self,
        _path: &str,
        _cancellation: &Cancellation,
    ) -> Result<HttpResponse, TransportError> {
        Err(TransportError::NotSent)
    }
}

struct Inner {
    prefs: Preferences,
    entries: Vec<MatchEntry>,
    fetched_at: Option<u64>,
    last_attempt_at: Option<u64>,
    state: State,
    ledger: StoredLedger,
    busy: bool,
    cancellation: Option<Cancellation>,
    startup_attempted: bool,
    generation: u64,
}

pub(crate) struct OpenDotaService {
    dir: PathBuf,
    install_key: Option<[u8; 32]>,
    transport: Arc<dyn HistoryTransport>,
    inner: Mutex<Inner>,
}

impl OpenDotaService {
    pub(crate) fn open(dir: &Path, install_key: Option<[u8; 32]>) -> Self {
        #[cfg(windows)]
        let transport: Arc<dyn HistoryTransport> =
            Arc::new(NativeTransport(WinHttpTransport::default()));
        #[cfg(not(windows))]
        let transport: Arc<dyn HistoryTransport> = Arc::new(NativeTransport);
        Self::open_with_transport(dir, install_key, transport)
    }

    fn open_with_transport(
        dir: &Path,
        install_key: Option<[u8; 32]>,
        transport: Arc<dyn HistoryTransport>,
    ) -> Self {
        let prefs_path = dir.join(SETTINGS_FILE);
        let prefs_bytes = fs::read(&prefs_path).ok();
        let parsed_prefs = prefs_bytes
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<Preferences>(bytes).ok());
        let prefs = parsed_prefs.filter(Preferences::valid).unwrap_or_else(|| {
            if prefs_bytes.is_some() {
                let _ = fs::remove_file(&prefs_path);
            }
            Preferences::default()
        });
        let cache_path = dir.join(CACHE_FILE);
        let cache = fs::read(&cache_path)
            .ok()
            .filter(|bytes| bytes.len() <= CACHE_FILE_LIMIT_BYTES)
            .and_then(|bytes| serde_json::from_slice::<CachedHistory>(&bytes).ok())
            .filter(CachedHistory::valid);
        let enabled = prefs.enabled;
        let account_available = prefs.account_id.is_some();
        let (entries, fetched_at) = if enabled && account_available {
            cache
                .map(|cache| (cache.entries, Some(cache.fetched_at)))
                .unwrap_or_default()
        } else {
            let _ = fs::remove_file(&cache_path);
            (Vec::new(), None)
        };
        let state = if !enabled {
            State::Disabled
        } else if !account_available {
            State::WaitingForAccount
        } else if fetched_at.is_some() {
            if entries.is_empty() {
                State::Empty
            } else {
                State::Merged
            }
        } else {
            State::Idle
        };
        let now_ms = unix_millis();
        let ledger_path = dir.join(LEDGER_FILE);
        let ledger = StoredLedger::load(&ledger_path, now_ms);
        let _ = save_json(&ledger_path, &ledger);
        Self {
            dir: dir.to_path_buf(),
            install_key,
            transport,
            inner: Mutex::new(Inner {
                prefs,
                entries,
                fetched_at,
                last_attempt_at: None,
                state,
                ledger,
                busy: false,
                cancellation: None,
                startup_attempted: false,
                generation: 0,
            }),
        }
    }

    pub(crate) fn prefs(&self) -> Value {
        let inner = lock(&self.inner);
        json!({"enabled": inner.prefs.enabled})
    }

    pub(crate) fn set_enabled(
        &self,
        enabled: bool,
        account_id: Option<u32>,
    ) -> Result<Value, String> {
        if enabled {
            let account_id = account_id.filter(|id| *id > 0);
            let mut inner = lock(&self.inner);
            self.enable_with_account_locked(&mut inner, account_id)?;
            drop(inner);
            return Ok(json!({"enabled": true}));
        }

        let cancellation = {
            let mut inner = lock(&self.inner);
            inner.prefs = Preferences::default();
            inner.entries.clear();
            inner.fetched_at = None;
            inner.state = State::Disabled;
            inner.startup_attempted = true;
            inner.generation = inner.generation.wrapping_add(1);
            let cancellation = inner.cancellation.take();
            if let Some(current) = &cancellation {
                current.cancel();
            }
            cancellation
        };
        let _ = cancellation;
        self.transport.cancel_current();
        let settings_path = self.dir.join(SETTINGS_FILE);
        let prefs_result = save_json(&settings_path, &Preferences::default());
        let prefs_result = if prefs_result.is_err() {
            delete_file(&settings_path)
        } else {
            Ok(())
        };
        let cache_result = delete_file(&self.dir.join(CACHE_FILE));
        prefs_result.map_err(|_| "OPENDOTA_SETTINGS_WRITE_FAILED")?;
        cache_result.map_err(|_| "OPENDOTA_CACHE_DELETE_FAILED")?;
        Ok(json!({"enabled": false}))
    }

    /// Attach a fresh verified account only while consent is still enabled.
    pub(crate) fn update_account_if_enabled(&self, account_id: u32) -> Result<(), String> {
        if account_id == 0 {
            return Err("OWN_ACCOUNT_UNAVAILABLE".to_owned());
        }
        let mut inner = lock(&self.inner);
        if !inner.prefs.enabled {
            return Ok(());
        }
        self.enable_with_account_locked(&mut inner, Some(account_id))
    }

    fn enable_with_account_locked(
        &self,
        inner: &mut Inner,
        account_id: Option<u32>,
    ) -> Result<(), String> {
        if inner.prefs.enabled && inner.prefs.account_id == account_id {
            return Ok(());
        }
        delete_file(&self.dir.join(CACHE_FILE)).map_err(|_| "OPENDOTA_CACHE_DELETE_FAILED")?;
        if inner.prefs.account_id != account_id {
            if let Some(cancellation) = inner.cancellation.take() {
                cancellation.cancel();
            }
            inner.generation = inner.generation.wrapping_add(1);
            inner.entries.clear();
            inner.fetched_at = None;
            self.transport.cancel_current();
        }
        let prefs = Preferences {
            schema_version: SCHEMA_VERSION,
            enabled: true,
            account_id,
        };
        save_json(&self.dir.join(SETTINGS_FILE), &prefs)
            .map_err(|_| "OPENDOTA_SETTINGS_WRITE_FAILED")?;
        inner.prefs = prefs;
        inner.entries.clear();
        inner.fetched_at = None;
        inner.state = if account_id.is_some() {
            State::Idle
        } else {
            State::WaitingForAccount
        };
        inner.startup_attempted = false;
        Ok(())
    }

    pub(crate) fn entries(&self) -> Vec<MatchEntry> {
        let inner = lock(&self.inner);
        if inner.prefs.enabled {
            inner.entries.clone()
        } else {
            Vec::new()
        }
    }

    pub(crate) fn view(&self) -> Value {
        let inner = lock(&self.inner);
        self.view_locked(&inner)
    }

    pub(crate) fn refresh(&self, retry: bool) -> Value {
        let now_ms = unix_millis();
        let (account_id, cancellation, reserved_at, generation) = {
            let mut inner = lock(&self.inner);
            if !inner.prefs.enabled {
                inner.state = State::Disabled;
                return self.view_locked(&inner);
            }
            if !retry {
                if inner.startup_attempted {
                    return self.view_locked(&inner);
                }
            }
            if inner.busy {
                return self.view_locked(&inner);
            }
            let Some(account_id) = inner.prefs.account_id else {
                inner.state = State::WaitingForAccount;
                return self.view_locked(&inner);
            };
            if !retry {
                inner.startup_attempted = true;
            }
            if self.install_key.is_none() {
                inner.state = State::Unavailable;
                return self.view_locked(&inner);
            }
            inner.ledger.prune(now_ms);
            if !inner.ledger.can_request(now_ms) {
                inner.state = State::BudgetExhausted;
                return self.view_locked(&inner);
            }
            let reserved_at = now_ms;
            inner.ledger.requests_ms.push(reserved_at);
            if save_json(&self.dir.join(LEDGER_FILE), &inner.ledger).is_err() {
                inner.ledger.requests_ms.retain(|at| *at != reserved_at);
                inner.state = State::Unavailable;
                return self.view_locked(&inner);
            }
            let cancellation = Cancellation::default();
            inner.last_attempt_at = Some(now_ms / 1000);
            inner.busy = true;
            inner.cancellation = Some(cancellation.clone());
            inner.state = State::Loading;
            (account_id, cancellation, reserved_at, inner.generation)
        };

        let result = self.transport.get(&request_path(account_id), &cancellation);
        let mut inner = lock(&self.inner);
        inner.busy = false;
        inner.cancellation = None;
        if inner.generation != generation || !inner.prefs.enabled {
            if !inner.prefs.enabled {
                inner.state = State::Disabled;
            }
            return self.view_locked(&inner);
        }
        match result {
            Err(error) => {
                if !error.reached_network() {
                    inner.ledger.requests_ms.retain(|at| *at != reserved_at);
                    let _ = save_json(&self.dir.join(LEDGER_FILE), &inner.ledger);
                    inner.state = State::Unavailable;
                } else {
                    inner.state = State::Offline;
                }
            }
            Ok(response) => match classify_response(response, self.install_key.as_ref()) {
                Classified::Rows(entries) => {
                    let now = unix_seconds();
                    let cache = CachedHistory {
                        schema_version: SCHEMA_VERSION,
                        fetched_at: now,
                        entries: entries.clone(),
                    };
                    if save_json(&self.dir.join(CACHE_FILE), &cache).is_ok() {
                        inner.entries = entries;
                        inner.fetched_at = Some(now);
                        inner.state = if inner.entries.is_empty() {
                            State::Empty
                        } else {
                            State::Merged
                        };
                    } else {
                        inner.state = State::Unavailable;
                    }
                }
                Classified::State(state) => inner.state = state,
            },
        }
        self.view_locked(&inner)
    }

    pub(crate) fn clear_cache(&self) -> Result<(), String> {
        {
            let mut inner = lock(&self.inner);
            inner.generation = inner.generation.wrapping_add(1);
            if let Some(cancellation) = inner.cancellation.take() {
                cancellation.cancel();
            }
            inner.entries.clear();
            inner.fetched_at = None;
            if inner.prefs.enabled {
                inner.state = if inner.prefs.account_id.is_some() {
                    State::Idle
                } else {
                    State::WaitingForAccount
                };
            }
        }
        self.transport.cancel_current();
        delete_file(&self.dir.join(CACHE_FILE)).map_err(|_| "OPENDOTA_CACHE_DELETE_FAILED")?;
        Ok(())
    }

    pub(crate) fn stop_for_exit(&self) {
        {
            let mut inner = lock(&self.inner);
            inner.generation = inner.generation.wrapping_add(1);
            if let Some(cancellation) = inner.cancellation.take() {
                cancellation.cancel();
            }
        }
        self.transport.cancel_current();
    }

    fn view_locked(&self, inner: &Inner) -> Value {
        let cached_entries = if inner.prefs.enabled {
            inner.entries.len()
        } else {
            0
        };
        let retry_available = inner.prefs.enabled
            && inner.prefs.account_id.is_some()
            && !inner.busy
            && inner.ledger.can_request(unix_millis());
        json!({
            "state": inner.state,
            "enabled": inner.prefs.enabled,
            "cached_entries": cached_entries,
            "fetched_at": if inner.prefs.enabled { inner.fetched_at } else { None },
            "last_attempt_at": inner.last_attempt_at,
            "retry_available": retry_available,
        })
    }
}

#[derive(Debug)]
enum Classified {
    Rows(Vec<MatchEntry>),
    State(State),
}

fn classify_response(response: HttpResponse, install_key: Option<&[u8; 32]>) -> Classified {
    if response.body.len() > BODY_LIMIT_BYTES {
        return Classified::State(State::Unavailable);
    }
    // Offline assumption: HTTP 403 or a 200 error mentioning a private profile means private;
    // HTTP 404 means no OpenDota profile. These status/body conventions were not live-verified.
    match response.status {
        403 => Classified::State(State::PrivateProfile),
        404 => Classified::State(State::NotFound),
        429 => Classified::State(State::RateLimited),
        200 if is_private_error(&response.body) => Classified::State(State::PrivateProfile),
        200 => match parse_matches(&response.body, install_key) {
            Ok(entries) => Classified::Rows(entries),
            Err(()) => Classified::State(State::Unavailable),
        },
        _ => Classified::State(State::Offline),
    }
}

fn is_private_error(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .as_object()
                .and_then(|object| object.get("error").or_else(|| object.get("message")))
                .and_then(Value::as_str)
                .map(str::to_ascii_lowercase)
        })
        .is_some_and(|message| message.contains("private") && message.contains("profile"))
}

fn parse_matches(bytes: &[u8], install_key: Option<&[u8; 32]>) -> Result<Vec<MatchEntry>, ()> {
    let rows: Vec<Value> = serde_json::from_slice(bytes).map_err(|_| ())?;
    rows.into_iter()
        .map(|row| parse_match(row, install_key.ok_or(())?))
        .collect()
}

fn parse_match(row: Value, install_key: &[u8; 32]) -> Result<MatchEntry, ()> {
    // Offline field assumptions: player_slot < 128 is Radiant, radiant_win is the match result,
    // and duration/start_time are seconds. hero_id remains numeric because no name map is pinned.
    let object = row.as_object().ok_or(())?;
    let match_id = object.get("match_id").and_then(Value::as_u64).ok_or(())?;
    let hero_id = object
        .get("hero_id")
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())
        .filter(|id| *id > 0)
        .ok_or(())?;
    let player_slot = object
        .get("player_slot")
        .and_then(Value::as_u64)
        .and_then(|slot| u8::try_from(slot).ok())
        .ok_or(())?;
    let radiant_win = object
        .get("radiant_win")
        .and_then(Value::as_bool)
        .ok_or(())?;
    let duration = object
        .get("duration")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(())?;
    let started_at = object.get("start_time").and_then(Value::as_u64).ok_or(())?;
    let lobby_type = object
        .get("lobby_type")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(())?;
    let game_mode = object
        .get("game_mode")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(())?;
    let radiant_player = player_slot < 128;
    let own_won = radiant_player == radiant_win;
    let end = started_at.saturating_add(u64::from(duration));
    let match_key = install_match_key_from_id(match_id, install_key);
    Ok(MatchEntry {
        match_key,
        hero: Some(format!("opendota_hero_id_{hero_id}")),
        result: Some(if own_won {
            MatchResult::Win
        } else {
            MatchResult::Loss
        }),
        end: MatchEnd::Completed,
        kills: optional_u32(object.get("kills"))?,
        deaths: optional_u32(object.get("deaths"))?,
        assists: optional_u32(object.get("assists"))?,
        duration_s: Some(duration),
        ended_at: end,
        started_at: Some(started_at),
        hero_id: Some(hero_id),
        match_type: classify_type(lobby_type, game_mode),
        source: "opendota".to_owned(),
    })
}

fn optional_u32(value: Option<&Value>) -> Result<Option<u32>, ()> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or(()),
    }
}

/// Offline assumptions: 0=normal/unranked, 5/6/7=ranked, 4/10=bot, 15=custom; the remaining
/// listed lobby values are treated as lobby or tournament-only. Unknown enum values remain unknown.
fn classify_type(lobby_type: u32, game_mode: u32) -> MatchType {
    if matches!(lobby_type, 4 | 10) {
        return MatchType::Bot;
    }
    if game_mode == 15 {
        return MatchType::Custom;
    }
    match lobby_type {
        0 => MatchType::Unranked,
        5 | 6 | 7 => MatchType::Ranked,
        1 | 2 | 3 | 8 | 11 | 12 | 13 => MatchType::Lobby,
        _ => MatchType::Unknown,
    }
}

fn request_path(account_id: u32) -> String {
    let project = PROJECT_FIELDS.replace(',', "%2C");
    format!("/api/players/{account_id}/matches?limit={HISTORY_LIMIT}&project={project}")
}

/// Same HMAC-SHA-256 as `kohbor_gsi::session::keyed(key, "match", value)`. `None` is used only
/// by the standalone parser tests, whose fixture helper supplies an explicit key via the wrapper.
fn install_match_key_from_id(match_id: u64, key: &[u8; 32]) -> String {
    let mut ipad = [0x36_u8; 64];
    let mut opad = [0x5c_u8; 64];
    for (index, byte) in key.iter().enumerate() {
        ipad[index] ^= *byte;
        opad[index] ^= *byte;
    }
    let id = match_id.to_string();
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(b"match\0");
    inner.update(id.as_bytes());
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn save_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).expect("serializable OpenDota file");
    if path.file_name().and_then(|name| name.to_str()) == Some(CACHE_FILE)
        && bytes.len() > CACHE_FILE_LIMIT_BYTES
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "OpenDota cache exceeds its file limit",
        ));
    }
    crate::atomic::write(path, &bytes)
}

fn delete_file(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::thread;

    struct FixtureTransport {
        responses: Mutex<VecDeque<Result<HttpResponse, TransportError>>>,
        paths: Mutex<Vec<String>>,
        cancels: AtomicUsize,
    }

    impl FixtureTransport {
        fn new(responses: impl IntoIterator<Item = Result<HttpResponse, TransportError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().collect()),
                paths: Mutex::new(Vec::new()),
                cancels: AtomicUsize::new(0),
            }
        }
    }

    impl HistoryTransport for FixtureTransport {
        fn get(
            &self,
            path: &str,
            _cancellation: &Cancellation,
        ) -> Result<HttpResponse, TransportError> {
            lock(&self.paths).push(path.to_owned());
            lock(&self.responses)
                .pop_front()
                .unwrap_or(Err(TransportError::NotSent))
        }
        fn cancel_current(&self) {
            self.cancels.fetch_add(1, Ordering::Relaxed);
        }
    }

    struct BlockingTransport {
        started: Mutex<Option<Sender<()>>>,
        release: Mutex<Receiver<()>>,
        cancels: AtomicUsize,
    }

    impl HistoryTransport for BlockingTransport {
        fn get(
            &self,
            _path: &str,
            _cancellation: &Cancellation,
        ) -> Result<HttpResponse, TransportError> {
            if let Some(started) = lock(&self.started).take() {
                let _ = started.send(());
            }
            let _ = lock(&self.release).recv();
            response(200, fixture())
        }
        fn cancel_current(&self) {
            self.cancels.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("kohbor-opendota-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn response(status: u16, body: &str) -> Result<HttpResponse, TransportError> {
        Ok(HttpResponse {
            status,
            retry_after: None,
            body: body.as_bytes().to_vec(),
        })
    }

    fn fixture() -> &'static str {
        r#"[{"match_id":8000000001,"hero_id":2,"player_slot":0,"radiant_win":true,"kills":8,"deaths":2,"assists":12,"duration":1820,"start_time":1795000000,"lobby_type":7,"game_mode":1}]"#
    }

    fn enabled_service(
        tag: &str,
        responses: impl IntoIterator<Item = Result<HttpResponse, TransportError>>,
    ) -> (OpenDotaService, Arc<FixtureTransport>, PathBuf) {
        let dir = temp_dir(tag);
        let transport = Arc::new(FixtureTransport::new(responses));
        let service = OpenDotaService::open_with_transport(&dir, Some([7; 32]), transport.clone());
        service.set_enabled(true, Some(12_345)).unwrap();
        (service, transport, dir)
    }

    #[test]
    fn enabling_without_account_waits_without_network_request() {
        let dir = temp_dir("pending-consent");
        let transport = Arc::new(FixtureTransport::new(std::iter::empty::<
            Result<HttpResponse, TransportError>,
        >()));
        let service = OpenDotaService::open_with_transport(&dir, Some([7; 32]), transport.clone());

        service.set_enabled(true, None).unwrap();
        let view = service.refresh(false);

        assert_eq!(view["state"], "waiting_for_account");
        assert_eq!(view["enabled"], true);
        assert_eq!(view["retry_available"], false);
        assert!(transport.paths.lock().unwrap().is_empty());
        let prefs: Value =
            serde_json::from_slice(&fs::read(dir.join(SETTINGS_FILE)).unwrap()).unwrap();
        assert_eq!(prefs["enabled"], true);
        assert!(prefs.get("account_id").is_none());

        let restored = OpenDotaService::open_with_transport(&dir, Some([7; 32]), transport.clone());
        assert_eq!(restored.prefs()["enabled"], true);
        assert_eq!(restored.view()["state"], "waiting_for_account");
        assert!(transport.paths.lock().unwrap().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn account_available_after_opt_in_is_saved_and_fetched_once() {
        let dir = temp_dir("pending-account-arrives");
        let transport = Arc::new(FixtureTransport::new([response(200, fixture())]));
        let service = OpenDotaService::open_with_transport(&dir, Some([7; 32]), transport.clone());

        service.set_enabled(true, None).unwrap();
        assert_eq!(service.refresh(false)["state"], "waiting_for_account");
        service.update_account_if_enabled(12_345).unwrap();
        assert_eq!(service.refresh(false)["state"], "merged");
        assert_eq!(service.refresh(false)["state"], "merged");

        let settings = fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap();
        assert!(settings.contains("12345"));
        assert!(!settings.contains("7656119"));
        assert_eq!(transport.paths.lock().unwrap().len(), 1);
        assert!(transport.paths.lock().unwrap()[0].starts_with("/api/players/12345/matches?"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn changed_verified_account_cancels_clears_and_refetches() {
        let dir = temp_dir("account-change");
        let transport = Arc::new(FixtureTransport::new([
            response(200, fixture()),
            response(200, fixture()),
        ]));
        let service = OpenDotaService::open_with_transport(&dir, Some([7; 32]), transport.clone());

        service.set_enabled(true, Some(12_345)).unwrap();
        assert_eq!(service.refresh(false)["state"], "merged");
        assert!(dir.join(CACHE_FILE).exists());
        service.update_account_if_enabled(23_456).unwrap();

        assert!(service.entries().is_empty());
        assert!(!dir.join(CACHE_FILE).exists());
        assert_eq!(service.refresh(false)["state"], "merged");
        let paths = transport.paths.lock().unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].starts_with("/api/players/12345/matches?"));
        assert!(paths[1].starts_with("/api/players/23456/matches?"));
        assert!(transport.cancels.load(Ordering::Relaxed) > 0);
        let settings = fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap();
        assert!(settings.contains("23456"));
        assert!(!settings.contains("12345"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn revoke_from_pending_clears_consent_account_and_cache() {
        let dir = temp_dir("pending-revoke");
        let transport = Arc::new(FixtureTransport::new(std::iter::empty::<
            Result<HttpResponse, TransportError>,
        >()));
        let service = OpenDotaService::open_with_transport(&dir, Some([7; 32]), transport.clone());

        service.set_enabled(true, None).unwrap();
        assert_eq!(service.refresh(false)["state"], "waiting_for_account");
        fs::write(dir.join(CACHE_FILE), b"pending cache").unwrap();
        service.set_enabled(false, None).unwrap();

        assert_eq!(service.prefs()["enabled"], false);
        assert_eq!(service.view()["state"], "disabled");
        assert!(!dir.join(CACHE_FILE).exists());
        let settings = fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap();
        assert!(!settings.contains("account_id"));
        assert!(transport.paths.lock().unwrap().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn account_poll_after_revoke_does_not_restore_consent() {
        let dir = temp_dir("pending-revoke-account-poll");
        let transport = Arc::new(FixtureTransport::new(std::iter::empty::<
            Result<HttpResponse, TransportError>,
        >()));
        let service = OpenDotaService::open_with_transport(&dir, Some([7; 32]), transport.clone());

        service.set_enabled(true, None).unwrap();
        service.set_enabled(false, None).unwrap();
        service.update_account_if_enabled(12_345).unwrap();

        assert_eq!(service.prefs()["enabled"], false);
        assert_eq!(service.view()["state"], "disabled");
        let settings = fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap();
        assert!(!settings.contains("account_id"));
        assert!(transport.paths.lock().unwrap().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn parses_only_projected_fields_and_hashes_match_id_before_cache_storage() {
        let entries = parse_matches(fixture().as_bytes(), Some(&[7; 32])).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hero_id, Some(2));
        assert_eq!(entries[0].match_type, MatchType::Ranked);
        assert_eq!(entries[0].result, Some(MatchResult::Win));
        assert!(
            !serde_json::to_string(&entries)
                .unwrap()
                .contains("8000000001")
        );
        assert_eq!(
            request_path(12_345),
            format!(
                "/api/players/12345/matches?limit=150&project={}",
                PROJECT_FIELDS.replace(',', "%2C")
            )
        );
        assert_eq!(HISTORY_ENDPOINT.host, "api.opendota.com");
    }

    #[test]
    fn private_profile_empty_and_missing_profile_are_distinct_states() {
        assert!(matches!(
            classify_response(
                HttpResponse {
                    status: 403,
                    retry_after: None,
                    body: vec![]
                },
                Some(&[7; 32])
            ),
            Classified::State(State::PrivateProfile)
        ));
        assert!(matches!(
            classify_response(
                HttpResponse {
                    status: 404,
                    retry_after: None,
                    body: vec![]
                },
                Some(&[7; 32])
            ),
            Classified::State(State::NotFound)
        ));
        assert!(
            matches!(classify_response(HttpResponse { status: 200, retry_after: None, body: b"[]".to_vec() }, Some(&[7; 32])), Classified::Rows(rows) if rows.is_empty())
        );
        assert!(matches!(
            classify_response(
                HttpResponse {
                    status: 200,
                    retry_after: None,
                    body: br#"{"error":"Private profile"}"#.to_vec()
                },
                Some(&[7; 32])
            ),
            Classified::State(State::PrivateProfile)
        ));
    }

    #[test]
    fn startup_fetch_is_once_and_manual_retries_use_a_persistent_four_request_budget() {
        let responses = (0..5).map(|_| response(200, fixture()));
        let (service, transport, _dir) = enabled_service("budget", responses);
        assert_eq!(service.refresh(false)["state"], "merged");
        assert_eq!(service.refresh(false)["state"], "merged");
        for _ in 0..3 {
            assert_eq!(service.refresh(true)["state"], "merged");
        }
        assert_eq!(transport.paths.lock().unwrap().len(), REQUEST_BUDGET);
        assert_eq!(service.refresh(true)["state"], "budget_exhausted");
        assert_eq!(transport.paths.lock().unwrap().len(), REQUEST_BUDGET);
        assert!(transport.paths.lock().unwrap()[0].starts_with("/api/players/12345/matches?"));
    }

    #[test]
    fn a_setup_failure_before_network_io_does_not_consume_the_budget() {
        let mut responses = vec![Err(TransportError::NotSent)];
        responses.extend((0..REQUEST_BUDGET).map(|_| response(200, fixture())));
        let (service, transport, _dir) = enabled_service("pre-send", responses);
        assert_eq!(service.refresh(false)["state"], "unavailable");
        for _ in 0..REQUEST_BUDGET {
            assert_eq!(service.refresh(true)["state"], "merged");
        }
        assert_eq!(transport.paths.lock().unwrap().len(), REQUEST_BUDGET + 1);
        assert_eq!(service.refresh(true)["state"], "budget_exhausted");
    }

    #[test]
    fn revoke_removes_account_id_and_cache_and_cancels_transport() {
        let (service, transport, dir) = enabled_service("revoke", [response(200, fixture())]);
        service.refresh(false);
        assert!(dir.join(CACHE_FILE).exists());
        let settings = fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap();
        assert!(settings.contains("12345"));
        assert!(!settings.contains("7656119"));
        service.set_enabled(false, None).unwrap();
        assert!(!dir.join(CACHE_FILE).exists());
        let settings = fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap();
        assert!(!settings.contains("account_id"));
        assert_eq!(service.view()["state"], "disabled");
        assert!(transport.cancels.load(Ordering::Relaxed) > 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn revoke_invalidates_a_late_in_flight_response() {
        let dir = temp_dir("late-revoke");
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let transport = Arc::new(BlockingTransport {
            started: Mutex::new(Some(started_tx)),
            release: Mutex::new(release_rx),
            cancels: AtomicUsize::new(0),
        });
        let service = Arc::new(OpenDotaService::open_with_transport(
            &dir,
            Some([7; 32]),
            transport.clone(),
        ));
        service.set_enabled(true, Some(12_345)).unwrap();
        let worker_service = service.clone();
        let worker = thread::spawn(move || worker_service.refresh(false));
        started_rx.recv().unwrap();
        service.set_enabled(false, None).unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(worker.join().unwrap()["state"], "disabled");
        assert!(!dir.join(CACHE_FILE).exists());
        assert!(service.entries().is_empty());
        assert!(transport.cancels.load(Ordering::Relaxed) > 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn hmac_matches_the_installation_session_key_scheme() {
        let key = [7; 32];
        let mut session = kohbor_gsi::session::SessionTracker::new(key);
        session.observe(Some("8000000001"), 1, || "test-session".to_owned());
        let expected = &session
            .current()
            .expect("a match starts a session")
            .match_key;
        assert_eq!(
            install_match_key_from_id(8_000_000_001, &key),
            expected.as_str()
        );
    }
}
