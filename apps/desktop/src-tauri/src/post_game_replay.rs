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
    let Some(hero) = heroes.iter().find(|hero| {
        hero.get("id").and_then(Value::as_str) == Some(own_hero)
    }) else {
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
            return PostGameReplayReply::unavailable(Some(record), PostGameParseState::Unknown, watcher)
                .with_session_match_id(session_match_id);
        };
        return PostGameReplayReply {
            ready: true,
            match_key: Some(match_key.to_owned()),
            bundle_match_id: Some(entry.match_id.clone()),
            session_match_id,
            digest: Some(bundle.match_data.clone()),
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
        return Ok(PostGameReplayReply::unavailable(None, PostGameParseState::NotFound, PostGameWatcherReply::unknown()));
    };
    let session_match_id = crate::intake::lock(&state.runtime.shared)
        .current_match_id_for_epoch(record.epoch);
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
        ).with_session_match_id(session_match_id));
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

    fn bundle(heroes: Value, deaths: Value) -> ReplayBundle {
        ReplayBundle {
            manifest: json!({}),
            match_data: json!({ "heroes": heroes, "deaths": deaths }),
            world: json!({}),
        }
    }

    fn axe_bundle() -> ReplayBundle {
        bundle(
            json!([{
                "id": OWN_HERO,
                "eco": [[500, 1200, 10, 1], [900, 18400, 200, 13]]
            }]),
            json!([
                { "hero": OWN_HERO },
                { "hero": "npc_dota_hero_lion" },
                { "hero": OWN_HERO }
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
        assert_eq!(result.digest.as_ref().and_then(|value| value["heroes"].as_array()).map(Vec::len), Some(1));
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
                "eco": [[400, 7000, 80, 5]]
            }]),
            json!([{ "hero": "npc_dota_hero_lion" }]),
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
}
