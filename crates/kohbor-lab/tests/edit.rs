use kohbor_lab::embedded;
use serde_json::json;

#[test]
fn batch_edit_validates_as_a_whole() {
    let base = embedded();
    // Raising the trigger above the default re-arm alone is invalid...
    assert_eq!(
        base.with_edits(&[("health_low", "threshold_percent", json!(40))])
            .unwrap_err()
            .code,
        "REARM_NOT_ABOVE_TRIGGER"
    );
    // ...but moving both together is a valid profile.
    let edited = base
        .with_edits(&[
            ("health_low", "threshold_percent", json!(40)),
            ("health_low", "rearm_percent", json!(55)),
        ])
        .unwrap();
    assert_eq!(edited.config().health_low_threshold_percent, 40);
    assert_eq!(
        edited.overrides(),
        vec![
            "health_low.threshold_percent".to_string(),
            "health_low.rearm_percent".to_string()
        ]
    );
    // The original is untouched.
    assert!(base.overrides().is_empty());
}

#[test]
fn edits_are_bounded_and_cannot_invent_parameters() {
    let base = embedded();
    assert_eq!(
        base.with_edits(&[("health_danger", "window_ms", json!(499))])
            .unwrap_err()
            .code,
        "PARAM_OUT_OF_RANGE"
    );
    assert_eq!(
        base.with_edits(&[("health_danger", "enabled", json!(false))])
            .unwrap_err()
            .code,
        "UNKNOWN_PARAM"
    );
    assert_eq!(
        base.with_edits(&[("risk_alert", "global_cooldown_ms", json!(30000))])
            .unwrap_err()
            .code,
        "UNKNOWN_EVENT"
    );
}

#[test]
fn reset_returns_a_value_to_the_embedded_default() {
    let edited = embedded()
        .with_edits(&[("health_danger", "cooldown_ms", json!(60000))])
        .unwrap();
    assert_eq!(edited.overrides().len(), 1);
    let back = edited
        .with_edits(&[("health_danger", "cooldown_ms", json!(20000))])
        .unwrap();
    assert!(back.overrides().is_empty());
}

#[cfg(any(feature = "lab", debug_assertions))]
#[test]
fn save_writes_an_absolute_path_atomically_and_reloads() {
    use kohbor_lab::dev_override::{resolve_from, save};
    use std::path::Path;

    let dir = std::env::temp_dir().join(format!("kohbor-lab-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("profile.json");
    let edited = embedded()
        .with_edits(&[("health_danger", "cooldown_ms", json!(60000))])
        .unwrap();
    assert_eq!(
        save(Path::new("relative.json"), &edited)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    save(&path, &edited).unwrap();
    assert!(!dir.join("profile.json.tmp").exists());

    let path_text = path.to_string_lossy().into_owned();
    let resolved = resolve_from(
        &|name| match name {
            "KOHBOR_LAB" => Some("1".to_owned()),
            "KOHBOR_LAB_PROFILE" => Some(path_text.clone()),
            _ => None,
        },
        &|p| std::fs::read_to_string(p),
    );
    assert!(resolved.rejected.is_none());
    assert_eq!(resolved.effective.config().danger_cooldown_ms, 60000);
    let _ = std::fs::remove_dir_all(&dir);
}
