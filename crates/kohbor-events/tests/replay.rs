use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_CORPUS: AtomicU64 = AtomicU64::new(0);

struct Corpus(PathBuf);

impl Corpus {
    fn new(records: &[(u64, Value)]) -> Self {
        let id = NEXT_CORPUS.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("kohbor-replay-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let corpus = dir.join("capture.jsonl");
        let contents = records
            .iter()
            .map(|(t_ms, body)| json!({"t_ms": t_ms, "gap_ms": 0, "body": body}).to_string())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&corpus, contents + "\n").unwrap();
        Self(corpus)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn dir(&self) -> &std::path::Path {
        self.0.parent().unwrap()
    }
}

impl Drop for Corpus {
    fn drop(&mut self) {
        if let Some(parent) = self.0.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}

fn payload(kills: u64, kill_streak: u64, alive: bool, activity: &str) -> Value {
    json!({
        "provider": {"appid": 570},
        "map": {
            "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS",
            "customgamename": "",
            "matchid": "8000000001",
            "radiant_score": 5,
            "dire_score": 5
        },
        "player": {
            "activity": activity,
            "steamid": "<STEAMID>",
            "team_name": "radiant",
            "kills": kills,
            "kill_streak": kill_streak
        },
        "hero": {
            "name": "npc_dota_hero_silencer",
            "level": 12,
            "health": if alive { 800 } else { 0 },
            "max_health": 1000,
            "mana_percent": 50,
            "alive": alive,
            "respawn_seconds": 0
        }
    })
}

fn run(corpus: &Corpus, args: &[&str]) -> std::process::Output {
    run_path(corpus.path(), args)
}

fn run_path(path: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_kohbor-replay"))
        .arg(path)
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn emits_a_double_kill_when_kills_arrive_within_eighteen_seconds() {
    let corpus = Corpus::new(&[
        (0, payload(0, 0, true, "playing")),
        (1_000, payload(1, 1, true, "playing")),
        (18_999, payload(2, 2, true, "playing")),
    ]);

    let output = run(&corpus, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout(&output).contains("multi_kill:2"));
}

#[test]
fn does_not_emit_a_double_kill_when_kills_are_eighteen_point_zero_zero_one_seconds_apart() {
    let corpus = Corpus::new(&[
        (0, payload(0, 0, true, "playing")),
        (1_000, payload(1, 1, true, "playing")),
        (19_001, payload(2, 2, true, "playing")),
    ]);

    let output = run(&corpus, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!stdout(&output).contains("multi_kill:"));
}

#[test]
fn an_own_death_ends_the_streak() {
    let corpus = Corpus::new(&[
        (0, payload(0, 0, true, "playing")),
        (100, payload(1, 1, true, "playing")),
        (200, payload(2, 2, true, "playing")),
        (300, payload(3, 3, true, "playing")),
        (400, payload(3, 0, false, "playing")),
        (500, payload(3, 0, true, "playing")),
        (600, payload(4, 1, true, "playing")),
    ]);

    let output = run(&corpus, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = stdout(&output);
    assert_eq!(text.matches("streak:3").count(), 1);
    assert!(text.contains("hero_death"));
}

#[test]
fn counts_not_playing_payloads_as_skipped() {
    let corpus = Corpus::new(&[
        (0, payload(0, 0, true, "menu")),
        (1_000, payload(0, 0, true, "playing")),
    ]);

    let output = run(&corpus, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout(&output).contains("NotPlaying=1"));
}

#[test]
fn json_mode_prints_one_parseable_report_object() {
    let corpus = Corpus::new(&[(0, payload(0, 0, true, "playing"))]);

    let output = run(&corpus, &["--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ownership"], "synthetic in replay");
    assert_eq!(report["ticks"]["admitted"], 1);
    assert_eq!(report["diagnostic"]["compared"], 1);
    assert_eq!(report["diagnostic"]["mismatches"], 0);
}

#[test]
fn reads_payload_directories_in_sorted_name_order_with_one_second_steps() {
    let corpus = Corpus::new(&[]);
    fs::write(
        corpus.dir().join("02.json"),
        payload(1, 1, true, "playing").to_string(),
    )
    .unwrap();
    fs::write(
        corpus.dir().join("01.json"),
        payload(0, 0, true, "playing").to_string(),
    )
    .unwrap();

    let output = run_path(corpus.dir(), &["--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ticks"]["admitted"], 2);
    assert_eq!(report["first_admitted_t_ms"], 0);
    assert_eq!(report["last_admitted_t_ms"], 1_000);
    assert_eq!(report["timeline"][0]["at_ms"], 1_000);
}
