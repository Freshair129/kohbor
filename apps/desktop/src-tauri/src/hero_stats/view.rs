use super::cache::{freshness, Freshness, StatsCache};
use super::counters::{
    counter_suggestions, counter_table, CounterInput, CounterRow, CounterState, Counters,
    IdentifiedSlot, Side,
};
use super::groups::PositionGroups;
use super::parse::{Counts, DurationTable, HeroStatsRow};
use super::prefs::DraftStatsPrefs;
use super::rank::{bracket_is_empty, rank_suggestions, SuggestionRow};
use super::request::{Bracket, LaneRole};
use serde::Serialize;
use std::collections::HashSet;

pub(crate) const GSI_STALE_HIDE_MS: u64 = 10_000;
pub(crate) const DURATION_MIN_GAMES: u64 = 300;
#[cfg(test)]
pub(crate) const LENGTH_SENTENCE_MIN_DELTA_TENTHS: i32 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DraftPhase {
    HeroSelection,
    StrategyTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    Hidden,
    ConsentChip,
    Suggestions,
    PickCard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HiddenReason {
    UnsupportedOutput,
    BarOff,
    NoDraft,
    ListBuildDisabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DataState {
    ConsentOff,
    Loading,
    Live,
    Stale,
    FetchFailed,
    RateLimited,
    ProviderError,
    BracketEmpty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Failure {
    Network,
    Timeout,
    HttpStatus,
    InvalidBody,
    TooLarge,
    RateLimited,
    CacheDeleteFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PickCardState {
    Live,
    Stale,
    Loading,
    UnknownHero,
    NotEnoughMatches,
    FetchFailed,
    RateLimited,
    ProviderError,
    BracketEmpty,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct OverlayView {
    pub(crate) visible: bool,
    pub(crate) hidden_reason: Option<HiddenReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DraftColumnView {
    pub(crate) schema: u8,
    pub(crate) mode: Mode,
    pub(crate) overlay: OverlayView,
    pub(crate) phase: Option<DraftPhase>,
    pub(crate) placement_verified: bool,
    pub(crate) data_state: DataState,
    pub(crate) context: ViewContext,
    pub(crate) source: ViewSource,
    pub(crate) suggestions: Option<SuggestionsView>,
    pub(crate) pick_card: Option<PickCardView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ViewContext {
    pub(crate) position: Option<u8>,
    pub(crate) bracket: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ViewSource {
    pub(crate) provider: &'static str,
    pub(crate) window: Option<String>,
    pub(crate) fetched_at_ms: Option<u64>,
    pub(crate) failed_at_ms: Option<u64>,
    pub(crate) failure: Option<Failure>,
    pub(crate) http_status: Option<u16>,
    pub(crate) retry_after_ms: Option<u64>,
    pub(crate) table_requests_24h: u32,
    pub(crate) table_request_budget: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct SuggestionsView {
    pub(crate) rows: Vec<SuggestionRowView>,
    pub(crate) ranked_count: u32,
    pub(crate) draft_bar: DraftBarView,
    pub(crate) counters: CountersView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CounterStateView {
    BuildDisabled,
    DraftBarUnavailable,
    SidesUnknown,
    NoEnemy,
    Ready,
}

impl From<CounterState> for CounterStateView {
    fn from(state: CounterState) -> Self {
        match state {
            CounterState::BuildDisabled => Self::BuildDisabled,
            CounterState::DraftBarUnavailable => Self::DraftBarUnavailable,
            CounterState::SidesUnknown => Self::SidesUnknown,
            CounterState::NoEnemy => Self::NoEnemy,
            CounterState::Ready => Self::Ready,
        }
    }
}

/// Counter-pick suggestions from the reviewed static table: a reason, never a statistic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct CountersView {
    pub(crate) state: CounterStateView,
    pub(crate) source: &'static str,
    pub(crate) enemies: Vec<String>,
    pub(crate) enemies_identified: u8,
    pub(crate) position_filtered: bool,
    pub(crate) rows: Vec<CounterRowView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct CounterRowView {
    pub(crate) hero_id: u32,
    pub(crate) name: String,
    pub(crate) against: Vec<String>,
    pub(crate) reason: String,
    /// Internal hero key for the local game portrait (`npc_dota_hero_*`).
    pub(crate) hero_key: Option<String>,
    /// Bracket games and win rate, as in the meta list; `None` below 1,000 games.
    pub(crate) games: Option<u64>,
    pub(crate) win_rate_tenths: Option<i32>,
    pub(crate) lane: Option<LaneView>,
}

impl From<CounterRow> for CounterRowView {
    fn from(row: CounterRow) -> Self {
        Self {
            hero_id: row.hero_id,
            name: row.name,
            against: row.against,
            reason: row.reason,
            hero_key: None,
            games: None,
            win_rate_tenths: None,
            lane: None,
        }
    }
}

/// The hero's numbers in the lane the selected position plays (draft card v3, owner Variant B,
/// 2026-11-23). From the cached OpenDota lane-role tables: all brackets, three lanes only, so
/// positions 1+5 share the safe lane and 3+4 the off lane (`shared_with`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LaneView {
    pub(crate) position: u8,
    pub(crate) lane: &'static str,
    pub(crate) shared_with: Option<u8>,
    pub(crate) games: u64,
    /// This hero's games in the lane / its games in all three lanes, in tenths of a percent.
    pub(crate) share_tenths: Option<i32>,
    /// Win rate in the lane, in tenths; `None` below `LANE_MIN_GAMES` lane games.
    pub(crate) win_rate_tenths: Option<i32>,
}

/// Lane games below this show no lane win rate (same floor as position groups, rule B).
pub(crate) const LANE_MIN_GAMES: u64 = 100;

impl From<Counters> for CountersView {
    fn from(counters: Counters) -> Self {
        Self {
            state: counters.state.into(),
            source: "kohbor_reviewed_table",
            enemies: counters.enemies,
            enemies_identified: counters.enemies_identified,
            position_filtered: counters.position_filtered,
            rows: counters.rows.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct SuggestionRowView {
    pub(crate) hero_id: u32,
    pub(crate) name: String,
    pub(crate) rank: Option<u32>,
    pub(crate) games: u64,
    pub(crate) win_rate_tenths: Option<i32>,
    pub(crate) low_data: bool,
    pub(crate) taken: bool,
    /// Internal hero key for the local game portrait (`npc_dota_hero_*`).
    pub(crate) hero_key: Option<String>,
    pub(crate) lane: Option<LaneView>,
}

impl From<SuggestionRow> for SuggestionRowView {
    fn from(row: SuggestionRow) -> Self {
        Self {
            hero_id: row.hero_id,
            name: row.name,
            rank: row.rank,
            games: row.games,
            win_rate_tenths: row.win_rate_tenths,
            low_data: row.low_data,
            taken: row.taken,
            hero_key: None,
            lane: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DraftBarView {
    pub(crate) available: bool,
    pub(crate) identified: u8,
    pub(crate) slots: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PickCardView {
    pub(crate) hero_id: Option<u32>,
    pub(crate) name: Option<String>,
    pub(crate) state: PickCardState,
    pub(crate) games: Option<u64>,
    pub(crate) win_rate_tenths: Option<i32>,
    pub(crate) form_tenths: Option<i32>,
    pub(crate) durations: DurationsView,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DurationsView {
    pub(crate) state: PickCardState,
    pub(crate) fetched_at_ms: Option<u64>,
    pub(crate) cells: [DurationCellView; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DurationCellView {
    pub(crate) bin: &'static str,
    pub(crate) games: Option<u64>,
    pub(crate) win_rate_tenths: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DraftBarInput {
    pub(crate) available: bool,
    pub(crate) identified: u8,
}

pub(crate) struct ViewInput<'a> {
    pub(crate) now_ms: u64,
    pub(crate) prefs: &'a DraftStatsPrefs,
    pub(crate) phase: Option<DraftPhase>,
    pub(crate) admitted_session: bool,
    pub(crate) ownership_verified: bool,
    pub(crate) gsi_age_ms: Option<u64>,
    pub(crate) own_hero_key: Option<&'a str>,
    pub(crate) output_supported: bool,
    pub(crate) consent_chip_visible: bool,
    pub(crate) data_state: DataState,
    pub(crate) failed_at_ms: Option<u64>,
    pub(crate) failure: Option<Failure>,
    pub(crate) http_status: Option<u16>,
    pub(crate) retry_after_ms: Option<u64>,
    pub(crate) table_requests_24h: u32,
    pub(crate) cache: &'a StatsCache,
    pub(crate) groups: Option<&'a PositionGroups>,
    pub(crate) draft_bar: DraftBarInput,
    pub(crate) taken_hero_ids: &'a HashSet<u32>,
    pub(crate) own_side: Option<Side>,
    /// Identified 5b slots with their sides; memory only.
    pub(crate) counter_slots: &'a [IdentifiedSlot],
}

fn hidden_view(
    input: &ViewInput<'_>,
    phase: Option<DraftPhase>,
    data_state: DataState,
    source: ViewSource,
    hidden_reason: HiddenReason,
) -> DraftColumnView {
    DraftColumnView {
        schema: 1,
        mode: Mode::Hidden,
        overlay: OverlayView {
            visible: false,
            hidden_reason: Some(hidden_reason),
        },
        phase,
        placement_verified: false,
        data_state,
        context: ViewContext {
            position: input.prefs.position,
            bracket: input.prefs.bracket,
        },
        source,
        suggestions: None,
        pick_card: None,
    }
}

fn source_view(
    fetched_at_ms: Option<u64>,
    failed_at_ms: Option<u64>,
    failure: Option<Failure>,
    http_status: Option<u16>,
    retry_after_ms: Option<u64>,
    table_requests_24h: u32,
) -> ViewSource {
    ViewSource {
        provider: "OpenDota",
        window: None,
        fetched_at_ms,
        failed_at_ms,
        failure,
        http_status,
        retry_after_ms,
        table_requests_24h,
        table_request_budget: 4,
    }
}

/// Lane played by a position and the other position sharing it (three lane tables only).
fn lane_for_position(position: u8) -> Option<(LaneRole, &'static str, Option<u8>)> {
    match position {
        1 => Some((LaneRole::Safe, "safe", Some(5))),
        2 => Some((LaneRole::Mid, "mid", None)),
        3 => Some((LaneRole::Off, "off", Some(4))),
        4 => Some((LaneRole::Off, "off", Some(3))),
        5 => Some((LaneRole::Safe, "safe", Some(1))),
        _ => None,
    }
}

/// The hero's numbers in the selected position's lane, or `None` without a position or tables.
pub(crate) fn lane_view(
    cache: &StatsCache,
    position: Option<u8>,
    hero_id: u32,
) -> Option<LaneView> {
    let (role, lane, shared_with) = lane_for_position(position?)?;
    let counts = |role: LaneRole| -> Option<Counts> {
        cache.lane_roles[role.index()]
            .as_ref()
            .map(|entry| entry.value.get(&hero_id).cloned().unwrap_or_default())
    };
    let here = counts(role)?;
    let all = LaneRole::ALL
        .iter()
        .map(|role| counts(*role).map(|c| c.games))
        .sum::<Option<u64>>()?;
    let share_tenths = (all > 0).then(|| {
        i32::try_from((u128::from(here.games) * 1000 + u128::from(all / 2)) / u128::from(all))
            .unwrap_or(1000)
    });
    Some(LaneView {
        position: position?,
        lane,
        shared_with,
        games: here.games,
        share_tenths,
        win_rate_tenths: (here.games >= LANE_MIN_GAMES).then(|| rate_tenths(&here)),
    })
}

fn count_for(hero: &HeroStatsRow, bracket: Bracket) -> Option<Counts> {
    hero.brackets.get(bracket.column()).cloned()
}

fn cache_state(input: &ViewInput<'_>) -> DataState {
    if !input.prefs.hero_stats_consent {
        return DataState::ConsentOff;
    }
    if input.failure == Some(Failure::RateLimited) {
        return if input.cache.public_tables_usable(input.now_ms) {
            DataState::Stale
        } else {
            DataState::RateLimited
        };
    }
    if input.failure.is_some() {
        return if input.cache.public_tables_usable(input.now_ms) {
            DataState::Stale
        } else if input.failure == Some(Failure::InvalidBody)
            || input.failure == Some(Failure::TooLarge)
        {
            DataState::ProviderError
        } else {
            DataState::FetchFailed
        };
    }
    if !input.cache.public_tables_usable(input.now_ms) {
        return input.data_state;
    }
    if input.cache.public_tables_fresh(input.now_ms) {
        DataState::Live
    } else {
        DataState::Stale
    }
}

fn durations_view(input: &ViewInput<'_>, hero_id: u32, fallback: PickCardState) -> DurationsView {
    let Some(entry) = input.cache.duration_for(hero_id) else {
        return DurationsView {
            state: fallback,
            fetched_at_ms: None,
            cells: empty_duration_cells(),
        };
    };
    let freshness_state = freshness(entry.fetched_at_ms, input.now_ms);
    if freshness_state == Freshness::Expired {
        return DurationsView {
            state: fallback,
            fetched_at_ms: None,
            cells: empty_duration_cells(),
        };
    }
    let Some(cells) = duration_cells(&entry.value) else {
        return DurationsView {
            state: PickCardState::ProviderError,
            fetched_at_ms: None,
            cells: empty_duration_cells(),
        };
    };
    DurationsView {
        state: if freshness_state == Freshness::Fresh {
            PickCardState::Live
        } else {
            PickCardState::Stale
        },
        fetched_at_ms: Some(entry.fetched_at_ms),
        cells,
    }
}

fn empty_duration_cells() -> [DurationCellView; 3] {
    [
        DurationCellView {
            bin: "short",
            games: None,
            win_rate_tenths: None,
        },
        DurationCellView {
            bin: "mid",
            games: None,
            win_rate_tenths: None,
        },
        DurationCellView {
            bin: "long",
            games: None,
            win_rate_tenths: None,
        },
    ]
}

fn duration_cells(table: &DurationTable) -> Option<[DurationCellView; 3]> {
    let mut games = [0_u128; 3];
    let mut wins = [0_u128; 3];
    for (bin, row) in table {
        let index = if *bin < 1800 {
            0
        } else if *bin <= 2700 {
            1
        } else {
            2
        };
        let (Some(game_total), Some(win_total)) = (games.get_mut(index), wins.get_mut(index))
        else {
            return None;
        };
        *game_total = game_total.checked_add(u128::from(row.games))?;
        *win_total = win_total.checked_add(u128::from(row.wins))?;
    }
    let cell = |index: usize| -> Option<DurationCellView> {
        let game_count = u64::try_from(*games.get(index)?).ok()?;
        let win_count = u64::try_from(*wins.get(index)?).ok()?;
        if win_count > game_count {
            return None;
        }
        Some(DurationCellView {
            bin: ["short", "mid", "long"].get(index)?,
            games: Some(game_count),
            win_rate_tenths: (game_count >= DURATION_MIN_GAMES).then(|| {
                rate_tenths(&Counts {
                    games: game_count,
                    wins: win_count,
                })
            }),
        })
    };
    Some([cell(0)?, cell(1)?, cell(2)?])
}

fn rate_tenths(counts: &Counts) -> i32 {
    if counts.games == 0 {
        return 0;
    }
    i32::try_from(
        (u128::from(counts.wins) * 1000 + u128::from(counts.games / 2)) / u128::from(counts.games),
    )
    .unwrap_or(i32::MAX)
}

#[cfg(test)]
pub(crate) fn has_duration_preference_sentence(cells: &[DurationCellView; 3]) -> bool {
    let Some(rates) = cells
        .iter()
        .map(|cell| cell.win_rate_tenths)
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let Some(minimum) = rates.iter().min().copied() else {
        return false;
    };
    let Some(maximum) = rates.iter().max().copied() else {
        return false;
    };
    maximum - minimum >= LENGTH_SENTENCE_MIN_DELTA_TENTHS
}

fn pick_card(input: &ViewInput<'_>, hero_key: &str, state: DataState) -> PickCardView {
    let Some(hero_stats) = input
        .cache
        .hero_stats
        .as_ref()
        .filter(|_| input.cache.public_tables_usable(input.now_ms))
        .map(|entry| &entry.value)
    else {
        return empty_pick_card(pick_card_state(state));
    };
    let hero = hero_stats.values().find(|hero| hero.name == hero_key);
    let Some(hero) = hero else {
        return empty_pick_card(if state == DataState::Loading {
            PickCardState::Loading
        } else {
            PickCardState::UnknownHero
        });
    };
    let bracket = Bracket::from_setting(input.prefs.bracket);
    let Some(counts) = count_for(hero, bracket) else {
        return PickCardView {
            hero_id: Some(hero.id),
            name: Some(hero.localized_name.clone()),
            ..empty_pick_card(PickCardState::ProviderError)
        };
    };
    let bracket_empty = bracket_is_empty(hero_stats, bracket);
    if bracket_empty == Some(true) {
        let state = PickCardState::BracketEmpty;
        return PickCardView {
            hero_id: Some(hero.id),
            name: Some(hero.localized_name.clone()),
            state,
            games: None,
            win_rate_tenths: None,
            form_tenths: None,
            durations: DurationsView {
                state,
                fetched_at_ms: None,
                cells: empty_duration_cells(),
            },
        };
    }
    if bracket_empty.is_none() {
        return PickCardView {
            hero_id: Some(hero.id),
            name: Some(hero.localized_name.clone()),
            ..empty_pick_card(PickCardState::ProviderError)
        };
    }
    let data_state = cache_state(input);
    let state = if counts.games < super::rank::RANK_MIN_PICKS {
        PickCardState::NotEnoughMatches
    } else {
        match data_state {
            DataState::Live => PickCardState::Live,
            DataState::Stale => PickCardState::Stale,
            DataState::Loading => PickCardState::Loading,
            DataState::RateLimited => PickCardState::RateLimited,
            DataState::ProviderError => PickCardState::ProviderError,
            DataState::FetchFailed => PickCardState::FetchFailed,
            DataState::ConsentOff => PickCardState::FetchFailed,
            DataState::BracketEmpty => PickCardState::BracketEmpty,
        }
    };
    let rate = (counts.games >= super::rank::RANK_MIN_PICKS).then(|| rate_tenths(&counts));
    let form = rate.map(|rate| rate - 500);
    let durations = durations_view(input, hero.id, state);
    PickCardView {
        hero_id: Some(hero.id),
        name: Some(hero.localized_name.clone()),
        state,
        games: Some(counts.games),
        win_rate_tenths: rate,
        form_tenths: form,
        durations,
    }
}

fn pick_card_state(state: DataState) -> PickCardState {
    match state {
        DataState::Loading => PickCardState::Loading,
        DataState::RateLimited => PickCardState::RateLimited,
        DataState::ProviderError => PickCardState::ProviderError,
        DataState::FetchFailed | DataState::ConsentOff => PickCardState::FetchFailed,
        DataState::BracketEmpty => PickCardState::BracketEmpty,
        DataState::Live | DataState::Stale => PickCardState::Loading,
    }
}

fn empty_pick_card(state: PickCardState) -> PickCardView {
    PickCardView {
        hero_id: None,
        name: None,
        state,
        games: None,
        win_rate_tenths: None,
        form_tenths: None,
        durations: DurationsView {
            state,
            fetched_at_ms: None,
            cells: empty_duration_cells(),
        },
    }
}

/// Shared-view shape deliberately follows DRAFT-COLUMN §10. The length-preference sentence is
/// not a field in that shared contract; the renderer derives its wording from these three cells.
pub(crate) fn build_view(input: ViewInput<'_>) -> DraftColumnView {
    let data_state = cache_state(&input);
    let fetched_at_ms = input
        .cache
        .public_tables_usable(input.now_ms)
        .then(|| input.cache.public_tables_fetched_at())
        .flatten();
    let valid_phase = input.phase.filter(|_| {
        input.admitted_session
            && input.ownership_verified
            && input.gsi_age_ms.is_some_and(|age| age <= GSI_STALE_HIDE_MS)
    });
    let source = source_view(
        fetched_at_ms,
        input.failed_at_ms,
        input.failure,
        input.http_status,
        input.retry_after_ms,
        input.table_requests_24h,
    );
    let Some(phase) = valid_phase else {
        return hidden_view(&input, None, data_state, source, HiddenReason::NoDraft);
    };
    let bracket = Bracket::from_setting(input.prefs.bracket);
    let tables = input.cache.public_tables_usable(input.now_ms);
    let bracket_state = if tables {
        input
            .cache
            .hero_stats
            .as_ref()
            .map(|entry| bracket_is_empty(&entry.value, bracket))
    } else {
        None
    };
    let bracket_empty = bracket_state == Some(Some(true));
    let data_state = match bracket_state {
        Some(Some(true)) => DataState::BracketEmpty,
        Some(None) => DataState::ProviderError,
        _ => data_state,
    };
    let own_hero = input.own_hero_key.filter(|hero_key| !hero_key.is_empty());
    let mode = if !input.prefs.hero_stats_consent {
        if input.consent_chip_visible {
            Mode::ConsentChip
        } else {
            Mode::Hidden
        }
    } else if own_hero.is_some() {
        Mode::PickCard
    } else {
        Mode::Suggestions
    };
    let suggestions = (mode == Mode::Suggestions).then(|| {
        let hero_table = input.cache.hero_stats.as_ref().map(|entry| &entry.value);
        let ranking = if tables && !bracket_empty {
            match (hero_table, input.groups) {
                (Some(heroes), Some(groups)) => rank_suggestions(
                    heroes,
                    groups,
                    input.prefs.position,
                    bracket,
                    input.taken_hero_ids,
                ),
                _ => Default::default(),
            }
        } else {
            Default::default()
        };
        let group_ids = match (input.prefs.position, input.groups) {
            (Some(position @ 1..=5), Some(groups)) => groups
                .groups
                .get(usize::from(position - 1))
                .map(Vec::as_slice),
            _ => None,
        };
        let counters = counter_suggestions(
            &CounterInput {
                build_enabled: super::COUNTERS_BUILD_ENABLED,
                draft_bar_available: input.draft_bar.available,
                own_side: input.own_side,
                slots: input.counter_slots,
                group_ids,
                heroes: hero_table,
            },
            counter_table(),
        );
        let position = input.prefs.position;
        let lanes_usable = tables;
        let hero_key = |id: u32| {
            hero_table
                .and_then(|table| table.get(&id))
                .map(|hero| hero.name.clone())
        };
        let rows = ranking
            .rows
            .into_iter()
            .map(|row| {
                let mut view = SuggestionRowView::from(row);
                view.hero_key = hero_key(view.hero_id);
                view.lane = lanes_usable
                    .then(|| lane_view(input.cache, position, view.hero_id))
                    .flatten();
                view
            })
            .collect();
        let mut counters: CountersView = counters.into();
        for row in &mut counters.rows {
            row.hero_key = hero_key(row.hero_id);
            let bracket_counts = hero_table
                .and_then(|table| table.get(&row.hero_id))
                .filter(|_| tables && !bracket_empty)
                .and_then(|hero| count_for(hero, bracket));
            row.games = bracket_counts.as_ref().map(|c| c.games);
            row.win_rate_tenths = bracket_counts
                .as_ref()
                .filter(|c| c.games >= 1000)
                .map(rate_tenths);
            row.lane = lanes_usable
                .then(|| lane_view(input.cache, position, row.hero_id))
                .flatten();
        }
        SuggestionsView {
            rows,
            ranked_count: ranking.ranked_count,
            counters,
            draft_bar: DraftBarView {
                available: input.draft_bar.available,
                identified: input.draft_bar.identified.min(10),
                slots: 10,
            },
        }
    });
    let pick_card = (mode == Mode::PickCard && input.prefs.hero_stats_consent).then(|| {
        pick_card(
            &input,
            own_hero.expect("pick-card mode has an own hero"),
            data_state,
        )
    });

    let mut hidden_reason = None;
    let mut overlay_visible = match mode {
        Mode::Hidden => false,
        Mode::ConsentChip => true,
        Mode::Suggestions => input.prefs.hero_stats_consent,
        Mode::PickCard => pick_card.is_some(),
    };
    if !input.output_supported {
        hidden_reason = Some(HiddenReason::UnsupportedOutput);
        overlay_visible = false;
    } else if !input.prefs.draft_bar_enabled {
        hidden_reason = Some(HiddenReason::BarOff);
        overlay_visible = false;
    } else if list_build_blocks(mode, super::SUGGESTIONS_LIST_BUILD_ENABLED) {
        hidden_reason = Some(HiddenReason::ListBuildDisabled);
        overlay_visible = false;
    }
    DraftColumnView {
        schema: 1,
        mode,
        overlay: OverlayView {
            visible: overlay_visible,
            hidden_reason,
        },
        phase: Some(phase),
        placement_verified: input.output_supported && phase == DraftPhase::HeroSelection,
        data_state,
        context: ViewContext {
            position: input.prefs.position,
            bracket: input.prefs.bracket,
        },
        source,
        suggestions,
        pick_card,
    }
}

/// The build-time list switch hides only the overlay's suggestions list; the deck keeps its rows
/// and the pick card is never gated.
fn list_build_blocks(mode: Mode, list_build_enabled: bool) -> bool {
    mode == Mode::Suggestions && !list_build_enabled
}

#[cfg(test)]
mod tests {
    use super::{
        build_view, cache_state, duration_cells, list_build_blocks, DataState, DraftBarInput,
        DraftPhase, Failure, Mode, ViewInput,
    };
    use crate::hero_stats::cache::{CacheEntry, DurationEntry, StatsCache};
    use crate::hero_stats::groups::PositionGroups;
    use crate::hero_stats::parse::{Counts, DurationTable, HeroStatsRow, HeroStatsTable};
    use crate::hero_stats::prefs::DraftStatsPrefs;
    use serde_json::Value;
    use std::collections::{BTreeMap, HashSet};
    use std::sync::OnceLock;

    static GROUPS: OnceLock<PositionGroups> = OnceLock::new();
    static TAKEN: OnceLock<HashSet<u32>> = OnceLock::new();

    fn table() -> HeroStatsTable {
        let brackets = (1..=8)
            .map(|bracket| {
                (
                    bracket.to_string(),
                    Counts {
                        games: 1000,
                        wins: 518,
                    },
                )
            })
            .chain(std::iter::once((
                "pub".to_owned(),
                Counts {
                    games: 1000,
                    wins: 518,
                },
            )))
            .collect::<BTreeMap<_, _>>();
        BTreeMap::from([(
            1,
            HeroStatsRow {
                id: 1,
                name: "npc_dota_hero_test".to_owned(),
                localized_name: "Test Hero".to_owned(),
                roles: vec!["Carry".to_owned()],
                brackets,
            },
        )])
    }

    fn input<'a>(
        prefs: &'a DraftStatsPrefs,
        cache: &'a StatsCache,
        own_hero: Option<&'a str>,
    ) -> ViewInput<'a> {
        ViewInput {
            now_ms: 1_000_000,
            prefs,
            phase: Some(DraftPhase::HeroSelection),
            admitted_session: true,
            ownership_verified: true,
            gsi_age_ms: Some(1000),
            own_hero_key: own_hero,
            output_supported: true,
            consent_chip_visible: true,
            data_state: DataState::Loading,
            failed_at_ms: None,
            failure: None,
            http_status: None,
            retry_after_ms: None,
            table_requests_24h: 0,
            cache,
            groups: Some(GROUPS.get_or_init(|| PositionGroups {
                groups: [vec![1], vec![], vec![], vec![], vec![]],
            })),
            draft_bar: DraftBarInput {
                available: false,
                identified: 0,
            },
            taken_hero_ids: TAKEN.get_or_init(HashSet::new),
            own_side: None,
            counter_slots: &[],
        }
    }

    #[test]
    fn phase_gate_table_covers_exact_screens_session_ownership_and_staleness() {
        let prefs = DraftStatsPrefs {
            hero_stats_consent: true,
            ..Default::default()
        };
        let cache = StatsCache::default();
        let rows = [
            ("no exact draft screen", None, true, true, Some(0), false),
            (
                "no admitted session",
                Some(DraftPhase::HeroSelection),
                false,
                true,
                Some(0),
                false,
            ),
            (
                "ownership unverified (spectating or replay is rejected upstream)",
                Some(DraftPhase::HeroSelection),
                true,
                false,
                Some(0),
                false,
            ),
            (
                "stale beyond ten seconds",
                Some(DraftPhase::HeroSelection),
                true,
                true,
                Some(10_001),
                false,
            ),
            (
                "unknown GSI age",
                Some(DraftPhase::HeroSelection),
                true,
                true,
                None,
                false,
            ),
            (
                "hero selection at ten-second boundary",
                Some(DraftPhase::HeroSelection),
                true,
                true,
                Some(10_000),
                true,
            ),
            (
                "strategy time at ten-second boundary",
                Some(DraftPhase::StrategyTime),
                true,
                true,
                Some(10_000),
                true,
            ),
        ];
        for (name, phase, admitted, owned, age, visible_phase) in rows {
            let mut data = input(&prefs, &cache, None);
            data.phase = phase;
            data.admitted_session = admitted;
            data.ownership_verified = owned;
            data.gsi_age_ms = age;
            let view = build_view(data);
            if visible_phase {
                assert_eq!(view.phase, phase, "{name}");
                assert_ne!(view.mode, Mode::Hidden, "{name}");
            } else {
                assert_eq!(view.mode, Mode::Hidden, "{name}");
            }
        }
    }

    #[test]
    fn view_json_shape_matches_section_ten_with_nullable_numbers_and_no_extra_fields() {
        let prefs = DraftStatsPrefs::default();
        let cache = StatsCache::default();
        let view = build_view(input(&prefs, &cache, None));
        let json = serde_json::to_value(view).unwrap();
        assert_eq!(json["schema"], 1);
        assert_eq!(json["mode"], "consent_chip");
        assert_eq!(json["overlay"]["visible"], true);
        assert_eq!(json["phase"], "hero_selection");
        assert_eq!(json["data_state"], "consent_off");
        assert!(json["source"]["window"].is_null());
        assert_eq!(json["source"]["provider"], "OpenDota");
        assert_eq!(json["source"]["table_request_budget"], 4);
        assert_eq!(json["suggestions"], Value::Null);
        assert_eq!(json["pick_card"], Value::Null);
        let keys = json
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                "context",
                "data_state",
                "mode",
                "overlay",
                "phase",
                "pick_card",
                "placement_verified",
                "schema",
                "source",
                "suggestions"
            ]
        );
    }

    #[test]
    fn consent_off_never_exposes_statistics_and_chip_precedes_known_hero_mode() {
        let prefs = DraftStatsPrefs::default();
        let cache = StatsCache {
            hero_stats: Some(CacheEntry {
                fetched_at_ms: 1_000_000,
                value: table(),
            }),
            ..Default::default()
        };
        let chip = build_view(input(&prefs, &cache, Some("npc_dota_hero_test")));
        assert_eq!(chip.mode, Mode::ConsentChip);
        assert!(chip.overlay.visible);
        assert_eq!(chip.data_state, DataState::ConsentOff);
        assert!(chip.suggestions.is_none());
        assert!(chip.pick_card.is_none());

        let mut hidden_input = input(&prefs, &cache, Some("npc_dota_hero_test"));
        hidden_input.consent_chip_visible = false;
        let hidden = build_view(hidden_input);
        assert_eq!(hidden.mode, Mode::Hidden);
        assert!(!hidden.overlay.visible);
        assert_eq!(hidden.data_state, DataState::ConsentOff);
        assert!(hidden.suggestions.is_none());
        assert!(hidden.pick_card.is_none());
    }

    #[test]
    fn list_disable_hides_only_the_overlay_and_preserves_deck_rows() {
        assert!(list_build_blocks(Mode::Suggestions, false));
        assert!(!list_build_blocks(Mode::Suggestions, true));
        assert!(!list_build_blocks(Mode::PickCard, false));
        assert!(!list_build_blocks(Mode::ConsentChip, false));
        assert!(!list_build_blocks(Mode::Hidden, false));
    }

    #[test]
    fn list_is_enabled_in_this_build_after_owner_option_b() {
        const { assert!(crate::hero_stats::SUGGESTIONS_LIST_BUILD_ENABLED) };
        let prefs = DraftStatsPrefs {
            hero_stats_consent: true,
            ..Default::default()
        };
        let cache = StatsCache {
            hero_stats: Some(CacheEntry {
                fetched_at_ms: 1_000_000,
                value: table(),
            }),
            lane_roles: std::array::from_fn(|_| {
                Some(CacheEntry {
                    fetched_at_ms: 1_000_000,
                    value: BTreeMap::new(),
                })
            }),
            ..Default::default()
        };
        let view = build_view(input(&prefs, &cache, None));
        assert_eq!(view.mode, Mode::Suggestions);
        assert_eq!(view.overlay.hidden_reason, None);
        assert!(view.overlay.visible, "consented suggestions show in game");
        assert!(view.suggestions.is_some());
    }

    #[test]
    fn counters_are_on_in_this_build_and_serialize_with_a_state() {
        const { assert!(crate::hero_stats::COUNTERS_BUILD_ENABLED) };
        let prefs = DraftStatsPrefs {
            hero_stats_consent: true,
            ..Default::default()
        };
        let cache = StatsCache::default();
        let slots = [crate::hero_stats::counters::IdentifiedSlot {
            hero_key: "npc_dota_hero_phantom_lancer".to_owned(),
            side: Some(crate::hero_stats::counters::Side::Dire),
        }];
        let mut view_input = input(&prefs, &cache, None);
        view_input.own_side = Some(crate::hero_stats::counters::Side::Radiant);
        view_input.counter_slots = &slots;
        view_input.draft_bar.available = true;
        view_input.draft_bar.identified = 1;
        let view = build_view(view_input);
        let counters = &view.suggestions.as_ref().unwrap().counters;
        assert_eq!(counters.state, super::CounterStateView::Ready);
        assert_eq!(counters.enemies, vec!["Phantom Lancer"]);
        assert!(counters.rows.iter().any(|row| row.name == "Earthshaker"));
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["suggestions"]["counters"]["state"], "ready");
        assert!(
            !json["suggestions"]["counters"].to_string().contains('%'),
            "counters carry no percentage"
        );
        assert_eq!(
            json["suggestions"]["counters"]["source"],
            "kohbor_reviewed_table"
        );
    }

    /// Card v3 (owner Variant B, 2026-11-23): lane numbers come only from the cached lane tables.
    #[test]
    fn lane_view_reports_share_and_lane_win_rate_with_the_shared_position() {
        let lane = |games: u64, wins: u64| {
            Some(CacheEntry {
                fetched_at_ms: 1_000_000,
                value: BTreeMap::from([(7_u32, Counts { games, wins })]),
            })
        };
        let cache = StatsCache {
            lane_roles: [lane(300, 160), lane(50, 30), lane(650, 330)],
            ..Default::default()
        };
        let off = super::lane_view(&cache, Some(3), 7).unwrap();
        assert_eq!(
            (
                off.lane,
                off.shared_with,
                off.games,
                off.share_tenths,
                off.win_rate_tenths
            ),
            ("off", Some(4), 650, Some(650), Some(508))
        );
        assert_eq!(
            super::lane_view(&cache, Some(4), 7).unwrap().shared_with,
            Some(3)
        );
        let safe = super::lane_view(&cache, Some(5), 7).unwrap();
        assert_eq!(
            (safe.lane, safe.shared_with, safe.share_tenths),
            ("safe", Some(1), Some(300))
        );
        let mid = super::lane_view(&cache, Some(2), 7).unwrap();
        assert_eq!(mid.shared_with, None);
        assert_eq!(
            mid.win_rate_tenths, None,
            "below 100 lane games the win rate is withheld"
        );
        assert_eq!(mid.share_tenths, Some(50));
        assert_eq!(
            super::lane_view(&cache, None, 7),
            None,
            "no position, no lane numbers"
        );
        let unknown = super::lane_view(&cache, Some(3), 999).unwrap();
        assert_eq!((unknown.games, unknown.share_tenths), (0, None));
        assert_eq!(super::lane_view(&StatsCache::default(), Some(3), 7), None);
    }

    #[test]
    fn pick_card_is_not_gated_by_list_build_and_repick_clears_old_values() {
        let prefs = DraftStatsPrefs {
            hero_stats_consent: true,
            ..Default::default()
        };
        let cache = StatsCache {
            hero_stats: Some(CacheEntry {
                fetched_at_ms: 1_000_000,
                value: table(),
            }),
            lane_roles: std::array::from_fn(|_| {
                Some(CacheEntry {
                    fetched_at_ms: 1_000_000,
                    value: BTreeMap::new(),
                })
            }),
            own_durations: vec![DurationEntry {
                hero_id: 1,
                fetched_at_ms: 1_000_000,
                last_used_at_ms: 1_000_000,
                value: DurationTable::new(),
            }],
        };
        let picked = build_view(input(&prefs, &cache, Some("npc_dota_hero_test")));
        assert_eq!(picked.mode, Mode::PickCard);
        assert!(picked.overlay.visible);
        assert_eq!(
            picked.pick_card.as_ref().unwrap().win_rate_tenths,
            Some(518)
        );
        let repicked = build_view(input(&prefs, &cache, Some("npc_dota_hero_unknown")));
        assert_eq!(repicked.pick_card.as_ref().unwrap().hero_id, None);
        assert_eq!(repicked.pick_card.as_ref().unwrap().win_rate_tenths, None);
        assert_eq!(repicked.pick_card.as_ref().unwrap().games, None);
        assert!(repicked.suggestions.is_none());
    }

    #[test]
    fn pick_card_and_duration_thresholds_are_inclusive_at_one_thousand_and_three_hundred() {
        let prefs = DraftStatsPrefs {
            hero_stats_consent: true,
            ..Default::default()
        };
        let mut low_table = table();
        low_table
            .get_mut(&1)
            .unwrap()
            .brackets
            .get_mut("pub")
            .unwrap()
            .games = 999;
        low_table
            .get_mut(&1)
            .unwrap()
            .brackets
            .get_mut("pub")
            .unwrap()
            .wins = 500;
        let mut durations = DurationTable::new();
        durations.insert(
            1799,
            Counts {
                games: 299,
                wins: 150,
            },
        );
        durations.insert(
            1800,
            Counts {
                games: 300,
                wins: 150,
            },
        );
        durations.insert(
            2701,
            Counts {
                games: 300,
                wins: 150,
            },
        );
        let cells = duration_cells(&durations).unwrap();
        assert_eq!(cells[0].games, Some(299));
        assert_eq!(cells[0].win_rate_tenths, None);
        assert_eq!(cells[1].games, Some(300));
        assert_eq!(cells[1].win_rate_tenths, Some(500));
        let cache = StatsCache {
            hero_stats: Some(CacheEntry {
                fetched_at_ms: 1_000_000,
                value: low_table,
            }),
            lane_roles: std::array::from_fn(|_| {
                Some(CacheEntry {
                    fetched_at_ms: 1_000_000,
                    value: BTreeMap::new(),
                })
            }),
            ..Default::default()
        };
        let card = build_view(input(&prefs, &cache, Some("npc_dota_hero_test")))
            .pick_card
            .unwrap();
        assert_eq!(card.state, super::PickCardState::NotEnoughMatches);
        assert_eq!(card.games, Some(999));
        assert_eq!(card.win_rate_tenths, None);
        assert_eq!(card.form_tenths, None);
    }

    #[test]
    fn duration_length_sentence_boundary_is_two_point_nine_versus_three_point_zero() {
        let cells = |short, mid, long| {
            [
                super::DurationCellView {
                    bin: "short",
                    games: Some(300),
                    win_rate_tenths: Some(short),
                },
                super::DurationCellView {
                    bin: "mid",
                    games: Some(300),
                    win_rate_tenths: Some(mid),
                },
                super::DurationCellView {
                    bin: "long",
                    games: Some(300),
                    win_rate_tenths: Some(long),
                },
            ]
        };
        assert!(!super::has_duration_preference_sentence(&cells(
            520, 500, 491
        )));
        assert!(super::has_duration_preference_sentence(&cells(
            520, 500, 490
        )));
    }

    #[test]
    fn cache_state_uses_stale_values_after_rate_limit_and_propagates_failure() {
        let now_ms = 100_000_000;
        let prefs = DraftStatsPrefs {
            hero_stats_consent: true,
            ..Default::default()
        };
        let mut cache = StatsCache {
            hero_stats: Some(CacheEntry {
                fetched_at_ms: now_ms - super::super::cache::FRESH_MS,
                value: table(),
            }),
            lane_roles: std::array::from_fn(|_| {
                Some(CacheEntry {
                    fetched_at_ms: now_ms - super::super::cache::FRESH_MS,
                    value: BTreeMap::new(),
                })
            }),
            ..Default::default()
        };
        let mut data = input(&prefs, &cache, None);
        data.now_ms = now_ms;
        data.failure = Some(Failure::RateLimited);
        data.failed_at_ms = Some(now_ms);
        assert_eq!(cache_state(&data), DataState::Stale);
        cache.hero_stats = None;
        let mut data = input(&prefs, &cache, None);
        data.now_ms = now_ms;
        data.failure = Some(Failure::InvalidBody);
        assert_eq!(cache_state(&data), DataState::ProviderError);
    }
}
