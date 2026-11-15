//! Read-only explanation of one detector tick (docs/design/KOHBOR-LAB.md section 3).
//!
//! `Detector::observe` fills a small scratch record at each decision point; this module
//! turns that record into guard rows. The scratch is plain data written by the same
//! statements that decide the event, so the explanation cannot drift from the decision:
//! tests/trace_parity.rs checks `fired == all guards pass` on every tick of the golden
//! traces and several synthetic walks.

use crate::{percent_at_or_below, Config, Detector, Event, EventId, Tick};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Status {
    #[default]
    Evaluated,
    Untrusted,
    Baseline,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Evaluated => "evaluated",
            Self::Untrusted => "untrusted",
            Self::Baseline => "baseline",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Threshold {
    pub was_observed: bool,
    pub armed: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Danger {
    pub best_drop: Option<f64>,
    pub dropped_enough: bool,
    pub armed: bool,
    pub cooldown_remaining_ms: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LevelEvidence {
    pub previous: Option<u32>,
    pub current: Option<u32>,
}

/// What the last `observe` call saw at each decision point. Private to the crate.
#[derive(Debug, Clone, Default)]
pub(crate) struct Scratch {
    pub status: Status,
    pub alive: Option<bool>,
    /// `(hp, max_hp)` only when valid (`max_hp > 0`, `hp <= max_hp`).
    pub hp_valid: Option<(u32, u32)>,
    pub mana_percent: Option<u32>,
    pub health: Option<Threshold>,
    pub mana: Option<Threshold>,
    pub danger: Option<Danger>,
    pub level: Option<LevelEvidence>,
    pub level_regressed: bool,
    pub phase_live: bool,
    pub kill_delta: Option<u32>,
    pub kills_regressed: bool,
    pub died: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Guard {
    pub name: String,
    /// Observed value; `None` = unknown (never zero).
    pub lhs: Option<f64>,
    pub op: &'static str,
    pub rhs: Option<f64>,
    pub unit: &'static str,
    /// `None` = could not be evaluated (unknown input).
    pub pass: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventTrace {
    pub id: &'static str,
    pub fired: bool,
    pub guards: Vec<Guard>,
    pub note: Option<&'static str>,
}

impl EventTrace {
    /// `true` when there is at least one guard and every guard is known and passing.
    pub fn all_guards_pass(&self) -> bool {
        !self.guards.is_empty() && self.guards.iter().all(|g| g.pass == Some(true))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    pub at_ms: u64,
    /// `evaluated`, `untrusted` (tick ignored) or `baseline` (state reset, no events).
    pub status: &'static str,
    pub events: Vec<EventTrace>,
}

impl Trace {
    pub fn event(&self, id: &str) -> Option<&EventTrace> {
        self.events.iter().find(|row| row.id == id)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "at_ms": self.at_ms,
            "status": self.status,
            "events": self.events.iter().map(|row| json!({
                "id": row.id,
                "fired": row.fired,
                "note": row.note,
                "guards": row.guards.iter().map(|g| json!({
                    "name": g.name, "lhs": g.lhs, "op": g.op, "rhs": g.rhs,
                    "unit": g.unit, "pass": g.pass,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }
}

fn flag(value: Option<bool>) -> Option<f64> {
    value.map(|v| if v { 1.0 } else { 0.0 })
}

fn guard(
    name: impl Into<String>,
    lhs: Option<f64>,
    op: &'static str,
    rhs: Option<f64>,
    unit: &'static str,
    pass: Option<bool>,
) -> Guard {
    Guard {
        name: name.into(),
        lhs,
        op,
        rhs,
        unit,
        pass,
    }
}

fn percent(hp: u32, max_hp: u32) -> f64 {
    f64::from(hp) * 100.0 / f64::from(max_hp)
}

impl Detector {
    /// Same events as [`Detector::observe`], plus the explanation of the tick.
    pub fn observe_traced(&mut self, tick: &Tick) -> (Vec<Event>, Trace) {
        let events = self.observe(tick);
        let trace = self.build_trace(tick.at_ms, &events);
        (events, trace)
    }

    /// The parameters in force (read-only), for display next to live values.
    pub fn config(&self) -> &Config {
        &self.config
    }

    fn build_trace(&self, at_ms: u64, events: &[Event]) -> Trace {
        let scratch = &self.scratch;
        let fired = |id: EventId| events.iter().any(|event| event.id == id);
        let config = &self.config;
        let alive = guard(
            "hero alive",
            flag(scratch.alive),
            "==",
            Some(1.0),
            "bool",
            scratch.alive,
        );
        let hp_pct = scratch.hp_valid.map(|(hp, max)| percent(hp, max));
        let mut rows = Vec::new();

        let hp_threshold = |threshold: u32| {
            let pass = scratch
                .hp_valid
                .map(|(hp, max)| percent_at_or_below(hp, max, threshold));
            guard("hp %", hp_pct, "<=", Some(f64::from(threshold)), "%", pass)
        };
        let armed_guard = |armed: Option<bool>, rearm: u32, what: &str| {
            guard(
                format!("armed (re-arms at ≥ {rearm}% {what})"),
                flag(armed),
                "==",
                Some(1.0),
                "bool",
                armed,
            )
        };

        // health_danger
        let danger = scratch.danger;
        rows.push(EventTrace {
            id: EventId::HealthDanger.as_str(),
            fired: fired(EventId::HealthDanger),
            guards: vec![
                alive.clone(),
                hp_threshold(config.danger_max_hp_percent),
                guard(
                    format!("drop in last {} ms", config.danger_window_ms),
                    danger.and_then(|d| d.best_drop),
                    ">=",
                    Some(f64::from(config.danger_drop_points)),
                    "points",
                    danger.map(|d| d.dropped_enough),
                ),
                armed_guard(danger.map(|d| d.armed), config.danger_rearm_percent, "hp"),
                guard(
                    format!(
                        "cooldown {} ms elapsed (remaining)",
                        config.danger_cooldown_ms
                    ),
                    danger.map(|d| d.cooldown_remaining_ms as f64),
                    "==",
                    Some(0.0),
                    "ms",
                    danger.map(|d| d.cooldown_remaining_ms == 0),
                ),
            ],
            note: None,
        });

        // health_low
        let health = scratch.health;
        rows.push(EventTrace {
            id: EventId::HealthLow.as_str(),
            fired: fired(EventId::HealthLow),
            guards: vec![
                alive.clone(),
                hp_threshold(config.health_low_threshold_percent),
                armed_guard(
                    health.map(|h| h.armed),
                    config.health_low_rearm_percent,
                    "hp",
                ),
                guard(
                    "previous sample known",
                    flag(health.map(|h| h.was_observed)),
                    "==",
                    Some(1.0),
                    "bool",
                    health.map(|h| h.was_observed),
                ),
            ],
            note: None,
        });

        // mana_low
        let mana = scratch.mana;
        rows.push(EventTrace {
            id: EventId::ManaLow.as_str(),
            fired: fired(EventId::ManaLow),
            guards: vec![
                alive.clone(),
                guard(
                    "mana %",
                    scratch.mana_percent.map(f64::from),
                    "<=",
                    Some(f64::from(config.mana_low_threshold_percent)),
                    "%",
                    scratch
                        .mana_percent
                        .map(|m| m <= config.mana_low_threshold_percent),
                ),
                armed_guard(mana.map(|m| m.armed), config.mana_low_rearm_percent, "mana"),
                guard(
                    "previous sample known",
                    flag(mana.map(|m| m.was_observed)),
                    "==",
                    Some(1.0),
                    "bool",
                    mana.map(|m| m.was_observed),
                ),
            ],
            note: None,
        });

        // level_milestone
        let level = scratch.level;
        let (previous, current) = level.map_or((None, None), |l| (l.previous, l.current));
        let both = previous.zip(current);
        let next_milestone =
            previous.and_then(|p| config.levels.iter().copied().filter(|m| *m > p).min());
        let crossed = both.map(|(p, c)| {
            !scratch.level_regressed && config.levels.iter().any(|m| p < *m && c >= *m)
        });
        rows.push(EventTrace {
            id: EventId::LevelMilestone.as_str(),
            fired: fired(EventId::LevelMilestone),
            guards: vec![
                guard(
                    "level increased",
                    current.map(f64::from),
                    ">",
                    previous.map(f64::from),
                    "level",
                    both.map(|(p, c)| c > p),
                ),
                guard(
                    "reached next milestone",
                    current.map(f64::from),
                    ">=",
                    next_milestone.map(f64::from),
                    "level",
                    crossed,
                ),
            ],
            note: None,
        });

        // hero_kill
        let kill_pass = if scratch.kills_regressed {
            Some(false)
        } else {
            scratch.kill_delta.map(|d| d > 0)
        };
        rows.push(EventTrace {
            id: EventId::HeroKill.as_str(),
            fired: fired(EventId::HeroKill),
            guards: vec![
                guard(
                    "phase is live",
                    Some(if scratch.phase_live { 1.0 } else { 0.0 }),
                    "==",
                    Some(1.0),
                    "bool",
                    Some(scratch.phase_live),
                ),
                guard(
                    "kill counter increased",
                    scratch.kill_delta.map(f64::from),
                    ">",
                    Some(0.0),
                    "kills",
                    kill_pass,
                ),
            ],
            note: None,
        });

        // hero_death
        rows.push(EventTrace {
            id: EventId::HeroDeath.as_str(),
            fired: fired(EventId::HeroDeath),
            guards: vec![guard(
                "death detected (counter; alive→dead only if no counter)",
                Some(if scratch.died { 1.0 } else { 0.0 }),
                "==",
                Some(1.0),
                "bool",
                Some(scratch.died),
            )],
            note: None,
        });

        // Events decided by native chain/state logic: reported, not decomposed in L1.
        for id in [
            EventId::MultiKill,
            EventId::Streak,
            EventId::FirstBlood,
            EventId::HeroRespawn,
            EventId::MatchStarted,
            EventId::MatchEnded,
        ] {
            rows.push(EventTrace {
                id: id.as_str(),
                fired: fired(id),
                guards: Vec::new(),
                note: Some("native chain/state logic; guards not exposed in L1 (EVENT-AUDIO-SPEC section 2)"),
            });
        }

        Trace {
            at_ms,
            status: scratch.status.as_str(),
            events: rows,
        }
    }
}
