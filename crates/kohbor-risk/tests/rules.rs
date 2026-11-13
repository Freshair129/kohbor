use kohbor_capture::Coverage;
use kohbor_risk::{Meter, OwnGsi, RiskEngine, RiskEvent, RiskOutput, TickInput};

fn own(hp_pct: Option<u8>) -> Option<OwnGsi> {
    own_state(true, hp_pct, true, 0)
}

fn dead_own(hp_pct: Option<u8>) -> Option<OwnGsi> {
    own_state(false, hp_pct, true, 0)
}

fn own_state(alive: bool, hp_pct: Option<u8>, in_match: bool, age_ms: u64) -> Option<OwnGsi> {
    Some(OwnGsi {
        alive,
        hp_pct,
        in_match,
        age_ms,
    })
}

fn tick(
    engine: &mut RiskEngine,
    now_ms: u64,
    coverage: Coverage,
    visible: Option<u8>,
    own: Option<OwnGsi>,
    gsi_events: &[&'static str],
) -> RiskOutput {
    engine.tick(&TickInput {
        now_ms,
        coverage,
        visible,
        own,
        gsi_events: gsi_events.to_vec(),
    })
}

fn valid_tick(
    engine: &mut RiskEngine,
    now_ms: u64,
    visible: Option<u8>,
    own: Option<OwnGsi>,
) -> RiskOutput {
    tick(engine, now_ms, Coverage::Valid, visible, own, &[])
}

fn continue_visible_until(
    engine: &mut RiskEngine,
    start_ms: u64,
    end_ms: u64,
    visible: u8,
    own: Option<OwnGsi>,
) -> RiskOutput {
    let mut now_ms = start_ms.saturating_add(100);
    while now_ms < end_ms {
        valid_tick(engine, now_ms, Some(visible), own);
        now_ms = now_ms.saturating_add(100);
    }
    valid_tick(engine, end_ms, Some(visible), own)
}

fn seed(engine: &mut RiskEngine, visible: u8, own: Option<OwnGsi>) -> RiskOutput {
    let mut output = RiskOutput {
        meter: Meter::Unknown,
        gsi_source: false,
        vision_source: false,
        visible: None,
        baseline: None,
        events: Vec::new(),
    };
    for index in 0..5 {
        output = valid_tick(engine, index * 100, Some(visible), own);
    }
    output
}

fn start_one_count_drop(engine: &mut RiskEngine, own: Option<OwnGsi>) -> RiskOutput {
    seed(engine, 5, own);
    valid_tick(engine, 500, Some(4), own);
    valid_tick(engine, 600, Some(4), own);
    valid_tick(engine, 700, Some(4), own)
}

fn start_two_count_drop(engine: &mut RiskEngine, own: Option<OwnGsi>) -> RiskOutput {
    seed(engine, 5, own);
    valid_tick(engine, 500, Some(4), own);
    valid_tick(engine, 600, Some(4), own);
    valid_tick(engine, 700, Some(4), own);
    valid_tick(engine, 800, Some(3), own);
    valid_tick(engine, 900, Some(3), own);
    valid_tick(engine, 1_000, Some(3), own)
}

fn has(output: &RiskOutput, event: RiskEvent) -> bool {
    output.events.contains(&event)
}

#[test]
fn meter_is_unknown_without_fresh_own_state_even_when_vision_is_valid() {
    let mut engine = RiskEngine::new();
    seed(&mut engine, 5, None);

    let output = valid_tick(&mut engine, 500, Some(3), None);

    assert_eq!(output.meter, Meter::Unknown);
    assert!(output.vision_source);
    assert!(!output.gsi_source);
}

#[test]
fn gsi_is_fresh_at_2000ms_and_unknown_at_2001ms() {
    let mut engine = RiskEngine::new();
    let fresh = tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own_state(true, Some(100), true, 2_000),
        &[],
    );
    let stale = tick(
        &mut engine,
        1,
        Coverage::Unavailable,
        None,
        own_state(true, Some(100), true, 2_001),
        &[],
    );

    assert_eq!(fresh.meter, Meter::Level(0));
    assert_eq!(stale.meter, Meter::Unknown);
    assert!(!stale.gsi_source);
}

#[test]
fn out_of_match_own_state_makes_meter_unknown() {
    let mut engine = RiskEngine::new();

    let output = tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own_state(true, Some(100), false, 0),
        &[],
    );

    assert_eq!(output.meter, Meter::Unknown);
    assert!(!output.gsi_source);
}

