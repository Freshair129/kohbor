use kohbor_events::{Config, Detector, Event, EventId, Phase, Tick};

fn tick(at_ms: u64) -> Tick {
    Tick {
        at_ms,
        session_epoch: 1,
        phase: Phase::Live,
        trusted: true,
        kills: Some(0),
        deaths: None,
        kill_streak: None,
        combined_score: Some(0),
        level: Some(1),
        hp: Some(1000),
        max_hp: Some(1000),
        mana_percent: Some(100),
        alive: Some(true),
    }
}

fn has(events: &[Event], id: EventId) -> bool {
    events.iter().any(|event| event.id == id)
}

fn tier(events: &[Event], id: EventId) -> Option<u8> {
    events
        .iter()
        .find(|event| event.id == id)
        .and_then(|event| event.tier)
}

#[test]
fn streak_is_unknown_after_nonzero_kill_baseline() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.kills = Some(5);
    assert!(detector.observe(&current).is_empty());

    current.at_ms = 100;
    current.kills = Some(8);
    let events = detector.observe(&current);

    assert!(!has(&events, EventId::Streak));
    assert_eq!(tier(&events, EventId::MultiKill), Some(3));
}

#[test]
fn streak_starts_at_zero_after_zero_kill_baseline() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    assert!(detector.observe(&current).is_empty());

    for kills in 1..=3 {
        current.at_ms = u64::from(kills) * 100;
        current.kills = Some(kills);
        let events = detector.observe(&current);
        assert_eq!(tier(&events, EventId::Streak), (kills == 3).then_some(3));
    }
}

#[test]
fn streak_becomes_known_after_own_death() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.kills = Some(5);
    assert!(detector.observe(&current).is_empty());

    current.at_ms = 100;
    current.alive = Some(false);
    assert!(has(&detector.observe(&current), EventId::HeroDeath));

    current.alive = Some(true);
    for kills in 6..=8 {
        current.at_ms += 100;
        current.kills = Some(kills);
        let events = detector.observe(&current);
        assert_eq!(tier(&events, EventId::Streak), (kills == 8).then_some(3));
    }
}

#[test]
fn late_tick_does_not_clear_health_danger_cooldown() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    assert!(detector.observe(&current).is_empty());

    current.at_ms = 1_000;
    current.hp = Some(200);
    assert!(has(&detector.observe(&current), EventId::HealthDanger));

    current.at_ms = 500;
    current.hp = Some(1_000);
    assert!(detector.observe(&current).is_empty());

    current.at_ms = 1_500;
    current.hp = Some(200);
    assert!(!has(&detector.observe(&current), EventId::HealthDanger));
}

#[test]
fn death_tick_does_not_emit_low_resource_alerts() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    assert!(detector.observe(&current).is_empty());

    current.at_ms = 100;
    current.alive = Some(false);
    current.hp = Some(0);
    current.mana_percent = Some(0);
    let events = detector.observe(&current);

    assert!(has(&events, EventId::HeroDeath));
    assert!(!has(&events, EventId::HealthLow));
    assert!(!has(&events, EventId::ManaLow));
    assert_eq!(events.len(), 1);
}

#[test]
fn hp_above_max_is_not_a_danger_sample() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.hp = Some(2_000);
    current.max_hp = Some(1_000);
    assert!(detector.observe(&current).is_empty());

    current.at_ms = 100;
    current.hp = Some(240);
    let events = detector.observe(&current);

    assert!(!has(&events, EventId::HealthDanger));
}

#[test]
fn invalid_hp_does_not_rearm_low_health() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    assert!(detector.observe(&current).is_empty());

    current.at_ms = 100;
    current.hp = Some(180);
    assert!(has(&detector.observe(&current), EventId::HealthLow));

    current.at_ms = 200;
    current.hp = Some(2_000);
    current.max_hp = Some(1_000);
    assert!(detector.observe(&current).is_empty());

    current.at_ms = 300;
    current.hp = Some(180);
    let events = detector.observe(&current);

    assert!(!has(&events, EventId::HealthLow));
}

#[test]
fn multi_kill_minimum_is_configurable() {
    let config = Config {
        multi_kill_min_kills: 4,
        ..Config::default()
    };
    let mut detector = Detector::with_config(config);
    let mut current = tick(0);
    assert!(detector.observe(&current).is_empty());

    current.at_ms = 100;
    current.kills = Some(2);
    assert!(!has(&detector.observe(&current), EventId::MultiKill));

    current.at_ms = 200;
    current.kills = Some(4);
    assert_eq!(
        tier(&detector.observe(&current), EventId::MultiKill),
        Some(4)
    );
}

#[test]
fn streak_minimum_is_configurable() {
    let config = Config {
        streak_min_kills: 5,
        ..Config::default()
    };
    let mut detector = Detector::with_config(config);
    let mut current = tick(0);
    assert!(detector.observe(&current).is_empty());

    for kills in 1..=5 {
        current.at_ms = u64::from(kills) * 100;
        current.kills = Some(kills);
        let events = detector.observe(&current);
        assert_eq!(tier(&events, EventId::Streak), (kills == 5).then_some(5));
    }
}

#[test]
fn tier_minimum_defaults_match_the_catalog() {
    let config = Config::default();

    assert_eq!(config.multi_kill_min_kills, 2);
    assert_eq!(config.streak_min_kills, 3);
}
