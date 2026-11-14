use kohbor_events::{adapter::tick_from_admitted, Detector, Event, EventId, Phase, Tick};
use kohbor_gsi::admission::{
    Admitted, Field, HeroFields, OwnCounters, Ownership, Phase as GsiPhase,
};

fn tick(at_ms: u64) -> Tick {
    Tick {
        at_ms,
        session_epoch: 1,
        phase: Phase::Live,
        trusted: true,
        kills: Some(0),
        deaths: Some(0),
        kill_streak: None,
        combined_score: Some(0),
        level: Some(1),
        hp: Some(1000),
        max_hp: Some(1000),
        mana_percent: Some(100),
        alive: Some(true),
    }
}

fn tier(events: &[Event]) -> Option<u8> {
    events
        .iter()
        .find(|event| event.id == EventId::Streak)
        .and_then(|event| event.tier)
}

fn catalog_streak_max() -> u8 {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("contracts/event-catalog.json")).unwrap())
            .unwrap();
    catalog["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["id"] == "streak")
        .unwrap()["tier_range"]["max"]
        .as_u64()
        .unwrap() as u8
}

#[test]
fn observed_streak_emits_after_a_nonzero_baseline() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.kills = Some(5);
    current.kill_streak = Some(2);
    assert!(detector.observe(&current).is_empty());

    current.at_ms = 100;
    current.kills = Some(6);
    current.kill_streak = Some(3);
    assert_eq!(tier(&detector.observe(&current)), Some(3));
}

#[test]
fn observed_zero_suppresses_computed_streak_on_a_kill_increase() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    detector.observe(&current);

    current.at_ms = 100;
    current.kills = Some(3);
    current.kill_streak = Some(3);
    assert_eq!(tier(&detector.observe(&current)), Some(3));

    current.at_ms = 200;
    current.kills = Some(4);
    current.kill_streak = Some(0);
    assert_eq!(tier(&detector.observe(&current)), None);
}

#[test]
fn two_kill_jump_uses_the_observed_streak_tier() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.kills = Some(5);
    current.kill_streak = Some(1);
    detector.observe(&current);

    current.at_ms = 100;
    current.kills = Some(7);
    current.kill_streak = Some(3);
    assert_eq!(tier(&detector.observe(&current)), Some(3));
}

#[test]
fn observed_streak_tier_is_capped_at_catalog_maximum() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.kills = Some(10);
    current.kill_streak = Some(11);
    detector.observe(&current);

    current.at_ms = 100;
    current.kills = Some(11);
    current.kill_streak = Some(12);
    assert_eq!(
        tier(&detector.observe(&current)),
        Some(catalog_streak_max())
    );
}

#[test]
fn observed_streak_regression_without_death_is_a_silent_reset() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.kills = Some(5);
    current.kill_streak = Some(5);
    detector.observe(&current);

    current.at_ms = 100;
    current.kills = Some(6);
    current.kill_streak = Some(4);
    assert_eq!(tier(&detector.observe(&current)), None);

    current.at_ms = 200;
    current.kills = Some(7);
    current.kill_streak = Some(5);
    assert_eq!(tier(&detector.observe(&current)), Some(5));
}

#[test]
fn missing_observed_streak_uses_computed_path_and_keeps_unknown_baselines() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    detector.observe(&current);
    current.at_ms = 100;
    current.kills = Some(3);
    assert_eq!(tier(&detector.observe(&current)), Some(3));

    let mut unknown_detector = Detector::new();
    let mut unknown = tick(0);
    unknown.kills = Some(5);
    unknown_detector.observe(&unknown);
    unknown.at_ms = 100;
    unknown.kills = Some(6);
    assert_eq!(tier(&unknown_detector.observe(&unknown)), None);
}

#[test]
fn adapter_maps_deaths_and_streak_with_checked_narrowing() {
    let admitted = |deaths: Field<i64>, kill_streak: Field<i64>| Admitted {
        phase: GsiPhase::Live,
        ownership: Ownership::Verified,
        team: None,
        match_id: None,
        steamid64: None,
        counters: OwnCounters {
            gold: Field::Missing,
            kills: Field::Value(7),
            deaths,
            kill_streak,
            assists: Field::Missing,
            last_hits: Field::Missing,
            denies: Field::Missing,
            gpm: Field::Missing,
            xpm: Field::Missing,
            combined_score: Field::Value(8),
        },
        hero: HeroFields {
            hero_id: Field::Missing,
            level: Field::Missing,
            hp: Field::Missing,
            max_hp: Field::Missing,
            mana_percent: Field::Missing,
            alive: Field::Missing,
            respawn_seconds: Field::Missing,
        },
    };

    let mapped = tick_from_admitted(&admitted(Field::Value(2), Field::Value(3)), 0, 1, true);
    assert_eq!(mapped.deaths, Some(2));
    assert_eq!(mapped.kill_streak, Some(3));

    let too_large = i64::from(u32::MAX) + 1;
    let unknown = tick_from_admitted(
        &admitted(Field::Value(too_large), Field::Value(-1)),
        1,
        1,
        true,
    );
    assert_eq!(unknown.deaths, None);
    assert_eq!(unknown.kill_streak, None);
}
