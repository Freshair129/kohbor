use kohbor_events::Config;
use kohbor_lab::sandbox::{compare, replay_json, ticks_from_json};
use kohbor_lab::{embedded, merge_override};
use serde_json::{json, Value};

fn golden() -> Value {
    let path = format!(
        "{}/../../fixtures/health-danger-golden.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// Ticks for one golden trace in the sandbox's JSON tick format.
fn ticks_of(trace_id: &str) -> Value {
    let fixture = golden();
    let trace = fixture["traces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == trace_id)
        .unwrap();
    let mut ticks = Vec::new();
    for sample in trace["samples"].as_array().unwrap() {
        ticks.push(json!({
            "at_ms": sample["t_ms"], "session_epoch": 1, "phase": "live",
            "trusted": sample["availability"] == "fresh",
            "kills": 0, "deaths": 0, "kill_streak": null, "combined_score": 0, "level": 1,
            "hp": sample["hp"], "max_hp": sample["max_hp"], "mana_percent": 100,
            "alive": sample["alive"],
        }));
    }
    Value::Array(ticks)
}

#[test]
fn tick_parser_keeps_unknown_unknown() {
    let ticks = ticks_from_json(&json!([
        {"at_ms": 0, "phase": "live", "hp": null, "max_hp": 100, "alive": null}
    ]))
    .unwrap();
    assert_eq!(ticks[0].hp, None);
    assert_eq!(ticks[0].alive, None);
    assert_eq!(ticks[0].max_hp, Some(100));
    assert!(
        ticks[0].trusted,
        "trusted defaults to true for offline replay"
    );
    assert!(ticks_from_json(&json!([{"at_ms": 0, "phase": "weird"}])).is_err());
    assert!(ticks_from_json(&json!([{"at_ms": -1, "phase": "live"}])).is_err());
    assert!(ticks_from_json(&json!({"at_ms": 0})).is_err());
}

#[test]
fn replay_reproduces_the_golden_cue_times() {
    let report = replay_json(
        &ticks_from_json(&ticks_of("death-rearm-and-cooldown")).unwrap(),
        &Config::default(),
    );
    let cues: Vec<u64> = report["fired"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["id"] == "health_danger")
        .map(|f| f["at_ms"].as_u64().unwrap())
        .collect();
    assert_eq!(cues, vec![2000, 25000]);
    assert_eq!(report["ticks"].as_array().unwrap().len(), 9);
    assert!(report["ticks"][0]["trace"]["events"].is_array());
}

#[test]
fn comparison_shows_what_an_edit_loses_and_gains() {
    let ticks = ticks_from_json(&ticks_of("burst")).unwrap();
    let same = compare(&ticks, &embedded().config(), &embedded().config());
    assert!(
        same["lost"].as_array().unwrap().is_empty()
            && same["gained"].as_array().unwrap().is_empty()
    );

    // Lowering the danger line to 10% hp makes the 24%-hp burst cue disappear.
    let stricter = merge_override(
        &json!({"profile_version": "1.0", "taxonomy_version": "1.1",
                "events": {"health_danger": {"params": {"max_hp_percent": 10}}}})
        .to_string(),
    )
    .unwrap();
    let diff = compare(&ticks, &embedded().config(), &stricter.config());
    let lost: Vec<&str> = diff["lost"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(lost, vec!["health_danger"]);
    assert!(diff["gained"].as_array().unwrap().is_empty());
}
