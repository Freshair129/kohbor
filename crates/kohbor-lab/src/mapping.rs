//! Dev "pack mapping" draft: which clip of the active pack the owner wants for each event.
//!
//! It is a PREVIEW / EXPORT artifact in the format the pack rebuild tool reads
//! (`{"schema_version":"1.0.0","bindings":[{"event_id","tier"?,"audio"}]}`); the shipped mapping
//! stays the pack's own descriptor. Pure validation, no I/O (the dev file write lives in
//! `dev_override`).

use crate::profile::{catalog, err, strict, ProfileError};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const SCHEMA_VERSION: &str = "1.0.0";
const MAX_BINDINGS: usize = 128;
const MAX_AUDIO_NAME: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub event_id: String,
    pub tier: Option<u8>,
    pub audio: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mapping {
    pub bindings: Vec<Binding>,
}

/// A plain `.wav` file name: letters, digits, `.`, `_`, `-`; no separators, no drive letters.
fn valid_audio_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".wav") else {
        return false;
    };
    !stem.is_empty()
        && name.len() <= MAX_AUDIO_NAME
        && !stem.starts_with('.')
        && !stem.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn tier_range(event: &Value) -> Option<(i64, i64)> {
    let range = event["tier_range"].as_object()?;
    Some((range.get("min")?.as_i64()?, range.get("max")?.as_i64()?))
}

pub fn parse(text: &str) -> Result<Mapping, ProfileError> {
    let root = strict(text.as_bytes())?;
    for key in root.keys() {
        if key != "schema_version" && key != "bindings" {
            return Err(err("UNKNOWN_FIELD", format!("unknown field {key}")));
        }
    }
    if root.get("schema_version").and_then(Value::as_str) != Some(SCHEMA_VERSION) {
        return Err(err("SCHEMA_VERSION", "schema_version must be \"1.0.0\""));
    }
    let rows = root
        .get("bindings")
        .and_then(Value::as_array)
        .ok_or_else(|| err("BINDINGS_TYPE", "bindings must be an array"))?;
    if rows.len() > MAX_BINDINGS {
        return Err(err(
            "TOO_MANY_BINDINGS",
            format!("at most {MAX_BINDINGS} bindings"),
        ));
    }
    let events = catalog()["events"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut seen = BTreeSet::new();
    let mut bindings = Vec::with_capacity(rows.len());
    for row in rows {
        let row = row
            .as_object()
            .ok_or_else(|| err("BINDINGS_TYPE", "each binding must be an object"))?;
        for key in row.keys() {
            if !["event_id", "tier", "audio"].contains(&key.as_str()) {
                return Err(err(
                    "UNKNOWN_FIELD",
                    format!("binding field {key} is not supported"),
                ));
            }
        }
        let event_id = row
            .get("event_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(event) = events.iter().find(|e| e["id"] == event_id) else {
            return Err(err(
                "UNKNOWN_EVENT",
                format!("{event_id:?} is not a catalog event"),
            ));
        };
        if event["media_mappable"] != true {
            return Err(err(
                "NOT_MAPPABLE",
                format!("{event_id} cannot carry media"),
            ));
        }
        let tier = match (row.get("tier"), tier_range(event)) {
            (None, None) => None,
            (None, Some(_)) => {
                return Err(err("TIER_REQUIRED", format!("{event_id} needs a tier")))
            }
            (Some(_), None) => {
                return Err(err("TIER_NOT_ALLOWED", format!("{event_id} has no tiers")));
            }
            (Some(value), Some((low, high))) => {
                let Some(tier) = value.as_i64() else {
                    return Err(err("TIER_NOT_ALLOWED", "tier must be an integer"));
                };
                if tier < low || tier > high {
                    return Err(err(
                        "TIER_OUT_OF_RANGE",
                        format!("{event_id} tier {tier} is outside {low}..{high}"),
                    ));
                }
                Some(tier as u8)
            }
        };
        let audio = row.get("audio").and_then(Value::as_str).unwrap_or_default();
        if !valid_audio_name(audio) {
            return Err(err(
                "BAD_AUDIO_NAME",
                "audio must be a plain .wav file name",
            ));
        }
        if !seen.insert((event_id.to_owned(), tier)) {
            return Err(err(
                "DUPLICATE_BINDING",
                format!("{event_id} is bound twice"),
            ));
        }
        bindings.push(Binding {
            event_id: event_id.to_owned(),
            tier,
            audio: audio.to_owned(),
        });
    }
    Ok(Mapping { bindings })
}

impl Mapping {
    /// The rebuild-tool format; `tier` appears only where there is one.
    pub fn to_json(&self) -> String {
        let rows: Vec<Value> = self
            .bindings
            .iter()
            .map(|b| match b.tier {
                Some(tier) => json!({"event_id": b.event_id, "tier": tier, "audio": b.audio}),
                None => json!({"event_id": b.event_id, "audio": b.audio}),
            })
            .collect();
        serde_json::to_string_pretty(&json!({"schema_version": SCHEMA_VERSION, "bindings": rows}))
            .expect("serializable")
            + "\n"
    }
}
