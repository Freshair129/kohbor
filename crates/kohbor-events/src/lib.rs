//! Kohbor event detector and arbiter (docs/design/EVENT-AUDIO-SPEC.md sections 2 and 3).
//!
//! Pure logic. The caller supplies one [`Tick`] per admitted payload with the
//! monotonic arrival time already attached; this crate never reads a clock, opens a
//! file or touches the network. The behaviour behind each function is the S1-3 task;
//! until then every body is `todo!()` so the S1-1 and S1-2 tests fail red.

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
        todo!("S1-3")
    }

    /// Ordinary rank from the catalog; `None` for unranked events.
    pub fn rank(self) -> Option<u32> {
        todo!("S1-3")
    }

    /// Voiced by default in the catalog (`hero_respawn` is not).
    pub fn voiced(self) -> bool {
        todo!("S1-3")
    }

    /// Catalog category is `critical`.
    pub fn is_critical(self) -> bool {
        todo!("S1-3")
    }

    /// Catalog `tier_range` as `(min, max)` for tiered events.
    pub fn tier_range(self) -> Option<(u8, u8)> {
        todo!("S1-3")
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
        todo!("S1-3")
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
}

impl Detector {
    pub fn new() -> Self {
        todo!("S1-3")
    }

    pub fn with_config(_config: Config) -> Self {
        todo!("S1-3")
    }

    /// Feed one admitted tick. Returns the events it produced, in [`EventId`]
    /// declaration order, at most one event per id.
    pub fn observe(&mut self, _tick: &Tick) -> Vec<Event> {
        todo!("S1-3")
    }
}

impl Default for Detector {
    fn default() -> Self {
        Self::new()
    }
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
pub fn arbitrate(_events: &[Event]) -> Arbitration {
    todo!("S1-3")
}
