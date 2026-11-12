use kohbor_events::{adapter::tick_from_admitted, Phase, Tick};
use kohbor_gsi::admission::{
    Admitted, Field, HeroFields, OwnCounters, Ownership, Phase as GsiPhase,
};

fn admitted(phase: GsiPhase) -> Admitted {
    Admitted {
        phase,
        ownership: Ownership::Verified,
        team: None,
        match_id: Some("synthetic-match".into()),
        steamid64: Some(76_561_197_960_265_729),
        counters: OwnCounters {
            kills: Field::Value(3),
            kill_streak: Field::Value(2),
            deaths: Field::Missing,
            combined_score: Field::Value(10),
        },
        hero: HeroFields {
            hero_id: Field::Value("npc_dota_hero_silencer".into()),
            level: Field::Value(12),
            hp: Field::Value(1201),
            max_hp: Field::Value(1200),
            mana_percent: Field::Value(50),
            alive: Field::Value(true),
            respawn_seconds: Field::Value(0),
        },
    }
}

#[test]
fn maps_admitted_values_and_keeps_hp_above_max_hp_unchanged() {
    let tick = tick_from_admitted(&admitted(GsiPhase::Live), 42, 7, true);

    assert_eq!(
        tick,
        Tick {
            at_ms: 42,
            session_epoch: 7,
            phase: Phase::Live,
            trusted: true,
            kills: Some(3),
            deaths: None,
            kill_streak: Some(2),
            combined_score: Some(10),
            level: Some(12),
            hp: Some(1201),
            max_hp: Some(1200),
            mana_percent: Some(50),
            alive: Some(true),
        }
    );
}

#[test]
fn maps_every_admitted_phase_one_to_one() {
    for (gsi, events) in [
        (GsiPhase::Draft, Phase::Draft),
        (GsiPhase::Pregame, Phase::Pregame),
        (GsiPhase::Live, Phase::Live),
        (GsiPhase::PostGame, Phase::PostGame),
    ] {
        assert_eq!(
            tick_from_admitted(&admitted(gsi), 0, 1, false).phase,
            events
        );
    }
}

#[test]
fn missing_and_invalid_fields_map_to_unknown() {
    let mut admitted = admitted(GsiPhase::Draft);
    admitted.counters.kills = Field::Missing;
    admitted.counters.combined_score = Field::Invalid;
    admitted.hero.level = Field::Missing;
    admitted.hero.hp = Field::Invalid;
    admitted.hero.max_hp = Field::Missing;
    admitted.hero.mana_percent = Field::Invalid;
    admitted.hero.alive = Field::Missing;

    let tick = tick_from_admitted(&admitted, 0, 1, true);
    assert_eq!(tick.kills, None);
    assert_eq!(tick.combined_score, None);
    assert_eq!(tick.level, None);
    assert_eq!(tick.hp, None);
    assert_eq!(tick.max_hp, None);
    assert_eq!(tick.mana_percent, None);
    assert_eq!(tick.alive, None);
}

#[test]
fn values_outside_u32_are_not_narrowed_or_wrapped() {
    let too_large = i64::from(u32::MAX) + 1;
    let mut admitted = admitted(GsiPhase::Live);
    admitted.counters.kills = Field::Value(-1);
    admitted.counters.combined_score = Field::Value(too_large);
    admitted.hero.level = Field::Value(too_large);
    admitted.hero.hp = Field::Value(-1);
    admitted.hero.max_hp = Field::Value(too_large);
    admitted.hero.mana_percent = Field::Value(too_large);

    let tick = tick_from_admitted(&admitted, 1, 1, true);
    assert_eq!(tick.kills, None);
    assert_eq!(tick.combined_score, None);
    assert_eq!(tick.level, None);
    assert_eq!(tick.hp, None);
    assert_eq!(tick.max_hp, None);
    assert_eq!(tick.mana_percent, None);
}
