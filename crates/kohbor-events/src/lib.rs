//! Kohbor event detector and arbiter (docs/design/EVENT-AUDIO-SPEC.md sections 2 and 3).
//!
//! Pure logic. The caller supplies one [`Tick`] per admitted payload with the
//! monotonic arrival time already attached; this crate never reads a clock, opens a
//! file or touches the network.

pub mod adapter;

use std::collections::VecDeque;

/// Catalog events the detector can emit (taxonomy 1.1, contracts/event-catalog.json).
/// Declaration order is the order in which one tick's events are returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventId {
    MultiKill,
    Streak,
    FirstBlood,
    HeroKill,
    HeroDeath,
    LevelMilestone,
    HealthLow,
    ManaLow,
    MatchEnded,
    MatchStarted,
    HeroRespawn,
    HealthDanger,
}

impl EventId {
    /// Catalog `id` string, for example `"multi_kill"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MultiKill => "multi_kill",
            Self::Streak => "streak",
            Self::FirstBlood => "first_blood",
            Self::HeroKill => "hero_kill",
            Self::HeroDeath => "hero_death",
            Self::LevelMilestone => "level_milestone",
            Self::HealthLow => "health_low",
            Self::ManaLow => "mana_low",
            Self::MatchEnded => "match_ended",
            Self::MatchStarted => "match_started",
            Self::HeroRespawn => "hero_respawn",
            Self::HealthDanger => "health_danger",
        }
    }

    /// Ordinary rank from the catalog; `None` for unranked events.
    pub fn rank(self) -> Option<u32> {
        match self {
            Self::MultiKill => Some(90),
            Self::Streak => Some(80),
            Self::FirstBlood => Some(70),
            Self::HeroKill => Some(60),
            Self::HeroDeath => Some(50),
            Self::LevelMilestone => Some(40),
            Self::HealthLow => Some(30),
            Self::ManaLow => Some(20),
            Self::MatchEnded => Some(15),
            Self::MatchStarted => Some(10),
            Self::HeroRespawn | Self::HealthDanger => None,
        }
    }

    /// Voiced by default in the catalog (`hero_respawn` is not).
    pub fn voiced(self) -> bool {
        !matches!(self, Self::HeroRespawn)
    }

    /// Catalog category is `critical`.
    pub fn is_critical(self) -> bool {
        matches!(self, Self::HealthDanger)
    }

    /// Catalog `tier_range` as `(min, max)` for tiered events.
    pub fn tier_range(self) -> Option<(u8, u8)> {
        match self {
            Self::MultiKill => Some((2, 5)),
            Self::Streak => Some((3, 10)),
            _ => None,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::MultiKill => 0,
            Self::Streak => 1,
            Self::FirstBlood => 2,
            Self::HeroKill => 3,
            Self::HeroDeath => 4,
            Self::LevelMilestone => 5,
            Self::HealthLow => 6,
            Self::ManaLow => 7,
            Self::MatchEnded => 8,
            Self::MatchStarted => 9,
            Self::HeroRespawn => 10,
            Self::HealthDanger => 11,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Draft,
    Pregame,
    Live,
    PostGame,
}

/// One admitted game-state payload, already reduced to the fields the detector uses.
/// `None` means the field was missing or invalid in the payload: unknown, never zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tick {
    /// Monotonic arrival time of the admitted payload in milliseconds.
    pub at_ms: u64,
    /// Session epoch from the intake; a change starts a new baseline.
    pub session_epoch: u64,
    pub phase: Phase,
    /// `true` only when the source is fresh and ownership is verified. An untrusted
    /// tick is ignored entirely: no event, no baseline change.
    pub trusted: bool,
    /// Own `player.kills` counter.
    pub kills: Option<u32>,
    /// Own `player.deaths` counter. `None` enables the alive-transition fallback.
    pub deaths: Option<u32>,
    /// Own `player.kill_streak` counter, when admitted and representable.
    pub kill_streak: Option<u32>,
    /// `map.radiant_score + map.dire_score`, for the first-blood rule (E-1).
    pub combined_score: Option<u32>,
    pub level: Option<u32>,
    pub hp: Option<u32>,
    pub max_hp: Option<u32>,
    pub mana_percent: Option<u32>,
    pub alive: Option<bool>,
}

