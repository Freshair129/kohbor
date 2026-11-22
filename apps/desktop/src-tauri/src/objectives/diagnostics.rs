use serde_json::{Map, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;

pub(crate) const MAX_DIAGNOSTIC_FILE_BYTES: u64 = 64 * 1024;
const MAX_ENTRY_BYTES: usize = 4 * 1024;
const MAX_MAP_KEYS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MapDiagnostic {
    pub(crate) map_keys: Vec<String>,
    pub(crate) roshan_values: Vec<(String, Value)>,
}

impl MapDiagnostic {
    pub(crate) fn from_map(map: &Map<String, Value>) -> Option<Self> {
        if map.len() > MAX_MAP_KEYS || map.keys().any(|key| key.len() > 128) {
            return None;
        }
        let mut map_keys = map.keys().cloned().collect::<Vec<_>>();
        map_keys.sort();
        let mut roshan_values = map
            .iter()
            .filter(|(key, _)| key.starts_with("roshan_"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        roshan_values.sort_by(|left, right| left.0.cmp(&right.0));
        Some(Self {
            map_keys,
            roshan_values,
        })
    }

    pub(crate) fn to_line(&self) -> Option<Vec<u8>> {
        let roshan = self
            .roshan_values
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Map<String, Value>>();
        let line = serde_json::to_vec(&serde_json::json!({
            "keys": &self.map_keys,
            "roshan": roshan,
        }))
        .ok()?;
        (line.len() + 1 <= MAX_ENTRY_BYTES).then_some(line)
    }
}

pub(crate) fn start_writer(
    receiver: Receiver<MapDiagnostic>,
    logs_dir: PathBuf,
    enabled: Arc<AtomicBool>,
) {
    let _ = thread::Builder::new()
        .name("objective-map-diagnostic".into())
        .spawn(move || {
            while let Ok(entry) = receiver.recv() {
                if !enabled.load(Ordering::Acquire) {
                    continue;
                }
                if let Some(line) = entry.to_line() {
                    let _ = append_bounded(&logs_dir, &line);
                }
            }
        });
}

fn append_bounded(logs_dir: &Path, line: &[u8]) -> std::io::Result<()> {
    if line.len() + 1 > MAX_ENTRY_BYTES as usize {
        return Ok(());
    }
    fs::create_dir_all(logs_dir)?;
    let path = logs_dir.join("objective-map-keys.log");
    let current_len = match fs::metadata(&path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error),
    };
    let entry_len = (line.len() + 1) as u64;
    let truncate = current_len.saturating_add(entry_len) > MAX_DIAGNOSTIC_FILE_BYTES;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(!truncate)
        .truncate(truncate)
        .open(path)?;
    file.write_all(line)?;
    file.write_all(b"\n")?;
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::{MapDiagnostic, MAX_DIAGNOSTIC_FILE_BYTES};
    use serde_json::{json, Map, Value};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "kohbor-objective-diagnostic-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn filter_keeps_sorted_map_key_names_and_only_roshan_values() {
        let map = json!({
            "zeta": "private player payload",
            "clock_time": 90,
            "roshan_state": "alive",
            "roshan_respawn_seconds": 42
        })
        .as_object()
        .unwrap()
        .clone();
        let entry = MapDiagnostic::from_map(&map).unwrap();
        assert_eq!(
            entry.map_keys,
            vec![
                "clock_time",
                "roshan_respawn_seconds",
                "roshan_state",
                "zeta"
            ]
        );
        assert_eq!(
            entry.roshan_values,
            vec![
                ("roshan_respawn_seconds".to_owned(), json!(42)),
                ("roshan_state".to_owned(), json!("alive")),
            ]
        );
        let line = String::from_utf8(entry.to_line().unwrap()).unwrap();
        assert!(line.contains("clock_time"));
        assert!(line.contains("roshan_state"));
        assert!(!line.contains("private player payload"));
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed.as_object().unwrap().len(), 2);
        assert_eq!(parsed["roshan"].as_object().unwrap().len(), 2);
    }

    #[test]
    fn diagnostic_file_rolls_over_at_its_exact_size_cap() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("objective-map-keys.log");
        fs::write(&path, vec![b'x'; (MAX_DIAGNOSTIC_FILE_BYTES - 2) as usize]).unwrap();
        super::append_bounded(&dir, b"{}").unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes, b"{}\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn filter_omits_values_for_non_roshan_keys() {
        let mut map = Map::new();
        map.insert("map_name".into(), json!("secret"));
        let entry = MapDiagnostic::from_map(&map).unwrap();
        assert_eq!(entry.map_keys, vec!["map_name"]);
        assert!(entry.roshan_values.is_empty());
        assert_eq!(
            entry.to_line().unwrap(),
            br#"{"keys":["map_name"],"roshan":{}}"#
        );
    }
}
