//! Replay of minimized payloads derived from the redacted M0 capture of 2026-11-11
//! (one bot match, Dota 2 provider version 48). Identifiers were replaced by
//! placeholders before the files were written; tests substitute synthetic values.
//! Evidence class: synthetic replay of captured shapes — not native/live acceptance.

use kohbor_gsi::admission::Phase;
use kohbor_gsi::{admit, build_snapshot, steamid64_from_account_id, Outcome, SourceState};
use serde_json::Value;

const TOKEN: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO-_";

fn owner() -> u64 {
    steamid64_from_account_id(424_242)
}

fn load(name: &str) -> Value {
    let path = format!("{}/tests/captured/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path)
        .unwrap()
        .replace("<TOKEN>", TOKEN)
        .replace("<STEAMID>", &owner().to_string())
        .replace("<MATCHID>", "8000000001");
    serde_json::from_str(&text).unwrap()
}

fn phase(name: &str) -> Phase {
    match admit(&load(name), TOKEN, Some(owner()), None) {
        Outcome::Admitted(a) => a.phase,
        other => panic!("{name}: expected admitted, got {other:?}"),
    }
}

fn src() -> SourceState {
    SourceState {
        generation: 1,
        sequence: Some(1),
        age_ms: Some(300),
        session_epoch: Some(1),
        identity_revision: 1,
    }
}

#[test]
fn menu_payload_is_not_playing() {
    assert_eq!(
        admit(&load("menu"), TOKEN, Some(owner()), None),
        Outcome::NotPlaying
    );
}

#[test]
fn every_captured_game_state_maps_to_a_phase() {
    assert_eq!(phase("hero-selection-no-pick"), Phase::Draft);
    assert_eq!(phase("team-showcase"), Phase::Draft);
    assert_eq!(phase("pre-game"), Phase::Pregame);
    assert_eq!(phase("in-progress-alive"), Phase::Live);
    assert_eq!(phase("in-progress-paused"), Phase::Live);
    assert_eq!(phase("post-game"), Phase::PostGame);
}

#[test]
fn empty_hero_name_during_selection_is_missing_not_invalid() {
    match admit(&load("hero-selection-no-pick"), TOKEN, Some(owner()), None) {
        Outcome::Admitted(a) => {
            assert_eq!(a.hero.hero_id, kohbor_gsi::admission::Field::Missing)
        }
        other => panic!("expected admitted, got {other:?}"),
    }
}

#[test]
fn dead_hero_reports_real_zero_hp_and_respawn() {
    let s = build_snapshot(
        &admit(&load("in-progress-dead"), TOKEN, Some(owner()), None),
        src(),
    );
    assert_eq!(s["availability"], "fresh");
    assert_eq!(s["own"]["fields"]["hp"]["value"], 0);
    assert_eq!(s["own"]["fields"]["alive"]["value"], false);
    assert!(
        s["own"]["fields"]["respawn_seconds"]["value"]
            .as_i64()
            .unwrap()
            > 0
    );
}

#[test]
fn captured_identity_must_match_platform_identity() {
    let other = owner() + 1;
    match admit(&load("in-progress-alive"), TOKEN, Some(other), None) {
        Outcome::Admitted(a) => {
            assert_eq!(a.ownership, kohbor_gsi::admission::Ownership::Mismatch)
        }
        o => panic!("{o:?}"),
    }
}
