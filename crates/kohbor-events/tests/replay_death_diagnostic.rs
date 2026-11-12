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
        let dir =
            std::env::temp_dir().join(format!("kohbor-replay-deaths-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("capture.jsonl");
        let contents = records
            .iter()
            .map(|(at_ms, body)| json!({"t_ms": at_ms, "body": body}).to_string())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, contents + "\n").unwrap();
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Corpus {
    fn drop(&mut self) {
        if let Some(parent) = self.0.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}

fn payload(kills: u64, kill_streak: u64, deaths: u64, alive: bool) -> Value {
    json!({
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
            "kills": kills,
            "kill_streak": kill_streak,
            "deaths": deaths
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

#[test]
fn text_diagnostic_uses_deaths_and_reports_alive_only_flips_without_identifiers() {
    let corpus = Corpus::new(&[
        (0, payload(0, 0, 0, true)),
        (100, payload(3, 3, 0, false)),
        (200, payload(3, 3, 0, true)),
        (300, payload(3, 0, 1, true)),
        (400, payload(3, 0, 1, false)),
        (500, payload(3, 0, 1, true)),
        (600, payload(4, 1, 1, true)),
    ]);
    let output = Command::new(env!("CARGO_BIN_EXE_kohbor-replay"))
        .arg(corpus.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("deaths-counter increases=1"), "{text}");
    assert!(text.contains("alive-only flips=1"), "{text}");
    assert!(text.contains("compared=7 mismatches=0"), "{text}");
    assert!(!text.contains("8000000001"));
    assert!(!text.contains("76561198012345678"));
    assert!(!text.contains("npc_dota_hero_silencer"));
}
