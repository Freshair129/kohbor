use super::lane_expectation;
use super::lane_history::LaneHistoryRecord;
use super::jungle_zones::{zone_for_camp, zone_names};
use crate::match_history::{MatchEntry, MatchResult};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const KNOWLEDGE_VERSION: u32 = 6;
pub(super) const ENGINE_REPARSE_REASON: &str = "re-parse with engine 0.3.0 to get this";

pub(crate) struct KnowledgeInput<'a> {
    pub(crate) manifest: &'a Value,
    pub(crate) match_data: &'a Value,
    pub(crate) world: &'a Value,
}

#[derive(Clone)]
struct HeroInfo<'a> {
    source: &'a Value,
    id: String,
    name: String,
    side: &'static str,
    slot: u64,
    lane: Option<&'static str>,
    lane_share: Option<f64>,
}

#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone)]
struct DeathSample {
    index: usize,
    time: f64,
    hero_id: String,
    killer_id: Option<String>,
    killer_kind: &'static str,
    position: Option<Point>,
}

pub(crate) fn bundle_hash(manifest: &Value) -> Result<String, &'static str> {
    let files = manifest
        .get("files")
        .and_then(Value::as_object)
        .ok_or("bundle_hash_unavailable")?;
    let match_hash = files
        .get("match.json")
        .and_then(|file| file.get("sha256"))
        .and_then(Value::as_str)
        .filter(|value| valid_sha256(value))
        .ok_or("bundle_hash_unavailable")?;
    let world_hash = files
        .get("world.json")
        .and_then(|file| file.get("sha256"))
        .and_then(Value::as_str)
        .filter(|value| valid_sha256(value))
        .ok_or("bundle_hash_unavailable")?;
    Ok(format!(
        "{:x}",
        Sha256::digest(format!("{match_hash}{world_hash}").as_bytes())
    ))
}

pub(crate) fn bundle_hash12(manifest: &Value) -> Result<String, &'static str> {
    Ok(bundle_hash(manifest)?[..12].to_owned())
}

pub(crate) fn build(input: &KnowledgeInput<'_>) -> Result<Value, &'static str> {
    build_with_sources(input, None, &[], &[], &[], None)
}

pub(crate) fn build_with_sources(
    input: &KnowledgeInput<'_>,
    match_key: Option<&str>,
    local_history: &[MatchEntry],
    opendota_history: &[MatchEntry],
    _lane_history: &[LaneHistoryRecord],
    _own_hero: Option<&str>,
) -> Result<Value, &'static str> {
    // Read the hash only as an integrity/cache input. It is deliberately absent from the digest.
    let _cache_hash = bundle_hash(input.manifest).ok();
    let match_data = input.match_data;
    let world = input.world;
    let engine_schema_31 = input.manifest.pointer("/schema").and_then(Value::as_str) == Some("3.1")
        && match_data.pointer("/meta/schema").and_then(Value::as_str) == Some("3.1");
    let t0 = number(match_data.get("t0")).unwrap_or(f64::NAN);
    let t1 = number(match_data.get("t1")).or_else(|| {
        match_data
            .get("meta")
            .and_then(|meta| number(meta.get("duration_s")))
    });
    let duration = match_data
        .get("meta")
        .and_then(|meta| number(meta.get("duration_s")))
        .or(t1.map(|end| end.max(0.0)));
    let heroes = roster(match_data);
    let hero_by_id = heroes
        .iter()
        .map(|hero| (hero.id.clone(), hero.clone()))
        .collect::<BTreeMap<_, _>>();
    let deaths = death_samples(match_data, world, &hero_by_id, t0);
    let death_data_available = match_data.get("deaths").and_then(Value::as_array).is_some();
    let fight_data_available =
        death_data_available && deaths.iter().all(|death| death.position.is_some());
    let game_mode = match_data
        .pointer("/meta/game_mode")
        .and_then(Value::as_u64);
    let mut lanes = lane_summaries(&heroes, t0, t1, game_mode);
    let fights = teamfights(&heroes, &deaths, t0);
    let phase_names = ["laning", "mid", "late"];
    let boundaries = lane_expectation::phase_boundaries();
    let phase_start = boundaries.first().copied().unwrap_or(0.0);
    let phase_laning_end = boundaries.get(1).copied().unwrap_or(0.0);
    let phase_mid_end = boundaries.get(2).copied().unwrap_or(phase_laning_end);
    let phase_bounds = [
        (phase_start, phase_laning_end),
        (phase_laning_end, phase_mid_end),
        (phase_mid_end, t1.unwrap_or(0.0)),
    ];
    let mut profiles = Vec::with_capacity(heroes.len());
    for hero in &heroes {
        profiles.push(hero_profile(
            hero,
            &heroes,
            &deaths,
            &fights,
            world,
            t0,
            t1,
            duration,
            death_data_available,
            fight_data_available,
            engine_schema_31,
            &phase_names,
            &phase_bounds,
        ));
    }
    // Personal lane history is projected for the UI elsewhere and never enters the cached digest.
    lane_expectation::enrich_lanes(match_data, world, &profiles, &mut lanes, _lane_history);
    let objective_data = objectives(world, &hero_by_id);
    let team_phases = team_phase_summaries(
        &heroes,
        &deaths,
        &fights,
        &objective_data,
        t0,
        t1,
        &phase_names,
        &phase_bounds,
        death_data_available,
        fight_data_available,
    );
    let result = resolve_match_result(
        match_data,
        &heroes,
        match_key,
        local_history,
        opendota_history,
        engine_schema_31,
    );
    let draft = safe_draft(match_data, engine_schema_31);
    let draft_source = (!draft.0.is_null()).then_some("engine");
    let digest = json!({
        "knowledge_version": KNOWLEDGE_VERSION,
        "provisional_thresholds": thresholds_json(),
        "meta": {
            "duration_s": duration.map(number_json).unwrap_or(Value::Null),
            "duration_s_reason": duration.is_none().then_some("match duration is missing").map(Value::from),
            "patch": match_data.pointer("/meta/patch").filter(|value| value.is_string()).cloned().unwrap_or(Value::Null),
            "patch_reason": match_data.pointer("/meta/patch").and_then(Value::as_str).is_none().then_some("patch is not in the bundle").map(Value::from),
            "game_mode": match_data.pointer("/meta/game_mode").filter(|value| value.is_string() || value.is_number()).cloned().unwrap_or(Value::Null),
            "game_mode_reason": match_data.pointer("/meta/game_mode").filter(|value| value.is_string() || value.is_number()).is_none().then_some("game mode is not in the bundle").map(Value::from),
            "t0": number(match_data.get("t0")).map(number_json).unwrap_or(Value::Null),
            "t0_reason": number(match_data.get("t0")).is_none().then_some("match start time is missing").map(Value::from),
            "t1": t1.map(number_json).unwrap_or(Value::Null),
            "t1_reason": t1.is_none().then_some("match end time is missing").map(Value::from),
            "result": result,
            "draft": draft.0,
            "draft_reason": draft.1,
            "draft_source": draft_source,
            "kills": side_kills(&deaths, &hero_by_id, death_data_available)
        },
        "heroes": profiles,
        "team_phases": team_phases,
        "advantage_per_min": t1.map(|_| json!(advantage_series(&heroes, t0, t1))).unwrap_or(Value::Null),
        "advantage_per_min_reason": t1.is_none().then_some("match end time is missing").map(Value::from),
        "lanes": lanes,
        "teamfights": fights,
        "teamfights_reason": (!fight_data_available).then_some(if death_data_available { "position samples are incomplete; teamfight grouping may be incomplete" } else { "death events are missing from the bundle" }).map(Value::from),
        "objectives": objective_data,
        "deaths": death_index(&deaths, &hero_by_id),
        "deaths_reason": (!death_data_available).then_some("death events are missing from the bundle").map(Value::from),
        "opponent_stats": opponent_stats(input.manifest, match_data, &hero_by_id)
    });
    Ok(canonicalize(digest))
}

/// Build the volatile viewer/session facts outside the cached digest block.
pub(crate) fn session_viewer(
    digest: &Value,
    own_hero: Option<&str>,
    gsi: Option<&MatchEntry>,
) -> Value {
    let hero_ids = digest
        .get("heroes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|hero| hero.get("id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let viewer_id = own_hero
        .or_else(|| gsi.and_then(|entry| entry.hero.as_deref()))
        .and_then(normalize_hero_id)
        .filter(|hero| hero_ids.contains(hero.as_str()));
    let mut viewer = viewer_json(viewer_id.as_deref(), gsi);
    let own_side = viewer_id.as_deref().and_then(|id| {
        digest
            .get("heroes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|hero| hero.get("id").and_then(Value::as_str) == Some(id))
            .and_then(|hero| hero.get("side"))
            .and_then(Value::as_str)
            .filter(|side| matches!(*side, "radiant" | "dire"))
    });
    let winner = digest
        .pointer("/meta/result/winner")
        .and_then(Value::as_str)
        .filter(|side| matches!(*side, "radiant" | "dire"));
    let own_result = own_side
        .zip(winner)
        .map(|(side, winner)| if side == winner { "win" } else { "loss" });
    if let Some(object) = viewer.as_object_mut() {
        object.insert(
            "own_side".into(),
            own_side.map(Value::from).unwrap_or(Value::Null),
        );
        object.insert(
            "own_result".into(),
            own_result.map(Value::from).unwrap_or(Value::Null),
        );
    }
    viewer
}

pub(crate) fn canonical_bytes(value: &Value) -> Result<Vec<u8>, &'static str> {
    serde_json::to_vec(&canonicalize(value.clone())).map_err(|_| "knowledge_serialization_failed")
}

pub(crate) fn remove_personal_fields(digest: &mut Value) {
    if let Some(meta) = digest.get_mut("meta").and_then(Value::as_object_mut) {
        meta.remove("viewer");
        if let Some(result) = meta.get_mut("result").and_then(Value::as_object_mut) {
            result.remove("own_side");
            result.remove("own_result");
        }
    }
    if let Some(lanes) = digest.get_mut("lanes").and_then(Value::as_array_mut) {
        for lane in lanes {
            if let Some(lane) = lane.as_object_mut() {
                lane.remove("personal_average");
            }
        }
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        Value::Object(values) => {
            let sorted = values
                .into_iter()
                .map(|(key, value)| (key, canonicalize(value)))
                .collect::<BTreeMap<_, _>>();
            let mut object = Map::new();
            for (key, value) in sorted {
                object.insert(key, value);
            }
            Value::Object(object)
        }
        value => value,
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

fn number_json(value: f64) -> Value {
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
        json!(value as i64)
    } else {
        json!(round_to(
            value,
            lane_expectation::config_integer("/number_precision") as i32
        ))
    }
}

fn round_to(value: f64, digits: i32) -> f64 {
    let scale = 10_f64.powi(digits);
    (value * scale).round() / scale
}

fn hero_id(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| value.starts_with("npc_dota_hero_") && value.len() <= 64)
        .filter(|value| {
            value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
        .map(str::to_owned)
}

fn normalize_hero_id(value: &str) -> Option<String> {
    let candidate = if value.starts_with("npc_dota_hero_") {
        value.to_owned()
    } else {
        format!("npc_dota_hero_{value}")
    };
    hero_id(Some(&Value::String(candidate)))
}

fn side(team: Option<&Value>) -> Option<&'static str> {
    match team.and_then(Value::as_i64) {
        Some(2) => Some("radiant"),
        Some(3) => Some("dire"),
        _ => None,
    }
}

fn safe_hero_name(value: Option<&Value>, id: &str) -> String {
    value
        .and_then(Value::as_str)
        .map(|name| {
            name.chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect::<String>()
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| id.trim_start_matches("npc_dota_hero_").replace('_', " "))
}

fn roster(match_data: &Value) -> Vec<HeroInfo<'_>> {
    let Some(source) = match_data.get("heroes").and_then(Value::as_array) else {
        return Vec::new();
    };
    let t0 = number(match_data.get("t0")).unwrap_or(f64::NAN);
    let mut heroes = source
        .iter()
        .filter_map(|hero| {
            let id = hero_id(hero.get("id"))?;
            let side = side(hero.get("team"))?;
            let slot = hero
                .get("pid")
                .and_then(Value::as_u64)
                .filter(|slot| *slot < 10)?;
            let (lane, lane_share) = assign_lane(hero, t0);
            Some(HeroInfo {
                source: hero,
                name: safe_hero_name(hero.get("name"), &id),
                id,
                side,
                slot,
                lane,
                lane_share,
            })
        })
        .collect::<Vec<_>>();
    heroes.sort_by(|left, right| {
        (left.side, left.slot, &left.id).cmp(&(right.side, right.slot, &right.id))
    });
    heroes
}

fn sample_index(t0: f64, time: f64, len: usize) -> Option<usize> {
    if !time.is_finite() || !t0.is_finite() || len == 0 {
        return None;
    }
    let index = (time.floor() - t0).floor();
    if index < 0.0 || index >= len as f64 {
        None
    } else {
        Some(index as usize)
    }
}

fn sample<'a>(hero: &'a Value, field: &str, time: f64, t0: f64) -> Option<&'a Value> {
    let series = hero.get(field)?.as_array()?;
    series.get(sample_index(t0, time, series.len())?)
}

fn sample_number(hero: &Value, field: &str, time: f64, t0: f64) -> Option<f64> {
    number(sample(hero, field, time, t0))
}

