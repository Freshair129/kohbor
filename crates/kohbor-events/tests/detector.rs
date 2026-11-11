use kohbor_events::{Detector, Event, EventId, Phase, Tick};
use serde_json::Value;
use std::{collections::HashSet, fs, path::PathBuf};

const IDS: [EventId; 12] = [
    EventId::MultiKill,
    EventId::Streak,
    EventId::FirstBlood,
    EventId::HeroKill,
    EventId::HeroDeath,
    EventId::LevelMilestone,
    EventId::HealthLow,
    EventId::ManaLow,
    EventId::MatchEnded,
    EventId::MatchStarted,
    EventId::HeroRespawn,
    EventId::HealthDanger,
];

fn json(path: &str) -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_slice(&fs::read(root.join(path)).unwrap()).unwrap()
}

fn catalog_row(id: &str) -> Value {
    json("contracts/event-catalog.json")["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap()
        .clone()
}

fn tick(at_ms: u64) -> Tick {
    Tick {
        at_ms,
        session_epoch: 1,
        phase: Phase::Live,
        trusted: true,
        kills: Some(0),
        combined_score: Some(0),
        level: Some(1),
        hp: Some(1000),
        max_hp: Some(1000),
        mana_percent: Some(100),
        alive: Some(true),
    }
}

fn has(events: &[Event], id: EventId) -> bool {
    events.iter().any(|e| e.id == id)
}
fn tier(events: &[Event], id: EventId) -> Option<u8> {
    events.iter().find(|e| e.id == id).and_then(|e| e.tier)
}

#[test]
fn enum_matches_active_gsi_catalog() {
    let catalog = json("contracts/event-catalog.json");
    let rows: Vec<_> = catalog["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["source"] == "gsi" && v["status"] == "active")
        .collect();
    let actual: HashSet<_> = IDS.iter().map(|id| id.as_str()).collect();
    let expected: HashSet<_> = rows.iter().map(|v| v["id"].as_str().unwrap()).collect();
    assert_eq!(actual.len(), IDS.len());
    assert_eq!(actual, expected);
    for id in IDS {
        let row = rows.iter().find(|v| v["id"] == id.as_str()).unwrap();
        assert_eq!(id.rank(), row["ordinary_rank"].as_u64().map(|n| n as u32));
        assert_eq!(id.voiced(), row["voiced_default"].as_bool().unwrap());
        assert_eq!(id.is_critical(), row["category"] == "critical");
        let range = row.get("tier_range").map(|v| {
            (
                v["min"].as_u64().unwrap() as u8,
                v["max"].as_u64().unwrap() as u8,
            )
        });
        assert_eq!(id.tier_range(), range);
    }
}

#[test]
fn first_tick_and_epoch_change_only_establish_baselines() {
    let mut d = Detector::new();
    let mut t = tick(0);
    t.kills = Some(5);
    t.level = Some(12);
    t.alive = Some(false);
    assert!(d.observe(&t).is_empty());
    t.at_ms = 1;
    t.session_epoch = 2;
    t.kills = Some(10);
    t.level = Some(25);
    t.alive = Some(true);
    assert!(d.observe(&t).is_empty());
    // Resume requires the intake to supply a new epoch; no separate resume signal exists.
    t.at_ms = 2;
    t.session_epoch = 3;
    t.phase = Phase::PostGame;
    t.alive = Some(false);
    assert!(d.observe(&t).is_empty());
}

#[test]
fn kill_delta_counts_each_kill_once_at_arrival() {
    let multi = catalog_row("multi_kill");
    let streak = catalog_row("streak");
    let mmin = multi["tier_range"]["min"].as_u64().unwrap() as u32;
    let mmax = multi["tier_range"]["max"].as_u64().unwrap() as u32;
    let smin = streak["tier_range"]["min"].as_u64().unwrap() as u32;
    let smax = streak["tier_range"]["max"].as_u64().unwrap() as u32;
    for n in [1, 2, 3, 7, 12] {
        let mut d = Detector::new();
        let mut t = tick(0);
        d.observe(&t);
        t.at_ms = 100;
        t.kills = Some(n);
        t.combined_score = Some(5);
        let out = d.observe(&t);
        assert_eq!(out.iter().filter(|e| e.id == EventId::HeroKill).count(), 1);
        assert!(out.iter().all(|e| e.at_ms == 100));
        assert_eq!(
            tier(&out, EventId::MultiKill),
            (n >= mmin).then_some(n.min(mmax) as u8)
        );
        assert_eq!(
            tier(&out, EventId::Streak),
            (n >= smin).then_some(n.min(smax) as u8)
        );
    }
}

#[test]
fn chain_window_is_inclusive_rolling_and_survives_death() {
    let row = catalog_row("multi_kill");
    let window = row["tunables"]["window_ms"]["default"].as_u64().unwrap();
    let cap = row["tier_range"]["max"].as_u64().unwrap() as u8;
    let mut d = Detector::new();
    let mut t = tick(0);
    d.observe(&t);
    for (time, kills, alive, expected) in [
        (100, 1, true, None),
        (100 + window, 2, true, Some(2)),
        (101 + window, 2, false, None),
        (100 + 2 * window, 3, false, Some(3)),
        (101 + 3 * window, 4, true, None),
        (102 + 3 * window, 5, true, Some(2)),
        (103 + 3 * window, 9, true, Some(cap)),
    ] {
        t.at_ms = time;
        t.kills = Some(kills);
        t.alive = Some(alive);
        t.combined_score = Some(10);
        let out = d.observe(&t);
        assert_eq!(tier(&out, EventId::MultiKill), expected, "time {time}");
    }
}

#[test]
fn streak_reemits_caps_and_resets_on_own_death() {
    let row = catalog_row("streak");
    let min = row["tier_range"]["min"].as_u64().unwrap() as u32;
    let max = row["tier_range"]["max"].as_u64().unwrap() as u32;
    let mut d = Detector::new();
    let mut t = tick(0);
    d.observe(&t);
    for n in 1..=12 {
        t.at_ms = n as u64 * 100;
        t.kills = Some(n);
        t.combined_score = Some(20);
        let out = d.observe(&t);
        assert_eq!(
            tier(&out, EventId::Streak),
            (n >= min).then_some(n.min(max) as u8)
        );
    }
    t.at_ms = 1300;
    t.alive = Some(false);
    assert!(has(&d.observe(&t), EventId::HeroDeath));
    t.alive = Some(true);
    t.at_ms = 1400;
    d.observe(&t);
    for n in 13..=15 {
        t.at_ms += 100;
        t.kills = Some(n);
        let out = d.observe(&t);
        assert_eq!(tier(&out, EventId::Streak), (n == 15).then_some(3));
    }
}

#[test]
fn first_blood_requires_both_exact_transitions() {
    for (old_score, new_score, old_kills, new_kills, expected) in [
        (0, 1, 0, 1, true),
        (0, 1, 0, 0, false),
        (0, 2, 0, 1, false),
        (1, 2, 0, 1, false),
        (0, 1, 0, 2, false),
        (0, 1, 1, 2, false),
    ] {
        let mut d = Detector::new();
        let mut t = tick(0);
        t.kills = Some(old_kills);
        t.combined_score = Some(old_score);
        d.observe(&t);
        t.at_ms = 1;
        t.kills = Some(new_kills);
        t.combined_score = Some(new_score);
        assert_eq!(has(&d.observe(&t), EventId::FirstBlood), expected);
    }
}

#[test]
fn death_and_respawn_are_edges() {
    let mut d = Detector::new();
    let mut t = tick(0);
    d.observe(&t);
    for (alive, death, respawn) in [
        (false, true, false),
        (false, false, false),
        (true, false, true),
        (true, false, false),
    ] {
        t.at_ms += 1;
        t.alive = Some(alive);
        let out = d.observe(&t);
        assert_eq!(has(&out, EventId::HeroDeath), death);
        assert_eq!(has(&out, EventId::HeroRespawn), respawn);
    }
    assert!(!EventId::HeroRespawn.voiced());
}

#[test]
fn level_milestones_only_on_crossing() {
    let catalog = json("contracts/event-catalog.json");
    let row = catalog["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "level_milestone")
        .unwrap();
    for level in row["tunables"]["levels"]["default"].as_array().unwrap() {
        let level = level.as_u64().unwrap() as u32;
        let mut d = Detector::new();
        let mut t = tick(0);
        t.level = Some(level - 1);
        d.observe(&t);
        t.at_ms = 1;
        t.level = Some(level);
        assert!(has(&d.observe(&t), EventId::LevelMilestone));
        t.at_ms = 2;
        assert!(!has(&d.observe(&t), EventId::LevelMilestone));
        let mut fresh = Detector::new();
        assert!(fresh.observe(&t).is_empty());
    }
}

#[test]
fn low_resources_rearm_at_boundary_and_respawn() {
    for (id, name) in [
        (EventId::HealthLow, "health_low"),
        (EventId::ManaLow, "mana_low"),
    ] {
        let row = catalog_row(name);
        let threshold = row["tunables"]["threshold_percent"]["default"]
            .as_u64()
            .unwrap() as u32;
        let rearm = row["tunables"]["rearm_percent"]["default"]
            .as_u64()
            .unwrap() as u32;
        let mut d = Detector::new();
        let mut t = tick(0);
        d.observe(&t);
        for (value, expected) in [
            (threshold, true),
            (threshold, false),
            (rearm - 1, false),
            (threshold, false),
            (rearm, false),
            (threshold, true),
        ] {
            t.at_ms += 1;
            if id == EventId::HealthLow {
                t.hp = Some(value * 10)
            } else {
                t.mana_percent = Some(value)
            }
            assert_eq!(has(&d.observe(&t), id), expected, "{id:?} {value}");
        }
        t.at_ms += 1;
        t.alive = Some(false);
        d.observe(&t);
        t.at_ms += 1;
        t.alive = Some(true);
        // The spec leaves the exact respawn-tick firing order unspecified.
        let respawn = d.observe(&t);
        t.at_ms += 1;
        assert!(
            has(&respawn, id) || has(&d.observe(&t), id),
            "{id:?} after respawn"
        );
    }
}

#[test]
fn phase_transitions_emit_match_events() {
    for phase in [Phase::Draft, Phase::Pregame, Phase::PostGame] {
        let mut d = Detector::new();
        let mut t = tick(0);
        t.phase = phase;
        assert!(d.observe(&t).is_empty());
        t.at_ms = 1;
        t.phase = Phase::Live;
        assert!(has(&d.observe(&t), EventId::MatchStarted));
        t.at_ms = 2;
        assert!(!has(&d.observe(&t), EventId::MatchStarted));
        t.at_ms = 3;
        t.phase = Phase::PostGame;
        assert!(has(&d.observe(&t), EventId::MatchEnded));
        t.at_ms = 4;
        assert!(!has(&d.observe(&t), EventId::MatchEnded));
    }
}

#[test]
fn health_danger_golden_traces() {
    let fixture = json("fixtures/health-danger-golden.json");
    for trace in fixture["traces"].as_array().unwrap() {
        let mut d = Detector::new();
        let mut actual = Vec::new();
        for sample in trace["samples"].as_array().unwrap() {
            let mut t = tick(sample["t_ms"].as_u64().unwrap());
            t.hp = sample["hp"].as_u64().map(|n| n as u32);
            t.max_hp = sample["max_hp"].as_u64().map(|n| n as u32);
            t.alive = sample["alive"].as_bool();
            t.trusted = sample["availability"] == "fresh";
            actual.extend(
                d.observe(&t)
                    .into_iter()
                    .filter(|e| e.id == EventId::HealthDanger)
                    .map(|e| e.at_ms),
            );
        }
        let expected: Vec<_> = trace["expected_cue_t_ms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        assert_eq!(actual, expected, "trace {}", trace["id"]);
    }
}

#[test]
fn danger_rearms_on_new_session() {
    let mut d = Detector::new();
    let mut t = tick(0);
    d.observe(&t);
    t.at_ms = 1000;
    t.hp = Some(200);
    assert!(has(&d.observe(&t), EventId::HealthDanger));
    t.at_ms = 22000;
    t.session_epoch = 2;
    t.hp = Some(1000);
    assert!(d.observe(&t).is_empty());
    t.at_ms = 23000;
    t.hp = Some(200);
    assert!(has(&d.observe(&t), EventId::HealthDanger));
}

#[test]
fn untrusted_and_unknown_fields_do_not_change_baselines() {
    let mut d = Detector::new();
    let mut t = tick(0);
    d.observe(&t);
    t.at_ms = 1;
    t.trusted = false;
    t.kills = Some(9);
    t.alive = Some(false);
    t.level = Some(20);
    assert!(d.observe(&t).is_empty());
    t.at_ms = 2;
    t.trusted = true;
    t.kills = None;
    t.alive = None;
    t.level = None;
    t.combined_score = None;
    t.hp = None;
    t.max_hp = None;
    t.mana_percent = None;
    assert!(d.observe(&t).is_empty());
    t.at_ms = 3;
    t.kills = Some(1);
    t.combined_score = Some(1);
    t.alive = Some(false);
    t.level = Some(6);
    let out = d.observe(&t);
    for id in [
        EventId::HeroKill,
        EventId::FirstBlood,
        EventId::HeroDeath,
        EventId::LevelMilestone,
    ] {
        assert!(has(&out, id), "{id:?}");
    }
}

#[test]
fn kill_counter_regression_resets_baseline() {
    let mut d = Detector::new();
    let mut t = tick(0);
    t.kills = Some(10);
    d.observe(&t);
    t.at_ms = 1;
    t.kills = Some(3);
    assert!(!has(&d.observe(&t), EventId::HeroKill));
    t.at_ms = 2;
    t.kills = Some(4);
    assert!(has(&d.observe(&t), EventId::HeroKill));
}

#[test]
fn output_is_unique_and_in_declaration_order() {
    let mut d = Detector::new();
    let mut t = tick(0);
    d.observe(&t);
    t.at_ms = 100;
    t.kills = Some(3);
    t.combined_score = Some(3);
    t.level = Some(6);
    t.hp = Some(200);
    t.mana_percent = Some(15);
    let out = d.observe(&t);
    for id in [
        EventId::MultiKill,
        EventId::Streak,
        EventId::HeroKill,
        EventId::LevelMilestone,
        EventId::HealthLow,
        EventId::ManaLow,
        EventId::HealthDanger,
    ] {
        assert!(has(&out, id), "{id:?}");
    }
    let positions: Vec<_> = out
        .iter()
        .map(|e| IDS.iter().position(|id| *id == e.id).unwrap())
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn non_live_kill_increases_do_not_emit() {
    // The spec does not explicitly specify phase gating for kills; this follows the task's requested reading.
    for phase in [Phase::Draft, Phase::Pregame, Phase::PostGame] {
        let mut d = Detector::new();
        let mut t = tick(0);
        t.phase = phase;
        d.observe(&t);
        t.at_ms = 1;
        t.kills = Some(3);
        t.combined_score = Some(3);
        assert!(d.observe(&t).is_empty(), "{phase:?}");
    }
}
