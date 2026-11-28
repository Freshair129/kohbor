//! Read-only post-game replay lookup keyed by the active host session record.

use crate::analytics::AnalyticsService;
use crate::intake::LastFinishedMatchRecord;
use crate::match_history::MatchEntry;
use crate::replay_library::{ReplayBundle, ReplayLibrary, ReplayLibraryEntry, ReplayStatus};
use crate::AppState;
use serde::Serialize;
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use tauri::State;

const MAX_RUNTIME_STATE_BYTES: usize = 64 * 1024;
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PostGameReplayReply {
    ready: bool,
    match_key: Option<String>,
    /// Raw ID from a hash-linked, validated ready replay bundle only.
    bundle_match_id: Option<String>,
    /// Raw ID from the current in-memory GSI session only; never part of persisted state.
    session_match_id: Option<String>,
    digest: Option<Value>,
    headline: PostGameHeadline,
    parse_state: PostGameParseState,
    failure_reason: Option<String>,
    last_finished_match: Option<LastFinishedMatchRecord>,
    watcher: PostGameWatcherReply,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PostGameParseState {
    NotFound,
    Available,
    Queued,
    Parsing,
    Ready,
    Failed,
    NeedsPack,
    AutoParseOff,
    Unknown,
    Off,
    WaitingForMatchEnd,
    LookingUp,
    Requested,
    Downloading,
    NotAvailableYet,
    Expired,
    RateLimited,
    Offline,
}

impl From<crate::replay_autofetch::FetchState> for PostGameParseState {
    fn from(state: crate::replay_autofetch::FetchState) -> Self {
        use crate::replay_autofetch::FetchState as Fetch;
        match state {
            Fetch::Off => Self::Off,
            Fetch::WaitingForMatchEnd => Self::WaitingForMatchEnd,
            Fetch::LookingUp => Self::LookingUp,
            Fetch::Requested => Self::Requested,
            Fetch::Downloading => Self::Downloading,
            Fetch::Parsing => Self::Parsing,
            Fetch::Ready => Self::Ready,
            Fetch::NotAvailableYet => Self::NotAvailableYet,
            Fetch::NotFound => Self::NotFound,
            Fetch::Expired => Self::Expired,
            Fetch::RateLimited => Self::RateLimited,
            Fetch::Offline => Self::Offline,
            Fetch::Failed => Self::Failed,
            Fetch::SkippedLocal => Self::NotFound,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PostGameReplayStateReply {
    state: PostGameParseState,
    context: PostGameReplayStateContext,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PostGameReplayStateContext {
    Match,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PostGameWatcherFolder {
    Known,
    NotFound,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PostGameWatcherReply {
    state: PostGameWatcherState,
    folder: PostGameWatcherFolder,
    /// The legacy replay-library list has no completed-scan timestamp.
    last_scan_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PostGameWatcherState {
    Unknown,
}

impl PostGameWatcherReply {
    fn unknown() -> Self {
        Self {
            state: PostGameWatcherState::Unknown,
            folder: PostGameWatcherFolder::Unknown,
            last_scan_at_ms: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PostGameHeadline {
    deaths: Option<u64>,
    net_worth: Option<u64>,
}

impl PostGameReplayReply {
    fn unavailable(
        record: Option<LastFinishedMatchRecord>,
        parse_state: PostGameParseState,
        watcher: PostGameWatcherReply,
    ) -> Self {
        Self {
            ready: false,
            match_key: record.as_ref().map(|value| value.match_key.clone()),
            bundle_match_id: None,
            session_match_id: None,
            digest: None,
            headline: PostGameHeadline::default(),
            parse_state,
            failure_reason: None,
            last_finished_match: record,
            watcher,
        }
    }
}

/// Only the typed, host-persisted last-finished record can identify the panel's match.
fn recorded_last_finished_match(runtime_state: &Value) -> Option<LastFinishedMatchRecord> {
    let record = serde_json::from_value::<LastFinishedMatchRecord>(
        runtime_state.get("last_finished_match")?.clone(),
    )
    .ok()?;
    record.valid().then_some(record)
}

fn valid_match_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn read_last_finished_match(data_dir: &Path) -> Option<LastFinishedMatchRecord> {
    let path = data_dir.join("runtime-state.json");
    let mut bytes = Vec::with_capacity(MAX_RUNTIME_STATE_BYTES);
    std::fs::File::open(path)
        .ok()?
        .take((MAX_RUNTIME_STATE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_RUNTIME_STATE_BYTES {
        return None;
    }
    let runtime_state = serde_json::from_slice::<Value>(&bytes).ok()?;
    recorded_last_finished_match(&runtime_state)
}

fn safe_nonnegative_integer(value: &Value) -> Option<u64> {
    let number = value.as_f64()?;
    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 || number > MAX_SAFE_INTEGER {
        return None;
    }
    Some(number as u64)
}

/// The own hero is sourced only from this session's local history entry. A missing hero or a
/// bundle without that hero leaves both replay headline values unknown.
fn headline_for(bundle: &ReplayBundle, own_hero: Option<&str>) -> PostGameHeadline {
    let Some(own_hero) = own_hero else {
        return PostGameHeadline::default();
    };
    let Some(heroes) = bundle.match_data.get("heroes").and_then(Value::as_array) else {
        return PostGameHeadline::default();
    };
    let Some(hero) = heroes
        .iter()
        .find(|hero| hero.get("id").and_then(Value::as_str) == Some(own_hero))
    else {
        return PostGameHeadline::default();
    };
    let deaths = bundle
        .match_data
        .get("deaths")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter(|row| row.get("hero").and_then(Value::as_str) == Some(own_hero))
                .count() as u64
        });
    let net_worth = hero
        .get("eco")
        .and_then(Value::as_array)
        .and_then(|samples| samples.last())
        .and_then(Value::as_array)
        .and_then(|sample| sample.get(1))
        .and_then(safe_nonnegative_integer);
    PostGameHeadline { deaths, net_worth }
}

fn parse_state(status: &ReplayStatus) -> PostGameParseState {
    match status {
        ReplayStatus::Available => PostGameParseState::Available,
        ReplayStatus::Queued => PostGameParseState::Queued,
        ReplayStatus::Parsing => PostGameParseState::Parsing,
        ReplayStatus::Ready => PostGameParseState::Ready,
        ReplayStatus::Failed { .. } => PostGameParseState::Failed,
        ReplayStatus::NeedsPack => PostGameParseState::NeedsPack,
        // The bundle was removed by retention; the post-game panel treats it as not available.
        ReplayStatus::Evicted => PostGameParseState::NotFound,
    }
}

fn is_watched_dota_replay(entry: &ReplayLibraryEntry) -> bool {
    if entry.source != crate::replay_library::ReplaySource::DotaReplaysFolder {
        return false;
    }
    let path = Path::new(&entry.source_path);
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dem"))
}

fn is_session_match_id(value: &str) -> bool {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    value
        .parse::<u64>()
        .is_ok_and(|number| number > 0 && number.to_string() == value)
}

fn entry_state(entry: &ReplayLibraryEntry, auto_parse_enabled: bool) -> PostGameParseState {
    match &entry.status {
        ReplayStatus::Available if !auto_parse_enabled => PostGameParseState::AutoParseOff,
        status => parse_state(status),
    }
}

fn fetched_entry_state(
    status: ReplayStatus,
    match_id: &str,
    library: &ReplayLibrary,
) -> PostGameParseState {
    match status {
        ReplayStatus::Available => PostGameParseState::Available,
        ReplayStatus::Queued => PostGameParseState::Queued,
        ReplayStatus::Parsing => PostGameParseState::Parsing,
        ReplayStatus::Ready if library.open_bundle(match_id).is_ok() => PostGameParseState::Ready,
        ReplayStatus::Ready => PostGameParseState::Unknown,
        ReplayStatus::Failed { .. } | ReplayStatus::NeedsPack => PostGameParseState::Failed,
        ReplayStatus::Evicted => PostGameParseState::NotFound,
    }
}

/// Match a watched .dem row to the in-memory session ID inside the host. The renderer receives
/// only two enums. A ready exact match must also pass the existing install-keyed history link and
/// bundle validation; unrelated watched rows never describe this match.
fn lookup_post_game_replay_state(
    session_match_id: Option<&str>,
    record: Option<&LastFinishedMatchRecord>,
    entries: &[ReplayLibraryEntry],
    auto_parse_enabled: bool,
    mut match_key_for: impl FnMut(&str) -> Option<String>,
    mut open_validated_bundle: impl FnMut(&str) -> bool,
) -> PostGameReplayStateReply {
    let matching_entry = session_match_id
        .filter(|match_id| is_session_match_id(match_id))
        .and_then(|match_id| {
            entries
                .iter()
                .filter(|entry| is_watched_dota_replay(entry))
                .filter(|entry| {
                    Path::new(&entry.source_path)
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        == Some(match_id)
                })
                .max_by_key(|entry| entry.created_at_unix_ms)
        });

    if let Some(entry) = matching_entry {
        let state = match &entry.status {
            ReplayStatus::Ready => {
                let history_linked = record.is_some_and(|record| {
                    match_key_for(&entry.match_id).as_deref() == Some(record.match_key.as_str())
                });
                if history_linked && open_validated_bundle(&entry.match_id) {
                    PostGameParseState::Ready
                } else {
                    PostGameParseState::NotFound
                }
            }
            _ => entry_state(entry, auto_parse_enabled),
        };
        return PostGameReplayStateReply {
            state,
            context: PostGameReplayStateContext::Match,
        };
    }

    PostGameReplayStateReply {
        state: PostGameParseState::NotFound,
        context: if session_match_id.is_some_and(is_session_match_id) {
            PostGameReplayStateContext::Match
        } else {
            PostGameReplayStateContext::Unknown
        },
    }
}

/// Map the replay library's watcher state (W2) to the post-game view. No path is returned: only
/// whether the Dota replay folder was found and when the last scan ran.
fn watcher_from_state(state: &crate::replay_library::ReplayWatcherState) -> PostGameWatcherReply {
    let folder = match state.folder_found {
        Some(true) => PostGameWatcherFolder::Known,
        Some(false) => PostGameWatcherFolder::NotFound,
        None => PostGameWatcherFolder::Unknown,
    };
    PostGameWatcherReply {
        state: PostGameWatcherState::Unknown,
        folder,
        last_scan_at_ms: state.last_scan_unix_ms,
    }
}

fn lookup_replay_summary(
    record: LastFinishedMatchRecord,
    watcher: PostGameWatcherReply,
    own_hero: Option<&str>,
    session_match_id: Option<String>,
    entries: &[ReplayLibraryEntry],
    mut match_key_for: impl FnMut(&str) -> Option<String>,
    mut open_validated_bundle: impl FnMut(&str) -> Option<ReplayBundle>,
) -> PostGameReplayReply {
    let match_key = record.match_key.as_str();
    for entry in entries {
        if match_key_for(&entry.match_id).as_deref() != Some(match_key) {
            continue;
        }
        let state = parse_state(&entry.status);
        if state != PostGameParseState::Ready {
            return PostGameReplayReply::unavailable(Some(record), state, watcher)
                .with_session_match_id(session_match_id);
        }
        let Some(bundle) = open_validated_bundle(&entry.match_id) else {
            return PostGameReplayReply::unavailable(
                Some(record),
                PostGameParseState::Unknown,
                watcher,
            )
            .with_session_match_id(session_match_id);
        };
        let Some(digest) = crate::analytics::knowledge::build(&crate::analytics::knowledge::KnowledgeInput {
            manifest: &bundle.manifest,
            match_data: &bundle.match_data,
            world: &bundle.world,
        })
        .ok() else {
            return PostGameReplayReply::unavailable(
                Some(record),
                PostGameParseState::Unknown,
                watcher,
            )
            .with_session_match_id(session_match_id);
        };
        return PostGameReplayReply {
            ready: true,
            match_key: Some(match_key.to_owned()),
            bundle_match_id: Some(entry.match_id.clone()),
            session_match_id,
            digest: Some(digest),
            headline: headline_for(&bundle, own_hero),
            parse_state: PostGameParseState::Ready,
            failure_reason: None,
            last_finished_match: Some(record),
            watcher,
        };
    }
    PostGameReplayReply::unavailable(Some(record), PostGameParseState::NotFound, watcher)
        .with_session_match_id(session_match_id)
}

impl PostGameReplayReply {
    fn with_session_match_id(mut self, match_id: Option<String>) -> Self {
        self.session_match_id = match_id;
        self
    }
}

#[tauri::command(async)]
pub(crate) fn post_game_replay_summary(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    analytics: State<'_, AnalyticsService>,
    library: State<'_, ReplayLibrary>,
    session_epoch: Option<u64>,
) -> Result<PostGameReplayReply, String> {
    if webview.label() != "control" {
        return Err("window_not_allowed".to_owned());
    }
    let Some(record) = read_last_finished_match(&state.settings_dir)
        .filter(|record| session_epoch.is_none_or(|epoch| record.epoch == epoch))
    else {
        return Ok(PostGameReplayReply::unavailable(
            None,
            PostGameParseState::NotFound,
            PostGameWatcherReply::unknown(),
        ));
    };
    let session_match_id =
        crate::intake::lock(&state.runtime.shared).current_match_id_for_epoch(record.epoch);
    let match_key = record.match_key.as_str();
    let history = state.match_history.entries();
    let own_hero = history
        .iter()
        .find(|entry: &&MatchEntry| entry.source == "kohbor" && entry.match_key == match_key)
        .and_then(|entry| entry.hero.as_deref());
    let Ok(list) = library.list() else {
        return Ok(PostGameReplayReply::unavailable(
            Some(record),
            PostGameParseState::Unknown,
            PostGameWatcherReply::unknown(),
        )
        .with_session_match_id(session_match_id));
    };
    Ok(lookup_replay_summary(
        record,
        watcher_from_state(&library.watcher_state()),
        own_hero,
        session_match_id,
        &list.matches,
        |match_id| analytics.store().match_key(match_id).ok(),
        |match_id| library.open_bundle(match_id).ok(),
    ))
}

#[tauri::command(async)]
pub(crate) fn post_game_replay_state(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    analytics: State<'_, AnalyticsService>,
    library: State<'_, ReplayLibrary>,
    session_epoch: Option<u64>,
) -> Result<PostGameReplayStateReply, String> {
    if webview.label() != "control" {
        return Err("window_not_allowed".to_owned());
    }
    let record = read_last_finished_match(&state.settings_dir)
        .filter(|record| session_epoch.is_none_or(|epoch| record.epoch == epoch));
    let session_match_id = record.as_ref().and_then(|record| {
        crate::intake::lock(&state.runtime.shared).current_match_id_for_epoch(record.epoch)
    });
    let entries = match library.watched_entries() {
        Ok(entries) => entries,
        Err(_) => Vec::new(),
    };
    let mut reply = lookup_post_game_replay_state(
        session_match_id.as_deref(),
        record.as_ref(),
        &entries,
        library.auto_analyse_enabled(),
        |match_id| analytics.store().match_key(match_id).ok(),
        |match_id| library.open_bundle(match_id).is_ok(),
    );
    if let Some(match_id) = session_match_id.as_deref().filter(|match_id| {
        !entries.iter().any(|entry| {
            is_watched_dota_replay(entry)
                && Path::new(&entry.source_path)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    == Some(*match_id)
        })
    }) {
        let fetched = library
            .fetched_status(match_id)
            .map(|status| fetched_entry_state(status, match_id, &library))
            .unwrap_or_else(|| state.replay_auto_fetch.state_for(Some(match_id)).into());
        // With the fetch off, the current match still shows the existing download-replay hint.
        if fetched != PostGameParseState::Off || reply.state == PostGameParseState::NotFound {
            reply.state = fetched;
            reply.context = PostGameReplayStateContext::Match;
        }
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay_library::ReplaySource;
    use serde_json::json;

    const SESSION_KEY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OWN_HERO: &str = "npc_dota_hero_axe";

    fn last_match() -> LastFinishedMatchRecord {
        LastFinishedMatchRecord {
            epoch: 7,
            match_key: SESSION_KEY.to_owned(),
            end_values: crate::intake::LastFinishedMatchValues {
                hero_id: Some(OWN_HERO.to_owned()),
                result: None,
                kills: None,
                deaths: None,
                assists: None,
                last_hits: None,
                denies: None,
                gpm: None,
                xpm: None,
                duration_seconds: None,
                radiant_score: None,
                dire_score: None,
                level: None,
                net_worth: None,
                items: None,
                backpack: None,
                neutral_item: None,
                game_mode: None,
                patch: None,
            },
            ended_at: 1_800_000_000,
        }
    }

    fn entry(match_id: &str) -> ReplayLibraryEntry {
        ReplayLibraryEntry {
            match_id: match_id.to_owned(),
            source: ReplaySource::ManualPick,
            source_path: "private-source-path.dem".to_owned(),
            dem_sha256: Some("dem-hash".to_owned()),
            engine_version: Some("engine".to_owned()),
            engine_sha256: Some("engine-hash".to_owned()),
            parse_time_unix_ms: Some(1),
            status: ReplayStatus::Ready,
            bundle_bytes: 1,
            storage_bytes: 1,
            created_at_unix_ms: 1,
            match_date_utc: None,
            own_hero: None,
            duration_s: None,
            patch: None,
            pinned: false,
            auto_queued: false,
        }
    }

    fn watched_entry(
        match_id: &str,
        path: &str,
        status: ReplayStatus,
        created_at_unix_ms: u64,
    ) -> ReplayLibraryEntry {
        ReplayLibraryEntry {
            source: ReplaySource::DotaReplaysFolder,
            source_path: path.to_owned(),
            status,
            created_at_unix_ms,
            ..entry(match_id)
        }
    }

    fn bundle(heroes: Value, deaths: Value) -> ReplayBundle {
        ReplayBundle {
            manifest: json!({
                "schema": "3.1",
                "files": {
                    "match.json": {"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                    "world.json": {"sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
                }
            }),
            match_data: json!({
                "t0": 0,
                "t1": 1200,
                "meta": {"schema":"3.1","duration_s":1200,"patch":"7.42","game_mode":2,"winner":"radiant","draft":[]},
                "heroes": heroes,
                "deaths": deaths
            }),
            world: json!({"heroes":{},"tracks":[],"runes":[]}),
        }
    }

    fn axe_bundle() -> ReplayBundle {
        bundle(
            json!([{
                "id": OWN_HERO,
                "name": "Axe",
                "team": 2,
                "pid": 0,
                "eco": [[500, 1200, 10, 1], [900, 18400, 200, 13]]
            }]),
            json!([
                { "t": 100, "hero": OWN_HERO, "killer": "npc_dota_hero_lion" },
                { "t": 200, "hero": "npc_dota_hero_lion", "killer": OWN_HERO },
                { "t": 300, "hero": OWN_HERO, "killer": "npc_dota_hero_lion" }
            ]),
        )
    }

    #[test]
    fn last_match_record_is_validated_and_survives_phase_changes_in_runtime_state() {
        let runtime = json!({ "last_finished_match": last_match() });
        assert_eq!(recorded_last_finished_match(&runtime), Some(last_match()));
        let mut bad_hash = last_match();
        bad_hash.match_key = "not-a-hash".to_owned();
        assert!(!bad_hash.valid());
    }

    #[test]
    fn lookup_finds_the_ready_bundle_by_its_install_keyed_match_hash() {
        let result = lookup_replay_summary(
            last_match(),
            PostGameWatcherReply::unknown(),
            Some(OWN_HERO),
            Some("8000000001".to_owned()),
            &[entry("123456789")],
            |match_id| (match_id == "123456789").then(|| SESSION_KEY.to_owned()),
            |match_id| (match_id == "123456789").then(axe_bundle),
        );
        assert!(result.ready);
        assert_eq!(result.match_key.as_deref(), Some(SESSION_KEY));
        assert_eq!(result.headline.deaths, Some(2));
        assert_eq!(result.headline.net_worth, Some(18_400));
        assert_eq!(result.parse_state, PostGameParseState::Ready);
        assert_eq!(result.bundle_match_id.as_deref(), Some("123456789"));
        assert_eq!(result.session_match_id.as_deref(), Some("8000000001"));
        assert_eq!(
            result
                .digest
                .as_ref()
                .and_then(|value| value["heroes"].as_array())
                .map(Vec::len),
            Some(1)
        );
        let digest = result.digest.as_ref().unwrap();
        assert_eq!(digest["heroes"][0]["side"], "radiant");
        assert_eq!(digest["heroes"][0]["slot"], 0);
        assert!(digest["heroes"][0].get("lane").is_some());
        assert!(digest["lanes"].is_array());
    }

    #[test]
    fn lookup_without_a_matching_key_is_unavailable() {
        let result = lookup_replay_summary(
            last_match(),
            PostGameWatcherReply::unknown(),
            Some(OWN_HERO),
            Some("8000000001".to_owned()),
            &[entry("987654321")],
            |_| Some("b".repeat(64)),
            |_| Some(axe_bundle()),
        );
        assert!(!result.ready);
        assert_eq!(result.match_key.as_deref(), Some(SESSION_KEY));
        assert_eq!(result.headline, PostGameHeadline::default());
        assert_eq!(result.parse_state, PostGameParseState::NotFound);
        assert_eq!(result.session_match_id.as_deref(), Some("8000000001"));
    }

    #[test]
    fn a_ready_bundle_without_the_history_hero_keeps_headline_values_unknown() {
        let other_hero_bundle = bundle(
            json!([{
                "id": "npc_dota_hero_lion",
                "name": "Lion",
                "team": 2,
                "pid": 0,
                "eco": [[400, 7000, 80, 5]]
            }]),
            json!([{ "t": 10, "hero": "npc_dota_hero_lion", "killer": "npc_dota_neutral_centaur_khan" }]),
        );
        let result = lookup_replay_summary(
            last_match(),
            PostGameWatcherReply::unknown(),
            Some(OWN_HERO),
            None,
            &[entry("123456789")],
            |_| Some(SESSION_KEY.to_owned()),
            |_| Some(other_hero_bundle.clone()),
        );
        assert!(result.ready);
        assert_eq!(result.headline, PostGameHeadline::default());
    }

    #[test]
    fn reply_exposes_raw_match_id_only_from_the_linked_validated_bundle() {
        let raw_match_id = "123456789";
        let result = lookup_replay_summary(
            last_match(),
            PostGameWatcherReply::unknown(),
            Some(OWN_HERO),
            None,
            &[entry(raw_match_id)],
            |_| Some(SESSION_KEY.to_owned()),
            |_| Some(axe_bundle()),
        );
        let serialized = serde_json::to_string(&result).unwrap();
        assert_eq!(result.bundle_match_id.as_deref(), Some(raw_match_id));
        assert!(serialized.contains(raw_match_id));
        assert!(!serialized.contains("private-source-path.dem"));
        assert!(serialized.contains(SESSION_KEY));
    }

    #[test]
    fn synthetic_replay_state_lookup_maps_each_watched_state_and_auto_parse_setting() {
        let statuses = [
            (ReplayStatus::Available, PostGameParseState::Available),
            (ReplayStatus::Queued, PostGameParseState::Queued),
            (ReplayStatus::Parsing, PostGameParseState::Parsing),
            (ReplayStatus::Ready, PostGameParseState::Ready),
            (
                ReplayStatus::Failed {
                    reason: "synthetic".to_owned(),
                },
                PostGameParseState::Failed,
            ),
            (ReplayStatus::NeedsPack, PostGameParseState::NeedsPack),
            (ReplayStatus::Evicted, PostGameParseState::NotFound),
        ];
        for (status, expected) in statuses {
            let row = watched_entry("123456789", "D:/synthetic/123456789.dem", status, 10);
            let state = lookup_post_game_replay_state(
                Some("123456789"),
                Some(&last_match()),
                &[row],
                true,
                |_| Some(SESSION_KEY.to_owned()),
                |_| true,
            );
            assert_eq!(state.state, expected);
            assert_eq!(state.context, PostGameReplayStateContext::Match);
        }
        let row = watched_entry(
            "123456789",
            "D:/synthetic/123456789.dem",
            ReplayStatus::Available,
            10,
        );
        assert_eq!(
            lookup_post_game_replay_state(
                Some("123456789"),
                Some(&last_match()),
                &[row],
                false,
                |_| None,
                |_| false,
            )
            .state,
            PostGameParseState::AutoParseOff
        );
    }

    #[test]
    fn synthetic_replay_state_ignores_other_matches_when_the_current_match_cannot_be_matched() {
        let entries = [
            watched_entry("1", "D:/synthetic/1.dem", ReplayStatus::Queued, 1),
            watched_entry("2", "D:/synthetic/2.dem", ReplayStatus::Parsing, 2),
        ];
        let state = lookup_post_game_replay_state(None, None, &entries, true, |_| None, |_| false);
        assert_eq!(state.state, PostGameParseState::NotFound);
        assert_eq!(state.context, PostGameReplayStateContext::Unknown);
    }

    #[test]
    fn synthetic_replay_state_reply_never_contains_the_session_id_or_replay_path() {
        let session_id = "8000000001";
        let path = "D:/private-user-data/8000000001.dem";
        let row = watched_entry(session_id, path, ReplayStatus::Parsing, 10);
        let reply = lookup_post_game_replay_state(
            Some(session_id),
            Some(&last_match()),
            &[row],
            true,
            |_| None,
            |_| false,
        );
        let serialized = serde_json::to_string(&reply).unwrap();
        let diagnostic = format!("{reply:?}");
        assert_eq!(reply.state, PostGameParseState::Parsing);
        assert_eq!(reply.context, PostGameReplayStateContext::Match);
        for output in [serialized, diagnostic] {
            assert!(!output.contains(session_id));
            assert!(!output.contains(path));
        }
        // This pure lookup has no logging dependency or log call; only its enum reply escapes.
    }

    #[test]
    fn replay_fetch_state_reply_is_enum_only_for_every_fetch_state() {
        let states = [
            (PostGameParseState::Off, "off"),
            (
                PostGameParseState::WaitingForMatchEnd,
                "waiting_for_match_end",
            ),
            (PostGameParseState::LookingUp, "looking_up"),
            (PostGameParseState::Requested, "requested"),
            (PostGameParseState::Downloading, "downloading"),
            (PostGameParseState::Parsing, "parsing"),
            (PostGameParseState::Ready, "ready"),
            (PostGameParseState::NotAvailableYet, "not_available_yet"),
            (PostGameParseState::NotFound, "not_found"),
            (PostGameParseState::Expired, "expired"),
            (PostGameParseState::RateLimited, "rate_limited"),
            (PostGameParseState::Offline, "offline"),
            (PostGameParseState::Failed, "failed"),
        ];
        for (state, expected) in states {
            let reply = PostGameReplayStateReply {
                state,
                context: PostGameReplayStateContext::Match,
            };
            let serialized = serde_json::to_string(&reply).unwrap();
            assert!(serialized.contains(&format!("\"state\":\"{expected}\"")));
            assert!(serialized.contains("\"context\":\"match\""));
            for private in ["9001", "replay45.valve.net", "777", "C:\\\\private", ".dem"] {
                assert!(!serialized.contains(private), "{expected}: {private}");
            }
        }
    }

    #[test]
    fn synthetic_ready_replay_state_requires_the_history_hash_and_validated_bundle_even_when_own_hero_is_null(
    ) {
        let mut ready = watched_entry(
            "123456789",
            "D:/synthetic/123456789.dem",
            ReplayStatus::Ready,
            10,
        );
        ready.own_hero = None;
        let state = lookup_post_game_replay_state(
            Some("123456789"),
            Some(&last_match()),
            &[ready.clone()],
            true,
            |_| Some(SESSION_KEY.to_owned()),
            |_| true,
        );
        assert_eq!(state.state, PostGameParseState::Ready);
        assert_eq!(state.context, PostGameReplayStateContext::Match);
        let unlinked = lookup_post_game_replay_state(
            Some("123456789"),
            Some(&last_match()),
            &[ready],
            true,
            |_| Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned()),
            |_| true,
        );
        assert_eq!(unlinked.state, PostGameParseState::NotFound);
        assert_eq!(unlinked.context, PostGameReplayStateContext::Match);
    }
}