/// Tunables with the catalog defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub multi_kill_window_ms: u64,
    pub multi_kill_min_kills: u32,
    pub streak_min_kills: u32,
    pub levels: Vec<u32>,
    pub health_low_threshold_percent: u32,
    pub health_low_rearm_percent: u32,
    pub mana_low_threshold_percent: u32,
    pub mana_low_rearm_percent: u32,
    pub danger_max_hp_percent: u32,
    pub danger_drop_points: u32,
    pub danger_window_ms: u64,
    pub danger_rearm_percent: u32,
    pub danger_cooldown_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            multi_kill_window_ms: 18_000,
            multi_kill_min_kills: 2,
            streak_min_kills: 3,
            levels: vec![6, 12, 18, 25],
            health_low_threshold_percent: 20,
            health_low_rearm_percent: 30,
            mana_low_threshold_percent: 15,
            mana_low_rearm_percent: 25,
            danger_max_hp_percent: 25,
            danger_drop_points: 15,
            danger_window_ms: 2_000,
            danger_rearm_percent: 40,
            danger_cooldown_ms: 20_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub id: EventId,
    /// `payload.count` for `MultiKill` and `Streak`, capped at the top of the tier
    /// range; `None` for every other event.
    pub tier: Option<u8>,
    pub at_ms: u64,
}

pub struct Detector {
    _private: (),
    config: Config,
    session_epoch: Option<u64>,
    last_at_ms: Option<u64>,
    phase: Option<Phase>,
    kills: Option<u32>,
    deaths: Option<u32>,
    kill_streak: Option<u32>,
    combined_score: Option<u32>,
    level: Option<u32>,
    alive: Option<bool>,
    death_since_respawn: bool,
    multi_kill_count: u32,
    multi_kill_at_ms: Option<u64>,
    streak_count: Option<u32>,
    first_blood_emitted: bool,
    health_low_observed: bool,
    health_low_armed: bool,
    mana_low_observed: bool,
    mana_low_armed: bool,
    danger_armed: bool,
    danger_last_cue_ms: Option<u64>,
    danger_samples: VecDeque<HealthSample>,
}

#[derive(Debug, Clone, Copy)]
struct HealthSample {
    at_ms: u64,
    hp: u32,
    max_hp: u32,
}

impl Detector {
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    pub fn with_config(config: Config) -> Self {
        Self {
            _private: (),
            config,
            session_epoch: None,
            last_at_ms: None,
            phase: None,
            kills: None,
            deaths: None,
            kill_streak: None,
            combined_score: None,
            level: None,
            alive: None,
            death_since_respawn: false,
            multi_kill_count: 0,
            multi_kill_at_ms: None,
            streak_count: None,
            first_blood_emitted: false,
            health_low_observed: false,
            health_low_armed: true,
            mana_low_observed: false,
            mana_low_armed: true,
            danger_armed: true,
            danger_last_cue_ms: None,
            danger_samples: VecDeque::new(),
        }
    }

