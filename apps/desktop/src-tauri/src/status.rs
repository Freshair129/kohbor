//! runtime.get_status payload (contracts/runtime-status.schema.json 1.1.0).

use crate::containment::Sample;
use crate::intake::{IntakeState, Shared, EVENT_UNAVAILABLE};
use kohbor_gsi::admission::{Ownership, Phase};
use kohbor_gsi::FRESH_MAX_AGE_MS;
use serde_json::{json, Value};
use std::time::Instant;

pub fn build(
    s: &Shared,
    sample: &Sample,
    throttled: bool,
    revision: u64,
    media: (&str, Option<&str>),
) -> Value {
    let now = Instant::now();
    let age = s.last_age_ms_at(now);
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
        // No memory limit exists (DEC-RESOURCE-BUDGET): the value is reported, never judged. `over_budget` stays reserved in the contract.
        Some(w) => json!({"state": "available", "working_set_bytes": w}),
        None => json!({"state": "unavailable", "working_set_bytes": null}),
    };
    let unavailable: Vec<Value> = EVENT_UNAVAILABLE
        .iter()
        .map(|id| json!({"id": id.as_str(), "reason": "CAPABILITY_UNAVAILABLE"}))
        .collect();
    json!({
        "schema_version": "1.1.0",
        "status_revision": revision.max(1),
        "intake": intake,
        "ownership": ownership,
        "session": session,
        "mode": {"state": "unverified", "value": null},
        "resource": {"containment": "established", "cpu": cpu, "memory": memory},
        "media": {"state": media.0, "reason": media.1},
        "capture": {"state": "unavailable", "reason": "CAPABILITY_UNAVAILABLE"},
        "account": {"state": "unavailable"},
        "events": {
            "detector": if s.event_detector_running(age) { "running" } else { "idle" },
            "last_event_age_ms": s.last_event_age_ms_at(now),
            "unavailable": unavailable
        },
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
        let v = build(&s, &sample, false, 1, ("not_ready", Some("MEDIA_UNMAPPED")));
        let errors: Vec<String> = validator().iter_errors(&v).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{errors:?}\n{v:#}");
        assert_eq!(v["intake"]["state"], "not_configured");
        assert_eq!(v["session"]["phase"], "none");
    }

    #[test]
    fn failed_setup_status_matches_contract() {
        let dir = std::env::temp_dir().join(format!("kohbor-status-test3-{}", std::process::id()));
        let mut s = Shared::new(dir);
        s.state = IntakeState::Unavailable("GSI_CONFIG_CONFLICT");
        let sample = Sample {
            cpu_percent: Some(0.4),
            working_set_bytes: Some(200_000_000),
            process_count: Some(4),
        };
        let v = build(&s, &sample, false, 3, ("not_ready", Some("MEDIA_UNMAPPED")));
        let errors: Vec<String> = validator().iter_errors(&v).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{errors:?}\n{v:#}");
        assert_eq!(v["intake"]["state"], "unavailable");
        assert_eq!(v["intake"]["reason"], "GSI_CONFIG_CONFLICT");
        assert!(v["intake"]["port"].is_null());
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
        let v = build(&s, &sample, true, 7, ("not_ready", Some("MEDIA_UNMAPPED")));
        let errors: Vec<String> = validator().iter_errors(&v).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{errors:?}\n{v:#}");
        assert_eq!(v["resource"]["cpu"]["throttle"], "throttled");
    }

    #[test]
    fn media_readiness_rows_match_the_1_1_0_contract() {
        use crate::audio::lanes::EngineShared;
        use crate::media::inventory::{media_status, MediaFacts};
        use std::sync::atomic::Ordering;
        use std::sync::Arc;

        let sample = Sample {
            cpu_percent: Some(0.0),
            working_set_bytes: Some(200_000_000),
            process_count: Some(1),
        };
        let intake = Shared::new(
            std::env::current_exe()
                .unwrap()
                .with_extension("status-data"),
        );
        let cases = [
            (
                "muted",
                true,
                true,
                true,
                false,
                0,
                ("unavailable", Some("AUDIO_MUTED")),
            ),
            (
                "no device",
                true,
                true,
                false,
                false,
                0,
                ("unavailable", Some("CAPABILITY_UNAVAILABLE")),
            ),
            (
                "unmapped",
                true,
                true,
                true,
                false,
                0,
                ("not_ready", Some("MEDIA_UNMAPPED")),
            ),
            (
                "store failure",
                false,
                true,
                true,
                false,
                0,
                ("not_ready", Some("MEDIA_INVALID")),
            ),
            (
                "stale heartbeat",
                true,
                false,
                true,
                false,
                0,
                ("unavailable", Some("CAPABILITY_UNAVAILABLE")),
            ),
            (
                "degraded",
                true,
                true,
                true,
                true,
                0,
                ("unavailable", Some("CAPABILITY_UNAVAILABLE")),
            ),
            (
                "critical overflow",
                true,
                true,
                true,
                false,
                1,
                ("unavailable", Some("CAPABILITY_UNAVAILABLE")),
            ),
        ];

        for (index, (label, store_ok, heartbeat_fresh, device, degraded, overflow, expected)) in
            cases.into_iter().enumerate()
        {
            let engine = Arc::new(EngineShared::new());
            engine
                .heartbeat_ms
                .store(if heartbeat_fresh { 3_000 } else { 0 }, Ordering::Relaxed);
            engine.device_available.store(device, Ordering::Relaxed);
            engine.muted.store(label == "muted", Ordering::Relaxed);
            engine.degraded.store(degraded, Ordering::Relaxed);
            engine.critical_overflow.store(overflow, Ordering::Relaxed);
            let facts = MediaFacts {
                store_ok,
                critical: Vec::new(),
            };
            let now_ms = 3_000;
            let value = build(
                &intake,
                &sample,
                false,
                index as u64 + 1,
                media_status(&facts, &engine, now_ms),
            );
            let errors: Vec<String> = validator()
                .iter_errors(&value)
                .map(|e| e.to_string())
                .collect();
            assert!(errors.is_empty(), "{label}: {errors:?}\n{value:#}");
            assert_eq!(value["media"]["state"], expected.0, "{label}");
            match expected.1 {
                Some(reason) => assert_eq!(value["media"]["reason"], reason, "{label}"),
                None => assert!(value["media"]["reason"].is_null(), "{label}"),
            }
        }
    }
}
