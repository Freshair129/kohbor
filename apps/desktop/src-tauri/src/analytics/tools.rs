use super::knowledge::PROVISIONAL_THRESHOLDS;
use super::store::AnalyticsStore;
use crate::replay_library::ReplayBundle;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) const MAX_TOOL_RESULT_BYTES: usize = 8 * 1024;
const MAX_WINDOW_SECONDS: f64 = 30.0 * 60.0;
const MAX_SERIES_POINTS: usize = 120;

pub(crate) fn definitions(tools_supported: bool) -> Vec<Value> {
    let mut tools = vec![
        tool(
            "get_deaths",
            "Read matching death events from this replay.",
            object_schema(
                json!({
                    "hero":hero_id_schema(),"killer":hero_id_schema(),"from":number_schema(0.0),"to":number_schema(0.0),"detail":{"type":"boolean"}
                }),
                vec![],
            ),
        ),
        tool(
            "get_hero_card",
            "Read one roster hero's values at a match time.",
            object_schema(
                json!({"hero":hero_id_schema(),"t":number_schema(0.0)}),
                vec!["hero"],
            ),
        ),
        tool(
            "get_hero_series",
            "Read a bounded, downsampled series for one roster hero.",
            object_schema(
                json!({
                    "hero":hero_id_schema(),"metric":{"type":"string","enum":["nw","gold","lh","dn","xp","lvl","hp","mp"]},"from":number_schema(0.0),"to":number_schema(0.0),"step_s":{"type":"integer","minimum":10,"maximum":1800}
                }),
                vec!["hero", "metric", "from", "to", "step_s"],
            ),
        ),
        tool(
            "get_items",
            "Read item purchase timing and final inventory for one roster hero.",
            object_schema(json!({"hero":hero_id_schema()}), vec!["hero"]),
        ),
        tool(
            "get_lane",
            "Read a provisional lane record and minute-by-minute advantage.",
            object_schema(
                json!({"lane":{"type":"string","enum":["top","mid","bot"]}}),
                vec!["lane"],
            ),
        ),
        tool(
            "get_objectives",
            "Read objective events in a bounded match window.",
            object_schema(
                json!({"from":number_schema(0.0),"to":number_schema(0.0)}),
                vec![],
            ),
        ),
        tool(
            "get_phase_stats",
            "Read one match phase and optional hero rows.",
            object_schema(
                json!({"phase":{"type":"string","enum":["laning","mid","late"]},"heroes":{"type":"array","items":hero_id_schema(),"maxItems":10}}),
                vec!["phase"],
            ),
        ),
        tool(
            "get_teamfight",
            "Read one provisional teamfight with its death and participant summaries.",
            object_schema(
                json!({"n":{"type":"integer","minimum":1,"maximum":100}}),
                vec!["n"],
            ),
        ),
        tool(
            "read_saved_analysis",
            "Read a saved local note or AI report by its displayed ID.",
            object_schema(
                json!({"id":{"type":"string","minLength":1,"maxLength":180}}),
                vec!["id"],
            ),
        ),
    ];
    if tools_supported {
        tools.push(tool("write_note", "Save a longer match analysis as a local Markdown note.", object_schema(json!({
            "title":{"type":"string","minLength":1,"maxLength":120},"markdown":{"type":"string","minLength":1,"maxLength":65536}
        }), vec!["title","markdown"])));
    }
    tools.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    tools
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name":name,"description":description,"input_schema":input_schema,"strict":true})
}

fn object_schema(properties: Value, required: Vec<&str>) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn hero_id_schema() -> Value {
    json!({"type":"string","minLength":16,"maxLength":64,"pattern":"^npc_dota_hero_[a-z0-9_]+$"})
}
fn number_schema(minimum: f64) -> Value {
    json!({"type":"number","minimum":minimum})
}

pub(crate) struct ToolContext<'a> {
    pub(crate) match_id: &'a str,
    pub(crate) digest: &'a Value,
    pub(crate) bundle: Option<&'a ReplayBundle>,
    pub(crate) store: &'a AnalyticsStore,
}

pub(crate) fn execute(
    context: &ToolContext<'_>,
    name: &str,
    input: &Value,
) -> Result<Value, &'static str> {
    let object = input.as_object().ok_or("invalid_tool_input")?;
    let allowed = allowed_keys(name).ok_or("unknown_tool")?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("invalid_tool_input");
    }
    if name == "read_saved_analysis" {
        return read_saved_analysis(context, input);
    }
    let bundle = context.bundle.ok_or("replay_removed")?;
    let roster = roster_ids(&bundle.match_data);
    let output = match name {
        "get_deaths" => get_deaths(context.digest, &bundle.match_data, input, &roster)?,
        "get_hero_card" => get_hero_card(context.digest, &bundle.match_data, input, &roster)?,
        "get_hero_series" => get_hero_series(&bundle.match_data, input, &roster)?,
        "get_items" => get_items(context.digest, &bundle.match_data, input, &roster)?,
        "get_lane" => get_lane(context.digest, input)?,
        "get_objectives" => get_objectives(context.digest, input)?,
        "get_phase_stats" => get_phase_stats(context.digest, input, &roster)?,
        "get_teamfight" => get_teamfight(context.digest, input, &roster)?,
        _ => return Err("unknown_tool"),
    };
    Ok(cap_result(output))
}

fn allowed_keys(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "get_deaths" => Some(&["hero", "killer", "from", "to", "detail"]),
        "get_hero_card" => Some(&["hero", "t"]),
        "get_hero_series" => Some(&["hero", "metric", "from", "to", "step_s"]),
        "get_items" => Some(&["hero"]),
        "get_lane" => Some(&["lane"]),
        "get_objectives" => Some(&["from", "to"]),
        "get_phase_stats" => Some(&["phase", "heroes"]),
        "get_teamfight" => Some(&["n"]),
        "read_saved_analysis" => Some(&["id"]),
        _ => None,
    }
}

