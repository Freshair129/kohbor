use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use crate::match_history::MatchEntry;

pub(crate) const MAX_CONTEXT_BYTES: usize = 150 * 1024;
const UNKNOWN: &str = "— ไม่ทราบ";

#[derive(Clone, Debug)]
pub(crate) struct MatchContextInput<'a> {
    pub(crate) manifest: &'a Value,
    pub(crate) match_data: &'a Value,
    pub(crate) world: &'a Value,
    pub(crate) own_hero: Option<&'a str>,
    pub(crate) gsi_match: Option<&'a MatchEntry>,
}

fn finite_num(value: &Value) -> Option<f64> { value.as_f64().filter(|value| value.is_finite()) }
fn field<'a>(object: &'a Value, key: &str) -> &'a Value { object.get(key).unwrap_or(&Value::Null) }

fn safe_hero_table(heroes: &[Value]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for hero in heroes {
        if let (Some(id), Some(name)) = (hero.get("id").and_then(Value::as_str), hero.get("name").and_then(Value::as_str)) {
            if id.starts_with("npc_dota_hero_") && id.len() <= 64 && name.len() <= 80 {
                map.insert(id.to_owned(), name.to_owned());
            }
        }
    }
    map
}

fn death_position(world: &Value, hero_id: &str, time: f64) -> Value {
    let Some(track) = world.get("heroes").and_then(|heroes| heroes.get(hero_id)) else { return json!(UNKNOWN); };
    let (Some(times), Some(xs), Some(ys)) = (track.get("t").and_then(Value::as_array), track.get("x").and_then(Value::as_array), track.get("y").and_then(Value::as_array)) else { return json!(UNKNOWN); };
    let nearest = times.iter().enumerate().filter_map(|(index, value)| finite_num(value).map(|candidate| (index, (candidate - time).abs())))
        .min_by(|left, right| left.1.total_cmp(&right.1)).map(|(index, _)| index);
    nearest.and_then(|index| Some(json!([finite_num(xs.get(index)?).unwrap_or(f64::NAN), finite_num(ys.get(index)?).unwrap_or(f64::NAN)])))
        .filter(|value| !value.is_null() && value.as_array().is_some_and(|values| values.iter().all(|value| value.as_f64().is_some_and(f64::is_finite))))
        .unwrap_or_else(|| json!(UNKNOWN))
}

