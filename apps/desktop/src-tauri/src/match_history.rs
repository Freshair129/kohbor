//! Local own-match history for the Overview page (OVERVIEW-002, owner decision 2026-11-23).
//!
//! One entry per match the verified local player played, built from admitted GSI payloads only.
//! The match is identified by the keyed hash the session tracker already uses
//! (GSI-INTAKE-SPEC §6): the raw match ID and the SteamID64 are never stored. No other player's
//! name or identifier is read or kept.
//!
//! Win rate counts real matches only (decision item 3): ranked and unranked public matches with a
//! known result. GSI exposes no game-mode or lobby-type field (GSI-INTAKE-SPEC §4.2, A-9), and the
//! app must not infer one, so every entry recorded today has `match_type = "unknown"` and is
//! excluded from the win rate. A quit or abandoned match keeps `result = null` ("—"), never a loss.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

pub const SCHEMA_VERSION: &str = "1.0.0";
pub const FILE_NAME: &str = "match-history.json";
/// Bounded history (DEC-RETENTION): the oldest entries fall off beyond this count.
pub const MAX_ENTRIES: usize = 500;
pub const MAX_FILE_BYTES: usize = 1_048_576;
/// Window of the "last N" tile.
pub const LAST_N: usize = 20;
/// Rolling window of the trend line; no line below this many counted matches.
pub const TREND_WINDOW: usize = 10;
/// At most this many trend points are returned (the newest ones).
pub const TREND_MAX_POINTS: usize = 50;
/// A hero's win rate is shown only from this many counted games; below it, W–L counts.
pub const HERO_RATE_MIN_GAMES: u32 = 5;
pub const TOP_HEROES: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchResult {
    Win,
    Loss,
}

/// How the recorded match ended, independent of the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchEnd {
    /// Post-game was observed with a winning team.
    Completed,
    /// The player left (menu, game closed, another match) before a winner was observed.
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    Ranked,
    Unranked,
    Bot,
    Custom,
    Lobby,
    Unknown,
}

impl MatchType {
    /// Only real public matches count toward win rate (owner decision item 3).
    pub fn counts_for_win_rate(self) -> bool {
        matches!(self, Self::Ranked | Self::Unranked)
    }
}

/// The match type from one admitted payload. GSI carries no mode or lobby field (A-9) and
/// GSI-INTAKE-SPEC §4.2 forbids inferring one from timing, gold, score or other heuristics, so
/// this is `Unknown` until an explicit, documented field exists. Custom games never reach here:
/// admission rejects them as unsupported.
pub fn match_type_from_gsi(_body: &Value) -> MatchType {
    MatchType::Unknown
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchEntry {
    /// HMAC-SHA-256 of the match ID under the per-installation key (64 lowercase hex).
    pub match_key: String,
    /// Internal hero name (`npc_dota_hero_*`), null when never observed.
    pub hero: Option<String>,
    pub result: Option<MatchResult>,
    pub end: MatchEnd,
    pub kills: Option<u32>,
    pub deaths: Option<u32>,
    pub assists: Option<u32>,
    /// Game clock at the last observation of a completed match; null for an abandoned one.
    pub duration_s: Option<u32>,
    /// Wall-clock time of recording, Unix seconds.
    pub ended_at: u64,
    pub match_type: MatchType,
    pub source: String,
}

impl MatchEntry {
    fn counted(&self) -> bool {
        self.match_type.counts_for_win_rate() && self.result.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryFile {
    schema_version: String,
    /// Newest first.
    entries: Vec<MatchEntry>,
}

impl Default for HistoryFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_owned(),
            entries: Vec::new(),
        }
    }
}

fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn valid_hero(hero: &str) -> bool {
    hero.len() <= 64
        && hero.starts_with("npc_dota_hero_")
        && hero
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn valid_entry(entry: &MatchEntry) -> bool {
    valid_key(&entry.match_key)
        && entry.hero.as_deref().is_none_or(valid_hero)
        && entry.source == "kohbor"
        && !(entry.end == MatchEnd::Abandoned && entry.result.is_some())
}

fn valid_file(file: &HistoryFile) -> bool {
    if file.schema_version != SCHEMA_VERSION || file.entries.len() > MAX_ENTRIES {
        return false;
    }
    let mut seen = std::collections::BTreeSet::new();
    file.entries
        .iter()
        .all(|entry| valid_entry(entry) && seen.insert(entry.match_key.as_str()))
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
struct StoreState {
    path: PathBuf,
    file: HistoryFile,
    write_error_logged: bool,
}

/// Atomic JSON file in the app data directory. Opening never creates a file; only a recorded
/// match or an explicit clear writes one.
#[derive(Clone, Debug)]
pub struct MatchHistoryStore {
    inner: Arc<Mutex<StoreState>>,
}

impl MatchHistoryStore {
    pub fn open_in(data_dir: &Path) -> Self {
        Self::open(data_dir.join(FILE_NAME))
    }

    pub fn open(path: PathBuf) -> Self {
        let file = match load(&path) {
            Ok(file) => file,
            Err(code) => {
                crate::log_warn!(
                    "history",
                    "match_history_unreadable",
                    "stored match history unreadable; starting empty",
                    code = code
                );
                HistoryFile::default()
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

    pub fn entries(&self) -> Vec<MatchEntry> {
        lock(&self.inner).file.entries.clone()
    }

    /// Insert or replace the entry for this match. An abandoned entry never replaces a
    /// completed one (for example a later reconnect followed by leaving to the menu).
    pub fn upsert(&self, entry: MatchEntry) -> Result<(), String> {
        if !valid_entry(&entry) {
            return Err("HISTORY_ENTRY_INVALID".to_owned());
        }
        let mut state = lock(&self.inner);
        let mut next = state.file.clone();
        if let Some(index) = next
            .entries
            .iter()
            .position(|e| e.match_key == entry.match_key)
        {
            if next.entries[index].end == MatchEnd::Completed && entry.end == MatchEnd::Abandoned {
                return Ok(());
            }
            next.entries.remove(index);
        }
        next.entries.insert(0, entry);
        next.entries.truncate(MAX_ENTRIES);
        commit(&mut state, next)
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut state = lock(&self.inner);
        commit(&mut state, HistoryFile::default())
    }

    /// The `match_history_get` reply: entries (newest first) and the computed summary.
    pub fn view(&self) -> Value {
        let entries = self.entries();
        json!({
            "schema_version": SCHEMA_VERSION,
            "summary": summarize(&entries),
            "entries": entries,
        })
    }
}

fn load(path: &Path) -> Result<HistoryFile, &'static str> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(HistoryFile::default())
        }
        Err(_) => return Err("HISTORY_READ_FAILED"),
    };
    if metadata.len() > MAX_FILE_BYTES as u64 {
        return Err("HISTORY_CORRUPT");
    }
    let bytes = fs::read(path).map_err(|_| "HISTORY_READ_FAILED")?;
    let file: HistoryFile = serde_json::from_slice(&bytes).map_err(|_| "HISTORY_CORRUPT")?;
    valid_file(&file).then_some(file).ok_or("HISTORY_CORRUPT")
}

fn commit(state: &mut StoreState, next: HistoryFile) -> Result<(), String> {
    let bytes = serde_json::to_vec(&next).map_err(|_| "HISTORY_WRITE_FAILED".to_owned())?;
    if bytes.len() > MAX_FILE_BYTES || crate::atomic::write(&state.path, &bytes).is_err() {
        if !state.write_error_logged {
            state.write_error_logged = true;
            crate::log_warn!(
                "history",
                "match_history_write_failed",
                "could not persist local match history",
                code = "HISTORY_WRITE_FAILED"
            );
        }
        return Err("HISTORY_WRITE_FAILED".to_owned());
    }
    state.file = next;
    Ok(())
}

/// Summary over the stored entries. Every rate is over counted matches only.
pub fn summarize(entries: &[MatchEntry]) -> Value {
    let counted: Vec<&MatchEntry> = entries.iter().filter(|e| e.counted()).collect();
    let wins = counted
        .iter()
        .filter(|e| e.result == Some(MatchResult::Win))
        .count();
    let rate = |w: usize, n: usize| (n > 0).then(|| (w as f64) * 100.0 / (n as f64));

    let last: Vec<&MatchEntry> = counted.iter().take(LAST_N).copied().collect();
    let last_wins = last
        .iter()
        .filter(|e| e.result == Some(MatchResult::Win))
        .count();

    // Rolling win rate, oldest -> newest, one point per counted match from the 10th on.
    let chronological: Vec<u8> = counted
        .iter()
        .rev()
        .map(|e| u8::from(e.result == Some(MatchResult::Win)))
        .collect();
    let mut trend: Vec<f64> = chronological
        .windows(TREND_WINDOW)
        .map(|w| {
            f64::from(w.iter().map(|v| u32::from(*v)).sum::<u32>()) * 100.0 / TREND_WINDOW as f64
        })
        .collect();
    if trend.len() > TREND_MAX_POINTS {
        trend.drain(..trend.len() - TREND_MAX_POINTS);
    }

    // Heroes by games recorded (all entries); W-L and rate from counted matches only.
    let mut heroes: BTreeMap<&str, (u32, u32, u32)> = BTreeMap::new();
    for entry in entries {
        let Some(hero) = entry.hero.as_deref() else {
            continue;
        };
        let slot = heroes.entry(hero).or_default();
        slot.0 += 1;
        if entry.counted() {
            if entry.result == Some(MatchResult::Win) {
                slot.1 += 1;
            } else {
                slot.2 += 1;
            }
        }
    }
    let mut top: Vec<(&str, (u32, u32, u32))> = heroes.into_iter().collect();
    top.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
    let top_heroes: Vec<Value> = top
        .into_iter()
        .take(TOP_HEROES)
        .map(|(hero, (games, w, l))| {
            let counted_games = w + l;
            json!({
                "hero": hero,
                "games": games,
                "counted_games": counted_games,
                "wins": w,
                "losses": l,
                "win_rate": (counted_games >= HERO_RATE_MIN_GAMES)
                    .then(|| f64::from(w) * 100.0 / f64::from(counted_games)),
            })
        })
        .collect();

    let unknown_type = entries
        .iter()
        .filter(|e| e.result.is_some() && !e.match_type.counts_for_win_rate())
        .count();
    let unknown_result = entries.iter().filter(|e| e.result.is_none()).count();
    json!({
        "recorded": entries.len(),
        "counted": counted.len(),
        "excluded_match_type": unknown_type,
        "excluded_no_result": unknown_result,
        "overall": {"wins": wins, "losses": counted.len() - wins, "win_rate": rate(wins, counted.len())},
        "last": {
            "results": last.iter().map(|e| e.result).collect::<Vec<_>>(),
            "wins": last_wins,
            "losses": last.len() - last_wins,
            "win_rate": rate(last_wins, last.len()),
        },
        "trend": {"window": TREND_WINDOW, "points": trend},
        "top_heroes": top_heroes,
    })
}

/// One verified, admitted payload reduced to what the history needs.
#[derive(Debug, Clone, Copy)]
pub struct Observation<'a> {
    pub match_key: &'a str,
    pub phase: kohbor_gsi::admission::Phase,
    pub team: Option<kohbor_gsi::admission::Team>,
    pub hero: Option<&'a str>,
    pub kills: Option<i64>,
    pub deaths: Option<i64>,
    pub assists: Option<i64>,
    pub clock_s: Option<u64>,
    /// `map.win_team` as sent: `radiant`, `dire`, `none` or absent.
    pub win_team: Option<&'a str>,
    pub match_type: MatchType,
}

#[derive(Debug, Clone)]
struct Pending {
    match_key: String,
    hero: Option<String>,
    team: Option<kohbor_gsi::admission::Team>,
    kills: Option<u32>,
    deaths: Option<u32>,
    assists: Option<u32>,
    clock_s: Option<u32>,
    match_type: MatchType,
    /// Pregame or live play was observed in this process.
    played: bool,
    finished: bool,
}

impl Pending {
    fn entry(&self, end: MatchEnd, result: Option<MatchResult>, now: u64) -> MatchEntry {
        MatchEntry {
            match_key: self.match_key.clone(),
            hero: self.hero.clone(),
            result,
            end,
            kills: self.kills,
            deaths: self.deaths,
            assists: self.assists,
            duration_s: (end == MatchEnd::Completed)
                .then_some(self.clock_s)
                .flatten(),
            ended_at: now,
            match_type: self.match_type,
            source: "kohbor".to_owned(),
        }
    }
}

fn count(value: Option<i64>) -> Option<u32> {
    value.and_then(|v| u32::try_from(v).ok())
}

/// Turns the admitted payload stream into history entries. Pure: the caller stores the entries.
#[derive(Debug, Default)]
pub struct MatchTracker {
    current: Option<Pending>,
}

impl MatchTracker {
    /// Observe one verified payload of the current session. Returns the entries to store:
    /// an abandoned previous match when the match key changed, and the completed match once
    /// post-game reports a winner.
    pub fn observe(&mut self, obs: &Observation<'_>, now: u64) -> Vec<MatchEntry> {
        use kohbor_gsi::admission::{Phase, Team};
        let mut out = Vec::new();
        if self
            .current
            .as_ref()
            .is_some_and(|p| p.match_key != obs.match_key)
        {
            out.extend(self.leave(now));
        }
        let pending = self.current.get_or_insert_with(|| Pending {
            match_key: obs.match_key.to_owned(),
            hero: None,
            team: None,
            kills: None,
            deaths: None,
            assists: None,
            clock_s: None,
            match_type: obs.match_type,
            played: false,
            finished: false,
        });
        if pending.finished {
            return out;
        }
        if let Some(hero) = obs.hero.filter(|h| valid_hero(h)) {
            pending.hero.get_or_insert_with(|| hero.to_owned());
        }
        if obs.team.is_some() {
            pending.team = obs.team;
        }
        if matches!(obs.phase, Phase::Pregame | Phase::Live) {
            pending.played = true;
        }
        if matches!(obs.phase, Phase::Live | Phase::PostGame) {
            pending.kills = count(obs.kills).or(pending.kills);
            pending.deaths = count(obs.deaths).or(pending.deaths);
            pending.assists = count(obs.assists).or(pending.assists);
            if let Some(clock) = obs.clock_s.and_then(|c| u32::try_from(c).ok()) {
                pending.clock_s = Some(clock);
            }
        }
        if obs.phase == Phase::PostGame {
            let winner = match obs.win_team {
                Some("radiant") => Some(Team::Radiant),
                Some("dire") => Some(Team::Dire),
                _ => None,
            };
            if let Some(winner) = winner {
                let result = pending.team.map(|own| {
                    if own == winner {
                        MatchResult::Win
                    } else {
                        MatchResult::Loss
                    }
                });
                pending.finished = true;
                out.push(pending.entry(MatchEnd::Completed, result, now));
            }
        }
        out
    }

    /// The player left the match (menu, window or process loss). A match that was played but
    /// never reported a winner is recorded as abandoned with no result.
    pub fn leave(&mut self, now: u64) -> Option<MatchEntry> {
        let pending = self.current.take()?;
        (pending.played && !pending.finished).then(|| pending.entry(MatchEnd::Abandoned, None, now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kohbor_gsi::admission::{Phase, Team};

    const KEY_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const KEY_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn obs<'a>(key: &'a str, phase: Phase, win_team: Option<&'a str>) -> Observation<'a> {
        Observation {
            match_key: key,
            phase,
            team: Some(Team::Radiant),
            hero: Some("npc_dota_hero_axe"),
            kills: Some(5),
            deaths: Some(2),
            assists: Some(9),
            clock_s: Some(1800),
            win_team,
            match_type: MatchType::Unknown,
        }
    }

    fn entry(
        key: String,
        result: Option<MatchResult>,
        match_type: MatchType,
        hero: &str,
    ) -> MatchEntry {
        MatchEntry {
            match_key: key,
            hero: Some(hero.to_owned()),
            result,
            end: if result.is_some() {
                MatchEnd::Completed
            } else {
                MatchEnd::Abandoned
            },
            kills: Some(1),
            deaths: Some(1),
            assists: Some(1),
            duration_s: result.map(|_| 1500),
            ended_at: 1_700_000_000,
            match_type,
            source: "kohbor".to_owned(),
        }
    }

    fn key(n: usize) -> String {
        format!("{n:064x}")
    }

    fn temp_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kohbor-history-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn post_game_with_own_team_winning_records_one_completed_win() {
        let mut t = MatchTracker::default();
        assert!(t.observe(&obs(KEY_A, Phase::Draft, None), 1).is_empty());
        assert!(t
            .observe(&obs(KEY_A, Phase::Live, Some("none")), 2)
            .is_empty());
        let out = t.observe(&obs(KEY_A, Phase::PostGame, Some("radiant")), 3);
        assert_eq!(out.len(), 1);
        let e = &out[0];
        assert_eq!(e.result, Some(MatchResult::Win));
        assert_eq!(e.end, MatchEnd::Completed);
        assert_eq!((e.kills, e.deaths, e.assists), (Some(5), Some(2), Some(9)));
        assert_eq!(e.duration_s, Some(1800));
        assert_eq!(e.match_type, MatchType::Unknown);
        // Further post-game payloads and the return to the menu add nothing.
        assert!(t
            .observe(&obs(KEY_A, Phase::PostGame, Some("radiant")), 4)
            .is_empty());
        assert!(t.leave(5).is_none());
    }

    #[test]
    fn other_team_winning_is_a_loss_and_unknown_team_is_no_result() {
        let mut t = MatchTracker::default();
        t.observe(&obs(KEY_A, Phase::Live, None), 1);
        assert_eq!(
            t.observe(&obs(KEY_A, Phase::PostGame, Some("dire")), 2)[0].result,
            Some(MatchResult::Loss)
        );
        let mut t = MatchTracker::default();
        let mut o = obs(KEY_B, Phase::Live, None);
        o.team = None;
        t.observe(&o, 1);
        o.phase = Phase::PostGame;
        o.win_team = Some("dire");
        let out = t.observe(&o, 2);
        assert_eq!(out[0].result, None);
        assert_eq!(out[0].end, MatchEnd::Completed);
    }

    #[test]
    fn leaving_before_a_winner_is_abandoned_never_a_loss() {
        let mut t = MatchTracker::default();
        t.observe(&obs(KEY_A, Phase::Live, Some("none")), 1);
        // Post-game without a winner yet is not a result.
        assert!(t
            .observe(&obs(KEY_A, Phase::PostGame, Some("none")), 2)
            .is_empty());
        let e = t.leave(3).expect("abandoned entry");
        assert_eq!(e.result, None);
        assert_eq!(e.end, MatchEnd::Abandoned);
        assert_eq!(e.duration_s, None);
    }

    #[test]
    fn a_new_match_key_abandons_the_previous_match() {
        let mut t = MatchTracker::default();
        t.observe(&obs(KEY_A, Phase::Live, None), 1);
        let out = t.observe(&obs(KEY_B, Phase::Draft, None), 2);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].match_key, KEY_A);
        assert_eq!(out[0].end, MatchEnd::Abandoned);
    }

    #[test]
    fn a_lobby_left_during_draft_is_not_recorded() {
        let mut t = MatchTracker::default();
        t.observe(&obs(KEY_A, Phase::Draft, None), 1);
        assert!(t.leave(2).is_none());
    }

    #[test]
    fn store_upserts_by_key_and_abandoned_never_replaces_completed() {
        let dir = temp_dir("upsert");
        let store = MatchHistoryStore::open_in(&dir);
        assert!(
            !dir.join(FILE_NAME).exists(),
            "opening never creates a file"
        );
        store
            .upsert(entry(
                KEY_A.into(),
                None,
                MatchType::Unknown,
                "npc_dota_hero_axe",
            ))
            .unwrap();
        store
            .upsert(entry(
                KEY_A.into(),
                Some(MatchResult::Win),
                MatchType::Unknown,
                "npc_dota_hero_axe",
            ))
            .unwrap();
        store
            .upsert(entry(
                KEY_A.into(),
                None,
                MatchType::Unknown,
                "npc_dota_hero_axe",
            ))
            .unwrap();
        let entries = MatchHistoryStore::open_in(&dir).entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].result, Some(MatchResult::Win));
        let text = fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert!(!text.contains("steamid") && !text.contains("matchid"));
        store.clear().unwrap();
        assert!(MatchHistoryStore::open_in(&dir).entries().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_is_bounded_and_rejects_raw_ids_and_corrupt_files() {
        let dir = temp_dir("bounded");
        let store = MatchHistoryStore::open_in(&dir);
        for n in 0..(MAX_ENTRIES + 7) {
            store
                .upsert(entry(
                    key(n),
                    Some(MatchResult::Win),
                    MatchType::Ranked,
                    "npc_dota_hero_axe",
                ))
                .unwrap();
        }
        let entries = store.entries();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].match_key, key(MAX_ENTRIES + 6), "newest first");
        assert!(store
            .upsert(entry(
                "8000000001".into(),
                None,
                MatchType::Unknown,
                "npc_dota_hero_axe"
            ))
            .is_err());
        fs::write(dir.join(FILE_NAME), b"{\"schema_version\":\"1.0.0\"").unwrap();
        assert!(MatchHistoryStore::open_in(&dir).entries().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn summary_counts_only_ranked_and_unranked_matches_with_a_result() {
        let mut entries = vec![
            entry(
                key(1),
                Some(MatchResult::Win),
                MatchType::Ranked,
                "npc_dota_hero_axe",
            ),
            entry(
                key(2),
                Some(MatchResult::Loss),
                MatchType::Unranked,
                "npc_dota_hero_axe",
            ),
            entry(
                key(3),
                Some(MatchResult::Win),
                MatchType::Bot,
                "npc_dota_hero_axe",
            ),
            entry(
                key(4),
                Some(MatchResult::Win),
                MatchType::Unknown,
                "npc_dota_hero_lion",
            ),
            entry(key(5), None, MatchType::Ranked, "npc_dota_hero_lion"),
        ];
        let s = summarize(&entries);
        assert_eq!(s["recorded"], 5);
        assert_eq!(s["counted"], 2);
        assert_eq!(s["excluded_match_type"], 2);
        assert_eq!(s["excluded_no_result"], 1);
        assert_eq!(s["overall"]["wins"], 1);
        assert_eq!(s["overall"]["losses"], 1);
        assert_eq!(s["overall"]["win_rate"], 50.0);
        assert_eq!(s["last"]["results"], json!(["win", "loss"]));
        assert_eq!(s["trend"]["points"], json!([]));
        let axe = &s["top_heroes"][0];
        assert_eq!(axe["hero"], "npc_dota_hero_axe");
        assert_eq!(
            (
                axe["games"].as_u64(),
                axe["wins"].as_u64(),
                axe["losses"].as_u64()
            ),
            (Some(3), Some(1), Some(1))
        );
        assert_eq!(
            axe["win_rate"],
            Value::Null,
            "fewer than five counted games"
        );

        entries.clear();
        let s = summarize(&entries);
        assert_eq!(s["overall"]["win_rate"], Value::Null);
        assert_eq!(s["last"]["win_rate"], Value::Null);
    }

    #[test]
    fn summary_only_unknown_type_matches_have_no_win_rate() {
        // What GSI produces today: every match type is unknown (A-9).
        let entries: Vec<MatchEntry> = (0..12)
            .map(|n| {
                entry(
                    key(n),
                    Some(MatchResult::Win),
                    MatchType::Unknown,
                    "npc_dota_hero_axe",
                )
            })
            .collect();
        let s = summarize(&entries);
        assert_eq!(s["counted"], 0);
        assert_eq!(s["overall"]["win_rate"], Value::Null);
        assert_eq!(s["trend"]["points"], json!([]));
        assert_eq!(s["top_heroes"][0]["games"], 12);
        assert_eq!(s["top_heroes"][0]["win_rate"], Value::Null);
    }

    #[test]
    fn trend_is_a_ten_match_rolling_rate_and_hero_rate_needs_five_games() {
        // Newest first: 12 counted matches, newest two are losses, all others wins.
        let entries: Vec<MatchEntry> = (0..12)
            .map(|n| {
                let result = if n < 2 {
                    MatchResult::Loss
                } else {
                    MatchResult::Win
                };
                entry(key(n), Some(result), MatchType::Ranked, "npc_dota_hero_axe")
            })
            .collect();
        let s = summarize(&entries);
        assert_eq!(s["trend"]["points"], json!([100.0, 90.0, 80.0]));
        assert_eq!(s["last"]["results"].as_array().unwrap().len(), 12);
        assert_eq!(
            s["top_heroes"][0]["win_rate"].as_f64().unwrap().round(),
            83.0
        );
    }

    #[test]
    fn gsi_match_type_is_unknown_without_an_explicit_field() {
        let body: Value = serde_json::from_str(include_str!(
            "../../../../crates/kohbor-gsi/tests/captured/post-game.json"
        ))
        .unwrap();
        assert_eq!(match_type_from_gsi(&body), MatchType::Unknown);
        assert!(!MatchType::Unknown.counts_for_win_rate());
        assert!(!MatchType::Bot.counts_for_win_rate());
        assert!(!MatchType::Custom.counts_for_win_rate());
        assert!(!MatchType::Lobby.counts_for_win_rate());
    }
}
