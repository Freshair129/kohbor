use kohbor_gsi::admission::{Field, HeroFields, Outcome, Ownership, Phase};
use kohbor_gsi::admit;
use serde_json::{json, Value};

const TOKEN: &str = "synthetic-replay-token";
const STEAMID: u64 = 76_561_197_960_265_729;

fn payload() -> Value {
    json!({
        "provider": {"appid": 570},
        "auth": {"token": TOKEN},
        "map": {
            "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS",
            "radiant_score": 4,
            "dire_score": 6,
            "matchid": "8000000001"
        },
        "player": {
            "activity": "playing",
            "steamid": STEAMID.to_string(),
            "kills": 3,
            "kill_streak": 2
        },
        "hero": {
            "name": "npc_dota_hero_silencer",
            "level": 12,
            "health": 800,
            "max_health": 1000,
            "mana_percent": 50,
            "alive": true,
            "respawn_seconds": 0
        }
    })
}

fn admitted(body: &Value) -> Box<kohbor_gsi::admission::Admitted> {
    match admit(body, TOKEN, Some(STEAMID), None) {
        Outcome::Admitted(admitted) => admitted,
        outcome => panic!("expected admitted payload, got {outcome:?}"),
    }
}

#[test]
fn present_counters_are_admitted_with_combined_score() {
    let body = payload();
    let admitted = admitted(&body);

    assert_eq!(admitted.ownership, Ownership::Verified);
    assert_eq!(admitted.counters.kills, Field::Value(3));
    assert_eq!(admitted.counters.kill_streak, Field::Value(2));
    assert_eq!(admitted.counters.combined_score, Field::Value(10));
}

#[test]
fn zero_and_one_hundred_thousand_are_in_the_valid_range() {
    let mut body = payload();
    body["player"]["kills"] = json!(0);
    body["player"]["kill_streak"] = json!(100_000);
    body["map"]["radiant_score"] = json!(100_000);
    body["map"]["dire_score"] = json!(0);

    let admitted = admitted(&body);
    assert_eq!(admitted.counters.kills, Field::Value(0));
    assert_eq!(admitted.counters.kill_streak, Field::Value(100_000));
    assert_eq!(admitted.counters.combined_score, Field::Value(100_000));
}

#[test]
fn absent_counters_and_score_side_are_missing() {
    let mut body = payload();
    body["player"].as_object_mut().unwrap().remove("kills");
    body["player"]
        .as_object_mut()
        .unwrap()
        .remove("kill_streak");
    body["map"].as_object_mut().unwrap().remove("radiant_score");

    let admitted = admitted(&body);
    assert_eq!(admitted.counters.kills, Field::Missing);
    assert_eq!(admitted.counters.kill_streak, Field::Missing);
    assert_eq!(admitted.counters.combined_score, Field::Missing);
}

#[test]
fn non_integer_counter_values_are_invalid() {
    let mut body = payload();
    body["player"]["kills"] = json!("3");
    body["player"]["kill_streak"] = json!(true);
    body["map"]["radiant_score"] = json!("4");

    let admitted = admitted(&body);
    assert_eq!(admitted.counters.kills, Field::Invalid);
    assert_eq!(admitted.counters.kill_streak, Field::Invalid);
    assert_eq!(admitted.counters.combined_score, Field::Invalid);
}

#[test]
fn counters_and_score_sources_above_the_valid_range_are_invalid() {
    let mut body = payload();
    body["player"]["kills"] = json!(100_001);
    body["player"]["kill_streak"] = json!(100_001);
    body["map"]["dire_score"] = json!(100_001);

    let admitted = admitted(&body);
    assert_eq!(admitted.counters.kills, Field::Invalid);
    assert_eq!(admitted.counters.kill_streak, Field::Invalid);
    assert_eq!(admitted.counters.combined_score, Field::Invalid);
}

#[test]
fn negative_and_huge_counter_values_are_invalid() {
    let mut body = payload();
    body["player"]["kills"] = json!(-1);
    body["player"]["kill_streak"] = json!(u64::MAX);

    let admitted = admitted(&body);
    assert_eq!(admitted.counters.kills, Field::Invalid);
    assert_eq!(admitted.counters.kill_streak, Field::Invalid);
}

