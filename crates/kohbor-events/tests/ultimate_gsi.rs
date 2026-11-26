use kohbor_events::{Detector, Event, EventId, Phase, Tick, UltimateContext};
use kohbor_gsi::admission::{AbilityObservation, Field};

const SNIPER: &str = "npc_dota_hero_sniper";
const ASSASSINATE: &str = "sniper_assassinate";

fn ability(
    name: &str,
    level: Field<i64>,
    cooldown: Field<i64>,
    ultimate: Field<bool>,
) -> AbilityObservation {
    AbilityObservation {
        slot: 6,
        name: Field::Value(name.to_owned()),
        level,
        cooldown,
        ultimate,
        can_cast: Field::Missing,
        passive: Field::Missing,
        active: Field::Missing,
    }
}

fn tick(
    at_ms: u64,
    session_epoch: u64,
    generation: u64,
    hero: &str,
    alive: bool,
    kills: u32,
    ultimate_ability: AbilityObservation,
) -> (Tick, UltimateContext) {
    (
        Tick {
            at_ms,
            session_epoch,
            phase: Phase::Live,
            trusted: true,
            kills: Some(kills),
            deaths: Some(0),
            kill_streak: Some(0),
            combined_score: Some(kills),
            level: Some(12),
            hp: Some(1000),
            max_hp: Some(1000),
            mana_percent: Some(100),
            alive: Some(alive),
        },
        UltimateContext {
            generation,
            hero_id: Some(hero.to_owned()),
            abilities: Field::Value(vec![ultimate_ability]),
        },
    )
}

fn observe(detector: &mut Detector, (tick, context): (Tick, UltimateContext)) -> Vec<Event> {
    detector.observe_with_ultimate(&tick, &context)
}

fn spell(cooldown: i64) -> AbilityObservation {
    ability(
        ASSASSINATE,
        Field::Value(3),
        Field::Value(cooldown),
        Field::Value(true),
    )
}

fn has(events: &[Event], id: EventId) -> bool {
    events.iter().any(|event| event.id == id)
}

fn event_count(events: &[Event], id: EventId) -> usize {
    events.iter().filter(|event| event.id == id).count()
}

#[test]
fn cooldown_zero_to_positive_emits_ultimate_with_hero_and_ability_payload() {
    let mut detector = Detector::new();
    assert!(observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, spell(0))).is_empty());

    let events = observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, spell(6)));
    let fired = events
        .iter()
        .find(|event| event.id == EventId::UltimateFired)
        .expect("cooldown transition fires the ultimate event");
    let payload = fired.payload.as_ref().expect("ultimate payload is present");
    assert_eq!(payload.hero, SNIPER);
    assert_eq!(payload.ability, ASSASSINATE);
}

#[test]
fn ability_level_up_rebaselines_without_firing() {
    let mut detector = Detector::new();
    let mut baseline_ability = spell(0);
    baseline_ability.level = Field::Value(1);
    assert!(observe(
        &mut detector,
        tick(0, 1, 1, SNIPER, true, 0, baseline_ability)
    )
    .is_empty());

    let mut leveled_ability = spell(5);
    leveled_ability.level = Field::Value(2);
    assert!(!has(
        &observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, leveled_ability.clone())),
        EventId::UltimateFired
    ));
    leveled_ability.cooldown = Field::Value(0);
    assert!(!has(
        &observe(&mut detector, tick(200, 1, 1, SNIPER, true, 0, leveled_ability.clone())),
        EventId::UltimateFired
    ));
    leveled_ability.cooldown = Field::Value(5);
    assert!(has(
        &observe(&mut detector, tick(300, 1, 1, SNIPER, true, 0, leveled_ability)),
        EventId::UltimateFired
    ));
}

#[test]
fn cooldown_already_running_at_baseline_does_not_fire() {
    let mut detector = Detector::new();
    assert!(observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, spell(5))).is_empty());
    assert!(!has(
        &observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, spell(4))),
        EventId::UltimateFired
    ));
    assert!(!has(
        &observe(&mut detector, tick(200, 1, 1, SNIPER, true, 0, spell(0))),
        EventId::UltimateFired
    ));
    assert!(has(
        &observe(&mut detector, tick(300, 1, 1, SNIPER, true, 0, spell(5))),
        EventId::UltimateFired
    ));
}

