use crate::{EventPayload, Phase, Tick};
use kohbor_gsi::admission::{AbilityObservation, Field};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const DOTA_STATS_JSON: &str =
    include_str!("../../../apps/desktop/src/assets/dota/6989/stats.json");

// Provisional owner-check window: review in play, and do not tune after seeing a result.
pub(crate) const ULTIMATE_KILL_WINDOW_MS: u64 = 4_000;

static LOCAL_ULTIMATES: OnceLock<HashMap<String, HashSet<String>>> = OnceLock::new();

/// GSI-only input kept separate from the stable ordinary [`Tick`] contract.
#[derive(Debug, Clone, PartialEq)]
pub struct UltimateContext {
    pub generation: u64,
    pub hero_id: Option<String>,
    pub abilities: Field<Vec<AbilityObservation>>,
}

impl Default for UltimateContext {
    fn default() -> Self {
        Self {
            generation: 0,
            hero_id: None,
            abilities: Field::Missing,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct UltimateTracker {
    observed_hero: Option<String>,
    baseline: Option<AbilityBaseline>,
    last_fired: Option<FiredUltimate>,
}

#[derive(Debug, Clone, PartialEq)]
struct AbilityBaseline {
    hero: String,
    ability: String,
    level: Field<i64>,
    cooldown: Field<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FiredUltimate {
    at_ms: u64,
    session_epoch: u64,
    generation: u64,
    hero: String,
    ability: String,
}

impl UltimateTracker {
    pub(crate) fn reset_to_baseline(&mut self, tick: &Tick, context: &UltimateContext) {
        self.observed_hero = valid_hero(context.hero_id.as_deref()).map(str::to_owned);
        self.baseline = (tick.alive == Some(true))
            .then(|| current_ability_baseline(tick, context))
            .flatten();
        self.last_fired = None;
    }

    pub(crate) fn observe(
        &mut self,
        tick: &Tick,
        context: &UltimateContext,
        own_hero_kill: bool,
    ) -> (Option<EventPayload>, Option<EventPayload>) {
        let hero = valid_hero(context.hero_id.as_deref()).map(str::to_owned);
        let hero_changed = hero != self.observed_hero;
        if hero_changed {
            self.observed_hero = hero.clone();
            self.baseline = None;
            self.last_fired = None;
        }

        if tick.phase != Phase::Live || hero.is_none() {
            self.baseline = None;
            self.last_fired = None;
            return (None, None);
        }

        if hero_changed {
            self.baseline = current_ability_baseline(tick, context);
            return (None, None);
        }

        // Cooldown transitions observed while dead or with unknown life state do not qualify.
        let current = (tick.alive == Some(true))
            .then(|| current_ability_baseline(tick, context))
            .flatten();
        let mut fired_payload = None;
        if let (Some(previous), Some(current_state)) = (self.baseline.as_ref(), current.as_ref()) {
            let same_pair = previous.hero == current_state.hero
                && previous.ability == current_state.ability
                && previous.level == current_state.level;
            let cooldown_started = matches!(
                (&previous.cooldown, &current_state.cooldown),
                (Field::Value(0) | Field::Missing, Field::Value(now)) if *now > 0
            );
            if same_pair
                && tick.alive == Some(true)
                && matches!(&current_state.level, Field::Value(level) if *level >= 1)
                && cooldown_started
            {
                let fired = FiredUltimate {
                    at_ms: tick.at_ms,
                    session_epoch: tick.session_epoch,
                    generation: context.generation,
                    hero: current_state.hero.clone(),
                    ability: current_state.ability.clone(),
                };
                fired_payload = Some(EventPayload {
                    hero: fired.hero.clone(),
                    ability: fired.ability.clone(),
                });
                self.last_fired = Some(fired);
            }
        }
        self.baseline = current;

        let kill_payload = if own_hero_kill {
            self.last_fired.as_ref().and_then(|fired| {
                let elapsed = tick.at_ms.checked_sub(fired.at_ms)?;
                (fired.session_epoch == tick.session_epoch
                    && fired.generation == context.generation
                    && fired.hero == hero.as_deref().unwrap_or_default()
                    && elapsed <= ULTIMATE_KILL_WINDOW_MS)
                    .then(|| EventPayload {
                        hero: fired.hero.clone(),
                        ability: fired.ability.clone(),
                    })
            })
        } else {
            None
        };

        if self.last_fired.as_ref().is_some_and(|fired| {
            tick.at_ms
                .checked_sub(fired.at_ms)
                .is_none_or(|elapsed| elapsed > ULTIMATE_KILL_WINDOW_MS)
        }) {
            self.last_fired = None;
        }

        (fired_payload, kill_payload)
    }
}

fn current_ability_baseline(tick: &Tick, context: &UltimateContext) -> Option<AbilityBaseline> {
    if tick.phase != Phase::Live {
        return None;
    }
    let hero = valid_hero(context.hero_id.as_deref())?;
    let Field::Value(abilities) = &context.abilities else {
        return None;
    };

    let mut matches = abilities.iter().filter_map(|ability| {
        let Field::Value(name) = &ability.name else {
            return None;
        };
        if !valid_identifier(name) {
            return None;
        }
        let is_ultimate = match &ability.ultimate {
            Field::Value(true) => true,
            Field::Value(false) | Field::Invalid => false,
            Field::Missing => local_stats_marks_ultimate(hero, name),
        };
        is_ultimate.then(|| AbilityBaseline {
            hero: hero.to_owned(),
            ability: name.clone(),
            level: ability.level.clone(),
            cooldown: ability.cooldown.clone(),
        })
    });
    let candidate = matches.next()?;
    matches.next().is_none().then_some(candidate)
}

fn valid_hero(value: Option<&str>) -> Option<&str> {
    let value = value?;
    let suffix = value.strip_prefix("npc_dota_hero_")?;
    valid_identifier(suffix).then_some(value)
}

fn valid_identifier(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn local_stats_marks_ultimate(hero: &str, ability: &str) -> bool {
    let Some(hero_name) = hero.strip_prefix("npc_dota_hero_") else {
        return false;
    };
    local_ultimates()
        .get(hero_name)
        .is_some_and(|abilities| abilities.contains(ability))
}

fn local_ultimates() -> &'static HashMap<String, HashSet<String>> {
    LOCAL_ULTIMATES.get_or_init(|| {
        let Ok(stats) = serde_json::from_str::<Value>(DOTA_STATS_JSON) else {
            return HashMap::new();
        };
        let Some(heroes) = stats.get("heroes").and_then(Value::as_object) else {
            return HashMap::new();
        };
        heroes
            .iter()
            .filter_map(|(hero, details)| {
                let abilities = details.get("abilities")?.as_array()?;
                let ultimate_names: HashSet<_> = abilities
                    .iter()
                    .filter(|ability| ability.get("ultimate").and_then(Value::as_bool) == Some(true))
                    .filter_map(|ability| ability.get("name").and_then(Value::as_str))
                    .filter(|name| valid_identifier(name))
                    .map(str::to_owned)
                    .collect();
                (!ultimate_names.is_empty()).then(|| (hero.clone(), ultimate_names))
            })
            .collect()
    })
}

