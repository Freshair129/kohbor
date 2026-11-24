use crate::match_history::MatchEntry;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const KNOWLEDGE_VERSION: u32 = 1;
const LANE_BOUNDARY_MODES: &[u64] = &[1, 2, 3, 4, 5, 16, 18, 22];

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProvisionalThresholds {
    pub(crate) lane_sample_from_s: i64,
    pub(crate) lane_sample_to_s: i64,
    pub(crate) mid_delta_max: f64,
    pub(crate) mid_sum_max: f64,
    pub(crate) lane_min_share: f64,
    pub(crate) even_advantage_pct: f64,
    pub(crate) stomp_advantage_pct: f64,
    pub(crate) teamfight_gap_s: f64,
    pub(crate) teamfight_death_distance: f64,
    pub(crate) teamfight_min_deaths: usize,
    pub(crate) teamfight_window_before_s: i64,
    pub(crate) teamfight_window_after_s: i64,
    pub(crate) teamfight_presence_distance: f64,
}

// These are one provisional source of truth for every thresholded Analytics label.
pub(crate) const PROVISIONAL_THRESHOLDS: ProvisionalThresholds = ProvisionalThresholds {
    lane_sample_from_s: 60,
    lane_sample_to_s: 480,
    mid_delta_max: 2_500.0,
    mid_sum_max: 5_000.0,
    lane_min_share: 0.5,
    even_advantage_pct: 5.0,
    stomp_advantage_pct: 15.0,
    teamfight_gap_s: 15.0,
    teamfight_death_distance: 3_000.0,
    teamfight_min_deaths: 3,
    teamfight_window_before_s: 10,
    teamfight_window_after_s: 5,
    teamfight_presence_distance: 2_000.0,
};

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
    // Read the hash only as an integrity/cache input. It is deliberately absent from the digest.
    let _cache_hash = bundle_hash(input.manifest).ok();
    let match_data = input.match_data;
    let world = input.world;
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
    let lanes = lane_summaries(&heroes, t0, t1, game_mode);
    let fights = teamfights(&heroes, &deaths, t0);
    let phase_names = ["laning", "mid", "late"];
    let phase_bounds = [(0.0, 600.0), (600.0, 1_500.0), (1_500.0, t1.unwrap_or(0.0))];
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
            &phase_names,
            &phase_bounds,
        ));
    }
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
    let winner = match_winner(match_data);
    let draft = safe_draft(match_data, &hero_by_id);
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
            "winner": winner.0,
            "winner_reason": winner.1,
            "draft": draft.0,
            "draft_reason": draft.1,
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
        "deaths_reason": (!death_data_available).then_some("death events are missing from the bundle").map(Value::from)
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
    viewer_json(viewer_id.as_deref(), gsi)
}