    /// Feed one admitted tick. Returns the events it produced, in [`EventId`]
    /// declaration order, at most one event per id.
    pub fn observe(&mut self, tick: &Tick) -> Vec<Event> {
        if !tick.trusted {
            return Vec::new();
        }

        if self.session_epoch != Some(tick.session_epoch) {
            self.set_baseline(tick, false);
            return Vec::new();
        }
        if self
            .last_at_ms
            .is_some_and(|last_at_ms| tick.at_ms < last_at_ms)
        {
            self.set_baseline(tick, true);
            return Vec::new();
        }

        let previous_phase = self.phase;
        let previous_alive = self.alive;
        let previous_kills = self.kills;
        let previous_deaths = self.deaths;
        let previous_kill_streak = self.kill_streak;
        let previous_score = self.combined_score;
        let previous_level = self.level;

        let alive_death = previous_alive == Some(true) && tick.alive == Some(false);
        let death_counter_known = previous_deaths.is_some() && tick.deaths.is_some();
        let death_counter_increased = matches!((previous_deaths, tick.deaths),
            (Some(previous), Some(current)) if current > previous);
        let died = if death_counter_known {
            death_counter_increased
        } else {
            alive_death
        };
        let alive_respawn = previous_alive == Some(false) && tick.alive == Some(true);
        let respawned = alive_respawn && (self.death_since_respawn || died);

        let (kill_delta, kills_regressed) = match (previous_kills, tick.kills) {
            (Some(previous), Some(current)) if current < previous => (None, true),
            (Some(previous), Some(current)) => (current.checked_sub(previous), false),
            _ => (None, false),
        };
        let score_regressed = matches!((previous_score, tick.combined_score),
            (Some(previous), Some(current)) if current < previous);
        let level_regressed = matches!((previous_level, tick.level),
            (Some(previous), Some(current)) if current < previous);
        let kill_streak_regressed = matches!((previous_kill_streak, tick.kill_streak),
            (Some(previous), Some(current)) if current < previous);
        let kill_streak_reset = kill_streak_regressed && !died;

        let match_started =
            previous_phase.is_some_and(|phase| phase != Phase::Live) && tick.phase == Phase::Live;
        let match_ended = previous_phase.is_some_and(|phase| phase != Phase::PostGame)
            && tick.phase == Phase::PostGame;

        let mut multi_kill_tier = None;
        let mut streak_tier = None;
        let mut first_blood = false;
        let mut hero_kill = false;
        if kills_regressed {
            self.multi_kill_count = 0;
            self.multi_kill_at_ms = None;
            self.streak_count = (tick.kills == Some(0)).then_some(0);
        } else if tick.phase == Phase::Live {
            if let Some(delta) = kill_delta.filter(|delta| *delta > 0) {
                hero_kill = true;

                let continues_chain = self.multi_kill_at_ms.is_some_and(|last_kill_ms| {
                    tick.at_ms
                        .checked_sub(last_kill_ms)
                        .is_some_and(|elapsed| elapsed <= self.config.multi_kill_window_ms)
                });
                let chain_base = if continues_chain {
                    self.multi_kill_count
                } else {
                    0
                };
                self.multi_kill_count = chain_base.saturating_add(delta);
                self.multi_kill_at_ms = Some(tick.at_ms);
                if let Some((min_tier, max_tier)) = tier_bounds(EventId::MultiKill) {
                    let minimum = self.config.multi_kill_min_kills.max(min_tier);
                    if self.multi_kill_count >= minimum {
                        multi_kill_tier = Some(self.multi_kill_count.min(max_tier) as u8);
                    }
                }

                if let Some(streak_count) = self.streak_count {
                    let streak_count = streak_count.saturating_add(delta);
                    self.streak_count = Some(streak_count);
                    if tick.kill_streak.is_none() && !kill_streak_reset {
                        if let Some((min_tier, max_tier)) = tier_bounds(EventId::Streak) {
                            let minimum = self.config.streak_min_kills.max(min_tier);
                            if streak_count >= minimum {
                                streak_tier = Some(streak_count.min(max_tier) as u8);
                            }
                        }
                    }
                }
                if let Some(observed_streak) = tick.kill_streak.filter(|_| !kill_streak_reset) {
                    if let Some((min_tier, max_tier)) = tier_bounds(EventId::Streak) {
                        let minimum = self.config.streak_min_kills.max(min_tier);
                        if observed_streak >= minimum {
                            streak_tier = Some(observed_streak.min(max_tier) as u8);
                        }
                    }
                }

                let exact_first_score =
                    matches!((previous_score, tick.combined_score), (Some(0), Some(1)));
                let exact_first_kill = matches!((previous_kills, tick.kills), (Some(0), Some(1)));
                if !self.first_blood_emitted
                    && !score_regressed
                    && exact_first_score
                    && exact_first_kill
                {
                    first_blood = true;
                    self.first_blood_emitted = true;
                }
            }
        }

        // A kill and a death can arrive in one admitted payload. Count the kill
        // against the pre-death streak, then reset the streak for later kills.
        if died {
            self.streak_count = Some(0);
            self.death_since_respawn = true;
            self.danger_armed = true;
            self.danger_samples.clear();
        }
        if kill_streak_reset {
            self.streak_count = None;
        }

        let mut level_milestone = false;
        if !level_regressed {
            if let (Some(previous), Some(current)) = (previous_level, tick.level) {
                level_milestone = current > previous
                    && self
                        .config
                        .levels
                        .iter()
                        .any(|milestone| previous < *milestone && current >= *milestone);
            }
        }

        let mut health_low = false;
        if respawned {
            self.death_since_respawn = false;
            self.health_low_armed = true;
            self.mana_low_armed = true;
        }
        if tick.alive == Some(true) {
            if let Some((hp, max_hp)) = tick
                .hp
                .zip(tick.max_hp)
                .filter(|(hp, max_hp)| *max_hp > 0 && hp <= max_hp)
            {
                if !self.health_low_observed {
                    self.health_low_observed = true;
                    self.health_low_armed =
                        !percent_at_or_below(hp, max_hp, self.config.health_low_threshold_percent);
                } else {
                    if percent_at_or_above(hp, max_hp, self.config.health_low_rearm_percent) {
                        self.health_low_armed = true;
                    }
                    if self.health_low_armed
                        && percent_at_or_below(hp, max_hp, self.config.health_low_threshold_percent)
                    {
                        health_low = true;
                        self.health_low_armed = false;
                    }
                }
            }
        }

        let mut mana_low = false;
        if tick.alive == Some(true) {
            if let Some(mana_percent) = tick.mana_percent {
                if !self.mana_low_observed {
                    self.mana_low_observed = true;
                    self.mana_low_armed = mana_percent > self.config.mana_low_threshold_percent;
                } else {
                    if mana_percent >= self.config.mana_low_rearm_percent {
                        self.mana_low_armed = true;
                    }
                    if self.mana_low_armed && mana_percent <= self.config.mana_low_threshold_percent
                    {
                        mana_low = true;
                        self.mana_low_armed = false;
                    }
                }
            }
        }

        let health_danger = self.observe_health_danger(tick);

        if let Some(value) = tick.kills {
            self.kills = Some(value);
        }
        self.deaths = tick.deaths;
        self.kill_streak = tick.kill_streak;
        if let Some(value) = tick.combined_score {
            self.combined_score = Some(value);
        }
        if let Some(value) = tick.level {
            self.level = Some(value);
        }
        if let Some(value) = tick.alive {
            self.alive = Some(value);
        }
        self.phase = Some(tick.phase);
        self.session_epoch = Some(tick.session_epoch);
        self.last_at_ms = Some(tick.at_ms);

        let mut events = Vec::with_capacity(12);
        if let Some(tier) = multi_kill_tier {
            events.push(event(EventId::MultiKill, Some(tier), tick.at_ms));
        }
        if let Some(tier) = streak_tier {
            events.push(event(EventId::Streak, Some(tier), tick.at_ms));
        }
        if first_blood {
            events.push(event(EventId::FirstBlood, None, tick.at_ms));
        }
        if hero_kill {
            events.push(event(EventId::HeroKill, None, tick.at_ms));
        }
        if died {
            events.push(event(EventId::HeroDeath, None, tick.at_ms));
        }
        if level_milestone {
            events.push(event(EventId::LevelMilestone, None, tick.at_ms));
        }
        if health_low {
            events.push(event(EventId::HealthLow, None, tick.at_ms));
        }
        if mana_low {
            events.push(event(EventId::ManaLow, None, tick.at_ms));
        }
        if match_ended {
            events.push(event(EventId::MatchEnded, None, tick.at_ms));
        }
        if match_started {
            events.push(event(EventId::MatchStarted, None, tick.at_ms));
        }
        if respawned {
            events.push(event(EventId::HeroRespawn, None, tick.at_ms));
        }
        if health_danger {
            events.push(event(EventId::HealthDanger, None, tick.at_ms));
        }
        events
    }

