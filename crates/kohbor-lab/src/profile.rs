//! Profile schema, validation, merge and export (KOHBOR-LAB.md section 2).

use crate::Source;
use kohbor_events::Config;
use kohbor_gsi::strict_json::{self, StrictJsonError};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

const EMBEDDED: &str = include_str!("../default-profile.json");
const CATALOG: &str = include_str!("../../../contracts/event-catalog.json");
const PROFILE_VERSION: &str = "1.0";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Scalar,
    List,
}

/// The catalog tunables the detector `Config` can take today, in display order.
const SPECS: [(&str, &str, Kind); 13] = [
    ("multi_kill", "window_ms", Kind::Scalar),
    ("multi_kill", "min_kills", Kind::Scalar),
    ("streak", "min_kills", Kind::Scalar),
    ("level_milestone", "levels", Kind::List),
    ("health_low", "threshold_percent", Kind::Scalar),
    ("health_low", "rearm_percent", Kind::Scalar),
    ("mana_low", "threshold_percent", Kind::Scalar),
    ("mana_low", "rearm_percent", Kind::Scalar),
    ("health_danger", "max_hp_percent", Kind::Scalar),
    ("health_danger", "drop_points", Kind::Scalar),
    ("health_danger", "window_ms", Kind::Scalar),
    ("health_danger", "rearm_percent", Kind::Scalar),
    ("health_danger", "cooldown_ms", Kind::Scalar),
];

/// (event, trigger param, re-arm param): re-arm must sit above the trigger.
const REARM_PAIRS: [(&str, &str, &str); 3] = [
    ("health_low", "threshold_percent", "rearm_percent"),
    ("mana_low", "threshold_percent", "rearm_percent"),
    ("health_danger", "max_hp_percent", "rearm_percent"),
];

const MAX_LEVELS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileError {
    pub code: &'static str,
    pub detail: String,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

pub(crate) fn err(code: &'static str, detail: impl Into<String>) -> ProfileError {
    ProfileError {
        code,
        detail: detail.into(),
    }
}

type Values = BTreeMap<(&'static str, &'static str), Value>;

/// Banner overlay placement: percent of the primary monitor, centre of the art stage
/// (the host's `overlay::BannerLayout` has the same fields; a host test pins the default).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BannerLayout {
    pub x_percent: f64,
    pub y_percent: f64,
    pub scale: f64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub banner: BannerLayout,
}

pub const PERCENT_BOUNDS: (f64, f64) = (0.0, 100.0);
pub const SCALE_BOUNDS: (f64, f64) = (0.2, 3.0);
const LAYOUT_FIELDS: [&str; 4] = ["x_percent", "y_percent", "scale", "enabled"];

fn check_range(name: &str, value: &Value, bounds: (f64, f64)) -> Result<f64, ProfileError> {
    let number = value.as_f64().ok_or_else(|| {
        err(
            "LAYOUT_TYPE",
            format!("layout.banner.{name} must be a number"),
        )
    })?;
    if !number.is_finite() || number < bounds.0 || number > bounds.1 {
        return Err(err(
            "LAYOUT_OUT_OF_RANGE",
            format!(
                "layout.banner.{name} = {number} is outside {}..{}",
                bounds.0, bounds.1
            ),
        ));
    }
    Ok(number)
}

/// Validate the `layout` object of a profile and return the banner fields it names.
fn parse_layout(value: &Value) -> Result<Vec<(&'static str, Value)>, ProfileError> {
    let modules = value
        .as_object()
        .ok_or_else(|| err("LAYOUT_TYPE", "layout must be an object"))?;
    let mut named = Vec::new();
    for (module, body) in modules {
        if module != "banner" {
            return Err(err(
                "UNKNOWN_MODULE",
                format!("layout.{module} is not an editable module (banner only)"),
            ));
        }
        let body = body
            .as_object()
            .ok_or_else(|| err("LAYOUT_TYPE", "layout.banner must be an object"))?;
        for (name, field) in body {
            let Some(known) = LAYOUT_FIELDS.iter().find(|f| **f == name.as_str()) else {
                return Err(err(
                    "UNKNOWN_FIELD",
                    format!("layout.banner.{name} is not a field"),
                ));
            };
            match *known {
                "x_percent" | "y_percent" => {
                    check_range(known, field, PERCENT_BOUNDS)?;
                }
                "scale" => {
                    check_range(known, field, SCALE_BOUNDS)?;
                }
                _ => {
                    if !field.is_boolean() {
                        return Err(err(
                            "LAYOUT_TYPE",
                            "layout.banner.enabled must be a boolean",
                        ));
                    }
                }
            }
            named.push((*known, field.clone()));
        }
    }
    Ok(named)
}

