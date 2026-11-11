use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

const SYNTHETIC_TOKEN: &str = "test-only-gsi-token-do-not-print";
const SYNTHETIC_STEAMID: &str = "76561198012345678";
static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);

struct Session {
    root: PathBuf,
    expected: PathBuf,
}

struct TempInput(PathBuf);

impl TempInput {
    fn new() -> Self {
        let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kohbor-replay-sequence-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempInput {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Session {
    fn new() -> Self {
        let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("kohbor-replay-gsilog-{}-{id}", std::process::id()));
        let raw = root.join("raw");
        fs::create_dir_all(&raw).unwrap();
        fs::write(root.join("manifest.json"), r#"{"format":"synthetic"}"#).unwrap();
        Self {
            expected: root.join("expected.json"),
            root,
        }
    }

    fn write_records(&self, records: &[Value]) {
        let lines = records
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(self.root.join("raw/000001.jsonl"), lines + "\n").unwrap();
    }

    fn write_expected(&self) {
        fs::write(
            &self.expected,
            json!({
                "events": [
                    {"kill_number": 1, "sequence": 10, "observed_clock_seconds": 2.0},
                    {"kill_number": 2, "sequence": 12, "observed_clock_seconds": 8.0},
                    {"kill_number": 3, "sequence": 100, "observed_clock_seconds": 27.0}
                ]
            })
            .to_string(),
        )
        .unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kohbor-replay"))
            .arg(&self.root)
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn record(sequence: u64, complete_ns: u64, kills: u64, streak: u64) -> Value {
    let body = payload(kills, streak).to_string();
    raw_record(sequence, complete_ns, &encode_base64(body.as_bytes()))
}

fn raw_record(sequence: u64, complete_ns: u64, body_base64: &str) -> Value {
    json!({
        "sequence": sequence,
        "body_complete_monotonic_ns": complete_ns.to_string(),
        "received_first_byte_monotonic_ns": complete_ns.saturating_sub(1_000_000).to_string(),
        "parse_status": "ok",
        "raw_body_base64": body_base64,
        "content_type": "application/json",
        "body_sha256": "synthetic",
        "capture_id": format!("capture-{sequence}")
    })
}

fn payload(kills: u64, streak: u64) -> Value {
    json!({
        "auth": {"token": SYNTHETIC_TOKEN},
        "provider": {"appid": 570},
        "map": {
            "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS",
            "customgamename": "",
            "matchid": "8000000001",
            "radiant_score": kills,
            "dire_score": 0
        },
        "player": {
            "activity": "playing",
            "steamid": SYNTHETIC_STEAMID,
            "team_name": "radiant",
            "kills": kills,
            "kill_streak": streak
        },
        "hero": {
            "name": "npc_dota_hero_silencer",
            "level": 5,
            "health": 800,
            "max_health": 1000,
            "mana_percent": 50,
            "alive": true,
            "respawn_seconds": 0
        }
    })
}

fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[usize::from(first >> 2)] as char);
        encoded.push(ALPHABET[usize::from(((first & 0b11) << 4) | (second >> 4))] as char);
        if chunk.len() > 1 {
            encoded.push(ALPHABET[usize::from(((second & 0b1111) << 2) | (third >> 6))] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(ALPHABET[usize::from(third & 0b11_1111)] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn run_input(path: &PathBuf, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kohbor-replay"))
        .arg(path)
        .args(args)
        .output()
        .unwrap()
}

fn assert_no_secret(output: &Output) {
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!combined.contains(SYNTHETIC_TOKEN));
    assert!(!combined.contains(SYNTHETIC_STEAMID));
    assert!(!combined.contains("8000000001"));
    assert!(!combined.contains("npc_dota_hero_silencer"));
    assert!(!combined.contains("capture-"));
}

fn build_session() -> Session {
    let session = Session::new();
    session.write_records(&[
        record(9, 100_000_000_000, 0, 0),
        record(10, 102_000_000_000, 1, 1),
        raw_record(11, 104_000_000_000, "%%%invalid-base64%%%"),
        record(12, 108_000_000_000, 2, 2),
        record(31, 127_000_000_000, 3, 3),
    ]);
    session.write_expected();
    session
}

#[test]
fn raw_session_streams_ticks_and_compares_ground_truth_without_exposing_identifiers() {
    let session = build_session();
    let expected_arg = session.expected.to_string_lossy().into_owned();
    let output = session.run(&["--expect-kills", &expected_arg, "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_no_secret(&output);

    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ticks"]["admitted"], 4);
    assert_eq!(report["ticks"]["undecodable"], 1);
    assert_eq!(report["timeline"][0]["seq"], 10);
    assert_eq!(report["timeline"][0]["t_ms"], 2_000);
    assert_eq!(report["timeline"][0]["at_ms"], 2_000);
    assert_eq!(report["ground_truth"]["expected"], 3);
    assert_eq!(report["ground_truth"]["matched"], 2);
    assert_eq!(report["ground_truth"]["unmatched"], 1);
    assert_eq!(report["ground_truth"]["unmatched_detector_hero_kills"], 1);
    assert_eq!(
        report["ground_truth"]["matches"][0]["detector_sequence"],
        10
    );
    assert_eq!(report["ground_truth"]["matches"][0]["event"], "hero_kill");
    assert_eq!(report["ground_truth"]["matches"][0]["delta"], 0);
    assert_eq!(
        report["ground_truth"]["matches"][1]["detector_sequence"],
        12
    );
    assert_eq!(report["ground_truth"]["matches"][1]["delta"], 0);
    assert!(report["ground_truth"]["matches"][2]["detector_sequence"].is_null());
    assert!(report["ground_truth"]["matches"][2]["delta"].is_null());
    assert_eq!(report["tier_events"][0]["id"], "multi_kill");
    assert_eq!(report["tier_events"][0]["tier"], 2);
    assert_eq!(report["tier_events"][0]["seq"], 12);
    assert_eq!(report["tier_events"][1]["id"], "streak");
    assert_eq!(report["tier_events"][1]["tier"], 3);
    assert_eq!(report["tier_events"][1]["seq"], 31);
    assert_eq!(report["tier_summary"]["multi_kill"]["2"], 1);
    assert_eq!(report["tier_summary"]["streak"]["3"], 1);
    assert_eq!(report["tier_summary"]["multi_kill"]["5"], 0);
    assert_eq!(report["tier_summary"]["streak"]["10"], 0);

    let text = session.run(&["--expect-kills", &expected_arg]);
    assert!(text.status.success());
    assert_no_secret(&text);
    let text = stdout(&text);
    assert!(text.contains("t_ms  seq  phase  kills  events"));
    assert!(text.contains("Ground truth comparison"));
    assert!(text.contains(
        "kill_number=3 expected_seq=100 observed_clock_seconds=27 hero_kill_seq=none delta=none"
    ));
    assert!(text.contains("expected=3 matched=2 unmatched=1 unmatched_detector_hero_kills=1"));
    assert!(text.contains("multi_kill tier=2 seq=12 t_ms=8000"));
    assert!(text.contains("streak tier=3 seq=31 t_ms=27000"));
}

#[test]
fn raw_session_json_output_is_parseable_without_optional_ground_truth() {
    let session = build_session();
    let output = session.run(&["--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_no_secret(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ticks"]["undecodable"], 1);
    assert_eq!(report["ground_truth"], Value::Null);
}

#[test]
fn earlier_input_forms_use_one_based_record_sequences() {
    let input = TempInput::new();
    let corpus = input.0.join("capture.jsonl");
    fs::write(
        &corpus,
        [
            json!({"t_ms": 0, "body": payload(0, 0)}).to_string(),
            json!({"t_ms": 1_000, "body": payload(1, 1)}).to_string(),
        ]
        .join("\n"),
    )
    .unwrap();
    let corpus_output = run_input(&corpus, &["--json"]);
    assert!(corpus_output.status.success());
    let corpus_report: Value = serde_json::from_slice(&corpus_output.stdout).unwrap();
    assert_eq!(corpus_report["timeline"][0]["seq"], 2);
    assert_eq!(corpus_report["timeline"][0]["t_ms"], 1_000);

    fs::write(input.0.join("02.json"), payload(1, 1).to_string()).unwrap();
    fs::write(input.0.join("01.json"), payload(0, 0).to_string()).unwrap();
    let directory_output = run_input(&input.0, &["--json"]);
    assert!(directory_output.status.success());
    let directory_report: Value = serde_json::from_slice(&directory_output.stdout).unwrap();
    assert_eq!(directory_report["timeline"][0]["seq"], 2);
    assert_eq!(directory_report["timeline"][0]["t_ms"], 1_000);
    assert_no_secret(&corpus_output);
    assert_no_secret(&directory_output);
}
