use super::parse::{HeroStatsTable, LaneRoleTable, ParseError};
#[cfg(any(test, feature = "lab"))]
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PositionGroups {
    /// Estimated positions 1 through 5, in that order. Hero IDs may appear in several groups.
    pub(crate) groups: [Vec<u32>; 5],
}

pub(crate) fn build_position_groups(
    heroes: &HeroStatsTable,
    lanes: [&LaneRoleTable; 3],
) -> Result<PositionGroups, ParseError> {
    let mut result = PositionGroups::default();
    for (hero_id, hero) in heroes {
        let safe = lanes[0].get(hero_id).map_or(0, |counts| counts.games);
        let mid = lanes[1].get(hero_id).map_or(0, |counts| counts.games);
        let off = lanes[2].get(hero_id).map_or(0, |counts| counts.games);
        let total = u128::from(safe) + u128::from(mid) + u128::from(off);
        if total < 200 {
            continue;
        }
        let safe_qualifies = qualifies(safe, total);
        let mid_qualifies = qualifies(mid, total);
        let off_qualifies = qualifies(off, total);
        let support = hero.roles.iter().any(|role| role == "Support");

        if safe_qualifies {
            result.groups[if support { 4 } else { 0 }].push(*hero_id);
        }
        if mid_qualifies {
            result.groups[1].push(*hero_id);
        }
        if off_qualifies {
            result.groups[if support { 3 } else { 2 }].push(*hero_id);
        }
    }
    apply_overrides(&mut result, heroes, position_overrides());
    for group in &mut result.groups {
        group.sort_unstable();
        group.dedup();
    }
    Ok(result)
}

/// The reviewed Kohbor-owned correction list (`position-overrides.json`), applied after rule B.
const POSITION_OVERRIDES_JSON: &str = include_str!("position-overrides.json");

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionOverride {
    pub(crate) hero_id: u32,
    pub(crate) hero_name: String,
    pub(crate) label: String,
    pub(crate) remove: Vec<u8>,
    pub(crate) add: Vec<u8>,
    pub(crate) reason: String,
    pub(crate) patch: String,
    pub(crate) date: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionOverrideFile {
    schema: u32,
    #[allow(dead_code)]
    note: String,
    overrides: Vec<PositionOverride>,
}

/// Parses and validates the embedded list. A list that fails validation is not applied at all, so
/// the groups fall back to plain rule B; a unit test keeps the shipped file valid.
pub(crate) fn parse_position_overrides(json: &str) -> Option<Vec<PositionOverride>> {
    let file: PositionOverrideFile = serde_json::from_str(json).ok()?;
    if file.schema != 1 {
        return None;
    }
    let mut seen = std::collections::HashSet::new();
    for row in &file.overrides {
        let positions_ok = row
            .remove
            .iter()
            .chain(&row.add)
            .all(|position| (1..=5).contains(position));
        let overlap = row.remove.iter().any(|position| row.add.contains(position));
        let text_ok = [
            &row.hero_name,
            &row.label,
            &row.reason,
            &row.patch,
            &row.date,
        ]
        .iter()
        .all(|text| !text.trim().is_empty());
        let changes = !(row.remove.is_empty() && row.add.is_empty());
        if !positions_ok || overlap || !text_ok || !changes || !seen.insert(row.hero_id) {
            return None;
        }
    }
    Some(file.overrides)
}

fn position_overrides() -> &'static [PositionOverride] {
    static OVERRIDES: std::sync::OnceLock<Vec<PositionOverride>> = std::sync::OnceLock::new();
    OVERRIDES.get_or_init(|| parse_position_overrides(POSITION_OVERRIDES_JSON).unwrap_or_default())
}

/// Applies each row only when the provider row has the same id and internal name, so a stale or
/// renumbered entry is skipped and rule B stays for that hero.
fn apply_overrides(
    groups: &mut PositionGroups,
    heroes: &HeroStatsTable,
    overrides: &[PositionOverride],
) {
    for row in overrides {
        let Some(hero) = heroes.get(&row.hero_id) else {
            continue;
        };
        if hero.name != row.hero_name {
            continue;
        }
        for position in &row.remove {
            groups.groups[usize::from(*position - 1)].retain(|id| *id != row.hero_id);
        }
        for position in &row.add {
            let group = &mut groups.groups[usize::from(*position - 1)];
            if !group.contains(&row.hero_id) {
                group.push(row.hero_id);
            }
        }
    }
}

fn qualifies(games: u64, total: u128) -> bool {
    games >= 100 && u128::from(games) * 4 >= total
}

