//! In-game column height for Option A (owner choice 2026-11-22: sectioned cards, text >= 12 px).
//! Mirrors `OVERLAY_LAYOUT` / `overlayColumnHeight` in `src/draft/draftColumnModel.ts`; the CSS in
//! `src/draft/draft-overlay.css` gives every block exactly these heights. Change all three together.

use super::view::{CounterStateView, DataState, DraftColumnView, Mode, SuggestionRowView};

pub(crate) const TOP: i32 = 132;
pub(crate) const MAX_BOTTOM: i32 = 860;
pub(crate) const MAX_HEIGHT: u32 = (MAX_BOTTOM - TOP) as u32;

const GAP: u32 = 6;
const BRAND: u32 = 30;
const CARD_BORDER: u32 = 2;
const HEAD: u32 = 50;
const COUNTER_HEAD: u32 = 70;
const META_ROW: u32 = 50;
const COUNTER_ROW: u32 = 62;
const MESSAGE: u32 = 88;
const PICK_BODY: u32 = 178;
const DURATION_BODY: u32 = 140;
const FOOTER: u32 = 84;
const CHIP: u32 = 56;
const OVERLAY_COUNTER_ROWS: usize = 3;
const MAX_META_ROWS: usize = 8;

/// `overlaySuggestionRows`: pickable rows first; a taken row fills a spare slot only when it
/// outranks a drawn pickable row.
fn overlay_meta_rows(rows: &[SuggestionRowView], limit: usize) -> usize {
    let slots = limit.min(MAX_META_ROWS);
    let untaken: Vec<&SuggestionRowView> =
        rows.iter().filter(|row| !row.taken).take(slots).collect();
    let remaining = slots - untaken.len();
    let taken = rows
        .iter()
        .filter(|row| {
            row.taken
                && row.rank.is_some_and(|rank| {
                    untaken
                        .iter()
                        .any(|pickable| pickable.rank.is_some_and(|other| rank < other))
                })
        })
        .take(remaining)
        .count();
    untaken.len() + taken
}

/// Exact content height in px at 1920×1080 scale 1; 0 when the overlay draws nothing.
pub(crate) fn overlay_column_height(view: &DraftColumnView) -> u32 {
    if !view.overlay.visible {
        return 0;
    }
    let height = match view.mode {
        Mode::Hidden => 0,
        Mode::ConsentChip => CHIP,
        Mode::PickCard => {
            BRAND
                + GAP
                + (CARD_BORDER + HEAD + PICK_BODY)
                + GAP
                + (CARD_BORDER + HEAD + DURATION_BODY)
                + GAP
                + FOOTER
        }
        Mode::Suggestions => {
            let Some(suggestions) = view.suggestions.as_ref() else {
                return 0;
            };
            let counters = if suggestions.counters.state == CounterStateView::Ready {
                suggestions.counters.rows.len().min(OVERLAY_COUNTER_ROWS)
            } else {
                0
            };
            let values = matches!(view.data_state, DataState::Live | DataState::Stale)
                && view.source.fetched_at_ms.is_some();
            let rows = if values {
                overlay_meta_rows(&suggestions.rows, MAX_META_ROWS - counters)
            } else {
                0
            };
            let counter_card = if counters > 0 {
                CARD_BORDER + COUNTER_HEAD + counters as u32 * COUNTER_ROW + GAP
            } else {
                0
            };
            let meta_body = if rows > 0 {
                rows as u32 * META_ROW
            } else {
                MESSAGE
            };
            BRAND + GAP + counter_card + (CARD_BORDER + HEAD + meta_body) + GAP + FOOTER
        }
    };
    height.min(MAX_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(rank: Option<u32>, taken: bool) -> SuggestionRowView {
        SuggestionRowView {
            hero_id: rank.unwrap_or(99),
            name: "Hero".to_owned(),
            rank,
            games: 50_000,
            win_rate_tenths: Some(500),
            low_data: false,
            taken,
        }
    }

    #[test]
    fn meta_row_selection_matches_the_overlay_rule() {
        let untaken: Vec<_> = (1..=12).map(|rank| row(Some(rank), false)).collect();
        assert_eq!(overlay_meta_rows(&untaken, 8), 8);
        assert_eq!(overlay_meta_rows(&untaken, 5), 5);
        let mut mixed = vec![row(Some(1), true), row(Some(2), true)];
        mixed.extend((3..=7).map(|rank| row(Some(rank), false)));
        assert_eq!(
            overlay_meta_rows(&mixed, 8),
            7,
            "two taken rows outrank drawn pickable rows"
        );
        // The host sends taken rows without a rank, so they never fill a slot.
        let unranked_taken = vec![row(None, true), row(Some(2), false)];
        assert_eq!(overlay_meta_rows(&unranked_taken, 8), 1);
    }

    #[test]
    fn every_state_ends_no_lower_than_y_860() {
        let worst_suggestions = BRAND
            + GAP
            + (CARD_BORDER + COUNTER_HEAD + 3 * COUNTER_ROW + GAP)
            + (CARD_BORDER + HEAD + 5 * META_ROW)
            + GAP
            + FOOTER;
        let full_meta = BRAND + GAP + (CARD_BORDER + HEAD + 8 * META_ROW) + GAP + FOOTER;
        let pick = BRAND
            + GAP
            + (CARD_BORDER + HEAD + PICK_BODY)
            + GAP
            + (CARD_BORDER + HEAD + DURATION_BODY)
            + GAP
            + FOOTER;
        let message = BRAND
            + GAP
            + (CARD_BORDER + COUNTER_HEAD + 3 * COUNTER_ROW + GAP)
            + (CARD_BORDER + HEAD + MESSAGE)
            + GAP
            + FOOTER;
        assert_eq!(worst_suggestions, 692);
        assert_eq!(full_meta, 578);
        assert_eq!(pick, 554);
        assert_eq!(message, 530);
        for height in [worst_suggestions, full_meta, pick, message, CHIP] {
            assert!(TOP + height as i32 <= MAX_BOTTOM, "{height}px from y {TOP}");
        }
        assert_eq!(MAX_HEIGHT, 728);
    }

    #[test]
    fn rust_layout_numbers_match_the_renderer() {
        let model = include_str!("../../../src/draft/draftColumnModel.ts");
        let start = model
            .find("export const OVERLAY_LAYOUT")
            .expect("layout block");
        let block = &model[start..start + model[start..].find("} as const").expect("end")];
        let value = |key: &str| -> u32 {
            let at = block
                .find(&format!("  {key}: "))
                .unwrap_or_else(|| panic!("{key}"));
            block[at + key.len() + 4..]
                .split(',')
                .next()
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        };
        assert_eq!(value("top") as i32, TOP);
        assert_eq!(value("maxBottom") as i32, MAX_BOTTOM);
        for (key, expected) in [
            ("gap", GAP),
            ("brand", BRAND),
            ("cardBorder", CARD_BORDER),
            ("head", HEAD),
            ("counterHead", COUNTER_HEAD),
            ("metaRow", META_ROW),
            ("counterRow", COUNTER_ROW),
            ("message", MESSAGE),
            ("pickBody", PICK_BODY),
            ("durationBody", DURATION_BODY),
            ("footer", FOOTER),
            ("chip", CHIP),
        ] {
            assert_eq!(value(key), expected, "{key}");
        }
    }
}
