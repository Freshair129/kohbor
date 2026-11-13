use super::resolve::{class_of, instance_id, Clip, Entry, Lookup, PrewarmTable};
use kohbor_audio::AudioClass;
use kohbor_events::EventId;
use serde_json::Value;
use std::{collections::HashSet, sync::Arc};

const IDS: [EventId; 12] = [
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
// An exhaustive match forces additions to EventId to be reviewed here.
fn known(id: EventId) {
    match id {
        EventId::MultiKill
        | EventId::Streak
        | EventId::FirstBlood
        | EventId::HeroKill
        | EventId::HeroDeath
        | EventId::LevelMilestone
        | EventId::HealthLow
        | EventId::ManaLow
        | EventId::MatchEnded
        | EventId::MatchStarted
        | EventId::HeroRespawn
        | EventId::HealthDanger => {}
    }
}
fn clip(tier: Option<u8>, marker: i16) -> Clip {
    Clip {
        pcm: Arc::from([marker, -marker]),
        bound_tier: tier,
        audio_ms: 37,
        visual_tail_ms: 99,
        visual_url: Some(Arc::from(
            "kohbor-media://localhost/v1/ins_0000000001/abcdef",
        )),
    }
}
fn table(entries: Vec<(EventId, Option<u8>, Entry)>) -> PrewarmTable {
    PrewarmTable::new(7, Some(Arc::from("ins_0000000007")), entries)
}
fn ready(tier: Option<u8>, marker: i16) -> Entry {
    Entry::Ready(clip(tier, marker))
}
fn assert_ready(
    table: &PrewarmTable,
    id: EventId,
    tier: Option<u8>,
    expected: Option<u8>,
    marker: i16,
) {
    match table.lookup(id, tier) {
        Lookup::Ready(c) => {
            assert_eq!(c.bound_tier, expected, "{id:?} {tier:?}");
            assert_eq!(c.pcm[0], marker, "{id:?} {tier:?}");
        }
        other => panic!("expected ready for {id:?} {tier:?}, got {other:?}"),
    }
}
#[test]
fn classes_cover_all_events() {
    for id in IDS {
        known(id);
        assert_eq!(
            class_of(id),
            if id.is_critical() {
                AudioClass::Critical
            } else {
                AudioClass::Ordinary
            },
            "{id:?}"
        );
    }
}
#[test]
fn instance_ids_match_format_and_are_unique() {
    let mut seen = HashSet::new();
    for g in [0, 1, u64::MAX] {
        for s in [0, 1, u64::MAX] {
            for i in [0, 1, usize::MAX] {
                let id = instance_id(g, s, i);
                assert_eq!(id, format!("ev_g{g}_s{s}_{i}"), "identity {g} {s} {i}");
                assert!(
                    (8..=96).contains(&id.len())
                        && id
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
                    "invalid id {id}"
                );
                assert!(seen.insert(id), "duplicate {g} {s} {i}");
            }
        }
    }
}
#[test]
fn empty_table_is_unmapped_and_not_ready() {
    let t = PrewarmTable::empty();
    assert_eq!(t.fence, 0);
    assert!(t.install_id.is_none());
    for id in IDS {
        assert!(matches!(t.lookup(id, None), Lookup::Unmapped), "{id:?}");
        assert!(
            matches!(t.lookup(id, Some(255)), Lookup::Unmapped),
            "{id:?}"
        );
        assert!(!t.has_ready_tierless(id), "{id:?}");
    }
}
#[test]
fn new_preserves_identity_and_replaces_duplicate() {
    let t = table(vec![
        (EventId::Streak, None, ready(None, 1)),
        (EventId::MultiKill, Some(2), ready(Some(2), 2)),
        (EventId::Streak, None, ready(None, 3)),
    ]);
    assert_eq!(t.fence, 7);
    assert_eq!(t.install_id.as_deref(), Some("ins_0000000007"));
    assert_ready(&t, EventId::Streak, None, None, 3);
    assert_ready(&t, EventId::MultiKill, Some(2), Some(2), 2);
}
#[test]
fn highest_eligible_tier_wins() {
    let t = table(vec![
        (EventId::MultiKill, None, ready(None, 1)),
        (EventId::MultiKill, Some(2), ready(Some(2), 2)),
        (EventId::MultiKill, Some(3), ready(Some(3), 3)),
        (EventId::MultiKill, Some(5), ready(Some(5), 5)),
    ]);
    for (input, expected) in [(2, 2), (3, 3), (4, 3), (5, 5), (10, 5)] {
        assert_ready(
            &t,
            EventId::MultiKill,
            Some(input),
            Some(expected),
            i16::from(expected),
        );
    }
    assert_ready(&t, EventId::MultiKill, Some(1), None, 1);
}
#[test]
fn tierless_covers_any_tier_without_eligible_binding() {
    let t = table(vec![(EventId::Streak, None, ready(None, 1))]);
    for n in [0, 1, 2, 255] {
        assert_ready(&t, EventId::Streak, Some(n), None, 1);
    }
}
#[test]
fn none_only_selects_tierless() {
    let t = table(vec![(EventId::Streak, Some(3), ready(Some(3), 3))]);
    assert!(matches!(t.lookup(EventId::Streak, None), Lookup::Unmapped));
}
#[test]
fn below_bound_without_tierless_is_unmapped() {
    let t = table(vec![(EventId::Streak, Some(3), ready(Some(3), 3))]);
    assert!(matches!(
        t.lookup(EventId::Streak, Some(2)),
        Lookup::Unmapped
    ));
}
#[test]
fn invalid_selected_tier_does_not_fall_back() {
    let t = table(vec![
        (EventId::Streak, Some(3), ready(Some(3), 3)),
        (EventId::Streak, Some(5), Entry::Invalid),
    ]);
    assert!(matches!(
        t.lookup(EventId::Streak, Some(6)),
        Lookup::Invalid
    ));
}
#[test]
fn invalid_tierless_is_invalid() {
    let t = table(vec![(EventId::Streak, None, Entry::Invalid)]);
    for tier in [None, Some(1)] {
        assert!(
            matches!(t.lookup(EventId::Streak, tier), Lookup::Invalid),
            "{tier:?}"
        );
    }
}
#[test]
fn event_ids_are_independent() {
    let t = table(vec![(EventId::MultiKill, None, ready(None, 1))]);
    assert!(matches!(t.lookup(EventId::Streak, None), Lookup::Unmapped));
    assert!(matches!(
        t.lookup(EventId::Streak, Some(4)),
        Lookup::Unmapped
    ));
}
#[test]
fn ready_preserves_entire_clip() {
    let t = table(vec![(EventId::Streak, Some(3), ready(Some(3), 7))]);
    let Lookup::Ready(c) = t.lookup(EventId::Streak, Some(3)) else {
        panic!("expected ready")
    };
    assert_eq!(&*c.pcm, &[7, -7]);
    assert_eq!(c.audio_ms, 37);
    assert_eq!(c.visual_tail_ms, 99);
    assert_eq!(
        c.visual_url.as_deref(),
        Some("kohbor-media://localhost/v1/ins_0000000001/abcdef")
    );
    assert_eq!(c.bound_tier, Some(3));
}
#[test]
fn readiness_requires_ready_tierless() {
    let t = table(vec![
        (EventId::Streak, None, Entry::Invalid),
        (EventId::MultiKill, Some(2), ready(Some(2), 2)),
        (EventId::HeroKill, None, ready(None, 1)),
    ]);
    assert!(!t.has_ready_tierless(EventId::Streak));
    assert!(!t.has_ready_tierless(EventId::MultiKill));
    assert!(t.has_ready_tierless(EventId::HeroKill));
}
#[test]
fn catalog_key_space_is_resolved() {
    let catalog: Value =
        serde_json::from_str(include_str!("../../../../../contracts/event-catalog.json")).unwrap();
    let mut entries = Vec::new();
    for id in IDS.into_iter().filter(|id| id.voiced()) {
        let event = catalog["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id.as_str())
            .unwrap();
        entries.push((id, None, ready(None, 1)));
        if let Some(range) = event.get("tier_range") {
            for tier in range["min"].as_u64().unwrap()..=range["max"].as_u64().unwrap() {
                let tier = u8::try_from(tier).unwrap();
                entries.push((id, Some(tier), ready(Some(tier), i16::from(tier))));
            }
        }
    }
    assert_eq!(entries.len(), 23, "catalog key count changed");
    let keys: Vec<_> = entries.iter().map(|(id, tier, _)| (*id, *tier)).collect();
    let t = table(entries);
    for (id, tier) in keys {
        assert_ready(&t, id, tier, tier, tier.map_or(1, i16::from));
    }
}
