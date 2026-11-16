use kohbor_events::Config;
use kohbor_lab::{embedded, embedded_sha256, merge_override, Effective, ProfileError, Source};
use serde_json::{json, Value};

fn catalog() -> Value {
    let path = format!(
        "{}/../../contracts/event-catalog.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn code(result: Result<Effective, ProfileError>) -> &'static str {
    result.expect_err("profile must be rejected").code
}

fn with_params(event: &str, params: Value) -> String {
    json!({"profile_version": "1.0", "taxonomy_version": "1.1",
           "events": {event: {"params": params}}})
    .to_string()
}

fn with_event(event: &str, body: Value) -> String {
    json!({"profile_version": "1.0", "taxonomy_version": "1.1",
           "events": {event: body}})
    .to_string()
}

#[test]
fn embedded_default_equals_detector_default() {
    assert_eq!(embedded().config(), Config::default());
    assert_eq!(embedded().source(), &Source::Embedded);
}

#[test]
fn embedded_default_equals_catalog_defaults_and_covers_every_supported_tunable() {
    let catalog = catalog();
    assert_eq!(
        catalog["taxonomy_version"], "1.1",
        "update default-profile.json with the catalog"
    );
    let described = embedded().describe_json();
    let mut seen = 0;
    for event in catalog["events"].as_array().unwrap() {
        let id = event["id"].as_str().unwrap();
        for (param, spec) in event["tunables"].as_object().unwrap() {
            let Some(row) = described["events"][id]["params"].get(param) else {
                continue;
            };
            seen += 1;
            assert_eq!(row["value"], spec["default"], "{id}.{param} default");
            assert_eq!(row["default"], spec["default"], "{id}.{param} default echo");
            assert_eq!(row["min"], spec["min"], "{id}.{param} min");
            assert_eq!(row["max"], spec["max"], "{id}.{param} max");
        }
    }
    assert_eq!(seen, 13, "all 13 detector tunables are described");
}

#[test]
fn override_changes_only_what_it_names() {
    let effective =
        merge_override(&with_params("health_low", json!({"threshold_percent": 25}))).unwrap();
    let config = effective.config();
    assert_eq!(config.health_low_threshold_percent, 25);
    assert_eq!(config.health_low_rearm_percent, 30);
    assert_eq!(config.danger_max_hp_percent, 25);
    assert_eq!(
        effective.overrides(),
        vec!["health_low.threshold_percent".to_string()]
    );
}

#[test]
fn rejects_out_of_bounds_unknown_and_malformed() {
    assert_eq!(
        code(merge_override(&with_params(
            "health_danger",
            json!({"window_ms": 499})
        ))),
        "PARAM_OUT_OF_RANGE"
    );
    assert_eq!(
        code(merge_override(&with_params(
            "health_danger",
            json!({"window_ms": 5001})
        ))),
        "PARAM_OUT_OF_RANGE"
    );
    assert_eq!(
        code(merge_override(&with_params(
            "health_danger",
            json!({"window_ms": 1.5})
        ))),
        "PARAM_TYPE"
    );
    assert_eq!(
        code(merge_override(&with_params(
            "health_danger",
            json!({"nope": 1})
        ))),
        "UNKNOWN_PARAM"
    );
    assert_eq!(
        code(merge_override(&with_params("made_up", json!({})))),
        "UNKNOWN_EVENT"
    );
    assert_eq!(
        code(merge_override(&with_params(
            "level_milestone",
            json!({"levels": [12, 6]})
        ))),
        "PARAM_TYPE"
    );
    assert_eq!(
        code(merge_override(&with_params(
            "level_milestone",
            json!({"levels": [1]})
        ))),
        "PARAM_OUT_OF_RANGE"
    );
    assert_eq!(code(merge_override("{")), "SYNTAX");
    assert_eq!(
        code(merge_override(
            r#"{"profile_version":"1.0","taxonomy_version":"1.1","events":{},"events":{}}"#
        )),
        "DUPLICATE_KEY"
    );
    assert_eq!(
        code(merge_override(
            r#"{"profile_version":"2.0","taxonomy_version":"1.1","events":{}}"#
        )),
        "PROFILE_VERSION"
    );
    assert_eq!(
        code(merge_override(
            r#"{"profile_version":"1.0","taxonomy_version":"2.0","events":{}}"#
        )),
        "TAXONOMY_VERSION"
    );
    assert_eq!(
        code(merge_override(
            r#"{"profile_version":"1.0","taxonomy_version":"1.9","events":{}}"#
        )),
        "TAXONOMY_VERSION"
    );
    assert_eq!(
        code(merge_override(
            r#"{"profile_version":"1.0","taxonomy_version":"1.1","events":{},"extra":1}"#
        )),
        "UNKNOWN_FIELD"
    );
}

#[test]
fn rearm_must_stay_above_trigger() {
    assert_eq!(
        code(merge_override(&with_params(
            "health_low",
            json!({"threshold_percent": 40, "rearm_percent": 30})
        ))),
        "REARM_NOT_ABOVE_TRIGGER"
    );
    assert_eq!(
        code(merge_override(&with_params(
            "mana_low",
            json!({"threshold_percent": 30})
        ))),
        "REARM_NOT_ABOVE_TRIGGER"
    );
    assert_eq!(
        code(merge_override(&with_params(
            "health_danger",
            json!({"max_hp_percent": 45})
        ))),
        "REARM_NOT_ABOVE_TRIGGER"
    );
}

#[test]
fn critical_events_cannot_be_disabled_or_given_unsupported_fields() {
    assert_eq!(
        code(merge_override(&with_event(
            "health_danger",
            json!({"enabled": false})
        ))),
        "CRITICAL_LOCKED"
    );
    assert_eq!(
        code(merge_override(&with_event(
            "health_danger",
            json!({"enabled": true})
        ))),
        "CRITICAL_LOCKED"
    );
    assert_eq!(
        code(merge_override(&with_event(
            "health_low",
            json!({"enabled": false})
        ))),
        "FIELD_NOT_SUPPORTED"
    );
    assert_eq!(
        code(merge_override(&with_event(
            "health_low",
            json!({"condition": {"all": []}})
        ))),
        "FIELD_NOT_SUPPORTED"
    );
}

#[test]
fn export_round_trips_and_ignores_its_own_header() {
    let effective = merge_override(&with_params(
        "health_danger",
        json!({"cooldown_ms": 30000, "drop_points": 20}),
    ))
    .unwrap();
    let exported = effective.export_json();
    let again = merge_override(&exported).unwrap();
    assert_eq!(again.config(), effective.config());
    assert_eq!(again.overrides(), effective.overrides());
    let value: Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(
        value["generated"]["embedded_default_sha256"],
        json!(embedded_sha256())
    );
    assert_eq!(value["generated"]["overrides"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["events"]["health_low"]["params"]["threshold_percent"],
        20
    );
}

#[test]
fn embedded_export_has_no_overrides() {
    let value: Value = serde_json::from_str(&embedded().export_json()).unwrap();
    assert_eq!(value["generated"]["overrides"], json!([]));
    assert_eq!(value["generated"]["source"], "embedded");
}

#[test]
fn levels_param_reaches_the_detector_config() {
    let effective =
        merge_override(&with_params("level_milestone", json!({"levels": [4, 8]}))).unwrap();
    assert_eq!(effective.config().levels, vec![4, 8]);
}
