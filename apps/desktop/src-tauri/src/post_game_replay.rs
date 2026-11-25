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
    }
}

/// Adapt the legacy list without returning its watched path to the renderer. It proves only that
/// the expected Dota folder location can be derived; it does not report a scan or file discovery.
fn watcher_from_library_list(list: &crate::replay_library::ReplayLibraryList) -> PostGameWatcherReply {
    let folder = match list.watched_replay_folder_path.as_deref() {
        Some(path) if !path.trim().is_empty() => PostGameWatcherFolder::Known,
        Some(_) => PostGameWatcherFolder::Unknown,
        None => PostGameWatcherFolder::NotFound,
    };
    PostGameWatcherReply {
        state: PostGameWatcherState::Unknown,
        folder,
        last_scan_at_ms: None,
    }
}

fn lookup_replay_summary(
    record: LastFinishedMatchRecord,
    watcher: PostGameWatcherReply,
    own_hero: Option<&str>,
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
            return PostGameReplayReply::unavailable(Some(record), state, watcher);
        }
        let Some(bundle) = open_validated_bundle(&entry.match_id) else {
            return PostGameReplayReply::unavailable(Some(record), PostGameParseState::Unknown, watcher);
        };
        return PostGameReplayReply {
            ready: true,
            match_key: Some(match_key.to_owned()),
            headline: headline_for(&bundle, own_hero),
            parse_state: PostGameParseState::Ready,
            failure_reason: None,
            last_finished_match: Some(record),
            watcher,
        };
    }
    PostGameReplayReply::unavailable(Some(record), PostGameParseState::NotFound, watcher)
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
        ));
    };
    Ok(lookup_replay_summary(
        record,
        watcher_from_library_list(&list),
        own_hero,
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
            &[entry("123456789")],
            |match_id| (match_id == "123456789").then(|| SESSION_KEY.to_owned()),
            |match_id| (match_id == "123456789").then(axe_bundle),
        );
        assert!(result.ready);
        assert_eq!(result.match_key.as_deref(), Some(SESSION_KEY));
        assert_eq!(result.headline.deaths, Some(2));
        assert_eq!(result.headline.net_worth, Some(18_400));
        assert_eq!(result.parse_state, PostGameParseState::Ready);
    }

    #[test]
    fn lookup_without_a_matching_key_is_unavailable() {
        let result = lookup_replay_summary(
            last_match(),
            PostGameWatcherReply::unknown(),
            Some(OWN_HERO),
            &[entry("987654321")],
            |_| Some("b".repeat(64)),
            |_| Some(axe_bundle()),
        );
        assert!(!result.ready);
        assert_eq!(result.match_key.as_deref(), Some(SESSION_KEY));
        assert_eq!(result.headline, PostGameHeadline::default());
        assert_eq!(result.parse_state, PostGameParseState::NotFound);
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
            &[entry("123456789")],
            |_| Some(SESSION_KEY.to_owned()),
            |_| Some(other_hero_bundle.clone()),
        );
        assert!(result.ready);
        assert_eq!(result.headline, PostGameHeadline::default());
    }

    #[test]
    fn reply_never_contains_the_raw_match_id_or_source_path() {
        let raw_match_id = "123456789";
        let result = lookup_replay_summary(
            last_match(),
            PostGameWatcherReply::unknown(),
            Some(OWN_HERO),
            &[entry(raw_match_id)],
            |_| Some(SESSION_KEY.to_owned()),
            |_| Some(axe_bundle()),
        );
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains(raw_match_id));
        assert!(!serialized.contains("private-source-path.dem"));
        assert!(serialized.contains(SESSION_KEY));
    }
}
