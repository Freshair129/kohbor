//! Own-snapshot read model, contracts/own-snapshot.schema.json 2.1.0.

use crate::admission::{Admitted, Field, Outcome, Ownership, Phase, Team};
use serde::Serialize;
use serde_json::{json, Value};

/// Fresh while age ≤ this many milliseconds (DATA-001).
pub const FRESH_MAX_AGE_MS: u64 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Availability {
    Fresh,
    Stale,
    Unknown,
    Invalid,
    Unsupported,
}

/// Source bookkeeping supplied by the intake owner.
#[derive(Debug, Clone, Copy)]
pub struct SourceState {
    pub generation: u64,
    pub sequence: Option<u64>,
    pub age_ms: Option<u64>,
    pub session_epoch: Option<u64>,
    pub identity_revision: u64,
}

fn age_availability(age_ms: u64) -> Availability {
    if age_ms <= FRESH_MAX_AGE_MS {
        Availability::Fresh
    } else {
        Availability::Stale
    }
}

fn field_json<T: Serialize>(f: &Field<T>, observed: Availability) -> Value {
    match f {
        Field::Value(v) => json!({"value": v, "availability": observed}),
        Field::Missing => json!({"value": null, "availability": Availability::Unknown}),
        Field::Invalid => json!({"value": null, "availability": Availability::Invalid}),
    }
}

/// Build the snapshot for the latest admission outcome. Values are only exposed for
/// verified ownership in pregame/live phases; everything else is an explicit state.
pub fn build(outcome: &Outcome, src: SourceState) -> Value {
    let base = |availability: Availability,
                own: Value,
                epoch: Option<u64>,
                seq: Option<u64>,
                age: Option<u64>| {
        json!({
            "schema_version": "2.1.0",
            "session_epoch": epoch,
            "availability": availability,
            "source": {"kind": "gsi", "generation": src.generation, "sequence": seq, "age_ms": age},
            "own": own,
        })
    };
    let admitted: &Admitted = match outcome {
        Outcome::Admitted(a) => a,
        Outcome::Unsupported(_) => {
            return base(Availability::Unsupported, Value::Null, None, None, None)
        }
        Outcome::Rejected(_) => return base(Availability::Invalid, Value::Null, None, None, None),
        Outcome::NotPlaying => return base(Availability::Unknown, Value::Null, None, None, None),
    };
    if admitted.ownership != Ownership::Verified {
        return base(Availability::Invalid, Value::Null, None, None, None);
    }
    let (Some(epoch), Some(seq), Some(age)) = (src.session_epoch, src.sequence, src.age_ms) else {
        return base(Availability::Unknown, Value::Null, None, None, None);
    };
    if !matches!(admitted.phase, Phase::Pregame | Phase::Live) {
        return base(Availability::Unknown, Value::Null, Some(epoch), None, None);
    }
    let observed = age_availability(age);
    let h = &admitted.hero;
    let c = &admitted.counters;
    let own = json!({
        "identity_revision": src.identity_revision,
        "team": admitted.team.map(|t| match t { Team::Radiant => "radiant", Team::Dire => "dire" }),
        "fields": {
            "hero_id": field_json(&h.hero_id, observed),
            "level": field_json(&h.level, observed),
            "hp": field_json(&h.hp, observed),
            "max_hp": field_json(&h.max_hp, observed),
            "mana_percent": field_json(&h.mana_percent, observed),
            "alive": field_json(&h.alive, observed),
            "respawn_seconds": field_json(&h.respawn_seconds, observed),
            "gold": field_json(&c.gold, observed),
            "kills": field_json(&c.kills, observed),
            "deaths": field_json(&c.deaths, observed),
            "assists": field_json(&c.assists, observed),
            "last_hits": field_json(&c.last_hits, observed),
            "denies": field_json(&c.denies, observed),
            "gpm": field_json(&c.gpm, observed),
            "xpm": field_json(&c.xpm, observed),
        }
    });
    base(observed, own, Some(epoch), Some(seq), Some(age))
}
