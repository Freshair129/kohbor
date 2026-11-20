//! Bounded, deterministic-at-the-seed selection over already-prewarmed bindings.
//! This module performs no I/O and is owned by the audio-engine thread.

use std::collections::HashMap;

use kohbor_audio::Outcome;
use kohbor_events::EventId;

use crate::audio::resolve::{
    BindingLookup, PrewarmTable, SelectedClip, VariantSelection, VariantSet,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionFailure {
    Unmapped,
    InvalidMedia,
    VariantUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BindingKey {
    event_id: EventId,
    bound_tier: Option<u8>,
}

#[derive(Debug, Default)]
struct BindingHistory {
    play_order: HashMap<String, u64>,
    next_order: u64,
    last_sequence_index: Option<usize>,
}

pub struct VariantSelector {
    rng: u64,
    session_epoch: Option<u64>,
    histories: HashMap<BindingKey, BindingHistory>,
}

impl VariantSelector {
    /// Production seed uses the host's existing OS RNG dependency. If it is not
    /// available, a fixed nonzero seed still keeps selection defined and safe.
    pub fn from_entropy() -> Self {
        let seed = crate::secrets::random::<8>()
            .map(u64::from_le_bytes)
            .unwrap_or(0x6b6f_6862_6f72_7634);
        Self::with_seed(seed)
    }

    pub fn with_seed(seed: u64) -> Self {
        Self {
            rng: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
            session_epoch: None,
            histories: HashMap::new(),
        }
    }

    /// Keep the legacy P2 audition behavior for callers that do not name a variant.
    pub fn first_ready(
        table: &PrewarmTable,
        event_id: EventId,
        requested_tier: Option<u8>,
    ) -> Result<SelectedClip, SelectionFailure> {
        match table.lookup_binding(event_id, requested_tier) {
            BindingLookup::Unmapped => Err(SelectionFailure::Unmapped),
            BindingLookup::Invalid => Err(SelectionFailure::InvalidMedia),
            BindingLookup::Ready { bound_tier, clip } => Ok(SelectedClip {
                event_id,
                bound_tier,
                variant_index: 0,
                variant_id: "v1".to_owned(),
                clip_id: None,
                selection: None,
                media: clip.clone(),
            }),
            BindingLookup::Variants {
                bound_tier,
                binding,
            } => {
                let Some((index, variant)) = binding
                    .variants
                    .iter()
                    .enumerate()
                    .find(|(_, variant)| variant.media.is_ok())
                else {
                    return Err(SelectionFailure::InvalidMedia);
                };
                let Ok(media) = variant.media.as_ref() else {
                    return Err(SelectionFailure::InvalidMedia);
                };
                Ok(SelectedClip {
                    event_id,
                    bound_tier,
                    variant_index: index,
                    variant_id: variant.variant_id.clone(),
                    clip_id: variant.clip_id.clone(),
                    selection: binding.selection,
                    media: media.clone(),
                })
            }
        }
    }

    /// Resolve the exact variant requested by a Packs row without applying the
    /// normal random, shuffle, or sequence selection policy.
    pub fn by_id(
        table: &PrewarmTable,
        event_id: EventId,
        requested_tier: Option<u8>,
        variant_id: &str,
    ) -> Result<SelectedClip, SelectionFailure> {
        match table.lookup_binding(event_id, requested_tier) {
            BindingLookup::Unmapped => Err(SelectionFailure::Unmapped),
            BindingLookup::Invalid => Err(SelectionFailure::InvalidMedia),
            BindingLookup::Ready { bound_tier, clip } => {
                if variant_id != "v1" {
                    return Err(SelectionFailure::VariantUnavailable);
                }
                Ok(SelectedClip {
                    event_id,
                    bound_tier,
                    variant_index: 0,
                    variant_id: "v1".to_owned(),
                    clip_id: None,
                    selection: None,
                    media: clip.clone(),
                })
            }
            BindingLookup::Variants {
                bound_tier,
                binding,
            } => {
                let Some((index, variant)) = binding
                    .variants
                    .iter()
                    .enumerate()
                    .find(|(_, variant)| variant.variant_id == variant_id)
                else {
                    return Err(SelectionFailure::VariantUnavailable);
                };
                let Ok(media) = variant.media.as_ref() else {
                    return Err(SelectionFailure::InvalidMedia);
                };
                Ok(SelectedClip {
                    event_id,
                    bound_tier,
                    variant_index: index,
                    variant_id: variant.variant_id.clone(),
                    clip_id: variant.clip_id.clone(),
                    selection: binding.selection,
                    media: media.clone(),
                })
            }
        }
    }

    /// Apply the unchanged highest-tier-at-or-below lookup, then choose only a
    /// media pair which the prewarm worker successfully verified and decoded.
    pub fn select(
        &mut self,
        table: &PrewarmTable,
        event_id: EventId,
        requested_tier: Option<u8>,
        session_epoch: u64,
    ) -> Result<SelectedClip, SelectionFailure> {
        self.begin_session(session_epoch);
        match table.lookup_binding(event_id, requested_tier) {
            BindingLookup::Unmapped => Err(SelectionFailure::Unmapped),
            BindingLookup::Invalid => Err(SelectionFailure::InvalidMedia),
            BindingLookup::Ready { bound_tier, clip } => Ok(SelectedClip {
                event_id,
                bound_tier,
                variant_index: 0,
                variant_id: "v1".to_owned(),
                clip_id: None,
                selection: None,
                media: clip.clone(),
            }),
            BindingLookup::Variants {
                bound_tier,
                binding,
            } => {
                let index = self.choose_index(event_id, bound_tier, binding)?;
                let Some(variant) = binding.variants.get(index) else {
                    return Err(SelectionFailure::InvalidMedia);
                };
                let Ok(media) = variant.media.as_ref() else {
                    return Err(SelectionFailure::InvalidMedia);
                };
                Ok(SelectedClip {
                    event_id,
                    bound_tier,
                    variant_index: index,
                    variant_id: variant.variant_id.clone(),
                    clip_id: variant.clip_id.clone(),
                    selection: binding.selection,
                    media: media.clone(),
                })
            }
        }
    }

    /// Advance play history only after the audio owner admits the selected clip.
    /// Queued or dropped requests therefore do not count as having been played.
    pub fn record_played(&mut self, selected: &SelectedClip) {
        let history = self
            .histories
            .entry(BindingKey {
                event_id: selected.event_id,
                bound_tier: selected.bound_tier,
            })
            .or_default();
        history.next_order = history.next_order.saturating_add(1);
        history
            .play_order
            .insert(selected.variant_id.clone(), history.next_order);
        if selected.selection == Some(VariantSelection::Sequence) {
            history.last_sequence_index = Some(selected.variant_index);
        }
    }

    pub fn reset_for_session(&mut self, session_epoch: u64) {
        if self
            .session_epoch
            .is_none_or(|current| session_epoch > current)
        {
            self.session_epoch = Some(session_epoch);
            self.histories.clear();
        }
    }

    fn begin_session(&mut self, session_epoch: u64) {
        self.reset_for_session(session_epoch);
    }

    fn choose_index(
        &mut self,
        event_id: EventId,
        bound_tier: Option<u8>,
        binding: &VariantSet,
    ) -> Result<usize, SelectionFailure> {
        let ready: Vec<usize> = binding
            .variants
            .iter()
            .enumerate()
            .filter_map(|(index, variant)| variant.media.is_ok().then_some(index))
            .collect();
        let Some(first) = ready.first().copied() else {
            return Err(SelectionFailure::InvalidMedia);
        };
        if binding.variants.len() == 1 || binding.selection.is_none() {
            return Ok(first);
        }

        let key = BindingKey {
            event_id,
            bound_tier,
        };
        match binding.selection {
            Some(VariantSelection::Random) => {
                let total: u64 = ready
                    .iter()
                    .filter_map(|index| binding.variants.get(*index))
                    .map(|variant| u64::from(variant.weight.max(1)))
                    .sum();
                let mut cursor = self.random_below(total.max(1));
                for index in ready {
                    let weight = binding
                        .variants
                        .get(index)
                        .map(|variant| u64::from(variant.weight.max(1)))
                        .unwrap_or(1);
                    if cursor < weight {
                        return Ok(index);
                    }
                    cursor = cursor.saturating_sub(weight);
                }
                Ok(first)
            }
            Some(VariantSelection::Sequence) => {
                let len = binding.variants.len();
                let start = self
                    .histories
                    .get(&key)
                    .and_then(|history| history.last_sequence_index)
                    .map_or(0, |index| index.saturating_add(1) % len);
                for offset in 0..len {
                    let index = start.saturating_add(offset) % len;
                    if binding
                        .variants
                        .get(index)
                        .is_some_and(|variant| variant.media.is_ok())
                    {
                        return Ok(index);
                    }
                }
                Err(SelectionFailure::InvalidMedia)
            }
            Some(VariantSelection::Shuffle) => {
                let least_recent = ready
                    .iter()
                    .filter_map(|index| binding.variants.get(*index))
                    .map(|variant| {
                        self.histories
                            .get(&key)
                            .and_then(|history| history.play_order.get(&variant.variant_id))
                            .copied()
                            .unwrap_or(0)
                    })
                    .min()
                    .unwrap_or(0);
                let ties: Vec<usize> = ready
                    .into_iter()
                    .filter(|index| {
                        binding.variants.get(*index).is_some_and(|variant| {
                            self.histories
                                .get(&key)
                                .and_then(|history| history.play_order.get(&variant.variant_id))
                                .copied()
                                .unwrap_or(0)
                                == least_recent
                        })
                    })
                    .collect();
                let tie = usize::try_from(self.random_below(ties.len() as u64)).unwrap_or(0);
                Ok(ties.get(tie).copied().unwrap_or(first))
            }
            None => Ok(first),
        }
    }

    fn random_below(&mut self, upper_exclusive: u64) -> u64 {
        let upper_exclusive = upper_exclusive.max(1);
        let threshold = upper_exclusive.wrapping_neg() % upper_exclusive;
        loop {
            let mut value = self.rng;
            value ^= value >> 12;
            value ^= value << 25;
            value ^= value >> 27;
            self.rng = value;
            let sample = value.wrapping_mul(0x2545_f491_4f6c_dd1d);
            if sample >= threshold {
                return sample % upper_exclusive;
            }
        }
    }
}

impl From<SelectionFailure> for Outcome {
    fn from(failure: SelectionFailure) -> Self {
        match failure {
            SelectionFailure::Unmapped => Self::Unmapped,
            SelectionFailure::InvalidMedia => Self::InvalidMedia,
            SelectionFailure::VariantUnavailable => Self::Unmapped,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use kohbor_audio::pcm::Pcm;
    use kohbor_events::EventId;

    use crate::audio::resolve::{
        Clip, Entry, PrewarmTable, VariantMedia, VariantSelection, VariantSet,
    };

    use super::{SelectionFailure, VariantSelector};

    fn clip(tier: Option<u8>, marker: i16) -> Clip {
        Clip {
            pcm: Pcm::from(vec![marker, -marker]),
            bound_tier: tier,
            audio_ms: 1,
            visual_tail_ms: 0,
            visual_url: None,
        }
    }

    fn candidate(id: &str, weight: u8, media: Option<Clip>) -> VariantMedia {
        VariantMedia {
            variant_id: id.to_owned(),
            clip_id: Some(format!("vrzo-th-{id:0>4}")),
            weight,
            media: media.ok_or("mixed_audio_invalid"),
        }
    }

    fn set(selection: Option<VariantSelection>, variants: Vec<VariantMedia>) -> VariantSet {
        VariantSet {
            selection,
            variants,
        }
    }

    fn table(id: EventId, tier: Option<u8>, variants: VariantSet) -> PrewarmTable {
        PrewarmTable::new(
            1,
            Some(Arc::from("ins_0000000001")),
            vec![(id, tier, Entry::Variants(variants))],
        )
    }

    fn choose(
        selector: &mut VariantSelector,
        table: &PrewarmTable,
        id: EventId,
        tier: Option<u8>,
        epoch: u64,
    ) -> String {
        selector
            .select(table, id, tier, epoch)
            .expect("a ready variant is selected")
            .variant_id
    }

    #[test]
    fn exact_variant_lookup_ignores_shuffle_and_sequence_selection() {
        let table = table(
            EventId::HeroKill,
            None,
            set(
                Some(VariantSelection::Shuffle),
                vec![
                    candidate("first", 1, Some(clip(None, 1))),
                    candidate("chosen", 1, Some(clip(None, 2))),
                ],
            ),
        );
        let chosen = VariantSelector::by_id(&table, EventId::HeroKill, None, "chosen")
            .expect("named variant is ready");
        assert_eq!(chosen.variant_id, "chosen");
        assert_eq!(chosen.media.pcm.as_ref(), &[2, -2]);
        assert_eq!(
            VariantSelector::by_id(&table, EventId::HeroKill, None, "missing").unwrap_err(),
            SelectionFailure::VariantUnavailable
        );
    }

    #[test]
    fn random_selection_follows_weights_for_a_fixed_seed() {
        let table = table(
            EventId::HeroKill,
            None,
            set(
                Some(VariantSelection::Random),
                vec![
                    candidate("light", 1, Some(clip(None, 1))),
                    candidate("heavy", 3, Some(clip(None, 2))),
                ],
            ),
        );
        let mut selector = VariantSelector::with_seed(0x5eed);
        let mut heavy = 0;
        for _ in 0..4_000 {
            if choose(&mut selector, &table, EventId::HeroKill, None, 1) == "heavy" {
                heavy += 1;
            }
        }
        assert!((2_850..=3_150).contains(&heavy), "heavy count was {heavy}");
    }

    #[test]
    fn sequence_selection_uses_list_order_and_wraps() {
        let table = table(
            EventId::HeroKill,
            None,
            set(
                Some(VariantSelection::Sequence),
                vec![
                    candidate("first", 1, Some(clip(None, 1))),
                    candidate("second", 1, Some(clip(None, 2))),
                    candidate("third", 1, Some(clip(None, 3))),
                ],
            ),
        );
        let mut selector = VariantSelector::with_seed(7);
        let mut chosen = Vec::new();
        for _ in 0..6 {
            let selected = selector
                .select(&table, EventId::HeroKill, None, 1)
                .expect("variant is ready");
            chosen.push(selected.variant_id.clone());
            selector.record_played(&selected);
        }
        assert_eq!(
            chosen,
            vec!["first", "second", "third", "first", "second", "third"]
        );
    }

    #[test]
    fn shuffle_uses_least_recent_first_and_never_repeats_immediately() {
        let table = table(
            EventId::HeroKill,
            None,
            set(
                Some(VariantSelection::Shuffle),
                ["a", "b", "c", "d"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, id)| candidate(id, 1, Some(clip(None, index as i16 + 1))))
                    .collect(),
            ),
        );
        let mut selector = VariantSelector::with_seed(41);
        let mut chosen = Vec::new();
        for _ in 0..20 {
            let selected = selector
                .select(&table, EventId::HeroKill, None, 1)
                .expect("variant is ready");
            chosen.push(selected.variant_id.clone());
            selector.record_played(&selected);
        }
        assert!(chosen.windows(2).all(|pair| pair[0] != pair[1]));
        for group in chosen.chunks_exact(4) {
            assert_eq!(group.iter().collect::<BTreeSet<_>>().len(), 4);
        }
    }

    #[test]
    fn a_new_match_session_clears_sequence_history() {
        let table = table(
            EventId::HeroKill,
            None,
            set(
                Some(VariantSelection::Sequence),
                vec![
                    candidate("first", 1, Some(clip(None, 1))),
                    candidate("second", 1, Some(clip(None, 2))),
                ],
            ),
        );
        let mut selector = VariantSelector::with_seed(9);
        let first = selector
            .select(&table, EventId::HeroKill, None, 12)
            .unwrap();
        selector.record_played(&first);
        assert_eq!(
            choose(&mut selector, &table, EventId::HeroKill, None, 12),
            "second"
        );
        assert_eq!(
            choose(&mut selector, &table, EventId::HeroKill, None, 13),
            "first"
        );
    }

    #[test]
    fn tierless_binding_shares_history_across_requested_tiers() {
        let table = table(
            EventId::Streak,
            None,
            set(
                Some(VariantSelection::Sequence),
                vec![
                    candidate("first", 1, Some(clip(None, 1))),
                    candidate("second", 1, Some(clip(None, 2))),
                ],
            ),
        );
        let mut selector = VariantSelector::with_seed(11);
        let first = selector
            .select(&table, EventId::Streak, Some(3), 1)
            .expect("tier 3 resolves to the tierless binding");
        assert_eq!(first.bound_tier, None);
        selector.record_played(&first);
        assert_eq!(
            choose(&mut selector, &table, EventId::Streak, Some(10), 1),
            "second"
        );
    }

    #[test]
    fn an_unloadable_variant_falls_back_within_its_binding_only() {
        let table = PrewarmTable::new(
            1,
            Some(Arc::from("ins_0000000001")),
            vec![
                (
                    EventId::Streak,
                    None,
                    Entry::Variants(set(
                        Some(VariantSelection::Sequence),
                        vec![
                            candidate("broken", 1, None),
                            candidate("usable", 1, Some(clip(None, 2))),
                        ],
                    )),
                ),
                (
                    EventId::HeroKill,
                    None,
                    Entry::Variants(set(
                        None,
                        vec![candidate("other-event", 1, Some(clip(None, 3)))],
                    )),
                ),
            ],
        );
        let mut selector = VariantSelector::with_seed(13);
        assert_eq!(
            choose(&mut selector, &table, EventId::Streak, None, 1),
            "usable"
        );
        let no_cross_event = PrewarmTable::new(
            1,
            Some(Arc::from("ins_0000000001")),
            vec![
                (
                    EventId::Streak,
                    None,
                    Entry::Variants(set(
                        Some(VariantSelection::Sequence),
                        vec![candidate("unavailable", 1, None)],
                    )),
                ),
                (
                    EventId::HeroKill,
                    None,
                    Entry::Variants(set(
                        None,
                        vec![candidate("other-event", 1, Some(clip(None, 3)))],
                    )),
                ),
            ],
        );
        assert!(
            matches!(
                selector.select(&no_cross_event, EventId::Streak, None, 1),
                Err(SelectionFailure::InvalidMedia)
            ),
            "lookup never substitutes an unrelated event's clip"
        );
    }

    #[test]
    fn one_legacy_variant_repeats_unchanged_without_a_selection_mode() {
        let table = table(
            EventId::HeroKill,
            None,
            set(None, vec![candidate("v1", 1, Some(clip(None, 17)))]),
        );
        let mut selector = VariantSelector::with_seed(17);
        for _ in 0..4 {
            let selected = selector
                .select(&table, EventId::HeroKill, None, 1)
                .expect("legacy clip remains ready");
            assert_eq!(selected.variant_id, "v1");
            assert_eq!(&*selected.media.pcm, &[17, -17]);
            assert_eq!(selected.selection, None);
            selector.record_played(&selected);
        }
    }
}
