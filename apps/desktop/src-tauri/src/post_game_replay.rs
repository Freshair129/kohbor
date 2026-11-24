//! Read-only post-game replay lookup keyed by the active host session record.

use crate::analytics::AnalyticsService;
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
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PostGameHeadline {
    deaths: Option<u64>,
    net_worth: Option<u64>,
}

impl PostGameReplayReply {
    fn unavailable(match_key: Option<String>) -> Self {
        Self {
            ready: false,
            match_key,
            headline: PostGameHeadline::default(),
        }
    }
}

/// Only use a match key recorded by the host for the exact session epoch shown by the panel.
fn recorded_match_key(runtime_state: &Value, expected_epoch: u64) -> Option<String> {
    let sessions = runtime_state.get("sessions")?;
    if sessions.get("schema_version")?.as_str()? != "1.0.0" {
        return None;
    }
    let current = sessions.get("current_session")?;
    if current.get("epoch")?.as_u64()? != expected_epoch {
        return None;
    }
    let key = current.get("match_key")?.as_str()?;
    valid_match_key(key).then(|| key.to_owned())
}

fn valid_match_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn read_recorded_match_key(data_dir: &Path, expected_epoch: u64) -> Option<String> {
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
    recorded_match_key(&runtime_state, expected_epoch)
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

fn lookup_replay_summary(
    match_key: &str,
    own_hero: Option<&str>,
    entries: &[ReplayLibraryEntry],
    mut match_key_for: impl FnMut(&str) -> Option<String>,
    mut open_validated_bundle: impl FnMut(&str) -> Option<ReplayBundle>,
) -> PostGameReplayReply {
    let unavailable = || PostGameReplayReply::unavailable(Some(match_key.to_owned()));
    for entry in entries {
        if match_key_for(&entry.match_id).as_deref() != Some(match_key) {
            continue;
        }
        if entry.status != ReplayStatus::Ready {
            continue;
        }
        let Some(bundle) = open_validated_bundle(&entry.match_id) else {
            continue;
        };
        return PostGameReplayReply {
            ready: true,
            match_key: Some(match_key.to_owned()),
            headline: headline_for(&bundle, own_hero),
        };
    }
    unavailable()
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
    let Some(session_epoch) = session_epoch else {
        return Ok(PostGameReplayReply::unavailable(None));
    };
    let Some(match_key) = read_recorded_match_key(&state.settings_dir, session_epoch) else {
        return Ok(PostGameReplayReply::unavailable(None));
    };
    let history = state.match_history.entries();
    let own_hero = history
        .iter()
        .find(|entry: &&MatchEntry| entry.source == "kohbor" && entry.match_key == match_key)
        .and_then(|entry| entry.hero.as_deref());
    let Ok(list) = library.list() else {
        return Ok(PostGameReplayReply::unavailable(Some(match_key)));
    };
    Ok(lookup_replay_summary(
        &match_key,
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
    fn session_key_is_read_only_for_the_matching_host_epoch() {
        let record = json!({
            "sessions": {
                "schema_version": "1.0.0",
                "current_session": { "epoch": 7, "match_key": SESSION_KEY }
            }
        });
        assert_eq!(recorded_match_key(&record, 7).as_deref(), Some(SESSION_KEY));
        assert_eq!(recorded_match_key(&record, 8), None);
    }

    #[test]
    fn lookup_finds_the_ready_bundle_by_its_install_keyed_match_hash() {
        let result = lookup_replay_summary(
            SESSION_KEY,
            Some(OWN_HERO),
            &[entry("123456789")],
            |match_id| (match_id == "123456789").then(|| SESSION_KEY.to_owned()),
            |match_id| (match_id == "123456789").then(axe_bundle),
        );
        assert!(result.ready);
        assert_eq!(result.match_key.as_deref(), Some(SESSION_KEY));
        assert_eq!(result.headline.deaths, Some(2));
        assert_eq!(result.headline.net_worth, Some(18_400));
    }

    #[test]
    fn lookup_without_a_matching_key_is_unavailable() {
        let result = lookup_replay_summary(
            SESSION_KEY,
            Some(OWN_HERO),
            &[entry("987654321")],
            |_| Some("b".repeat(64)),
            |_| Some(axe_bundle()),
        );
        assert!(!result.ready);
        assert_eq!(result.match_key.as_deref(), Some(SESSION_KEY));
        assert_eq!(result.headline, PostGameHeadline::default());
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
            SESSION_KEY,
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
            SESSION_KEY,
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