#[test]
fn fresh_gsi_without_hp_and_without_other_signal_is_unknown() {
    let mut engine = RiskEngine::new();

    let output = tick(&mut engine, 0, Coverage::Stale, Some(5), own(None), &[]);

    assert_eq!(output.meter, Meter::Unknown);
    assert!(output.gsi_source);
    assert!(!output.vision_source);
}

#[test]
fn gsi_hp_at_50_maps_to_level_one() {
    let mut engine = RiskEngine::new();

    let output = tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own(Some(50)),
        &[],
    );

    assert_eq!(output.meter, Meter::Level(1));
}

#[test]
fn gsi_hp_above_50_maps_to_level_zero() {
    let mut engine = RiskEngine::new();

    let output = tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own(Some(51)),
        &[],
    );

    assert_eq!(output.meter, Meter::Level(0));
}

#[test]
fn gsi_hp_at_30_maps_to_level_two() {
    let mut engine = RiskEngine::new();

    let output = tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own(Some(30)),
        &[],
    );

    assert_eq!(output.meter, Meter::Level(2));
}

#[test]
fn recent_health_low_maps_to_level_two() {
    let mut engine = RiskEngine::new();

    let output = tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &["health_low"],
    );

    assert_eq!(output.meter, Meter::Level(2));
}

#[test]
fn fresh_gsi_without_hp_can_use_a_recent_health_event_signal() {
    let mut engine = RiskEngine::new();

    let output = tick(
        &mut engine,
        0,
        Coverage::Stale,
        None,
        own(None),
        &["health_low"],
    );

    assert_eq!(output.meter, Meter::Level(2));
}

#[test]
fn recent_health_danger_maps_to_level_three() {
    let mut engine = RiskEngine::new();

    let output = tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &["health_danger"],
    );

    assert_eq!(output.meter, Meter::Level(3));
}

#[test]
fn recent_health_event_includes_the_5000ms_boundary() {
    let mut engine = RiskEngine::new();
    tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &["health_low"],
    );

    let output = tick(
        &mut engine,
        5_000,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &[],
    );

    assert_eq!(output.meter, Meter::Level(2));
}

#[test]
fn recent_health_event_expires_after_5000ms() {
    let mut engine = RiskEngine::new();
    tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &["health_danger"],
    );

    let output = tick(
        &mut engine,
        5_001,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &[],
    );

    assert_eq!(output.meter, Meter::Level(0));
}

#[test]
fn v_is_the_median_of_the_last_five_raw_counts() {
    let mut engine = RiskEngine::new();
    for (index, count) in [2, 5, 5, 3, 4].into_iter().enumerate() {
        valid_tick(&mut engine, index as u64 * 100, Some(count), own(None));
    }

    let output = valid_tick(&mut engine, 500, Some(4), own(None));

    assert_eq!(output.baseline, Some(4));
    assert_eq!(output.visible, Some(4));
}

#[test]
fn v_is_unknown_until_five_valid_counts_arrive() {
    let mut engine = RiskEngine::new();
    for index in 0..4 {
        let output = valid_tick(&mut engine, index * 100, Some(5), own(Some(100)));
        assert_eq!(output.baseline, None);
    }

    let output = valid_tick(&mut engine, 400, Some(5), own(Some(100)));

    assert_eq!(output.baseline, Some(5));
}

#[test]
fn b_is_the_maximum_of_v_values_in_its_20_second_window() {
    let mut engine = RiskEngine::new();
    seed(&mut engine, 5, own(Some(100)));
    for index in 0..6 {
        valid_tick(&mut engine, 500 + index * 100, Some(2), own(Some(100)));
    }

    let output = valid_tick(&mut engine, 1_100, Some(2), own(Some(100)));

    assert_eq!(output.baseline, Some(5));
}

#[test]
fn b_keeps_the_20000ms_boundary_then_expires_the_old_peak() {
    let mut engine = RiskEngine::new();
    seed(&mut engine, 5, own(Some(100)));
    for now_ms in (500..=20_600).step_by(100) {
        valid_tick(&mut engine, now_ms, Some(0), own(Some(100)));
    }
    let boundary = valid_tick(&mut engine, 20_600, Some(0), own(Some(100)));

    let expired = valid_tick(&mut engine, 20_601, Some(0), own(Some(100)));

    assert_eq!(boundary.baseline, Some(5));
    assert_eq!(expired.baseline, Some(0));
}