    fn set_baseline(&mut self, tick: &Tick, preserve_danger_state: bool) {
        let danger_armed = self.danger_armed;
        let danger_last_cue_ms = self.danger_last_cue_ms;
        self.session_epoch = Some(tick.session_epoch);
        self.last_at_ms = Some(tick.at_ms);
        self.phase = Some(tick.phase);
        self.kills = tick.kills;
        self.deaths = tick.deaths;
        self.kill_streak = tick.kill_streak;
        self.combined_score = tick.combined_score;
        self.level = tick.level;
        self.alive = tick.alive;
        self.death_since_respawn = false;
        self.multi_kill_count = 0;
        self.multi_kill_at_ms = None;
        self.streak_count = (tick.kills == Some(0)).then_some(0);
        self.first_blood_emitted = false;
        self.health_low_observed = false;
        self.health_low_armed = true;
        self.mana_low_observed = false;
        self.mana_low_armed = true;
        self.danger_armed = if preserve_danger_state {
            danger_armed
        } else {
            true
        };
        self.danger_last_cue_ms = if preserve_danger_state {
            danger_last_cue_ms
        } else {
            None
        };
        self.danger_samples.clear();

        if let Some((hp, max_hp)) = tick
            .hp
            .zip(tick.max_hp)
            .filter(|(hp, max_hp)| *max_hp > 0 && hp <= max_hp)
        {
            self.health_low_observed = true;
            self.health_low_armed =
                !percent_at_or_below(hp, max_hp, self.config.health_low_threshold_percent);
            if tick.alive == Some(true) {
                self.danger_samples.push_back(HealthSample {
                    at_ms: tick.at_ms,
                    hp,
                    max_hp,
                });
            }
        }
        if let Some(mana_percent) = tick.mana_percent {
            self.mana_low_observed = true;
            self.mana_low_armed = mana_percent > self.config.mana_low_threshold_percent;
        }
    }