fn roster_ids(match_data: &Value) -> BTreeMap<String, String> {
    match_data
        .get("heroes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|hero| {
            let id = hero.get("id")?.as_str()?;
            if !valid_hero_id(id) {
                return None;
            }
            Some((
                id.to_owned(),
                hero.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("— ไม่ทราบ")
                    .to_owned(),
            ))
        })
        .collect()
}
fn valid_hero_id(value: &str) -> bool {
    value.starts_with("npc_dota_hero_")
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
fn hero_arg(
    input: &Value,
    key: &str,
    roster: &BTreeMap<String, String>,
) -> Result<String, &'static str> {
    let id = input
        .get(key)
        .and_then(Value::as_str)
        .filter(|id| valid_hero_id(id) && roster.contains_key(*id))
        .ok_or("invalid_hero")?;
    Ok(id.to_owned())
}
fn finite_number(input: &Value, key: &str) -> Result<f64, &'static str> {
    input
        .get(key)
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && *number >= 0.0)
        .ok_or("invalid_time")
}
fn match_window(
    digest: &Value,
    input: &Value,
    needs_bounds: bool,
) -> Result<(f64, f64), &'static str> {
    let duration = digest
        .pointer("/meta/duration_s")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0);
    let from = if input.get("from").is_some() {
        finite_number(input, "from")?
    } else {
        0.0
    };
    let to = if input.get("to").is_some() {
        finite_number(input, "to")?
    } else {
        duration
    };
    if needs_bounds && (input.get("from").is_none() || input.get("to").is_none()) {
        return Err("window_required");
    }
    if from > to
        || to > duration
        || ((input.get("from").is_some() || input.get("to").is_some())
            && to - from > MAX_WINDOW_SECONDS)
    {
        return Err("window_out_of_bounds");
    }
    Ok((from, to))
}
fn cap_result(mut value: Value) -> Value {
    let mut bytes = serde_json::to_vec(&value)
        .map(|v| v.len())
        .unwrap_or(MAX_TOOL_RESULT_BYTES + 1);
    if bytes <= MAX_TOOL_RESULT_BYTES {
        return value;
    }
    if let Some(object) = value.as_object_mut() {
        object.insert("truncated".into(), Value::Bool(true));
    }
    if let Some(markdown) = value
        .get("markdown")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        let mut end = markdown.len().min(MAX_TOOL_RESULT_BYTES - 512);
        loop {
            while !markdown.is_char_boundary(end) {
                end -= 1;
            }
            if let Some(object) = value.as_object_mut() {
                object.insert("markdown".into(), Value::String(markdown[..end].to_owned()));
                object.insert(
                    "truncation_reason".into(),
                    Value::from("saved analysis exceeds the 8 KB tool result limit"),
                );
            }
            bytes = serde_json::to_vec(&value)
                .map(|v| v.len())
                .unwrap_or(MAX_TOOL_RESULT_BYTES + 1);
            if bytes <= MAX_TOOL_RESULT_BYTES {
                return value;
            }
            if end == 0 {
                break;
            }
            end = end.saturating_sub(128);
        }
        return json!({"truncated":true,"reason":"saved analysis exceeds the 8 KB tool result limit"});
    }
    loop {
        bytes = serde_json::to_vec(&value)
            .map(|v| v.len())
            .unwrap_or(MAX_TOOL_RESULT_BYTES + 1);
        if bytes <= MAX_TOOL_RESULT_BYTES {
            return value;
        }
        let mut shortened = false;
        for key in [
            "rows",
            "deaths",
            "objectives",
            "points",
            "heroes",
            "participants",
            "items",
        ] {
            if let Some(array) = value.get_mut(key).and_then(Value::as_array_mut) {
                if !array.is_empty() {
                    array.pop();
                    shortened = true;
                    break;
                }
            }
        }
        if !shortened {
            return json!({"truncated":true,"reason":"tool result exceeds the 8 KB limit"});
        }
    }
}

fn get_deaths(
    digest: &Value,
    match_data: &Value,
    input: &Value,
    roster: &BTreeMap<String, String>,
) -> Result<Value, &'static str> {
    let (from, to) = match_window(
        digest,
        input,
        input.get("from").is_some() || input.get("to").is_some(),
    )?;
    let hero = if input.get("hero").is_some() {
        Some(hero_arg(input, "hero", roster)?)
    } else {
        None
    };
    let killer = if input.get("killer").is_some() {
        Some(hero_arg(input, "killer", roster)?)
    } else {
        None
    };
    let detail = input
        .get("detail")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(deaths) = digest.get("deaths").and_then(Value::as_array) else {
        return Ok(
            json!({"rows":null,"reason":digest.get("deaths_reason").cloned().unwrap_or_else(||Value::from("death events are missing from the bundle")),"window_s":[from,to],"detail":detail}),
        );
    };
    let rows = deaths
        .iter()
        .filter(|row| {
            let time = row.get("time_s").and_then(Value::as_f64).unwrap_or(-1.0);
            time >= from
                && time <= to
                && hero
                    .as_ref()
                    .is_none_or(|id| row.get("hero").and_then(Value::as_str) == Some(id))
                && killer
                    .as_ref()
                    .is_none_or(|id| row.get("killer").and_then(Value::as_str) == Some(id))
        })
        .cloned()
        .collect::<Vec<_>>();
    let output = rows
        .into_iter()
        .map(|mut row| {
            if detail {
                let index = row
                    .get("event_ref")
                    .and_then(Value::as_str)
                    .and_then(|value| value.rsplit('/').next())
                    .and_then(|value| value.parse::<usize>().ok());
                let raw = index.and_then(|index| {
                    match_data
                        .get("deaths")
                        .and_then(Value::as_array)?
                        .get(index)
                });
                row["detail"] = raw
                    .map(|raw| safe_death_detail(raw, roster))
                    .unwrap_or_else(|| json!({"reason":"full death record is not present"}));
            } else if let Some(object) = row.as_object_mut() {
                object.remove("position");
                object.remove("position_reason");
            }
            row
        })
        .collect::<Vec<_>>();
    Ok(json!({"rows":output,"window_s":[from,to],"detail":detail}))
}