fn eco_number(hero: &Value, column: usize, time: f64, t0: f64) -> Option<f64> {
    sample(hero, "eco", time, t0)?
        .as_array()?
        .get(column)
        .and_then(|value| number(Some(value)))
}

fn alive_at(hero: &Value, time: f64, t0: f64) -> Option<bool> {
    sample(hero, "alive", time, t0).and_then(|value| {
        value
            .as_bool()
            .or_else(|| value.as_i64().map(|number| number != 0))
    })
}

fn position_at(hero: &Value, time: f64, t0: f64) -> Option<Point> {
    let point = sample(hero, "pos", time, t0)?.as_array()?;
    Some(Point {
        x: number(point.first())?,
        y: number(point.get(1))?,
    })
}

fn classify_lane(point: Point) -> &'static str {
    if (point.x - point.y).abs() < lane_expectation::config_number("/lane_assignment/mid_delta_max")
        && (point.x + point.y).abs()
            < lane_expectation::config_number("/lane_assignment/mid_sum_max")
    {
        "mid"
    } else if point.y > point.x {
        "top"
    } else {
        "bot"
    }
}

fn assign_lane(hero: &Value, t0: f64) -> (Option<&'static str>, Option<f64>) {
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut total = 0_usize;
    for time in lane_expectation::config_signed_integer("/lane_assignment/sample_from_s")
        ..=lane_expectation::config_signed_integer("/lane_assignment/sample_to_s")
    {
        if alive_at(hero, time as f64, t0) == Some(false) {
            continue;
        }
        if let Some(point) = position_at(hero, time as f64, t0) {
            *counts.entry(classify_lane(point)).or_default() += 1;
            total += 1;
        }
    }
    if total == 0 {
        return (None, None);
    }
    let Some((lane, count)) = counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(right.0)))
    else {
        return (None, None);
    };
    let share = count as f64 / total as f64;
    if share < lane_expectation::config_number("/lane_assignment/minimum_lane_share") {
        (
            None,
            Some(round_to(
                share,
                lane_expectation::config_integer("/number_precision") as i32,
            )),
        )
    } else {
        let lane = match lane {
            "top" => "top",
            "mid" => "mid",
            "bot" => "bot",
            _ => return (None, None),
        };
        (
            Some(lane),
            Some(round_to(
                share,
                lane_expectation::config_integer("/number_precision") as i32,
            )),
        )
    }
}

fn death_samples(
    match_data: &Value,
    world: &Value,
    heroes: &BTreeMap<String, HeroInfo<'_>>,
    t0: f64,
) -> Vec<DeathSample> {
    match_data
        .get("deaths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, death)| {
            let time = number(death.get("t"))?;
            let victim = hero_id(death.get("hero"))?;
            if !heroes.contains_key(&victim) {
                return None;
            }
            let killer_source = death.get("killer").and_then(Value::as_str);
            let killer = hero_id(death.get("killer")).filter(|id| heroes.contains_key(id));
            let killer_kind = match killer_source {
                Some(_) if killer.is_some() => "hero",
                Some(id) if id.starts_with("npc_dota_hero_") => "unknown",
                Some(_) => "other",
                None => "unknown",
            };
            Some(DeathSample {
                index,
                time,
                position: death_position(world, &victim, time).or_else(|| {
                    heroes
                        .get(&victim)
                        .and_then(|hero| position_at(hero.source, time, t0))
                }),
                hero_id: victim,
                killer_id: killer,
                killer_kind,
            })
        })
        .collect()
}

fn death_position(world: &Value, hero_id: &str, time: f64) -> Option<Point> {
    let track = world.get("heroes")?.get(hero_id)?;
    let times = track.get("t")?.as_array()?;
    let xs = track.get("x")?.as_array()?;
    let ys = track.get("y")?.as_array()?;
    let index = times
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            number(Some(value)).map(|candidate| (index, (candidate - time).abs()))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))?
        .0;
    if number(times.get(index)).is_none_or(|sample_time| (sample_time - time).abs() > 2.0) {
        return None;
    }
    Some(Point {
        x: number(xs.get(index))?,
        y: number(ys.get(index))?,
    })
}

fn hero_profile<'a>(
    hero: &HeroInfo<'a>,
    heroes: &[HeroInfo<'a>],
    deaths: &[DeathSample],
    fights: &[Value],
    world: &Value,
    t0: f64,
    t1: Option<f64>,
    duration: Option<f64>,
    death_data_available: bool,
    fight_data_available: bool,
    engine_schema_31: bool,
    phase_names: &[&str; 3],
    phase_bounds: &[(f64, f64); 3],
) -> Value {
    let end = t1.unwrap_or_else(|| duration.unwrap_or(0.0));
    let final_level = sample_number(hero.source, "lvl", end, t0);
    let final_nw = eco_number(hero.source, 1, end, t0);
    let final_lh = eco_number(hero.source, 2, end, t0);
    let final_dn = eco_number(hero.source, 3, end, t0);
    let final_xp = sample_number(hero.source.get("st").unwrap_or(&Value::Null), "xp", end, t0);
    let final_kills = death_data_available.then(|| {
        deaths
            .iter()
            .filter(|death| death.killer_id.as_deref() == Some(hero.id.as_str()))
            .count() as f64
    });
    let final_deaths = death_data_available.then(|| {
        deaths
            .iter()
            .filter(|death| death.hero_id == hero.id)
            .count() as f64
    });
    let nw_per_min = duration
        .filter(|value| *value > 0.0)
        .and_then(|value| final_nw.map(|nw| nw / (value / 60.0)));
    let xpm = duration
        .filter(|value| *value > 0.0)
        .and_then(|value| final_xp.map(|xp| xp / (value / 60.0)));
    let gold_earned = engine_schema_31
        .then(|| number(hero.source.get("gold_earned_final")).filter(|value| *value >= 0.0))
        .flatten();
    let gpm = duration
        .filter(|value| *value > 0.0)
        .and_then(|value| gold_earned.map(|gold| gold / (value / 60.0)));
    let gpm_reason = if !engine_schema_31 {
        ENGINE_REPARSE_REASON
    } else if duration.is_none_or(|value| value <= 0.0) {
        "match duration is missing"
    } else {
        "total gold earned is unavailable for this hero"
    };
    let (role, role_reason) = role_for(hero, heroes, t0);
    let mut final_stats = Map::new();
    add_number(
        &mut final_stats,
        "lvl",
        final_level,
        "level samples are missing",
    );
    add_number(
        &mut final_stats,
        "k",
        final_kills,
        "death events are missing from the bundle",
    );
    add_number(
        &mut final_stats,
        "d",
        final_deaths,
        "death events are missing from the bundle",
    );
    add_number(
        &mut final_stats,
        "a",
        None,
        "exact assists are not in the bundle",
    );
    add_number(
        &mut final_stats,
        "nw",
        final_nw,
        "net-worth samples are missing",
    );
    add_number(
        &mut final_stats,
        "lh",
        final_lh,
        "last-hit samples are missing",
    );
    add_number(&mut final_stats, "dn", final_dn, "deny samples are missing");
    add_number(&mut final_stats, "xp", final_xp, "XP samples are missing");
    let mut at_10 = Map::new();
    let boundary = lane_expectation::standard_boundary();
    if end < boundary {
        at_10.insert("available".into(), Value::Bool(false));
        at_10.insert(
            "reason".into(),
            Value::String("match ended before 10:00".into()),
        );
    } else {
        at_10.insert("available".into(), Value::Bool(true));
        add_number(
            &mut at_10,
            "nw",
            eco_number(hero.source, 1, boundary, t0),
            "net-worth sample at 10:00 is missing",
        );
        add_number(
            &mut at_10,
            "lh",
            eco_number(hero.source, 2, boundary, t0),
            "last-hit sample at 10:00 is missing",
        );
        add_number(
            &mut at_10,
            "dn",
            eco_number(hero.source, 3, boundary, t0),
            "deny sample at 10:00 is missing",
        );
        add_number(
            &mut at_10,
            "xp",
            sample_number(
                hero.source.get("st").unwrap_or(&Value::Null),
                "xp",
                boundary,
                t0,
            ),
            "XP sample at 10:00 is missing",
        );
        add_number(
            &mut at_10,
            "lvl",
            sample_number(hero.source, "lvl", boundary, t0),
            "level sample at 10:00 is missing",
        );
    }
    let mut phases = Map::new();
    for (name, (start, stop)) in phase_names.iter().zip(phase_bounds) {
        if *start >= end && *name != "laning" {
            continue;
        }
        phases.insert(
            (*name).to_owned(),
            hero_phase(
                hero,
                deaths,
                *start,
                stop.min(end),
                t0,
                death_data_available,
            ),
        );
    }
    let mut rates = Map::new();
    add_number(
        &mut rates,
        "nw_per_min",
        nw_per_min,
        "net worth or match duration is missing",
    );
    add_number(&mut rates, "xpm", xpm, "XP or match duration is missing");
    add_number(&mut rates, "gpm", gpm, gpm_reason);
    rates.insert(
        "gpm_source".into(),
        if gpm.is_some() {
            Value::from("engine")
        } else {
            Value::Null
        },
    );
    let gold_earned_reason = if !engine_schema_31 {
        ENGINE_REPARSE_REASON
    } else {
        "total gold earned is unavailable for this hero"
    };
    let mut lane = Map::new();
    lane.insert(
        "lane".into(),
        hero.lane
            .map_or(Value::Null, |value| Value::String(value.to_owned())),
    );
    lane.insert(
        "share".into(),
        hero.lane_share.map_or(Value::Null, number_json),
    );
    if hero.lane.is_none() {
        lane.insert(
            "reason".into(),
            Value::String("roaming or positions missing".into()),
        );
    }
    let mut role_value = Map::new();
    role_value.insert("role".into(), role.map_or(Value::Null, Value::String));
    role_value.insert(
        "basis".into(),
        hero.lane
            .map_or(Value::Null, |value| Value::String(format!("lane={value}"))),
    );
    role_value.insert("provisional".into(), Value::Bool(true));
    if let Some(reason) = role_reason {
        role_value.insert("reason".into(), Value::String(reason.into()));
    }
    let teamfights_present = fights
        .iter()
        .filter(|fight| {
            fight
                .get("present")
                .and_then(Value::as_array)
                .is_some_and(|present| present.iter().any(|id| id.as_str() == Some(&hero.id)))
        })
        .filter_map(|fight| fight.get("n").cloned())
        .collect::<Vec<_>>();
    let (final_items, final_items_reason) = final_inventory(hero.source, end);
    let runes = world.get("runes").and_then(Value::as_array).map(|runes| {
        runes
            .iter()
            .filter(|rune| rune.get("by").and_then(Value::as_str) == Some(hero.id.as_str()))
            .count() as f64
    });
    let dead_seconds = dead_seconds(hero.source, t0, end);
    let damage_out = |key: &str| {
        engine_schema_31
            .then(|| {
                number(hero.source.pointer(&format!("/dmg_out/{key}")))
                    .filter(|value| *value >= 0.0)
            })
            .flatten()
    };
    let engine_damage = damage_out("total");
    let damage_reason = if !engine_schema_31 {
        ENGINE_REPARSE_REASON
    } else {
        "attacker-side hero damage is not present in the bundle"
    };
    let stun = number(hero.source.get("stun_s_dealt"));
    json!({
        "hero": hero.name,
        "id": hero.id,
        "side": hero.side,
        "slot": hero.slot,
        "lane": lane,
        "role": role_value,
        "final": Value::Object(final_stats),
        "at_10": Value::Object(at_10),
        "rates": Value::Object(rates),
        "phases": Value::Object(phases),
        "items": purchase_timings(hero.source),
        "final_items": final_items,
        "final_items_reason": final_items_reason,
        "runes": runes.map(number_json).unwrap_or(Value::Null),
        "runes_reason": runes.is_none().then_some("rune tracks are missing").map(Value::from),
        "dead_s": dead_seconds.map(number_json).unwrap_or(Value::Null),
        "dead_s_reason": dead_seconds.is_none().then_some("alive samples are missing").map(Value::from),
        "teamfights_present": if fight_data_available { Value::Array(teamfights_present) } else { Value::Null },
        "teamfights_present_reason": (!fight_data_available).then_some(if death_data_available { "death positions are incomplete; teamfight presence is unknown" } else { "death events are missing from the bundle" }).map(Value::from),
        "gold_earned": gold_earned.map(number_json).unwrap_or(Value::Null),
        "gold_earned_reason": gold_earned.is_none().then_some(gold_earned_reason).map(Value::from),
        "gold_earned_source": gold_earned.is_some().then_some("engine"),
        "hero_damage": engine_damage.map(number_json).unwrap_or(Value::Null),
        "hero_damage_reason": engine_damage.is_none().then_some(damage_reason).map(Value::from),
        "hero_damage_source": engine_damage.is_some().then_some("engine"),
        "damage_out": {
            "total": engine_damage.map(number_json).unwrap_or(Value::Null),
            "p": damage_out("p").map(number_json).unwrap_or(Value::Null),
            "m": damage_out("m").map(number_json).unwrap_or(Value::Null),
            "u": damage_out("u").map(number_json).unwrap_or(Value::Null)
        },
        "damage_out_reason": engine_damage.is_none().then_some(damage_reason).map(Value::from),
        "damage_out_source": engine_damage.is_some().then_some("engine"),
        "stun_s_dealt": stun.map(number_json).unwrap_or(Value::Null),
        "stun_s_dealt_reason": stun.is_none().then_some("attacker-side control duration is not present in the bundle").map(Value::from),
        "tower_damage": Value::Null,
        "tower_damage_reason": "attacker-side building damage is not present in the bundle",
        "healing_done": Value::Null,
        "healing_done_reason": "healing source is not present in the bundle",
        "talents": Value::Null,
        "talents_reason": "talent picks are not present in the bundle",
        "buybacks": Value::Null,
        "buybacks_reason": "buyback events are not present in the bundle"
    })
}