#[test]
fn dead_hero_cooldown_transition_does_not_fire_and_revive_rebaselines() {
    let mut detector = Detector::new();
    assert!(!has(
        &observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, spell(0))),
        EventId::UltimateFired
    ));
    assert!(!has(
        &observe(&mut detector, tick(100, 1, 1, SNIPER, false, 0, spell(5))),
        EventId::UltimateFired
    ));
    assert!(!has(
        &observe(&mut detector, tick(200, 1, 1, SNIPER, true, 0, spell(5))),
        EventId::UltimateFired
    ));
    assert!(!has(
        &observe(&mut detector, tick(300, 1, 1, SNIPER, true, 0, spell(0))),
        EventId::UltimateFired
    ));
    assert!(has(
        &observe(&mut detector, tick(400, 1, 1, SNIPER, true, 0, spell(5))),
        EventId::UltimateFired
    ));
}

#[test]
fn dead_cooldown_zero_does_not_form_a_transition_with_the_revive_sample() {
    let mut detector = Detector::new();
    assert!(observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, spell(0))).is_empty());
    assert!(!has(
        &observe(&mut detector, tick(100, 1, 1, SNIPER, false, 0, spell(0))),
        EventId::UltimateFired
    ));
    assert!(!has(
        &observe(&mut detector, tick(200, 1, 1, SNIPER, true, 0, spell(5))),
        EventId::UltimateFired
    ));
    assert!(!has(
        &observe(&mut detector, tick(300, 1, 1, SNIPER, true, 0, spell(0))),
        EventId::UltimateFired
    ));
    assert!(has(
        &observe(&mut detector, tick(400, 1, 1, SNIPER, true, 0, spell(5))),
        EventId::UltimateFired
    ));
}

#[test]
fn hero_change_establishes_a_new_ultimate_baseline() {
    let mut detector = Detector::new();
    assert!(observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, spell(0))).is_empty());
    let lina_ultimate = ability(
        "lina_laguna_blade",
        Field::Value(1),
        Field::Value(5),
        Field::Value(true),
    );
    assert!(!has(
        &observe(&mut detector, tick(
            100,
            1,
            1,
            "npc_dota_hero_lina",
            true,
            0,
            lina_ultimate.clone()
        )),
        EventId::UltimateFired
    ));
    let mut lina_ultimate = lina_ultimate;
    lina_ultimate.cooldown = Field::Value(0);
    assert!(!has(
        &observe(&mut detector, tick(
            200,
            1,
            1,
            "npc_dota_hero_lina",
            true,
            0,
            lina_ultimate.clone()
        )),
        EventId::UltimateFired
    ));
    lina_ultimate.cooldown = Field::Value(5);
    let events = observe(&mut detector, tick(
        300,
        1,
        1,
        "npc_dota_hero_lina",
        true,
        0,
        lina_ultimate,
    ));
    let fired = events
        .iter()
        .find(|event| event.id == EventId::UltimateFired)
        .expect("Lina transition fires only after its baseline");
    assert_eq!(fired.payload.as_ref().unwrap().hero, "npc_dota_hero_lina");
}

#[test]
fn session_resume_and_source_generation_changes_only_establish_baselines() {
    let mut detector = Detector::new();
    assert!(observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, spell(0))).is_empty());
    assert!(!has(
        &observe(&mut detector, tick(100, 1, 2, SNIPER, true, 0, spell(5))),
        EventId::UltimateFired
    ));
    assert!(!has(
        &observe(&mut detector, tick(200, 1, 2, SNIPER, true, 0, spell(0))),
        EventId::UltimateFired
    ));
    assert!(has(
        &observe(&mut detector, tick(300, 1, 2, SNIPER, true, 0, spell(5))),
        EventId::UltimateFired
    ));
    assert!(!has(
        &observe(&mut detector, tick(400, 2, 2, SNIPER, true, 0, spell(0))),
        EventId::UltimateFired
    ));
    assert!(!has(
        &observe(&mut detector, tick(500, 2, 2, SNIPER, true, 0, spell(0))),
        EventId::UltimateFired
    ));
    assert!(has(
        &observe(&mut detector, tick(600, 2, 2, SNIPER, true, 0, spell(5))),
        EventId::UltimateFired
    ));
}

