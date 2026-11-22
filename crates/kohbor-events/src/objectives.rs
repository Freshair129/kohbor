//! Pure map-objective timer scheduler (ALERT-008).
//!
//! The caller supplies admitted GSI map-clock observations and reports stale
//! gaps explicitly. This module never reads a clock, the filesystem, or a game.

use std::collections::{BTreeMap, HashSet};

const MAX_FORWARD_STEP_SECONDS: i64 = 5;
const TIMING_TABLE: &str = include_str!("../data/objective-timings.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ObjectiveId {
    WaterRune,
    PowerRune,
    BountyRune,
    WisdomRune,
    Tormentor,
}

impl ObjectiveId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WaterRune => "water_rune",
            Self::PowerRune => "power_rune",
            Self::BountyRune => "bounty_rune",
            Self::WisdomRune => "wisdom_rune",
            Self::Tormentor => "tormentor",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "water_rune" => Some(Self::WaterRune),
            "power_rune" => Some(Self::PowerRune),
            "bounty_rune" => Some(Self::BountyRune),
            "wisdom_rune" => Some(Self::WisdomRune),
            "tormentor" => Some(Self::Tormentor),
            _ => None,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::WaterRune => 0,
            Self::PowerRune => 1,
            Self::BountyRune => 2,
            Self::WisdomRune => 3,
            Self::Tormentor => 4,
        }
    }

    fn enabled(self, settings: ObjectiveSettings) -> bool {
        match self {
            Self::WaterRune => settings.water_rune,
            Self::PowerRune => settings.power_rune,
            Self::BountyRune => settings.bounty_rune,
            Self::WisdomRune => settings.wisdom_rune,
            Self::Tormentor => settings.tormentor,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ObjectivePhase {
    PreWarn { lead_s: u8 },
    Spawn,
}

impl ObjectivePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreWarn { .. } => "pre_warn",
            Self::Spawn => "spawn",
        }
    }

    pub const fn lead_s(self) -> Option<u8> {
        match self {
            Self::PreWarn { lead_s } => Some(lead_s),
            Self::Spawn => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectiveSettings {
    pub water_rune: bool,
    pub power_rune: bool,
    pub bounty_rune: bool,
    pub wisdom_rune: bool,
    pub tormentor: bool,
    /// Two optional lead times per objective, indexed by `ObjectiveId::index`.
    pub lead_times: [[Option<u8>; 2]; 5],
}

impl Default for ObjectiveSettings {
    fn default() -> Self {
        Self {
            water_rune: true,
            power_rune: true,
            bounty_rune: true,
            wisdom_rune: true,
            tormentor: true,
            lead_times: [
                [Some(30), Some(15)],
                [Some(30), Some(15)],
                [None, None],
                [None, None],
                [None, None],
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectiveAlert {
    pub objective: ObjectiveId,
    pub phase: ObjectivePhase,
    pub occurrence_seconds: i64,
    pub due_seconds: i64,
}

/// All alerts found by one admitted frame. A single batch means one overlay
/// banner and one tone even when several objectives share a due second.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectiveAlertBatch {
    pub observed_clock_seconds: i64,
    pub alerts: Vec<ObjectiveAlert>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TimingRule {
    objective: ObjectiveId,
    spawn_times_seconds: Vec<i64>,
    repeat_every_seconds: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FiredKey {
    objective: ObjectiveId,
    occurrence_seconds: i64,
    phase: ObjectivePhase,
}

/// Pure scheduler. Invalid embedded timing data fails closed to no alerts.
pub struct ObjectiveScheduler {
    rules: Vec<TimingRule>,
    previous_clock_seconds: Option<i64>,
    fired: HashSet<FiredKey>,
}

impl Default for ObjectiveScheduler {
    fn default() -> Self {
        Self::from_table_json(TIMING_TABLE).unwrap_or_else(|_| Self {
            rules: Vec::new(),
            previous_clock_seconds: None,
            fired: HashSet::new(),
        })
    }
}

impl ObjectiveScheduler {
    pub fn from_table_json(raw: &str) -> Result<Self, &'static str> {
        let root: serde_json::Value = serde_json::from_str(raw).map_err(|_| "TABLE_INVALID")?;
        if root.get("schema_version").and_then(|value| value.as_str()) != Some("1.0.0")
            || root.get("patch").and_then(|value| value.as_str()) != Some("7.42")
        {
            return Err("TABLE_VERSION_UNSUPPORTED");
        }
        let rows = root
            .get("objectives")
            .and_then(|value| value.as_array())
            .ok_or("TABLE_INVALID")?;
        let mut rules = Vec::new();
        for row in rows {
            if row.get("patch").and_then(|value| value.as_str()) != Some("7.42") {
                return Err("TABLE_VERSION_UNSUPPORTED");
            }
            if row.get("status").and_then(|value| value.as_str()) != Some("available") {
                continue;
            }
            let objective = row
                .get("id")
                .and_then(|value| value.as_str())
                .and_then(ObjectiveId::parse)
                .ok_or("TABLE_INVALID")?;
            let has_source = row
                .get("source_urls")
                .and_then(|value| value.as_array())
                .is_some_and(|urls| {
                    !urls.is_empty()
                        && urls.iter().all(|url| {
                            url.as_str()
                                .is_some_and(|url| url.starts_with("https://") && url.len() <= 256)
                        })
                });
            let _confirmed = row
                .get("confirmed_by_owner")
                .and_then(|value| value.as_bool())
                .ok_or("TABLE_INVALID")?;
            let verified = row
                .get("verified_in_official_notes")
                .and_then(|value| value.as_bool())
                .ok_or("TABLE_INVALID")?;
            if !has_source || (objective == ObjectiveId::Tormentor && !verified) {
                continue;
            }
            let spawn_times_seconds = row
                .get("spawn_times_seconds")
                .and_then(|value| value.as_array())
                .ok_or("TABLE_INVALID")?
                .iter()
                .map(|value| value.as_i64().ok_or("TABLE_INVALID"))
                .collect::<Result<Vec<_>, _>>()?;
            if spawn_times_seconds.is_empty() || spawn_times_seconds.iter().any(|time| *time < 0) {
                return Err("TABLE_INVALID");
            }
            let repeat_every_seconds = match row.get("repeat_every_seconds") {
                Some(serde_json::Value::Null) => None,
                Some(value) => Some(
                    value
                        .as_i64()
                        .filter(|seconds| *seconds > 0)
                        .ok_or("TABLE_INVALID")?,
                ),
                None => return Err("TABLE_INVALID"),
            };
            let documented_default_lead_s = row
                .get("pre_warning_seconds")
                .and_then(|value| value.as_array())
                .ok_or("TABLE_INVALID")?
                .iter()
                .map(|value| value.as_i64().ok_or("TABLE_INVALID"))
                .collect::<Result<Vec<_>, _>>()?;
            if documented_default_lead_s.len() > 2
                || documented_default_lead_s
                    .iter()
                    .any(|seconds| !(5..=60).contains(seconds))
                || documented_default_lead_s
                    .iter()
                    .enumerate()
                    .any(|(index, seconds)| documented_default_lead_s[..index].contains(seconds))
            {
                return Err("TABLE_INVALID");
            }
            rules.push(TimingRule {
                objective,
                spawn_times_seconds,
                repeat_every_seconds,
            });
        }
        Ok(Self {
            rules,
            previous_clock_seconds: None,
            fired: HashSet::new(),
        })
    }

    /// Reset occurrence deduplication when the host observes a new match.
    pub fn reset_match(&mut self) {
        self.previous_clock_seconds = None;
        self.fired.clear();
    }

    /// Drop only the clock edge after a stale frame gap; retain the per-match
    /// fired set so reconnecting cannot repeat an alert already shown.
    pub fn resync_clock(&mut self) {
        self.previous_clock_seconds = None;
    }

    /// Observe one admitted frame. Missing pause state, stale gaps, and frames
    /// outside active unpaused play clear the clock edge before it can fire.
    pub fn observe(
        &mut self,
        clock_seconds: Option<i64>,
        game_in_progress: bool,
        paused: Option<bool>,
        stale_gap: bool,
        new_match: bool,
        settings: ObjectiveSettings,
    ) -> Option<ObjectiveAlertBatch> {
        if new_match {
            self.reset_match();
        }
        let Some(current) = clock_seconds.filter(|seconds| *seconds >= -600) else {
            self.resync_clock();
            return None;
        };
        if stale_gap || !game_in_progress || paused != Some(false) {
            self.resync_clock();
            return None;
        }
        let Some(previous) = self.previous_clock_seconds.replace(current) else {
            return None;
        };
        let step = current.saturating_sub(previous);
        if step <= 0 || step > MAX_FORWARD_STEP_SECONDS {
            return None;
        }

        let mut due = BTreeMap::<i64, Vec<ObjectiveAlert>>::new();
        let rules = &self.rules;
        let fired = &mut self.fired;
        for rule in rules {
            if !rule.objective.enabled(settings) {
                continue;
            }
            let lead_times = settings.lead_times[rule.objective.index()];
            let max_warning = lead_times
                .into_iter()
                .flatten()
                .map(i64::from)
                .max()
                .unwrap_or(0);
            let candidate_end = current.saturating_add(max_warning);
            for occurrence in rule.occurrences_between(previous, candidate_end) {
                for lead_s in lead_times.into_iter().flatten() {
                    let due_seconds = occurrence.saturating_sub(i64::from(lead_s));
                    if previous < due_seconds && due_seconds <= current {
                        Self::push_if_new(
                            fired,
                            &mut due,
                            rule.objective,
                            occurrence,
                            due_seconds,
                            ObjectivePhase::PreWarn { lead_s },
                        );
                    }
                }
                if previous < occurrence && occurrence <= current {
                    Self::push_if_new(
                        fired,
                        &mut due,
                        rule.objective,
                        occurrence,
                        occurrence,
                        ObjectivePhase::Spawn,
                    );
                }
            }
        }
        if due.is_empty() {
            return None;
        }

        // The GSI frame is the delivery boundary. Merge all due rows from this
        // frame; due_seconds remains available to the renderer for localization.
        let alerts = due.into_values().flatten().collect();
        Some(ObjectiveAlertBatch {
            observed_clock_seconds: current,
            alerts,
        })
    }

    fn push_if_new(
        fired: &mut HashSet<FiredKey>,
        due: &mut BTreeMap<i64, Vec<ObjectiveAlert>>,
        objective: ObjectiveId,
        occurrence_seconds: i64,
        due_seconds: i64,
        phase: ObjectivePhase,
    ) {
        let key = FiredKey {
            objective,
            occurrence_seconds,
            phase,
        };
        if fired.insert(key) {
            due.entry(due_seconds).or_default().push(ObjectiveAlert {
                objective,
                phase,
                occurrence_seconds,
                due_seconds,
            });
        }
    }
}

impl TimingRule {
    fn occurrences_between(&self, previous: i64, current: i64) -> Vec<i64> {
        let mut occurrences: Vec<i64> = self
            .spawn_times_seconds
            .iter()
            .copied()
            .filter(|time| previous < *time && *time <= current)
            .collect();
        if let (Some(first), Some(interval)) = (
            self.spawn_times_seconds.first().copied(),
            self.repeat_every_seconds,
        ) {
            let first_index = if previous < first {
                0
            } else {
                previous.saturating_sub(first) / interval + 1
            };
            let last_index = current.saturating_sub(first) / interval;
            if last_index >= first_index {
                for index in first_index..=last_index {
                    let Some(time) = index
                        .checked_mul(interval)
                        .and_then(|delta| first.checked_add(delta))
                    else {
                        break;
                    };
                    if previous < time && time <= current && !occurrences.contains(&time) {
                        occurrences.push(time);
                    }
                }
            }
        }
        occurrences.sort_unstable();
        occurrences
    }
}

#[cfg(test)]
mod tests {
    use super::{ObjectiveId, ObjectivePhase, ObjectiveScheduler, ObjectiveSettings, TIMING_TABLE};

    fn scheduler() -> ObjectiveScheduler {
        ObjectiveScheduler::default()
    }

    fn observe(
        scheduler: &mut ObjectiveScheduler,
        clock: i64,
        in_progress: bool,
        paused: bool,
    ) -> Vec<(ObjectiveId, ObjectivePhase, i64)> {
        observe_with_settings(
            scheduler,
            clock,
            in_progress,
            paused,
            ObjectiveSettings::default(),
        )
    }

    fn observe_with_settings(
        scheduler: &mut ObjectiveScheduler,
        clock: i64,
        in_progress: bool,
        paused: bool,
        settings: ObjectiveSettings,
    ) -> Vec<(ObjectiveId, ObjectivePhase, i64)> {
        scheduler
            .observe(
                Some(clock),
                in_progress,
                Some(paused),
                false,
                false,
                settings,
            )
            .map(|batch| {
                batch
                    .alerts
                    .into_iter()
                    .map(|alert| (alert.objective, alert.phase, alert.due_seconds))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn water_and_power_have_one_thirty_second_fifteen_second_and_spawn_alert() {
        let mut s = scheduler();
        for (before, due, phase) in [
            (89, 90, ObjectivePhase::PreWarn { lead_s: 30 }),
            (104, 105, ObjectivePhase::PreWarn { lead_s: 15 }),
            (119, 120, ObjectivePhase::Spawn),
        ] {
            let _ = observe(&mut s, before, true, false);
            assert_eq!(
                observe(&mut s, due, true, false),
                vec![(ObjectiveId::WaterRune, phase, due)]
            );
            assert!(observe(&mut s, due, true, false).is_empty());
        }

        let mut power = scheduler();
        for (before, due, phase) in [
            (329, 330, ObjectivePhase::PreWarn { lead_s: 30 }),
            (344, 345, ObjectivePhase::PreWarn { lead_s: 15 }),
            (359, 360, ObjectivePhase::Spawn),
        ] {
            let _ = observe(&mut power, before, true, false);
            assert_eq!(
                observe(&mut power, due, true, false),
                vec![(ObjectiveId::PowerRune, phase, due)]
            );
        }

        let _ = observe(&mut power, 449, true, false);
        assert_eq!(
            observe(&mut power, 450, true, false),
            vec![(
                ObjectiveId::PowerRune,
                ObjectivePhase::PreWarn { lead_s: 30 },
                450
            )]
        );
        let _ = observe(&mut power, 464, true, false);
        assert_eq!(
            observe(&mut power, 465, true, false),
            vec![(
                ObjectiveId::PowerRune,
                ObjectivePhase::PreWarn { lead_s: 15 },
                465
            )]
        );
        let _ = observe(&mut power, 479, true, false);
        assert_eq!(
            observe(&mut power, 480, true, false),
            // 480 s is also the second bounty spawn (2 x 240 s): one merged batch.
            vec![
                (ObjectiveId::PowerRune, ObjectivePhase::Spawn, 480),
                (ObjectiveId::BountyRune, ObjectivePhase::Spawn, 480),
            ]
        );
    }

    #[test]
    fn bounty_wisdom_and_tormentor_are_spawn_only_and_repeat_by_table() {
        let mut s = scheduler();
        let _ = observe(&mut s, -1, true, false);
        assert!(observe(&mut s, 0, true, false).contains(&(
            ObjectiveId::BountyRune,
            ObjectivePhase::Spawn,
            0
        )));
        let _ = observe(&mut s, 239, true, false);
        let merged = observe(&mut s, 240, true, false);
        assert!(merged.contains(&(ObjectiveId::BountyRune, ObjectivePhase::Spawn, 240)));
        assert!(merged.contains(&(ObjectiveId::WaterRune, ObjectivePhase::Spawn, 240)));

        let mut s = scheduler();
        let _ = observe(&mut s, 419, true, false);
        assert_eq!(
            observe(&mut s, 420, true, false),
            vec![(ObjectiveId::WisdomRune, ObjectivePhase::Spawn, 420)]
        );
        let _ = observe(&mut s, 839, true, false);
        assert_eq!(
            observe(&mut s, 840, true, false),
            // 840 s is also a power rune spawn (360 + 4 x 120 s): one merged batch.
            vec![
                (ObjectiveId::PowerRune, ObjectivePhase::Spawn, 840),
                (ObjectiveId::WisdomRune, ObjectivePhase::Spawn, 840),
            ]
        );

        let mut s = scheduler();
        let _ = observe(&mut s, 899, true, false);
        assert_eq!(
            observe(&mut s, 900, true, false),
            vec![(ObjectiveId::Tormentor, ObjectivePhase::Spawn, 900)]
        );
    }

    #[test]
    fn pause_missing_frame_stale_gap_and_non_game_state_never_fire() {
        let mut s = scheduler();
        let _ = observe(&mut s, 89, true, false);
        assert!(observe(&mut s, 90, true, true).is_empty());
        assert!(observe(&mut s, 91, true, false).is_empty());

        let _ = s.observe(
            Some(104),
            true,
            Some(false),
            false,
            false,
            ObjectiveSettings::default(),
        );
        assert!(s
            .observe(
                Some(105),
                true,
                None,
                false,
                false,
                ObjectiveSettings::default()
            )
            .is_none());
        assert!(s
            .observe(
                Some(105),
                true,
                Some(false),
                false,
                false,
                ObjectiveSettings::default()
            )
            .is_none());

        let _ = observe(&mut s, 329, true, false);
        assert!(s
            .observe(
                Some(330),
                true,
                Some(false),
                true,
                false,
                ObjectiveSettings::default()
            )
            .is_none());
        assert!(observe(&mut s, 331, true, false).is_empty());

        let _ = observe(&mut s, 344, true, false);
        assert!(observe(&mut s, 345, false, false).is_empty());
        assert!(observe(&mut s, 346, true, false).is_empty());

        assert!(s
            .observe(
                None,
                true,
                Some(false),
                false,
                false,
                ObjectiveSettings::default()
            )
            .is_none());
        assert!(observe(&mut s, 360, true, false).is_empty());

        let mut transition = scheduler();
        assert!(observe(&mut transition, 89, false, false).is_empty());
        assert!(observe(&mut transition, 90, true, false).is_empty());
        assert!(observe(&mut transition, 91, true, false).is_empty());
    }

    #[test]
    fn backward_and_large_forward_jumps_resync_without_backlog() {
        let mut s = scheduler();
        let _ = observe(&mut s, 89, true, false);
        assert!(observe(&mut s, 95, true, false).is_empty()); // > 5 s
        assert!(observe(&mut s, 96, true, false).is_empty());

        let _ = observe(&mut s, 100, true, false);
        assert!(observe(&mut s, 95, true, false).is_empty());
        assert!(observe(&mut s, 100, true, false).is_empty());
    }

    #[test]
    fn an_occurrence_phase_fires_once_until_a_new_match() {
        let mut s = scheduler();
        let _ = observe(&mut s, 89, true, false);
        assert!(!observe(&mut s, 90, true, false).is_empty());
        let _ = observe(&mut s, 80, true, false);
        assert!(observe(&mut s, 90, true, false).is_empty());

        let _ = s.observe(
            Some(89),
            true,
            Some(false),
            false,
            true,
            ObjectiveSettings::default(),
        );
        assert!(!observe(&mut s, 90, true, false).is_empty());
    }

    #[test]
    fn disabled_objective_produces_no_alert() {
        let mut s = scheduler();
        let settings = ObjectiveSettings {
            water_rune: false,
            ..ObjectiveSettings::default()
        };
        let _ = s.observe(Some(89), true, Some(false), false, false, settings);
        assert!(s
            .observe(Some(90), true, Some(false), false, false, settings)
            .is_none());
    }

    #[test]
    fn scheduler_uses_custom_leads_and_does_not_fire_a_passed_alert_after_a_change() {
        let mut scheduled = scheduler();
        let mut settings = ObjectiveSettings::default();
        settings.lead_times[ObjectiveId::WaterRune.index()] = [Some(20), Some(7)];

        let _ = observe_with_settings(&mut scheduled, 99, true, false, settings);
        assert_eq!(
            observe_with_settings(&mut scheduled, 100, true, false, settings),
            vec![(
                ObjectiveId::WaterRune,
                ObjectivePhase::PreWarn { lead_s: 20 },
                100,
            )]
        );
        let _ = observe_with_settings(&mut scheduled, 112, true, false, settings);
        assert_eq!(
            observe_with_settings(&mut scheduled, 113, true, false, settings),
            vec![(
                ObjectiveId::WaterRune,
                ObjectivePhase::PreWarn { lead_s: 7 },
                113,
            )]
        );

        let mut changed = scheduler();
        let _ = observe(&mut changed, 90, true, false);
        let mut late = ObjectiveSettings::default();
        late.lead_times[ObjectiveId::WaterRune.index()] = [Some(30), None];
        assert!(observe_with_settings(&mut changed, 91, true, false, late).is_empty());
    }

    #[test]
    fn unavailable_respawns_have_no_scheduler_rows_or_times() {
        let table: serde_json::Value = serde_json::from_str(TIMING_TABLE).unwrap();
        for id in ["roshan_respawn", "tormentor_respawn"] {
            let row = table["objectives"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == id)
                .unwrap();
            assert_eq!(row["status"], "unavailable");
            assert!(row["spawn_times_seconds"].is_null());
        }
        assert!(ObjectiveScheduler::from_table_json(TIMING_TABLE).is_ok());
    }

    #[test]
    fn every_timing_row_is_patch_tagged_sourced_and_rune_rows_owner_confirmed() {
        let table: serde_json::Value = serde_json::from_str(TIMING_TABLE).unwrap();
        for row in table["objectives"].as_array().unwrap() {
            assert_eq!(row["patch"], "7.42");
            let rune = ["water_rune", "power_rune", "bounty_rune", "wisdom_rune"]
                .iter()
                .any(|id| row["id"] == *id);
            // DEC-EVENT-RUNE-SPAWN revision 3: the owner confirmed the wiki rune
            // timings for 7.42 pending a live check; other rows stay unconfirmed.
            assert_eq!(row["confirmed_by_owner"], rune);
            if rune {
                assert_eq!(
                    row["owner_note"],
                    "owner confirmed 2026-11-22, pending live check"
                );
            }
            assert!(!row["source_urls"].as_array().unwrap().is_empty());
            assert!(row["source_urls"].as_array().unwrap().iter().all(|source| {
                source
                    .as_str()
                    .is_some_and(|source| source.starts_with("https://"))
            }));
            if row["id"] == "tormentor" {
                assert_eq!(row["verified_in_official_notes"], true);
            }
            if [
                "water_rune",
                "power_rune",
                "bounty_rune",
                "wisdom_rune",
                "roshan_respawn",
                "tormentor_respawn",
            ]
            .contains(&row["id"].as_str().unwrap())
            {
                assert_eq!(row["verified_in_official_notes"], false);
            }
        }
    }

    #[test]
    fn a_timing_row_without_a_source_is_unavailable_to_the_scheduler() {
        let mut table: serde_json::Value = serde_json::from_str(TIMING_TABLE).unwrap();
        let water = table["objectives"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["id"] == "water_rune")
            .unwrap();
        water["source_urls"] = serde_json::json!([]);
        let scheduler = ObjectiveScheduler::from_table_json(&table.to_string()).unwrap();
        assert!(!scheduler
            .rules
            .iter()
            .any(|rule| rule.objective == ObjectiveId::WaterRune));
    }

    #[test]
    fn same_second_objectives_merge_into_one_batch() {
        let mut s = scheduler();
        let _ = observe(&mut s, 239, true, false);
        let batch = s
            .observe(
                Some(240),
                true,
                Some(false),
                false,
                false,
                ObjectiveSettings::default(),
            )
            .unwrap();
        assert_eq!(batch.alerts.len(), 2);
        assert!(batch
            .alerts
            .iter()
            .any(|alert| alert.objective == ObjectiveId::WaterRune));
        assert!(batch
            .alerts
            .iter()
            .any(|alert| alert.objective == ObjectiveId::BountyRune));
    }
}