fn safe_death_detail(raw: &Value, roster: &BTreeMap<String, String>) -> Value {
    let safe_sources = |key: &str| {
        raw.get(key).and_then(Value::as_array).map(|rows| rows.iter().take(6).filter_map(|item| {
        let source = item.as_str().or_else(||item.get("hero").and_then(Value::as_str)).or_else(||item.get("by").and_then(Value::as_str))?;
        if !roster.contains_key(source) { return None; }
        let amount = item.get("damage").or_else(||item.get("amount")).or_else(||item.get("value")).and_then(Value::as_f64).filter(|value|value.is_finite());
        Some(json!({"hero":source,"amount":amount,"amount_reason":amount.is_none().then_some("amount is not recorded").map(Value::from)}))
    }).collect::<Vec<_>>())
    };
    let seen = raw.get("seen").and_then(Value::as_object).map(|rows| {
        rows.iter()
            .filter(|(id, _)| roster.contains_key(*id))
            .map(|(id, row)| {
                let time = row
                    .get("t")
                    .and_then(Value::as_f64)
                    .filter(|value| value.is_finite());
                let mut item = json!({"hero":id,"last_seen_s":time});
                if time.is_none() {
                    item["last_seen_s_reason"] =
                        Value::from("no numeric last-seen time is recorded");
                }
                item
            })
            .collect::<Vec<_>>()
    });
    json!({
        "top_damage_sources":safe_sources("by").map(Value::Array).unwrap_or(Value::Null),
        "top_damage_sources_reason":raw.get("by").is_none().then_some("damage source rows are missing").map(Value::from),
        "hits":safe_sources("hits").map(Value::Array).unwrap_or(Value::Null),
        "hits_reason":raw.get("hits").is_none().then_some("hit rows are missing").map(Value::from),
        "heals":safe_sources("heals").map(Value::Array).unwrap_or(Value::Null),
        "heals_reason":raw.get("heals").is_none().then_some("healing rows are missing").map(Value::from),
        "enemies_seen":seen.clone().map(Value::Array).unwrap_or(Value::Null),
        "enemies_seen_reason":seen.is_none().then_some("enemy visibility data is missing").map(Value::from),
        "win":raw.get("win").and_then(Value::as_array).map(|values|Value::Array(values.clone())).unwrap_or(Value::Null),
        "win_reason":raw.get("win").is_none().then_some("death window data is missing").map(Value::from)
    })
}

fn get_hero_card(
    digest: &Value,
    match_data: &Value,
    input: &Value,
    roster: &BTreeMap<String, String>,
) -> Result<Value, &'static str> {
    let hero_id = hero_arg(input, "hero", roster)?;
    let profile = digest
        .get("heroes")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(&hero_id))
        })
        .cloned()
        .ok_or("invalid_hero")?;
    let t = if input.get("t").is_some() {
        finite_number(input, "t")?
    } else {
        digest
            .pointer("/meta/duration_s")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let duration = digest
        .pointer("/meta/duration_s")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if t > duration {
        return Err("time_out_of_bounds");
    }
    let raw = match_data
        .get("heroes")
        .and_then(Value::as_array)
        .and_then(|heroes| {
            heroes
                .iter()
                .find(|hero| hero.get("id").and_then(Value::as_str) == Some(&hero_id))
        })
        .ok_or("invalid_hero")?;
    let t0 = match_data
        .get("t0")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite());
    let index = sample_index(t, t0);
    let sample = |series: Option<&Value>| sample_value(series, index);
    let stat = raw.get("st").unwrap_or(&Value::Null);
    let eco_sample = sample(raw.get("eco"));
    let eco = [
        ("gold", 0usize, "gold sample is missing at this time"),
        ("nw", 1usize, "net-worth sample is missing at this time"),
        ("lh", 2usize, "last-hit sample is missing at this time"),
        ("dn", 3usize, "deny sample is missing at this time"),
    ]
    .into_iter()
    .map(|(key, index, reason)| {
        let value = eco_sample
            .as_ref()
            .and_then(Value::as_array)
            .and_then(|row| row.get(index))
            .cloned()
            .unwrap_or(Value::Null);
        (
            key.to_owned(),
            json!({"value":value,"reason":value.is_null().then_some(reason).map(Value::from)}),
        )
    })
    .collect::<serde_json::Map<_, _>>();
    let profile_known = |path: &str| profile.pointer(path).cloned().unwrap_or(Value::Null);
    let field = |series: Option<&Value>, reason: &str| {
        let value = sample(series).unwrap_or(Value::Null);
        json!({"value":value,"reason":value.is_null().then_some(reason).map(Value::from)})
    };
    let attr_pair = |base: &str, total: &str| {
        let base = sample(stat.get(base)).unwrap_or(Value::Null);
        let total = sample(stat.get(total)).unwrap_or(Value::Null);
        let reason = if base.is_null() || total.is_null() {
            Some("attribute samples are missing at this time")
        } else {
            None
        };
        json!({"values":[base,total],"reason":reason})
    };
    let dmg_values = ["dmin", "dmax", "dbonus"]
        .into_iter()
        .map(|key| sample(stat.get(key)).unwrap_or(Value::Null))
        .collect::<Vec<_>>();
    let dmg_reason = dmg_values
        .iter()
        .any(Value::is_null)
        .then_some("attack damage samples are missing at this time");
    let primary = match raw.get("prim").and_then(Value::as_u64) {
        Some(0) => Value::from("str"),
        Some(1) => Value::from("agi"),
        Some(2) => Value::from("int"),
        _ => Value::Null,
    };
    let primary_reason = primary
        .is_null()
        .then_some("primary attribute is missing from the bundle");
    let respawn = respawn_in(raw, index, t0, t);
    let deaths = match_data.get("deaths").and_then(Value::as_array);
    let (kills, deaths_count) = if let Some(deaths) = deaths {
        let mut kills = 0_u64;
        let mut deaths_count = 0_u64;
        for death in deaths {
            if death
                .get("t")
                .and_then(Value::as_f64)
                .is_none_or(|time| time > t)
            {
                continue;
            }
            if death.get("killer").and_then(Value::as_str) == Some(&hero_id) {
                kills += 1;
            }
            if death.get("hero").and_then(Value::as_str) == Some(&hero_id) {
                deaths_count += 1;
            }
        }
        (json!([kills, deaths_count]), Value::Null)
    } else {
        (
            Value::Null,
            Value::from("death events are missing from the bundle"),
        )
    };
    let abilities = raw.get("abil").and_then(Value::as_array).map(|rows| {
        rows.iter().map(|ability| {
            let name = ability.get("n").and_then(Value::as_str).filter(|value| valid_token(value, 96));
            let value_at = |key: &str| sample(ability.get(key));
            let level = value_at("lv").unwrap_or(Value::Null);
            let cooldown = value_at("cd").unwrap_or(Value::Null);
            let cooldown_length = value_at("cl").unwrap_or(Value::Null);
            let mana_cost = value_at("mc").unwrap_or(Value::Null);
            json!({
                "ability":name,
                "ability_reason":name.is_none().then_some("ability name is missing or invalid").map(Value::from),
                "level":level,
                "level_reason":level.is_null().then_some("ability level sample is missing at this time").map(Value::from),
                "cooldown_s":cooldown,
                "cooldown_s_reason":cooldown.is_null().then_some("ability cooldown sample is missing at this time").map(Value::from),
                "cooldown_length_s":cooldown_length,
                "cooldown_length_s_reason":cooldown_length.is_null().then_some("ability cooldown length is missing at this time").map(Value::from),
                "mana_cost":mana_cost,
                "mana_cost_reason":mana_cost.is_null().then_some("ability mana cost sample is missing at this time").map(Value::from)
            })
        }).collect::<Vec<_>>()
    });
    let (items, items_reason) = inventory_at(raw, t);
    let buffs = raw.get("mods").and_then(Value::as_array).map(|rows| {
        rows.iter().filter_map(|row| {
            let values = row.as_array()?;
            let start = values.first().and_then(Value::as_f64).filter(|value| value.is_finite())?;
            let end = values.get(1).and_then(Value::as_f64).filter(|value| value.is_finite())?;
            if start > t || end < t { return None; }
            let modifier = values.get(2).and_then(Value::as_str).filter(|value| valid_token(value, 128));
            let source = values.get(3).and_then(Value::as_str).filter(|id| roster.contains_key(*id));
            Some(json!({
                "modifier":modifier,
                "modifier_reason":modifier.is_none().then_some("modifier name is missing or invalid").map(Value::from),
                "remaining_s":(end-t).max(0.0),
                "source_hero":source,
                "source_hero_reason":source.is_none().then_some("modifier source is not a roster hero").map(Value::from)
            }))
        }).take(12).collect::<Vec<_>>()
    });
    let finds = raw.get("finds").and_then(Value::as_array).map(|rows| {
        rows.iter().filter_map(|row| {
            let values = row.as_array()?;
            let time = values.first().and_then(Value::as_f64).filter(|value| value.is_finite())?;
            if time > t { return None; }
            let item = values.get(1).and_then(item_name);
            let enchantment = values.get(2).and_then(Value::as_str).filter(|value| valid_token(value, 96));
            let tier = values.get(3).and_then(Value::as_u64);
            Some(json!({
                "time_s":time,
                "item":item,
                "item_reason":item.is_none().then_some("neutral item is missing or invalid").map(Value::from),
                "enchantment":enchantment,
                "enchantment_reason":enchantment.is_none().then_some("neutral enchantment is missing or invalid").map(Value::from),
                "tier":tier,
                "tier_reason":tier.is_none().then_some("neutral item tier is missing").map(Value::from)
            }))
        }).collect::<Vec<_>>()
    });
    let prim_reason = primary_reason.map(Value::from).unwrap_or(Value::Null);
    Ok(json!({
        "hero":roster.get(&hero_id).cloned().unwrap_or_else(||"— ไม่ทราบ".into()),"id":hero_id,"t":t,
        "alive":field(raw.get("alive"),"alive samples are missing at this time"),
        "lvl":field(raw.get("lvl"),"level samples are missing at this time"),
        "hp":field(raw.get("hp"),"health samples are missing at this time"),
        "mhp":field(raw.get("mhp"),"maximum health samples are missing at this time"),
        "mp":field(raw.get("mp"),"mana samples are missing at this time"),
        "mmp":field(raw.get("mmp"),"maximum mana samples are missing at this time"),
        "respawn_in_s":respawn.0,
        "respawn_in_s_reason":respawn.1,
        "regen":{"hp":field(stat.get("hpreg"),"health regen sample is missing at this time"),"mp":field(stat.get("mpreg"),"mana regen sample is missing at this time")},
        "attr":{"str":attr_pair("str","strT"),"agi":attr_pair("agi","agiT"),"int":attr_pair("int","intT"),"primary":{"value":primary,"reason":prim_reason}},
        "dmg":{"values":dmg_values,"reason":dmg_reason},
        "attack_range":field(stat.get("range"),"attack range sample is missing at this time"),
        "bat":field(stat.get("bat"),"base attack time sample is missing at this time"),
        "eco":eco,"xp":field(stat.get("xp"),"XP sample is missing at this time"),"stats":{"final":profile_known("/final"),"at_10":profile_known("/at_10"),"phase":profile_known("/phases")},
        "kd":{"value":kills,"reason":deaths_count},
        "abilities":{"value":abilities.clone().map(Value::Array).unwrap_or(Value::Null),"reason":abilities.is_none().then_some("ability tracks are missing from the bundle").map(Value::from)},
        "items":{"value":items,"reason":items_reason},
        "neutral_items":{"value":finds.clone().map(Value::Array).unwrap_or(Value::Null),"reason":finds.is_none().then_some("neutral item tracks are missing from the bundle").map(Value::from)},
        "buffs":{"value":buffs.clone().map(Value::Array).unwrap_or(Value::Null),"reason":buffs.is_none().then_some("modifier tracks are missing from the bundle").map(Value::from)},
        "derived":{"value":null,"reason":"patch-scoped derived-stat tables are not verified by this host slice"},
        "hero_damage":{"value":null,"reason":"attacker-side damage is not present in the bundle"},
        "gold_earned":{"value":null,"reason":"total gold earned is not present in the bundle"}
    }))
}

