use kohbor_events::{Detector, Event, EventId, Phase, Tick};

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
fn alive_flicker_without_death_keeps_streak_and_emits_no_death_or_respawn() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    detector.observe(&current);

    current.at_ms = 100;
    current.kills = Some(3);
    assert_eq!(tier(&detector.observe(&current), EventId::Streak), Some(3));

    current.at_ms = 200;
    current.alive = Some(false);
    let down = detector.observe(&current);
    assert!(!has(&down, EventId::HeroDeath));
    assert!(!has(&down, EventId::HeroRespawn));

    current.at_ms = 300;
    current.alive = Some(true);
    let up = detector.observe(&current);
    assert!(!has(&up, EventId::HeroDeath));
    assert!(!has(&up, EventId::HeroRespawn));

    current.at_ms = 400;
    current.kills = Some(4);
    assert_eq!(tier(&detector.observe(&current), EventId::Streak), Some(4));
}

#[test]
fn death_counter_can_lead_alive_and_is_counted_once_with_streak_reset() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    detector.observe(&current);
    current.at_ms = 100;
    current.kills = Some(3);
    assert_eq!(tier(&detector.observe(&current), EventId::Streak), Some(3));

    current.at_ms = 200;
    current.deaths = Some(1);
    let death = detector.observe(&current);
    assert_eq!(
        death
            .iter()
            .filter(|event| event.id == EventId::HeroDeath)
            .count(),
        1
    );
    assert!(!has(&death, EventId::HeroRespawn));

    current.at_ms = 300;
    current.alive = Some(false);
    let delayed_alive_edge = detector.observe(&current);
    assert!(!has(&delayed_alive_edge, EventId::HeroDeath));

    current.at_ms = 400;
    current.alive = Some(true);
    assert!(has(&detector.observe(&current), EventId::HeroRespawn));

    current.at_ms = 500;
    current.kills = Some(4);
    assert_eq!(tier(&detector.observe(&current), EventId::Streak), None);
    current.at_ms = 600;
    current.kills = Some(6);
    assert_eq!(tier(&detector.observe(&current), EventId::Streak), Some(3));
}

#[test]
fn separate_death_counter_increases_emit_one_death_each() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    detector.observe(&current);

    let mut death_events = 0;
    for (at_ms, deaths) in [(100, 1), (200, 2)] {
        current.at_ms = at_ms;
        current.deaths = Some(deaths);
        let events = detector.observe(&current);
        death_events += events
            .iter()
            .filter(|event| event.id == EventId::HeroDeath)
            .count();
    }
    assert_eq!(death_events, 2);
}

#[test]
fn a_multi_death_counter_jump_emits_one_event_at_that_tick() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    detector.observe(&current);

    current.at_ms = 100;
    current.deaths = Some(2);
    let events = detector.observe(&current);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.id == EventId::HeroDeath)
            .count(),
        1
    );
}

#[test]
fn death_counter_regression_rebaselines_without_death() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.deaths = Some(2);
    detector.observe(&current);

    current.at_ms = 100;
    current.kills = Some(3);
    assert_eq!(tier(&detector.observe(&current), EventId::Streak), Some(3));

    current.at_ms = 200;
    current.deaths = Some(1);
    current.alive = Some(false);
    let regression = detector.observe(&current);
    assert!(!has(&regression, EventId::HeroDeath));
    assert!(!has(&regression, EventId::HeroRespawn));

    current.at_ms = 300;
    current.deaths = Some(2);
    current.alive = Some(true);
    assert!(has(&detector.observe(&current), EventId::HeroDeath));

    current.at_ms = 400;
    current.kills = Some(4);
    assert_eq!(tier(&detector.observe(&current), EventId::Streak), None);
}

#[test]
fn missing_death_counter_falls_back_to_alive_transition() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.deaths = None;
    detector.observe(&current);

    current.at_ms = 100;
    current.alive = Some(false);
    assert!(has(&detector.observe(&current), EventId::HeroDeath));

    current.at_ms = 200;
    current.alive = Some(true);
    assert!(has(&detector.observe(&current), EventId::HeroRespawn));
}