fn build_projection(input: &MatchContextInput<'_>) -> Value {
    let heroes = field(input.match_data, "heroes").as_array().cloned().unwrap_or_default();
    let names = safe_hero_table(&heroes);
    let mut kills: BTreeMap<String, u64> = BTreeMap::new();
    let mut deaths: BTreeMap<String, u64> = BTreeMap::new();
    let raw_deaths = field(input.match_data, "deaths").as_array().cloned().unwrap_or_default();
    for death in &raw_deaths {
        if let Some(victim) = death.get("hero").and_then(Value::as_str) { *deaths.entry(victim.to_owned()).or_default() += 1; }
        if let Some(killer) = death.get("killer").and_then(Value::as_str).filter(|value| names.contains_key(*value)) { *kills.entry(killer.to_owned()).or_default() += 1; }
    }
    let lineups = [2_i64, 3_i64].map(|team| {
        let entries = heroes.iter().filter(|hero| hero.get("team").and_then(Value::as_i64) == Some(team)).map(|hero| {
            let id = hero.get("id").and_then(Value::as_str).unwrap_or(UNKNOWN);
            let name = hero.get("name").and_then(Value::as_str).unwrap_or(UNKNOWN);
            let mut row = Map::new();
            row.insert("hero".into(), json!(name));
            row.insert("id".into(), json!(id));
            row.insert("hero_kills".into(), json!(kills.get(id).copied().unwrap_or(0)));
            row.insert("deaths".into(), json!(deaths.get(id).copied().unwrap_or(0)));
            row.insert("net_worth_series".into(), hero.get("eco").cloned().unwrap_or_else(|| json!(UNKNOWN)));
            row.insert("items".into(), hero.get("inv").cloned().unwrap_or_else(|| json!(UNKNOWN)));
            row.insert("item_purchases".into(), hero.get("buys").cloned().unwrap_or_else(|| json!(UNKNOWN)));
            Value::Object(row)
        }).collect::<Vec<_>>();
        (if team == 2 { "radiant" } else { "dire" }, Value::Array(entries))
    });
    let mut kill_counts = Map::new();
    for (team, entries) in &lineups {
        let total = entries.as_array().into_iter().flatten().map(|hero| hero.get("hero_kills").and_then(Value::as_u64).unwrap_or(0)).sum::<u64>();
        kill_counts.insert((*team).into(), json!(total));
    }
    let events = raw_deaths.iter().enumerate().map(|(index, death)| {
        let victim_id = death.get("hero").and_then(Value::as_str).unwrap_or(UNKNOWN);
        let killer_id = death.get("killer").and_then(Value::as_str).unwrap_or(UNKNOWN);
        let time = death.get("t").and_then(finite_num);
        let victim = names.get(victim_id).map(String::as_str).unwrap_or(UNKNOWN);
        let killer = names.get(killer_id).map(String::as_str).unwrap_or(UNKNOWN);
        let mut row = Map::new();
        row.insert("event_ref".into(), json!(format!("deaths/{index}")));
        row.insert("time_s".into(), time.map_or_else(|| json!(UNKNOWN), |value| json!(value)));
        row.insert("victim_hero".into(), json!(victim));
        row.insert("killer_hero".into(), json!(killer));
        row.insert("position".into(), time.map_or_else(|| json!(UNKNOWN), |value| death_position(input.world, victim_id, value)));
        Value::Object(row)
    }).collect::<Vec<_>>();
    let meta = field(input.match_data, "meta");
    let mut root = Map::new();
    root.insert("duration_s".into(), meta.get("duration_s").cloned().filter(|v| finite_num(v).is_some()).unwrap_or_else(|| json!(UNKNOWN)));
    root.insert("patch".into(), meta.get("patch").cloned().filter(|v| v.is_string()).unwrap_or_else(|| json!(UNKNOWN)));
    root.insert("own_hero".into(), input.own_hero.and_then(|hero| names.get(hero)).map_or_else(|| json!(UNKNOWN), |hero| json!(hero)));
    root.insert("local_history".into(), input.gsi_match.map_or_else(|| json!(UNKNOWN), |entry| json!({
        "result": entry.result.map(|result| match result { crate::match_history::MatchResult::Win => "win", crate::match_history::MatchResult::Loss => "loss" }).unwrap_or(UNKNOWN),
        "kills": entry.kills.map_or_else(|| json!(UNKNOWN), |value| json!(value)),
        "deaths": entry.deaths.map_or_else(|| json!(UNKNOWN), |value| json!(value)),
        "assists": entry.assists.map_or_else(|| json!(UNKNOWN), |value| json!(value)),
        "duration_s": entry.duration_s.map_or_else(|| json!(UNKNOWN), |value| json!(value))
    })));
    root.insert("heroes_per_side".into(), Value::Object(lineups.into_iter().map(|(team, heroes)| (team.to_owned(), heroes)).collect()));
    root.insert("hero_kill_counts".into(), Value::Object(kill_counts));
    root.insert("deaths".into(), Value::Array(events));
    root.insert("context_note".into(), json!("Kills count hero-kill events only. Full movement tracks and non-hero world tracks are omitted; use local Replay tools for them."));
    let _ = input.manifest; // Do not copy manifest identifiers, paths, hashes or timestamps to the cloud projection.
    Value::Object(root)
}