#[test]
fn stale_coverage_clears_v_and_b_and_hides_the_vision_count() {
    let mut engine = RiskEngine::new();
    seed(&mut engine, 5, own(Some(100)));

    let output = tick(
        &mut engine,
        500,
        Coverage::Stale,
        Some(2),
        own(Some(100)),
        &[],
    );

    assert_eq!(output.visible, None);
    assert_eq!(output.baseline, None);
    assert!(!output.vision_source);
    assert_eq!(output.meter, Meter::Level(0));
}

#[test]
fn valid_coverage_with_no_visible_count_clears_v_and_b() {
    let mut engine = RiskEngine::new();
    seed(&mut engine, 5, own(Some(100)));

    let output = valid_tick(&mut engine, 500, None, own(Some(100)));

    assert_eq!(output.visible, None);
    assert_eq!(output.baseline, None);
    assert!(!output.vision_source);
}

#[test]
fn coverage_gap_mid_episode_cancels_events_and_rebuilds_baseline() {
    let mut engine = RiskEngine::new();
    start_one_count_drop(&mut engine, own(Some(100)));
    tick(
        &mut engine,
        800,
        Coverage::Stale,
        Some(4),
        own(Some(100)),
        &[],
    );
    for index in 0..5 {
        valid_tick(&mut engine, 900 + index * 100, Some(4), own(Some(100)));
    }
    valid_tick(&mut engine, 1_400, Some(3), own(Some(100)));
    valid_tick(&mut engine, 1_500, Some(3), own(Some(100)));
    let new_drop = valid_tick(&mut engine, 1_600, Some(3), own(Some(100)));
    for now_ms in (1_700..=6_500).step_by(100) {
        valid_tick(&mut engine, now_ms, Some(3), own(Some(100)));
    }
    let exact = valid_tick(&mut engine, 6_600, Some(3), own(Some(100)));
    let after = valid_tick(&mut engine, 6_601, Some(3), own(Some(100)));

    assert_eq!(new_drop.baseline, Some(4));
    assert!(!has(&exact, RiskEvent::EnemyMissing));
    assert!(has(&after, RiskEvent::EnemyMissing));
}

#[test]
fn a_2000ms_observation_gap_keeps_state_but_2001ms_resets_it() {
    let mut engine = RiskEngine::new();
    start_one_count_drop(&mut engine, own(Some(100)));
    let exact_boundary = valid_tick(&mut engine, 2_700, Some(4), own(Some(100)));
    valid_tick(&mut engine, 3_700, Some(4), own(Some(100)));

    let over_boundary = valid_tick(&mut engine, 5_701, Some(4), own(Some(100)));

    assert_eq!(exact_boundary.baseline, Some(5));
    assert!(!has(&exact_boundary, RiskEvent::EnemyMissing));
    assert_eq!(over_boundary.baseline, None);
    assert!(!has(&over_boundary, RiskEvent::EnemyMissing));
}

#[test]
fn enemy_missing_does_not_fire_at_exactly_5000ms() {
    let mut engine = RiskEngine::new();
    start_one_count_drop(&mut engine, own(Some(100)));

    let output = continue_visible_until(&mut engine, 700, 5_700, 4, own(Some(100)));

    assert!(!has(&output, RiskEvent::EnemyMissing));
}

#[test]
fn enemy_missing_fires_after_5000ms() {
    let mut engine = RiskEngine::new();
    start_one_count_drop(&mut engine, own(Some(100)));

    let output = continue_visible_until(&mut engine, 700, 5_701, 4, own(Some(100)));

    assert!(has(&output, RiskEvent::EnemyMissing));
}

#[test]
fn enemy_missing_requires_a_nonzero_baseline() {
    let mut engine = RiskEngine::new();
    seed(&mut engine, 0, own(Some(100)));

    let output = continue_visible_until(&mut engine, 400, 5_500, 0, own(Some(100)));

    assert_eq!(output.baseline, Some(0));
    assert!(!has(&output, RiskEvent::EnemyMissing));
}

#[test]
fn enemy_missing_emits_only_once_during_one_drop_episode() {
    let mut engine = RiskEngine::new();
    start_one_count_drop(&mut engine, own(Some(100)));
    let first = continue_visible_until(&mut engine, 700, 5_701, 4, own(Some(100)));

    let later = continue_visible_until(&mut engine, 5_701, 6_000, 4, own(Some(100)));

    assert!(has(&first, RiskEvent::EnemyMissing));
    assert!(!has(&later, RiskEvent::EnemyMissing));
}