#[test]
fn a_missing_counter_on_either_tick_uses_the_alive_fallback() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    detector.observe(&current);

    current.at_ms = 100;
    current.deaths = None;
    current.alive = Some(false);
    assert!(has(&detector.observe(&current), EventId::HeroDeath));

    current.at_ms = 200;
    current.deaths = Some(1);
    current.alive = Some(true);
    assert!(has(&detector.observe(&current), EventId::HeroRespawn));
}

#[test]
fn respawn_requires_a_counted_death_since_the_last_respawn() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.alive = Some(false);
    detector.observe(&current);

    current.at_ms = 100;
    current.alive = Some(true);
    assert!(!has(&detector.observe(&current), EventId::HeroRespawn));
    current.at_ms = 200;
    current.alive = Some(false);
    assert!(!has(&detector.observe(&current), EventId::HeroDeath));
    current.at_ms = 300;
    current.alive = Some(true);
    assert!(!has(&detector.observe(&current), EventId::HeroRespawn));

    current.at_ms = 400;
    current.deaths = Some(1);
    assert!(has(&detector.observe(&current), EventId::HeroDeath));
    current.at_ms = 500;
    current.alive = Some(false);
    assert!(!has(&detector.observe(&current), EventId::HeroDeath));
    current.at_ms = 600;
    current.alive = Some(true);
    assert!(has(&detector.observe(&current), EventId::HeroRespawn));
    current.at_ms = 700;
    assert!(!has(&detector.observe(&current), EventId::HeroRespawn));
}

#[test]
fn health_and_mana_rearm_only_on_a_real_respawn() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    detector.observe(&current);

    current.at_ms = 100;
    current.hp = Some(150);
    current.mana_percent = Some(10);
    let low = detector.observe(&current);
    assert!(has(&low, EventId::HealthLow));
    assert!(has(&low, EventId::ManaLow));

    current.at_ms = 200;
    current.alive = Some(false);
    current.hp = Some(1000);
    current.mana_percent = Some(100);
    detector.observe(&current);
    current.at_ms = 300;
    current.alive = Some(true);
    current.hp = Some(150);
    current.mana_percent = Some(10);
    let flicker = detector.observe(&current);
    assert!(!has(&flicker, EventId::HealthLow));
    assert!(!has(&flicker, EventId::ManaLow));

    current.at_ms = 400;
    current.deaths = Some(1);
    current.alive = Some(false);
    detector.observe(&current);
    current.at_ms = 500;
    current.alive = Some(true);
    let respawn = detector.observe(&current);
    assert!(has(&respawn, EventId::HeroRespawn));
    assert!(has(&respawn, EventId::HealthLow));
    assert!(has(&respawn, EventId::ManaLow));
}

#[test]
fn counted_death_clears_the_health_danger_sample_window() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.hp = Some(800);
    detector.observe(&current);

    current.at_ms = 100;
    current.hp = Some(600);
    assert!(!has(&detector.observe(&current), EventId::HealthDanger));

    current.at_ms = 200;
    current.deaths = Some(1);
    current.hp = Some(200);
    assert!(!has(&detector.observe(&current), EventId::HealthDanger));

    current.at_ms = 300;
    current.hp = Some(800);
    detector.observe(&current);
    current.at_ms = 400;
    current.hp = Some(200);
    assert!(has(&detector.observe(&current), EventId::HealthDanger));
}

#[test]
fn epoch_change_sets_the_death_counter_baseline_silently() {
    let mut detector = Detector::new();
    let mut current = tick(0);
    current.deaths = Some(4);
    current.alive = Some(false);
    detector.observe(&current);

    current.at_ms = 100;
    current.session_epoch = 2;
    current.deaths = Some(0);
    current.alive = Some(true);
    assert!(detector.observe(&current).is_empty());
}
