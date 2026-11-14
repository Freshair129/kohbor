//! Synthetic contract tests (evidence class: synthetic). Payloads below are
//! fabricated test inputs, not captured game data, and never ship in production.

use kohbor_gsi::admission::{Ownership, Rejection, UnsupportedReason};
use kohbor_gsi::{admit, build_snapshot, steamid64_from_account_id, Outcome, SourceState};
use serde_json::{json, Value};

const TOKEN: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO-_";
const ACCOUNT: u32 = 12_345;
const OWN_STAT_CASES: [(&str, i64, i64); 8] = [
    ("gold", 45_123, 1_000_000),
    ("kills", 7, 10_000),
    ("deaths", 2, 10_000),
    ("assists", 9, 10_000),
    ("last_hits", 100, 10_000),
    ("denies", 8, 10_000),
    ("gpm", 456, 10_000),
    ("xpm", 512, 10_000),
];

fn owner() -> u64 {
    steamid64_from_account_id(ACCOUNT)
}

fn payload() -> Value {
    json!({
        "provider": {"name": "Dota 2", "appid": 570, "version": 47, "timestamp": 1_700_000_000},
        "map": {"matchid": "8000000001", "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "customgamename": ""},
        "player": {"steamid": owner().to_string(), "activity": "playing", "team_name": "radiant"},
        "hero": {"name": "npc_dota_hero_axe", "level": 12, "health": 850, "max_health": 1200,
                 "mana_percent": 43, "alive": true, "respawn_seconds": 0},
        "auth": {"token": TOKEN}
    })
}

fn src(age: u64) -> SourceState {
    SourceState {
        generation: 2,
        sequence: Some(15),
        age_ms: Some(age),
        session_epoch: Some(3),
        identity_revision: 1,
    }
}

fn schema() -> jsonschema::Validator {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/own-snapshot.schema.json"
    );
    let text = std::fs::read_to_string(path).expect("own-snapshot schema present");
    let schema: Value = serde_json::from_str(&text).unwrap();
    jsonschema::validator_for(&schema).expect("valid schema")
}

fn assert_contract(snapshot: &Value) {
    let v = schema();
    let errors: Vec<String> = v.iter_errors(snapshot).map(|e| e.to_string()).collect();
    assert!(
        errors.is_empty(),
        "snapshot violates own-snapshot 2.1.0: {errors:?}\n{snapshot:#}"
    );
}

#[test]
fn indexed_own_snapshot_fixtures_match_their_schema_expectations() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let index: Value = serde_json::from_slice(
        &std::fs::read(root.join("fixtures/index.json")).expect("fixture index present"),
    )
    .expect("fixture index is JSON");
    let validator = schema();
    let cases = index["cases"].as_array().expect("fixture cases listed");
    let mut checked = 0;
    for case in cases
        .iter()
        .filter(|case| case["schema"] == "contracts/own-snapshot.schema.json")
    {
        let path = root.join(
            case["fixture"]
                .as_str()
                .expect("own-snapshot fixture path is a string"),
        );
        let value: Value = serde_json::from_slice(
            &std::fs::read(&path).expect("indexed own-snapshot fixture present"),
        )
        .expect("indexed own-snapshot fixture is JSON");
        let expected = case["schema_valid"]
            .as_bool()
            .expect("fixture schema validity is boolean");
        let actual = validator.iter_errors(&value).next().is_none();
        assert_eq!(
            actual, expected,
            "fixture {} disagrees with its indexed schema result",
            case["id"]
        );
        if let Some(fields) = value["own"]["fields"].as_object() {
            for name in [
                "gold",
                "kills",
                "deaths",
                "assists",
                "last_hits",
                "denies",
                "gpm",
                "xpm",
            ] {
                assert!(
                    fields.contains_key(name),
                    "fixture {} lacks own field {name}",
                    case["id"]
                );
            }
        }
        checked += 1;
    }
    assert!(checked > 0, "indexed own-snapshot fixtures were checked");
}

fn admitted(p: &Value) -> Outcome {
    admit(p, TOKEN, Some(owner()), None)
}

#[test]
fn fresh_snapshot_matches_contract() {
    let s = build_snapshot(&admitted(&payload()), src(2000));
    assert_eq!(s["availability"], "fresh");
    assert_eq!(s["schema_version"], "2.1.0");
    assert_eq!(s["own"]["fields"]["hp"]["value"], 850);
    assert_contract(&s);
}