/// Deterministic compact context. Data is assembled from an allow-list; bundle identifiers, player
/// names, account IDs, file names, hashes and timestamps cannot enter the projection.
pub(crate) fn build_match_context(input: &MatchContextInput<'_>) -> Result<String, &'static str> {
    let mut projection = build_projection(input);
    let bytes = serde_json::to_vec(&projection).map_err(|_| "context_serialization_failed")?;
    if bytes.len() <= MAX_CONTEXT_BYTES { return String::from_utf8(bytes).map_err(|_| "context_serialization_failed"); }
    let root = projection.as_object_mut().ok_or("context_serialization_failed")?;
    let mut omitted = Vec::new();
    if let Some(lineups) = root.get_mut("heroes_per_side").and_then(Value::as_object_mut) {
        for heroes in lineups.values_mut().filter_map(Value::as_array_mut) {
            for hero in heroes {
                if let Some(row) = hero.as_object_mut() { if row.remove("net_worth_series").is_some() { omitted.push("full net-worth series".to_owned()); } }
            }
        }
    }
    if let Some(lineups) = root.get_mut("heroes_per_side").and_then(Value::as_object_mut) {
        for heroes in lineups.values_mut().filter_map(Value::as_array_mut) {
            for hero in heroes {
                if let Some(row) = hero.as_object_mut() {
                    if row.remove("item_purchases").is_some() { omitted.push("item purchase timeline".to_owned()); }
                    if row.remove("items").is_some() { omitted.push("inventory timeline".to_owned()); }
                }
            }
        }
    }
    let mut omitted_deaths = 0_usize;
    root.insert("omitted_for_context_cap".into(), json!(omitted));
    let mut bytes = serde_json::to_vec(&Value::Object(root.clone())).map_err(|_| "context_serialization_failed")?;
    while bytes.len() > MAX_CONTEXT_BYTES {
        let total = root.get("deaths").and_then(Value::as_array).map_or(0, Vec::len);
        if total == 0 { return Err("context_exceeds_limit"); }
        let excess = bytes.len() - MAX_CONTEXT_BYTES;
        let remove_count = (excess / 160).max(1).min(total);
        if let Some(events) = root.get_mut("deaths").and_then(Value::as_array_mut) {
            events.truncate(total - remove_count);
        }
        omitted_deaths = omitted_deaths.saturating_add(remove_count);
        if let Some(items) = root.get_mut("omitted_for_context_cap").and_then(Value::as_array_mut) {
            if !items.iter().any(|item| item.as_str() == Some("later death events")) { items.push(json!("later death events")); }
        }
        root.insert("omitted_death_events".into(), json!(omitted_deaths));
        bytes = serde_json::to_vec(&Value::Object(root.clone())).map_err(|_| "context_serialization_failed")?;
    }
    String::from_utf8(bytes).map_err(|_| "context_serialization_failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample() -> (Value, Value, Value) {
        let manifest = json!({"match_id": 7654321, "dem_file": "C:/private/123.dem", "timestamp": "2026-11-24"});
        let match_data = json!({"meta":{"match_id":7654321,"duration_s":10,"patch":"7.41f","player_name":"Sensitive"},"names":{"1":"Steam user"},"heroes":[
          {"id":"npc_dota_hero_axe","name":"Axe","team":2,"eco":[[0,100,0,0]],"inv":["blink"],"buys":[[1,"blink"]]},
          {"id":"npc_dota_hero_lina","name":"Lina","team":3,"eco":[[0,200,0,0]],"inv":[],"buys":[]}],
          "deaths":[{"t":3,"hero":"npc_dota_hero_axe","killer":"npc_dota_hero_lina"}]});
        let world = json!({"heroes":{"npc_dota_hero_axe":{"t":[2.0,3.0],"x":[4.0,5.0],"y":[6.0,7.0]}}});
        (manifest, match_data, world)
    }

    #[test]
    fn context_serializes_byte_identically_and_excludes_identifiers_and_paths() {
        let (manifest, match_data, world) = sample();
        let input = MatchContextInput { manifest: &manifest, match_data: &match_data, world: &world, own_hero: None, gsi_match: None };
        let first = build_match_context(&input).unwrap(); let second = build_match_context(&input).unwrap();
        assert_eq!(first.as_bytes(), second.as_bytes());
        for forbidden in ["7654321", "Steam user", "Sensitive", "C:/private", "dem_file", "timestamp"] { assert!(!first.contains(forbidden)); }
        assert!(first.contains("Axe")); assert!(first.contains("deaths/0")); assert!(first.contains("Axe"));
    }

    #[test]
    fn context_cap_reports_every_dropped_series() {
        let (manifest, mut match_data, world) = sample();
        let huge = "x".repeat(MAX_CONTEXT_BYTES);
        match_data["heroes"][0]["eco"] = json!([huge]);
        let context = build_match_context(&MatchContextInput { manifest: &manifest, match_data: &match_data, world: &world, own_hero: None, gsi_match: None }).unwrap();
        assert!(context.len() <= MAX_CONTEXT_BYTES);
        assert!(context.contains("omitted_for_context_cap"));
        assert!(context.contains("full net-worth series"));
    }

    #[test]
    fn optional_real_bundle_context_sizes() {
        let Ok(root) = std::env::var("KOHBOR_ANALYTICS_REAL_BUNDLE_ROOT") else { return; };
        let mut sizes = Vec::new();
        for item in fs::read_dir(root).expect("real bundle root should be readable") {
            let directory = item.expect("bundle entry should be readable").path();
            if !directory.is_dir() { continue; }
            let read = |name: &str| -> Option<Value> {
                let bytes = fs::read(directory.join(name)).ok()?;
                serde_json::from_slice(&bytes).ok()
            };
            let (Some(manifest), Some(match_data), Some(world)) =
                (read("manifest.json"), read("match.json"), read("world.json")) else { continue; };
            let context = build_match_context(&MatchContextInput {
                manifest: &manifest,
                match_data: &match_data,
                world: &world,
                own_hero: None,
                gsi_match: None,
            }).expect("real bundle should produce bounded context");
            sizes.push(context.len());
        }
        assert!(!sizes.is_empty(), "the selected replay library should contain at least one ready bundle");
        sizes.sort_unstable();
        println!("real bundle contexts: count={} bytes_sorted={sizes:?} max_bytes={MAX_CONTEXT_BYTES}", sizes.len());
    }
}