pub(crate) fn canonical_bytes(value: &Value) -> Result<Vec<u8>, &'static str> {
    serde_json::to_vec(&canonicalize(value.clone())).map_err(|_| "knowledge_serialization_failed")
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
        json!(round_to(value, 3))
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
    if (point.x - point.y).abs() < PROVISIONAL_THRESHOLDS.mid_delta_max
        && (point.x + point.y).abs() < PROVISIONAL_THRESHOLDS.mid_sum_max
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
    for time in PROVISIONAL_THRESHOLDS.lane_sample_from_s..=PROVISIONAL_THRESHOLDS.lane_sample_to_s
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
    if share < PROVISIONAL_THRESHOLDS.lane_min_share {
        (None, Some(round_to(share, 3)))
    } else {
        let lane = match lane {
            "top" => "top",
            "mid" => "mid",
            "bot" => "bot",
            _ => return (None, None),
        };
        (Some(lane), Some(round_to(share, 3)))
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
            let killer = hero_id(death.get("killer")).filter(|id| heroes.contains_key(id));
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
    let gold_earned = number(hero.source.get("gold_earned"))
        .or_else(|| number(hero.source.pointer("/stats/gold_earned")));
    let gpm = duration
        .filter(|value| *value > 0.0)
        .and_then(|value| gold_earned.map(|gold| gold / (value / 60.0)));
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
    if end < 600.0 {
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
            eco_number(hero.source, 1, 600.0, t0),
            "net-worth sample at 10:00 is missing",
        );
        add_number(
            &mut at_10,
            "lh",
            eco_number(hero.source, 2, 600.0, t0),
            "last-hit sample at 10:00 is missing",
        );
        add_number(
            &mut at_10,
            "dn",
            eco_number(hero.source, 3, 600.0, t0),
            "deny sample at 10:00 is missing",
        );
        add_number(
            &mut at_10,
            "xp",
            sample_number(
                hero.source.get("st").unwrap_or(&Value::Null),
                "xp",
                600.0,
                t0,
            ),
            "XP sample at 10:00 is missing",
        );
        add_number(
            &mut at_10,
            "lvl",
            sample_number(hero.source, "lvl", 600.0, t0),
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
    add_number(
        &mut rates,
        "gpm",
        gpm,
        "total gold earned is not present in the bundle",
    );
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
    let engine_damage = number(hero.source.get("hero_damage"));
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
        "hero_damage": engine_damage.map(number_json).unwrap_or(Value::Null),
        "hero_damage_reason": engine_damage.is_none().then_some("attacker-side damage is not present in the bundle").map(Value::from),
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
                .map(|candidate| (candidate, eco_number(candidate.source, 1, 600.0, t0)))
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
        let (nw_diff, xp_diff, adv_pct, outcome, reason) = if !game_mode.is_some_and(|mode| LANE_BOUNDARY_MODES.contains(&mode)) {
            (None, None, None, None, Some("lane boundaries are not established for this game mode"))
        } else if t1.is_none_or(|end| end < 600.0) {
            (None, None, None, None, Some("match ended before 10:00"))
        } else if radiant.is_empty() || dire.is_empty() {
            (None, None, None, None, Some("lane assignment or positions are missing on one side"))
        } else {
            let radiant_nw = sum_metric(&radiant, Metric::NetWorth, 600.0, t0);
            let dire_nw = sum_metric(&dire, Metric::NetWorth, 600.0, t0);
            let radiant_xp = sum_metric(&radiant, Metric::Xp, 600.0, t0);
            let dire_xp = sum_metric(&dire, Metric::Xp, 600.0, t0);
            match (radiant_nw, dire_nw, radiant_xp, dire_xp) {
                (Some(rnw), Some(dnw), Some(rxp), Some(dxp)) if rnw + dnw > 0.0 => {
                    let nw = rnw - dnw;
                    let xp = rxp - dxp;
                    let pct = 100.0 * nw / (rnw + dnw);
                    let label = if pct.abs() < PROVISIONAL_THRESHOLDS.even_advantage_pct || (nw != 0.0 && xp != 0.0 && nw.signum() != xp.signum()) {
                        "even"
                    } else if pct.abs() >= PROVISIONAL_THRESHOLDS.stomp_advantage_pct {
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
        let mut value = json!({
            "lane": lane,
            "radiant": radiant.iter().map(|hero| json!({"hero":hero.id,"name":hero.name})).collect::<Vec<_>>(),
            "dire": dire.iter().map(|hero| json!({"hero":hero.id,"name":hero.name})).collect::<Vec<_>>(),
            "nw_diff_10": nw_diff.map(number_json).unwrap_or(Value::Null),
            "xp_diff_10": xp_diff.map(number_json).unwrap_or(Value::Null),
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
        value
    }).collect()
}

#[derive(Clone, Copy)]
enum Metric {
    NetWorth,
    Xp,
}

fn sum_metric(heroes: &[&HeroInfo<'_>], metric: Metric, time: f64, t0: f64) -> Option<f64> {
    if heroes.is_empty() {
        return None;
    }
    heroes.iter().try_fold(0.0, |sum, hero| {
        let value = match metric {
            Metric::NetWorth => eco_number(hero.source, 1, time, t0),
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

fn match_winner(match_data: &Value) -> (Value, Value) {
    if let Some(winner) = match_data
        .pointer("/meta/winner")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "radiant" | "dire"))
    {
        return (Value::String(winner.into()), Value::Null);
    }
    (
        Value::Null,
        Value::from("winner is not present in the replay bundle"),
    )
}

fn safe_draft(match_data: &Value, heroes: &BTreeMap<String, HeroInfo<'_>>) -> (Value, Value) {
    let Some(draft) = match_data.pointer("/meta/draft").and_then(Value::as_object) else {
        return (
            Value::Null,
            Value::from("draft is not present in the bundle"),
        );
    };
    let mut output = Map::new();
    for key in ["picks", "bans", "order"] {
        if let Some(values) = draft.get(key).and_then(Value::as_array) {
            let safe = values
                .iter()
                .map(|value| {
                    hero_id(Some(value))
                        .filter(|id| heroes.contains_key(id))
                        .map(Value::String)
                })
                .collect::<Option<Vec<_>>>();
            if let Some(safe) = safe {
                output.insert(key.into(), Value::Array(safe));
                output.insert(format!("{key}_reason"), Value::Null);
            } else {
                output.insert(key.into(), Value::Null);
                output.insert(
                    format!("{key}_reason"),
                    Value::from("draft contains a hero outside the replay roster"),
                );
            }
        } else {
            output.insert(key.into(), Value::Null);
            output.insert(
                format!("{key}_reason"),
                Value::from("draft field is not in the bundle"),
            );
        }
    }
    (Value::Object(output), Value::Null)
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
            "killer_name": death.killer_id.as_deref().and_then(|id|heroes.get(id)).map(|hero|hero.name.as_str()),
            "position": death.position.map(|point|json!([number_json(point.x),number_json(point.y)])).unwrap_or(Value::Null)
        });
        if death.position.is_none() { row["position_reason"] = Value::from("no matching hero position was sampled near the death") }
        if death.killer_id.is_none() { row["killer_reason"] = Value::from("killer is not an identified roster hero") }
        row
    }).collect()
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
                death.time - previous.time <= PROVISIONAL_THRESHOLDS.teamfight_gap_s
                    && death
                        .position
                        .zip(previous.position)
                        .is_some_and(|(left, right)| {
                            distance(left, right) <= PROVISIONAL_THRESHOLDS.teamfight_death_distance
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
    clusters.into_iter().filter(|cluster| cluster.len() >= PROVISIONAL_THRESHOLDS.teamfight_min_deaths).enumerate().map(|(index, cluster)| {
        let first = cluster.first().map(|death| death.time).unwrap_or_default();
        let last = cluster.last().map(|death| death.time).unwrap_or(first);
        let points = cluster.iter().filter_map(|death| death.position).collect::<Vec<_>>();
        let center = if points.is_empty() { None } else { Some(Point { x: points.iter().map(|p|p.x).sum::<f64>() / points.len() as f64, y: points.iter().map(|p|p.y).sum::<f64>() / points.len() as f64 }) };
        let start = first - PROVISIONAL_THRESHOLDS.teamfight_window_before_s as f64;
        let stop = last + PROVISIONAL_THRESHOLDS.teamfight_window_after_s as f64;
        let present = center.map(|center| heroes.iter().filter(|hero| {
            let Some(alive) = hero.source.get("alive").and_then(Value::as_array) else { return false };
            let start_idx = sample_index(t0, start.max(0.0), alive.len()).unwrap_or(0);
            let end_idx = sample_index(t0, stop, alive.len()).unwrap_or_else(||alive.len().saturating_sub(1));
            (start_idx..=end_idx).any(|i| {
                let time = t0 + i as f64;
                alive_at(hero.source,time,t0) == Some(true) && position_at(hero.source,time,t0).is_some_and(|point|distance(point,center) <= PROVISIONAL_THRESHOLDS.teamfight_presence_distance)
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
    json!({
        "label":"เกณฑ์ชั่วคราว",
        "lane_sample_from_s":PROVISIONAL_THRESHOLDS.lane_sample_from_s,
        "lane_sample_to_s":PROVISIONAL_THRESHOLDS.lane_sample_to_s,
        "mid_delta_max":PROVISIONAL_THRESHOLDS.mid_delta_max,
        "mid_sum_max":PROVISIONAL_THRESHOLDS.mid_sum_max,
        "lane_min_share":PROVISIONAL_THRESHOLDS.lane_min_share,
        "even_advantage_pct":PROVISIONAL_THRESHOLDS.even_advantage_pct,
        "stomp_advantage_pct":PROVISIONAL_THRESHOLDS.stomp_advantage_pct,
        "teamfight_gap_s":PROVISIONAL_THRESHOLDS.teamfight_gap_s,
        "teamfight_death_distance":PROVISIONAL_THRESHOLDS.teamfight_death_distance,
        "teamfight_min_deaths":PROVISIONAL_THRESHOLDS.teamfight_min_deaths,
        "teamfight_window_before_s":PROVISIONAL_THRESHOLDS.teamfight_window_before_s,
        "teamfight_window_after_s":PROVISIONAL_THRESHOLDS.teamfight_window_after_s,
        "teamfight_presence_distance":PROVISIONAL_THRESHOLDS.teamfight_presence_distance
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_history::{MatchEnd, MatchResult, MatchType};
    use serde_json::json;

    fn fixture() -> (Value, Value, Value) {
        let manifest = json!({
            "files": {
                "match.json":{"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                "world.json":{"sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
            },
            "path":"F:/synthetic/private/replay"
        });
        let match_data = json!({
            "t0":0,"t1":2,"meta":{"duration_s":2,"patch":"synthetic","game_mode":2},
            "heroes":[{"id":"npc_dota_hero_axe","name":"Axe","team":2,"pid":0,"player_name":"private-name","account_id":123456}],
            "deaths":[]
        });
        let world = json!({"heroes":{},"tracks":[],"runes":[]});
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
        assert!(digest["meta"]["winner"].is_null());
        assert!(digest["meta"]["winner_reason"].as_str().is_some());
        assert!(digest["meta"]["draft"].is_null());
        assert!(digest["meta"]["draft_reason"].as_str().is_some());
    }

    #[test]
    fn local_gsi_result_does_not_fill_engine_winner() {
        let (manifest, match_data, world) = fixture();
        let digest = build(&KnowledgeInput {
            manifest: &manifest,
            match_data: &match_data,
            world: &world,
        })
        .unwrap();
        assert!(digest["meta"]["winner"].is_null());
        assert_eq!(
            digest["meta"]["winner_reason"],
            "winner is not present in the replay bundle"
        );
        assert!(digest["meta"].get("viewer").is_none());
        assert!(digest["heroes"][0]["final"]["a"].is_null());
        assert_eq!(
            digest["heroes"][0]["final"]["a_reason"],
            "exact assists are not in the bundle"
        );
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
            "b289e7c5031848e3bc3745d5a28c2c28767ddb21fbf2c7adad8d7118b662e111"
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
        assert!(digest["meta"]["winner"].is_null());
        assert!(digest["meta"]["winner_reason"].as_str().is_some());
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