#[test]
fn enemy_missing_requires_current_in_match_own_state() {
    let mut engine = RiskEngine::new();
    start_one_count_drop(&mut engine, own(Some(100)));

    let output = continue_visible_until(&mut engine, 700, 5_701, 4, None);

    assert!(!has(&output, RiskEvent::EnemyMissing));
    assert_eq!(output.meter, Meter::Unknown);
}

#[test]
fn missing_global_cooldown_blocks_before_10000ms_and_allows_at_10000ms() {
    let mut engine = RiskEngine::new();
    start_one_count_drop(&mut engine, own(Some(100)));
    let first = continue_visible_until(&mut engine, 700, 5_701, 4, own(Some(100)));
    for (now_ms, count) in [
        (5_800, 5),
        (5_900, 5),
        (6_000, 5),
        (6_100, 4),
        (6_200, 4),
        (6_300, 4),
    ] {
        valid_tick(&mut engine, now_ms, Some(count), own(Some(100)));
    }
    let blocked = continue_visible_until(&mut engine, 6_300, 11_301, 4, own(Some(100)));
    let allowed = continue_visible_until(&mut engine, 11_301, 15_701, 4, own(Some(100)));

    assert!(has(&first, RiskEvent::EnemyMissing));
    assert!(!has(&blocked, RiskEvent::EnemyMissing));
    assert!(has(&allowed, RiskEvent::EnemyMissing));
}

#[test]
fn level_zero_requires_known_hp_above_50_when_v_equals_b() {
    let mut engine = RiskEngine::new();
    seed(&mut engine, 5, own(Some(100)));

    let output = valid_tick(&mut engine, 500, Some(5), own(Some(100)));

    assert_eq!(output.meter, Meter::Level(0));
    assert!(output.vision_source);
}

#[test]
fn vision_meter_level_one_is_active_from_the_first_count_drop() {
    let mut engine = RiskEngine::new();

    let output = start_one_count_drop(&mut engine, own(None));

    assert_eq!(output.meter, Meter::Level(1));
    assert_eq!(output.baseline, Some(5));
}

#[test]
fn vision_meter_level_two_uses_a_two_count_deficit() {
    let mut engine = RiskEngine::new();

    let output = start_two_count_drop(&mut engine, own(None));

    assert_eq!(output.baseline, Some(5));
    assert_eq!(output.meter, Meter::Level(2));
    assert!(!has(&output, RiskEvent::RiskAlert));
}

#[test]
fn vision_meter_level_three_is_active_after_risk_alert() {
    let mut engine = RiskEngine::new();
    start_two_count_drop(&mut engine, own(None));
    let output = continue_visible_until(&mut engine, 1_000, 6_001, 3, own(None));

    assert!(has(&output, RiskEvent::RiskAlert));
    assert_eq!(output.meter, Meter::Level(3));
}

#[test]
fn risk_alert_requires_the_two_count_drop_to_hold_more_than_5000ms() {
    let mut engine = RiskEngine::new();
    start_two_count_drop(&mut engine, own(Some(100)));
    let exact = continue_visible_until(&mut engine, 1_000, 6_000, 3, own(Some(100)));
    let after = valid_tick(&mut engine, 6_001, Some(3), own(Some(100)));

    assert!(!has(&exact, RiskEvent::RiskAlert));
    assert!(has(&after, RiskEvent::RiskAlert));
}

#[test]
fn risk_alert_accepts_a_two_count_drop_reached_within_3000ms() {
    let mut engine = RiskEngine::new();
    start_one_count_drop(&mut engine, own(Some(100)));
    continue_visible_until(&mut engine, 700, 3_400, 4, own(Some(100)));
    valid_tick(&mut engine, 3_500, Some(3), own(Some(100)));
    valid_tick(&mut engine, 3_600, Some(3), own(Some(100)));
    valid_tick(&mut engine, 3_700, Some(3), own(Some(100)));

    let exact_hold = continue_visible_until(&mut engine, 3_700, 8_700, 3, own(Some(100)));
    let after_hold = valid_tick(&mut engine, 8_701, Some(3), own(Some(100)));

    assert!(!has(&exact_hold, RiskEvent::RiskAlert));
    assert!(has(&after_hold, RiskEvent::RiskAlert));
}