fn sample_index(time: f64, t0: Option<f64>) -> Option<usize> {
    let start = t0?;
    let index = (time.floor() - start).floor();
    (index.is_finite() && index >= 0.0 && index <= usize::MAX as f64).then_some(index as usize)
}

fn sample_value(series: Option<&Value>, index: Option<usize>) -> Option<Value> {
    Some(series?.as_array()?.get(index?)?.clone())
}

fn valid_token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn item_name(value: &Value) -> Option<&str> {
    let name = value
        .as_str()
        .or_else(|| value.get("name").and_then(Value::as_str))
        .or_else(|| value.get("id").and_then(Value::as_str))
        .or_else(|| value.get("item").and_then(Value::as_str))?;
    valid_token(name, 64).then_some(name)
}

fn respawn_in(hero: &Value, index: Option<usize>, t0: Option<f64>, time: f64) -> (Value, Value) {
    let Some(index) = index else {
        return (
            Value::Null,
            Value::from("alive samples are missing at this time"),
        );
    };
    let Some(alive) = hero.get("alive").and_then(Value::as_array) else {
        return (
            Value::Null,
            Value::from("alive samples are missing at this time"),
        );
    };
    match alive.get(index).and_then(alive_value) {
        Some(true) => (Value::Null, Value::from("hero is alive at this time")),
        Some(false) => {
            let Some(start) = t0 else {
                return (Value::Null, Value::from("match start time is missing"));
            };
            if let Some(next) = alive
                .iter()
                .enumerate()
                .skip(index.saturating_add(1))
                .find_map(|(index, value)| alive_value(value).filter(|alive| *alive).map(|_| index))
            {
                (json!((start + next as f64 - time).max(0.0)), Value::Null)
            } else {
                (
                    Value::Null,
                    Value::from("next alive sample is not present in the bundle"),
                )
            }
        }
        _ => (
            Value::Null,
            Value::from("alive sample is missing at this time"),
        ),
    }
}

