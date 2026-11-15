use kohbor_lab::{embedded, merge_override, BannerLayout, Effective, ProfileError};
use serde_json::{json, Value};

fn layout_doc(banner: Value) -> String {
    json!({"profile_version": "1.0", "taxonomy_version": "1.1", "events": {},
           "layout": {"banner": banner}})
    .to_string()
}

fn code(result: Result<Effective, ProfileError>) -> &'static str {
    result.expect_err("layout must be rejected").code
}

#[test]
fn embedded_banner_default_is_the_owners_profile_one() {
    let banner = embedded().layout().banner;
    assert_eq!(
        banner,
        BannerLayout {
            x_percent: 50.0,
            y_percent: 10.5,
            scale: 0.45,
            enabled: true
        }
    );
}

#[test]
fn partial_banner_override_changes_only_named_fields() {
    let effective = merge_override(&layout_doc(json!({"x_percent": 62.5, "scale": 0.8}))).unwrap();
    let banner = effective.layout().banner;
    assert_eq!(banner.x_percent, 62.5);
    assert_eq!(banner.scale, 0.8);
    assert_eq!(banner.y_percent, 10.5);
    assert!(banner.enabled);
    assert_eq!(
        effective.overrides(),
        vec![
            "layout.banner.x_percent".to_string(),
            "layout.banner.scale".to_string()
        ]
    );
}

#[test]
fn layout_bounds_types_and_unknown_names_are_rejected() {
    assert_eq!(
        code(merge_override(&layout_doc(json!({"x_percent": 100.5})))),
        "LAYOUT_OUT_OF_RANGE"
    );
    assert_eq!(
        code(merge_override(&layout_doc(json!({"y_percent": -0.1})))),
        "LAYOUT_OUT_OF_RANGE"
    );
    assert_eq!(
        code(merge_override(&layout_doc(json!({"scale": 0.19})))),
        "LAYOUT_OUT_OF_RANGE"
    );
    assert_eq!(
        code(merge_override(&layout_doc(json!({"scale": 3.01})))),
        "LAYOUT_OUT_OF_RANGE"
    );
    assert_eq!(
        code(merge_override(&layout_doc(json!({"x_percent": "50"})))),
        "LAYOUT_TYPE"
    );
    assert_eq!(
        code(merge_override(&layout_doc(json!({"enabled": 1})))),
        "LAYOUT_TYPE"
    );
    assert_eq!(
        code(merge_override(&layout_doc(json!({"rotation": 3})))),
        "UNKNOWN_FIELD"
    );
    let other_module = json!({"profile_version": "1.0", "taxonomy_version": "1.1", "events": {},
                              "layout": {"chip": {"x_percent": 1}}})
    .to_string();
    assert_eq!(code(merge_override(&other_module)), "UNKNOWN_MODULE");
    let not_object = json!({"profile_version": "1.0", "taxonomy_version": "1.1", "events": {},
                            "layout": 5})
    .to_string();
    assert_eq!(code(merge_override(&not_object)), "LAYOUT_TYPE");
}

#[test]
fn boundary_values_are_accepted() {
    let edge = merge_override(&layout_doc(json!({
        "x_percent": 0, "y_percent": 100, "scale": 0.2, "enabled": false
    })))
    .unwrap();
    assert_eq!(edge.layout().banner.y_percent, 100.0);
    assert!(!edge.layout().banner.enabled);
    assert!(merge_override(&layout_doc(json!({"scale": 3.0}))).is_ok());
}

#[test]
fn with_layout_validates_and_round_trips_through_export() {
    let base = embedded();
    let moved = BannerLayout {
        x_percent: 30.0,
        y_percent: 20.0,
        scale: 1.25,
        enabled: true,
    };
    let edited = base.with_layout(moved).unwrap();
    assert_eq!(edited.layout().banner, moved);
    assert_eq!(base.layout().banner.x_percent, 50.0, "original untouched");
    assert!(base
        .with_layout(BannerLayout {
            scale: 9.0,
            ..moved
        })
        .is_err());
    assert!(base
        .with_layout(BannerLayout {
            x_percent: f64::NAN,
            ..moved
        })
        .is_err());

    let again = merge_override(&edited.export_json()).unwrap();
    assert_eq!(again.layout().banner, moved);
    assert_eq!(again.overrides(), edited.overrides());
    let exported: Value = serde_json::from_str(&edited.export_json()).unwrap();
    assert_eq!(exported["layout"]["banner"]["scale"], json!(1.25));
}

#[test]
fn describe_reports_layout_with_defaults_and_bounds() {
    let described = embedded()
        .with_layout(BannerLayout {
            x_percent: 40.0,
            ..embedded().layout().banner
        })
        .unwrap()
        .describe_json();
    assert_eq!(described["layout"]["banner"]["x_percent"], json!(40.0));
    assert_eq!(
        described["layout"]["default"]["banner"]["x_percent"],
        json!(50.0)
    );
    assert_eq!(described["layout"]["bounds"]["scale"], json!([0.2, 3.0]));
}

#[test]
fn event_edits_keep_the_layout() {
    let moved = embedded()
        .with_layout(BannerLayout {
            x_percent: 70.0,
            ..embedded().layout().banner
        })
        .unwrap();
    let both = moved
        .with_edits(&[("health_danger", "cooldown_ms", json!(60000))])
        .unwrap();
    assert_eq!(both.layout().banner.x_percent, 70.0);
    assert_eq!(both.config().danger_cooldown_ms, 60000);
}
