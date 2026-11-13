//! Event to audio mapping helpers and the prewarmed clip table (S3-5, design
//! docs/design/S3-5-HOST-CRITICAL-PATH.md sections 2 and 3). API STUB: the bodies are
//! Implemented for S3-5d; the S3-5c tests are written against these signatures.
//! Admission code: pure, no I/O, no lock, no panic on any input.

#![allow(dead_code, unused_variables)]

use std::fmt::Write as _;
use std::sync::Arc;

use kohbor_audio::pcm::Pcm;
use kohbor_audio::AudioClass;
use kohbor_events::EventId;

/// `Critical` when `id.is_critical()`, otherwise `Ordinary`. (C2, C3 and C5 are not
/// reachable from a detector event in S3-5.)
pub fn class_of(id: EventId) -> AudioClass {
    if id.is_critical() {
        AudioClass::Critical
    } else {
        AudioClass::Ordinary
    }
}

/// `ev_g{generation}_s{sequence}_{index}`, for example `ev_g3_s120_0`. The result
/// always matches the contract pattern `^[A-Za-z0-9_-]{8,96}$` for any input.
pub fn instance_id(generation: u64, sequence: u64, index: usize) -> String {
    let mut value = String::with_capacity(67);
    let _ = write!(&mut value, "ev_g{generation}_s{sequence}_{index}");
    value
}

/// One verified, decoded clip held in memory (never read from disk on admission).
#[derive(Debug, Clone)]
pub struct Clip {
    pub pcm: Pcm,
    /// The tier the binding was authored for (`None` for a tier-less binding).
    pub bound_tier: Option<u8>,
    /// `ceil(samples * 1000 / 48000)`, the owner's audio deadline.
    pub audio_ms: u32,
    pub visual_tail_ms: u32,
    /// Logical URL `kohbor-media://localhost/v1/<install_id>/<64 hex>` or `None`.
    pub visual_url: Option<Arc<str>>,
}

#[derive(Debug, Clone)]
pub enum Entry {
    Ready(Clip),
    /// Bound, but verification, read or decode failed.
    Invalid,
}

#[derive(Debug)]
pub enum Lookup<'a> {
    Ready(&'a Clip),
    Unmapped,
    Invalid,
}

/// Immutable table of prewarmed clips for one active revision.
#[derive(Debug)]
pub struct PrewarmTable {
    /// The decimal number of the `install_id` (`ins_0000000007` gives 7); 0 = no revision.
    pub fence: u64,
    pub install_id: Option<Arc<str>>,
    entries: Vec<(EventId, Option<u8>, Entry)>,
}

impl PrewarmTable {
    /// The table before any revision is active: fence 0, no install id, no entries.
    pub fn empty() -> Arc<PrewarmTable> {
        Arc::new(Self {
            fence: 0,
            install_id: None,
            entries: Vec::new(),
        })
    }

    /// Entries are keyed by `(event, bound tier)`; at most one entry per key (a later
    /// duplicate key replaces the earlier one).
    pub fn new(
        fence: u64,
        install_id: Option<Arc<str>>,
        mut entries: Vec<(EventId, Option<u8>, Entry)>,
    ) -> PrewarmTable {
        let mut position = 0;
        while let Some((id, tier, _)) = entries.get(position) {
            let key = (*id, *tier);
            let mut later = position.saturating_add(1);

            while later < entries.len() {
                let is_duplicate =
                    entries
                        .get(later)
                        .is_some_and(|(candidate_id, candidate_tier, _)| {
                            *candidate_id == key.0 && *candidate_tier == key.1
                        });
                if is_duplicate {
                    let replacement = entries.remove(later);
                    if let Some(current) = entries.get_mut(position) {
                        *current = replacement;
                    }
                } else {
                    later = later.saturating_add(1);
                }
            }

            position = position.saturating_add(1);
        }

        PrewarmTable {
            fence,
            install_id,
            entries,
        }
    }

    /// Tier rule (owner decision 2026-11-11, EVENT-AUDIO-SPEC section 2):
    /// - `tier == Some(t)`: the entry with the highest bound tier `b` with `b <= t`;
    ///   when none exists, the tier-less entry; when that does not exist either,
    ///   `Unmapped`. If the chosen entry is `Entry::Invalid` the result is
    ///   `Lookup::Invalid` (no further fallback to a lower tier: a corrupt intended
    ///   clip is reported, never silently replaced by a weaker one).
    /// - `tier == None`: only the tier-less entry (`Unmapped` when absent).
    /// - A tiered entry never answers a lookup below its bound tier.
    pub fn lookup(&self, id: EventId, tier: Option<u8>) -> Lookup<'_> {
        let mut best_tiered: Option<(u8, &Entry)> = None;
        let mut tierless: Option<&Entry> = None;

        for (entry_id, bound_tier, entry) in &self.entries {
            if *entry_id != id {
                continue;
            }

            match bound_tier {
                Some(bound) if tier.is_some_and(|requested| *bound <= requested) => {
                    let is_better = match best_tiered {
                        Some((current, _)) => *bound > current,
                        None => true,
                    };
                    if is_better {
                        best_tiered = Some((*bound, entry));
                    }
                }
                None => tierless = Some(entry),
                Some(_) => {}
            }
        }

        match best_tiered.map(|(_, entry)| entry).or(tierless) {
            Some(Entry::Ready(clip)) => Lookup::Ready(clip),
            Some(Entry::Invalid) => Lookup::Invalid,
            None => Lookup::Unmapped,
        }
    }

    /// True when the table has a `Ready` tier-less entry for `id` (readiness helper).
    pub fn has_ready_tierless(&self, id: EventId) -> bool {
        self.entries.iter().any(|(entry_id, tier, entry)| {
            *entry_id == id && tier.is_none() && matches!(entry, Entry::Ready(_))
        })
    }

    /// Clone the table while removing visuals for the supplied event IDs.
    /// Decoded PCM remains shared through its `Arc`; the service keeps the source
    /// table so a later successful renderer probe can restore the visual URLs.
    pub fn without_visuals(&self, ids: &[EventId]) -> Arc<PrewarmTable> {
        let entries = self
            .entries
            .iter()
            .map(|(id, tier, entry)| {
                let copied = match entry {
                    Entry::Ready(clip) if ids.contains(id) => Entry::Ready(Clip {
                        pcm: clip.pcm.clone(),
                        bound_tier: clip.bound_tier,
                        audio_ms: clip.audio_ms,
                        visual_tail_ms: clip.visual_tail_ms,
                        visual_url: None,
                    }),
                    other => other.clone(),
                };
                (*id, *tier, copied)
            })
            .collect();
        Arc::new(PrewarmTable::new(
            self.fence,
            self.install_id.clone(),
            entries,
        ))
    }
}
