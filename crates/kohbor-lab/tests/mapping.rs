use kohbor_lab::mapping::{parse, Binding, Mapping};
use serde_json::{json, Value};

fn doc(bindings: Value) -> String {
    json!({"schema_version": "1.0.0", "bindings": bindings}).to_string()
}

fn code(text: &str) -> &'static str {
    parse(text).expect_err("mapping must be rejected").code
}

#[test]
fn the_rebuild_format_round_trips() {
    let text = doc(json!([
        {"event_id": "hero_kill", "audio": "06_eliminate-enemy.wav"},
        {"event_id": "streak", "tier": 3, "audio": "streak-t3.wav"},
        {"event_id": "multi_kill", "tier": 2, "audio": "double.wav"}
    ]));
    let mapping = parse(&text).unwrap();
    assert_eq!(
        mapping.bindings[1],
        Binding {
            event_id: "streak".to_owned(),
            tier: Some(3),
            audio: "streak-t3.wav".to_owned()
        }
    );
    let exported = mapping.to_json();
    let value: Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(value["schema_version"], "1.0.0");
    assert_eq!(
        value["bindings"][0],
        json!({"event_id": "hero_kill", "audio": "06_eliminate-enemy.wav"})
    );
    assert_eq!(
        value["bindings"][1],
        json!({"event_id": "streak", "tier": 3, "audio": "streak-t3.wav"})
    );
    assert_eq!(
        parse(&exported).unwrap(),
        mapping,
        "order and content survive a round trip"
    );
}

#[test]
fn an_empty_mapping_is_valid() {
    assert_eq!(parse(&doc(json!([]))).unwrap(), Mapping::default());
}

#[test]
fn events_must_exist_and_be_mappable() {
    assert_eq!(
        code(&doc(json!([{"event_id": "made_up", "audio": "a.wav"}]))),
        "UNKNOWN_EVENT"
    );
    assert_eq!(
        code(&doc(
            json!([{"event_id": "hero_respawn", "audio": "a.wav"}])
        )),
        "NOT_MAPPABLE"
    );
    assert_eq!(
        code(&doc(
            json!([{"event_id": "latency_probe", "audio": "a.wav"}])
        )),
        "NOT_MAPPABLE"
    );
}

#[test]
fn tiers_follow_the_catalog_tier_range() {
    assert_eq!(
        code(&doc(json!([{"event_id": "multi_kill", "audio": "a.wav"}]))),
        "TIER_REQUIRED"
    );
    assert_eq!(
        code(&doc(
            json!([{"event_id": "multi_kill", "tier": 1, "audio": "a.wav"}])
        )),
        "TIER_OUT_OF_RANGE"
    );
    assert_eq!(
        code(&doc(
            json!([{"event_id": "multi_kill", "tier": 6, "audio": "a.wav"}])
        )),
        "TIER_OUT_OF_RANGE"
    );
    assert_eq!(
        code(&doc(
            json!([{"event_id": "streak", "tier": 2, "audio": "a.wav"}])
        )),
        "TIER_OUT_OF_RANGE"
    );
    assert!(parse(&doc(
        json!([{"event_id": "streak", "tier": 10, "audio": "a.wav"}])
    ))
    .is_ok());
    assert_eq!(
        code(&doc(
            json!([{"event_id": "hero_kill", "tier": 2, "audio": "a.wav"}])
        )),
        "TIER_NOT_ALLOWED"
    );
    assert_eq!(
        code(&doc(
            json!([{"event_id": "streak", "tier": "3", "audio": "a.wav"}])
        )),
        "TIER_NOT_ALLOWED"
    );
}

#[test]
fn audio_is_a_plain_wav_file_name_never_a_path() {
    for bad in [
        "../x.wav",
        "a/b.wav",
        "a\\b.wav",
        "C:x.wav",
        "x.mp3",
        ".wav",
        "",
        "a b.wav",
        "x.WAV.exe",
    ] {
        assert_eq!(
            code(&doc(json!([{"event_id": "hero_kill", "audio": bad}]))),
            "BAD_AUDIO_NAME",
            "{bad:?}"
        );
    }
    assert_eq!(
        code(&doc(json!([{"event_id": "hero_kill", "audio": 5}]))),
        "BAD_AUDIO_NAME"
    );
    assert!(parse(&doc(
        json!([{"event_id": "hero_kill", "audio": "06_eliminate-enemy.wav"}])
    ))
    .is_ok());
}

#[test]
fn structure_is_strict() {
    assert_eq!(code("{"), "SYNTAX");
    assert_eq!(
        code(r#"{"schema_version":"1.0.0","bindings":[],"bindings":[]}"#),
        "DUPLICATE_KEY"
    );
    assert_eq!(
        code(&json!({"schema_version": "2.0.0", "bindings": []}).to_string()),
        "SCHEMA_VERSION"
    );
    assert_eq!(code(&json!({"bindings": []}).to_string()), "SCHEMA_VERSION");
    assert_eq!(
        code(&json!({"schema_version": "1.0.0", "bindings": [], "x": 1}).to_string()),
        "UNKNOWN_FIELD"
    );
    assert_eq!(
        code(&doc(
            json!([{"event_id": "hero_kill", "audio": "a.wav", "video": "v.webm"}])
        )),
        "UNKNOWN_FIELD"
    );
    assert_eq!(
        code(&doc(json!({"event_id": "hero_kill"}))),
        "BINDINGS_TYPE"
    );
    assert_eq!(
        code(&doc(json!([
            {"event_id": "hero_kill", "audio": "a.wav"},
            {"event_id": "hero_kill", "audio": "b.wav"}
        ]))),
        "DUPLICATE_BINDING"
    );
    let many: Vec<Value> = (0..129)
        .map(|_| json!({"event_id": "hero_kill", "audio": "a.wav"}))
        .collect();
    assert_eq!(code(&doc(Value::Array(many))), "TOO_MANY_BINDINGS");
}

#[test]
fn the_same_event_may_have_different_tiers() {
    let text = doc(json!([
        {"event_id": "multi_kill", "tier": 2, "audio": "a.wav"},
        {"event_id": "multi_kill", "tier": 3, "audio": "b.wav"}
    ]));
    assert_eq!(parse(&text).unwrap().bindings.len(), 2);
}
