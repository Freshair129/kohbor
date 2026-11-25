use kohbor_events::{Detector, EventId, Phase, Tick};

fn base(at_ms: u64) -> Tick {
    Tick {
        at_ms,
        session_epoch: 7,
        phase: Phase::Live,
        trusted: true,
        kills: Some(0),
        deaths: Some(0),
        kill_streak: None,
        combined_score: Some(0),
        level: Some(1),
        hp: Some(100),
        max_hp: Some(100),
        mana_percent: Some(100),
        alive: Some(true),
    }
}

fn has(tick_events: &[kohbor_events::Event], id: EventId) -> bool {
    tick_events.iter().any(|event| event.id == id)
}

#[test]
fn clutch_requires_a_live_kill_and_low_hp_in_the_recent_window() {
    let mut detector = Detector::new();
    detector.observe(&base(0));

    let mut low = base(1_000);
    low.hp = Some(10);
    assert!(
        !has(&detector.observe(&low), EventId::ClutchKill),
        "low health without a kill is not a clutch cue"
    );

    let mut healed_kill = base(2_000);
    healed_kill.hp = Some(70);
    healed_kill.kills = Some(1);
    assert!(has(&detector.observe(&healed_kill), EventId::ClutchKill));

    let mut old_low_kill = base(6_001);
    old_low_kill.hp = Some(80);
    old_low_kill.kills = Some(2);
    assert!(!has(&detector.observe(&old_low_kill), EventId::ClutchKill));

    let mut low_but_dead = base(6_100);
    low_but_dead.hp = Some(5);
    low_but_dead.kills = Some(3);
    low_but_dead.deaths = Some(1);
    low_but_dead.alive = Some(false);
    assert!(!has(&detector.observe(&low_but_dead), EventId::ClutchKill));

    let mut low_but_unknown = base(6_200);
    low_but_unknown.hp = None;
    low_but_unknown.max_hp = None;
    low_but_unknown.kills = Some(4);
    assert!(!has(
        &detector.observe(&low_but_unknown),
        EventId::ClutchKill
    ));
}

#[test]
fn clutch_uses_the_exact_ten_percent_and_three_second_boundaries() {
    let mut detector = Detector::new();
    detector.observe(&base(0));

    let mut low = base(1_000);
    low.hp = Some(10);
    detector.observe(&low);
    let mut boundary_kill = base(4_000);
    boundary_kill.kills = Some(1);
    boundary_kill.hp = Some(90);
    assert!(has(&detector.observe(&boundary_kill), EventId::ClutchKill));

    let mut expired_low = base(4_001);
    expired_low.hp = Some(10);
    detector.observe(&expired_low);
    let mut expired_kill = base(7_002);
    expired_kill.kills = Some(2);
    expired_kill.hp = Some(90);
    assert!(!has(&detector.observe(&expired_kill), EventId::ClutchKill));
}

#[test]
fn clutch_unknown_or_invalid_health_does_not_create_a_low_health_sample() {
    let mut detector = Detector::new();
    detector.observe(&base(0));

    let mut missing = base(1_000);
    missing.hp = None;
    missing.max_hp = None;
    detector.observe(&missing);
    let mut invalid = base(2_000);
    invalid.hp = Some(101);
    invalid.max_hp = Some(100);
    detector.observe(&invalid);
    let mut kill = base(2_500);
    kill.kills = Some(1);
    kill.hp = Some(90);
    assert!(!has(&detector.observe(&kill), EventId::ClutchKill));
}

#[test]
fn clutch_ignores_untrusted_health_and_kill_counter_regressions() {
    let mut detector = Detector::new();
    let mut baseline = base(0);
    baseline.kills = Some(5);
    detector.observe(&baseline);

    let mut untrusted_low = base(1_000);
    untrusted_low.trusted = false;
    untrusted_low.hp = Some(1);
    assert!(detector.observe(&untrusted_low).is_empty());
    let mut regression = base(2_000);
    regression.kills = Some(3);
    regression.hp = Some(5);
    assert!(!has(&detector.observe(&regression), EventId::ClutchKill));
    let mut next_kill = base(2_100);
    next_kill.kills = Some(4);
    next_kill.hp = Some(90);
    assert!(!has(&detector.observe(&next_kill), EventId::ClutchKill));
}

#[test]
fn posthumous_requires_a_counted_death_and_allows_same_tick_trade() {
    let mut detector = Detector::new();
    detector.observe(&base(0));

    let mut uncounted_dead = base(100);
    uncounted_dead.alive = Some(false);
    detector.observe(&uncounted_dead);
    let mut kill_without_death = base(200);
    kill_without_death.alive = Some(false);
    kill_without_death.kills = Some(1);
    assert!(!has(
        &detector.observe(&kill_without_death),
        EventId::PosthumousKill
    ));

    let mut same_tick_trade = base(300);
    same_tick_trade.alive = Some(false);
    same_tick_trade.kills = Some(2);
    same_tick_trade.deaths = Some(1);
    let events = detector.observe(&same_tick_trade);
    assert!(has(&events, EventId::HeroDeath));
    assert!(has(&events, EventId::PosthumousKill));

    let mut alive_kill = base(400);
    alive_kill.kills = Some(3);
    assert!(!has(
        &detector.observe(&alive_kill),
        EventId::PosthumousKill
    ));
}

#[test]
fn posthumous_needs_known_dead_state_and_a_counted_death() {
    let mut detector = Detector::new();
    detector.observe(&base(0));

    let mut unknown_alive = base(100);
    unknown_alive.alive = None;
    unknown_alive.kills = Some(1);
    unknown_alive.deaths = Some(1);
    assert!(!has(
        &detector.observe(&unknown_alive),
        EventId::PosthumousKill
    ));
    let mut live_kill = base(200);
    live_kill.kills = Some(2);
    live_kill.deaths = Some(1);
    assert!(!has(&detector.observe(&live_kill), EventId::PosthumousKill));
}