#[test]
fn combined_score_is_missing_when_either_side_is_absent() {
    let mut body = payload();
    body["map"].as_object_mut().unwrap().remove("dire_score");

    assert_eq!(admitted(&body).counters.combined_score, Field::Missing);
}

fn fixture(text: &str) -> Value {
    let text = text
        .replace("<TOKEN>", TOKEN)
        .replace("<STEAMID>", &STEAMID.to_string())
        .replace("<MATCHID>", "8000000001");
    serde_json::from_str(&text).unwrap()
}

#[test]
fn captured_playing_fixtures_keep_their_phase_and_hero_fields() {
    let fixtures = [
        (
            "hero-selection-no-pick",
            Phase::Draft,
            HeroFields {
                hero_id: Field::Missing,
                level: Field::Missing,
                hp: Field::Missing,
                max_hp: Field::Missing,
                mana_percent: Field::Missing,
                alive: Field::Missing,
                respawn_seconds: Field::Missing,
            },
        ),
        (
            "team-showcase",
            Phase::Draft,
            HeroFields {
                hero_id: Field::Value("npc_dota_hero_silencer".into()),
                level: Field::Missing,
                hp: Field::Missing,
                max_hp: Field::Missing,
                mana_percent: Field::Missing,
                alive: Field::Missing,
                respawn_seconds: Field::Missing,
            },
        ),
        (
            "pre-game",
            Phase::Pregame,
            HeroFields {
                hero_id: Field::Value("npc_dota_hero_silencer".into()),
                level: Field::Value(1),
                hp: Field::Value(516),
                max_hp: Field::Value(516),
                mana_percent: Field::Value(100),
                alive: Field::Value(true),
                respawn_seconds: Field::Value(0),
            },
        ),
        (
            "in-progress-alive",
            Phase::Live,
            HeroFields {
                hero_id: Field::Value("npc_dota_hero_silencer".into()),
                level: Field::Value(1),
                hp: Field::Value(604),
                max_hp: Field::Value(604),
                mana_percent: Field::Value(100),
                alive: Field::Value(true),
                respawn_seconds: Field::Value(0),
            },
        ),
        (
            "in-progress-paused",
            Phase::Live,
            HeroFields {
                hero_id: Field::Value("npc_dota_hero_silencer".into()),
                level: Field::Value(22),
                hp: Field::Value(0),
                max_hp: Field::Value(2960),
                mana_percent: Field::Value(41),
                alive: Field::Value(false),
                respawn_seconds: Field::Value(45),
            },
        ),
        (
            "in-progress-dead",
            Phase::Live,
            HeroFields {
                hero_id: Field::Value("npc_dota_hero_silencer".into()),
                level: Field::Value(6),
                hp: Field::Value(0),
                max_hp: Field::Value(846),
                mana_percent: Field::Value(84),
                alive: Field::Value(false),
                respawn_seconds: Field::Value(25),
            },
        ),
        (
            "post-game",
            Phase::PostGame,
            HeroFields {
                hero_id: Field::Value("npc_dota_hero_silencer".into()),
                level: Field::Value(22),
                hp: Field::Value(0),
                max_hp: Field::Value(2960),
                mana_percent: Field::Value(41),
                alive: Field::Value(false),
                respawn_seconds: Field::Value(37),
            },
        ),
    ];

    for (name, expected_phase, expected_hero) in fixtures {
        let body = match name {
            "hero-selection-no-pick" => {
                fixture(include_str!("captured/hero-selection-no-pick.json"))
            }
            "team-showcase" => fixture(include_str!("captured/team-showcase.json")),
            "pre-game" => fixture(include_str!("captured/pre-game.json")),
            "in-progress-alive" => fixture(include_str!("captured/in-progress-alive.json")),
            "in-progress-paused" => fixture(include_str!("captured/in-progress-paused.json")),
            "in-progress-dead" => fixture(include_str!("captured/in-progress-dead.json")),
            "post-game" => fixture(include_str!("captured/post-game.json")),
            _ => unreachable!(),
        };
        let admitted = admitted(&body);
        assert_eq!(admitted.phase, expected_phase, "fixture {name}");
        assert_eq!(admitted.hero, expected_hero, "fixture {name}");
    }
}
