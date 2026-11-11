//! Conversion from admitted GSI fields to the event detector's tick shape.

use crate::{Phase, Tick};
use kohbor_gsi::admission::{Admitted, Field, Phase as GsiPhase};

/// Build an event-detector tick from source-admitted values.
///
/// Missing and invalid source fields stay unknown. Integer values that cannot be
/// represented by the detector's `u32` fields also stay unknown.
pub fn tick_from_admitted(
    admitted: &Admitted,
    at_ms: u64,
    session_epoch: u64,
    trusted: bool,
) -> Tick {
    Tick {
        at_ms,
        session_epoch,
        phase: match admitted.phase {
            GsiPhase::Draft => Phase::Draft,
            GsiPhase::Pregame => Phase::Pregame,
            GsiPhase::Live => Phase::Live,
            GsiPhase::PostGame => Phase::PostGame,
        },
        trusted,
        kills: to_u32(&admitted.counters.kills),
        deaths: to_u32(&admitted.counters.deaths),
        kill_streak: to_u32(&admitted.counters.kill_streak),
        combined_score: to_u32(&admitted.counters.combined_score),
        level: to_u32(&admitted.hero.level),
        hp: to_u32(&admitted.hero.hp),
        max_hp: to_u32(&admitted.hero.max_hp),
        mana_percent: to_u32(&admitted.hero.mana_percent),
        alive: to_bool(&admitted.hero.alive),
    }
}

fn to_u32(field: &Field<i64>) -> Option<u32> {
    match field {
        Field::Value(value) => u32::try_from(*value).ok(),
        Field::Missing | Field::Invalid => None,
    }
}

fn to_bool(field: &Field<bool>) -> Option<bool> {
    match field {
        Field::Value(value) => Some(*value),
        Field::Missing | Field::Invalid => None,
    }
}
