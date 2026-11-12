//! Host-owned inventory and readiness view for critical event media.
//!
//! The production functions are pure apart from reading the supplied engine
//! atomics. They do not perform I/O, acquire locks, read a clock, or panic.

use std::sync::atomic::Ordering;

use kohbor_audio::{assess, CriticalMedia, ReadinessInput};
use kohbor_events::EventId;

/// Every event known to this host's detector, in detector declaration order.
pub const ALL_EVENTS: [kohbor_events::EventId; 12] = [
    EventId::MultiKill,
    EventId::Streak,
    EventId::FirstBlood,
    EventId::HeroKill,
    EventId::HeroDeath,
    EventId::LevelMilestone,
    EventId::HealthLow,
    EventId::ManaLow,
    EventId::MatchEnded,
    EventId::MatchStarted,
    EventId::HeroRespawn,
    EventId::HealthDanger,
];

// Keep this exhaustive match independent of ALL_EVENTS: adding an EventId must
// fail compilation until the host inventory and its fixed-size declaration are
// deliberately updated.
fn event_variant_guard(id: EventId) {
    match id {
        EventId::MultiKill => {}
        EventId::Streak => {}
        EventId::FirstBlood => {}
        EventId::HeroKill => {}
        EventId::HeroDeath => {}
        EventId::LevelMilestone => {}
        EventId::HealthLow => {}
        EventId::ManaLow => {}
        EventId::MatchEnded => {}
        EventId::MatchStarted => {}
        EventId::HeroRespawn => {}
        EventId::HealthDanger => {}
    }
}

const _: fn(EventId) = event_variant_guard;

/// Critical detector events available to this host, regardless of caller input.
pub fn available_critical() -> Vec<EventId> {
    ALL_EVENTS
        .iter()
        .copied()
        .filter(|id| id.is_critical() && !crate::intake::EVENT_UNAVAILABLE.contains(id))
        .collect()
}

/// Store and tier-less table facts published by the future media service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFacts {
    pub store_ok: bool,
    pub critical: Vec<CriticalMedia>,
}

/// Build readiness facts from the engine-held table for the complete host inventory.
pub fn facts_from_table(
    table: &crate::audio::resolve::PrewarmTable,
    probe: &dyn Fn(EventId) -> Option<bool>,
    store_ok: bool,
) -> MediaFacts {
    let available = available_critical();
    let mut critical = Vec::with_capacity(available.len());

    for id in available {
        let media = match table.lookup(id, None) {
            crate::audio::resolve::Lookup::Ready(clip) => {
                let probe_ok = if clip.visual_url.is_none() {
                    // DEC-PACK-AUDIO-OPTIONAL-VISUAL: audio-only clips need no probe.
                    Some(true)
                } else {
                    probe(id)
                };
                CriticalMedia {
                    event_id: id.as_str().to_owned(),
                    mapped: true,
                    verified: true,
                    prewarmed: true,
                    probe_ok,
                }
            }
            crate::audio::resolve::Lookup::Invalid => CriticalMedia {
                event_id: id.as_str().to_owned(),
                mapped: true,
                verified: false,
                prewarmed: false,
                probe_ok: None,
            },
            crate::audio::resolve::Lookup::Unmapped => CriticalMedia {
                event_id: id.as_str().to_owned(),
                mapped: false,
                verified: false,
                prewarmed: false,
                probe_ok: None,
            },
        };
        critical.push(media);
    }

    MediaFacts { store_ok, critical }
}

