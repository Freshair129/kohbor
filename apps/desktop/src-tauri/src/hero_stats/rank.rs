use super::groups::PositionGroups;
use super::parse::{Counts, HeroStatsTable};
use super::request::Bracket;
use std::collections::HashSet;

pub(crate) const RANK_MIN_PICKS: u64 = 1_000;
const MAX_SUGGESTION_ROWS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SuggestionRow {
    pub(crate) hero_id: u32,
    pub(crate) name: String,
    pub(crate) rank: Option<u32>,
    pub(crate) games: u64,
    pub(crate) win_rate_tenths: Option<i32>,
    pub(crate) low_data: bool,
    pub(crate) taken: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Suggestions {
    pub(crate) rows: Vec<SuggestionRow>,
    /// Number of sampled heroes in the selected group with at least 1,000 bracket picks.
    pub(crate) ranked_count: u32,
}

#[derive(Clone, Copy)]
struct Candidate {
    hero_id: u32,
    games: u64,
    wins: u64,
}

fn counts_for(hero: &super::parse::HeroStatsRow, bracket: Bracket) -> Option<&Counts> {
    hero.brackets.get(bracket.column())
}

pub(crate) fn bracket_is_empty(heroes: &HeroStatsTable, bracket: Bracket) -> Option<bool> {
    heroes.values().try_fold(true, |empty, hero| {
        counts_for(hero, bracket).map(|counts| empty && counts.games == 0)
    })
}

fn rate_tenths(candidate: Candidate) -> i32 {
    if candidate.games == 0 {
        return 0;
    }
    let rounded = (u128::from(candidate.wins) * 1000 + u128::from(candidate.games / 2))
        / u128::from(candidate.games);
    i32::try_from(rounded).unwrap_or(i32::MAX)
}

/// Meta first (owner, 2026-11-22): the most-picked heroes in the bracket lead, then a higher win
/// rate, then the lower hero ID.
fn compare_meta(left: &Candidate, right: &Candidate) -> std::cmp::Ordering {
    let left_scaled = u128::from(left.wins) * u128::from(right.games);
    let right_scaled = u128::from(right.wins) * u128::from(left.games);
    right
        .games
        .cmp(&left.games)
        .then_with(|| right_scaled.cmp(&left_scaled))
        .then_with(|| left.hero_id.cmp(&right.hero_id))
}

fn add_row(
    rows: &mut Vec<SuggestionRow>,
    candidate: Candidate,
    hero: &super::parse::HeroStatsRow,
    rank: Option<u32>,
    taken: bool,
    low_data: bool,
) {
    rows.push(SuggestionRow {
        hero_id: candidate.hero_id,
        name: hero.localized_name.clone(),
        rank,
        games: candidate.games,
        win_rate_tenths: (!low_data).then(|| rate_tenths(candidate)),
        low_data,
        taken,
    });
}

pub(crate) fn rank_suggestions(
    heroes: &HeroStatsTable,
    groups: &PositionGroups,
    position: Option<u8>,
    bracket: Bracket,
    taken: &HashSet<u32>,
) -> Suggestions {
    let group_ids: Vec<u32> = match position {
        Some(position @ 1..=5) => groups
            .groups
            .get(usize::from(position - 1))
            .cloned()
            .unwrap_or_default(),
        Some(_) => Vec::new(),
        None => heroes.keys().copied().collect(),
    };
    let mut ranked = Vec::new();
    let mut low_data = Vec::new();
    for hero_id in group_ids {
        let Some(hero) = heroes.get(&hero_id) else {
            continue;
        };
        let Some(stats) = counts_for(hero, bracket) else {
            continue;
        };
        let candidate = Candidate {
            hero_id,
            games: stats.games,
            wins: stats.wins,
        };
        if candidate.games >= RANK_MIN_PICKS {
            ranked.push(candidate);
        } else {
            low_data.push(candidate);
        }
    }
    ranked.sort_by(compare_meta);
    low_data.sort_by(|left, right| {
        right
            .games
            .cmp(&left.games)
            .then_with(|| left.hero_id.cmp(&right.hero_id))
    });

    let ranked_count = u32::try_from(ranked.len()).unwrap_or(u32::MAX);
    let mut rows = Vec::new();
    let mut untaken_rank = 0_u32;
    for candidate in ranked {
        if untaken_rank >= 8 || rows.len() >= MAX_SUGGESTION_ROWS {
            break;
        }
        let is_taken = taken.contains(&candidate.hero_id);
        if !is_taken {
            untaken_rank += 1;
        }
        if let Some(hero) = heroes.get(&candidate.hero_id) {
            add_row(
                &mut rows,
                candidate,
                hero,
                (!is_taken).then_some(untaken_rank),
                is_taken,
                false,
            );
        }
        if untaken_rank >= 8 || rows.len() >= MAX_SUGGESTION_ROWS {
            break;
        }
    }
    if untaken_rank < 8 {
        for candidate in low_data {
            if rows.len() >= 8 || rows.len() >= MAX_SUGGESTION_ROWS {
                break;
            }
            let is_taken = taken.contains(&candidate.hero_id);
            if let Some(hero) = heroes.get(&candidate.hero_id) {
                add_row(&mut rows, candidate, hero, None, is_taken, true);
            }
        }
    }
    Suggestions { rows, ranked_count }
}

#[cfg(test)]
mod tests {
    use super::{bracket_is_empty, rank_suggestions};
    use crate::hero_stats::groups::PositionGroups;
    use crate::hero_stats::parse::{Counts, HeroStatsRow, HeroStatsTable};
    use crate::hero_stats::request::Bracket;
    use std::collections::{BTreeMap, HashSet};

    fn hero(id: u32, games: u64, wins: u64) -> HeroStatsRow {
        let brackets = (1..=8)
            .map(|bracket| (bracket.to_string(), Counts { games, wins }))
            .chain(std::iter::once(("pub".to_owned(), Counts { games, wins })))
            .collect::<BTreeMap<_, _>>();
        HeroStatsRow {
            id,
            name: format!("npc_dota_hero_test_{id}"),
            localized_name: format!("Hero {id}"),
            roles: vec!["Carry".to_owned()],
            brackets,
        }
    }

    fn all_group(heroes: &HeroStatsTable) -> PositionGroups {
        let ids = heroes.keys().copied().collect::<Vec<_>>();
        PositionGroups {
            groups: [ids.clone(), ids.clone(), ids.clone(), ids.clone(), ids],
        }
    }

    #[test]
    fn ranking_uses_the_exact_one_thousand_pick_threshold_and_fills_low_data_without_rates() {
        let heroes = BTreeMap::from([(1, hero(1, 999, 700)), (2, hero(2, 1000, 600))]);
        let suggestions = rank_suggestions(
            &heroes,
            &all_group(&heroes),
            None,
            Bracket::Public,
            &HashSet::new(),
        );
        assert_eq!(suggestions.ranked_count, 1);
        assert_eq!(suggestions.rows[0].hero_id, 2);
        assert_eq!(suggestions.rows[0].win_rate_tenths, Some(600));
        assert!(suggestions.rows[1].low_data);
        assert_eq!(suggestions.rows[1].games, 999);
        assert_eq!(suggestions.rows[1].win_rate_tenths, None);
    }

    #[test]
    fn ranking_is_meta_first_by_picks_then_win_rate_then_hero_id() {
        // 5 has the highest rate but the fewest picks; 4 is the most picked.
        let heroes = BTreeMap::from([
            (5, hero(5, 1500, 900)),
            (4, hero(4, 9000, 4300)),
            (3, hero(3, 3000, 1400)),
            (2, hero(2, 3000, 1500)),
            (1, hero(1, 3000, 1500)),
        ]);
        let suggestions = rank_suggestions(
            &heroes,
            &all_group(&heroes),
            None,
            Bracket::Public,
            &HashSet::new(),
        );
        assert_eq!(
            suggestions
                .rows
                .iter()
                .map(|row| row.hero_id)
                .collect::<Vec<_>>(),
            [4, 1, 2, 3, 5]
        );
    }

    #[test]
    fn ranking_breaks_pick_ties_by_rate_then_hero_id() {
        let heroes = BTreeMap::from([
            (3, hero(3, 3000, 1400)),
            (2, hero(2, 3000, 1500)),
            (1, hero(1, 3000, 1500)),
        ]);
        let suggestions = rank_suggestions(
            &heroes,
            &all_group(&heroes),
            None,
            Bracket::Public,
            &HashSet::new(),
        );
        assert_eq!(
            suggestions
                .rows
                .iter()
                .map(|row| row.hero_id)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            suggestions
                .rows
                .iter()
                .map(|row| row.rank.unwrap())
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }

    #[test]
    fn bracket_empty_is_distinct_and_position_filter_uses_one_estimated_group() {
        let mut empty_hero = hero(1, 1000, 500);
        empty_hero
            .brackets
            .insert("8".to_owned(), Counts { games: 0, wins: 0 });
        let empty = BTreeMap::from([(1, empty_hero)]);
        assert_eq!(bracket_is_empty(&empty, Bracket::Immortal), Some(true));
        assert_eq!(bracket_is_empty(&empty, Bracket::Public), Some(false));
        let heroes = BTreeMap::from([(1, hero(1, 2000, 1200)), (2, hero(2, 2000, 1000))]);
        let groups = PositionGroups {
            groups: [vec![1], vec![2], vec![], vec![], vec![]],
        };
        assert_eq!(
            rank_suggestions(&heroes, &groups, Some(1), Bracket::Public, &HashSet::new()).rows[0]
                .hero_id,
            1
        );
        assert_eq!(
            rank_suggestions(&heroes, &groups, Some(2), Bracket::Public, &HashSet::new()).rows[0]
                .hero_id,
            2
        );
    }

    #[test]
    fn low_data_fill_reaches_eight_rows_and_taken_rows_are_struck_without_ranks() {
        let heroes = (1..=10)
            .map(|id| {
                (
                    id,
                    hero(id, if id <= 2 { 1200 } else { 900 - u64::from(id) }, 500),
                )
            })
            .collect::<HeroStatsTable>();
        let taken = HashSet::from([1, 3, 4]);
        let suggestions =
            rank_suggestions(&heroes, &all_group(&heroes), None, Bracket::Public, &taken);
        assert!(suggestions.rows.len() >= 8);
        let taken_row = suggestions
            .rows
            .iter()
            .find(|row| row.hero_id == 1)
            .unwrap();
        assert!(taken_row.taken);
        assert_eq!(taken_row.rank, None);
        assert_eq!(taken_row.win_rate_tenths, Some(417));
        let low_taken = suggestions
            .rows
            .iter()
            .find(|row| row.hero_id == 3)
            .unwrap();
        assert!(low_taken.taken && low_taken.low_data);
        assert_eq!(low_taken.rank, None);
        assert_eq!(low_taken.win_rate_tenths, None);
        assert_eq!(suggestions.ranked_count, 2);
    }

    #[test]
    fn rows_never_exceed_sixteen_when_taken_rows_precede_untaken_rows() {
        let heroes = (1..=18)
            .map(|id| (id, hero(id, 2_000, 1_000)))
            .collect::<HeroStatsTable>();
        let taken = (1..=10).collect::<HashSet<_>>();
        let suggestions =
            rank_suggestions(&heroes, &all_group(&heroes), None, Bracket::Public, &taken);
        assert_eq!(suggestions.rows.len(), 16);
        assert_eq!(suggestions.rows.iter().filter(|row| row.taken).count(), 10);
        assert_eq!(suggestions.rows.iter().filter(|row| !row.taken).count(), 6);
    }

    #[test]
    fn missing_bracket_counts_are_invalid_and_never_indexed() {
        let mut malformed = hero(1, 2_000, 1_000);
        malformed.brackets.remove("pub");
        let heroes = BTreeMap::from([(1, malformed)]);
        assert_eq!(bracket_is_empty(&heroes, Bracket::Public), None);
        let suggestions = rank_suggestions(
            &heroes,
            &all_group(&heroes),
            None,
            Bracket::Public,
            &HashSet::new(),
        );
        assert!(suggestions.rows.is_empty());
        assert_eq!(suggestions.ranked_count, 0);
    }
}