fn apply_layout(mut base: BannerLayout, patches: &[(&'static str, Value)]) -> BannerLayout {
    for (name, value) in patches {
        match *name {
            "x_percent" => base.x_percent = value.as_f64().unwrap_or(base.x_percent),
            "y_percent" => base.y_percent = value.as_f64().unwrap_or(base.y_percent),
            "scale" => base.scale = value.as_f64().unwrap_or(base.scale),
            _ => base.enabled = value.as_bool().unwrap_or(base.enabled),
        }
    }
    base
}

fn layout_json(layout: &BannerLayout) -> Value {
    json!({
        "x_percent": layout.x_percent, "y_percent": layout.y_percent,
        "scale": layout.scale, "enabled": layout.enabled,
    })
}

struct Parsed {
    values: Values,
    layout: Vec<(&'static str, Value)>,
}

/// A validated profile: every supported tunable has a value.
#[derive(Debug, Clone)]
pub struct Effective {
    values: Values,
    defaults: Values,
    layout: Layout,
    default_layout: Layout,
    source: Source,
}

pub(crate) fn catalog() -> &'static Value {
    static PARSED: OnceLock<Value> = OnceLock::new();
    PARSED.get_or_init(|| serde_json::from_str(CATALOG).expect("catalog is valid JSON"))
}

fn catalog_spec(event: &str, name: &str) -> Value {
    catalog()["events"]
        .as_array()
        .and_then(|events| events.iter().find(|e| e["id"] == event))
        .map_or(Value::Null, |e| e["tunables"][name].clone())
}

fn catalog_taxonomy() -> (u64, u64) {
    parse_taxonomy(catalog()["taxonomy_version"].as_str().unwrap_or("0.0")).unwrap_or((0, 0))
}

fn is_critical(event: &str) -> bool {
    catalog()["events"]
        .as_array()
        .and_then(|events| events.iter().find(|e| e["id"] == event))
        .is_some_and(|e| e["category"] == "critical")
}

/// `MAJOR.MINOR`, two unsigned integers, no leading zeros (MP-05 grammar).
fn parse_taxonomy(text: &str) -> Option<(u64, u64)> {
    let (major, minor) = text.split_once('.')?;
    let ok = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|b| b.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    if !(ok(major) && ok(minor)) {
        return None;
    }
    Some((major.parse().ok()?, minor.parse().ok()?))
}

fn spec_of(event: &str, name: &str) -> Option<(&'static str, &'static str, Kind)> {
    SPECS
        .iter()
        .copied()
        .find(|(e, n, _)| *e == event && *n == name)
}

fn int_in_range(value: &Value, min: i64, max: i64, label: &str) -> Result<i64, ProfileError> {
    let number = value
        .as_i64()
        .ok_or_else(|| err("PARAM_TYPE", format!("{label} must be an integer")))?;
    if number < min || number > max {
        return Err(err(
            "PARAM_OUT_OF_RANGE",
            format!("{label} = {number} is outside {min}..{max}"),
        ));
    }
    Ok(number)
}

fn check_value(event: &str, name: &str, kind: Kind, value: &Value) -> Result<(), ProfileError> {
    let label = format!("{event}.{name}");
    let spec = catalog_spec(event, name);
    let (min, max) = (
        spec["min"].as_i64().unwrap_or(i64::MIN),
        spec["max"].as_i64().unwrap_or(i64::MAX),
    );
    match kind {
        Kind::Scalar => {
            int_in_range(value, min, max, &label)?;
        }
        Kind::List => {
            let items = value
                .as_array()
                .ok_or_else(|| err("PARAM_TYPE", format!("{label} must be a list of integers")))?;
            if items.is_empty() || items.len() > MAX_LEVELS {
                return Err(err(
                    "PARAM_TYPE",
                    format!("{label} needs 1..{MAX_LEVELS} entries"),
                ));
            }
            let mut previous = None;
            for item in items {
                let level = item
                    .as_i64()
                    .ok_or_else(|| err("PARAM_TYPE", format!("{label} must hold integers")))?;
                if previous.is_some_and(|p| level <= p) {
                    return Err(err(
                        "PARAM_TYPE",
                        format!("{label} must be strictly increasing"),
                    ));
                }
                previous = Some(level);
                int_in_range(item, min, max, &label)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn strict(bytes: &[u8]) -> Result<Map<String, Value>, ProfileError> {
    match strict_json::parse(bytes) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(err("SYNTAX", "the profile must be a JSON object")),
        Err(StrictJsonError::Syntax) => Err(err("SYNTAX", "not valid JSON")),
        Err(StrictJsonError::DuplicateKey) => Err(err("DUPLICATE_KEY", "an object repeats a key")),
        Err(StrictJsonError::TooDeep) => Err(err("TOO_DEEP", "nesting is too deep")),
    }
}

/// Validate a profile document and return the parameters it names.
fn parse_named(text: &str) -> Result<Parsed, ProfileError> {
    let root = strict(text.as_bytes())?;
    for key in root.keys() {
        if ![
            "profile_version",
            "taxonomy_version",
            "events",
            "generated",
            "layout",
        ]
        .contains(&key.as_str())
        {
            return Err(err(
                "UNKNOWN_FIELD",
                format!("unknown top-level field {key}"),
            ));
        }
    }
    if root.get("profile_version").and_then(Value::as_str) != Some(PROFILE_VERSION) {
        return Err(err(
            "PROFILE_VERSION",
            format!("profile_version must be \"{PROFILE_VERSION}\""),
        ));
    }
    let taxonomy = root
        .get("taxonomy_version")
        .and_then(Value::as_str)
        .and_then(parse_taxonomy)
        .ok_or_else(|| err("TAXONOMY_VERSION", "taxonomy_version must be MAJOR.MINOR"))?;
    let (major, minor) = catalog_taxonomy();
    if taxonomy.0 != major || taxonomy.1 > minor {
        return Err(err(
            "TAXONOMY_VERSION",
            format!(
                "profile taxonomy {}.{} does not fit runtime {major}.{minor}",
                taxonomy.0, taxonomy.1
            ),
        ));
    }
    let events = root
        .get("events")
        .and_then(Value::as_object)
        .ok_or_else(|| err("MISSING_FIELD", "events must be an object"))?;

    let mut named = Values::new();
    for (event, body) in events {
        let Some(&(event_name, _, _)) = SPECS.iter().find(|(e, _, _)| e == event) else {
            return Err(err(
                "UNKNOWN_EVENT",
                format!("{event} has no editable parameters"),
            ));
        };
        let body = body
            .as_object()
            .ok_or_else(|| err("PARAM_TYPE", format!("{event} must be an object")))?;
        for key in body.keys() {
            if key == "enabled" && is_critical(event_name) {
                return Err(err(
                    "CRITICAL_LOCKED",
                    format!("{event} is a safety event and cannot be switched"),
                ));
            }
            if key != "params" {
                return Err(err(
                    "FIELD_NOT_SUPPORTED",
                    format!("{event}.{key} is not supported in profile 1.0"),
                ));
            }
        }
        let Some(params) = body.get("params") else {
            continue;
        };
        let params = params
            .as_object()
            .ok_or_else(|| err("PARAM_TYPE", format!("{event}.params must be an object")))?;
        for (name, value) in params {
            let Some((e, n, kind)) = spec_of(event, name) else {
                return Err(err(
                    "UNKNOWN_PARAM",
                    format!("{event}.{name} is not a tunable"),
                ));
            };
            check_value(e, n, kind, value)?;
            named.insert((e, n), value.clone());
        }
    }
    let layout = match root.get("layout") {
        Some(value) => parse_layout(value)?,
        None => Vec::new(),
    };
    Ok(Parsed {
        values: named,
        layout,
    })
}

fn check_pairs(values: &Values) -> Result<(), ProfileError> {
    for (event, trigger, rearm) in REARM_PAIRS {
        let (Some(t), Some(r)) = (
            values.get(&(event, trigger)).and_then(Value::as_i64),
            values.get(&(event, rearm)).and_then(Value::as_i64),
        ) else {
            continue;
        };
        if r <= t {
            return Err(err(
                "REARM_NOT_ABOVE_TRIGGER",
                format!("{event}.{rearm} ({r}) must be above {event}.{trigger} ({t})"),
            ));
        }
    }
    Ok(())
}

pub fn embedded_sha256() -> String {
    Sha256::digest(EMBEDDED.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn embedded() -> Effective {
    let parsed = parse_named(EMBEDDED).expect("embedded default profile is valid");
    let values = parsed.values;
    assert_eq!(
        parsed.layout.len(),
        LAYOUT_FIELDS.len(),
        "embedded layout is complete"
    );
    let layout = Layout {
        banner: apply_layout(
            BannerLayout {
                x_percent: 0.0,
                y_percent: 0.0,
                scale: 0.0,
                enabled: false,
            },
            &parsed.layout,
        ),
    };
    assert_eq!(
        values.len(),
        SPECS.len(),
        "embedded profile covers every tunable"
    );
    Effective {
        defaults: values.clone(),
        values,
        layout,
        default_layout: layout,
        source: Source::Embedded,
    }
}

/// Embedded default overlaid with the parameters a profile document names.
pub fn merge_override(text: &str) -> Result<Effective, ProfileError> {
    let mut effective = embedded();
    let parsed = parse_named(text)?;
    effective.values.extend(parsed.values);
    effective.layout.banner = apply_layout(effective.layout.banner, &parsed.layout);
    check_pairs(&effective.values)?;
    Ok(effective)
}

impl Effective {
    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn with_source(mut self, source: Source) -> Self {
        self.source = source;
        self
    }

    fn number(&self, event: &'static str, name: &'static str) -> u64 {
        self.values[&(event, name)].as_u64().unwrap_or(0)
    }

    /// The detector parameters this profile means.
    pub fn config(&self) -> Config {
        let n = |event, name| self.number(event, name);
        let levels = self.values[&("level_milestone", "levels")]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_u64)
                    .map(|v| v as u32)
                    .collect()
            })
            .unwrap_or_default();
        Config {
            multi_kill_window_ms: n("multi_kill", "window_ms"),
            multi_kill_min_kills: n("multi_kill", "min_kills") as u32,
            streak_min_kills: n("streak", "min_kills") as u32,
            levels,
            health_low_threshold_percent: n("health_low", "threshold_percent") as u32,
            health_low_rearm_percent: n("health_low", "rearm_percent") as u32,
            mana_low_threshold_percent: n("mana_low", "threshold_percent") as u32,
            mana_low_rearm_percent: n("mana_low", "rearm_percent") as u32,
            danger_max_hp_percent: n("health_danger", "max_hp_percent") as u32,
            danger_drop_points: n("health_danger", "drop_points") as u32,
            danger_window_ms: n("health_danger", "window_ms"),
            danger_rearm_percent: n("health_danger", "rearm_percent") as u32,
            danger_cooldown_ms: n("health_danger", "cooldown_ms"),
        }
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// A copy with the banner layout replaced; validated like a profile file.
    pub fn with_layout(&self, banner: BannerLayout) -> Result<Effective, ProfileError> {
        let finite = [banner.x_percent, banner.y_percent, banner.scale]
            .iter()
            .all(|n| n.is_finite());
        if !finite {
            return Err(err("LAYOUT_OUT_OF_RANGE", "layout numbers must be finite"));
        }
        let mut doc: Value = serde_json::from_str(&self.export_json()).expect("own export parses");
        doc["layout"]["banner"] = layout_json(&banner);
        let mut edited = merge_override(&doc.to_string())?;
        edited.source = self.source.clone();
        Ok(edited)
    }

    /// A copy with the named parameters replaced. The whole result is validated again, so
    /// pairs such as trigger and re-arm can be moved together; the original is unchanged.
    pub fn with_edits(&self, edits: &[(&str, &str, Value)]) -> Result<Effective, ProfileError> {
        let mut doc: Value = serde_json::from_str(&self.export_json()).expect("own export parses");
        for (event, name, value) in edits {
            if doc["events"].get(*event).is_none() {
                doc["events"][*event] = json!({"params": {}});
            }
            doc["events"][*event]["params"][*name] = value.clone();
        }
        let mut edited = merge_override(&doc.to_string())?;
        edited.source = self.source.clone();
        Ok(edited)
    }

    /// `event.param` of every value that differs from the embedded default.
    pub fn overrides(&self) -> Vec<String> {
        let mut changed: Vec<String> = SPECS
            .iter()
            .filter(|(e, n, _)| self.values[&(*e, *n)] != self.defaults[&(*e, *n)])
            .map(|(e, n, _)| format!("{e}.{n}"))
            .collect();
        let (now, base) = (&self.layout.banner, &self.default_layout.banner);
        for (name, differs) in [
            ("x_percent", now.x_percent != base.x_percent),
            ("y_percent", now.y_percent != base.y_percent),
            ("scale", now.scale != base.scale),
            ("enabled", now.enabled != base.enabled),
        ] {
            if differs {
                changed.push(format!("layout.banner.{name}"));
            }
        }
        changed
    }

    fn source_label(&self) -> &'static str {
        match self.source {
            Source::Embedded => "embedded",
            Source::Override(_) => "override",
        }
    }

    /// Full snapshot in profile format, with an ignored `generated` header.
    pub fn export_json(&self) -> String {
        let mut events = Map::new();
        for (event, name, _) in SPECS {
            let entry = events
                .entry(event.to_owned())
                .or_insert_with(|| json!({"params": {}}));
            entry["params"][name] = self.values[&(event, name)].clone();
        }
        let doc = json!({
            "profile_version": PROFILE_VERSION,
            "taxonomy_version": catalog()["taxonomy_version"],
            "generated": {
                "embedded_default_sha256": embedded_sha256(),
                "source": self.source_label(),
                "overrides": self.overrides(),
            },
            "layout": {"banner": layout_json(&self.layout.banner)},
            "events": events,
        });
        serde_json::to_string_pretty(&doc).expect("serializable") + "\n"
    }

    /// Per-parameter view for the Lab UI: value, default, bounds, unit, changed.
    pub fn describe_json(&self) -> Value {
        let mut events = Map::new();
        for (event, name, _) in SPECS {
            let spec = catalog_spec(event, name);
            let entry = events
                .entry(event.to_owned())
                .or_insert_with(|| json!({"critical": is_critical(event), "params": {}}));
            let value = &self.values[&(event, name)];
            entry["params"][name] = json!({
                "value": value,
                "default": self.defaults[&(event, name)],
                "min": spec["min"],
                "max": spec["max"],
                "unit": spec["unit"],
                "changed": *value != self.defaults[&(event, name)],
            });
        }
        json!({
            "profile_version": PROFILE_VERSION,
            "taxonomy_version": catalog()["taxonomy_version"],
            "source": self.source_label(),
            "embedded_default_sha256": embedded_sha256(),
            "overrides": self.overrides(),
            "layout": {
                "banner": layout_json(&self.layout.banner),
                "default": {"banner": layout_json(&self.default_layout.banner)},
                "bounds": {
                    "x_percent": [PERCENT_BOUNDS.0, PERCENT_BOUNDS.1],
                    "y_percent": [PERCENT_BOUNDS.0, PERCENT_BOUNDS.1],
                    "scale": [SCALE_BOUNDS.0, SCALE_BOUNDS.1],
                },
            },
            "events": events,
        })
    }
}

/// `(event, param)` pairs the profile can edit (the 13 detector tunables).
pub(crate) fn editable_tunables() -> Vec<(&'static str, &'static str)> {
    SPECS
        .iter()
        .map(|(event, name, _)| (*event, *name))
        .collect()
}
