//! Synthetic contract tests (evidence class: synthetic). Payloads below are
//! fabricated test inputs, not captured game data, and never ship in production.

use kohbor_gsi::admission::{Ownership, Rejection, UnsupportedReason};
use kohbor_gsi::{admit, build_snapshot, steamid64_from_account_id, Outcome, SourceState};
use serde_json::{json, Value};

const TOKEN: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO-_";
const ACCOUNT: u32 = 12_345;

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
        "snapshot violates own-snapshot 2.0.0: {errors:?}\n{snapshot:#}"
    );
}

fn admitted(p: &Value) -> Outcome {
    admit(p, TOKEN, Some(owner()), None)
}

#[test]
fn fresh_snapshot_matches_contract() {
    let s = build_snapshot(&admitted(&payload()), src(2000));
    assert_eq!(s["availability"], "fresh");
    assert_eq!(s["own"]["fields"]["hp"]["value"], 850);
    assert_contract(&s);
}

#[test]
fn age_2001_is_stale_and_fields_are_not_fresh() {
    let s = build_snapshot(&admitted(&payload()), src(2001));
    assert_eq!(s["availability"], "stale");
    assert_eq!(s["own"]["fields"]["level"]["availability"], "stale");
    assert_contract(&s);
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
    p["player"]["team2"] = json!({});
    assert_eq!(
        admitted(&p),
        Outcome::Unsupported(UnsupportedReason::Spectating)
    );

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
    let p = payload();
    let unverified = admit(&p, TOKEN, None, None);
    match &unverified {
        Outcome::Admitted(a) => assert_eq!(a.ownership, Ownership::Unverified),
        other => panic!("expected admitted, got {other:?}"),
    }
    let s = build_snapshot(&unverified, src(100));
    assert_eq!(s["availability"], "invalid");
    assert_contract(&s);

    match admit(&p, TOKEN, Some(owner() + 1), None) {
        Outcome::Admitted(a) => assert_eq!(a.ownership, Ownership::Mismatch),
        other => panic!("expected admitted, got {other:?}"),
    }
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
