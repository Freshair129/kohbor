//! Counter-pick suggestions from the reviewed static table (DEC-PICK-STATS revision 3, option c).
//!
//! No request is made here and identified heroes never leave this module; the table ships with the
//! app and is reviewed by the owner every patch. Rows carry a reason, not a statistic.

use super::parse::HeroStatsTable;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// The host keeps at most this many counter rows; the overlay draws at most three of them.
pub(crate) const MAX_COUNTER_ROWS: usize = 6;

const COUNTER_TABLE_JSON: &str = include_str!("counter-table.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Radiant,
    Dire,
}

impl Side {
    pub(crate) fn from_slot_team(team: &str) -> Option<Self> {
        match team {
            "radiant" => Some(Self::Radiant),
            "dire" => Some(Self::Dire),
            _ => None,
        }
    }
}

/// One identified 5b slot. `side` is `None` when the slot's team was not reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IdentifiedSlot {
    pub(crate) hero_key: String,
    pub(crate) side: Option<Side>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CounterHero {
    pub(crate) hero_id: u32,
    pub(crate) hero_name: String,
    pub(crate) label: String,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CounterEntry {
    pub(crate) enemy_id: u32,
    pub(crate) enemy_name: String,
    pub(crate) enemy_label: String,
    pub(crate) patch: String,
    pub(crate) date: String,
    pub(crate) counters: Vec<CounterHero>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CounterTableFile {
    schema: u32,
    #[allow(dead_code)]
    note: String,
    entries: Vec<CounterEntry>,
}

fn is_hero_key(text: &str) -> bool {
    text.starts_with("npc_dota_hero_") && text.len() <= 128
}

fn filled(text: &str) -> bool {
    !text.trim().is_empty()
}

/// Validates the whole table; any defect rejects it, and then no counter is shown.
pub(crate) fn parse_counter_table(json: &str) -> Option<Vec<CounterEntry>> {
    let file: CounterTableFile = serde_json::from_str(json).ok()?;
    if file.schema != 1 {
        return None;
    }
    let mut enemies = HashSet::new();
    for entry in &file.entries {
        let entry_ok = is_hero_key(&entry.enemy_name)
            && filled(&entry.enemy_label)
            && filled(&entry.patch)
            && filled(&entry.date)
            && !entry.counters.is_empty()
            && enemies.insert(entry.enemy_name.as_str());
        if !entry_ok {
            return None;
        }
        let mut seen = HashSet::new();
        for counter in &entry.counters {
            let counter_ok = is_hero_key(&counter.hero_name)
                && counter.hero_name != entry.enemy_name
                && filled(&counter.label)
                && filled(&counter.reason)
                && seen.insert(counter.hero_name.as_str());
            if !counter_ok {
                return None;
            }
        }
    }
    Some(file.entries)
}

pub(crate) fn counter_table() -> &'static [CounterEntry] {
    static TABLE: OnceLock<Vec<CounterEntry>> = OnceLock::new();
    TABLE.get_or_init(|| parse_counter_table(COUNTER_TABLE_JSON).unwrap_or_default())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CounterState {
    /// The build switch is off (owner review of the table and the mock is pending).
    BuildDisabled,
    /// Draft Bar 5b has no usable output.
    DraftBarUnavailable,
    /// The own side or a slot side is unknown, so enemies cannot be told from allies.
    SidesUnknown,
    /// No enemy hero is identified yet.
    NoEnemy,
    /// At least one enemy is identified; rows may still be empty if the table has no entry.
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CounterRow {
    pub(crate) hero_id: u32,
    pub(crate) name: String,
    /// Enemy labels this hero answers, in enemy slot order.
    pub(crate) against: Vec<String>,
    /// The table reason for the first enemy in `against`.
    pub(crate) reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Counters {
    pub(crate) state: CounterState,
    /// Labels of identified enemies that have a table entry, in slot order.
    pub(crate) enemies: Vec<String>,
    /// Identified enemies (with or without a table entry).
    pub(crate) enemies_identified: u8,
    pub(crate) rows: Vec<CounterRow>,
    /// True only when the deck position was applied with known position groups.
    pub(crate) position_filtered: bool,
}

impl Counters {
    pub(crate) fn empty(state: CounterState) -> Self {
        Self {
            state,
            enemies: Vec::new(),
            enemies_identified: 0,
            rows: Vec::new(),
            position_filtered: false,
        }
    }
}

pub(crate) struct CounterInput<'a> {
    pub(crate) build_enabled: bool,
    pub(crate) draft_bar_available: bool,
    pub(crate) own_side: Option<Side>,
    pub(crate) slots: &'a [IdentifiedSlot],
    /// Hero IDs of the selected position group, when both a position and the groups are known.
    pub(crate) group_ids: Option<&'a [u32]>,
    /// The cached provider table, used only to reject rows whose id and name disagree.
    pub(crate) heroes: Option<&'a HeroStatsTable>,
}

fn id_matches(heroes: Option<&HeroStatsTable>, id: u32, name: &str) -> bool {
    heroes.is_none_or(|table| table.get(&id).is_none_or(|hero| hero.name == name))
}

pub(crate) fn counter_suggestions(input: &CounterInput<'_>, table: &[CounterEntry]) -> Counters {
    if !input.build_enabled {
        return Counters::empty(CounterState::BuildDisabled);
    }
    if !input.draft_bar_available {
        return Counters::empty(CounterState::DraftBarUnavailable);
    }
    let Some(own_side) = input.own_side else {
        return Counters::empty(CounterState::SidesUnknown);
    };
    if input.slots.iter().any(|slot| slot.side.is_none()) {
        return Counters::empty(CounterState::SidesUnknown);
    }
    let enemy_keys: Vec<&str> = input
        .slots
        .iter()
        .filter(|slot| slot.side != Some(own_side))
        .map(|slot| slot.hero_key.as_str())
        .collect();
    if enemy_keys.is_empty() {
        return Counters::empty(CounterState::NoEnemy);
    }
    let taken: HashSet<&str> = input
        .slots
        .iter()
        .map(|slot| slot.hero_key.as_str())
        .collect();

    let mut enemies = Vec::new();
    // hero_name -> (first seen order, row)
    let mut rows: HashMap<&str, (usize, CounterRow)> = HashMap::new();
    let mut order = 0_usize;
    for enemy_key in &enemy_keys {
        let Some(entry) = table.iter().find(|entry| entry.enemy_name == *enemy_key) else {
            continue;
        };
        if !id_matches(input.heroes, entry.enemy_id, &entry.enemy_name) {
            continue;
        }
        enemies.push(entry.enemy_label.clone());
        for counter in &entry.counters {
            let allowed = !taken.contains(counter.hero_name.as_str())
                && id_matches(input.heroes, counter.hero_id, &counter.hero_name)
                && input
                    .group_ids
                    .is_none_or(|ids| ids.contains(&counter.hero_id));
            if !allowed {
                continue;
            }
            rows.entry(counter.hero_name.as_str())
                .and_modify(|(_, row)| {
                    if !row.against.contains(&entry.enemy_label) {
                        row.against.push(entry.enemy_label.clone());
                    }
                })
                .or_insert_with(|| {
                    order += 1;
                    (
                        order,
                        CounterRow {
                            hero_id: counter.hero_id,
                            name: counter.label.clone(),
                            against: vec![entry.enemy_label.clone()],
                            reason: counter.reason.clone(),
                        },
                    )
                });
        }
    }
    let mut rows: Vec<(usize, CounterRow)> = rows.into_values().collect();
    rows.sort_by(|(left_order, left), (right_order, right)| {
        right
            .against
            .len()
            .cmp(&left.against.len())
            .then_with(|| left_order.cmp(right_order))
            .then_with(|| left.hero_id.cmp(&right.hero_id))
    });
    Counters {
        state: CounterState::Ready,
        enemies,
        enemies_identified: u8::try_from(enemy_keys.len()).unwrap_or(u8::MAX),
        rows: rows
            .into_iter()
            .take(MAX_COUNTER_ROWS)
            .map(|(_, row)| row)
            .collect(),
        position_filtered: input.group_ids.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        counter_suggestions, parse_counter_table, CounterInput, CounterState, IdentifiedSlot, Side,
        COUNTER_TABLE_JSON,
    };
    use crate::hero_stats::parse::{HeroStatsRow, HeroStatsTable};
    use std::collections::BTreeMap;

    fn slot(key: &str, side: Option<Side>) -> IdentifiedSlot {
        IdentifiedSlot {
            hero_key: format!("npc_dota_hero_{key}"),
            side,
        }
    }

    fn input<'a>(slots: &'a [IdentifiedSlot], own: Option<Side>) -> CounterInput<'a> {
        CounterInput {
            build_enabled: true,
            draft_bar_available: true,
            own_side: own,
            slots,
            group_ids: None,
            heroes: None,
        }
    }

    fn table() -> Vec<super::CounterEntry> {
        parse_counter_table(COUNTER_TABLE_JSON).expect("the shipped counter table must validate")
    }

    #[test]
    fn shipped_table_is_valid_dated_and_answers_the_owner_example() {
        let table = table();
        assert!(table.len() >= 10);
        for entry in &table {
            assert_eq!(entry.patch, "7.42", "{}", entry.enemy_label);
            assert_eq!(entry.date, "2026-11-22", "{}", entry.enemy_label);
        }
        let slots = [slot("phantom_lancer", Some(Side::Dire))];
        let counters = counter_suggestions(&input(&slots, Some(Side::Radiant)), &table);
        assert_eq!(counters.state, CounterState::Ready);
        assert_eq!(counters.enemies, vec!["Phantom Lancer"]);
        let names: Vec<&str> = counters.rows.iter().map(|row| row.name.as_str()).collect();
        assert!(names.contains(&"Earthshaker") && names.contains(&"Sven"));
    }

    #[test]
    fn every_gate_yields_its_own_state_and_no_rows() {
        let table = table();
        let slots = [slot("phantom_lancer", Some(Side::Dire))];
        let mut off = input(&slots, Some(Side::Radiant));
        off.build_enabled = false;
        assert_eq!(
            counter_suggestions(&off, &table).state,
            CounterState::BuildDisabled
        );
        let mut no_bar = input(&slots, Some(Side::Radiant));
        no_bar.draft_bar_available = false;
        assert_eq!(
            counter_suggestions(&no_bar, &table).state,
            CounterState::DraftBarUnavailable
        );
        let unknown_own = counter_suggestions(&input(&slots, None), &table);
        assert_eq!(unknown_own.state, CounterState::SidesUnknown);
        assert!(unknown_own.rows.is_empty());
        let unknown_slot = [slot("phantom_lancer", None)];
        assert_eq!(
            counter_suggestions(&input(&unknown_slot, Some(Side::Radiant)), &table).state,
            CounterState::SidesUnknown
        );
        let ally_only = [slot("phantom_lancer", Some(Side::Radiant))];
        let ally = counter_suggestions(&input(&ally_only, Some(Side::Radiant)), &table);
        assert_eq!(
            ally.state,
            CounterState::NoEnemy,
            "an ally PL is not countered"
        );
        assert!(ally.rows.is_empty());
    }

    #[test]
    fn rows_rank_by_enemies_answered_and_skip_taken_heroes() {
        let table = table();
        // Earthshaker answers both PL and Naga; Sven is already picked by an ally.
        let slots = [
            slot("phantom_lancer", Some(Side::Dire)),
            slot("naga_siren", Some(Side::Dire)),
            slot("sven", Some(Side::Radiant)),
            slot("unknown_to_table", Some(Side::Dire)),
        ];
        let counters = counter_suggestions(&input(&slots, Some(Side::Radiant)), &table);
        assert_eq!(counters.enemies_identified, 3);
        assert_eq!(counters.enemies, vec!["Phantom Lancer", "Naga Siren"]);
        assert_eq!(counters.rows[0].name, "Earthshaker");
        assert_eq!(
            counters.rows[0].against,
            vec!["Phantom Lancer", "Naga Siren"]
        );
        assert!(counters.rows.iter().all(|row| row.name != "Sven"));
        assert!(counters.rows.len() <= super::MAX_COUNTER_ROWS);
    }

    #[test]
    fn position_group_and_provider_ids_filter_rows() {
        let table = table();
        let slots = [slot("phantom_lancer", Some(Side::Dire))];
        let mut filtered = input(&slots, Some(Side::Radiant));
        let group = [18_u32];
        filtered.group_ids = Some(&group);
        let counters = counter_suggestions(&filtered, &table);
        assert!(counters.position_filtered);
        assert_eq!(
            counters
                .rows
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<_>>(),
            ["Sven"]
        );

        let mut heroes = HeroStatsTable::new();
        heroes.insert(
            7,
            HeroStatsRow {
                id: 7,
                name: "npc_dota_hero_renumbered".to_owned(),
                localized_name: "Other".to_owned(),
                roles: Vec::new(),
                brackets: BTreeMap::new(),
            },
        );
        let mut checked = input(&slots, Some(Side::Radiant));
        checked.heroes = Some(&heroes);
        let counters = counter_suggestions(&checked, &table);
        assert!(
            counters.rows.iter().all(|row| row.name != "Earthshaker"),
            "an id/name disagreement drops that row"
        );
        assert!(counters.rows.iter().any(|row| row.name == "Sven"));
    }

    #[test]
    fn invalid_tables_are_rejected_whole() {
        let entry = |enemy: &str, counter: &str, reason: &str| {
            format!(
                r#"{{"schema":1,"note":"n","entries":[{{"enemy_id":1,"enemy_name":"{enemy}","enemy_label":"E","patch":"7.42","date":"2026-11-22","counters":[{{"hero_id":2,"hero_name":"{counter}","label":"C","reason":"{reason}"}}]}}]}}"#
            )
        };
        assert!(parse_counter_table(&entry("npc_dota_hero_a", "npc_dota_hero_b", "r")).is_some());
        assert!(parse_counter_table(&entry("a", "npc_dota_hero_b", "r")).is_none());
        assert!(parse_counter_table(&entry("npc_dota_hero_a", "npc_dota_hero_a", "r")).is_none());
        assert!(parse_counter_table(&entry("npc_dota_hero_a", "npc_dota_hero_b", " ")).is_none());
        assert!(parse_counter_table(r#"{"schema":2,"note":"n","entries":[]}"#).is_none());
        assert!(parse_counter_table("nope").is_none());
    }

    #[test]
    fn side_parsing_accepts_only_the_two_teams() {
        assert_eq!(Side::from_slot_team("radiant"), Some(Side::Radiant));
        assert_eq!(Side::from_slot_team("dire"), Some(Side::Dire));
        assert_eq!(Side::from_slot_team("Radiant"), None);
    }
}