#[test]
fn refresher_style_zero_then_positive_transition_emits_again() {
    let mut detector = Detector::new();
    observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, spell(0)));
    assert!(has(
        &observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, spell(8))),
        EventId::UltimateFired
    ));
    assert!(!has(
        &observe(&mut detector, tick(200, 1, 1, SNIPER, true, 0, spell(0))),
        EventId::UltimateFired
    ));
    assert!(has(
        &observe(&mut detector, tick(300, 1, 1, SNIPER, true, 0, spell(8))),
        EventId::UltimateFired
    ));
}

#[test]
fn missing_gsi_ultimate_flag_uses_only_the_local_stats_metadata() {
    let mut detector = Detector::new();
    let from_stats = ability(
        ASSASSINATE,
        Field::Value(3),
        Field::Value(0),
        Field::Missing,
    );
    assert!(observe(
        &mut detector,
        tick(0, 1, 1, SNIPER, true, 0, from_stats.clone())
    )
    .is_empty());
    let mut from_stats = from_stats;
    from_stats.cooldown = Field::Value(8);
    let events = observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, from_stats));
    assert!(has(&events, EventId::UltimateFired));
    assert_eq!(
        events
            .iter()
            .find(|event| event.id == EventId::UltimateFired)
            .unwrap()
            .payload
            .as_ref()
            .unwrap()
            .ability,
        ASSASSINATE
    );
}

#[test]
fn unknown_ultimate_identity_emits_nothing() {
    let mut detector = Detector::new();
    let unknown = ability(
        "synthetic_unknown_spell",
        Field::Value(1),
        Field::Value(0),
        Field::Missing,
    );
    observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, unknown.clone()));
    let mut unknown = unknown;
    unknown.cooldown = Field::Value(8);
    assert!(!has(
        &observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, unknown)),
        EventId::UltimateFired
    ));
}

#[test]
fn explicit_gsi_false_does_not_fall_back_to_local_ultimate_metadata() {
    let mut detector = Detector::new();
    let not_ultimate = ability(
        ASSASSINATE,
        Field::Value(3),
        Field::Value(0),
        Field::Value(false),
    );
    observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, not_ultimate.clone()));
    let mut not_ultimate = not_ultimate;
    not_ultimate.cooldown = Field::Value(8);
    assert!(!has(
        &observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, not_ultimate)),
        EventId::UltimateFired
    ));
}

#[test]
fn absent_previous_cooldown_can_transition_to_positive() {
    let mut detector = Detector::new();
    let mut missing = spell(0);
    missing.cooldown = Field::Missing;
    observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, missing));
    assert!(has(
        &observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, spell(5))),
        EventId::UltimateFired
    ));
}

#[test]
fn ultimate_kill_inside_window_is_additional_to_hero_kill() {
    let mut detector = Detector::new();
    observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, spell(0)));
    observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, spell(8)));

    let events = observe(&mut detector, tick(4100, 1, 1, SNIPER, true, 1, spell(8)));
    assert!(has(&events, EventId::HeroKill));
    let ult_kill = events
        .iter()
        .find(|event| event.id == EventId::UltimateKill)
        .expect("own kill at the 4.0 second boundary is correlated");
    assert_eq!(ult_kill.payload.as_ref().unwrap().hero, SNIPER);
    assert_eq!(ult_kill.payload.as_ref().unwrap().ability, ASSASSINATE);
}

#[test]
fn ultimate_kill_outside_window_is_not_emitted() {
    let mut detector = Detector::new();
    observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, spell(0)));
    observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, spell(8)));

    let events = observe(&mut detector, tick(4101, 1, 1, SNIPER, true, 1, spell(8)));
    assert!(has(&events, EventId::HeroKill));
    assert!(!has(&events, EventId::UltimateKill));
}

#[test]
fn two_kill_burst_keeps_existing_events_and_emits_one_ultimate_kill() {
    let mut detector = Detector::new();
    observe(&mut detector, tick(0, 1, 1, SNIPER, true, 0, spell(0)));
    observe(&mut detector, tick(100, 1, 1, SNIPER, true, 0, spell(8)));

    let events = observe(&mut detector, tick(200, 1, 1, SNIPER, true, 2, spell(8)));
    assert!(has(&events, EventId::HeroKill));
    assert!(has(&events, EventId::MultiKill));
    assert_eq!(event_count(&events, EventId::UltimateKill), 1);
}
