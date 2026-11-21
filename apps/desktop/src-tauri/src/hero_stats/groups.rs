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
    for group in &mut result.groups {
        group.sort_unstable();
    }
    Ok(result)
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
    use super::{build_position_groups, PositionGroups};
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