fn alive_value(value: &Value) -> Option<bool> {
    value
        .as_bool()
        .or_else(|| value.as_i64().map(|value| value != 0))
}

fn inventory_at(hero: &Value, time: f64) -> (Value, Value) {
    let Some(snapshots) = hero.get("inv").and_then(Value::as_array) else {
        return (Value::Null, Value::from("inventory timeline is missing"));
    };
    let snapshot = snapshots
        .iter()
        .filter_map(|row| {
            let values = row.as_array()?;
            let timestamp = values
                .first()
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())?;
            (timestamp <= time).then_some((timestamp, values.get(1)?))
        })
        .max_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, slots)| slots);
    let Some(slots) = snapshot.and_then(Value::as_array) else {
        return (
            Value::Null,
            Value::from("no inventory snapshot exists at or before this time"),
        );
    };
    let mut by_slot = BTreeMap::<u64, Value>::new();
    for slot in slots {
        let Some(values) = slot.as_array() else {
            continue;
        };
        let Some(index) = values
            .first()
            .and_then(Value::as_u64)
            .filter(|index| *index <= 16)
        else {
            continue;
        };
        let item = values.get(1).and_then(item_name).map(Value::from);
        let charges = values.get(2).and_then(Value::as_u64).map(Value::from);
        let cooldown_index = time.floor().max(0.0) as usize;
        let cooldown_pair = hero
            .get("icd")
            .and_then(Value::as_object)
            .and_then(|icd| icd.get(&index.to_string()))
            .and_then(Value::as_array)
            .and_then(|samples| samples.get(cooldown_index))
            .and_then(Value::as_array);
        let cooldown = cooldown_pair
            .and_then(|pair| pair.first())
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .map(Value::from);
        let cooldown_length = cooldown_pair
            .and_then(|pair| pair.get(1))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .map(Value::from);
        by_slot.insert(index, json!({
            "slot":index,
            "item":item,
            "item_reason":item.is_none().then_some("item name is missing or invalid").map(Value::from),
            "charges":charges,
            "charges_reason":charges.is_none().then_some("item charges are missing").map(Value::from),
            "cooldown_s":cooldown,
            "cooldown_length_s":cooldown_length,
            "cooldown_reason":(cooldown.is_none() || cooldown_length.is_none()).then_some("item cooldown sample is missing at this time").map(Value::from)
        }));
    }
    let slots = (0..=16)
        .filter_map(|index| by_slot.remove(&index).map(|value| (index, value)))
        .collect::<BTreeMap<_, _>>();
    let list = |range: std::ops::RangeInclusive<u64>| {
        range
            .map(|index| slots.get(&index).cloned().unwrap_or(Value::Null))
            .collect::<Vec<_>>()
    };
    (
        json!({"main":list(0..=5),"backpack":list(6..=8),"tp":slots.get(&15).cloned().unwrap_or(Value::Null),"neutral":slots.get(&16).cloned().unwrap_or(Value::Null)}),
        Value::Null,
    )
}

fn get_hero_series(
    match_data: &Value,
    input: &Value,
    roster: &BTreeMap<String, String>,
) -> Result<Value, &'static str> {
    let id = hero_arg(input, "hero", roster)?;
    let metric = input
        .get("metric")
        .and_then(Value::as_str)
        .ok_or("invalid_metric")?;
    if !["nw", "gold", "lh", "dn", "xp", "lvl", "hp", "mp"].contains(&metric) {
        return Err("invalid_metric");
    }
    let digest = json!({"meta":{"duration_s":match_data.pointer("/meta/duration_s").and_then(Value::as_f64).or_else(||match_data.get("t1").and_then(Value::as_f64)).unwrap_or(0.0)}});
    let (from, to) = match_window(&digest, input, true)?;
    let requested_step = input
        .get("step_s")
        .and_then(Value::as_u64)
        .filter(|step| *step >= 10 && *step <= 1800)
        .ok_or("invalid_step")? as usize;
    let (step, times) = bounded_sample_times(from, to, requested_step);
    let hero = match_data
        .get("heroes")
        .and_then(Value::as_array)
        .and_then(|heroes| {
            heroes
                .iter()
                .find(|hero| hero.get("id").and_then(Value::as_str) == Some(&id))
        })
        .ok_or("invalid_hero")?;
    let t0 = match_data
        .get("t0")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite());
    let points = times.into_iter().map(|time| {
        let index = t0.map(|start| (time as f64 - start).floor() as i64).filter(|value|*value>=0);
        let sample = |series: Option<&Value>| -> Value { index.and_then(|value|usize::try_from(value).ok()).and_then(|index|series?.as_array()?.get(index).cloned()).unwrap_or(Value::Null) };
        let value = match metric { "nw"=>sample(hero.get("eco")).get(1).cloned().unwrap_or(Value::Null),"gold"=>sample(hero.get("eco")).get(0).cloned().unwrap_or(Value::Null),"lh"=>sample(hero.get("eco")).get(2).cloned().unwrap_or(Value::Null),"dn"=>sample(hero.get("eco")).get(3).cloned().unwrap_or(Value::Null),"xp"=>sample(hero.pointer("/st/xp")),"lvl"=>sample(hero.get("lvl")),"hp"=>sample(hero.get("hp")),"mp"=>sample(hero.get("mp")),_=>Value::Null};
        json!({"t":time,"value":value,"reason":value.is_null().then_some("series sample is missing").map(Value::from)})
    }).collect::<Vec<_>>();
    Ok(
        json!({"hero":id,"metric":metric,"from":from,"to":to,"step_s":step,"requested_step_s":requested_step,"points":points,"threshold_label":"เกณฑ์ชั่วคราว"}),
    )
}

fn bounded_sample_times(from: f64, to: f64, requested_step: usize) -> (usize, Vec<usize>) {
    let start = from.floor().max(0.0) as usize;
    let end = to.floor().max(from.floor()).max(0.0) as usize;
    let span = end.saturating_sub(start);
    let coverage_step = span.div_ceil(MAX_SERIES_POINTS.saturating_sub(1)).max(1);
    let step = requested_step.max(coverage_step);
    let mut times = (start..=end)
        .step_by(step)
        .take(MAX_SERIES_POINTS)
        .collect::<Vec<_>>();
    if times.last().copied() != Some(end) && times.len() < MAX_SERIES_POINTS {
        times.push(end);
    }
    (step, times)
}

