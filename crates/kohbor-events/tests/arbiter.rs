use kohbor_events::{arbitrate, DropReason, Event, EventId};
use serde_json::Value;
use std::{fs, path::PathBuf};

fn event(id: EventId, tier: Option<u8>, at_ms: u64) -> Event {
    Event { id, tier, at_ms }
}

#[test]
fn highest_rank_wins_in_any_input_order() {
    let ids = [
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
    ];
    for (i, &winner) in ids.iter().enumerate() {
        for &loser in &ids[i + 1..] {
            for input in [
                [event(winner, None, 1), event(loser, None, 1)],
                [event(loser, None, 1), event(winner, None, 1)],
            ] {
                let result = arbitrate(&input);
                assert_eq!(result.winner, Some(event(winner, None, 1)));
                assert_eq!(result.dropped.len(), 1);
                assert_eq!(result.dropped[0].event, event(loser, None, 1));
                assert_eq!(result.dropped[0].reason, DropReason::LowerRankSameTick);
            }
        }
    }
}

#[test]
fn unvoiced_and_critical_events_are_excluded() {
    let respawn = event(EventId::HeroRespawn, None, 1);
    let danger = event(EventId::HealthDanger, None, 1);
    let kill = event(EventId::HeroKill, None, 1);
    for input in [vec![], vec![respawn], vec![danger], vec![respawn, danger]] {
        let result = arbitrate(&input);
        assert_eq!(result.winner, None);
        assert!(result.dropped.is_empty());
    }
    let result = arbitrate(&[danger, kill, respawn]);
    assert_eq!(result.winner, Some(kill));
    assert!(result.dropped.is_empty());
}

#[test]
fn duplicate_id_keeps_first_and_drops_second() {
    let first = event(EventId::HeroKill, None, 1);
    let second = event(EventId::HeroKill, None, 2);
    let result = arbitrate(&[first, second]);
    assert_eq!(result.winner, Some(first));
    assert_eq!(result.dropped.len(), 1);
    assert_eq!(result.dropped[0].event, second);
    assert_eq!(result.dropped[0].reason, DropReason::LowerRankSameTick);
}

#[test]
fn multi_kill_outranks_streak_at_every_tier() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/event-catalog.json");
    let catalog: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let range = |id: &str| {
        let row = catalog["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap();
        (
            row["tier_range"]["min"].as_u64().unwrap() as u8,
            row["tier_range"]["max"].as_u64().unwrap() as u8,
        )
    };
    let (multi_min, multi_max) = range("multi_kill");
    let (streak_min, streak_max) = range("streak");
    for multi in multi_min..=multi_max {
        for streak in streak_min..=streak_max {
            let m = event(EventId::MultiKill, Some(multi), 1);
            let s = event(EventId::Streak, Some(streak), 1);
            let result = arbitrate(&[s, m]);
            assert_eq!(result.winner, Some(m));
            assert_eq!(result.dropped.len(), 1);
            assert_eq!(result.dropped[0].event, s);
            assert_eq!(result.dropped[0].reason, DropReason::LowerRankSameTick);
        }
    }
}
