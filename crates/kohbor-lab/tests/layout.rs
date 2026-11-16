use kohbor_lab::{embedded, merge_override, BannerLayout, Effective, HudLayout, ProfileError};
use serde_json::{json, Value};

fn layout_doc(banner: Value) -> String {
    json!({"profile_version": "1.0", "taxonomy_version": "1.1", "events": {},
           "layout": {"banner": banner}})
    .to_string()
}

fn profile_doc(layout: Value) -> String {
    json!({"profile_version": "1.0", "taxonomy_version": "1.1", "events": {},
           "layout": layout})
    .to_string()
}

fn default_hud() -> Value {
    json!({
        "gmeter": {"x_percent": 50.0, "y_percent": 5.5, "scale": 0.65, "enabled": true},
        "kda": {"x_percent": 9.64, "y_percent": 2.2, "scale": 0.78, "enabled": true},
        "gold": {"x_percent": 13.56, "y_percent": 2.2, "scale": 0.78, "enabled": true},
        "gpm": {"x_percent": 17.23, "y_percent": 2.2, "scale": 0.78, "enabled": true},
        "nw": {"x_percent": 21.02, "y_percent": 2.2, "scale": 0.78, "enabled": true},
        "xpm": {"x_percent": 24.82, "y_percent": 2.2, "scale": 0.78, "enabled": true},
    })
}

fn code(result: Result<Effective, ProfileError>) -> &'static str {
    result.expect_err("layout must be rejected").code
}