#[test]
fn risk_alert_rejects_a_two_count_drop_reached_after_3000ms() {
    let mut engine = RiskEngine::new();
    start_one_count_drop(&mut engine, own(Some(100)));
    continue_visible_until(&mut engine, 700, 3_401, 4, own(Some(100)));
    valid_tick(&mut engine, 3_501, Some(3), own(Some(100)));
    valid_tick(&mut engine, 3_601, Some(3), own(Some(100)));
    valid_tick(&mut engine, 3_701, Some(3), own(Some(100)));

    let output = continue_visible_until(&mut engine, 3_701, 8_702, 3, own(Some(100)));

    assert!(!has(&output, RiskEvent::RiskAlert));
}

#[test]
fn risk_alert_is_suppressed_when_own_hero_is_dead() {
    let mut engine = RiskEngine::new();
    start_two_count_drop(&mut engine, dead_own(Some(100)));

    let output = continue_visible_until(&mut engine, 1_000, 6_001, 3, dead_own(Some(20)));

    assert!(!has(&output, RiskEvent::RiskAlert));
    assert_eq!(output.meter, Meter::Level(2));
}

#[test]
fn risk_alert_is_suppressed_when_own_gsi_is_unknown() {
    let mut engine = RiskEngine::new();
    start_two_count_drop(&mut engine, own(Some(100)));

    let output = continue_visible_until(&mut engine, 1_000, 6_001, 3, None);

    assert!(!has(&output, RiskEvent::RiskAlert));
    assert_eq!(output.meter, Meter::Unknown);
}

#[test]
fn risk_alert_global_cooldown_suppresses_until_30000ms() {
    let mut engine = RiskEngine::new();
    start_two_count_drop(&mut engine, own(Some(100)));
    let first = continue_visible_until(&mut engine, 1_000, 6_001, 3, own(Some(100)));
    for (now_ms, count) in [(6_100, 5), (6_200, 5), (6_300, 5)] {
        valid_tick(&mut engine, now_ms, Some(count), own(Some(100)));
    }
    for now_ms in (6_400..=30_000).step_by(100) {
        valid_tick(&mut engine, now_ms, Some(5), own(Some(100)));
    }
    for (now_ms, count) in [
        (30_100, 4),
        (30_200, 4),
        (30_300, 4),
        (30_400, 3),
        (30_500, 3),
        (30_600, 3),
    ] {
        valid_tick(&mut engine, now_ms, Some(count), own(Some(100)));
    }
    let before = continue_visible_until(&mut engine, 30_600, 36_000, 3, own(Some(100)));
    let at = valid_tick(&mut engine, 36_001, Some(3), own(Some(100)));

    assert!(has(&first, RiskEvent::RiskAlert));
    assert!(!has(&before, RiskEvent::RiskAlert));
    assert!(has(&at, RiskEvent::RiskAlert));
}

#[test]
fn risk_revision_requires_1000ms_of_recovery_and_emits_once() {
    let mut engine = RiskEngine::new();
    start_two_count_drop(&mut engine, own(Some(100)));
    continue_visible_until(&mut engine, 1_000, 6_001, 3, own(Some(100)));
    valid_tick(&mut engine, 6_100, Some(5), own(Some(100)));
    valid_tick(&mut engine, 6_200, Some(5), own(Some(100)));
    valid_tick(&mut engine, 6_300, Some(5), own(Some(100)));
    let too_soon = valid_tick(&mut engine, 7_299, Some(5), own(Some(100)));
    let revision = valid_tick(&mut engine, 7_300, Some(5), own(Some(100)));

    let later = valid_tick(&mut engine, 8_300, Some(5), own(Some(100)));

    assert!(!has(&too_soon, RiskEvent::RiskRevision));
    assert!(has(&revision, RiskEvent::RiskRevision));
    assert!(!has(&later, RiskEvent::RiskRevision));
    assert_eq!(later.meter, Meter::Level(0));
}

#[test]
fn risk_revision_is_allowed_when_recovery_completes_at_10000ms() {
    let mut engine = RiskEngine::new();
    start_two_count_drop(&mut engine, own(Some(100)));
    continue_visible_until(&mut engine, 1_000, 6_001, 3, own(Some(100)));
    continue_visible_until(&mut engine, 6_001, 14_900, 3, own(Some(100)));
    valid_tick(&mut engine, 14_901, Some(5), own(Some(100)));
    valid_tick(&mut engine, 15_000, Some(5), own(Some(100)));
    valid_tick(&mut engine, 15_001, Some(5), own(Some(100)));

    let output = continue_visible_until(&mut engine, 15_001, 16_001, 5, own(Some(100)));

    assert!(has(&output, RiskEvent::RiskRevision));
}

