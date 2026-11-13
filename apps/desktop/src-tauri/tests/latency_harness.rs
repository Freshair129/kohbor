use kohbor_desktop_lib::latency::{artifact_sha256, run_stage_a, write_evidence};
const SAMPLE_COUNT: usize = 300;
// The burst trace is replayed in real time (about 2 s per sample), so 300 samples take
// about ten minutes. Run explicitly to produce a receipt:
//   cargo test -p kohbor-desktop --test latency_harness --offline --locked -j 4 -- --ignored
#[test]
#[ignore = "engineering fixture; about 10 minutes of real-time replay; run with --ignored"]
fn stage_a_writes_valid_synthetic_receipt() {
    let report = run_stage_a(SAMPLE_COUNT).expect("stage A");
    let (receipt_path, sample_path) = write_evidence(&report).expect("write evidence");
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap();
    let samples: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&sample_path).unwrap()).unwrap();
    assert_eq!(samples.as_array().unwrap().len(), report.samples.len());
    let schema: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../contracts/evidence.schema.json"
    )))
    .unwrap();
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&receipt)
        .unwrap();
    assert!(report.samples.len() >= SAMPLE_COUNT);
    for sample in &report.samples {
        let timestamps = [0, sample.t1_us, sample.t2_us, sample.t3p_us];
        assert!(timestamps.windows(2).all(|pair| pair[0] <= pair[1]));
    }
    assert_ne!(receipt["status"], "PASS");
    assert!(receipt["limitations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "engineering fixture; not QUALITY-001"));
    assert_eq!(receipt["artifact_sha256"], artifact_sha256().unwrap());
}
