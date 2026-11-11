//! runtime.get_status payload (contracts/runtime-status.schema.json 1.0.0).

use crate::containment::{Sample, WORKING_SET_LIMIT_BYTES};
use crate::intake::{IntakeState, Shared};
use kohbor_gsi::admission::{Ownership, Phase};
use kohbor_gsi::FRESH_MAX_AGE_MS;
use serde_json::{json, Value};

pub fn build(s: &Shared, sample: &Sample, throttled: bool, revision: u64) -> Value {
    let age = s.last_age_ms();
    let (state, reason) = match &s.state {
        IntakeState::NotConfigured(code) => ("not_configured", Some(*code)),
        IntakeState::Unavailable(code) => ("unavailable", Some(*code)),
        IntakeState::Listening => match age {
            None => ("listening", None),
            Some(a) if a <= FRESH_MAX_AGE_MS => ("receiving", None),
            Some(_) => ("stale", Some("SOURCE_STALE")),
        },
    };
    let listening = matches!(state, "listening" | "receiving" | "stale");
    let intake = json!({
        "state": state,
        "port": if listening { s.port.map(Value::from).unwrap_or(Value::Null) } else { Value::Null },
        "generation": if listening { json!(s.generation.max(1)) } else { Value::Null },
        "last_admitted_age_ms": if listening { age.map(Value::from).unwrap_or(Value::Null) } else { Value::Null },
        "reason": reason,
    });
    let ownership = match s.ownership {
        None => json!({"state": "unknown", "reason": null}),
        Some(Ownership::Verified) => json!({"state": "verified", "reason": null}),
        Some(Ownership::Unverified) => json!({"state": "unverified", "reason": s.ownership_reason}),
        Some(Ownership::Mismatch) => json!({"state": "mismatch", "reason": s.ownership_reason}),
    };
    let phase = match (s.phase, s.epoch) {
        (Some(p), Some(_)) => match p {
            Phase::Draft => "draft",
            Phase::Pregame => "pregame",
            Phase::Live => "live",
            Phase::PostGame => "post_game",
        },
        _ => "none",
    };
    let session = json!({
        "phase": phase,
        "session_epoch": if phase == "none" { Value::Null } else { json!(s.epoch) },
    });
    let cpu = match sample.cpu_percent {
        Some(c) => json!({"state": "available", "last_sample_percent": (c * 10.0).round() / 10.0,
                          "throttle": if throttled { "throttled" } else { "normal" }}),
        None => {
            json!({"state": "unavailable", "last_sample_percent": null, "throttle": "throttled"})
        }
    };
    let memory = match sample.working_set_bytes {
        Some(w) if w > WORKING_SET_LIMIT_BYTES => {
            json!({"state": "over_budget", "working_set_bytes": w})
        }
        Some(w) => json!({"state": "available", "working_set_bytes": w}),
        None => json!({"state": "unavailable", "working_set_bytes": null}),
    };
    json!({
        "schema_version": "1.0.0",
        "status_revision": revision.max(1),
        "intake": intake,
        "ownership": ownership,
        "session": session,
        "mode": {"state": "unverified", "value": null},
        "resource": {"containment": "established", "cpu": cpu, "memory": memory},
        "media": {"state": "not_ready", "reason": "MEDIA_UNMAPPED"},
        "capture": {"state": "unavailable", "reason": "CAPABILITY_UNAVAILABLE"},
        "account": {"state": "unavailable"},
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validator() -> jsonschema::Validator {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../contracts/runtime-status.schema.json"
        );
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        jsonschema::validator_for(&schema).unwrap()
    }

    #[test]
    fn fresh_install_status_matches_contract() {
        let dir = std::env::temp_dir().join(format!("kohbor-status-test-{}", std::process::id()));
        let s = Shared::new(dir);
        let sample = Sample {
            cpu_percent: Some(2.4),
            working_set_bytes: Some(200_000_000),
            process_count: Some(4),
        };
        let v = build(&s, &sample, false, 1);
        let errors: Vec<String> = validator().iter_errors(&v).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{errors:?}\n{v:#}");
        assert_eq!(v["intake"]["state"], "not_configured");
        assert_eq!(v["session"]["phase"], "none");
    }

    #[test]
    fn unavailable_metrics_status_matches_contract() {
        let dir = std::env::temp_dir().join(format!("kohbor-status-test2-{}", std::process::id()));
        let s = Shared::new(dir);
        let sample = Sample {
            cpu_percent: None,
            working_set_bytes: None,
            process_count: None,
        };
        let v = build(&s, &sample, true, 7);
        let errors: Vec<String> = validator().iter_errors(&v).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{errors:?}\n{v:#}");
        assert_eq!(v["resource"]["cpu"]["throttle"], "throttled");
    }
}