fn add_number(target: &mut Map<String, Value>, name: &str, value: Option<f64>, reason: &str) {
    target.insert(
        name.to_owned(),
        value.map(number_json).unwrap_or(Value::Null),
    );
    if value.is_none() {
        target.insert(format!("{name}_reason"), Value::String(reason.to_owned()));
    }
}

fn hero_phase(
    hero: &HeroInfo<'_>,
    deaths: &[DeathSample],
    from: f64,
    to: f64,
    t0: f64,
    death_data_available: bool,
) -> Value {
    let start_nw = eco_number(hero.source, 1, from, t0);
    let end_nw = eco_number(hero.source, 1, to, t0);
    let start_lh = eco_number(hero.source, 2, from, t0);
    let end_lh = eco_number(hero.source, 2, to, t0);
    let start_dn = eco_number(hero.source, 3, from, t0);
    let end_dn = eco_number(hero.source, 3, to, t0);
    let xp = hero.source.get("st").unwrap_or(&Value::Null);
    let start_xp = sample_number(xp, "xp", from, t0);
    let end_xp = sample_number(xp, "xp", to, t0);
    let start_lvl = sample_number(hero.source, "lvl", from, t0);
    let end_lvl = sample_number(hero.source, "lvl", to, t0);
    let kills = death_data_available.then(|| {
        deaths
            .iter()
            .filter(|death| {
                death.killer_id.as_deref() == Some(hero.id.as_str())
                    && death.time > from
                    && death.time <= to
            })
            .count()
    });
    let hero_deaths = death_data_available.then(|| {
        deaths
            .iter()
            .filter(|death| death.hero_id == hero.id && death.time > from && death.time <= to)
            .count()
    });
    let dead_s = dead_seconds_between(hero.source, t0, from, to);
    json!({
        "nw": [start_nw.map(number_json).unwrap_or(Value::Null), end_nw.map(number_json).unwrap_or(Value::Null)],
        "nw_reason": (start_nw.is_none() || end_nw.is_none()).then_some("net-worth samples are missing at a phase boundary").map(Value::from),
        "lh": end_lh.zip(start_lh).map(|(end, start)| number_json(end - start)).unwrap_or(Value::Null),
        "lh_reason": (end_lh.is_none() || start_lh.is_none()).then_some("last-hit samples are missing at a phase boundary").map(Value::from),
        "dn": end_dn.zip(start_dn).map(|(end, start)| number_json(end - start)).unwrap_or(Value::Null),
        "dn_reason": (end_dn.is_none() || start_dn.is_none()).then_some("deny samples are missing at a phase boundary").map(Value::from),
        "xp": [start_xp.map(number_json).unwrap_or(Value::Null), end_xp.map(number_json).unwrap_or(Value::Null)],
        "xp_reason": (start_xp.is_none() || end_xp.is_none()).then_some("XP samples are missing at a phase boundary").map(Value::from),
        "lvl": [start_lvl.map(number_json).unwrap_or(Value::Null), end_lvl.map(number_json).unwrap_or(Value::Null)],
        "lvl_reason": (start_lvl.is_none() || end_lvl.is_none()).then_some("level samples are missing at a phase boundary").map(Value::from),
        "k": kills,
        "k_reason": (!death_data_available).then_some("death events are missing from the bundle").map(Value::from),
        "d": hero_deaths,
        "d_reason": (!death_data_available).then_some("death events are missing from the bundle").map(Value::from),
        "dead_s": dead_s.map(number_json).unwrap_or(Value::Null),
        "dead_s_reason": dead_s.is_none().then_some("alive samples are missing in this phase").map(Value::from)
    })
}

fn dead_seconds(hero: &Value, t0: f64, end: f64) -> Option<f64> {
    dead_seconds_between(hero, t0, 0.0, end)
}

fn dead_seconds_between(hero: &Value, t0: f64, from: f64, to: f64) -> Option<f64> {
    let alive = hero.get("alive")?.as_array()?;
    let start = sample_index(t0, from, alive.len())?;
    let stop = sample_index(t0, to, alive.len()).unwrap_or_else(|| alive.len().saturating_sub(1));
    if stop < start {
        return Some(0.0);
    }
    Some(
        alive[start..=stop]
            .iter()
            .filter(|value| value.as_bool() == Some(false) || value.as_i64() == Some(0))
            .count() as f64,
    )
}

fn role_for(
    hero: &HeroInfo<'_>,
    heroes: &[HeroInfo<'_>],
    t0: f64,
) -> (Option<String>, Option<&'static str>) {
    match hero.lane {
        None => (None, Some("lane is unknown")),
        Some("mid") => (Some("mid".into()), None),
        Some(lane) => {
            let same_lane = heroes
                .iter()
                .filter(|other| other.side == hero.side && other.lane == Some(lane))
                .collect::<Vec<_>>();
            if same_lane.len() != 2 {
                return (
                    None,
                    Some("lane swaps, trilanes, or missing teammate positions"),
                );
            }
            let mut by_nw = same_lane
                .iter()
                .map(|candidate| {
                    (
                        candidate,
                        eco_number(
                            candidate.source,
                            1,
                            lane_expectation::standard_boundary(),
                            t0,
                        ),
                    )
                })
                .collect::<Vec<_>>();
            if by_nw.iter().any(|(_, nw)| nw.is_none()) {
                return (None, Some("net worth at 10:00 is missing"));
            }
            by_nw.sort_by(|left, right| {
                right
                    .1
                    .unwrap_or_default()
                    .total_cmp(&left.1.unwrap_or_default())
                    .then_with(|| left.0.slot.cmp(&right.0.slot))
            });
            let is_core = by_nw
                .first()
                .is_some_and(|(candidate, _)| candidate.id == hero.id);
            let safe_lane =
                (hero.side == "radiant" && lane == "bot") || (hero.side == "dire" && lane == "top");
            let role = match (safe_lane, is_core) {
                (true, true) => "carry",
                (false, true) => "offlane",
                (false, false) => "soft_support",
                (true, false) => "hard_support",
            };
            (Some(role.into()), None)
        }
    }
}

fn purchase_timings(hero: &Value) -> Value {
    let Some(buys) = hero.get("buys").and_then(Value::as_array) else {
        return json!({"value":null,"reason":"purchase timeline is missing"});
    };
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for buy in buys {
        let Some(row) = buy.as_array() else { continue };
        let (Some(time), Some(name)) = (number(row.first()), row.get(1).and_then(Value::as_str))
        else {
            continue;
        };
        if !valid_item_id(name) || !seen.insert(name.to_owned()) {
            continue;
        }
        output.push(json!([number_json(time), name]));
    }
    Value::Array(output)
}

fn final_inventory(hero: &Value, end: f64) -> (Value, Value) {
    let Some(inventory) = hero.get("inv").and_then(Value::as_array) else {
        return (Value::Null, Value::from("inventory timeline is missing"));
    };
    let latest = inventory
        .iter()
        .filter_map(|row| {
            let values = row.as_array()?;
            let time = number(values.first())?;
            if time <= end {
                values.get(1).map(|slots| (time, slots))
            } else {
                None
            }
        })
        .max_by(|left, right| left.0.total_cmp(&right.0));
    let Some((_, slots)) = latest else {
        return (
            Value::Null,
            Value::from("inventory has no snapshot at or before match end"),
        );
    };
    let Some(slots) = slots.as_array() else {
        return (Value::Null, Value::from("inventory slots are malformed"));
    };
    (
        Value::Array(
            slots
                .iter()
                .take(6)
                .map(|slot| {
                    if slot.is_null() {
                        Value::Null
                    } else {
                        item_id(slot).map(Value::String).unwrap_or(Value::Null)
                    }
                })
                .collect(),
        ),
        Value::Null,
    )
}

fn item_id(value: &Value) -> Option<String> {
    let name = value.as_str().or_else(|| {
        value
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| value.get("id").and_then(Value::as_str))
            .or_else(|| value.get("item").and_then(Value::as_str))
    })?;
    valid_item_id(name).then(|| name.to_owned())
}

