//! `observe_traced` must never change what `observe` emits, and for the explained events
//! the guard rows must agree with the emission (KOHBOR-LAB.md section 3).

use kohbor_events::{Detector, Event, EventId, Phase, Tick};
use serde_json::Value;

fn base(at_ms: u64) -> Tick {
    Tick {
        at_ms,
        session_epoch: 1,
        phase: Phase::Live,
        trusted: true,
        kills: Some(0),
        deaths: Some(0),
        kill_streak: None,
        combined_score: Some(0),
        level: Some(1),
        hp: Some(1000),
        max_hp: Some(1000),
        mana_percent: Some(100),
        alive: Some(true),
    }
}

fn read_json(path: &str) -> Value {
    let full = format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_slice(&std::fs::read(full).unwrap()).unwrap()
}

/// Run `ticks` through a plain and a traced detector in lockstep and check the laws.
fn check_lockstep(ticks: &[Tick]) -> Vec<Vec<Event>> {
    let mut plain = Detector::new();
    let mut traced = Detector::new();
    let mut all = Vec::new();
    for tick in ticks {
        let expected = plain.observe(tick);
        let (events, trace) = traced.observe_traced(tick);
        assert_eq!(events, expected, "events differ at {} ms", tick.at_ms);
        assert_eq!(trace.at_ms, tick.at_ms);
        let evaluated = trace.status == "evaluated";
        for id in [
            EventId::HealthDanger,
            EventId::HealthLow,
            EventId::ManaLow,
            EventId::LevelMilestone,
            EventId::HeroKill,
            EventId::HeroDeath,
            EventId::ClutchKill,
            EventId::PosthumousKill,
            EventId::ComebackKill,
        ] {
            let row = trace
                .event(id.as_str())
                .unwrap_or_else(|| panic!("no trace row for {}", id.as_str()));
            let fired = events.iter().any(|event| event.id == id);
            assert_eq!(
                row.fired,
                fired,
                "{} fired flag at {} ms",
                id.as_str(),
                tick.at_ms
            );
            if evaluated {
                assert_eq!(
                    row.all_guards_pass(),
                    fired,
                    "{} guards vs emission at {} ms: {:?}",
                    id.as_str(),
                    tick.at_ms,
                    row.guards
                );
            } else {
                assert!(!fired);
            }
        }
        all.push(events);
    }
    all
}

#[test]
fn health_danger_golden_traces_are_explained_consistently() {
    let fixture = read_json("fixtures/health-danger-golden.json");
    for trace in fixture["traces"].as_array().unwrap() {
        let ticks: Vec<Tick> = trace["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|sample| {
                let mut t = base(sample["t_ms"].as_u64().unwrap());
                t.hp = sample["hp"].as_u64().map(|n| n as u32);
                t.max_hp = sample["max_hp"].as_u64().map(|n| n as u32);
                t.alive = sample["alive"].as_bool();
                t.trusted = sample["availability"] == "fresh";
                t
            })
            .collect();
        let per_tick = check_lockstep(&ticks);
        let actual: Vec<u64> = per_tick
            .iter()
            .flatten()
            .filter(|event| event.id == EventId::HealthDanger)
            .map(|event| event.at_ms)
            .collect();
        let expected: Vec<u64> = trace["expected_cue_t_ms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        assert_eq!(actual, expected, "trace {}", trace["id"]);
    }
}

#[test]
fn danger_burst_shows_the_values_that_decided_it() {
    let mut d = Detector::new();
    let mut t = base(0);
    d.observe_traced(&t);
    t.at_ms = 1000;
    t.hp = Some(800);
    let (_, quiet) = d.observe_traced(&t);
    let row = quiet.event("health_danger").unwrap();
    assert!(!row.fired);
    let percent = row
        .guards
        .iter()
        .find(|g| g.name.starts_with("hp %"))
        .unwrap();
    assert_eq!(percent.lhs, Some(80.0));
    assert_eq!(percent.rhs, Some(25.0));
    assert_eq!(percent.pass, Some(false));

    t.at_ms = 2000;
    t.hp = Some(200);
    let (events, fired) = d.observe_traced(&t);
    assert!(events.iter().any(|e| e.id == EventId::HealthDanger));
    let row = fired.event("health_danger").unwrap();
    let drop = row
        .guards
        .iter()
        .find(|g| g.name.starts_with("drop"))
        .unwrap();
    assert_eq!(drop.lhs, Some(80.0), "largest drop in the 2000 ms window");
    assert_eq!(drop.rhs, Some(15.0));
    assert!(row.all_guards_pass());
}

#[test]
fn unknown_hp_is_unknown_not_zero() {
    let mut d = Detector::new();
    d.observe_traced(&base(0));
    let mut t = base(100);
    t.hp = None;
    let (_, trace) = d.observe_traced(&t);
    let row = trace.event("health_danger").unwrap();
    let percent = row
        .guards
        .iter()
        .find(|g| g.name.starts_with("hp %"))
        .unwrap();
    assert_eq!(percent.lhs, None);
    assert_eq!(percent.pass, None);
    assert!(!row.all_guards_pass());
}

#[test]
fn untrusted_and_baseline_ticks_are_labelled() {
    let mut d = Detector::new();
    let (_, first) = d.observe_traced(&base(0));
    assert_eq!(first.status, "baseline");
    let mut t = base(10);
    t.trusted = false;
    let (events, trace) = d.observe_traced(&t);
    assert!(events.is_empty());
    assert_eq!(trace.status, "untrusted");
}

#[test]
fn threshold_events_and_milestones_agree_with_guards() {
    let mut ticks = Vec::new();
    let mut t = base(0);
    ticks.push(t.clone());
    // Health walks down through low, recovers past the re-arm line, and falls again.
    for (at, hp, mana, level, kills, deaths, alive) in [
        (200, 900, 90, 1, 0, 0, true),
        (400, 190, 14, 5, 1, 0, true),
        (600, 150, 10, 6, 2, 0, true),
        (800, 400, 30, 6, 2, 0, true),
        (1000, 150, 10, 12, 3, 0, true),
        (1200, 0, 10, 12, 3, 1, false),
        (1400, 1000, 100, 12, 3, 1, true),
        (1600, 100, 5, 18, 5, 1, true),
    ] {
        t.at_ms = at;
        t.hp = Some(hp);
        t.mana_percent = Some(mana);
        t.level = Some(level);
        t.kills = Some(kills);
        t.deaths = Some(deaths);
        t.alive = Some(alive);
        ticks.push(t.clone());
    }
    let per_tick = check_lockstep(&ticks);
    let ids: Vec<EventId> = per_tick.iter().flatten().map(|e| e.id).collect();
    for expected in [
        EventId::HealthLow,
        EventId::ManaLow,
        EventId::LevelMilestone,
        EventId::HeroKill,
        EventId::HeroDeath,
    ] {
        assert!(
            ids.contains(&expected),
            "scenario never produced {expected:?}"
        );
    }
}

#[test]
fn epoch_change_and_late_samples_stay_consistent() {
    let mut a = base(0);
    let mut ticks = vec![a.clone()];
    a.at_ms = 100;
    a.hp = Some(100);
    ticks.push(a.clone());
    a.at_ms = 50; // late sample: new baseline
    ticks.push(a.clone());
    a.at_ms = 200;
    a.session_epoch = 2;
    a.hp = Some(900);
    ticks.push(a.clone());
    a.at_ms = 300;
    a.hp = Some(100);
    ticks.push(a.clone());
    check_lockstep(&ticks);
}