fn get_items(
    digest: &Value,
    match_data: &Value,
    input: &Value,
    roster: &BTreeMap<String, String>,
) -> Result<Value, &'static str> {
    let id = hero_arg(input, "hero", roster)?;
    let profile = digest
        .get("heroes")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(&id))
        })
        .ok_or("invalid_hero")?;
    let items = profile.get("items").cloned().unwrap_or(Value::Null);
    let purchases_reason = items.get("reason").cloned().unwrap_or(Value::Null);
    let raw = match_data
        .get("heroes")
        .and_then(Value::as_array)
        .and_then(|heroes| {
            heroes
                .iter()
                .find(|hero| hero.get("id").and_then(Value::as_str) == Some(&id))
        })
        .ok_or("invalid_hero")?;
    let duration = digest
        .pointer("/meta/duration_s")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let mut boundary_times = vec![("start".to_owned(), 0.0)];
    for (name, time) in [("ten_minutes", 600.0), ("twenty_five_minutes", 1500.0)] {
        if time <= duration {
            boundary_times.push((name.to_owned(), time));
        }
    }
    if duration > 0.0 {
        boundary_times.push(("match_end".to_owned(), duration));
    }
    let inventories = boundary_times
        .into_iter()
        .map(|(name, time)| {
            let (value, reason) = inventory_at(raw, time);
            json!({"boundary":name,"t":time,"inventory":value,"reason":reason})
        })
        .collect::<Vec<_>>();
    let neutral_items = raw.get("finds").and_then(Value::as_array).map(|rows| rows.iter().filter_map(|row| {
        let values=row.as_array()?;
        let time=values.first().and_then(Value::as_f64).filter(|value|value.is_finite())?;
        let item=values.get(1).and_then(item_name);
        let enchantment=values.get(2).and_then(Value::as_str).filter(|value|valid_token(value,96));
        let tier=values.get(3).and_then(Value::as_u64);
        Some(json!({"time_s":time,"item":item,"item_reason":item.is_none().then_some("neutral item is missing or invalid").map(Value::from),"enchantment":enchantment,"enchantment_reason":enchantment.is_none().then_some("neutral enchantment is missing or invalid").map(Value::from),"tier":tier,"tier_reason":tier.is_none().then_some("neutral item tier is missing").map(Value::from)}))
    }).collect::<Vec<_>>());
    Ok(json!({
        "hero":id,
        "purchases":items,
        "purchases_reason":purchases_reason,
        "final_items":profile.get("final_items").cloned().unwrap_or(Value::Null),
        "final_items_reason":profile.get("final_items_reason").cloned().unwrap_or_else(||Value::from("final inventory is missing")),
        "inventories_at_phase_boundaries":inventories,
        "neutral_items":{"value":neutral_items.clone().map(Value::Array).unwrap_or(Value::Null),"reason":neutral_items.is_none().then_some("neutral item tracks are missing from the bundle").map(Value::from)}
    }))
}

fn get_lane(digest: &Value, input: &Value) -> Result<Value, &'static str> {
    let lane = input
        .get("lane")
        .and_then(Value::as_str)
        .filter(|lane| ["top", "mid", "bot"].contains(lane))
        .ok_or("invalid_lane")?;
    let row = digest
        .get("lanes")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("lane").and_then(Value::as_str) == Some(lane))
        })
        .cloned()
        .ok_or("lane_unknown")?;
    let advantage = digest
        .get("advantage_per_min")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|point| {
            point
                .get("minute")
                .and_then(Value::as_u64)
                .is_some_and(|minute| minute <= 10)
        })
        .cloned()
        .collect::<Vec<_>>();
    Ok(json!({"lane":row,"advantage_per_min":advantage,"threshold_label":"เกณฑ์ชั่วคราว"}))
}

fn get_objectives(digest: &Value, input: &Value) -> Result<Value, &'static str> {
    let (from, to) = match_window(
        digest,
        input,
        input.get("from").is_some() || input.get("to").is_some(),
    )?;
    let Some(objectives) = digest.get("objectives") else {
        return Ok(json!({"rows":null,"reason":"objective data is missing","window_s":[from,to]}));
    };
    if !["buildings", "wards", "other_tracks", "runes"]
        .iter()
        .any(|key| objectives.get(*key).and_then(Value::as_array).is_some())
    {
        return Ok(
            json!({"rows":null,"reason":"objective tracks and rune events are missing","window_s":[from,to]}),
        );
    }
    let mut rows = Vec::new();
    for (kind, key) in [
        ("building", "buildings"),
        ("ward", "wards"),
        ("track", "other_tracks"),
        ("rune", "runes"),
    ] {
        let Some(events) = objectives.get(key).and_then(Value::as_array) else {
            continue;
        };
        for event in events {
            if kind == "building" && event.get("destroyed").and_then(Value::as_bool) == Some(false)
            {
                continue;
            }
            let time = if kind == "building" {
                event.get("destroyed_at_s").and_then(Value::as_f64)
            } else {
                event
                    .get("start_s")
                    .or_else(|| event.get("t"))
                    .and_then(Value::as_f64)
            };
            if kind == "building" && event.get("destroyed").is_some_and(Value::is_null) {
                let start = event.get("start_s").and_then(Value::as_f64);
                let end = event.get("end_s").and_then(Value::as_f64);
                if start
                    .zip(end)
                    .is_some_and(|(start, end)| end < from || start > to)
                {
                    continue;
                }
                let mut row = event.clone();
                row["category"] = Value::from(kind);
                row["event_time_s"] = Value::Null;
                row["event_time_reason"] = event
                    .get("destroyed_at_s_reason")
                    .cloned()
                    .unwrap_or_else(|| Value::from("building destruction time is unknown"));
                rows.push(row);
            } else if time.is_some_and(|time| time >= from && time <= to) {
                let mut row = event.clone();
                row["category"] = Value::from(kind);
                row["event_time_s"] = time.map(Value::from).unwrap_or(Value::Null);
                rows.push(row);
            }
        }
    }
    rows.sort_by(|left, right| {
        left.get("event_time_s")
            .or_else(|| left.get("destroyed_at_s"))
            .or_else(|| left.get("start_s"))
            .or_else(|| left.get("t"))
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY)
            .total_cmp(
                &right
                    .get("event_time_s")
                    .or_else(|| right.get("destroyed_at_s"))
                    .or_else(|| right.get("start_s"))
                    .or_else(|| right.get("t"))
                    .and_then(Value::as_f64)
                    .unwrap_or(f64::INFINITY),
            )
    });
    Ok(json!({"rows":rows,"window_s":[from,to],"reason":null}))
}