    fn observe_health_danger(&mut self, tick: &Tick) -> bool {
        if tick.alive == Some(false) {
            return false;
        }
        let (Some(hp), Some(max_hp)) = (tick.hp, tick.max_hp) else {
            return false;
        };
        if tick.alive != Some(true) || max_hp == 0 || hp > max_hp {
            return false;
        }

        while self.danger_samples.front().is_some_and(|sample| {
            tick.at_ms
                .checked_sub(sample.at_ms)
                .is_some_and(|elapsed| elapsed > self.config.danger_window_ms)
        }) {
            self.danger_samples.pop_front();
        }

        if percent_at_or_above(hp, max_hp, self.config.danger_rearm_percent) {
            self.danger_armed = true;
        }

        let dropped_enough = self.danger_samples.iter().any(|sample| {
            tick.at_ms
                .checked_sub(sample.at_ms)
                .is_some_and(|elapsed| elapsed <= self.config.danger_window_ms)
                && percent_drop_at_least(
                    sample.hp,
                    sample.max_hp,
                    hp,
                    max_hp,
                    self.config.danger_drop_points,
                )
        });
        self.danger_samples.push_back(HealthSample {
            at_ms: tick.at_ms,
            hp,
            max_hp,
        });

        let is_dangerous = percent_at_or_below(hp, max_hp, self.config.danger_max_hp_percent);
        let cooldown_elapsed = self.danger_last_cue_ms.is_none_or(|last_cue_ms| {
            tick.at_ms >= last_cue_ms
                && tick.at_ms.saturating_sub(last_cue_ms) >= self.config.danger_cooldown_ms
        });
        if self.danger_armed && is_dangerous && dropped_enough && cooldown_elapsed {
            self.danger_armed = false;
            self.danger_last_cue_ms = Some(tick.at_ms);
            return true;
        }
        false
    }
}

impl Default for Detector {
    fn default() -> Self {
        Self::new()
    }
}

fn event(id: EventId, tier: Option<u8>, at_ms: u64) -> Event {
    Event { id, tier, at_ms }
}

fn tier_bounds(id: EventId) -> Option<(u32, u32)> {
    id.tier_range()
        .map(|(min, max)| (u32::from(min), u32::from(max)))
}

fn percent_at_or_below(value: u32, maximum: u32, percent: u32) -> bool {
    u128::from(value).saturating_mul(100) <= u128::from(maximum).saturating_mul(u128::from(percent))
}

fn percent_at_or_above(value: u32, maximum: u32, percent: u32) -> bool {
    u128::from(value).saturating_mul(100) >= u128::from(maximum).saturating_mul(u128::from(percent))
}

fn percent_drop_at_least(
    previous_hp: u32,
    previous_max_hp: u32,
    current_hp: u32,
    current_max_hp: u32,
    drop_points: u32,
) -> bool {
    let previous_scaled = u128::from(previous_hp)
        .saturating_mul(100)
        .saturating_mul(u128::from(current_max_hp));
    let current_scaled = u128::from(current_hp)
        .saturating_mul(100)
        .saturating_mul(u128::from(previous_max_hp));
    let required_drop = u128::from(drop_points)
        .saturating_mul(u128::from(previous_max_hp))
        .saturating_mul(u128::from(current_max_hp));
    previous_scaled >= current_scaled.saturating_add(required_drop)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// Outcome `lower_rank_same_tick` (EVENT-AUDIO-SPEC section 5).
    LowerRankSameTick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dropped {
    pub event: Event,
    pub reason: DropReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arbitration {
    pub winner: Option<Event>,
    pub dropped: Vec<Dropped>,
}

/// Ordinary arbitration for one admitted tick (EVENT-AUDIO-SPEC section 2). Only
/// voiced, non-critical events take part; others are in neither `winner` nor `dropped`.
pub fn arbitrate(events: &[Event]) -> Arbitration {
    let mut seen = [false; 12];
    let mut eligible = Vec::new();
    for candidate in events {
        if !candidate.id.voiced() || candidate.id.is_critical() {
            continue;
        }
        let index = candidate.id.index();
        let first_for_id = !seen[index];
        seen[index] = true;
        eligible.push((*candidate, first_for_id));
    }

    let mut winner_index = None;
    let mut winner_rank = 0;
    for (index, (candidate, first_for_id)) in eligible.iter().enumerate() {
        if !first_for_id {
            continue;
        }
        let rank = candidate.id.rank().unwrap_or(0);
        if winner_index.is_none() || rank > winner_rank {
            winner_index = Some(index);
            winner_rank = rank;
        }
    }

    let winner = winner_index.and_then(|index| eligible.get(index).map(|(event, _)| *event));
    let dropped = eligible
        .into_iter()
        .enumerate()
        .filter_map(|(index, (event, _))| {
            (Some(index) != winner_index).then_some(Dropped {
                event,
                reason: DropReason::LowerRankSameTick,
            })
        })
        .collect();
    Arbitration { winner, dropped }
}
