mod support;

use kohbor_capture::Coverage;
use kohbor_risk::{OwnGsi, RiskEngine, RiskEvent, TickInput};
use support::{load, Json};

fn replay() -> String {
    let index = load("index.json");
    let mut cases = Vec::new();
    for entry in index.get("fixtures").array() {
        if entry.get("kind").string() != "trace" {
            continue;
        }
        let id = entry.get("id").string();
        let trace = load(entry.get("file").string());
        let mut engine = RiskEngine::new();
        let mut alerts = Vec::new();
        for tick in trace.get("ticks").array() {
            let now = tick.get("t_ms").number() as u64;
            let coverage = match tick.get("coverage").string() {
                "valid" => Coverage::Valid,
                "stale" => Coverage::Stale,
                "unavailable" => Coverage::Unavailable,
                other => panic!("{id}: coverage {other}"),
            };
            let own = match tick.get("own") {
                Json::Null => None,
                own => Some(OwnGsi {
                    alive: own.get("alive").boolean(),
                    hp_pct: None,
                    in_match: own.get("in_match").boolean(),
                    age_ms: if own.get("fresh").boolean() { 0 } else { 3000 },
                }),
            };
            let count = u8::try_from(tick.get("sightings").array().len()).expect("visible count");
            let out = engine.tick(&TickInput {
                now_ms: now,
                coverage,
                visible: Some(count),
                own,
                gsi_events: vec![],
            });
            alerts.extend(
                out.events
                    .iter()
                    .filter(|e| **e == RiskEvent::RiskAlert)
                    .map(|_| now),
            );
        }
        let proposed: Vec<u64> = trace
            .get("expected")
            .get("events")
            .array()
            .iter()
            .filter(|e| e.get("kind").string() == "would_admit")
            .map(|e| e.get("t_ms").number() as u64)
            .collect();
        let same = alerts.len() == proposed.len()
            && alerts
                .iter()
                .zip(&proposed)
                .all(|(a, b)| a.abs_diff(*b) <= 2000);
        cases.push((
            id.to_owned(),
            alerts,
            proposed,
            if same { "same" } else { "differs" },
        ));
    }
    cases.sort_by(|a, b| a.0.cmp(&b.0));
    fn list(v: &[u64]) -> String {
        if v.is_empty() {
            return "[]".into();
        }
        format!(
            "[\n{}\n   ]",
            v.iter()
                .map(|x| format!("    {x}"))
                .collect::<Vec<_>>()
                .join(",\n")
        )
    }
    let rows = cases.iter().map(|(id, alerts, proposed, verdict)| format!(
        "  {{\n   \"id\": \"{id}\",\n   \"proposed_admit_ms\": {},\n   \"risk_alert_ms\": {},\n   \"verdict\": \"{verdict}\"\n  }}", list(proposed), list(alerts)
    )).collect::<Vec<_>>().join(",\n");
    format!(
        "{{\n \"cases\": [\n{rows}\n ],\n \"status\": \"characterization_of_current_code\"\n}}\n"
    )
}

#[test]
fn current_engine_replay_matches_characterization() {
    let expected = load("current-engine-replay.json");
    let actual = support::parse(&replay()).expect("generated JSON");
    assert_eq!(actual, expected);
}

#[test]
#[ignore]
fn print_current_engine_replay() {
    print!("{}", replay());
}