#[test]
fn posthumous_ignores_untrusted_death_and_death_counter_regression() {
    let mut detector = Detector::new();
    detector.observe(&base(0));

    let mut untrusted_death = base(100);
    untrusted_death.trusted = false;
    untrusted_death.alive = Some(false);
    untrusted_death.deaths = Some(1);
    assert!(detector.observe(&untrusted_death).is_empty());
    let mut untrusted_kill = base(200);
    untrusted_kill.kills = Some(1);
    untrusted_kill.alive = Some(false);
    assert!(!has(
        &detector.observe(&untrusted_kill),
        EventId::PosthumousKill
    ));

    let mut counted_deaths = base(300);
    counted_deaths.deaths = Some(3);
    counted_deaths.alive = Some(false);
    detector.observe(&counted_deaths);
    let mut regressed = base(400);
    regressed.deaths = Some(1);
    regressed.kills = Some(2);
    regressed.alive = Some(false);
    assert!(!has(&detector.observe(&regressed), EventId::PosthumousKill));
}

#[test]
fn comeback_requires_three_counted_deaths_since_the_previous_kill_and_resets() {
    let mut detector = Detector::new();
    detector.observe(&base(0));
    let mut first_kill = base(100);
    first_kill.kills = Some(1);
    detector.observe(&first_kill);

    let mut two_deaths = base(200);
    two_deaths.kills = Some(1);
    two_deaths.deaths = Some(2);
    two_deaths.alive = Some(false);
    detector.observe(&two_deaths);
    let mut ordinary_kill = base(300);
    ordinary_kill.kills = Some(2);
    ordinary_kill.deaths = Some(2);
    assert!(!has(
        &detector.observe(&ordinary_kill),
        EventId::ComebackKill
    ));

    let mut three_more_deaths = base(400);
    three_more_deaths.kills = Some(2);
    three_more_deaths.deaths = Some(5);
    three_more_deaths.alive = Some(false);
    detector.observe(&three_more_deaths);
    let mut comeback = base(500);
    comeback.kills = Some(3);
    comeback.deaths = Some(5);
    assert!(has(&detector.observe(&comeback), EventId::ComebackKill));

    let mut one_death_after = base(600);
    one_death_after.kills = Some(3);
    one_death_after.deaths = Some(6);
    one_death_after.alive = Some(false);
    detector.observe(&one_death_after);
    let mut no_stale_comeback = base(700);
    no_stale_comeback.kills = Some(4);
    no_stale_comeback.deaths = Some(6);
    assert!(!has(
        &detector.observe(&no_stale_comeback),
        EventId::ComebackKill
    ));
}

#[test]
fn comeback_ignores_untrusted_ticks_session_baselines_and_counter_regressions() {
    let mut detector = Detector::new();
    detector.observe(&base(0));

    let mut untrusted_deaths = base(100);
    untrusted_deaths.trusted = false;
    untrusted_deaths.deaths = Some(3);
    untrusted_deaths.alive = Some(false);
    assert!(detector.observe(&untrusted_deaths).is_empty());
    let mut untrusted_kill = base(200);
    untrusted_kill.trusted = false;
    untrusted_kill.kills = Some(1);
    assert!(detector.observe(&untrusted_kill).is_empty());

    let mut deaths = base(300);
    deaths.deaths = Some(3);
    deaths.alive = Some(false);
    detector.observe(&deaths);
    let mut death_regression = base(400);
    death_regression.deaths = Some(1);
    death_regression.kills = Some(1);
    assert!(!has(
        &detector.observe(&death_regression),
        EventId::ComebackKill
    ));

    let mut next_deaths = base(500);
    next_deaths.deaths = Some(4);
    next_deaths.alive = Some(false);
    detector.observe(&next_deaths);
    let mut kill_regression = base(600);
    kill_regression.kills = Some(0);
    kill_regression.deaths = Some(4);
    assert!(!has(
        &detector.observe(&kill_regression),
        EventId::ComebackKill
    ));
    let mut session_baseline = base(700);
    session_baseline.session_epoch = 8;
    session_baseline.kills = Some(4);
    session_baseline.deaths = Some(9);
    assert!(!has(
        &detector.observe(&session_baseline),
        EventId::ComebackKill
    ));
    let mut kill_after_reconnect = session_baseline.clone();
    kill_after_reconnect.at_ms = 800;
    kill_after_reconnect.kills = Some(5);
    assert!(!has(
        &detector.observe(&kill_after_reconnect),
        EventId::ComebackKill
    ));
}

#[test]
fn special_event_ranks_match_the_approved_priority() {
    assert_eq!(EventId::ClutchKill.rank(), Some(75));
    assert_eq!(EventId::PosthumousKill.rank(), Some(65));
    assert_eq!(EventId::ComebackKill.rank(), Some(62));
}

#[test]
fn special_detectors_do_not_emit_outside_live_phase() {
    let mut detector = Detector::new();
    detector.observe(&base(0));
    let mut postgame = base(100);
    postgame.phase = Phase::PostGame;
    postgame.kills = Some(4);
    postgame.deaths = Some(3);
    postgame.alive = Some(false);
    postgame.hp = Some(1);
    assert!(!detector.observe(&postgame).iter().any(|event| matches!(
        event.id,
        EventId::ClutchKill | EventId::PosthumousKill | EventId::ComebackKill
    )));
}