#[test]
fn risk_revision_does_not_fire_when_recovery_completes_after_10000ms() {
    let mut engine = RiskEngine::new();
    start_two_count_drop(&mut engine, own(Some(100)));
    continue_visible_until(&mut engine, 1_000, 6_001, 3, own(Some(100)));
    continue_visible_until(&mut engine, 6_001, 15_800, 3, own(Some(100)));
    valid_tick(&mut engine, 15_900, Some(5), own(Some(100)));
    valid_tick(&mut engine, 16_000, Some(5), own(Some(100)));
    valid_tick(&mut engine, 16_100, Some(5), own(Some(100)));

    let output = valid_tick(&mut engine, 17_100, Some(5), own(Some(100)));

    assert!(!has(&output, RiskEvent::RiskRevision));
}

#[test]
fn coverage_gap_cancels_alert_and_prevents_a_revision() {
    let mut engine = RiskEngine::new();
    start_two_count_drop(&mut engine, own(Some(100)));
    let alert = continue_visible_until(&mut engine, 1_000, 6_001, 3, own(Some(100)));
    let gap = tick(
        &mut engine,
        6_100,
        Coverage::Unavailable,
        Some(5),
        own(Some(100)),
        &[],
    );

    let output = valid_tick(&mut engine, 6_200, Some(5), own(Some(100)));

    assert!(has(&alert, RiskEvent::RiskAlert));
    assert_eq!(gap.baseline, None);
    assert!(!has(&output, RiskEvent::RiskRevision));
}

#[test]
fn coverage_stale_does_not_emit_risk_unavailable() {
    let mut engine = RiskEngine::new();

    let output = tick(&mut engine, 0, Coverage::Stale, None, own(Some(100)), &[]);

    assert!(!has(&output, RiskEvent::RiskUnavailable));
}

#[test]
fn risk_unavailable_emits_once_for_an_unavailable_transition_in_match() {
    let mut engine = RiskEngine::new();
    let first = tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &[],
    );
    let repeated = tick(
        &mut engine,
        100,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &[],
    );

    assert!(has(&first, RiskEvent::RiskUnavailable));
    assert!(!has(&repeated, RiskEvent::RiskUnavailable));
}

#[test]
fn risk_unavailable_requires_in_match_own_state_at_transition() {
    let mut engine = RiskEngine::new();
    let outside = tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own_state(true, Some(100), false, 0),
        &[],
    );
    let still_unavailable = tick(
        &mut engine,
        100,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &[],
    );

    assert!(!has(&outside, RiskEvent::RiskUnavailable));
    assert!(!has(&still_unavailable, RiskEvent::RiskUnavailable));
}

#[test]
fn risk_unavailable_rearms_after_2000ms_of_continuous_valid_coverage() {
    let mut engine = RiskEngine::new();
    tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &[],
    );
    tick(
        &mut engine,
        100,
        Coverage::Valid,
        Some(5),
        own(Some(100)),
        &[],
    );
    let before = tick(
        &mut engine,
        2_099,
        Coverage::Valid,
        Some(5),
        own(Some(100)),
        &[],
    );
    let clear = tick(
        &mut engine,
        2_100,
        Coverage::Valid,
        Some(5),
        own(Some(100)),
        &[],
    );
    let next = tick(
        &mut engine,
        2_101,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &[],
    );

    assert!(!has(&before, RiskEvent::RiskUnavailable));
    assert!(!has(&clear, RiskEvent::RiskUnavailable));
    assert!(has(&next, RiskEvent::RiskUnavailable));
}

#[test]
fn stale_coverage_interrupts_unavailable_recovery_clear_time() {
    let mut engine = RiskEngine::new();
    tick(
        &mut engine,
        0,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &[],
    );
    tick(
        &mut engine,
        100,
        Coverage::Valid,
        Some(5),
        own(Some(100)),
        &[],
    );
    tick(
        &mut engine,
        1_000,
        Coverage::Stale,
        None,
        own(Some(100)),
        &[],
    );
    tick(
        &mut engine,
        1_500,
        Coverage::Valid,
        Some(5),
        own(Some(100)),
        &[],
    );
    tick(
        &mut engine,
        3_499,
        Coverage::Valid,
        Some(5),
        own(Some(100)),
        &[],
    );
    let still_latched = tick(
        &mut engine,
        3_500,
        Coverage::Unavailable,
        None,
        own(Some(100)),
        &[],
    );

    assert!(!has(&still_latched, RiskEvent::RiskUnavailable));
}