/// Assess runtime media readiness using the complete host inventory and engine state.
pub fn media_status(
    facts: &MediaFacts,
    shared: &crate::audio::lanes::EngineShared,
    now_ms: u64,
) -> (&'static str, Option<&'static str>) {
    let heartbeat_ms = shared.heartbeat_ms.load(Ordering::Relaxed);
    if now_ms.saturating_sub(heartbeat_ms) > 2_000
        || shared.degraded.load(Ordering::Relaxed)
        || shared.critical_overflow.load(Ordering::Relaxed) > 0
    {
        return ("unavailable", Some("CAPABILITY_UNAVAILABLE"));
    }

    let muted = shared.muted.load(Ordering::Relaxed);
    let device_available = shared.device_available.load(Ordering::Relaxed);
    let available_critical = available_critical()
        .into_iter()
        .map(|id| {
            facts
                .critical
                .iter()
                .find(|media| media.event_id == id.as_str())
                .cloned()
                .unwrap_or_else(|| CriticalMedia {
                    event_id: id.as_str().to_owned(),
                    mapped: false,
                    verified: false,
                    prewarmed: false,
                    probe_ok: None,
                })
        })
        .collect();
    let readiness = assess(&ReadinessInput {
        available_critical,
        muted,
        device_available,
    });

    if !facts.store_ok && !muted && device_available {
        return ("not_ready", Some("MEDIA_INVALID"));
    }

    (readiness.state.as_str(), readiness.reason)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use kohbor_audio::CriticalMedia;
    use kohbor_events::EventId;
    use serde_json::Value;

    use super::{available_critical, facts_from_table, media_status, MediaFacts};
    use crate::audio::lanes::EngineShared;
    use crate::audio::resolve::{Clip, Entry, PrewarmTable};

    fn contract_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../contracts")
            .join(name)
    }

    fn critical_media(
        mapped: bool,
        verified: bool,
        prewarmed: bool,
        probe_ok: Option<bool>,
    ) -> CriticalMedia {
        CriticalMedia {
            event_id: EventId::HealthDanger.as_str().to_owned(),
            mapped,
            verified,
            prewarmed,
            probe_ok,
        }
    }

    fn ready_clip(visual_url: Option<&str>) -> Clip {
        Clip {
            pcm: Arc::from([0_i16]),
            bound_tier: None,
            audio_ms: 1,
            visual_tail_ms: 0,
            visual_url: visual_url.map(Arc::<str>::from),
        }
    }

    fn table(entries: Vec<(EventId, Option<u8>, Entry)>) -> PrewarmTable {
        PrewarmTable::new(1, Some(Arc::<str>::from("ins_0000000001")), entries)
    }

    fn all_good_facts() -> MediaFacts {
        MediaFacts {
            store_ok: true,
            critical: vec![critical_media(true, true, true, Some(true))],
        }
    }

    fn generated_audio_only_facts() -> MediaFacts {
        let table = table(vec![(
            EventId::HealthDanger,
            None,
            Entry::Ready(ready_clip(None)),
        )]);
        facts_from_table(&table, &|_| Some(false), true)
    }

    fn engine_shared(
        heartbeat_ms: u64,
        muted: bool,
        device_available: bool,
        degraded: bool,
        critical_overflow: u64,
    ) -> EngineShared {
        let shared = EngineShared::new();
        shared.heartbeat_ms.store(heartbeat_ms, Ordering::Relaxed);
        shared.muted.store(muted, Ordering::Relaxed);
        shared
            .device_available
            .store(device_available, Ordering::Relaxed);
        shared.degraded.store(degraded, Ordering::Relaxed);
        shared
            .critical_overflow
            .store(critical_overflow, Ordering::Relaxed);
        shared
    }

    fn assert_media_status_case(
        facts: &MediaFacts,
        shared: &EngineShared,
        now_ms: u64,
        expected: (&'static str, Option<&'static str>),
        case: &str,
    ) {
        let actual = media_status(facts, shared, now_ms);
        assert_eq!(actual, expected, "{case}: exact media status");

        let schema_text = std::fs::read_to_string(contract_path("runtime-status.schema.json"))
            .expect("runtime-status schema is readable");
        let schema: Value =
            serde_json::from_str(&schema_text).expect("runtime-status schema is valid JSON");
        let media = &schema["properties"]["media"]["properties"];
        let states = media["state"]["enum"]
            .as_array()
            .expect("media state enum exists");
        let reasons = media["reason"]["anyOf"][0]["enum"]
            .as_array()
            .expect("media reason enum exists");
        assert!(
            states.iter().any(|value| value.as_str() == Some(actual.0)),
            "{case}: returned state is in runtime-status media enum"
        );
        assert!(
            actual.1.is_none_or(|reason| {
                reasons.iter().any(|value| value.as_str() == Some(reason))
            }),
            "{case}: returned reason is in runtime-status media enum"
        );
    }

    macro_rules! event_name_cases {
        ($($case:ident: $variant:ident => $expected:literal),+ $(,)?) => {
            $(
                #[test]
                fn $case() {
                    let id = EventId::$variant;
                    assert_eq!(
                        (super::ALL_EVENTS.contains(&id), id.as_str()),
                        (true, $expected),
                        "ALL_EVENTS includes the expected detector variant"
                    );
                }
            )+
        };
    }

    event_name_cases! {
        all_events_has_multi_kill: MultiKill => "multi_kill",
        all_events_has_streak: Streak => "streak",
        all_events_has_first_blood: FirstBlood => "first_blood",
        all_events_has_hero_kill: HeroKill => "hero_kill",
        all_events_has_hero_death: HeroDeath => "hero_death",
        all_events_has_level_milestone: LevelMilestone => "level_milestone",
        all_events_has_health_low: HealthLow => "health_low",
        all_events_has_mana_low: ManaLow => "mana_low",
        all_events_has_match_ended: MatchEnded => "match_ended",
        all_events_has_match_started: MatchStarted => "match_started",
        all_events_has_hero_respawn: HeroRespawn => "hero_respawn",
        all_events_has_health_danger: HealthDanger => "health_danger",
    }

    #[test]
    fn all_events_has_no_duplicates_and_matches_every_variant_name_in_order() {
        let names: Vec<_> = super::ALL_EVENTS.iter().map(|id| id.as_str()).collect();
        let unique_count = names
            .iter()
            .enumerate()
            .filter(|(index, name)| !names[..*index].contains(name))
            .count();
        assert_eq!(
            unique_count,
            names.len(),
            "ALL_EVENTS contains no duplicates"
        );
        assert_eq!(
            names,
            [
                "multi_kill",
                "streak",
                "first_blood",
                "hero_kill",
                "hero_death",
                "level_milestone",
                "health_low",
                "mana_low",
                "match_ended",
                "match_started",
                "hero_respawn",
                "health_danger",
            ],
            "ALL_EVENTS covers every detector variant name in declaration order"
        );
    }

    #[test]
    fn current_critical_inventory_and_detector_unavailability_match_the_host() {
        assert_eq!(
            available_critical(),
            vec![EventId::HealthDanger],
            "HealthDanger is today's only available critical event"
        );
        assert_eq!(
            crate::intake::EVENT_UNAVAILABLE,
            &[EventId::FirstBlood],
            "first_blood is the only EVENT_UNAVAILABLE entry"
        );
        assert!(
            !EventId::FirstBlood.is_critical(),
            "first_blood is not a critical event"
        );
    }

    #[test]
    fn catalog_critical_media_inventory_cross_check_includes_expected_detector_gap() {
        let catalog_text = std::fs::read_to_string(contract_path("event-catalog.json"))
            .expect("event catalog is readable");
        let catalog: Value =
            serde_json::from_str(&catalog_text).expect("event catalog is valid JSON");
        let events = catalog["events"].as_array().expect("catalog events exist");
        let available = available_critical();

        for event in events {
            let id = event["id"].as_str().expect("catalog event id exists");
            let source = event["source"].as_str().expect("catalog source exists");
            let required = event["critical_media_required"]
                .as_bool()
                .expect("catalog critical media flag exists");
            let in_inventory = available.iter().any(|candidate| candidate.as_str() == id);
            let unavailable = crate::intake::EVENT_UNAVAILABLE
                .iter()
                .any(|candidate| candidate.as_str() == id);

            if required && source == "gsi" {
                assert!(
                    in_inventory || unavailable,
                    "critical GSI catalog event {id} is inventoried or explicitly unavailable"
                );
            }
            if in_inventory {
                assert!(
                    required,
                    "available critical inventory event {id} requires critical media in catalog"
                );
            }
        }

        for id in &available {
            let event = events
                .iter()
                .find(|event| event["id"].as_str() == Some(id.as_str()));
            assert!(
                event.is_some(),
                "inventory event {} exists in catalog",
                id.as_str()
            );
            assert_eq!(
                event.expect("inventory event found")["critical_media_required"],
                true,
                "inventory event {} has critical_media_required=true",
                id.as_str()
            );
        }

        // Expected gap: these native risk events are critical in the catalog but
        // have no EventId/detector in this host and are therefore outside its inventory.
        for id in ["risk_alert", "risk_revision"] {
            let event = events.iter().find(|event| event["id"].as_str() == Some(id));
            assert!(event.is_some(), "expected-gap catalog event {id} exists");
            assert!(
                !super::ALL_EVENTS
                    .iter()
                    .any(|candidate| candidate.as_str() == id),
                "expected-gap event {id} has no host EventId/detector"
            );
            assert!(
                !available.iter().any(|candidate| candidate.as_str() == id),
                "expected-gap event {id} is not in the host readiness inventory"
            );
        }
    }

    #[test]
    fn empty_table_produces_an_unmapped_critical_fact() {
        let table = PrewarmTable::empty();
        let facts = facts_from_table(&table, &|_| Some(true), true);
        assert_eq!(
            facts.critical,
            vec![critical_media(false, false, false, None)],
            "empty table leaves the available critical event unmapped"
        );
    }

    #[test]
    fn ready_audio_only_clip_needs_no_probe_even_when_probe_would_fail() {
        let table = table(vec![(
            EventId::HealthDanger,
            None,
            Entry::Ready(ready_clip(None)),
        )]);
        let facts = facts_from_table(&table, &|_| Some(false), true);
        assert_eq!(
            facts.critical,
            vec![critical_media(true, true, true, Some(true))],
            "ready audio-only clip is mapped, verified, prewarmed, and probe-safe"
        );
    }

    #[test]
    fn ready_visual_clip_with_successful_probe_has_ready_facts() {
        let table = table(vec![(
            EventId::HealthDanger,
            None,
            Entry::Ready(ready_clip(Some("kohbor-media://localhost/v1/visual"))),
        )]);
        let facts = facts_from_table(&table, &|_| Some(true), true);
        assert_eq!(
            facts.critical,
            vec![critical_media(true, true, true, Some(true))],
            "ready visual clip records its successful renderer probe"
        );
    }

    #[test]
    fn ready_visual_clip_with_pending_probe_stays_unknown() {
        let table = table(vec![(
            EventId::HealthDanger,
            None,
            Entry::Ready(ready_clip(Some("kohbor-media://localhost/v1/visual"))),
        )]);
        let facts = facts_from_table(&table, &|_| None, true);
        assert_eq!(
            facts.critical,
            vec![critical_media(true, true, true, None)],
            "missing probe result is preserved as pending or timed out"
        );
    }

    #[test]
    fn ready_visual_clip_with_failed_probe_records_failure() {
        let table = table(vec![(
            EventId::HealthDanger,
            None,
            Entry::Ready(ready_clip(Some("kohbor-media://localhost/v1/visual"))),
        )]);
        let facts = facts_from_table(&table, &|_| Some(false), true);
        assert_eq!(
            facts.critical,
            vec![critical_media(true, true, true, Some(false))],
            "failed visual probe is preserved"
        );
    }

    #[test]
    fn invalid_tierless_entry_is_mapped_but_unverified() {
        let table = table(vec![(EventId::HealthDanger, None, Entry::Invalid)]);
        let facts = facts_from_table(&table, &|_| Some(true), true);
        assert_eq!(
            facts.critical,
            vec![critical_media(true, false, false, None)],
            "invalid tier-less entry exists but is not verified or prewarmed"
        );
    }

    #[test]
    fn tiered_only_entry_does_not_map_the_tierless_readiness_key() {
        // Readiness deliberately uses lookup(id, None); a tiered-only row does not
        // satisfy the host's tier-less critical mapping.
        let table = table(vec![(
            EventId::HealthDanger,
            Some(2),
            Entry::Ready(ready_clip(None)),
        )]);
        let facts = facts_from_table(&table, &|_| Some(true), true);
        assert_eq!(
            facts.critical,
            vec![critical_media(false, false, false, None)],
            "tiered-only HealthDanger entry remains unmapped for tier-less readiness"
        );
    }

    macro_rules! media_status_cases {
        ($(
            $case:ident => ($facts:expr, $shared:expr, $now_ms:expr)
                => ($state:literal, $reason:expr)
        ),+ $(,)?) => {
            $(
                #[test]
                fn $case() {
                    let facts = $facts;
                    let shared = $shared;
                    assert_media_status_case(
                        &facts,
                        &shared,
                        $now_ms,
                        ($state, $reason),
                        stringify!($case),
                    );
                }
            )+
        };
    }

    media_status_cases! {
        today_empty_table_is_exactly_not_ready_unmapped => (
            facts_from_table(&PrewarmTable::empty(), &|_| None, true),
            engine_shared(10_000, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_UNMAPPED")),
        muted_beats_perfect_media => (
            all_good_facts(), engine_shared(10_000, true, true, false, 0), 10_000
        ) => ("unavailable", Some("AUDIO_MUTED")),
        no_device_is_capability_unavailable => (
            all_good_facts(), engine_shared(10_000, false, false, false, 0), 10_000
        ) => ("unavailable", Some("CAPABILITY_UNAVAILABLE")),
        mute_beats_device_unavailable => (
            all_good_facts(), engine_shared(10_000, true, false, false, 0), 10_000
        ) => ("unavailable", Some("AUDIO_MUTED")),
        heartbeat_age_2000_is_fresh => (
            facts_from_table(&PrewarmTable::empty(), &|_| None, true),
            engine_shared(10_000, false, true, false, 0),
            12_000
        ) => ("not_ready", Some("MEDIA_UNMAPPED")),
        heartbeat_age_2001_is_stale => (
            all_good_facts(), engine_shared(10_000, false, true, false, 0), 12_001
        ) => ("unavailable", Some("CAPABILITY_UNAVAILABLE")),
        zero_heartbeat_at_5000_is_stale => (
            all_good_facts(), engine_shared(0, false, true, false, 0), 5_000
        ) => ("unavailable", Some("CAPABILITY_UNAVAILABLE")),
        degraded_engine_is_unavailable => (
            all_good_facts(), engine_shared(10_000, false, true, true, 0), 10_000
        ) => ("unavailable", Some("CAPABILITY_UNAVAILABLE")),
        critical_overflow_is_unavailable => (
            all_good_facts(), engine_shared(10_000, false, true, false, 1), 10_000
        ) => ("unavailable", Some("CAPABILITY_UNAVAILABLE")),
        failed_store_with_device_is_media_invalid => (
            MediaFacts { store_ok: false, critical: all_good_facts().critical },
            engine_shared(10_000, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_INVALID")),
        failed_store_preserves_muted_reason => (
            MediaFacts { store_ok: false, critical: all_good_facts().critical },
            engine_shared(10_000, true, true, false, 0),
            10_000
        ) => ("unavailable", Some("AUDIO_MUTED")),
        failed_store_preserves_missing_device_reason => (
            MediaFacts { store_ok: false, critical: all_good_facts().critical },
            engine_shared(10_000, false, false, false, 0),
            10_000
        ) => ("unavailable", Some("CAPABILITY_UNAVAILABLE")),
        visual_probe_pending_is_not_ready => (
            MediaFacts { store_ok: true, critical: vec![critical_media(true, true, true, None)] },
            engine_shared(10_000, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_PROBE_FAILED")),
        visual_probe_failure_is_not_ready => (
            MediaFacts { store_ok: true, critical: vec![critical_media(true, true, true, Some(false))] },
            engine_shared(10_000, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_PROBE_FAILED")),
        missing_inventory_facts_are_not_vacuously_ready => (
            MediaFacts { store_ok: true, critical: Vec::new() },
            engine_shared(10_000, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_UNMAPPED")),
        wrong_event_fact_does_not_shorten_inventory => (
            MediaFacts { store_ok: true, critical: vec![CriticalMedia {
                event_id: "risk_revision".to_owned(), mapped: true, verified: true,
                prewarmed: true, probe_ok: Some(true),
            }] },
            engine_shared(10_000, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_UNMAPPED")),
        first_duplicate_fact_wins_and_later_ready_duplicate_is_ignored => (
            MediaFacts { store_ok: true, critical: vec![
                critical_media(false, false, false, None),
                critical_media(true, true, true, Some(true)),
            ] },
            engine_shared(10_000, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_UNMAPPED")),
        noninventory_extra_fact_is_ignored => (
            MediaFacts { store_ok: true, critical: vec![
                critical_media(false, false, false, None),
                CriticalMedia {
                    event_id: "risk_alert".to_owned(), mapped: true, verified: true,
                    prewarmed: true, probe_ok: Some(true),
                },
            ] },
            engine_shared(10_000, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_UNMAPPED")),
        invalid_inventory_fact_is_media_invalid => (
            MediaFacts { store_ok: true, critical: vec![critical_media(true, false, false, None)] },
            engine_shared(10_000, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_INVALID")),
        store_failure_overrides_unmapped_when_audio_is_unmuted_and_available => (
            MediaFacts { store_ok: false, critical: vec![critical_media(false, false, false, None)] },
            engine_shared(10_000, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_INVALID")),
        future_heartbeat_saturates_to_fresh_age => (
            facts_from_table(&PrewarmTable::empty(), &|_| None, true),
            engine_shared(10_001, false, true, false, 0),
            10_000
        ) => ("not_ready", Some("MEDIA_UNMAPPED")),
    }

    #[test]
    fn generated_audio_only_clip_can_report_ready_only_as_synthetic_test_evidence() {
        // Sole ready test: a generated-clip synthetic test, not authorized critical media.
        let facts = generated_audio_only_facts();
        let shared = engine_shared(10_000, false, true, false, 0);
        assert_media_status_case(&facts, &shared, 10_000, ("ready", None), "generated clip");
    }
}