#[test]
fn embedded_banner_default_is_the_owners_saved_layout() {
    let banner = embedded().layout().banner;
    assert_eq!(
        banner,
        BannerLayout {
            x_percent: 49.46,
            y_percent: 10.3,
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
    assert_eq!(banner.y_percent, 10.3);
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
    assert_eq!(base.layout().banner.x_percent, 49.46, "original untouched");
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
        json!(49.46)
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

#[test]
fn embedded_hud_defaults_match_the_six_compiled_host_defaults() {
    let exported: Value = serde_json::from_str(&embedded().export_json()).unwrap();
    assert_eq!(exported["layout"]["hud"], default_hud());
}

#[test]
fn hud_override_can_name_only_one_module_and_one_field() {
    let effective = merge_override(&profile_doc(json!({
        "hud": {"gmeter": {"x_percent": 63.25}}
    })))
    .unwrap();
    let exported: Value = serde_json::from_str(&effective.export_json()).unwrap();
    assert_eq!(
        exported["layout"]["hud"]["gmeter"]["x_percent"],
        json!(63.25)
    );
    assert_eq!(exported["layout"]["hud"]["gmeter"]["y_percent"], json!(5.5));
    assert_eq!(exported["layout"]["hud"]["gmeter"]["enabled"], json!(true));
    assert_eq!(exported["layout"]["hud"]["kda"], default_hud()["kda"]);
}

#[test]
fn hud_override_merges_multiple_partial_modules_independently() {
    let effective = merge_override(&profile_doc(json!({
        "hud": {
            "kda": {"x_percent": 11.0, "enabled": false},
            "nw": {"scale": 1.2}
        }
    })))
    .unwrap();
    let exported: Value = serde_json::from_str(&effective.export_json()).unwrap();
    assert_eq!(exported["layout"]["hud"]["kda"]["x_percent"], json!(11.0));
    assert_eq!(exported["layout"]["hud"]["kda"]["enabled"], json!(false));
    assert_eq!(exported["layout"]["hud"]["kda"]["scale"], json!(0.78));
    assert_eq!(exported["layout"]["hud"]["nw"]["scale"], json!(1.2));
    assert_eq!(exported["layout"]["hud"]["nw"]["y_percent"], json!(2.2));
}

#[test]
fn legacy_profile_without_hud_keeps_embedded_hud_defaults() {
    let legacy = r#"{"profile_version":"1.0","taxonomy_version":"1.1","events":{"health_low":{"params":{"threshold_percent":25}}},"layout":{"banner":{"x_percent":42}}}"#;
    let effective = merge_override(legacy).unwrap();
    let exported: Value = serde_json::from_str(&effective.export_json()).unwrap();
    assert_eq!(exported["layout"]["hud"], default_hud());
    assert_eq!(exported["layout"]["banner"]["x_percent"], json!(42.0));
    assert_eq!(effective.config().health_low_threshold_percent, 25);
}

#[test]
fn hud_layout_accepts_every_inclusive_numeric_boundary() {
    let effective = merge_override(&profile_doc(json!({
        "hud": {"gmeter": {
            "x_percent": 0, "y_percent": 100, "scale": 3.0, "enabled": false
        }}
    })))
    .unwrap();
    let exported: Value = serde_json::from_str(&effective.export_json()).unwrap();
    assert_eq!(
        exported["layout"]["hud"]["gmeter"],
        json!({
            "x_percent": 0.0, "y_percent": 100.0, "scale": 3.0, "enabled": false
        })
    );
    assert!(merge_override(&profile_doc(json!({
        "hud": {"xpm": {"scale": 0.2, "x_percent": 100, "y_percent": 0}}
    })))
    .is_ok());
}

#[test]
fn hud_layout_rejects_each_out_of_range_numeric_field() {
    for (module, field, value) in [
        ("gmeter", "x_percent", json!(-0.1)),
        ("gmeter", "x_percent", json!(100.1)),
        ("kda", "y_percent", json!(-0.1)),
        ("kda", "y_percent", json!(100.1)),
        ("gold", "scale", json!(0.19)),
        ("gold", "scale", json!(3.01)),
    ] {
        let doc = profile_doc(json!({"hud": {module: {field: value}}}));
        assert_eq!(
            code(merge_override(&doc)),
            "LAYOUT_OUT_OF_RANGE",
            "{module}.{field}={value}"
        );
    }
}

#[test]
fn hud_layout_rejects_non_numeric_and_non_boolean_field_types() {
    for (field, value) in [
        ("x_percent", json!("50")),
        ("y_percent", json!(null)),
        ("scale", json!(true)),
        ("enabled", json!(1)),
        ("enabled", json!("true")),
    ] {
        let doc = profile_doc(json!({"hud": {"gpm": {field: value}}}));
        assert_eq!(code(merge_override(&doc)), "LAYOUT_TYPE", "gpm.{field}");
    }
}

#[test]
fn hud_layout_rejects_unknown_module_names() {
    let doc = profile_doc(json!({"hud": {"chip": {"x_percent": 50}}}));
    assert_eq!(code(merge_override(&doc)), "UNKNOWN_MODULE");
}

#[test]
fn hud_layout_rejects_unknown_module_fields() {
    let doc = profile_doc(json!({"hud": {"gmeter": {"opacity": 0.5}}}));
    assert_eq!(code(merge_override(&doc)), "UNKNOWN_FIELD");
}

#[test]
fn hud_layout_must_be_an_object_and_each_module_body_must_be_an_object() {
    assert_eq!(
        code(merge_override(&profile_doc(json!({"hud": 5})))),
        "LAYOUT_TYPE"
    );
    assert_eq!(
        code(merge_override(&profile_doc(json!({"hud": {"gmeter": []}})))),
        "LAYOUT_TYPE"
    );
}

#[test]
fn hud_layout_exports_and_describes_current_and_default_values() {
    let effective = merge_override(&profile_doc(json!({
        "hud": {"xpm": {"y_percent": 8.75}}
    })))
    .unwrap();
    let described = effective.describe_json();
    assert_eq!(described["layout"]["hud"]["xpm"]["y_percent"], json!(8.75));
    assert_eq!(
        described["layout"]["default"]["hud"]["xpm"]["y_percent"],
        json!(2.2)
    );
    assert_eq!(
        described["layout"]["hud"]["gmeter"],
        default_hud()["gmeter"]
    );
    let again = merge_override(&effective.export_json()).unwrap();
    assert_eq!(
        again.describe_json()["layout"]["hud"],
        described["layout"]["hud"]
    );
}

#[test]
fn hud_field_changes_are_named_in_the_override_summary() {
    let effective = merge_override(&profile_doc(json!({
        "hud": {"gold": {"x_percent": 50, "enabled": false}}
    })))
    .unwrap();
    assert!(effective
        .overrides()
        .contains(&"layout.hud.gold.x_percent".to_owned()));
    assert!(effective
        .overrides()
        .contains(&"layout.hud.gold.enabled".to_owned()));
}

#[test]
fn banner_and_hud_layouts_merge_without_changing_each_other() {
    let effective = merge_override(&profile_doc(json!({
        "banner": {"x_percent": 44.0},
        "hud": {"gmeter": {"x_percent": 61.0}}
    })))
    .unwrap();
    let exported: Value = serde_json::from_str(&effective.export_json()).unwrap();
    assert_eq!(exported["layout"]["banner"]["x_percent"], json!(44.0));
    assert_eq!(exported["layout"]["banner"]["y_percent"], json!(10.3));
    assert_eq!(
        exported["layout"]["hud"]["gmeter"]["x_percent"],
        json!(61.0)
    );
    assert_eq!(exported["layout"]["hud"]["kda"], default_hud()["kda"]);
}

#[test]
fn with_hud_layout_replaces_named_modules_and_preserves_unspecified_modules() {
    let base = embedded();
    let edited = base
        .with_hud_layout(&[(
            "gmeter",
            HudLayout {
                x_percent: 41.0,
                ..base.layout().hud[0]
            },
        )])
        .unwrap();
    let exported: Value = serde_json::from_str(&edited.export_json()).unwrap();
    assert_eq!(
        exported["layout"]["hud"]["gmeter"]["x_percent"],
        json!(41.0)
    );
    assert_eq!(exported["layout"]["hud"]["kda"], default_hud()["kda"]);
}

#[test]
fn with_hud_layout_rejects_unknown_duplicate_out_of_range_and_non_finite_values() {
    let base = embedded();
    let moved = HudLayout {
        x_percent: 40.0,
        ..base.layout().hud[0]
    };
    assert_eq!(
        base.with_hud_layout(&[("chip", moved)]).unwrap_err().code,
        "UNKNOWN_MODULE"
    );
    assert_eq!(
        base.with_hud_layout(&[("gmeter", moved), ("gmeter", moved)])
            .unwrap_err()
            .code,
        "DUPLICATE_KEY"
    );
    assert_eq!(
        base.with_hud_layout(&[(
            "gmeter",
            HudLayout {
                scale: 3.01,
                ..moved
            }
        )])
        .unwrap_err()
        .code,
        "LAYOUT_OUT_OF_RANGE"
    );
    assert_eq!(
        base.with_hud_layout(&[(
            "gmeter",
            HudLayout {
                x_percent: f64::NAN,
                ..moved
            }
        )])
        .unwrap_err()
        .code,
        "LAYOUT_OUT_OF_RANGE"
    );
}

#[test]
fn hud_layout_rejects_numbers_that_overflow_to_non_finite_values() {
    let doc = r#"{"profile_version":"1.0","taxonomy_version":"1.1","events":{},"layout":{"hud":{"gmeter":{"x_percent":1e9999}}}}"#;
    assert!(merge_override(doc).is_err());
}
