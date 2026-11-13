//! MVP-1 W5 deterministic risk rules over admitted GSI state and anonymous vision counts.

use std::collections::VecDeque;

use kohbor_capture::Coverage;

pub mod rules {
    /// PROPOSED — source: contracts/event-catalog.json, enemy_missing.min_absence_ms.
    pub const MIN_ABSENCE_MS: u64 = 5_000;
    /// PROPOSED — source: contracts/event-catalog.json, enemy_missing.global_cooldown_ms.
    pub const MISSING_COOLDOWN_MS: u64 = 10_000;
    /// PROPOSED — source: contracts/event-catalog.json, risk_alert.global_cooldown_ms.
    pub const ALERT_COOLDOWN_MS: u64 = 30_000;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const MEDIAN_N: usize = 5;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const BASELINE_WINDOW_MS: u64 = 20_000;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const COORDINATED_WINDOW_MS: u64 = 3_000;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const RISK_ALERT_HOLD_MS: u64 = 5_000;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const REVISION_WINDOW_MS: u64 = 10_000;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const REVISION_HOLD_MS: u64 = 1_000;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const UNAVAILABLE_CLEAR_MS: u64 = 2_000;
    /// PROPOSED — source: MVP-PLAN.md section (d), Freshness and unknown.
    pub const GSI_STALE_MS: u64 = 2_000;
    /// PROPOSED — source: MVP-PLAN.md section (d), Freshness and unknown; VISION-002.
    pub const OBSERVATION_GAP_MS: u64 = 2_000;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const HP_LEVEL_ONE_THRESHOLD: u8 = 50;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const HP_LEVEL_TWO_THRESHOLD: u8 = 30;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const METER_LEVEL_ZERO: u8 = 0;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const METER_LEVEL_ONE: u8 = 1;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const METER_LEVEL_TWO: u8 = 2;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const METER_LEVEL_THREE: u8 = 3;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const RECENT_EVENT_WINDOW_MS: u64 = 5_000;
    /// PROPOSED — source: MVP-PLAN.md section (d), Rules.
    pub const COORDINATED_DROP_COUNT: u8 = 2;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnGsi {
    pub alive: bool,
    pub hp_pct: Option<u8>,
    pub in_match: bool,
    pub age_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickInput {
    pub now_ms: u64,
    pub coverage: Coverage,
    pub visible: Option<u8>,
    pub own: Option<OwnGsi>,
    pub gsi_events: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meter {
    Unknown,
    Level(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskEvent {
    EnemyMissing,
    RiskAlert,
    RiskRevision,
    /// Emitted once when coverage becomes Unavailable while current GSI confirms in-match state.
    RiskUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskOutput {
    pub meter: Meter,
    pub gsi_source: bool,
    pub vision_source: bool,
    pub visible: Option<u8>,
    pub baseline: Option<u8>,
    pub events: Vec<RiskEvent>,
}

pub struct RiskEngine {
    raw_counts: VecDeque<u8>,
    v_history: VecDeque<Observation>,
    last_visible_ms: Option<u64>,
    drop_episode: Option<DropEpisode>,
    alert: Option<AlertEpisode>,
    last_missing_ms: Option<u64>,
    last_alert_ms: Option<u64>,
    health_low_ms: Option<u64>,
    health_danger_ms: Option<u64>,
    unavailable_latched: bool,
    unavailable_clear_since_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct Observation {
    at_ms: u64,
    v: u8,
}

#[derive(Debug, Clone, Copy)]
struct DropEpisode {
    since_ms: u64,
    missing_emitted: bool,
    coordinated_since_ms: Option<u64>,
    alert_emitted: bool,
}

#[derive(Debug, Clone, Copy)]
struct AlertEpisode {
    at_ms: u64,
    recovered_since_ms: Option<u64>,
}

impl RiskEngine {
    pub fn new() -> Self {
        Self {
            raw_counts: VecDeque::new(),
            v_history: VecDeque::new(),
            last_visible_ms: None,
            drop_episode: None,
            alert: None,
            last_missing_ms: None,
            last_alert_ms: None,
            health_low_ms: None,
            health_danger_ms: None,
            unavailable_latched: false,
            unavailable_clear_since_ms: None,
        }
    }

    pub fn tick(&mut self, input: &TickInput) -> RiskOutput {
        let own = input
            .own
            .filter(|own| own.in_match && own.age_ms <= rules::GSI_STALE_MS);
        let gsi_source = own.is_some();
        let vision_count = (input.coverage == Coverage::Valid)
            .then_some(input.visible)
            .flatten();
        let vision_source = vision_count.is_some();
        let mut events = Vec::new();

        self.update_gsi_events(input, own.is_some());

        let (visible, v, baseline) = if let Some(count) = vision_count {
            let observation_gap = self.last_visible_ms.is_some_and(|previous| {
                input
                    .now_ms
                    .checked_sub(previous)
                    .is_none_or(|elapsed| elapsed > rules::OBSERVATION_GAP_MS)
            });
            if observation_gap {
                self.clear_vision_state();
            }
            self.last_visible_ms = Some(input.now_ms);
            let (v, baseline) = self.record_vision(input.now_ms, count);
            if let (Some(v), Some(baseline)) = (v, baseline) {
                self.update_drop_and_alert(input.now_ms, v, baseline, own, &mut events);
            }
            (Some(count), v, baseline)
        } else {
            self.clear_vision_state();
            (None, None, None)
        };

        self.update_unavailable(input.now_ms, input.coverage, own, &mut events);

        let meter = match own {
            None => Meter::Unknown,
            Some(own) => self
                .meter_level(own, input.now_ms, v, baseline)
                .map_or(Meter::Unknown, Meter::Level),
        };

        RiskOutput {
            meter,
            gsi_source,
            vision_source,
            visible,
            baseline,
            events,
        }
    }

    fn update_gsi_events(&mut self, input: &TickInput, gsi_is_current: bool) {
        if gsi_is_current {
            for event in &input.gsi_events {
                match *event {
                    "health_low" => self.health_low_ms = Some(input.now_ms),
                    "health_danger" => self.health_danger_ms = Some(input.now_ms),
                    _ => {}
                }
            }
        }

        if !is_recent(
            input.now_ms,
            self.health_low_ms,
            rules::RECENT_EVENT_WINDOW_MS,
        ) {
            self.health_low_ms = None;
        }
        if !is_recent(
            input.now_ms,
            self.health_danger_ms,
            rules::RECENT_EVENT_WINDOW_MS,
        ) {
            self.health_danger_ms = None;
        }
    }

    fn record_vision(&mut self, now_ms: u64, visible: u8) -> (Option<u8>, Option<u8>) {
        self.raw_counts.push_back(visible);
        if self.raw_counts.len() > rules::MEDIAN_N {
            self.raw_counts.pop_front();
        }

        let Some(v) = self.current_v() else {
            return (None, None);
        };

        self.v_history.retain(|sample| {
            now_ms
                .checked_sub(sample.at_ms)
                .is_some_and(|age| age <= rules::BASELINE_WINDOW_MS)
        });
        self.v_history.push_back(Observation { at_ms: now_ms, v });
        let baseline = self.v_history.iter().map(|sample| sample.v).max();
        (Some(v), baseline)
    }

    fn current_v(&self) -> Option<u8> {
        if self.raw_counts.len() != rules::MEDIAN_N {
            return None;
        }
        let mut counts = self.raw_counts.iter().copied().collect::<Vec<_>>();
        counts.sort_unstable();
        counts.get(counts.len() / 2).copied()
    }

    fn update_drop_and_alert(
        &mut self,
        now_ms: u64,
        v: u8,
        baseline: u8,
        own: Option<OwnGsi>,
        events: &mut Vec<RiskEvent>,
    ) {
        let deficit = baseline.saturating_sub(v);
        let is_drop = baseline >= 1 && v < baseline;
        if !is_drop {
            self.drop_episode = None;
        } else if self.drop_episode.is_none() {
            self.drop_episode = Some(DropEpisode {
                since_ms: now_ms,
                missing_emitted: false,
                coordinated_since_ms: None,
                alert_emitted: false,
            });
        }

        let Some(episode) = self.drop_episode.as_mut() else {
            self.update_alert_recovery(now_ms, v, baseline, events);
            return;
        };

        if let Some(held) = now_ms.checked_sub(episode.since_ms) {
            if deficit >= rules::COORDINATED_DROP_COUNT && held <= rules::COORDINATED_WINDOW_MS {
                episode.coordinated_since_ms.get_or_insert(now_ms);
            } else if deficit < rules::COORDINATED_DROP_COUNT {
                episode.coordinated_since_ms = None;
            }
        }

        if !episode.missing_emitted
            && now_ms
                .checked_sub(episode.since_ms)
                .is_some_and(|elapsed| elapsed > rules::MIN_ABSENCE_MS)
            && own.is_some_and(|own| own.in_match)
            && cooldown_elapsed(now_ms, self.last_missing_ms, rules::MISSING_COOLDOWN_MS)
        {
            events.push(RiskEvent::EnemyMissing);
            self.last_missing_ms = Some(now_ms);
            episode.missing_emitted = true;
        }

        if !episode.alert_emitted
            && episode
                .coordinated_since_ms
                .and_then(|since| now_ms.checked_sub(since))
                .is_some_and(|elapsed| elapsed > rules::RISK_ALERT_HOLD_MS)
            && own.is_some_and(|own| own.alive && own.in_match)
            && cooldown_elapsed(now_ms, self.last_alert_ms, rules::ALERT_COOLDOWN_MS)
        {
            events.push(RiskEvent::RiskAlert);
            self.last_alert_ms = Some(now_ms);
            episode.alert_emitted = true;
            self.alert = Some(AlertEpisode {
                at_ms: now_ms,
                recovered_since_ms: None,
            });
        }

        self.update_alert_recovery(now_ms, v, baseline, events);
    }

    fn update_alert_recovery(
        &mut self,
        now_ms: u64,
        v: u8,
        baseline: u8,
        events: &mut Vec<RiskEvent>,
    ) {
        let Some(alert) = self.alert.as_mut() else {
            return;
        };

        if v < baseline {
            alert.recovered_since_ms = None;
            return;
        }

        let recovered_since = *alert.recovered_since_ms.get_or_insert(now_ms);
        let recovery_held = now_ms
            .checked_sub(recovered_since)
            .is_some_and(|elapsed| elapsed >= rules::REVISION_HOLD_MS);
        if !recovery_held {
            return;
        }

        if now_ms
            .checked_sub(alert.at_ms)
            .is_some_and(|elapsed| elapsed <= rules::REVISION_WINDOW_MS)
        {
            events.push(RiskEvent::RiskRevision);
        }
        self.alert = None;
    }

    fn meter_level(
        &self,
        own: OwnGsi,
        now_ms: u64,
        v: Option<u8>,
        baseline: Option<u8>,
    ) -> Option<u8> {
        let mut level = own.hp_pct.map(|hp| {
            if hp <= rules::HP_LEVEL_TWO_THRESHOLD {
                rules::METER_LEVEL_TWO
            } else if hp <= rules::HP_LEVEL_ONE_THRESHOLD {
                rules::METER_LEVEL_ONE
            } else {
                rules::METER_LEVEL_ZERO
            }
        });

        if is_recent(now_ms, self.health_low_ms, rules::RECENT_EVENT_WINDOW_MS) {
            level = Some(level.map_or(rules::METER_LEVEL_TWO, |level| {
                level.max(rules::METER_LEVEL_TWO)
            }));
        }
        if is_recent(now_ms, self.health_danger_ms, rules::RECENT_EVENT_WINDOW_MS) {
            level = Some(level.map_or(rules::METER_LEVEL_THREE, |level| {
                level.max(rules::METER_LEVEL_THREE)
            }));
        }

        if let (Some(v), Some(baseline)) = (v, baseline) {
            if self.drop_episode.is_some() {
                level = Some(level.map_or(rules::METER_LEVEL_ONE, |level| {
                    level.max(rules::METER_LEVEL_ONE)
                }));
            }
            if baseline.saturating_sub(v) >= rules::COORDINATED_DROP_COUNT {
                level = Some(level.map_or(rules::METER_LEVEL_TWO, |level| {
                    level.max(rules::METER_LEVEL_TWO)
                }));
            }
        }
        if self.alert.is_some() {
            level = Some(level.map_or(rules::METER_LEVEL_THREE, |level| {
                level.max(rules::METER_LEVEL_THREE)
            }));
        }

        level
    }

    fn clear_vision_state(&mut self) {
        self.raw_counts.clear();
        self.v_history.clear();
        self.last_visible_ms = None;
        self.drop_episode = None;
        self.alert = None;
    }

    fn update_unavailable(
        &mut self,
        now_ms: u64,
        coverage: Coverage,
        own: Option<OwnGsi>,
        events: &mut Vec<RiskEvent>,
    ) {
        match coverage {
            Coverage::Unavailable => {
                self.unavailable_clear_since_ms = None;
                if !self.unavailable_latched {
                    self.unavailable_latched = true;
                    if own.is_some_and(|own| own.in_match) {
                        events.push(RiskEvent::RiskUnavailable);
                    }
                }
            }
            Coverage::Stale => {
                self.unavailable_clear_since_ms = None;
            }
            Coverage::Valid => {
                if self.unavailable_latched {
                    let since = *self.unavailable_clear_since_ms.get_or_insert(now_ms);
                    if now_ms
                        .checked_sub(since)
                        .is_some_and(|elapsed| elapsed >= rules::UNAVAILABLE_CLEAR_MS)
                    {
                        self.unavailable_latched = false;
                        self.unavailable_clear_since_ms = None;
                    }
                }
            }
        }
    }
}

fn cooldown_elapsed(now_ms: u64, last_event_ms: Option<u64>, cooldown_ms: u64) -> bool {
    last_event_ms.is_none_or(|last| {
        now_ms
            .checked_sub(last)
            .is_some_and(|elapsed| elapsed >= cooldown_ms)
    })
}

fn is_recent(now_ms: u64, event_ms: Option<u64>, window_ms: u64) -> bool {
    event_ms.is_some_and(|event| {
        now_ms
            .checked_sub(event)
            .is_some_and(|elapsed| elapsed <= window_ms)
    })
}

impl Default for RiskEngine {
    fn default() -> Self {
        Self::new()
    }
}
