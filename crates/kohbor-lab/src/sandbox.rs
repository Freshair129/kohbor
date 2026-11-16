//! Offline replay sandbox: run recorded ticks through a detector with chosen parameters
//! and report events plus the explanation of every tick (KOHBOR-LAB.md section 5.5).
//!
//! Pure: ticks come in as data. Results are evidence class "recorded or synthetic,
//! offline"; they say nothing about a live match.

use kohbor_events::{Config, Detector, Event, Phase, Tick};
use serde_json::{json, Value};

fn field_u32(row: &Value, key: &str) -> Result<Option<u32>, String> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("{key} must be a non-negative integer or null")),
    }
}

fn field_bool(row: &Value, key: &str) -> Result<Option<bool>, String> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("{key} must be a boolean or null")),
    }
}

/// Tick rows: `{at_ms, phase, session_epoch?, trusted?, kills?, deaths?, kill_streak?,
/// combined_score?, level?, hp?, max_hp?, mana_percent?, alive?}`. A missing or null field is
/// unknown, never zero.
pub fn ticks_from_json(rows: &Value) -> Result<Vec<Tick>, String> {
    let rows = rows.as_array().ok_or("ticks must be an array")?;
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let at_ms = row
                .get("at_ms")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("tick {index}: at_ms must be a non-negative integer"))?;
            let phase = match row.get("phase").and_then(Value::as_str) {
                Some("draft") => Phase::Draft,
                Some("pregame") => Phase::Pregame,
                Some("live") => Phase::Live,
                Some("postgame") => Phase::PostGame,
                _ => {
                    return Err(format!(
                        "tick {index}: phase must be draft|pregame|live|postgame"
                    ))
                }
            };
            let wrap = |e: String| format!("tick {index}: {e}");
            Ok(Tick {
                at_ms,
                session_epoch: row
                    .get("session_epoch")
                    .and_then(Value::as_u64)
                    .unwrap_or(1),
                phase,
                trusted: field_bool(row, "trusted").map_err(wrap)?.unwrap_or(true),
                kills: field_u32(row, "kills").map_err(wrap)?,
                deaths: field_u32(row, "deaths").map_err(wrap)?,
                kill_streak: field_u32(row, "kill_streak").map_err(wrap)?,
                combined_score: field_u32(row, "combined_score").map_err(wrap)?,
                level: field_u32(row, "level").map_err(wrap)?,
                hp: field_u32(row, "hp").map_err(wrap)?,
                max_hp: field_u32(row, "max_hp").map_err(wrap)?,
                mana_percent: field_u32(row, "mana_percent").map_err(wrap)?,
                alive: field_bool(row, "alive").map_err(wrap)?,
            })
        })
        .collect()
}

fn event_json(event: &Event) -> Value {
    json!({"id": event.id.as_str(), "tier": event.tier, "at_ms": event.at_ms})
}

/// Replay with a fresh detector. `fired` lists events in order; `ticks` holds the per-tick
/// explanation.
pub fn replay_json(ticks: &[Tick], config: &Config) -> Value {
    let mut detector = Detector::with_config(config.clone());
    let mut fired = Vec::new();
    let mut rows = Vec::with_capacity(ticks.len());
    for tick in ticks {
        let (events, trace) = detector.observe_traced(tick);
        fired.extend(events.iter().map(event_json));
        rows.push(json!({"at_ms": tick.at_ms, "trace": trace.to_json()}));
    }
    json!({"evidence_class": "offline replay (synthetic or recorded); not live", "fired": fired, "ticks": rows})
}

fn events_of(ticks: &[Tick], config: &Config) -> Vec<Value> {
    let mut detector = Detector::with_config(config.clone());
    ticks
        .iter()
        .flat_map(|tick| detector.observe(tick))
        .map(|event| event_json(&event))
        .collect()
}

/// Events the `edited` parameters lose or gain compared with `base`.
pub fn compare(ticks: &[Tick], base: &Config, edited: &Config) -> Value {
    let before = events_of(ticks, base);
    let after = events_of(ticks, edited);
    let lost: Vec<&Value> = before.iter().filter(|e| !after.contains(e)).collect();
    let gained: Vec<&Value> = after.iter().filter(|e| !before.contains(e)).collect();
    json!({
        "base_events": before.len(),
        "edited_events": after.len(),
        "lost": lost,
        "gained": gained,
    })
}