fn valid_item_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn lane_summaries(
    heroes: &[HeroInfo<'_>],
    t0: f64,
    t1: Option<f64>,
    game_mode: Option<u64>,
) -> Vec<Value> {
    ["top", "mid", "bot"].into_iter().map(|lane| {
        let radiant = heroes.iter().filter(|hero| hero.side == "radiant" && hero.lane == Some(lane)).collect::<Vec<_>>();
        let dire = heroes.iter().filter(|hero| hero.side == "dire" && hero.lane == Some(lane)).collect::<Vec<_>>();
        let boundary = lane_expectation::boundary_for_mode(game_mode);
        let sample_boundary = boundary.filter(|value| {
            t1.is_some_and(|end| end >= *value) && !radiant.is_empty() && !dire.is_empty()
        });
        let radiant_lh = sample_boundary.and_then(|time| sum_metric(&radiant, Metric::LastHits, time, t0));
        let dire_lh = sample_boundary.and_then(|time| sum_metric(&dire, Metric::LastHits, time, t0));
        let radiant_dn = sample_boundary.and_then(|time| sum_metric(&radiant, Metric::Denies, time, t0));
        let dire_dn = sample_boundary.and_then(|time| sum_metric(&dire, Metric::Denies, time, t0));
        let (nw_diff, xp_diff, adv_pct, outcome, reason) = if boundary.is_none() {
            (None, None, None, None, Some("lane boundaries are not established for this game mode"))
        } else if t1.is_none_or(|end| end < boundary.unwrap_or(0.0)) {
            (None, None, None, None, Some("match ended before 10:00"))
        } else if radiant.is_empty() || dire.is_empty() {
            (None, None, None, None, Some("lane assignment or positions are missing on one side"))
        } else {
            let boundary = boundary.unwrap_or(0.0);
            let radiant_nw = sum_metric(&radiant, Metric::NetWorth, boundary, t0);
            let dire_nw = sum_metric(&dire, Metric::NetWorth, boundary, t0);
            let radiant_xp = sum_metric(&radiant, Metric::Xp, boundary, t0);
            let dire_xp = sum_metric(&dire, Metric::Xp, boundary, t0);
            match (radiant_nw, dire_nw, radiant_xp, dire_xp) {
                (Some(rnw), Some(dnw), Some(rxp), Some(dxp)) if rnw + dnw > 0.0 => {
                    let nw = rnw - dnw;
                    let xp = rxp - dxp;
                    let pct = lane_expectation::config_number("/score/percent_scale") * nw / (rnw + dnw);
                    let label = if pct.abs() < lane_expectation::config_number("/raw_lane_result/even_advantage_pct") || (nw != 0.0 && xp != 0.0 && nw.signum() != xp.signum()) {
                        "even"
                    } else if pct.abs() >= lane_expectation::config_number("/raw_lane_result/stomp_advantage_pct") {
                        if pct > 0.0 { "radiant_stomp" } else { "dire_stomp" }
                    } else if pct > 0.0 {
                        "radiant_won"
                    } else {
                        "dire_won"
                    };
                    (Some(nw), Some(xp), Some(pct), Some(label), None)
                }
                _ => (None, None, None, None, Some("net-worth or XP samples at 10:00 are missing")),
            }
        };
        let lh_10 = [radiant_lh, dire_lh]
            .map(|value| value.map(number_json).unwrap_or(Value::Null));
        let dn_10 = [radiant_dn, dire_dn]
            .map(|value| value.map(number_json).unwrap_or(Value::Null));
        let mut value = json!({
            "lane": lane,
            "radiant": radiant.iter().map(|hero| json!({"hero":hero.id,"name":hero.name})).collect::<Vec<_>>(),
            "dire": dire.iter().map(|hero| json!({"hero":hero.id,"name":hero.name})).collect::<Vec<_>>(),
            "nw_diff_10": nw_diff.map(number_json).unwrap_or(Value::Null),
            "xp_diff_10": xp_diff.map(number_json).unwrap_or(Value::Null),
            "lh_10": lh_10,
            "dn_10": dn_10,
            "adv_pct": adv_pct.map(|value| number_json(round_to(value, 1))).unwrap_or(Value::Null),
            "outcome": outcome.map(Value::from).unwrap_or(Value::Null),
            "threshold_label": "เกณฑ์ชั่วคราว"
        });
        if let Some(reason) = reason {
            value["outcome_reason"] = Value::String(reason.into());
            value["nw_diff_10_reason"] = Value::String(reason.into());
            value["xp_diff_10_reason"] = Value::String(reason.into());
            value["adv_pct_reason"] = Value::String(reason.into());
        }
        if radiant_lh.is_none() || dire_lh.is_none() {
            value["lh_10_reason"] = Value::String("last-hit samples at 10:00 are missing".into());
        }
        if radiant_dn.is_none() || dire_dn.is_none() {
            value["dn_10_reason"] = Value::String("deny samples at 10:00 are missing".into());
        }
        value
    }).collect()
}

#[derive(Clone, Copy)]
enum Metric {
    NetWorth,
    Xp,
    LastHits,
    Denies,
}

fn sum_metric(heroes: &[&HeroInfo<'_>], metric: Metric, time: f64, t0: f64) -> Option<f64> {
    if heroes.is_empty() {
        return None;
    }
    heroes.iter().try_fold(0.0, |sum, hero| {
        let value = match metric {
            Metric::NetWorth => eco_number(hero.source, 1, time, t0),
            Metric::LastHits => eco_number(hero.source, 2, time, t0),
            Metric::Denies => eco_number(hero.source, 3, time, t0),
            Metric::Xp => sample_number(
                hero.source.get("st").unwrap_or(&Value::Null),
                "xp",
                time,
                t0,
            ),
        }?;
        Some(sum + value)
    })
}

fn advantage_series(heroes: &[HeroInfo<'_>], t0: f64, t1: Option<f64>) -> Vec<Value> {
    let Some(end) = t1 else { return Vec::new() };
    if end < 0.0 {
        return Vec::new();
    }
    let last_minute = (end / 60.0).floor() as usize;
    (0..=last_minute).map(|minute| {
        let time = (minute * 60) as f64;
        let radiant = heroes.iter().filter(|hero| hero.side == "radiant").collect::<Vec<_>>();
        let dire = heroes.iter().filter(|hero| hero.side == "dire").collect::<Vec<_>>();
        let nw = sum_metric(&radiant, Metric::NetWorth, time, t0).zip(sum_metric(&dire, Metric::NetWorth, time, t0)).map(|(rad, dire)| rad - dire);
        let xp = sum_metric(&radiant, Metric::Xp, time, t0).zip(sum_metric(&dire, Metric::Xp, time, t0)).map(|(rad, dire)| rad - dire);
        json!({
            "minute": minute,
            "t": time as i64,
            "radiant_nw_diff": nw.map(number_json).unwrap_or(Value::Null),
            "radiant_nw_diff_reason": nw.is_none().then_some("team net-worth samples are missing").map(Value::from),
            "radiant_xp_diff": xp.map(number_json).unwrap_or(Value::Null),
            "radiant_xp_diff_reason": xp.is_none().then_some("team XP samples are missing").map(Value::from)
        })
    }).collect()
}

fn team_phase_summaries(
    heroes: &[HeroInfo<'_>],
    deaths: &[DeathSample],
    fights: &[Value],
    objectives: &Value,
    t0: f64,
    t1: Option<f64>,
    names: &[&str; 3],
    bounds: &[(f64, f64); 3],
    death_data_available: bool,
    fight_data_available: bool,
) -> Value {
    let mut phases = Map::new();
    let end = t1.unwrap_or(0.0);
    for (name, (from, to)) in names.iter().zip(bounds) {
        if *from >= end && *name != "laning" {
            continue;
        }
        let stop = to.min(end);
        let counts = |side_name: &str, killer: bool| -> Value {
            if !death_data_available {
                return Value::Null;
            }
            json!(deaths
                .iter()
                .filter(|death| {
                    death.time > *from
                        && death.time <= stop
                        && (if killer {
                            death.killer_id.as_deref().and_then(|id| {
                                heroes
                                    .iter()
                                    .find(|hero| hero.id == id)
                                    .map(|hero| hero.side)
                            }) == Some(side_name)
                        } else {
                            heroes
                                .iter()
                                .find(|hero| hero.id == death.hero_id)
                                .map(|hero| hero.side)
                                == Some(side_name)
                        })
                })
                .count())
        };
        let at_start = team_diff(heroes, *from, t0);
        let at_end = team_diff(heroes, stop, t0);
        let fights_in_phase = fights
            .iter()
            .filter(|fight| {
                fight
                    .pointer("/t/0")
                    .and_then(Value::as_f64)
                    .is_some_and(|time| time >= *from && time <= stop)
            })
            .count();
        let (radiant_towers_lost, radiant_towers_reason) =
            phase_tower_losses(objectives, "radiant", *from, stop);
        let (dire_towers_lost, dire_towers_reason) =
            phase_tower_losses(objectives, "dire", *from, stop);
        phases.insert((*name).to_owned(), json!({
            "t": [number_json(*from), number_json(stop)],
            "radiant_kills": counts("radiant", true),
            "radiant_kills_reason": (!death_data_available).then_some("death events are missing from the bundle").map(Value::from),
            "dire_kills": counts("dire", true),
            "dire_kills_reason": (!death_data_available).then_some("death events are missing from the bundle").map(Value::from),
            "radiant_deaths": counts("radiant", false),
            "radiant_deaths_reason": (!death_data_available).then_some("death events are missing from the bundle").map(Value::from),
            "dire_deaths": counts("dire", false),
            "dire_deaths_reason": (!death_data_available).then_some("death events are missing from the bundle").map(Value::from),
            "nw_diff_end": at_end.as_ref().and_then(|value| value.0).map(number_json).unwrap_or(Value::Null),
            "nw_diff_end_reason": at_end.as_ref().and_then(|value| value.0).is_none().then_some("team net-worth samples are missing").map(Value::from),
            "xp_diff_end": at_end.as_ref().and_then(|value| value.1).map(number_json).unwrap_or(Value::Null),
            "xp_diff_end_reason": at_end.as_ref().and_then(|value| value.1).is_none().then_some("team XP samples are missing").map(Value::from),
            "nw_diff_change": at_start.as_ref().and_then(|start| start.0).zip(at_end.as_ref().and_then(|end| end.0)).map(|(start,end)|number_json(end-start)).unwrap_or(Value::Null),
            "nw_diff_change_reason": (at_start.as_ref().and_then(|value|value.0).is_none() || at_end.as_ref().and_then(|value|value.0).is_none()).then_some("team net-worth samples are missing").map(Value::from),
            "radiant_towers_lost": radiant_towers_lost,
            "radiant_towers_lost_reason": radiant_towers_reason,
            "dire_towers_lost": dire_towers_lost,
            "dire_towers_lost_reason": dire_towers_reason,
            "teamfights": if fight_data_available { json!(fights_in_phase) } else { Value::Null },
            "teamfights_reason": (!fight_data_available).then_some("death or position samples are incomplete").map(Value::from)
        }));
    }
    Value::Object(phases)
}

fn phase_tower_losses(
    objectives: &Value,
    side_name: &str,
    from: f64,
    to: f64,
) -> (Value, Option<&'static str>) {
    let Some(buildings) = objectives.get("buildings").and_then(Value::as_array) else {
        return (Value::Null, Some("building tracks are missing"));
    };
    let mut destroyed = 0_u64;
    let mut incomplete = false;
    for building in buildings {
        let team = building.get("team").and_then(Value::as_str);
        let start = number(building.get("start_s"));
        let end = number(building.get("end_s"));
        let overlaps = match (start, end) {
            (Some(start), Some(end)) => end >= from && start <= to,
            _ => true,
        };
        if !overlaps {
            continue;
        }
        if team.is_none() || team == Some(side_name) {
            match building.get("destroyed").and_then(Value::as_bool) {
                Some(true) if team == Some(side_name) => {
                    match building.get("destroyed_at_s").and_then(Value::as_f64) {
                        Some(time) if time >= from && time <= to => destroyed += 1,
                        Some(_) => {}
                        None => incomplete = true,
                    }
                }
                Some(false) => {}
                _ => incomplete = true,
            }
        }
    }
    if incomplete {
        (Value::Null, Some("building destruction data is incomplete"))
    } else {
        (Value::from(destroyed), None)
    }
}

fn team_diff(heroes: &[HeroInfo<'_>], time: f64, t0: f64) -> Option<(Option<f64>, Option<f64>)> {
    let radiant = heroes
        .iter()
        .filter(|hero| hero.side == "radiant")
        .collect::<Vec<_>>();
    let dire = heroes
        .iter()
        .filter(|hero| hero.side == "dire")
        .collect::<Vec<_>>();
    let nw = sum_metric(&radiant, Metric::NetWorth, time, t0)?
        - sum_metric(&dire, Metric::NetWorth, time, t0)?;
    let xp = sum_metric(&radiant, Metric::Xp, time, t0)? - sum_metric(&dire, Metric::Xp, time, t0)?;
    Some((Some(nw), Some(xp)))
}

fn side_kills(
    deaths: &[DeathSample],
    heroes: &BTreeMap<String, HeroInfo<'_>>,
    available: bool,
) -> Value {
    let mut radiant = 0_u64;
    let mut dire = 0_u64;
    for death in deaths {
        if let Some(killer) = death.killer_id.as_deref().and_then(|id| heroes.get(id)) {
            match killer.side {
                "radiant" => radiant += 1,
                "dire" => dire += 1,
                _ => {}
            }
        }
    }
    if available {
        json!({"radiant":radiant,"dire":dire,"radiant_reason":null,"dire_reason":null})
    } else {
        json!({"radiant":null,"dire":null,"radiant_reason":"death events are missing from the bundle","dire_reason":"death events are missing from the bundle"})
    }
}

fn resolve_match_result(
    match_data: &Value,
    heroes: &[HeroInfo<'_>],
    match_key: Option<&str>,
    local_history: &[MatchEntry],
    opendota_history: &[MatchEntry],
    engine_schema_31: bool,
) -> Value {
    let engine = engine_schema_31
        .then(|| {
            match_data
                .pointer("/meta/winner")
                .and_then(Value::as_str)
                .filter(|winner| matches!(*winner, "radiant" | "dire"))
        })
        .flatten();
    let mut candidates = Vec::<(&str, &'static str)>::new();
    if let Some(winner) = engine {
        candidates.push((
            "engine",
            if winner == "radiant" {
                "radiant"
            } else {
                "dire"
            },
        ));
    }
    let mut mismatch = false;
    if let Some(match_key) = match_key {
        if let Some(entry) = local_history.iter().find(|entry| {
            entry.match_key == match_key && entry.source == "kohbor" && entry.result.is_some()
        }) {
            match history_winner(entry, heroes) {
                Ok(Some(winner)) => candidates.push(("gsi_history", winner)),
                Ok(None) => {}
                Err(()) => mismatch = true,
            }
        }
        if let Some(entry) = opendota_history.iter().find(|entry| {
            entry.match_key == match_key && entry.source == "opendota" && entry.result.is_some()
        }) {
            match history_winner(entry, heroes) {
                Ok(Some(winner)) => candidates.push(("opendota_history", winner)),
                Ok(None) => {}
                Err(()) => mismatch = true,
            }
        }
    }
    let distinct = candidates
        .iter()
        .map(|(_, winner)| *winner)
        .collect::<BTreeSet<_>>();
    if distinct.len() > 1 {
        crate::log_warn!(
            "analytics",
            "match_result_conflict",
            "replay engine and match history results disagree; hiding result"
        );
        return json!({
            "winner":null,"source":null,
            "conflict":true,"reason":"sources_disagree","warning":"sources_disagree"
        });
    }
    if mismatch {
        return json!({
            "winner":null,"source":null,
            "conflict":false,"reason":"history_hero_mismatch"
        });
    }
    if let Some((source, winner)) = candidates.first() {
        return json!({
            "winner":winner,"source":source,
            "conflict":false,"reason":null
        });
    }
    let reason = if !engine_schema_31 {
        ENGINE_REPARSE_REASON
    } else {
        match_data
            .pointer("/meta/winner_reason")
            .and_then(Value::as_str)
            .unwrap_or("match winner is not present in the bundle")
    };
    json!({
        "winner":null,"source":null,
        "conflict":false,"reason":reason
    })
}

fn history_winner(entry: &MatchEntry, heroes: &[HeroInfo<'_>]) -> Result<Option<&'static str>, ()> {
    let Some(result) = entry.result else {
        return Ok(None);
    };
    let Some(hero) = entry.hero.as_deref() else {
        return Err(());
    };
    let Some(side) = heroes
        .iter()
        .find(|candidate| candidate.id == hero)
        .map(|hero| hero.side)
    else {
        return Err(());
    };
    Ok(Some(match (side, result) {
        ("radiant", MatchResult::Win) | ("dire", MatchResult::Loss) => "radiant",
        _ => "dire",
    }))
}

fn safe_draft(match_data: &Value, engine_schema_31: bool) -> (Value, Value) {
    if !engine_schema_31 {
        return (Value::Null, Value::from(ENGINE_REPARSE_REASON));
    }
    let meta = match_data.get("meta").unwrap_or(&Value::Null);
    let Some(entries) = meta.get("draft").and_then(Value::as_array) else {
        let reason = meta
            .get("draft_reason")
            .and_then(Value::as_str)
            .unwrap_or("draft is not present in the bundle");
        return (Value::Null, Value::from(reason));
    };
    let mut output = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(order) = entry.get("order").and_then(Value::as_u64) else {
            return (Value::Null, Value::from("draft order is invalid"));
        };
        let Some(team) = entry
            .get("team")
            .and_then(Value::as_str)
            .filter(|team| matches!(*team, "radiant" | "dire"))
        else {
            return (Value::Null, Value::from("draft team is invalid"));
        };
        let Some(kind) = entry
            .get("kind")
            .and_then(Value::as_str)
            .filter(|kind| matches!(*kind, "pick" | "ban"))
        else {
            return (Value::Null, Value::from("draft action is invalid"));
        };
        let hero = match entry.get("hero") {
            Some(Value::Null) => Value::Null,
            Some(value) => match hero_id(Some(value)) {
                Some(id) => Value::String(id),
                None => return (Value::Null, Value::from("draft hero is invalid")),
            },
            None => return (Value::Null, Value::from("draft hero is missing")),
        };
        output.push(json!({"order":order,"team":team,"kind":kind,"hero":hero}));
    }
    output.sort_by_key(|entry| {
        entry
            .get("order")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX)
    });
    (Value::Array(output), Value::Null)
}

fn viewer_json(viewer_id: Option<&str>, gsi: Option<&MatchEntry>) -> Value {
    let Some(hero) = viewer_id else {
        return json!({"own_hero":null,"own_hero_reason":"no local GSI match or selected own hero"});
    };
    let kda = gsi.map(|entry| {
        [
            entry.kills.map(Value::from).unwrap_or(Value::Null),
            entry.deaths.map(Value::from).unwrap_or(Value::Null),
            entry.assists.map(Value::from).unwrap_or(Value::Null),
        ]
    });
    json!({
        "own_hero": hero,
        "source": if gsi.and_then(|entry|entry.hero.as_deref()).and_then(normalize_hero_id).as_deref() == Some(hero) { "gsi" } else { "user" },
        "kda": kda.clone().unwrap_or([Value::Null,Value::Null,Value::Null]),
        "kda_reason": kda.map(|values| values.map(|value|value.is_null().then_some(Value::from("local GSI field is unavailable")).unwrap_or(Value::Null))).unwrap_or([Value::from("local GSI history is unavailable"),Value::from("local GSI history is unavailable"),Value::from("local GSI history is unavailable")])
    })
}

fn death_index(deaths: &[DeathSample], heroes: &BTreeMap<String, HeroInfo<'_>>) -> Vec<Value> {
    deaths.iter().map(|death| {
        let mut row = json!({
            "event_ref": format!("deaths/{}", death.index),
            "time_s": number_json(death.time),
            "hero": death.hero_id,
            "hero_name": heroes.get(&death.hero_id).map(|hero|hero.name.as_str()),
            "killer": death.killer_id,
            "killer_kind": death.killer_kind,
            "killer_name": death.killer_id.as_deref().and_then(|id|heroes.get(id)).map(|hero|hero.name.as_str()),
            "position": death.position.map(|point|json!([number_json(point.x),number_json(point.y)])).unwrap_or(Value::Null)
        });
        if death.position.is_none() { row["position_reason"] = Value::from("no matching hero position was sampled near the death") }
        if death.killer_id.is_none() { row["killer_reason"] = Value::from("killer is not an identified roster hero") }
        row
    }).collect()
}

type PlayerGoldMap = BTreeMap<String, BTreeMap<String, u64>>;
type PlayerCompleteMap = BTreeMap<String, bool>;
type NeutralGoldByCamp = BTreeMap<String, BTreeMap<u64, u64>>;

fn zero_gold_by_enemy(heroes: &BTreeMap<String, HeroInfo<'_>>) -> PlayerGoldMap {
    heroes
        .values()
        .map(|player| {
            let enemies = heroes
                .values()
                .filter(|enemy| enemy.side != player.side)
                .map(|enemy| (enemy.id.clone(), 0))
                .collect();
            (player.id.clone(), enemies)
        })
        .collect()
}

fn all_players_complete(heroes: &BTreeMap<String, HeroInfo<'_>>) -> PlayerCompleteMap {
    heroes.keys().map(|id| (id.clone(), true)).collect()
}

fn opponent_gold_field(
    values: &PlayerGoldMap,
    complete_by_player: &PlayerCompleteMap,
    other_by_player: Option<&BTreeMap<String, u64>>,
) -> Value {
    let complete = complete_by_player.values().all(|value| *value);
    let by_player = values
        .iter()
        .map(|(player_id, values)| {
            let value = if complete_by_player.get(player_id) == Some(&true) {
                json!(values)
            } else {
                Value::Null
            };
            (player_id.clone(), value)
        })
        .collect::<Map<_, _>>();
    let mut field = Map::new();
    field.insert("complete".into(), json!(complete));
    field.insert("complete_by_player_hero_id".into(), json!(complete_by_player));
    field.insert("by_player_hero_id".into(), Value::Object(by_player));
    if let Some(other) = other_by_player {
        let other = other
            .iter()
            .map(|(player_id, gold)| {
                let value = if complete_by_player.get(player_id) == Some(&true) {
                    json!(gold)
                } else {
                    Value::Null
                };
                (player_id.clone(), value)
            })
            .collect();
        field.insert("other_by_player_hero_id".into(), Value::Object(other));
    }
    Value::Object(field)
}

fn add_gold(total: &mut u64, amount: u64) -> bool {
    let Some(sum) = total.checked_add(amount) else {
        return false;
    };
    *total = sum;
    true
}

fn aggregate_kill_gold_by_victim(
    match_data: &Value,
    heroes: &BTreeMap<String, HeroInfo<'_>>,
) -> Value {
    if match_data.pointer("/meta/gold_unattached").and_then(Value::as_u64) != Some(0) {
        return Value::Null;
    }
    let Some(deaths) = match_data.get("deaths").and_then(Value::as_array) else {
        return Value::Null;
    };
    let mut values = zero_gold_by_enemy(heroes);
    let mut complete = all_players_complete(heroes);
    for death in deaths {
        let Some(victim_id) = hero_id(death.get("hero")) else {
            return Value::Null;
        };
        let Some(victim) = heroes.get(&victim_id) else {
            return Value::Null;
        };
        let Some(bounty) = death.get("bounty").and_then(Value::as_object) else {
            for player in heroes.values().filter(|player| player.side != victim.side) {
                complete.insert(player.id.clone(), false);
            }
            continue;
        };
        for player in heroes.values().filter(|player| player.side != victim.side) {
            let amount = match bounty.get(&player.id) {
                None => 0,
                Some(value) => match value.as_u64() {
                    Some(value) => value,
                    None => {
                        complete.insert(player.id.clone(), false);
                        continue;
                    }
                },
            };
            let Some(total) = values
                .get_mut(&player.id)
                .and_then(|enemies| enemies.get_mut(&victim.id))
            else {
                complete.insert(player.id.clone(), false);
                continue;
            };
            if !add_gold(total, amount) {
                complete.insert(player.id.clone(), false);
            }
        }
    }
    opponent_gold_field(&values, &complete, None)
}

fn aggregate_gold_lost_by_killer(
    match_data: &Value,
    heroes: &BTreeMap<String, HeroInfo<'_>>,
) -> Value {
    let Some(deaths) = match_data.get("deaths").and_then(Value::as_array) else {
        return Value::Null;
    };
    let mut values = zero_gold_by_enemy(heroes);
    let mut other = heroes.keys().map(|id| (id.clone(), 0)).collect::<BTreeMap<_, _>>();
    let mut complete = all_players_complete(heroes);
    for death in deaths {
        let Some(victim_id) = hero_id(death.get("hero")) else {
            return Value::Null;
        };
        let Some(victim) = heroes.get(&victim_id) else {
            return Value::Null;
        };
        let Some(gold_lost) = death.get("gold_lost").and_then(Value::as_u64) else {
            complete.insert(victim_id, false);
            continue;
        };
        let killer_id = death.get("killer").and_then(Value::as_str);
        if let Some(killer) = killer_id.and_then(|id| heroes.get(id)) {
            if killer.side != victim.side {
                let Some(total) = values
                    .get_mut(&victim.id)
                    .and_then(|enemies| enemies.get_mut(&killer.id))
                else {
                    complete.insert(victim.id.clone(), false);
                    continue;
                };
                if !add_gold(total, gold_lost) {
                    complete.insert(victim.id.clone(), false);
                }
            } else {
                let Some(total) = other.get_mut(&victim.id) else {
                    complete.insert(victim.id.clone(), false);
                    continue;
                };
                if !add_gold(total, gold_lost) {
                    complete.insert(victim.id.clone(), false);
                }
            }
        } else if killer_id.is_none()
            || killer_id.is_some_and(|id| hero_id(Some(&Value::String(id.to_owned()))).is_none())
        {
            if let Some(total) = other.get_mut(&victim.id) {
                if !add_gold(total, gold_lost) {
                    complete.insert(victim.id.clone(), false);
                }
            }
        } else {
            // A hero-shaped killer missing from the roster has unknown side; do not guess.
            complete.insert(victim.id.clone(), false);
        }
    }
    opponent_gold_field(&values, &complete, Some(&other))
}

fn neutral_gold_by_camp(match_data: &Value) -> Option<NeutralGoldByCamp> {
    let raw_heroes = match_data.get("heroes")?.as_array()?;
    let mut sums = NeutralGoldByCamp::new();
    for hero in raw_heroes {
        let id = hero_id(hero.get("id"))?;
        let entries = match hero.get("neut_gold")? {
            Value::Null => continue,
            Value::Array(entries) => entries,
            _ => return None,
        };
        for entry in entries {
            let row = entry.as_array()?;
            if row.len() != 3 || number(row.first()).is_none() {
                return None;
            }
            let camp_value = row.get(1)?;
            let gold = row.get(2)?.as_u64()?;
            if camp_value.is_null() {
                continue;
            }
            let camp = camp_value.as_u64()?;
            let total = sums.entry(id.clone()).or_default().entry(camp).or_default();
            if !add_gold(total, gold) {
                return None;
            }
        }
    }
    Some(sums)
}

fn neutral_gold_complete_by_player(
    match_data: &Value,
    heroes: &BTreeMap<String, HeroInfo<'_>>,
) -> PlayerCompleteMap {
    let rows = match_data.get("heroes").and_then(Value::as_array);
    heroes
        .keys()
        .map(|id| {
            let complete = rows.is_some_and(|rows| {
                rows.iter().any(|hero| {
                    hero_id(hero.get("id")).as_deref() == Some(id.as_str())
                        && hero.get("neut_gold").is_some_and(Value::is_array)
                })
            });
            (id.clone(), complete)
        })
        .collect()
}

fn unattached_neutral_gold_by_player(match_data: &Value) -> Option<BTreeMap<String, u64>> {
    let mut unattached = BTreeMap::new();
    for hero in match_data.get("heroes")?.as_array()? {
        let id = hero_id(hero.get("id"))?;
        unattached.insert(id, hero.get("neut_gold_unattached")?.as_u64()?);
    }
    Some(unattached)
}

fn neutral_gold_by_zone(
    match_data: &Value,
    camp_gold: &NeutralGoldByCamp,
) -> PlayerGoldMap {
    let bundle_camps = match_data.pointer("/meta/camps");
    let mut zones: PlayerGoldMap = BTreeMap::new();
    for (player_id, camps) in camp_gold {
        for (camp_id, gold) in camps {
            let Some(bundle_camps) = bundle_camps else {
                continue;
            };
            let Some(zone) = zone_for_camp(*camp_id, bundle_camps) else {
                continue;
            };
            let total = zones
                .entry(player_id.clone())
                .or_default()
                .entry(zone.as_str().to_owned())
                .or_insert(0);
            let _ = add_gold(total, *gold);
        }
    }
    zones
}

fn neutral_gold_zone_field(
    match_data: &Value,
    heroes: &BTreeMap<String, HeroInfo<'_>>,
    camp_gold: &NeutralGoldByCamp,
) -> Value {
    let zone_gold = neutral_gold_by_zone(match_data, camp_gold);
    let complete = neutral_gold_complete_by_player(match_data, heroes);
    let mut field = opponent_gold_field(&zone_gold, &complete, None)
        .as_object()
        .cloned()
        .unwrap_or_default();
    field.insert("zone_names".into(), json!(zone_names()));
    let unattached = unattached_neutral_gold_by_player(match_data).unwrap_or_default();
    field.insert("unattached_by_player_hero_id".into(), json!(unattached));
    Value::Object(field)
}

fn opponent_stats(manifest: &Value, match_data: &Value, heroes: &BTreeMap<String, HeroInfo<'_>>) -> Value {
    if manifest.pointer("/schema").and_then(Value::as_str) != Some("3.3")
        || match_data.pointer("/meta/schema").and_then(Value::as_str) != Some("3.3")
    {
        return Value::Null;
    }
    let Some(meta) = match_data.get("meta").and_then(Value::as_object) else {
        return Value::Null;
    };
    let Some(deaths) = match_data.get("deaths").and_then(Value::as_array) else {
        return Value::Null;
    };
    let Some(raw_heroes) = match_data.get("heroes").and_then(Value::as_array) else {
        return Value::Null;
    };
    let has_v33_fields = meta.get("gold_unattached").and_then(Value::as_u64).is_some()
        && meta.get("neut_gold_unattached").and_then(Value::as_u64).is_some()
        && meta.contains_key("camps")
        && deaths.iter().all(|death| death.get("bounty").is_some() && death.get("gold_lost").is_some())
        && raw_heroes.iter().all(|hero| {
            hero.get("neut_gold").is_some()
                && hero.get("neut_gold_unattached").and_then(Value::as_u64).is_some()
        });
    if !has_v33_fields {
        return Value::Null;
    }
    let Some(camp_gold) = neutral_gold_by_camp(match_data) else {
        return Value::Null;
    };
    json!({
        "kill_gold_by_victim": aggregate_kill_gold_by_victim(match_data, heroes),
        "gold_lost_by_killer": aggregate_gold_lost_by_killer(match_data, heroes),
        "neutral_kill_gold_by_camp": neutral_gold_zone_field(match_data, heroes, &camp_gold)
    })
}

fn teamfights(heroes: &[HeroInfo<'_>], deaths: &[DeathSample], t0: f64) -> Vec<Value> {
    let mut sorted = deaths.to_vec();
    sorted.sort_by(|left, right| {
        left.time
            .total_cmp(&right.time)
            .then(left.index.cmp(&right.index))
    });
    let mut clusters: Vec<Vec<DeathSample>> = Vec::new();
    for death in sorted {
        let can_join = clusters
            .last()
            .and_then(|cluster| cluster.last())
            .is_some_and(|previous| {
                death.time - previous.time
                    <= lane_expectation::config_number("/teamfight/death_gap_s")
                    && death
                        .position
                        .zip(previous.position)
                        .is_some_and(|(left, right)| {
                            distance(left, right)
                                <= lane_expectation::config_number("/teamfight/death_distance")
                        })
            });
        if can_join {
            if let Some(cluster) = clusters.last_mut() {
                cluster.push(death);
            }
        } else {
            clusters.push(vec![death]);
        }
    }
    clusters.into_iter().filter(|cluster| cluster.len() >= lane_expectation::config_integer("/teamfight/minimum_deaths")).enumerate().map(|(index, cluster)| {
        let first = cluster.first().map(|death| death.time).unwrap_or_default();
        let last = cluster.last().map(|death| death.time).unwrap_or(first);
        let points = cluster.iter().filter_map(|death| death.position).collect::<Vec<_>>();
        let center = if points.is_empty() { None } else { Some(Point { x: points.iter().map(|p|p.x).sum::<f64>() / points.len() as f64, y: points.iter().map(|p|p.y).sum::<f64>() / points.len() as f64 }) };
        let start = first - lane_expectation::config_signed_integer("/teamfight/window_before_s") as f64;
        let stop = last + lane_expectation::config_signed_integer("/teamfight/window_after_s") as f64;
        let present = center.map(|center| heroes.iter().filter(|hero| {
            let Some(alive) = hero.source.get("alive").and_then(Value::as_array) else { return false };
            let start_idx = sample_index(t0, start.max(0.0), alive.len()).unwrap_or(0);
            let end_idx = sample_index(t0, stop, alive.len()).unwrap_or_else(||alive.len().saturating_sub(1));
            (start_idx..=end_idx).any(|i| {
                let time = t0 + i as f64;
                alive_at(hero.source,time,t0) == Some(true) && position_at(hero.source,time,t0).is_some_and(|point|distance(point,center) <= lane_expectation::config_number("/teamfight/presence_distance"))
            })
        }).map(|hero|hero.id.clone()).collect::<Vec<_>>());
        let nw_start = team_diff(heroes,start.max(0.0),t0).and_then(|value|value.0);
        let nw_end = team_diff(heroes,stop,t0).and_then(|value|value.0);
        let mut fight = json!({
            "n": index + 1,
            "t": [number_json(first),number_json(last)],
            "window": [number_json(start),number_json(stop)],
            "center": center.map(|point|json!([number_json(point.x),number_json(point.y)])).unwrap_or(Value::Null),
            "deaths": cluster.iter().map(|death|death.index).collect::<Vec<_>>(),
            "radiant_deaths": cluster.iter().filter(|death|heroes.iter().find(|hero|hero.id == death.hero_id).is_some_and(|hero|hero.side == "radiant")).count(),
            "dire_deaths": cluster.iter().filter(|death|heroes.iter().find(|hero|hero.id == death.hero_id).is_some_and(|hero|hero.side == "dire")).count(),
            "present": present.as_ref().map(|ids|json!(ids)).unwrap_or(Value::Null),
            "nw_swing_radiant": nw_start.zip(nw_end).map(|(start,end)|number_json(end-start)).unwrap_or(Value::Null)
        });
        if center.is_none() { fight["center_reason"] = Value::from("death positions are missing") }
        if present.is_none() { fight["present_reason"] = Value::from("teamfight positions are missing") }
        if nw_start.is_none() || nw_end.is_none() { fight["nw_swing_radiant_reason"] = Value::from("team net-worth samples are missing at the fight window") }
        fight
    }).collect()
}

fn distance(left: Point, right: Point) -> f64 {
    ((left.x - right.x).powi(2) + (left.y - right.y).powi(2)).sqrt()
}

fn objectives(world: &Value, heroes: &BTreeMap<String, HeroInfo<'_>>) -> Value {
    let tracks = world.get("tracks").and_then(Value::as_array);
    let mut buildings = Vec::new();
    let mut wards = Vec::new();
    let mut other = Vec::new();
    if let Some(tracks) = tracks {
        for track in tracks {
            let kind = track.get("k").and_then(Value::as_str).unwrap_or("");
            let position = track_position(track);
            let mut entry = json!({
                "kind": kind,
                "team": side(track.get("team")).map(Value::from).unwrap_or(Value::Null),
                "start_s": number(track.get("t0")).map(number_json).unwrap_or(Value::Null),
                "end_s": number(track.get("t1")).map(number_json).unwrap_or(Value::Null),
                "name": track.get("n").and_then(Value::as_str).filter(|name|!name.contains('/') && name.len() <= 80),
                "position": position
            });
            for (field, reason) in [
                ("team", "objective team is missing"),
                ("start_s", "objective start time is missing"),
                ("end_s", "objective end time is missing"),
                ("name", "objective name is missing"),
                ("position", "objective position is missing"),
            ] {
                if entry[field].is_null() {
                    entry[format!("{field}_reason")] = Value::from(reason);
                }
            }
            if kind == "bld" {
                let last_hp = last_track_hp(track);
                let destroyed = last_hp.map(|hp| hp <= 10.0);
                entry["destroyed"] = destroyed.map(Value::from).unwrap_or(Value::Null);
                entry["destroyed_reason"] = destroyed
                    .is_none()
                    .then_some("building health samples are missing")
                    .map(Value::from)
                    .unwrap_or(Value::Null);
                let destroyed_at = destroyed
                    .filter(|destroyed| *destroyed)
                    .and_then(|_| number(track.get("t1")));
                entry["destroyed_at_s"] = destroyed_at.map(number_json).unwrap_or(Value::Null);
                entry["destroyed_at_s_reason"] = if destroyed == Some(false) {
                    Value::from("building is not destroyed")
                } else {
                    destroyed_at
                        .is_none()
                        .then_some("building destruction time is missing")
                        .map(Value::from)
                        .unwrap_or(Value::Null)
                };
            }
            match kind {
                "bld" => buildings.push(entry),
                "ward" => wards.push(entry),
                "rosh" | "torm" => other.push(json!({
                    "kind": kind,
                    "start_s": number(track.get("t0")).map(number_json).unwrap_or(Value::Null),
                    "end_s": number(track.get("t1")).map(number_json).unwrap_or(Value::Null),
                    "result": null,
                    "result_reason": "kill versus despawn is not established by the track"
                })),
                _ => {}
            }
        }
    }
    let runes = world.get("runes").and_then(Value::as_array).map(|runes| {
        runes
            .iter()
            .map(|rune| {
                let by = rune
                    .get("by")
                    .and_then(Value::as_str)
                    .and_then(|id| hero_id(Some(&Value::String(id.to_owned()))))
                    .filter(|id| heroes.contains_key(id));
                let mut item = json!({
                    "t": number(rune.get("t0")).map(number_json).unwrap_or(Value::Null),
                    "type": rune.get("type").and_then(Value::as_str),
                    "outcome": rune.get("out").and_then(Value::as_str),
                    "by": by
                });
                if item["t"].is_null() {
                    item["t_reason"] = Value::from("rune time is missing")
                }
                if item["type"].is_null() {
                    item["type_reason"] = Value::from("rune type is missing")
                }
                if item["outcome"].is_null() {
                    item["outcome_reason"] = Value::from("rune outcome is missing")
                }
                if item["by"].is_null() {
                    item["by_reason"] = Value::from("taker is not an identified roster hero")
                }
                item
            })
            .collect::<Vec<_>>()
    });
    let tracks_missing = tracks.is_none();
    json!({
        "buildings": if tracks_missing { Value::Null } else { json!(buildings) },
        "buildings_reason": tracks_missing.then_some("objective tracks are missing").map(Value::from),
        "wards": if tracks_missing { Value::Null } else { json!(wards) },
        "wards_reason": tracks_missing.then_some("objective tracks are missing").map(Value::from),
        "other_tracks": if tracks_missing { Value::Null } else { json!(other) },
        "other_tracks_reason": tracks_missing.then_some("objective tracks are missing").map(Value::from),
        "runes": runes.clone().map_or(Value::Null, Value::Array),
        "runes_reason": runes.is_none().then_some("rune tracks are missing").map(Value::from)
    })
}

fn last_track_hp(track: &Value) -> Option<f64> {
    let samples = track.get("hp")?.as_array()?;
    let last = samples.last()?;
    let sample = last
        .as_array()
        .and_then(|row| row.get(1))
        .or_else(|| last.is_number().then_some(last));
    number(sample)
}

fn track_position(track: &Value) -> Option<Value> {
    let coordinates = track.get("p")?.as_array()?;
    let times = coordinates.first()?.as_array()?;
    let xs = coordinates.get(1)?.as_array()?;
    let ys = coordinates.get(2)?.as_array()?;
    let target = number(track.get("t1")).or_else(|| number(track.get("t0")))?;
    let index = times
        .iter()
        .enumerate()
        .filter_map(|(index, time)| number(Some(time)).map(|time| (index, (time - target).abs())))
        .min_by(|left, right| left.1.total_cmp(&right.1))?
        .0;
    Some(json!([
        number_json(number(xs.get(index))?),
        number_json(number(ys.get(index))?)
    ]))
}

fn thresholds_json() -> Value {
    lane_expectation::provisional_thresholds()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_history::{MatchEnd, MatchResult, MatchType};
    use serde_json::json;

    fn fixture() -> (Value, Value, Value) {
        let manifest = json!({
            "schema":"3.1",
            "files": {
                "match.json":{"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                "world.json":{"sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
            },
            "path":"F:/synthetic/private/replay"
        });
        let match_data = json!({
            "t0":0,"t1":2,"meta":{"schema":"3.1","duration_s":2,"patch":"synthetic","game_mode":2,"winner":null,"winner_reason":"winner not in replay","draft":null,"draft_reason":"draft not recorded"},
            "heroes":[{"id":"npc_dota_hero_axe","name":"Axe","team":2,"pid":0,"player_name":"private-name","account_id":123456}],
            "deaths":[]
        });
        let world = json!({"heroes":{},"tracks":[],"runes":[]});
        (manifest, match_data, world)
    }

    fn schema_33_opponents_fixture() -> (Value, Value, Value) {
        let (mut manifest, mut match_data, world) = fixture();
        manifest["schema"] = json!("3.3");
        match_data["meta"]["schema"] = json!("3.3");
        match_data["meta"]["gold_unattached"] = json!(0);
        match_data["meta"]["neut_gold_unattached"] = json!(1);
        match_data["meta"]["camps"] = json!([
            {"id":1,"x":-8313,"y":-553,"type":"ancient"},
            {"id":2,"x":-5024,"y":-96,"type":"small"}
        ]);
        match_data["heroes"] = json!([
            {"id":"npc_dota_hero_axe","name":"Axe","team":2,"pid":0,"neut_gold":[[10,1,100],[11,null,25],[13,2,200]],"neut_gold_unattached":1},
            {"id":"npc_dota_hero_lina","name":"Lina","team":3,"pid":5,"neut_gold":[[12,2,50]],"neut_gold_unattached":0}
        ]);
        match_data["deaths"] = json!([
            {"t":10,"hero":"npc_dota_hero_lina","killer":"npc_dota_hero_axe","bounty":{"npc_dota_hero_axe":250},"gold_lost":300},
            {"t":20,"hero":"npc_dota_hero_axe","killer":"npc_dota_hero_lina","bounty":{"npc_dota_hero_lina":220},"gold_lost":410},
            {"t":30,"hero":"npc_dota_hero_axe","killer":"npc_dota_goodguys_tower1_top","bounty":{},"gold_lost":160},
            {"t":40,"hero":"npc_dota_hero_axe","killer":"npc_dota_hero_axe","bounty":{},"gold_lost":70}
        ]);
        (manifest, match_data, world)
    }

    fn synthetic_digest() -> Value {
        let (manifest, match_data, world) = fixture();
        build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap()
    }

    fn schema_31_fixture() -> (Value, Value, Value) {
        let (mut manifest, mut match_data, world) = fixture();
        match_data["t1"] = json!(720);
        match_data["meta"]["duration_s"] = json!(720);
        match_data["meta"]["winner"] = json!("radiant");
        match_data["meta"]
            .as_object_mut()
            .unwrap()
            .remove("winner_reason");
        match_data["meta"]["draft"] = json!([
            {"order":0,"team":"radiant","kind":"pick","hero":"npc_dota_hero_axe"},
            {"order":1,"team":"dire","kind":"ban","hero":"npc_dota_hero_tiny"}
        ]);
        match_data["meta"]
            .as_object_mut()
            .unwrap()
            .remove("draft_reason");
        match_data["heroes"][0]["gold_earned"] = json!([100, 1000, 1200]);
        match_data["heroes"][0]["gold_earned_final"] = json!(1200);
        match_data["heroes"][0]["dmg_out"] = json!({"total":5000,"p":2500,"m":2000,"u":500});
        manifest["schema"] = json!("3.1");
        (manifest, match_data, world)
    }

    #[test]
    fn unavailable_engine_fields_are_null_with_reasons() {
        let manifest = json!({});
        let match_data = json!({"meta":{"duration_s":600,"patch":"7.41f"},"heroes":[],"deaths":[]});
        let world = json!({"heroes":{},"tracks":[],"runes":[]});
        let digest = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();
        assert!(digest["meta"]["result"]["winner"].is_null());
        assert_eq!(digest["meta"]["result"]["reason"], ENGINE_REPARSE_REASON);
        assert!(digest["meta"]["draft"].is_null());
        assert!(digest["meta"]["draft_reason"].as_str().is_some());
    }

    #[test]
    fn schema_33_opponents_aggregates_are_computed_from_event_fields() {
        let (manifest, match_data, world) = schema_33_opponents_fixture();
        let digest = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();

        let stats = &digest["opponent_stats"];
        assert_eq!(stats["kill_gold_by_victim"]["complete_by_player_hero_id"]["npc_dota_hero_axe"], true);
        assert_eq!(stats["kill_gold_by_victim"]["by_player_hero_id"]["npc_dota_hero_axe"]["npc_dota_hero_lina"], 250);
        assert_eq!(stats["gold_lost_by_killer"]["complete_by_player_hero_id"]["npc_dota_hero_axe"], true);
        assert_eq!(stats["gold_lost_by_killer"]["by_player_hero_id"]["npc_dota_hero_axe"]["npc_dota_hero_lina"], 410);
        assert_eq!(stats["gold_lost_by_killer"]["other_by_player_hero_id"]["npc_dota_hero_axe"], 230);
        assert_eq!(stats["neutral_kill_gold_by_camp"]["complete_by_player_hero_id"]["npc_dota_hero_axe"], true);
        assert_eq!(stats["neutral_kill_gold_by_camp"]["by_player_hero_id"]["npc_dota_hero_axe"]["R1"], 200);
        assert_eq!(stats["neutral_kill_gold_by_camp"]["by_player_hero_id"]["npc_dota_hero_axe"]["R2"], 100);
        assert_eq!(stats["neutral_kill_gold_by_camp"]["zone_names"]["R1"], "ป่า Radiant 1 (R1)");
        assert_eq!(stats["neutral_kill_gold_by_camp"]["unattached_by_player_hero_id"]["npc_dota_hero_axe"], 1);
        let camp_sums = neutral_gold_by_camp(&match_data).unwrap();
        assert_eq!(camp_sums["npc_dota_hero_axe"][&1], 100);
        assert_eq!(camp_sums["npc_dota_hero_axe"][&2], 200);
        assert_eq!(camp_sums["npc_dota_hero_lina"][&2], 50);
        assert_eq!(digest["deaths"][0]["killer_kind"], "hero");
        assert_eq!(digest["deaths"][2]["killer_kind"], "other");
    }

    #[test]
    fn null_bounty_makes_the_affected_player_kill_gold_unknown() {
        let (manifest, mut match_data, world) = schema_33_opponents_fixture();
        match_data["deaths"][0]["bounty"] = Value::Null;
        let digest = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();

        let kill_gold = &digest["opponent_stats"]["kill_gold_by_victim"];
        assert_eq!(kill_gold["complete_by_player_hero_id"]["npc_dota_hero_axe"], false);
        assert!(kill_gold["by_player_hero_id"]["npc_dota_hero_axe"].is_null());
        assert_eq!(digest["opponent_stats"]["gold_lost_by_killer"]["complete_by_player_hero_id"]["npc_dota_hero_axe"], true);
    }

    #[test]
    fn schema_32_bundles_keep_opponent_stats_null() {
        let (mut manifest, mut match_data, world) = fixture();
        manifest["schema"] = json!("3.2");
        match_data["meta"]["schema"] = json!("3.2");
        let digest = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();
        assert!(digest["opponent_stats"].is_null());
    }

    #[test]
    fn jungle_gold_selects_the_max_zone_and_does_not_count_unmapped_rows() {
        let (manifest, match_data, world) = schema_33_opponents_fixture();
        let digest = build(&KnowledgeInput { manifest: &manifest, match_data: &match_data, world: &world }).unwrap();
        let field = &digest["opponent_stats"]["neutral_kill_gold_by_camp"];
        let own = &field["by_player_hero_id"]["npc_dota_hero_axe"];
        let (zone, gold) = own.as_object().unwrap().iter().max_by_key(|(_, gold)| gold.as_u64().unwrap()).unwrap();
        assert_eq!(zone, "R1");
        assert_eq!(gold, &json!(200));
        assert_eq!(own.as_object().unwrap().len(), 2, "camp:null must not become a zone");
        assert_eq!(field["unattached_by_player_hero_id"]["npc_dota_hero_axe"], 1);
    }

    #[test]
    fn null_neutral_rows_are_unknown_and_attached_null_camp_rows_are_excluded() {
        let (manifest, mut match_data, world) = schema_33_opponents_fixture();
        match_data["heroes"][0]["neut_gold"] = Value::Null;
        match_data["heroes"][0]["neut_gold_reason"] = json!("neutral gold unavailable");
        let digest = build(&KnowledgeInput { manifest: &manifest, match_data: &match_data, world: &world }).unwrap();
        let field = &digest["opponent_stats"]["neutral_kill_gold_by_camp"];
        assert_eq!(field["complete_by_player_hero_id"]["npc_dota_hero_axe"], false);
        assert!(field["by_player_hero_id"]["npc_dota_hero_axe"].is_null());
        assert_eq!(field["unattached_by_player_hero_id"]["npc_dota_hero_axe"], 1);

        let camp_sums = neutral_gold_by_camp(&schema_33_opponents_fixture().1).unwrap();
        assert_eq!(camp_sums["npc_dota_hero_axe"], BTreeMap::from([(1, 100), (2, 200)]));
    }

    #[test]
    fn cached_digest_bytes_do_not_change_with_personal_lane_history_or_own_hero() {
        let (manifest, match_data, world) = fixture();
        let input = KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        };
        let plain = build_with_sources(&input, None, &[], &[], &[], None).unwrap();
        let personal_lane_history = vec![LaneHistoryRecord {
            patch: Some("synthetic".into()),
            game_mode: Some(2),
            lane: "top".into(),
            shape: "1v1".into(),
            radiant: vec![super::super::lane_history::LaneHeroRecord {
                hero: "npc_dota_hero_axe".into(),
                role: Some("offlane".into()),
            }],
            dire: vec![],
            score: Some(7.0),
            score_reason: None,
            constants_version: 0,
            own_lane: true,
        }];
        let with_personal_history = build_with_sources(
            &input,
            None,
            &[],
            &[],
            &personal_lane_history,
            Some("npc_dota_hero_axe"),
        )
        .unwrap();

        assert_eq!(
            canonical_bytes(&plain).unwrap(),
            canonical_bytes(&with_personal_history).unwrap()
        );
        assert!(plain.pointer("/meta/result/own_side").is_none());
        assert!(plain.pointer("/meta/result/own_result").is_none());
        assert!(plain
            .get("lanes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .all(|lane| lane.get("personal_average").is_none()));
    }

    #[test]
    fn local_gsi_result_is_resolved_after_the_engine_result() {
        let (manifest, match_data, world) = fixture();
        let gsi = history_entry("key", Some(MatchResult::Win), "kohbor", "npc_dota_hero_axe");
        let digest = build_with_sources(
            &KnowledgeInput {
                manifest: &manifest,
                match_data: &match_data,
                world: &world,
            },
            Some("key"),
            &[gsi],
            &[],
            &[],
            Some("npc_dota_hero_axe"),
        )
        .unwrap();
        assert_eq!(digest["meta"]["result"]["winner"], "radiant");
        assert_eq!(digest["meta"]["result"]["source"], "gsi_history");
        assert!(digest["meta"]["result"].get("own_side").is_none());
        assert!(digest["meta"]["result"].get("own_result").is_none());
        assert_eq!(
            session_viewer(
                &digest,
                Some("npc_dota_hero_axe"),
                Some(&history_entry(
                    "key",
                    Some(MatchResult::Win),
                    "kohbor",
                    "npc_dota_hero_axe",
                ))
            )["own_result"],
            "win"
        );
        assert!(digest["meta"].get("viewer").is_none());
        assert!(digest["heroes"][0]["final"]["a"].is_null());
        assert_eq!(
            digest["heroes"][0]["final"]["a_reason"],
            "exact assists are not in the bundle"
        );
    }

    fn history_entry(
        match_key: &str,
        result: Option<MatchResult>,
        source: &str,
        hero: &str,
    ) -> MatchEntry {
        MatchEntry {
            match_key: match_key.to_owned(),
            hero: Some(hero.to_owned()),
            result,
            end: if result.is_some() {
                MatchEnd::Completed
            } else {
                MatchEnd::Abandoned
            },
            kills: None,
            deaths: None,
            assists: None,
            duration_s: None,
            ended_at: 1,
            started_at: None,
            hero_id: None,
            match_type: MatchType::Unknown,
            source: source.to_owned(),
        }
    }

    #[test]
    fn match_result_uses_source_order_and_keeps_abandoned_or_unjoined_results_unknown() {
        let (manifest, match_data, world) = fixture();
        let input = KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        };
        let own = Some("npc_dota_hero_axe");
        let gsi_win = history_entry("key", Some(MatchResult::Win), "kohbor", own.unwrap());
        let gsi_loss = history_entry("key", Some(MatchResult::Loss), "kohbor", own.unwrap());
        let open_win = history_entry("key", Some(MatchResult::Win), "opendota", own.unwrap());
        let engine_win = {
            let mut data = match_data.clone();
            data["meta"]["winner"] = json!("radiant");
            data["meta"]
                .as_object_mut()
                .unwrap()
                .remove("winner_reason");
            build_with_sources(
                &KnowledgeInput {
                    manifest: &manifest,
                    match_data: &data,
                    world: &world,
                },
                Some("key"),
                &[gsi_win.clone()],
                &[],
                &[],
                own,
            )
            .unwrap()
        };
        assert_eq!(engine_win["meta"]["result"]["winner"], "radiant");
        assert_eq!(engine_win["meta"]["result"]["source"], "engine");
        let loss = build_with_sources(&input, Some("key"), &[gsi_loss], &[], &[], own).unwrap();
        assert_eq!(loss["meta"]["result"]["winner"], "dire");
        let open = build_with_sources(&input, Some("key"), &[], &[open_win], &[], own).unwrap();
        assert_eq!(open["meta"]["result"]["source"], "opendota_history");
        assert_eq!(open["meta"]["result"]["winner"], "radiant");
        let abandoned = history_entry("key", None, "kohbor", own.unwrap());
        let mut no_engine_winner = match_data.clone();
        no_engine_winner["meta"]["winner"] = Value::Null;
        no_engine_winner["meta"]["winner_reason"] = json!("winner not in replay");
        let unknown = build_with_sources(
            &KnowledgeInput {
                manifest: &manifest,
                match_data: &no_engine_winner,
                world: &world,
            },
            Some("key"),
            &[abandoned],
            &[],
            &[],
            own,
        )
        .unwrap();
        assert!(unknown["meta"]["result"]["winner"].is_null());
        assert_eq!(unknown["meta"]["result"]["reason"], "winner not in replay");
        let no_key = build_with_sources(&input, None, &[gsi_win], &[], &[], own).unwrap();
        assert!(no_key["meta"]["result"]["winner"].is_null());
    }

    #[test]
    fn match_result_conflicts_and_hero_mismatch_return_unknown_without_identifiers() {
        let (manifest, mut match_data, world) = fixture();
        match_data["meta"]["winner"] = json!("radiant");
        match_data["meta"]
            .as_object_mut()
            .unwrap()
            .remove("winner_reason");
        let input = KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        };
        let own = Some("npc_dota_hero_axe");
        let loss = history_entry(
            "raw-match-key",
            Some(MatchResult::Loss),
            "kohbor",
            own.unwrap(),
        );
        let conflict =
            build_with_sources(&input, Some("raw-match-key"), &[loss], &[], &[], own).unwrap();
        assert!(conflict["meta"]["result"]["winner"].is_null());
        assert_eq!(conflict["meta"]["result"]["conflict"], true);
        assert_eq!(conflict["meta"]["result"]["reason"], "sources_disagree");
        let mismatch = history_entry(
            "raw-match-key",
            Some(MatchResult::Win),
            "kohbor",
            "npc_dota_hero_lina",
        );
        let digest =
            build_with_sources(&input, Some("raw-match-key"), &[mismatch], &[], &[], own).unwrap();
        assert!(digest["meta"]["result"]["winner"].is_null());
        assert_eq!(digest["meta"]["result"]["reason"], "history_hero_mismatch");
        let bytes = canonical_bytes(&digest).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("raw-match-key"));
        assert!(!text.contains("match_key"));
    }

    #[test]
    fn lane_analysis_is_independent_of_match_result_source() {
        let (manifest, match_data, world) = fixture();
        let mut radiant = match_data.clone();
        radiant["meta"]["winner"] = json!("radiant");
        radiant["meta"]
            .as_object_mut()
            .unwrap()
            .remove("winner_reason");
        let mut dire = match_data.clone();
        dire["meta"]["winner"] = json!("dire");
        dire["meta"]
            .as_object_mut()
            .unwrap()
            .remove("winner_reason");
        let a = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &radiant,
            world: &world,
        })
        .unwrap();
        let b = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &dire,
            world: &world,
        })
        .unwrap();
        assert_eq!(a["lanes"], b["lanes"]);
        let result_null = {
            let mut data = match_data.clone();
            data["meta"]["winner"] = Value::Null;
            build(&KnowledgeInput {
                manifest: &manifest,
                match_data: &data,
                world: &world,
            })
            .unwrap()
        };
        assert_eq!(a["lanes"], result_null["lanes"]);
    }

    #[test]
    fn threshold_table_is_one_labeled_source() {
        let value = thresholds_json();
        assert_eq!(value["label"], "เกณฑ์ชั่วคราว");
        assert_eq!(value["even_advantage_pct"], 5.0);
        assert_eq!(value["stomp_advantage_pct"], 15.0);
        assert_eq!(value["teamfight_gap_s"], 15.0);
    }

    #[test]
    fn turbo_lane_outcomes_remain_unknown_until_mode_boundaries_are_defined() {
        let (manifest, mut match_data, world) = fixture();
        match_data["meta"]["game_mode"] = json!(23);
        let digest = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();
        assert!(digest["lanes"][0]["outcome"].is_null());
        assert_eq!(
            digest["lanes"][0]["outcome_reason"],
            "lane boundaries are not established for this game mode"
        );
    }

    #[test]
    fn building_objectives_require_observed_health_and_keep_unknown_counts_explicit() {
        let heroes = BTreeMap::new();
        let world = json!({
            "tracks":[{"k":"bld","team":2,"t0":0,"t1":600,"p":[[600],[1],[2]],"hp":[[600,9]],"n":"tower"}],
            "runes":[]
        });
        let known = objectives(&world, &heroes);
        assert_eq!(known["buildings"][0]["destroyed"], true);
        assert_eq!(known["buildings"][0]["destroyed_at_s"], 600);
        assert_eq!(known["buildings"][0]["position"], json!([1, 2]));
        assert_eq!(phase_tower_losses(&known, "radiant", 0.0, 600.0).0, 1);

        let unknown_world = json!({
            "tracks":[{"k":"bld","team":2,"t0":0,"t1":600,"p":[[],[],[]],"n":"tower"}],
            "runes":[]
        });
        let unknown = objectives(&unknown_world, &heroes);
        assert!(unknown["buildings"][0]["destroyed"].is_null());
        assert!(unknown["buildings"][0]["destroyed_reason"].is_string());
        let (count, reason) = phase_tower_losses(&unknown, "radiant", 0.0, 600.0);
        assert!(count.is_null());
        assert_eq!(reason, Some("building destruction data is incomplete"));
    }

    #[test]
    fn synthetic_bundle_matches_golden_digest_bytes() {
        let digest = synthetic_digest();
        let bytes = canonical_bytes(&digest).unwrap();
        let hash = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            hash,
            "7ddb5828faa56cbbd0485cd550ad0e987c634b9fe422f24c35203175b70368a8"
        );
    }

    #[test]
    fn schema_31_digest_uses_engine_winner_draft_gold_earned_and_damage() {
        let (manifest, match_data, world) = schema_31_fixture();
        let digest = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();
        assert_eq!(digest["knowledge_version"], 6);
        assert_eq!(digest["meta"]["result"]["winner"], "radiant");
        assert_eq!(digest["meta"]["result"]["source"], "engine");
        assert_eq!(digest["meta"]["draft"], match_data["meta"]["draft"]);
        assert_eq!(digest["meta"]["draft_source"], "engine");
        assert_eq!(digest["heroes"][0]["gold_earned"], 1200);
        assert_eq!(digest["heroes"][0]["gold_earned_source"], "engine");
        assert_eq!(digest["heroes"][0]["rates"]["gpm"], 100);
        assert_eq!(digest["heroes"][0]["rates"]["gpm_source"], "engine");
        assert_eq!(digest["heroes"][0]["hero_damage"], 5000);
        assert_eq!(digest["heroes"][0]["hero_damage_source"], "engine");
        assert_eq!(
            digest["heroes"][0]["damage_out"],
            json!({"total":5000,"p":2500,"m":2000,"u":500})
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(canonical_bytes(&digest).unwrap())),
            "3551301ff28943ccfb7417ca085a7740c7e12e99a9421644ac6c90dfff24d01d"
        );
    }

    #[test]
    fn schema_30_digest_keeps_new_engine_fields_unknown_with_reparse_reasons() {
        let (mut manifest, mut match_data, world) = fixture();
        manifest["schema"] = json!("3.0");
        match_data["meta"]["schema"] = json!("3.0");
        for key in ["winner", "winner_reason", "draft", "draft_reason"] {
            match_data["meta"].as_object_mut().unwrap().remove(key);
        }
        let digest = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();
        let reparse = "re-parse with engine 0.3.0 to get this";
        assert!(digest["meta"]["result"]["winner"].is_null());
        assert_eq!(digest["meta"]["result"]["reason"], reparse);
        assert!(digest["meta"]["draft"].is_null());
        assert_eq!(digest["meta"]["draft_reason"], reparse);
        assert!(digest["heroes"][0]["rates"]["gpm"].is_null());
        assert_eq!(digest["heroes"][0]["rates"]["gpm_reason"], reparse);
        assert!(digest["heroes"][0]["gold_earned"].is_null());
        assert_eq!(digest["heroes"][0]["gold_earned_reason"], reparse);
        assert!(digest["heroes"][0]["hero_damage"].is_null());
        assert_eq!(digest["heroes"][0]["hero_damage_reason"], reparse);
        assert_eq!(digest["heroes"][0]["damage_out_reason"], reparse);
        assert_eq!(
            format!("{:x}", Sha256::digest(canonical_bytes(&digest).unwrap())),
            "cbdc6918d1c2507a526f655cce5a425fe80efc6ad423736e3398e183f08500e5"
        );
    }

    #[test]
    fn l0_engine_result_conflict_null_and_schema_30_fallback_are_explicit() {
        let (manifest, mut match_data, world) = schema_31_fixture();
        let own = Some("npc_dota_hero_axe");
        let gsi_win = history_entry("key", Some(MatchResult::Win), "kohbor", own.unwrap());
        let gsi_loss = history_entry("key", Some(MatchResult::Loss), "kohbor", own.unwrap());
        let build_for = |data: &Value, match_key: Option<&str>, history: &[MatchEntry]| {
            build_with_sources(
                &KnowledgeInput {
                    manifest: &manifest,
                    match_data: data,
                    world: &world,
                },
                match_key,
                history,
                &[],
                &[],
                own,
            )
            .unwrap()
        };
        let agrees = build_for(&match_data, Some("key"), &[gsi_win.clone()]);
        assert_eq!(agrees["meta"]["result"]["winner"], "radiant");
        assert_eq!(agrees["meta"]["result"]["source"], "engine");

        let conflict = build_for(&match_data, Some("key"), &[gsi_loss]);
        assert!(conflict["meta"]["result"]["winner"].is_null());
        assert_eq!(conflict["meta"]["result"]["conflict"], true);
        assert_eq!(conflict["meta"]["result"]["warning"], "sources_disagree");

        match_data["meta"]["winner"] = Value::Null;
        match_data["meta"]["winner_reason"] = json!("not in replay");
        let engine_null = build_for(&match_data, Some("key"), &[gsi_win]);
        assert_eq!(engine_null["meta"]["result"]["winner"], "radiant");
        assert_eq!(engine_null["meta"]["result"]["source"], "gsi_history");

        let mut old_manifest = manifest;
        old_manifest["schema"] = json!("3.0");
        match_data["meta"]["schema"] = json!("3.0");
        match_data["meta"]["winner"] = json!("dire");
        let old_input = KnowledgeInput {
            manifest: &old_manifest,
            match_data: &match_data,
            world: &world,
        };
        let old = build_with_sources(&old_input, None, &[], &[], &[], own).unwrap();
        assert!(old["meta"]["result"]["winner"].is_null());
        assert_eq!(
            old["meta"]["result"]["reason"],
            "re-parse with engine 0.3.0 to get this"
        );
    }

    #[test]
    fn digest_is_byte_identical_and_excludes_player_ids_and_paths() {
        let (manifest, match_data, world) = fixture();
        assert_eq!(bundle_hash12(&manifest).unwrap().len(), 12);
        let input = KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        };
        let first = canonical_bytes(&build(&input).unwrap()).unwrap();
        let second = canonical_bytes(&build(&input).unwrap()).unwrap();
        assert_eq!(first, second);
        let raw = String::from_utf8(first).unwrap();
        assert!(!raw.contains("private-name"));
        assert!(!raw.contains("123456"));
        assert!(!raw.contains("F:/"));
    }

    #[test]
    fn digest_bytes_are_stable_for_the_same_bundle() {
        let (manifest, match_data, world) = fixture();
        let input = KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        };
        let first_bytes = canonical_bytes(&build(&input).unwrap()).unwrap();
        let second_bytes = canonical_bytes(&build(&input).unwrap()).unwrap();
        assert_eq!(first_bytes, second_bytes);
    }

    #[test]
    fn gsi_history_changes_session_facts_without_changing_cached_digest_bytes() {
        let (manifest, match_data, world) = fixture();
        let digest = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();
        let gsi = |assists| MatchEntry {
            match_key: "install-keyed-match".into(),
            hero: Some("npc_dota_hero_axe".into()),
            result: Some(MatchResult::Win),
            end: MatchEnd::Completed,
            kills: Some(4),
            deaths: Some(2),
            assists: Some(assists),
            duration_s: Some(600),
            ended_at: 1,
            started_at: None,
            hero_id: None,
            match_type: MatchType::Unknown,
            source: "kohbor".into(),
        };
        let first_gsi = gsi(7);
        let second_gsi = gsi(11);
        let first_session = session_viewer(&digest, None, Some(&first_gsi));
        let second_session = session_viewer(&digest, None, Some(&second_gsi));
        let digest_bytes = canonical_bytes(&digest).unwrap();
        let rebuilt = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();

        assert_eq!(digest_bytes, canonical_bytes(&rebuilt).unwrap());
        assert_ne!(first_session, second_session);
        assert_eq!(first_session["kda"][2], 7);
        assert_eq!(second_session["kda"][2], 11);
        assert!(digest["meta"].get("viewer").is_none());
        assert!(digest["heroes"][0]["final"]["a"].is_null());
    }

    #[test]
    fn missing_positions_and_engine_fields_remain_unknown_with_reasons() {
        let digest = synthetic_digest();
        assert!(digest["heroes"][0]["lane"]["lane"].is_null());
        assert!(digest["heroes"][0]["lane"]["reason"].as_str().is_some());
        assert!(digest["meta"]["result"]["winner"].is_null());
        assert!(digest["meta"]["result"]["reason"].as_str().is_some());
        assert!(digest["meta"]["draft"].is_null());
        assert!(digest["meta"]["draft_reason"].as_str().is_some());
        assert!(digest["heroes"][0]["hero_damage"].is_null());
        assert!(digest["heroes"][0]["hero_damage_reason"].as_str().is_some());
        assert!(digest["heroes"][0]["rates"]["gpm"].is_null());
        assert!(digest["heroes"][0]["rates"]["gpm_reason"]
            .as_str()
            .is_some());
    }

    #[test]
    #[ignore = "opt-in only: set KOHBOR_ANALYTICS_REAL_BUNDLE_ROOT to a sanitized local bundle directory"]
    fn sanitized_real_bundle_reproduces_the_design_rows() {
        let root = std::env::var_os("KOHBOR_ANALYTICS_REAL_BUNDLE_ROOT")
            .map(std::path::PathBuf::from)
            .expect("set KOHBOR_ANALYTICS_REAL_BUNDLE_ROOT to run this opt-in test");
        let read = |name: &str| -> Value {
            let bytes = std::fs::read(root.join(name)).expect("sanitized bundle file missing");
            serde_json::from_slice(&bytes).expect("sanitized bundle JSON invalid")
        };
        let manifest = read("manifest.json");
        let match_data = read("match.json");
        let world = read("world.json");
        let digest = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();
        let lane_score = |name: &str| {
            digest["lanes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|lane| lane["lane"] == name)
                .and_then(|lane| lane.pointer("/score/value"))
                .and_then(Value::as_f64)
                .unwrap_or_else(|| panic!("{name} lane score is unknown"))
        };
        assert_eq!(lane_score("top"), 12.3);
        assert_eq!(lane_score("mid"), -7.3);
        assert_eq!(lane_score("bot"), -19.9);
        assert_eq!(digest["lanes"][0]["label"], "ทดลอง · เกณฑ์ชั่วคราว");
        let hero = |short: &str| -> &Value {
            let id = format!("npc_dota_hero_{short}");
            digest["heroes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == id)
                .expect("design hero row is missing")
        };
        let ember = hero("ember_spirit");
        let shaker = hero("earthshaker");
        for (profile, nw, lh, dn, xp, xpm, dead_s) in [
            (ember, 3998, 53, 3, 4160, 1071.7, 144),
            (shaker, 4753, 72, 8, 5362, 554.3, 316),
        ] {
            assert_eq!(
                profile.pointer("/at_10/nw").and_then(Value::as_i64),
                Some(nw)
            );
            assert_eq!(
                profile.pointer("/at_10/lh").and_then(Value::as_i64),
                Some(lh)
            );
            assert_eq!(
                profile.pointer("/at_10/dn").and_then(Value::as_i64),
                Some(dn)
            );
            assert_eq!(
                profile.pointer("/at_10/xp").and_then(Value::as_i64),
                Some(xp)
            );
            let actual_xpm = profile
                .pointer("/rates/xpm")
                .and_then(Value::as_f64)
                .expect("XPM unknown");
            assert!((actual_xpm - xpm).abs() < 0.2);
            assert_eq!(profile.get("dead_s").and_then(Value::as_i64), Some(dead_s));
        }
    }
}
