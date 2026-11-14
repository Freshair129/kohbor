use kohbor_gsi::admission::Field;
use kohbor_gsi::{admit, Outcome};
use serde_json::{json, Value};

const TOKEN: &str = "synthetic-deaths-token";
const STEAMID: u64 = 76_561_197_960_265_729;

fn payload() -> Value {
    json!({
        "provider": {"appid": 570},
        "auth": {"token": TOKEN},
        "map": {"game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS"},
        "player": {
            "activity": "playing",
            "steamid": STEAMID.to_string(),
            "kills": 3,
            "kill_streak": 2,
            "deaths": 1
        },
        "hero": {"alive": true}
    })
}

fn admitted(body: &Value) -> Box<kohbor_gsi::admission::Admitted> {
    match admit(body, TOKEN, Some(STEAMID), None) {
        Outcome::Admitted(admitted) => admitted,
        other => panic!("expected admitted payload, got {other:?}"),
    }
}

fn captured(text: &str) -> Value {
    let text = text
        .replace("<TOKEN>", TOKEN)
        .replace("<STEAMID>", &STEAMID.to_string())
        .replace("<MATCHID>", "8000000001");
    serde_json::from_str(&text).unwrap()
}

#[test]
fn present_deaths_and_inclusive_counter_bounds_are_admitted() {
    let mut body = payload();
    body["player"]["deaths"] = json!(0);
    assert_eq!(admitted(&body).counters.deaths, Field::Value(0));

    body["player"]["deaths"] = json!(10_000);
    assert_eq!(admitted(&body).counters.deaths, Field::Value(10_000));
}

#[test]
fn absent_deaths_are_missing() {
    let mut body = payload();
    body["player"].as_object_mut().unwrap().remove("deaths");

    assert_eq!(admitted(&body).counters.deaths, Field::Missing);
}

#[test]
fn wrong_type_deaths_are_invalid() {
    for value in [json!("1"), json!(true), json!(1.5)] {
        let mut body = payload();
        body["player"]["deaths"] = value;
        assert_eq!(admitted(&body).counters.deaths, Field::Invalid);
    }
}

#[test]
fn out_of_range_deaths_are_invalid() {
    for value in [json!(-1), json!(10_001), json!(u64::MAX)] {
        let mut body = payload();
        body["player"]["deaths"] = value;
        assert_eq!(admitted(&body).counters.deaths, Field::Invalid);
    }
}

#[test]
fn captured_playing_fixtures_still_admit_with_deaths_unknown() {
    let fixtures = [
        include_str!("captured/hero-selection-no-pick.json"),
        include_str!("captured/team-showcase.json"),
        include_str!("captured/pre-game.json"),
        include_str!("captured/in-progress-alive.json"),
        include_str!("captured/in-progress-paused.json"),
        include_str!("captured/in-progress-dead.json"),
        include_str!("captured/post-game.json"),
    ];

    for fixture in fixtures {
        let body = captured(fixture);
        assert_eq!(admitted(&body).counters.deaths, Field::Missing);
    }
}