fn get_phase_stats(
    digest: &Value,
    input: &Value,
    roster: &BTreeMap<String, String>,
) -> Result<Value, &'static str> {
    let phase = input
        .get("phase")
        .and_then(Value::as_str)
        .filter(|phase| ["laning", "mid", "late"].contains(phase))
        .ok_or("invalid_phase")?;
    let mut heroes = Vec::new();
    for id in input
        .get("heroes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = hero_arg(&json!({"hero":id}), "hero", roster)?;
        let row = digest
            .get("heroes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|hero| hero.get("id").and_then(Value::as_str) == Some(&id))
            .cloned()
            .ok_or("invalid_hero")?;
        let phase_value = row
            .pointer(&format!("/phases/{phase}"))
            .cloned()
            .unwrap_or(Value::Null);
        heroes.push(json!({"hero":id,"phase":phase_value,"phase_reason":phase_value.is_null().then_some("match ended before this phase or the phase data is missing").map(Value::from)}));
    }
    let team = digest
        .pointer(&format!("/team_phases/{phase}"))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(
        json!({"phase":phase,"team":team,"team_reason":team.is_null().then_some("match ended before this phase or the phase data is missing").map(Value::from),"heroes":heroes,"bounds_s":match phase {"laning"=>json!([0,600]),"mid"=>json!([600,1500]),_=>json!([1500,digest.pointer("/meta/duration_s").cloned().unwrap_or(Value::Null)])}}),
    )
}

fn get_teamfight(
    digest: &Value,
    input: &Value,
    roster: &BTreeMap<String, String>,
) -> Result<Value, &'static str> {
    let n = input
        .get("n")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or("invalid_teamfight")?;
    let Some(fight) = digest
        .get("teamfights")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("n").and_then(Value::as_u64) == Some(n))
        })
        .cloned()
    else {
        return Ok(
            json!({"teamfight":null,"reason":digest.get("teamfights_reason").and_then(Value::as_str).unwrap_or("no teamfight has this index")}),
        );
    };
    let death_ids = fight
        .get("deaths")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let deaths = digest
        .get("deaths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|death| {
            death_ids.iter().any(|index| {
                index.as_u64()
                    == death
                        .get("event_ref")
                        .and_then(Value::as_str)
                        .and_then(|v| v.rsplit('/').next())
                        .and_then(|v| v.parse::<u64>().ok())
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    let present = fight
        .get("present")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| roster.contains_key(*id))
        .map(|id| json!({"id":id,"name":roster[id]}))
        .collect::<Vec<_>>();
    Ok(
        json!({"teamfight":fight,"deaths":deaths,"participants":present,"threshold_label":"เกณฑ์ชั่วคราว"}),
    )
}

fn read_saved_analysis(context: &ToolContext<'_>, input: &Value) -> Result<Value, &'static str> {
    let id = input
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 180
                && !id.contains('/')
                && !id.contains('\\')
                && !id.contains(':')
                && !id.contains("..")
        })
        .ok_or("invalid_saved_analysis_id")?;
    let (kind, markdown) = if let Ok(report) = context.store.read_ai_report(context.match_id, id) {
        ("ai_report", report.markdown)
    } else {
        ("note", context.store.read_note(context.match_id, id)?)
    };
    Ok(json!({"id":id,"kind":kind,"markdown":markdown}))
}

pub(crate) fn validate_read_only_input(name: &str, input: &Value) -> Result<(), &'static str> {
    if matches!(name, "write_note") {
        return Err("write_note_requires_chat_handler");
    }
    if allowed_keys(name).is_none() {
        return Err("unknown_tool");
    }
    let object = input.as_object().ok_or("invalid_tool_input")?;
    let allowed = allowed_keys(name).ok_or("unknown_tool")?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("invalid_tool_input");
    }
    if name == "get_hero_series" {
        let from = finite_number(input, "from")?;
        let to = finite_number(input, "to")?;
        if from > to || to - from > MAX_WINDOW_SECONDS {
            return Err("window_out_of_bounds");
        }
        if input
            .get("step_s")
            .and_then(Value::as_u64)
            .is_none_or(|step| step < 10 || step > 1800)
        {
            return Err("invalid_step");
        }
    }
    Ok(())
}