#[test]
fn age_2001_is_stale_and_fields_are_not_fresh() {
    let mut body = payload();
    for (key, value, _) in OWN_STAT_CASES {
        body["player"][key] = json!(value);
    }
    let s = build_snapshot(&admitted(&body), src(2001));
    assert_eq!(s["availability"], "stale");
    assert_eq!(s["own"]["fields"]["level"]["availability"], "stale");
    for (key, _, _) in OWN_STAT_CASES {
        assert_eq!(
            s["own"]["fields"][key]["availability"], "stale",
            "{key} must age with the snapshot"
        );
    }
    assert_contract(&s);
}

#[test]
fn own_player_stats_are_fresh_values_from_the_player_block() {
    let mut body = payload();
    for (key, value, _) in OWN_STAT_CASES {
        body["player"][key] = json!(value);
    }

    let snapshot = build_snapshot(&admitted(&body), src(2000));
    for (key, value, _) in OWN_STAT_CASES {
        assert_eq!(
            snapshot["own"]["fields"][key],
            json!({"value": value, "availability": "fresh"}),
            "{key} must be projected from player"
        );
    }
    assert_contract(&snapshot);
}

#[test]
fn missing_own_player_stats_are_unknown_not_zero() {
    let snapshot = build_snapshot(&admitted(&payload()), src(2000));
    for (key, _, _) in OWN_STAT_CASES {
        assert_eq!(
            snapshot["own"]["fields"][key],
            json!({"value": null, "availability": "unknown"}),
            "missing {key} must remain unknown"
        );
    }
    assert_contract(&snapshot);
}

#[test]
fn wrong_type_own_player_stats_are_invalid() {
    for (key, _, _) in OWN_STAT_CASES {
        let mut body = payload();
        body["player"][key] = json!("7");
        let snapshot = build_snapshot(&admitted(&body), src(2000));
        assert_eq!(
            snapshot["own"]["fields"][key],
            json!({"value": null, "availability": "invalid"}),
            "wrong type for {key} must be invalid"
        );
    }
}

#[test]
fn own_player_stats_reject_negative_out_of_range_and_overflow_values() {
    for (key, _, maximum) in OWN_STAT_CASES {
        for invalid in [json!(-1), json!(maximum + 1), json!(u64::MAX)] {
            let mut body = payload();
            body["player"][key] = invalid;
            let snapshot = build_snapshot(&admitted(&body), src(2000));
            assert_eq!(
                snapshot["own"]["fields"][key],
                json!({"value": null, "availability": "invalid"}),
                "out-of-range {key} must be invalid"
            );
        }

        let mut body = payload();
        body["player"][key] = json!(maximum);
        let snapshot = build_snapshot(&admitted(&body), src(2000));
        assert_eq!(
            snapshot["own"]["fields"][key],
            json!({"value": maximum, "availability": "fresh"}),
            "the inclusive maximum for {key} must be valid"
        );
    }
}

#[test]
fn captured_style_payload_produces_the_own_snapshot_2_1_golden() {
    let mut body = payload();
    for (key, value, _) in OWN_STAT_CASES {
        body["player"][key] = json!(value);
    }
    let snapshot = build_snapshot(&admitted(&body), src(2000));
    let expected = json!({
        "schema_version": "2.1.0",
        "session_epoch": 3,
        "availability": "fresh",
        "source": {"kind": "gsi", "generation": 2, "sequence": 15, "age_ms": 2000},
        "own": {
            "identity_revision": 1,
            "team": "radiant",
            "fields": {
                "hero_id": {"value": "npc_dota_hero_axe", "availability": "fresh"},
                "level": {"value": 12, "availability": "fresh"},
                "hp": {"value": 850, "availability": "fresh"},
                "max_hp": {"value": 1200, "availability": "fresh"},
                "mana_percent": {"value": 43, "availability": "fresh"},
                "alive": {"value": true, "availability": "fresh"},
                "respawn_seconds": {"value": 0, "availability": "fresh"},
                "gold": {"value": 45123, "availability": "fresh"},
                "kills": {"value": 7, "availability": "fresh"},
                "deaths": {"value": 2, "availability": "fresh"},
                "assists": {"value": 9, "availability": "fresh"},
                "last_hits": {"value": 100, "availability": "fresh"},
                "denies": {"value": 8, "availability": "fresh"},
                "gpm": {"value": 456, "availability": "fresh"},
                "xpm": {"value": 512, "availability": "fresh"}
            }
        }
    });
    assert_eq!(snapshot, expected);
    assert_contract(&snapshot);
}

#[test]
fn missing_hp_is_unknown_and_real_zero_stays_zero() {
    let mut p = payload();
    p["hero"].as_object_mut().unwrap().remove("health");
    let s = build_snapshot(&admitted(&p), src(100));
    assert_eq!(
        s["own"]["fields"]["hp"],
        json!({"value": null, "availability": "unknown"})
    );
    assert_eq!(s["own"]["fields"]["level"]["value"], 12);
    assert_contract(&s);

    let mut p = payload();
    p["hero"]["health"] = json!(0);
    p["hero"]["alive"] = json!(false);
    let s = build_snapshot(&admitted(&p), src(100));
    assert_eq!(
        s["own"]["fields"]["hp"],
        json!({"value": 0, "availability": "fresh"})
    );
    assert_contract(&s);
}

