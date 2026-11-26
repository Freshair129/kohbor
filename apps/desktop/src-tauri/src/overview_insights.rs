//! Read-only, privacy-minimized lane facts for the Overview's local nemesis summary.
//!
//! Only Ready replay entries with a validated current bundle hash and a cached digest for that
//! exact bundle are considered. The projection deliberately discards names and identifiers.

use crate::analytics::AnalyticsStore;
use crate::match_history::MatchEntry;
use crate::replay_library::{ReplayLibrary, ReplayStatus};
use serde_json::{json, Value};

const LANE_NAMES: &[&str] = &["top", "mid", "bot"];
const LANE_OUTCOMES: &[&str] = &[
    "even",
    "radiant_won",
    "dire_won",
    "radiant_stomp",
    "dire_stomp",
];

fn valid_hero_id(value: &str) -> bool {
    value
        .strip_prefix("npc_dota_hero_")
        .is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix.len() <= 48
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
}

fn hero_ids(raw: Option<&Value>) -> Vec<String> {
    let mut heroes = Vec::new();
    for value in raw.and_then(Value::as_array).into_iter().flatten() {
        let Some(hero) = value.get("hero").and_then(Value::as_str) else {
            continue;
        };
        if valid_hero_id(hero) && !heroes.iter().any(|existing| existing == hero) {
            heroes.push(hero.to_owned());
        }
    }
    heroes
}

fn project_lanes(digest: &Value) -> Vec<Value> {
    digest
        .get("lanes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|lane| {
            let lane_name = lane.get("lane").and_then(Value::as_str)?;
            if !LANE_NAMES.contains(&lane_name) {
                return None;
            }
            let radiant = hero_ids(lane.get("radiant"));
            let dire = hero_ids(lane.get("dire"));
            let outcome = lane
                .get("outcome")
                .and_then(Value::as_str)
                .filter(|value| LANE_OUTCOMES.contains(value));
            Some(json!({
                "lane": lane_name,
                "radiant": radiant,
                "dire": dire,
                "outcome": outcome,
            }))
        })
        .collect()
}

fn valid_own_hero(value: Option<String>) -> Option<String> {
    value.filter(|hero| valid_hero_id(hero))
}

/// Load exact-bundle knowledge digests and return only the own hero plus lane assignments/outcomes.
pub(crate) fn local_lane_samples(
    library: &ReplayLibrary,
    store: &AnalyticsStore,
    history: &[MatchEntry],
) -> Result<Value, &'static str> {
    let library_list = library.list()?;
    let serialized = serde_json::to_value(&library_list).map_err(|_| "library_serialization_failed")?;
    let views = serialized
        .get("matches")
        .and_then(Value::as_array)
        .ok_or("library_serialization_failed")?;
    let mut matches = Vec::new();

    for (entry, view) in library_list.matches.iter().zip(views) {
        if entry.status != ReplayStatus::Ready {
            continue;
        }
        let Some(bundle_hash) = view.get("bundle_hash").and_then(Value::as_str) else {
            continue;
        };
        let local_hero = store
            .match_key(&entry.match_id)
            .ok()
            .and_then(|key| history.iter().find(|item| item.match_key == key))
            .and_then(|item| item.hero.clone());
        let own_hero = valid_own_hero(local_hero)
            .or_else(|| valid_own_hero(entry.own_hero.clone()))
            .or_else(|| valid_own_hero(store.own_hero(&entry.match_id)));
        let Some(own_hero) = own_hero else {
            continue;
        };
        let Ok(Some((_meta, digest))) = store.read_knowledge(&entry.match_id, bundle_hash) else {
            // A missing, stale or unreadable digest is unknown evidence, not a failed lane.
            continue;
        };
        let lanes = project_lanes(&digest);
        if lanes.is_empty() {
            continue;
        }
        matches.push(json!({ "own_hero": own_hero, "lanes": lanes }));
    }

    Ok(json!({ "schema_version": "1.0.0", "matches": matches }))
}

#[cfg(test)]
mod tests {
    use super::project_lanes;
    use serde_json::json;

    #[test]
    fn lane_projection_discards_player_names_and_account_identifiers() {
        let digest = json!({
            "lanes": [{
                "lane": "top",
                "radiant": [
                    {"hero": "npc_dota_hero_lion", "name": "private player", "account_id": 123456},
                    {"hero": "not_a_hero", "name": "must be dropped"}
                ],
                "dire": [{"hero": "npc_dota_hero_sniper", "name": "another private player"}],
                "outcome": "dire_won",
                "player_names": ["never project this"],
                "steam_id": "76561198000000000"
            }]
        });
        let projected = json!({
            "own_hero": "npc_dota_hero_lion",
            "lanes": project_lanes(&digest)
        });
        let text = projected.to_string();
        assert_eq!(projected["lanes"][0]["radiant"][0], "npc_dota_hero_lion");
        assert_eq!(projected["lanes"][0]["outcome"], "dire_won");
        assert!(!text.contains("private player"));
        assert!(!text.contains("account_id"));
        assert!(!text.contains("steam_id"));
        assert!(!text.contains("player_names"));
    }

    #[test]
    fn lane_projection_preserves_unknown_outcomes_as_unknown() {
        let digest = json!({
            "lanes": [{
                "lane": "mid",
                "radiant": [{"hero": "npc_dota_hero_lion"}],
                "dire": [{"hero": "npc_dota_hero_axe"}],
                "outcome": null
            }]
        });
        assert!(project_lanes(&digest)[0]["outcome"].is_null());
    }
}
