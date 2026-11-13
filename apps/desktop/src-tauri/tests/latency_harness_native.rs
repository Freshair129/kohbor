use kohbor_audio::PlaybackId;
use kohbor_audio_native::{StartError, WasapiSink};
use kohbor_desktop_lib::latency::{
    artifact_sha256, run_stage_b, summarize, write_stage_b_evidence, write_stage_b_not_run,
};
use serde_json::Value;

const SAMPLE_COUNT: usize = 300;

// The full burst replay takes about ten minutes. Run explicitly with --ignored.
// Set KOHBOR_STAGE_B_SAMPLES to a positive value below 300 for a no-receipt smoke run.
#[test]
#[ignore = "native digital-silence fixture; about 10 minutes; run with --ignored"]
fn stage_b_writes_valid_native_silence_receipt() {
    let smoke_count = std::env::var("KOHBOR_STAGE_B_SAMPLES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|count| *count > 0 && *count < SAMPLE_COUNT);
    let count = smoke_count.unwrap_or(SAMPLE_COUNT);

    let sink = match WasapiSink::start() {
        Ok(sink) => sink,
        Err(StartError::NoRenderEndpoint) => {
            let reason = "no active render endpoint";
            let receipt_path = write_stage_b_not_run(reason).expect("write NOT_RUN receipt");
            let receipt: Value =
                serde_json::from_slice(&std::fs::read(receipt_path).expect("read receipt"))
                    .expect("parse receipt");
            validate_receipt(&receipt);
            assert_device_identity_is_null(&receipt);
            println!("{reason}");
            return;
        }
        Err(error) => panic!("WASAPI start failed: {error}"),
    };

    let probe = sink.first_fill_probe();
    let reader = move |playback_id: PlaybackId| {
        probe
            .latest()
            .and_then(|(observed_id, instant)| (observed_id == playback_id).then_some(instant))
    };
    let report = run_stage_b(count, Box::new(sink), &reader).expect("stage B run");
    for sample in &report.samples {
        let timestamps = [
            0,
            sample.sample.t1_us,
            sample.sample.t2_us,
            sample.sample.t3p_us,
        ];
        assert!(timestamps.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(sample.t3pp_us >= sample.sample.t2_us);
    }

    if smoke_count.is_some() {
        let values: Vec<u64> = report.samples.iter().map(|sample| sample.t3pp_us).collect();
        let summary = summarize(&values).expect("smoke summary");
        println!(
            "smoke T0 to T3'' microseconds: count={}, p50={}, p95={}, max={}",
            summary.count, summary.p50, summary.p95, summary.max
        );
        return;
    }

    let (receipt_path, sample_path) =
        write_stage_b_evidence(&report).expect("write stage B evidence");
    let receipt: Value =
        serde_json::from_slice(&std::fs::read(&receipt_path).expect("read receipt"))
            .expect("parse receipt");
    let samples: Value =
        serde_json::from_slice(&std::fs::read(&sample_path).expect("read samples"))
            .expect("parse samples");
    validate_receipt(&receipt);
    assert_eq!(report.samples.len(), SAMPLE_COUNT);
    assert_eq!(
        samples.as_array().expect("sample array").len(),
        SAMPLE_COUNT
    );
    assert_ne!(receipt["status"], "PASS");
    assert!(receipt["limitations"]
        .as_array()
        .expect("limitations array")
        .iter()
        .any(|value| value == "engineering fixture; not QUALITY-001"));
    assert_eq!(
        receipt["artifact_sha256"],
        artifact_sha256().expect("artifact hash")
    );
    assert_device_identity_is_null(&receipt);
}

fn validate_receipt(receipt: &Value) {
    let schema: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../contracts/evidence.schema.json"
    )))
    .expect("evidence schema");
    jsonschema::validator_for(&schema)
        .expect("evidence validator")
        .validate(receipt)
        .expect("receipt matches evidence schema");
}

fn assert_device_identity_is_null(value: &Value) {
    // The required generic limitation says "default render endpoint"; reject identity fields,
    // while allowing that non-identifying phrase and requiring hardware to remain null.
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                let normalized = key.to_ascii_lowercase();
                assert!(!normalized.contains("endpoint"));
                assert!(!normalized.contains("device_id"));
                assert!(!normalized.contains("friendly"));
                assert!(!normalized.contains("driver"));
                if normalized == "hardware" {
                    assert_eq!(child, &Value::Null);
                }
                assert_device_identity_is_null(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                assert_device_identity_is_null(item);
            }
        }
        _ => {}
    }
}