pub(crate) fn cache_ttl_seconds() -> u64 {
    60 * 60
}
pub(crate) fn provisional_thresholds() -> Value {
    json!({"label":"เกณฑ์ชั่วคราว","lane_sample_from_s":PROVISIONAL_THRESHOLDS.lane_sample_from_s,"lane_sample_to_s":PROVISIONAL_THRESHOLDS.lane_sample_to_s})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn test_store() -> (std::path::PathBuf, AnalyticsStore) {
        let path = std::env::temp_dir().join(format!(
            "analytics-tools-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        let store = AnalyticsStore::open(&path, Some([5; 32]));
        (path, store)
    }

    #[test]
    fn tool_schemas_are_sorted_strict_and_never_accept_paths() {
        let tools = definitions(true);
        let names = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        assert!(tools
            .iter()
            .all(|tool| tool["strict"] == true
                && tool["input_schema"]["additionalProperties"] == false));
        let schema = serde_json::to_string(&tools).unwrap();
        assert!(!schema.contains("path"));
        assert!(!definitions(false)
            .iter()
            .any(|tool| tool["name"] == "write_note"));
        assert_eq!(cache_ttl_seconds(), 3_600);
        assert_eq!(provisional_thresholds()["label"], "เกณฑ์ชั่วคราว");
    }

    #[test]
    fn input_validator_rejects_paths_and_unbounded_series() {
        assert_eq!(
            validate_read_only_input("get_lane", &json!({"lane":"mid","path":"C:/x"})),
            Err("invalid_tool_input")
        );
        assert_eq!(
            validate_read_only_input(
                "get_hero_series",
                &json!({"hero":"npc_dota_hero_axe","metric":"nw","from":0,"to":99999,"step_s":10})
            ),
            Err("window_out_of_bounds")
        );
    }

    #[test]
    fn hero_validation_rejects_ids_outside_the_roster() {
        let roster = BTreeMap::from([("npc_dota_hero_axe".to_owned(), "Axe".to_owned())]);
        assert_eq!(
            hero_arg(&json!({"hero":"npc_dota_hero_lina"}), "hero", &roster),
            Err("invalid_hero")
        );
    }

    #[test]
    fn hero_time_rejects_negative_values_instead_of_using_the_default() {
        let digest = json!({"meta":{"duration_s":600},"heroes":[{"id":"npc_dota_hero_axe"}]});
        let match_data = json!({"t0":0,"heroes":[{"id":"npc_dota_hero_axe"}]});
        let roster = BTreeMap::from([("npc_dota_hero_axe".to_owned(), "Axe".to_owned())]);
        assert_eq!(
            get_hero_card(
                &digest,
                &match_data,
                &json!({"hero":"npc_dota_hero_axe","t":-1}),
                &roster
            ),
            Err("invalid_time")
        );
    }

    #[test]
    fn hero_card_reads_bounded_replay_fields_and_keeps_missing_values_explained() {
        let id = "npc_dota_hero_axe";
        let digest = json!({
            "meta":{"duration_s":60},
            "heroes":[{"id":id,"hero":"Axe","final":{},"at_10":{},"phases":{}}]
        });
        let match_data = json!({
            "t0":0,
            "deaths":[
                {"t":0.5,"hero":"npc_dota_hero_lina","killer":id},
                {"t":1,"hero":id,"killer":"npc_dota_hero_lina"}
            ],
            "heroes":[{
                "id":id,"prim":1,
                "alive":[false,true],"lvl":[5,6],"hp":[500,600],"mhp":[700,700],"mp":[200,250],"mmp":[300,300],
                "eco":[[100,2000,20,2],[110,2100,22,3]],
                "st":{"str":[20,20],"strT":[24,24],"agi":[18,19],"agiT":[20,21],"int":[16,16],"intT":[18,18],"dmin":[50,51],"dmax":[56,57],"dbonus":[3,4],"range":[150,150],"bat":[1.7,1.7],"hpreg":[2.5,2.5],"mpreg":[1.0,1.0],"xp":[400,500]},
                "abil":[{"n":"axe_berserkers_call","lv":[1,2],"cd":[0,3],"cl":[16,16],"mc":[80,80]}],
                "inv":[[0,[[0,"item_blink",1],[15,"item_tpscroll",2]]]],
                "icd":{"0":[[0,0],[0,0]],"15":[[0,0],[4,45]]},
                "finds":[[0,"item_blink","enchant_test",1]],
                "mods":[[0,20,"modifier_test", "npc_dota_hero_lina"]]
            }]
        });
        let roster = BTreeMap::from([
            (id.to_owned(), "Axe".to_owned()),
            ("npc_dota_hero_lina".to_owned(), "Lina".to_owned()),
        ]);
        let card = get_hero_card(&digest, &match_data, &json!({"hero":id,"t":1}), &roster).unwrap();
        assert_eq!(card["attr"]["str"]["values"], json!([20, 24]));
        assert_eq!(card["attr"]["primary"]["value"], "agi");
        assert_eq!(card["dmg"]["values"], json!([51, 57, 4]));
        assert_eq!(card["kd"]["value"], json!([1, 1]));
        assert_eq!(card["abilities"]["value"][0]["level"], 2);
        assert_eq!(card["items"]["value"]["tp"]["item"], "item_tpscroll");
        assert_eq!(
            card["items"]["value"]["tp"]["cooldown_s"].as_f64(),
            Some(4.0)
        );
        assert_eq!(
            card["buffs"]["value"][0]["source_hero"],
            "npc_dota_hero_lina"
        );
        assert!(card["derived"]["value"].is_null());
        assert!(card["derived"]["reason"].as_str().is_some());
        assert!(card["respawn_in_s"].is_null());
        assert_eq!(card["respawn_in_s_reason"], "hero is alive at this time");
    }

    #[test]
    fn items_tool_returns_purchase_inventory_boundaries_and_neutral_rows() {
        let id = "npc_dota_hero_axe";
        let digest = json!({
            "meta":{"duration_s":900},
            "heroes":[{"id":id,"items":[[120,"item_blink"]],"final_items":["item_blink"]}]
        });
        let match_data = json!({
            "heroes":[{
                "id":id,
                "buys":[[120,"item_blink"]],
                "finds":[[300,"item_neutral_test","enchant_test",1]],
                "inv":[[0,[[0,"item_tango",3]]],[600,[[0,"item_blink",0]]]]
            }]
        });
        let roster = BTreeMap::from([(id.to_owned(), "Axe".to_owned())]);
        let result = get_items(&digest, &match_data, &json!({"hero":id}), &roster).unwrap();
        assert_eq!(
            result["inventories_at_phase_boundaries"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            result["inventories_at_phase_boundaries"][1]["inventory"]["main"][0]["item"],
            "item_blink"
        );
        assert_eq!(
            result["neutral_items"]["value"][0]["time_s"].as_f64(),
            Some(300.0)
        );
        assert!(result["neutral_items"]["value"][0]["tier_reason"].is_null());
    }

    #[test]
    fn long_series_is_downsampled_across_the_entire_window() {
        let (step, times) = bounded_sample_times(0.0, 1_800.0, 10);
        assert!(times.len() <= MAX_SERIES_POINTS);
        assert!(step > 10);
        assert_eq!(times.first(), Some(&0));
        assert_eq!(times.last(), Some(&1_800));
    }

    #[test]
    fn oversized_saved_analysis_is_capped_in_utf8_bytes() {
        let result = cap_result(json!({"id":"safe-id","markdown":"ก".repeat(10_000)}));
        assert_eq!(result["truncated"], true);
        assert!(serde_json::to_vec(&result).unwrap().len() <= MAX_TOOL_RESULT_BYTES);
    }

    #[test]
    fn read_tools_report_replay_removed_after_bundle_eviction() {
        let (path, store) = test_store();
        let digest = json!({"meta":{"duration_s":600}});
        let context = ToolContext {
            match_id: "12345",
            digest: &digest,
            bundle: None,
            store: &store,
        };
        assert_eq!(
            execute(&context, "get_lane", &json!({"lane":"mid"})),
            Err("replay_removed")
        );
        drop(context);
        drop(store);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn saved_analysis_ids_reject_paths() {
        let (path, store) = test_store();
        let digest = Value::Null;
        let context = ToolContext {
            match_id: "12345",
            digest: &digest,
            bundle: None,
            store: &store,
        };
        assert_eq!(
            execute(
                &context,
                "read_saved_analysis",
                &json!({"id":"..\\outside"})
            ),
            Err("invalid_saved_analysis_id")
        );
        drop(context);
        drop(store);
        let _ = fs::remove_dir_all(path);
    }
}