#[test]
fn hp_above_max_is_invalid_field() {
    let mut p = payload();
    p["hero"]["health"] = json!(1300);
    let s = build_snapshot(&admitted(&p), src(100));
    assert_eq!(s["own"]["fields"]["hp"]["availability"], "invalid");
    assert_contract(&s);
}

#[test]
fn wrong_or_missing_token_is_rejected_without_snapshot_values() {
    let mut p = payload();
    p["auth"]["token"] = json!("x".repeat(43));
    assert_eq!(admitted(&p), Outcome::Rejected(Rejection::AuthRejected));
    p.as_object_mut().unwrap().remove("auth");
    let o = admitted(&p);
    assert_eq!(o, Outcome::Rejected(Rejection::AuthRejected));
    let s = build_snapshot(&o, src(100));
    assert_eq!(s["availability"], "invalid");
    assert!(s["own"].is_null());
    assert_contract(&s);
}

#[test]
fn wrong_provider_is_invalid() {
    let mut p = payload();
    p["provider"]["appid"] = json!(730);
    assert_eq!(admitted(&p), Outcome::Rejected(Rejection::SourceInvalid));
}

#[test]
fn menu_spectator_and_custom_game_are_not_admitted() {
    let mut p = payload();
    p["player"]["activity"] = json!("menu");
    assert_eq!(admitted(&p), Outcome::NotPlaying);

    let mut p = payload();
    for (key, value, _) in OWN_STAT_CASES {
        p["player"][key] = json!(value);
    }
    p["player"]["team2"] = json!({});
    let spectator = admitted(&p);
    assert_eq!(
        spectator,
        Outcome::Unsupported(UnsupportedReason::Spectating)
    );
    let spectator_snapshot = build_snapshot(&spectator, src(100));
    assert_eq!(spectator_snapshot["availability"], "unsupported");
    assert!(spectator_snapshot["own"].is_null());
    assert_contract(&spectator_snapshot);

    let mut p = payload();
    p["map"]["customgamename"] = json!("some_custom_map");
    let o = admitted(&p);
    assert_eq!(o, Outcome::Unsupported(UnsupportedReason::CustomGame));
    let s = build_snapshot(&o, src(100));
    assert_eq!(s["availability"], "unsupported");
    assert_contract(&s);
}

#[test]
fn ownership_requires_platform_identity_match() {
    let mut p = payload();
    for (key, value, _) in OWN_STAT_CASES {
        p["player"][key] = json!(value);
    }
    let unverified = admit(&p, TOKEN, None, None);
    match &unverified {
        Outcome::Admitted(a) => assert_eq!(a.ownership, Ownership::Unverified),
        other => panic!("expected admitted, got {other:?}"),
    }
    let s = build_snapshot(&unverified, src(100));
    assert_eq!(s["availability"], "invalid");
    assert_contract(&s);

    let mismatched = match admit(&p, TOKEN, Some(owner() + 1), None) {
        Outcome::Admitted(a) => {
            assert_eq!(a.ownership, Ownership::Mismatch);
            Outcome::Admitted(a)
        }
        other => panic!("expected admitted, got {other:?}"),
    };
    let mismatch_snapshot = build_snapshot(&mismatched, src(100));
    assert!(mismatch_snapshot["own"].is_null());
    assert_contract(&mismatch_snapshot);
    match admit(&p, TOKEN, Some(owner()), Some(owner() + 7)) {
        Outcome::Admitted(a) => assert_eq!(a.ownership, Ownership::Mismatch),
        other => panic!("expected admitted, got {other:?}"),
    }
}

#[test]
fn draft_phase_keeps_session_but_exposes_no_vitals() {
    let mut p = payload();
    p["map"]["game_state"] = json!("DOTA_GAMERULES_STATE_HERO_SELECTION");
    let s = build_snapshot(&admitted(&p), src(100));
    assert_eq!(s["availability"], "unknown");
    assert!(s["own"].is_null());
    assert_eq!(s["session_epoch"], 3);
    assert_contract(&s);
}

#[test]
fn unknown_game_state_is_unsupported_not_mapped() {
    let mut p = payload();
    p["map"]["game_state"] = json!("DOTA_GAMERULES_STATE_SOMETHING_NEW");
    assert_eq!(
        admitted(&p),
        Outcome::Unsupported(UnsupportedReason::UnknownGameState)
    );
}