/// Dev-only offline group dump for the lead's supplied provider response files.
#[cfg(any(test, feature = "lab"))]
#[allow(dead_code)]
pub(crate) fn print_position_groups_from_files(
    hero_stats_path: &Path,
    safe_path: &Path,
    mid_path: &Path,
    off_path: &Path,
) -> Result<(), String> {
    let read =
        |path: &Path| std::fs::read(path).map_err(|_| "DRAFT_STATS_DUMP_READ_FAILED".to_owned());
    let heroes = super::parse::parse_hero_stats(&read(hero_stats_path)?)
        .map_err(|_| "DRAFT_STATS_DUMP_INVALID_HERO_STATS".to_owned())?;
    let safe = super::parse::parse_lane_roles(&read(safe_path)?, super::request::LaneRole::Safe)
        .map_err(|_| "DRAFT_STATS_DUMP_INVALID_SAFE_LANE")?;
    let mid = super::parse::parse_lane_roles(&read(mid_path)?, super::request::LaneRole::Mid)
        .map_err(|_| "DRAFT_STATS_DUMP_INVALID_MID_LANE")?;
    let off = super::parse::parse_lane_roles(&read(off_path)?, super::request::LaneRole::Off)
        .map_err(|_| "DRAFT_STATS_DUMP_INVALID_OFF_LANE")?;
    let groups = build_position_groups(&heroes, [&safe, &mid, &off])
        .map_err(|_| "DRAFT_STATS_DUMP_OVERFLOW".to_owned())?;
    for (index, group) in groups.groups.iter().enumerate() {
        println!("Position {} (ค่าประมาณ)", index + 1);
        for hero_id in group {
            if let Some(hero) = heroes.get(hero_id) {
                println!("  {}", hero.localized_name);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_position_groups, parse_position_overrides, PositionGroups};
    use crate::hero_stats::parse::{Counts, HeroStatsRow, HeroStatsTable};
    use std::collections::BTreeMap;

    fn hero(roles: &[&str]) -> HeroStatsRow {
        HeroStatsRow {
            id: 1,
            name: "npc_dota_hero_test".to_owned(),
            localized_name: "Test Hero".to_owned(),
            roles: roles.iter().map(|role| (*role).to_owned()).collect(),
            brackets: BTreeMap::new(),
        }
    }

    fn groups_for(safe: u64, mid: u64, off: u64, roles: &[&str]) -> PositionGroups {
        let mut heroes = HeroStatsTable::new();
        heroes.insert(1, hero(roles));
        let lane = |games| {
            BTreeMap::from([(
                1,
                Counts {
                    games,
                    wins: games / 2,
                },
            )])
        };
        let safe = lane(safe);
        let mid = lane(mid);
        let off = lane(off);
        build_position_groups(&heroes, [&safe, &mid, &off]).unwrap()
    }

    #[test]
    fn rule_b_requires_one_hundred_lane_games_and_two_hundred_total_games() {
        assert!(
            groups_for(99, 0, 101, &[]).groups[0].is_empty(),
            "99 safe games is below the lane floor"
        );
        assert_eq!(
            groups_for(100, 0, 99, &[]).groups[0],
            Vec::<u32>::new(),
            "199 total games is below the floor"
        );
        assert_eq!(
            groups_for(100, 0, 100, &[]).groups[0],
            vec![1],
            "200 total and 100 safe games qualify"
        );
    }

    #[test]
    fn rule_b_uses_exact_twenty_five_percent_threshold_and_allows_multiple_groups() {
        assert!(
            groups_for(100, 0, 302, &[]).groups[0].is_empty(),
            "100/402 is below 25 percent"
        );
        assert_eq!(
            groups_for(100, 0, 300, &[]).groups[0],
            vec![1],
            "100/400 is exactly 25 percent"
        );
        let multi = groups_for(100, 100, 200, &["Support"]);
        assert_eq!(multi.groups[1], vec![1]);
        assert_eq!(multi.groups[3], vec![1]);
        assert_eq!(multi.groups[4], vec![1]);
    }

    fn named_hero(id: u32, name: &str, roles: &[&str]) -> HeroStatsRow {
        HeroStatsRow {
            id,
            name: name.to_owned(),
            ..hero(roles)
        }
    }

    fn counts(games: u64) -> Counts {
        Counts {
            games,
            wins: games / 2,
        }
    }

    #[test]
    fn shipped_override_list_is_valid_and_dated_for_the_reviewed_patch() {
        let rows = parse_position_overrides(super::POSITION_OVERRIDES_JSON)
            .expect("the shipped override list must validate");
        assert_eq!(rows.len(), 11);
        for row in &rows {
            assert_eq!(row.patch, "7.42", "{}", row.label);
            assert_eq!(row.date, "2026-11-22", "{}", row.label);
            assert!(row.hero_name.starts_with("npc_dota_hero_"), "{}", row.label);
        }
        let by_label = |label: &str| rows.iter().find(|row| row.label == label).unwrap();
        assert_eq!(by_label("Alchemist").remove, vec![4, 5]);
        assert!(by_label("Wraith King").add.contains(&1));
        for label in [
            "Pudge",
            "Pugna",
            "Techies",
            "Clockwerk",
            "Tusk",
            "Necrophos",
            "Riki",
        ] {
            assert!(by_label(label).remove.contains(&1), "{label} leaves pos 1");
        }
        for label in ["Treant Protector", "Chen"] {
            assert_eq!(by_label(label).add, vec![4, 5]);
        }
    }

    #[test]
    fn invalid_override_lists_are_rejected_whole() {
        let row = |remove: &str, add: &str, reason: &str| {
            format!(
                r#"{{"schema":1,"note":"n","overrides":[{{"hero_id":1,"hero_name":"npc_dota_hero_test","label":"T","remove":{remove},"add":{add},"reason":"{reason}","patch":"7.42","date":"2026-11-22"}}]}}"#
            )
        };
        assert!(parse_position_overrides(&row("[1]", "[4]", "r")).is_some());
        assert!(parse_position_overrides(&row("[0]", "[4]", "r")).is_none());
        assert!(parse_position_overrides(&row("[1]", "[6]", "r")).is_none());
        assert!(parse_position_overrides(&row("[4]", "[4]", "r")).is_none());
        assert!(parse_position_overrides(&row("[]", "[]", "r")).is_none());
        assert!(parse_position_overrides(&row("[1]", "[4]", " ")).is_none());
        assert!(parse_position_overrides(r#"{"schema":2,"note":"n","overrides":[]}"#).is_none());
        assert!(parse_position_overrides("not json").is_none());
    }

    #[test]
    fn overrides_apply_after_rule_b_and_only_to_matching_rows() {
        // Alchemist: rule B puts a Support-tagged safe-lane hero in pos 5.
        // Pudge: rule B puts an untagged safe+off hero in pos 1 and 3.
        // Treant: below the 200-game floor, so rule B leaves it out.
        // id 105 (Techies) carries a different internal name than the override expects.
        let mut heroes = HeroStatsTable::new();
        heroes.insert(
            73,
            named_hero(73, "npc_dota_hero_alchemist", &["Carry", "Support"]),
        );
        heroes.insert(14, named_hero(14, "npc_dota_hero_pudge", &["Disabler"]));
        heroes.insert(83, named_hero(83, "npc_dota_hero_treant", &["Support"]));
        heroes.insert(105, named_hero(105, "npc_dota_hero_renamed", &[]));
        let safe = BTreeMap::from([
            (73, counts(500)),
            (14, counts(300)),
            (83, counts(90)),
            (105, counts(400)),
        ]);
        let mid = BTreeMap::new();
        let off = BTreeMap::from([(14, counts(300)), (83, counts(80))]);
        let groups = build_position_groups(&heroes, [&safe, &mid, &off]).unwrap();

        assert_eq!(
            groups.groups[0],
            vec![73, 105],
            "pos 1: Alchemist in, Pudge out"
        );
        assert_eq!(groups.groups[2], vec![14], "rule B pos 3 for Pudge is kept");
        assert_eq!(
            groups.groups[3],
            vec![14, 83],
            "pos 4: Pudge and Treant added"
        );
        assert_eq!(
            groups.groups[4],
            vec![14, 83],
            "pos 5: Alchemist out, Pudge and Treant in"
        );
        assert!(
            groups.groups[0].contains(&105),
            "a name mismatch skips the Techies row and keeps rule B"
        );
    }

    #[test]
    fn override_application_is_idempotent_and_sorted() {
        let mut heroes = HeroStatsTable::new();
        heroes.insert(83, named_hero(83, "npc_dota_hero_treant", &["Support"]));
        let mut groups = PositionGroups::default();
        groups.groups[3] = vec![83];
        let rows = parse_position_overrides(super::POSITION_OVERRIDES_JSON).unwrap();
        super::apply_overrides(&mut groups, &heroes, &rows);
        super::apply_overrides(&mut groups, &heroes, &rows);
        assert_eq!(groups.groups[3], vec![83]);
        assert_eq!(groups.groups[4], vec![83]);
    }

    #[test]
    #[ignore = "set KOHBOR_HERO_STATS_FILE and KOHBOR_LANE_SAFE_FILE, KOHBOR_LANE_MID_FILE, KOHBOR_LANE_OFF_FILE; then run with --ignored --nocapture"]
    fn print_five_position_groups_from_provider_files() {
        let get = |name: &str| std::env::var_os(name).expect("set the provider response file path");
        super::print_position_groups_from_files(
            std::path::Path::new(&get("KOHBOR_HERO_STATS_FILE")),
            std::path::Path::new(&get("KOHBOR_LANE_SAFE_FILE")),
            std::path::Path::new(&get("KOHBOR_LANE_MID_FILE")),
            std::path::Path::new(&get("KOHBOR_LANE_OFF_FILE")),
        )
        .unwrap();
    }
}
