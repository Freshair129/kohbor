use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Owner-requested prototype caps; production retention remains governed by
// DEC-RETENTION in docs/governance/DECISION-LOG.md. Only matching log files
// inside this application's logs folder are eligible for rotation deletion.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FILES: usize = 5;
const RING_CAPACITY: usize = 2000;
const DEFAULT_TAIL_LIMIT: usize = 200;
const MAX_TAIL_LIMIT: usize = 1000;
const REDACTED: &str = "[redacted]";
const MAX_FIELDS: usize = 16;
const MAX_SECRETS: usize = 64;
const MAX_MESSAGE_BYTES: usize = 1024;
const MAX_FRONTEND_BYTES: usize = 300;
const MAX_KEY_BYTES: usize = 64;
const QUEUE_CAPACITY: usize = 4096;
const FLUSH_TIMEOUT: Duration = Duration::from_millis(200);
type WriterProbe = Box<dyn FnOnce() + Send>;

#[cfg(test)]
mod round3_tests {
    use super::*;
    use std::sync::mpsc;

    fn root(tag: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kohbor-log-r3-{tag}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn registry() -> SecretRegistry {
        let mut registry = SecretRegistry::default();
        registry.register("Ab7+/!qZ");
        registry
    }

    #[test]
    fn registered_short_secret_and_lowercase_are_redacted() {
        let registry = registry();
        assert_eq!(
            redact_registered("Ab7+/!qZ ab7+/!qz", &registry),
            "[redacted] [redacted]"
        );
    }

    #[test]
    fn registered_secret_with_inserted_punctuation_is_redacted() {
        assert_eq!(redact_registered("A.b.7.q.Z", &registry()), "[redacted]");
    }

    #[test]
    fn registered_secret_url_encoded_and_stripped_variants_are_redacted() {
        for variant in ["Ab7%2B%2F%21qZ", "%41%62%37%2b%2f%21%71%5a", "Ab7qZ"] {
            assert_eq!(redact_registered(variant, &registry()), "[redacted]");
        }
    }

    #[test]
    fn secret_registry_has_a_six_byte_minimum_and_never_evicts_an_active_value() {
        let mut registry = SecretRegistry::default();
        registry.register("short");
        assert!(registry.values.is_empty());
        for index in 0..70 {
            registry.register(&format!("credential-{index:02}"));
        }
        assert_eq!(registry.values.len(), MAX_SECRETS);
        assert!(registry.values.iter().any(|secret| secret
            .variants
            .first()
            .is_some_and(|v| v == "credential-00")));
    }

    #[test]
    fn a_message_cut_at_the_cap_never_keeps_the_prefix_of_a_secret_crossing_the_cut() {
        let secret = "SECRETSECRETSECRETSECRETSECRETSECRET-0123456789";
        // The secret starts 10 bytes before the cap: a plain cut would keep 10 readable bytes.
        let text = format!("{}{}", "x".repeat(MAX_MESSAGE_BYTES - 10), secret);
        let kept = safe_prefix(&text, MAX_MESSAGE_BYTES);
        assert!(kept.len() <= MAX_MESSAGE_BYTES - SECRET_TAIL_GUARD);
        assert!(!kept.contains("SECRET"));
        assert_eq!(safe_prefix("short", MAX_MESSAGE_BYTES), "short");
    }

    #[test]
    fn registered_values_are_removed_from_file_ring_and_field_keys() {
        let logger = Arc::new(Logger::new_with_limits(Some(root("exact")), 4096, 5, 32));
        with_test_logger(logger.clone(), || register_secret("Ab7+/!qZ"));
        logger.log(
            Level::Error,
            "test",
            "exact",
            "Ab7%2B%2F%21qZ",
            Map::from_iter([("Ab7+/!qZ".into(), json!("A.b.7.q.Z"))]),
        );
        logger.flush();
        let disk = fs::read_to_string(logger.current_file().unwrap()).unwrap();
        let ring = serde_json::to_string(&logger.entries()).unwrap();
        for text in [&disk, &ring] {
            for raw in ["Ab7+/!qZ", "Ab7%2B%2F%21qZ", "A.b.7.q.Z"] {
                assert!(!text.contains(raw));
            }
            assert!(text.contains("[redacted]"));
        }
    }

    #[test]
    fn frontend_five_megabyte_message_is_cut_before_sanitizing() {
        let logger = Logger::new_in_memory(32);
        let message = "!".repeat(5 * 1024 * 1024);
        logger.log_frontend_at(
            Instant::now(),
            Level::Error,
            "renderer",
            "frontend",
            &message,
            Fields::new(),
        );
        // Cut early by SECRET_TAIL_GUARD so a secret crossing the cut leaves no readable prefix.
        assert_eq!(
            logger.entries()[0].msg.len(),
            MAX_FRONTEND_BYTES - SECRET_TAIL_GUARD
        );
    }

    #[test]
    fn frontend_drops_nested_oversized_and_disallowed_fields() {
        let logger = Logger::new_in_memory(32);
        logger.log_frontend_at(
            Instant::now(),
            Level::Info,
            "renderer",
            "frontend",
            "safe",
            Map::from_iter([
                ("nested".into(), json!({"body": "private"})),
                ("array".into(), json!([1, 2])),
                ("large".into(), json!("!".repeat(41))),
                ("punctuation".into(), json!("{private}")),
                ("count".into(), json!(3)),
                ("ok".into(), json!(true)),
                ("code".into(), json!("ERROR_CODE")),
            ]),
        );
        let entry = &logger.entries()[0];
        assert_eq!(
            entry.fields,
            json!({"count": 3, "ok": true, "code": "ERROR_CODE"})
        );
    }

    #[test]
    fn frontend_hides_nine_digit_and_sixteen_character_runs() {
        let logger = Logger::new_in_memory(32);
        logger.log_frontend_at(
            Instant::now(),
            Level::Info,
            "renderer",
            "frontend",
            "123456789 ABCDEFGHIJKLMNOP",
            Fields::new(),
        );
        assert_eq!(logger.entries()[0].msg, "[redacted] [redacted]");
    }

    #[test]
    fn frontend_command_validation_rejects_long_strings_without_content_scanning() {
        let giant = "x".repeat(5 * 1024 * 1024);
        assert_eq!(
            validate_frontend_input("info", "renderer", &giant, None),
            Err("LOG_INPUT_TOO_LARGE")
        );
        assert!(validate_frontend_input(
            "info",
            "renderer",
            "safe",
            Some(&json!({"nested": [giant]}))
        )
        .is_err());
    }

    #[test]
    fn overlong_field_keys_cannot_hide_sensitive_suffixes() {
        let logger = Logger::new_in_memory(32);
        let key = format!("{}password", "prefix".repeat(20));
        logger.log(
            Level::Info,
            "test",
            "key",
            "safe",
            Map::from_iter([(key, json!("short private value"))]),
        );
        assert!(logger.entries()[0].fields.as_object().unwrap().is_empty());
    }

    #[test]
    fn redaction_expansion_cannot_exceed_output_byte_caps() {
        let logger = Arc::new(Logger::new_in_memory(32));
        with_test_logger(logger.clone(), || register_secret("AbCdEf"));
        logger.log(
            Level::Info,
            "test",
            "host",
            &"AbCdEf".repeat(200),
            Fields::new(),
        );
        logger.log_frontend_at(
            Instant::now(),
            Level::Info,
            "renderer",
            "frontend",
            &"AbCdEf".repeat(50),
            Map::from_iter([("safe".to_owned(), json!("AbCdEf".repeat(6)))]),
        );
        let entries = logger.entries();
        assert!(entries[0].msg.len() <= 1024);
        assert!(entries[1].msg.len() <= 300);
        assert!(entries[1].fields["safe"].as_str().unwrap().len() <= 40);
        assert!(entries.iter().all(|entry| !entry.msg.contains("AbCdEf")));
    }

    #[test]
    fn host_message_and_fields_have_byte_and_count_caps() {
        let logger = Logger::new_in_memory(32);
        let fields = (0..40)
            .map(|n| (format!("f{n}"), json!("€ ".repeat(500))))
            .collect();
        logger.log(Level::Info, "test", "bounds", &"€ ".repeat(1000), fields);
        let entry = &logger.entries()[0];
        assert!(entry.msg.len() <= 1024);
        assert!(entry.fields.as_object().unwrap().len() <= 16);
        assert!(entry.fields.as_object().unwrap().values().all(|value| value
            .as_str()
            .unwrap()
            .len()
            <= 1024));
    }

    #[test]
    fn panic_with_logger_lock_held_finishes_and_flush_is_bounded() {
        install_panic_hook();
        let logger = Arc::new(Logger::new_in_memory(32));
        let _guard = logger.state.lock().unwrap();
        let start = Instant::now();
        with_test_logger(logger.clone(), || {
            assert!(std::panic::catch_unwind(|| panic!("private panic probe")).is_err());
        });
        logger.flush();
        assert!(start.elapsed() < Duration::from_millis(450));
    }

    #[test]
    fn panic_message_secret_is_redacted_without_forwarding_original_hook() {
        install_panic_hook();
        let logger = Arc::new(Logger::new_in_memory(32));
        logger.register_for_test("Ab7+/!qZ");
        with_test_logger(logger.clone(), || {
            let _ = std::panic::catch_unwind(|| panic!("probe Ab7+/!qZ"));
        });
        let entries = logger.entries();
        let entry = entries.iter().find(|entry| entry.event == "panic").unwrap();
        assert_eq!(entry.msg, "probe [redacted]");
    }

    #[test]
    fn reentrant_panic_reporting_uses_message_free_fallback() {
        install_panic_hook();
        let logger = Arc::new(Logger::new_in_memory(32));
        with_test_logger(logger.clone(), || {
            PANIC_ACTIVE.with(|active| {
                active.set(true);
                let _ = std::panic::catch_unwind(|| panic!("must not escape"));
                active.set(false);
            });
        });
        assert!(logger.entries().is_empty());
    }

    #[test]
    fn reparse_attributes_are_refused_even_without_symlink_flag() {
        assert!(redirected_directory(false, 0x400));
        assert!(redirected_directory(true, 0));
        assert!(!redirected_directory(false, 0x10));
    }

    #[test]
    fn redirected_logs_directory_never_writes_or_deletes_target_files() {
        let root = root("redirected");
        let outside = root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        for n in 0..7 {
            fs::write(
                outside.join(format!("kohbor-20200101-0000{n:02}.log")),
                "protected",
            )
            .unwrap();
        }
        #[cfg(windows)]
        let link = std::os::windows::fs::symlink_dir(&outside, root.join("logs"));
        #[cfg(unix)]
        let link = std::os::unix::fs::symlink(&outside, root.join("logs"));
        if let Err(error) = link {
            eprintln!("SKIP directory-symlink test: creation is not permitted: {error}");
            return;
        }
        let logger = Logger::new_with_limits(Some(root), 512, 5, 32);
        for _ in 0..12 {
            logger.log(
                Level::Warn,
                "test",
                "redirect",
                "must stay in memory",
                Fields::new(),
            );
        }
        logger.flush();
        assert!(logger.is_degraded());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 7);
        assert_eq!(
            logger
                .entries()
                .iter()
                .filter(|entry| entry.event == "log_storage_unavailable")
                .count(),
            1
        );
    }

    #[test]
    fn directory_replaced_after_initialization_is_refused_before_rotation() {
        let root = root("rotation-redirect");
        let outside = root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("kohbor-20200101-000000.log"), "protected").unwrap();
        let logger = Logger::new_with_limits(Some(root.clone()), 512, 5, 32);
        logger.flush();
        assert!(logger.current_file().is_some());
        // Windows can deny renaming a directory containing an open file. Close
        // this test file at a rotation boundary before replacing the directory.
        logger.state.lock().unwrap().writer.take();
        fs::rename(root.join("logs"), root.join("original-logs")).unwrap();
        #[cfg(windows)]
        let link = std::os::windows::fs::symlink_dir(&outside, root.join("logs"));
        #[cfg(unix)]
        let link = std::os::unix::fs::symlink(&outside, root.join("logs"));
        if let Err(error) = link {
            eprintln!("SKIP rotation-symlink test: creation is not permitted: {error}");
            return;
        }
        for _ in 0..12 {
            logger.log(
                Level::Warn,
                "test",
                "rotation",
                &"x ".repeat(160),
                Fields::new(),
            );
        }
        logger.flush();
        assert!(logger.is_degraded());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
        assert_eq!(
            fs::read_to_string(outside.join("kohbor-20200101-000000.log")).unwrap(),
            "protected"
        );
    }

    #[test]
    fn stalled_fake_sink_does_not_block_ten_thousand_callers() {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let logger = Logger::with_test_sink(Box::new(move || {
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
        }));
        logger.log(Level::Info, "test", "stall", "first", Fields::new());
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let start = Instant::now();
        for _ in 0..10_000 {
            logger.log(Level::Info, "test", "queue", "small", Fields::new());
        }
        let elapsed = start.elapsed();
        let dropped = logger.dropped_entries();
        let flush_start = Instant::now();
        logger.flush();
        let flush_elapsed = flush_start.elapsed();
        release_tx.send(()).unwrap();
        assert!(elapsed < Duration::from_secs(1), "caller time: {elapsed:?}");
        assert!(flush_elapsed < Duration::from_millis(300));
        assert!(dropped > 0);
        std::thread::sleep(Duration::from_millis(1100));
        let entries = logger.entries();
        let warnings: Vec<_> = entries
            .iter()
            .filter(|entry| entry.event == "log_entries_dropped")
            .collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].fields["dropped"], dropped);
    }

    #[test]
    fn admission_logging_source_never_passes_body_or_payload_values() {
        fn unsafe_arguments(source: &str) -> bool {
            let mut stripped = String::new();
            let mut quoted = false;
            let mut escaped = false;
            for ch in source.chars() {
                if quoted {
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        quoted = false;
                    }
                    stripped.push(' ');
                } else if ch == '"' {
                    quoted = true;
                    stripped.push(' ');
                } else {
                    stripped.push(ch);
                }
            }
            stripped.split("log_").skip(1).any(|tail| {
                let Some((_, arguments)) = tail.split_once("!(") else {
                    return false;
                };
                let mut depth = 1;
                let end = arguments
                    .char_indices()
                    .find_map(|(index, ch)| {
                        if ch == '(' {
                            depth += 1;
                        }
                        if ch == ')' {
                            depth -= 1;
                        }
                        (depth == 0).then_some(index)
                    })
                    .unwrap_or(arguments.len());
                arguments[..end]
                    .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                    .any(|word| matches!(word, "payload" | "body"))
            })
        }
        assert!(unsafe_arguments("log_info!(\"gsi\", \"event\", body);"));
        assert!(!unsafe_arguments(
            "log_info!(\"gsi\", \"event\", \"payload rejected\");"
        ));
        assert!(!unsafe_arguments(include_str!("intake.rs")));
    }
}

pub type Fields = Map<String, Value>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    fn as_u8(self) -> u8 {
        match self {
            Self::Error => 1,
            Self::Warn => 2,
            Self::Info => 3,
            Self::Debug => 4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        if value.len() > 5 {
            return Err("INVALID_LOG_LEVEL");
        }
        match value.to_ascii_lowercase().as_str() {
            "error" => Ok(Self::Error),
            "warn" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            _ => Err("INVALID_LOG_LEVEL"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LogEntry {
    pub seq: u64,
    pub ts: String,
    pub level: Level,
    pub module: String,
    pub event: String,
    pub msg: String,
    pub fields: Value,
    pub v: String,
    pub commit: Option<String>,
}

impl LogEntry {
    fn file_value(&self) -> Value {
        json!({
            "ts": self.ts,
            "level": self.level.as_str(),
            "module": self.module,
            "event": self.event,
            "msg": self.msg,
            "fields": self.fields,
            "v": self.v,
            "commit": self.commit,
        })
    }

    fn tail_value(&self) -> Value {
        json!({
            "seq": self.seq,
            "ts": self.ts,
            "level": self.level.as_str(),
            "module": self.module,
            "event": self.event,
            "msg": self.msg,
            "fields": self.fields,
            "v": self.v,
            "commit": self.commit,
        })
    }
}

struct LoggerState {
    logs_dir: Option<PathBuf>,
    current_file: Option<PathBuf>,
    writer: Option<BufWriter<File>>,
    file_len: u64,
    ring: VecDeque<LogEntry>,
    next_seq: u64,
    last_flush: Instant,
    degraded: bool,
    degradation_reported: bool,
}
struct SinkConfig {
    data_dir: Option<PathBuf>,
    max_file_bytes: u64,
    max_files: usize,
    ring_capacity: usize,
    memory_only: bool,
    report_stderr: bool,
}
struct RawEntry {
    at: SystemTime,
    level: Level,
    module: String,
    event: String,
    msg: String,
    fields: Fields,
    frontend: bool,
}
impl RawEntry {
    fn bounded(
        level: Level,
        module: &str,
        event: &str,
        msg: &str,
        fields: Fields,
        frontend: bool,
    ) -> Self {
        let fields = fields
            .into_iter()
            .take(MAX_FIELDS)
            .filter_map(|(key, value)| {
                // Do not truncate away a sensitive suffix before classification.
                // A key beyond the inspection budget makes its whole field ineligible.
                if key.len() > MAX_KEY_BYTES {
                    return None;
                }
                let value = match value {
                    Value::String(text) if !frontend || text.len() <= 40 => Value::String(
                        safe_prefix(&text, if frontend { 40 } else { MAX_MESSAGE_BYTES })
                            .to_owned(),
                    ),
                    Value::Number(_) | Value::Bool(_) => value,
                    Value::Null if !frontend => value,
                    _ => return None,
                };
                Some((prefix(&key, MAX_KEY_BYTES).to_owned(), value))
            })
            .collect();
        Self {
            at: SystemTime::now(),
            level,
            module: prefix(module, 64).to_owned(),
            event: prefix(event, 96).to_owned(),
            msg: safe_prefix(
                msg,
                if frontend {
                    MAX_FRONTEND_BYTES
                } else {
                    MAX_MESSAGE_BYTES
                },
            )
            .to_owned(),
            fields,
            frontend,
        }
    }
}
enum WriterCommand {
    Entry(RawEntry),
    Flush(SyncSender<()>),
}
struct Sink {
    state: Arc<Mutex<LoggerState>>,
    config: SinkConfig,
    secrets: Arc<Mutex<SecretRegistry>>,
}
pub struct Logger {
    state: Arc<Mutex<LoggerState>>,
    sender: SyncSender<WriterCommand>,
    level: AtomicU8,
    dropped: Arc<AtomicU64>,
    pending_drops: Arc<AtomicU64>,
    frontend_origin: Instant,
    frontend_rate: AtomicU64,
    requested_logs_dir: Option<PathBuf>,
    #[cfg(test)]
    secrets: Arc<Mutex<SecretRegistry>>,
}
impl Logger {
    fn start(
        config: SinkConfig,
        secrets: Arc<Mutex<SecretRegistry>>,
        probe: Option<WriterProbe>,
    ) -> Self {
        let requested_logs_dir = config.data_dir.as_ref().map(|dir| dir.join("logs"));
        let state = Arc::new(Mutex::new(LoggerState {
            logs_dir: None,
            current_file: None,
            writer: None,
            file_len: 0,
            ring: VecDeque::with_capacity(config.ring_capacity.max(1)),
            next_seq: 0,
            last_flush: Instant::now(),
            degraded: config.memory_only,
            degradation_reported: config.memory_only,
        }));
        let sink = Arc::new(Sink {
            state: state.clone(),
            config,
            secrets: secrets.clone(),
        });
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let pending_drops = Arc::new(AtomicU64::new(0));
        let worker_drops = pending_drops.clone();
        let worker_sink = sink.clone();
        if std::thread::Builder::new()
            .name("log-writer".to_owned())
            .spawn(move || {
                crate::host::set_current_thread_priority(crate::host::ThreadPriority::BelowNormal);
                writer_loop(worker_sink, receiver, worker_drops, probe);
            })
            .is_err()
        {
            let mut state = state.lock().unwrap_or_else(|poison| poison.into_inner());
            sink.degrade(&mut state, "LOG_WRITER_UNAVAILABLE");
        }
        Self {
            state,
            sender,
            level: AtomicU8::new(Level::Info.as_u8()),
            dropped,
            pending_drops,
            frontend_origin: Instant::now(),
            frontend_rate: AtomicU64::new(0),
            requested_logs_dir,
            #[cfg(test)]
            secrets,
        }
    }
    fn config(
        data_dir: Option<PathBuf>,
        max_file_bytes: u64,
        max_files: usize,
        ring_capacity: usize,
        report_stderr: bool,
    ) -> SinkConfig {
        SinkConfig {
            data_dir,
            max_file_bytes: max_file_bytes.max(512),
            max_files: max_files.max(1),
            ring_capacity: ring_capacity.max(1),
            memory_only: false,
            report_stderr,
        }
    }
    #[cfg(test)]
    fn new_with_limits(
        data_dir: Option<PathBuf>,
        max_file_bytes: u64,
        max_files: usize,
        ring_capacity: usize,
    ) -> Self {
        Self::start(
            Self::config(data_dir, max_file_bytes, max_files, ring_capacity, false),
            Arc::new(Mutex::new(SecretRegistry::default())),
            None,
        )
    }
    #[cfg(test)]
    fn new_in_memory(ring_capacity: usize) -> Self {
        let mut config = Self::config(None, MAX_FILE_BYTES, MAX_FILES, ring_capacity, false);
        config.memory_only = true;
        Self::start(
            config,
            Arc::new(Mutex::new(SecretRegistry::default())),
            None,
        )
    }
    #[cfg(test)]
    fn with_test_sink(probe: WriterProbe) -> Self {
        let mut config = Self::config(None, MAX_FILE_BYTES, MAX_FILES, 8192, false);
        config.memory_only = true;
        Self::start(
            config,
            Arc::new(Mutex::new(SecretRegistry::default())),
            Some(probe),
        )
    }
    #[cfg(test)]
    fn register_for_test(&self, value: &str) {
        self.secrets
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .register(value);
    }
    fn enqueue(&self, entry: RawEntry) -> bool {
        match self.sender.try_send(WriterCommand::Entry(entry)) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                self.pending_drops.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
    /// Host callers must never supply player names, OCR text, raw GSI payloads,
    /// pack contents, screenshots, or credentials as diagnostics. This API copies
    /// bounded scalar data and try_sends only; the writer performs all scanning,
    /// secret redaction, ring updates, and file I/O.
    pub fn log(&self, level: Level, module: &str, event: &str, msg: &str, fields: Fields) {
        let _ = self.try_log(level, module, event, msg, fields);
    }
    fn try_log(&self, level: Level, module: &str, event: &str, msg: &str, fields: Fields) -> bool {
        if level.as_u8() > self.level.load(Ordering::Relaxed) {
            return false;
        }
        self.enqueue(RawEntry::bounded(level, module, event, msg, fields, false))
    }
    pub fn dropped_entries(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    pub fn flush(&self) {
        let (ack, reply) = mpsc::sync_channel(1);
        if self.sender.try_send(WriterCommand::Flush(ack)).is_ok() {
            let _ = reply.recv_timeout(FLUSH_TIMEOUT);
        }
    }
    fn panic_record(&self, message: &str, location: &str, line: u32, column: u32) {
        let Ok(guard) = self.state.try_lock() else {
            panic_stderr(line, column);
            return;
        };
        drop(guard);
        let fields = Map::from_iter([
            (
                "location".to_owned(),
                Value::String(prefix(location, MAX_MESSAGE_BYTES).to_owned()),
            ),
            ("line".to_owned(), json!(line)),
            ("column".to_owned(), json!(column)),
        ]);
        self.enqueue(RawEntry::bounded(
            Level::Error,
            "panic",
            "panic",
            message,
            fields,
            false,
        ));
        self.flush();
    }
    pub fn entries(&self) -> Vec<LogEntry> {
        #[cfg(test)]
        self.flush();
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .ring
            .iter()
            .cloned()
            .collect()
    }
    pub fn tail_value(&self, since_seq: Option<u64>, limit: Option<u32>) -> Value {
        #[cfg(test)]
        self.flush();
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let limit = limit
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_TAIL_LIMIT)
            .clamp(1, MAX_TAIL_LIMIT);
        let dropped = since_seq.is_some_and(|since| {
            state
                .ring
                .front()
                .is_some_and(|first| first.seq > since.saturating_add(1))
        });
        let matching: Vec<_> = state
            .ring
            .iter()
            .filter(|entry| since_seq.is_none_or(|since| entry.seq > since))
            .collect();
        let start = if since_seq.is_some() {
            0
        } else {
            matching.len().saturating_sub(limit)
        };
        let selected = &matching[start..matching.len().min(start.saturating_add(limit))];
        let last_seq = selected.last().map_or(state.next_seq, |entry| entry.seq);
        json!({"entries": selected.iter().map(|entry| entry.tail_value()).collect::<Vec<_>>(), "last_seq": last_seq, "dropped": dropped})
    }
    pub fn set_level(&self, level: Level) {
        self.level.store(level.as_u8(), Ordering::Relaxed);
    }
    pub fn is_degraded(&self) -> bool {
        #[cfg(test)]
        self.flush();
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .degraded
    }
    pub fn current_file(&self) -> Option<PathBuf> {
        #[cfg(test)]
        self.flush();
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .current_file
            .clone()
    }
    pub fn log_dir_value(&self) -> String {
        self.requested_logs_dir
            .as_ref()
            .map(|dir| dir.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
    pub fn log_frontend_at(
        &self,
        now: Instant,
        level: Level,
        module: &str,
        event: &str,
        msg: &str,
        fields: Fields,
    ) -> bool {
        let window = now
            .saturating_duration_since(self.frontend_origin)
            .as_secs()
            .min(u64::from(u32::MAX));
        let mut old = self.frontend_rate.load(Ordering::Relaxed);
        loop {
            let count = if old >> 32 == window {
                old & 0xffff_ffff
            } else {
                0
            };
            if count > 20 {
                return false;
            }
            let new = (window << 32) | (count + 1);
            match self.frontend_rate.compare_exchange_weak(
                old,
                new,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) if count == 20 => {
                    self.enqueue(RawEntry::bounded(
                        Level::Warn,
                        "logging",
                        "frontend_rate_limited",
                        "renderer log rate limit exceeded",
                        Fields::new(),
                        false,
                    ));
                    return false;
                }
                Ok(_) => {
                    if level.as_u8() > self.level.load(Ordering::Relaxed) {
                        return false;
                    }
                    return self
                        .enqueue(RawEntry::bounded(level, module, event, msg, fields, true));
                }
                Err(current) => old = current,
            }
        }
    }
}
impl Sink {
    fn initialize(&self) {
        if self.config.memory_only {
            return;
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let result = self
            .config
            .data_dir
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOG_DIRECTORY_UNAVAILABLE"))
            .and_then(|root| {
                let dir = root.join("logs");
                match fs::symlink_metadata(&dir) {
                    Ok(_) => check_logs_dir(&dir)?,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
                fs::create_dir_all(&dir)?;
                check_logs_dir(&dir)?;
                state.logs_dir = Some(dir);
                self.open_new_file(&mut state)
            });
        if result.is_err() {
            self.degrade(&mut state, "LOG_DIRECTORY_UNAVAILABLE");
        }
    }
    fn make_entry(&self, sequence: u64, raw: RawEntry) -> LogEntry {
        let registry = self
            .secrets
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let version = crate::version::AppVersion::current();
        let mut entry = LogEntry {
            seq: sequence,
            ts: utc_timestamp(raw.at),
            level: raw.level,
            module: redact_with(&raw.module, &registry, raw.frontend),
            event: redact_with(&raw.event, &registry, raw.frontend),
            msg: redact_with(&raw.msg, &registry, raw.frontend),
            fields: redact_scalar_fields(raw.fields, &registry, raw.frontend),
            v: prefix(version.version, 64).to_owned(),
            commit: version.git_commit.map(|value| prefix(&value, 7).to_owned()),
        };
        fit_entry_to_file_limit(&mut entry, self.config.max_file_bytes);
        entry
    }
    fn append(&self, state: &mut LoggerState, raw: RawEntry) -> LogEntry {
        state.next_seq = state.next_seq.saturating_add(1);
        let entry = self.make_entry(state.next_seq, raw);
        state.ring.push_back(entry.clone());
        while state.ring.len() > self.config.ring_capacity {
            state.ring.pop_front();
        }
        entry
    }
    fn degrade(&self, state: &mut LoggerState, code: &str) {
        state.degraded = true;
        state.writer = None;
        state.logs_dir = None;
        state.current_file = None;
        if !state.degradation_reported {
            state.degradation_reported = true;
            self.append(
                state,
                RawEntry::bounded(
                    Level::Warn,
                    "logging",
                    "log_storage_unavailable",
                    "file logging unavailable; using the in-memory ring",
                    Map::from_iter([("code".to_owned(), Value::String(code.to_owned()))]),
                    false,
                ),
            );
            if self.config.report_stderr {
                let _ = io::stderr().write_all(b"Kohbor file logging unavailable; ring only.\n");
            }
        }
    }
    fn open_new_file(&self, state: &mut LoggerState) -> io::Result<()> {
        let dir = state
            .logs_dir
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOG_DIRECTORY_UNAVAILABLE"))?;
        check_logs_dir(dir)?;
        let mut seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let (file, path) = loop {
            let path = dir.join(log_filename_at(seconds));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => break (file, path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    seconds = seconds.saturating_add(1)
                }
                Err(error) => return Err(error),
            }
        };
        state.writer = Some(BufWriter::new(file));
        state.current_file = Some(path);
        state.file_len = 0;
        retain_logs(dir, self.config.max_files, state.current_file.as_deref())?;
        state.last_flush = Instant::now();
        Ok(())
    }
    fn write_line(&self, state: &mut LoggerState, line: &[u8]) -> io::Result<()> {
        let dir = state
            .logs_dir
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOG_DIRECTORY_UNAVAILABLE"))?;
        check_logs_dir(dir)?;
        if line.len() as u64 > self.config.max_file_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LOG_RECORD_TOO_LARGE",
            ));
        }
        if state.writer.is_none()
            || state.file_len.saturating_add(line.len() as u64) > self.config.max_file_bytes
        {
            if let Some(writer) = state.writer.as_mut() {
                writer.flush()?;
            }
            self.open_new_file(state)?;
        }
        let writer = state
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOG_FILE_UNAVAILABLE"))?;
        writer.write_all(line)?;
        state.file_len = state.file_len.saturating_add(line.len() as u64);
        Ok(())
    }
    fn record(&self, raw: RawEntry) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let entry = self.append(&mut state, raw);
        if state.degraded {
            return;
        }
        let result = serde_json::to_vec(&entry.file_value())
            .map_err(io::Error::other)
            .and_then(|mut bytes| {
                bytes.push(b'\n');
                self.write_line(&mut state, &bytes)?;
                if matches!(entry.level, Level::Error | Level::Warn) {
                    if let Some(writer) = state.writer.as_mut() {
                        writer.flush()?;
                    }
                    state.last_flush = Instant::now();
                }
                Ok(())
            });
        if result.is_err() {
            self.degrade(&mut state, "LOG_WRITE_FAILED");
        }
    }
    fn flush(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.degraded {
            return;
        }
        let result = state
            .logs_dir
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOG_DIRECTORY_UNAVAILABLE"))
            .and_then(|dir| check_logs_dir(dir))
            .and_then(|()| state.writer.as_mut().map_or(Ok(()), Write::flush));
        if result.is_err() {
            self.degrade(&mut state, "LOG_FLUSH_FAILED");
        }
        state.last_flush = Instant::now();
    }
}
fn writer_loop(
    sink: Arc<Sink>,
    receiver: Receiver<WriterCommand>,
    pending_drops: Arc<AtomicU64>,
    mut probe: Option<WriterProbe>,
) {
    sink.initialize();
    let mut next_periodic = Instant::now() + Duration::from_secs(1);
    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(WriterCommand::Entry(raw)) => {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Some(probe) = probe.take() {
                        let _guard = sink
                            .state
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner());
                        probe();
                    }
                    sink.record(raw);
                }))
                .is_err()
                {
                    sink.record(RawEntry::bounded(
                        Level::Error,
                        "logging",
                        "writer_panicked",
                        "log writer recovered from a panic",
                        Fields::new(),
                        false,
                    ));
                }
            }
            Ok(WriterCommand::Flush(ack)) => {
                sink.flush();
                let _ = ack.try_send(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                sink.flush();
                break;
            }
        }
        if Instant::now() >= next_periodic {
            let dropped = pending_drops.swap(0, Ordering::Relaxed);
            if dropped > 0 {
                sink.record(RawEntry::bounded(
                    Level::Warn,
                    "logging",
                    "log_entries_dropped",
                    "bounded log queue dropped entries",
                    Map::from_iter([("dropped".to_owned(), json!(dropped))]),
                    false,
                ));
            }
            sink.flush();
            next_periodic = Instant::now() + Duration::from_secs(1);
        }
    }
}
fn redirected_directory(is_symlink: bool, attributes: u32) -> bool {
    is_symlink || attributes & 0x400 != 0
}
fn redirected_metadata(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    let attributes = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes()
    };
    #[cfg(not(windows))]
    let attributes = 0;
    redirected_directory(metadata.file_type().is_symlink(), attributes)
}
fn check_logs_dir(dir: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(dir)?;
    if !metadata.is_dir() || redirected_metadata(&metadata) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "LOG_DIRECTORY_REDIRECTED",
        ));
    }
    Ok(())
}
fn retain_logs(dir: &Path, max_files: usize, current_file: Option<&Path>) -> io::Result<()> {
    check_logs_dir(dir)?;
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).ok()?;
            (path.parent() == Some(dir)
                && metadata.is_file()
                && !redirected_metadata(&metadata)
                && is_log_filename(&entry.file_name().to_string_lossy()))
            .then_some(path)
        })
        .collect();
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    while files.len() > max_files {
        let Some(index) = files
            .iter()
            .position(|path| Some(path.as_path()) != current_file)
        else {
            break;
        };
        let oldest = files.remove(index);
        check_logs_dir(dir)?;
        let metadata = fs::symlink_metadata(&oldest)?;
        if metadata.is_file() && !redirected_metadata(&metadata) && oldest.parent() == Some(dir) {
            fs::remove_file(oldest)?;
        }
    }
    Ok(())
}

fn is_log_filename(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 26
        && &bytes[..7] == b"kohbor-"
        && bytes[7..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'-'
        && bytes[16..22].iter().all(u8::is_ascii_digit)
        && &bytes[22..] == b".log"
}

fn log_filename_at(seconds: u64) -> String {
    let timestamp = utc_parts(seconds, 0);
    format!(
        "kohbor-{:04}{:02}{:02}-{:02}{:02}{:02}.log",
        timestamp.year,
        timestamp.month,
        timestamp.day,
        timestamp.hour,
        timestamp.minute,
        timestamp.second
    )
}

struct UtcParts {
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millis: u32,
}

fn utc_parts(seconds: u64, millis: u32) -> UtcParts {
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    UtcParts {
        year,
        month: month as u32,
        day: day as u32,
        hour: (day_seconds / 3_600) as u32,
        minute: ((day_seconds % 3_600) / 60) as u32,
        second: (day_seconds % 60) as u32,
        millis,
    }
}

fn utc_timestamp(time: SystemTime) -> String {
    let since_epoch = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let parts = utc_parts(since_epoch.as_secs(), since_epoch.subsec_millis());
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second, parts.millis
    )
}

#[derive(Clone)]
struct RegisteredSecret {
    variants: Vec<String>,
    normalized: Vec<char>,
}
#[derive(Clone, Default)]
struct SecretRegistry {
    values: VecDeque<RegisteredSecret>,
}
impl SecretRegistry {
    fn register(&mut self, value: &str) {
        let value = prefix(value, MAX_MESSAGE_BYTES);
        if value.len() < 6
            || self
                .values
                .iter()
                .any(|secret| secret.variants.first().is_some_and(|v| v == value))
        {
            return;
        }
        let stripped: String = value.chars().filter(|ch| ch.is_alphanumeric()).collect();
        let mut variants = vec![
            value.to_owned(),
            value.to_lowercase(),
            stripped.clone(),
            stripped.to_lowercase(),
        ];
        for text in variants.clone() {
            let encoded = url_encode(&text, false);
            let all_encoded = url_encode(&text, true);
            variants.extend([
                encoded.clone(),
                encoded.to_lowercase(),
                all_encoded.clone(),
                all_encoded.to_lowercase(),
            ]);
        }
        variants.retain(|value| !value.is_empty());
        let normalized = normalized_units(value)
            .into_iter()
            .map(|(ch, _, _)| ch)
            .collect();
        if self.values.len() >= MAX_SECRETS {
            // Never evict an active credential: refuse the newest value instead (the caller set
            // is tiny: token, install key, owner ids).
            return;
        }
        self.values.push_back(RegisteredSecret {
            variants,
            normalized,
        });
    }
}
static SECRETS: OnceLock<Arc<Mutex<SecretRegistry>>> = OnceLock::new();
fn secret_registry() -> Arc<Mutex<SecretRegistry>> {
    SECRETS
        .get_or_init(|| Arc::new(Mutex::new(SecretRegistry::default())))
        .clone()
}
/// Register known credentials/identity values in memory only. Values shorter
/// than six bytes are ignored; up to 64 distinct values are kept and an active one is never evicted.
/// Never emit the value itself as a diagnostic.
pub fn register_secret(value: &str) {
    #[cfg(test)]
    if let Some(logger) = SCOPED_TEST_LOGGER.with(|active| active.borrow().clone()) {
        logger.register_for_test(value);
        return;
    }
    secret_registry()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .register(value);
}
/// Longest registered secret the redactor can be asked to hide. A message that has to be cut is cut
/// this much EARLIER, so a secret that crosses the cut can never leave a readable prefix behind
/// (exact-value redaction only sees what is left after truncation).
const SECRET_TAIL_GUARD: usize = 128;

fn safe_prefix(input: &str, cap: usize) -> &str {
    if input.len() <= cap {
        input
    } else {
        prefix(input, cap.saturating_sub(SECRET_TAIL_GUARD))
    }
}

fn prefix(input: &str, cap: usize) -> &str {
    let mut end = input.len().min(cap);
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    &input[..end]
}

fn cap_redacted_output(mut output: String, cap: usize) -> String {
    if output.len() <= cap {
        return output;
    }
    let mut end = prefix(&output, cap).len();
    // Preserve a complete marker when replacement text crosses the byte cap.
    if let Some(start) = output[..end].rfind('[') {
        if output[start..].starts_with(REDACTED) && start + REDACTED.len() > end {
            end = start;
        }
    }
    output.truncate(end);
    output
}
fn url_encode(input: &str, all: bool) -> String {
    let mut result = String::new();
    for byte in input.bytes() {
        if !all && (is_base64url(byte) || byte == b'.' || byte == b'~') {
            result.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(result, "%{byte:02X}");
        }
    }
    result
}
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
fn normalized_units(input: &str) -> Vec<(char, usize, usize)> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut spans = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let decoded_hex = if bytes[index] == b'%' && index + 2 < bytes.len() {
            hex_value(bytes[index + 1]).zip(hex_value(bytes[index + 2]))
        } else {
            None
        };
        if let Some((high, low)) = decoded_hex {
            decoded.push(high * 16 + low);
            spans.push((index, index + 3));
            index += 3;
        } else {
            decoded.push(bytes[index]);
            spans.push((index, index + 1));
            index += 1;
        }
    }
    let mut result = Vec::new();
    if let Ok(text) = std::str::from_utf8(&decoded) {
        for (index, ch) in text.char_indices() {
            for lower in ch.to_lowercase().filter(|ch| ch.is_alphanumeric()) {
                result.push((lower, spans[index].0, spans[index + ch.len_utf8() - 1].1));
            }
        }
    } else {
        for (index, byte) in decoded
            .iter()
            .enumerate()
            .filter(|(_, byte)| byte.is_ascii_alphanumeric())
        {
            result.push((
                char::from(byte.to_ascii_lowercase()),
                spans[index].0,
                spans[index].1,
            ));
        }
    }
    result
}
fn redact_registered(input: &str, registry: &SecretRegistry) -> String {
    let input = prefix(input, MAX_MESSAGE_BYTES);
    let normalized = normalized_units(input);
    let mut ranges = Vec::new();
    for secret in &registry.values {
        for variant in &secret.variants {
            ranges.extend(
                input
                    .match_indices(variant)
                    .map(|(start, value)| (start, start + value.len())),
            );
        }
        let size = secret.normalized.len();
        if size > 0 && size <= normalized.len() {
            for (index, window) in normalized.windows(size).enumerate() {
                if window
                    .iter()
                    .map(|unit| unit.0)
                    .eq(secret.normalized.iter().copied())
                {
                    ranges.push((normalized[index].1, window[size - 1].2));
                }
            }
        }
    }
    ranges.sort_unstable();
    let mut result = String::new();
    let mut previous = 0;
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if start < last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    for (start, end) in merged {
        result.push_str(&input[previous..start]);
        result.push_str(REDACTED);
        previous = end;
    }
    result.push_str(&input[previous..]);
    result
}
fn redact(input: &str) -> String {
    let registry = secret_registry();
    let guard = registry.lock().unwrap_or_else(|poison| poison.into_inner());
    redact_with(prefix(input, MAX_MESSAGE_BYTES), &guard, false)
}
fn redact_with(input: &str, registry: &SecretRegistry, frontend: bool) -> String {
    let registered = redact_registered(
        prefix(
            input,
            if frontend {
                MAX_FRONTEND_BYTES
            } else {
                MAX_MESSAGE_BYTES
            },
        ),
        registry,
    );
    let bytes = registered.as_bytes();
    let mut result = String::with_capacity(registered.len());
    let mut index = 0;
    while index < bytes.len() {
        if let Some((_, end)) = profile_user_segment(bytes, index) {
            result.push_str("C:\\Users\\<user>");
            if end < bytes.len() {
                result.push('\\');
                index = end + 1;
            } else {
                index = end;
            }
            continue;
        }
        if bytes[index].is_ascii_digit() {
            let end = digit_run_end(bytes, index);
            if (frontend && end - index >= 9)
                || (bytes[index..].starts_with(b"7656119") && end - index >= 17)
            {
                result.push_str(REDACTED);
                index = end;
                continue;
            }
        }
        if is_base64url(bytes[index]) {
            let end = base64url_run_end(bytes, index);
            if end - index >= if frontend { 16 } else { 40 } {
                result.push_str(REDACTED);
                index = end;
                continue;
            }
        }
        if let Some(ch) = registered[index..].chars().next() {
            result.push(ch);
            index += ch.len_utf8();
        } else {
            break;
        }
    }
    cap_redacted_output(
        result,
        if frontend {
            MAX_FRONTEND_BYTES
        } else {
            MAX_MESSAGE_BYTES
        },
    )
}
fn frontend_string_allowed(text: &str) -> bool {
    text.len() <= 40
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b" ._:/-".contains(&byte))
}
fn redact_scalar_fields(fields: Fields, registry: &SecretRegistry, frontend: bool) -> Value {
    Value::Object(
        fields
            .into_iter()
            .take(MAX_FIELDS)
            .filter_map(|(key, value)| {
                if frontend
                    && !matches!(&value, Value::Number(_) | Value::Bool(_))
                    && !matches!(&value, Value::String(text) if frontend_string_allowed(text))
                {
                    return None;
                }
                let cleaned = if sensitive_field_name(&key) {
                    Value::String(REDACTED.to_owned())
                } else {
                    match value {
                        Value::String(text) => Value::String(redact_with(
                            prefix(&text, MAX_MESSAGE_BYTES),
                            registry,
                            frontend,
                        )),
                        Value::Number(number) => {
                            let text = number.to_string();
                            let cleaned = redact_with(&text, registry, frontend);
                            if cleaned == text {
                                Value::Number(number)
                            } else {
                                Value::String(cleaned)
                            }
                        }
                        other => other,
                    }
                };
                let cleaned = match cleaned {
                    Value::String(text) => Value::String(cap_redacted_output(
                        text,
                        if frontend { 40 } else { MAX_MESSAGE_BYTES },
                    )),
                    other => other,
                };
                Some((
                    cap_redacted_output(redact_with(&key, registry, frontend), MAX_KEY_BYTES),
                    cleaned,
                ))
            })
            .collect(),
    )
}
/// Length checks only: never scan the contents of oversized renderer strings.
pub(crate) fn validate_frontend_input(
    level: &str,
    module: &str,
    msg: &str,
    fields: Option<&Value>,
) -> Result<(), &'static str> {
    if [level, module, msg].iter().any(|text| text.len() > 4096) {
        return Err("LOG_INPUT_TOO_LARGE");
    }
    let mut pending: Vec<&Value> = fields.into_iter().collect();
    let mut nodes = 0;
    while let Some(value) = pending.pop() {
        nodes += 1;
        if nodes > 1024 {
            return Err("LOG_INPUT_TOO_LARGE");
        }
        match value {
            Value::String(text) if text.len() > 4096 => return Err("LOG_INPUT_TOO_LARGE"),
            Value::Array(values) => {
                if values.len() > 1024 {
                    return Err("LOG_INPUT_TOO_LARGE");
                }
                pending.extend(values);
            }
            Value::Object(values) => {
                if values.len() > 1024 || values.keys().any(|key| key.len() > 4096) {
                    return Err("LOG_INPUT_TOO_LARGE");
                }
                pending.extend(values.values());
            }
            _ => {}
        }
        if pending.len() > 1024 {
            return Err("LOG_INPUT_TOO_LARGE");
        }
    }
    Ok(())
}

fn profile_user_segment(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    let prefix = b"c:\\users\\";
    let slash_prefix = b"c:/users/";
    let suffix = bytes.get(index..index.checked_add(prefix.len())?)?;
    if !suffix.eq_ignore_ascii_case(prefix) && !suffix.eq_ignore_ascii_case(slash_prefix) {
        return None;
    }
    let username_start = index + prefix.len();
    let username_end = bytes[username_start..]
        .iter()
        .position(|byte| *byte == b'\\' || *byte == b'/')
        .map(|offset| username_start + offset)
        .unwrap_or(bytes.len());
    (username_end > username_start).then_some((username_start, username_end))
}

fn digit_run_end(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .map(|offset| start + offset)
        .unwrap_or(bytes.len())
}

fn is_base64url(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

fn base64url_run_end(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| !is_base64url(*byte))
        .map(|offset| start + offset)
        .unwrap_or(bytes.len())
}

fn sensitive_field_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "token", "auth", "key", "secret", "password", "steamid", "account", "name",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn file_line_len(entry: &LogEntry) -> usize {
    serde_json::to_vec(&entry.file_value())
        .map_or(usize::MAX, |bytes| bytes.len().saturating_add(1))
}

fn fit_entry_to_file_limit(entry: &mut LogEntry, max_file_bytes: u64) {
    let limit = usize::try_from(max_file_bytes).unwrap_or(usize::MAX);
    if file_line_len(entry) <= limit {
        return;
    }
    entry.fields = json!({ "_truncated": true });
    for field in 0..3 {
        loop {
            if file_line_len(entry) <= limit {
                return;
            }
            let value = match field {
                0 => &mut entry.msg,
                1 => &mut entry.event,
                _ => &mut entry.module,
            };
            if value.is_empty() {
                break;
            }
            let target = value.len() / 2;
            let mut end = target;
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            value.truncate(end);
        }
    }
}

static GLOBAL: OnceLock<Arc<Logger>> = OnceLock::new();
static PANIC_HOOK: OnceLock<()> = OnceLock::new();
thread_local! { static PANIC_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
thread_local! { static SCOPED_TEST_LOGGER: std::cell::RefCell<Option<Arc<Logger>>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
fn with_test_logger<T>(logger: Arc<Logger>, action: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<Logger>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = SCOPED_TEST_LOGGER.try_with(|active| active.replace(self.0.take()));
        }
    }
    let _restore = Restore(SCOPED_TEST_LOGGER.with(|active| active.replace(Some(logger))));
    action()
}
#[cfg(test)]
pub(crate) fn collect_test_logs(action: impl FnOnce()) -> Vec<LogEntry> {
    let logger = Arc::new(Logger::new_in_memory(RING_CAPACITY));
    with_test_logger(logger.clone(), action);
    logger.entries()
}
pub fn initialize(data_dir: Option<&Path>) {
    let logger = GLOBAL.get_or_init(|| {
        let config = Logger::config(
            data_dir.map(Path::to_path_buf),
            MAX_FILE_BYTES,
            MAX_FILES,
            RING_CAPACITY,
            true,
        );
        let logger = Arc::new(Logger::start(config, secret_registry(), None));
        if std::env::var("KOHBOR_LOG").is_ok_and(|value| value.eq_ignore_ascii_case("debug")) {
            logger.set_level(Level::Debug);
        }
        logger
    });
    install_panic_hook();
    logger.flush();
}
fn panic_stderr(line: u32, column: u32) {
    let _ = writeln!(
        io::stderr(),
        "Kohbor panic: private details omitted (line {line}, column {column})."
    );
}
pub fn install_panic_hook() {
    PANIC_HOOK.get_or_init(|| {
        // The previous hook is intentionally not called: it would receive and
        // print the original unredacted payload, including in debug builds.
        std::panic::set_hook(Box::new(|info| {
            let line = info.location().map_or(0, |location| location.line());
            let column = info.location().map_or(0, |location| location.column());
            if PANIC_ACTIVE
                .try_with(|active| active.replace(true))
                .unwrap_or(true)
            {
                panic_stderr(line, column);
                return;
            }
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            let location = info
                .location()
                .map(|location| location.file())
                .unwrap_or("unknown");
            #[cfg(test)]
            let scoped = SCOPED_TEST_LOGGER
                .try_with(|active| active.try_borrow().ok().and_then(|logger| logger.clone()))
                .ok()
                .flatten();
            #[cfg(not(test))]
            let scoped: Option<Arc<Logger>> = None;
            if let Some(logger) = scoped.as_ref().or_else(|| GLOBAL.get()) {
                logger.panic_record(message, location, line, column);
            }
            panic_stderr(line, column);
            let _ = PANIC_ACTIVE.try_with(|active| active.set(false));
        }));
    });
}
/// Host code must never pass player names, OCR text, raw payloads, screenshots,
/// pack contents, or credential values. This facade performs bounded copies and
/// a nonblocking try_send only; all redaction and sink work belongs to the writer.
pub fn emit(level: Level, module: &str, event: &str, msg: &str, fields: Fields) {
    #[cfg(test)]
    if let Some(logger) = SCOPED_TEST_LOGGER.with(|active| active.borrow().clone()) {
        logger.log(level, module, event, msg, fields);
        return;
    }
    if let Some(logger) = GLOBAL.get() {
        logger.log(level, module, event, msg, fields);
    }
    // No synchronous stderr formatting or I/O on a hot caller before startup.
}
pub(crate) fn try_clip_played(event_id: &str) {
    let fields = Map::from_iter([
        (
            "event_id".to_owned(),
            Value::String(prefix(event_id, 80).to_owned()),
        ),
        ("outcome".to_owned(), Value::String("admitted".to_owned())),
    ]);
    emit(
        Level::Info,
        "audio",
        "clip_played",
        "announcer clip started",
        fields,
    );
}
pub fn emit_frontend(level: Level, module: &str, event: &str, msg: &str, fields: Fields) -> bool {
    GLOBAL.get().is_some_and(|logger| {
        logger.log_frontend_at(Instant::now(), level, module, event, msg, fields)
    })
}
pub fn tail(since_seq: Option<u64>, limit: Option<u32>) -> Value {
    GLOBAL
        .get()
        .map(|logger| logger.tail_value(since_seq, limit))
        .unwrap_or_else(|| json!({"entries": [], "last_seq": 0, "dropped": false}))
}
pub fn set_level(value: &str) -> Result<(), &'static str> {
    let level = Level::parse(value)?;
    GLOBAL
        .get()
        .ok_or("LOGGER_UNAVAILABLE")
        .map(|logger| logger.set_level(level))
}
pub fn log_dir() -> String {
    GLOBAL
        .get()
        .map(|logger| redact(&logger.log_dir_value()))
        .unwrap_or_default()
}
pub fn open_log_dir() -> Result<(), String> {
    let dir = GLOBAL
        .get()
        .map(|logger| logger.log_dir_value())
        .filter(|dir| !dir.is_empty())
        .ok_or("LOG_DIRECTORY_UNAVAILABLE")?;
    check_logs_dir(Path::new(&dir)).map_err(|_| "LOG_DIRECTORY_UNAVAILABLE".to_owned())?;
    Command::new("explorer.exe")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|_| "LOG_FOLDER_OPEN_FAILED".to_owned())
}
pub fn flush() {
    if let Some(logger) = GLOBAL.get() {
        logger.flush();
    }
}
#[macro_export]
macro_rules! __kohbor_log {
    ($level:expr, $module:expr, $event:expr, $message:expr $(, $key:ident = $value:expr)* $(,)?) => {{
        let fields = $crate::logging::Fields::from_iter([
            $((stringify!($key).to_owned(), serde_json::json!($value)),)*
        ]);
        $crate::logging::emit($level, $module, $event, &$message, fields);
    }};
}

#[macro_export]
macro_rules! log_error {
    ($module:expr, $event:expr, $message:expr $(, $key:ident = $value:expr)* $(,)?) => {
        $crate::__kohbor_log!($crate::logging::Level::Error, $module, $event, $message $(, $key = $value)*)
    };
}

#[macro_export]
macro_rules! log_warn {
    ($module:expr, $event:expr, $message:expr $(, $key:ident = $value:expr)* $(,)?) => {
        $crate::__kohbor_log!($crate::logging::Level::Warn, $module, $event, $message $(, $key = $value)*)
    };
}

#[macro_export]
macro_rules! log_info {
    ($module:expr, $event:expr, $message:expr $(, $key:ident = $value:expr)* $(,)?) => {
        $crate::__kohbor_log!($crate::logging::Level::Info, $module, $event, $message $(, $key = $value)*)
    };
}

#[macro_export]
macro_rules! log_debug {
    ($module:expr, $event:expr, $message:expr $(, $key:ident = $value:expr)* $(,)?) => {
        $crate::__kohbor_log!($crate::logging::Level::Debug, $module, $event, $message $(, $key = $value)*)
    };
}

#[cfg(test)]
mod tests {
    use super::{redact, Fields, Level, Logger};
    use serde_json::{json, Map, Value};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    fn temp_dir(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kohbor-logging-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fields(entries: &[(&str, Value)]) -> Fields {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect::<Map<_, _>>()
    }

    fn log(logger: &Logger, level: Level, message: &str, fields: Fields) {
        logger.log(level, "test", "event", message, fields);
    }

    fn file_names(dir: &Path) -> Vec<String> {
        fs::read_dir(dir.join("logs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn token(length: usize) -> String {
        "A".repeat(length)
    }

    #[test]
    fn redacts_the_44_character_token_shape() {
        assert_eq!(redact(&format!("token={}", token(44))), "token=[redacted]");
    }

    #[test]
    fn redacts_the_app_43_character_gsi_token_shape() {
        assert_eq!(redact(&token(43)), "[redacted]");
    }

    #[test]
    fn redacts_any_40_character_base64url_run() {
        assert_eq!(redact(&format!("x-{}-y", token(40))), "[redacted]");
    }

    #[test]
    fn redaction_preserves_punctuation_outside_a_token_run() {
        assert_eq!(redact(&format!("x={} ;y", token(40))), "x=[redacted] ;y");
    }

    #[test]
    fn redacts_a_steam_id_in_text() {
        assert_eq!(
            redact("owner 76561197960265728 changed"),
            "owner [redacted] changed"
        );
    }

    #[test]
    fn redacts_a_steam_id_at_the_start_of_a_long_digit_run() {
        assert_eq!(redact("765611979602657281234"), "[redacted]");
    }

    #[test]
    fn redacts_token_field_values_by_key() {
        let logger = Logger::new_in_memory(16);
        log(
            &logger,
            Level::Info,
            "safe",
            fields(&[("access_token", json!("visible"))]),
        );
        assert_eq!(logger.entries()[0].fields["access_token"], "[redacted]");
    }

    #[test]
    fn redacts_auth_field_values_by_key() {
        let logger = Logger::new_in_memory(16);
        log(
            &logger,
            Level::Info,
            "safe",
            fields(&[("auth_header", json!("visible"))]),
        );
        assert_eq!(logger.entries()[0].fields["auth_header"], "[redacted]");
    }

    #[test]
    fn redacts_key_field_values_by_key() {
        let logger = Logger::new_in_memory(16);
        log(
            &logger,
            Level::Info,
            "safe",
            fields(&[("install_key", json!("visible"))]),
        );
        assert_eq!(logger.entries()[0].fields["install_key"], "[redacted]");
    }

    #[test]
    fn redacts_secret_field_values_by_key() {
        let logger = Logger::new_in_memory(16);
        log(
            &logger,
            Level::Info,
            "safe",
            fields(&[("secret_note", json!("visible"))]),
        );
        assert_eq!(logger.entries()[0].fields["secret_note"], "[redacted]");
    }

    #[test]
    fn redacts_password_field_values_by_key() {
        let logger = Logger::new_in_memory(16);
        log(
            &logger,
            Level::Info,
            "safe",
            fields(&[("password_hint", json!("visible"))]),
        );
        assert_eq!(logger.entries()[0].fields["password_hint"], "[redacted]");
    }

    #[test]
    fn redacts_steamid_field_values_by_key() {
        let logger = Logger::new_in_memory(16);
        log(
            &logger,
            Level::Info,
            "safe",
            fields(&[("steamid64", json!(76561197960265728u64))]),
        );
        assert_eq!(logger.entries()[0].fields["steamid64"], "[redacted]");
    }

    #[test]
    fn redacts_name_field_values_by_key() {
        let logger = Logger::new_in_memory(16);
        log(
            &logger,
            Level::Info,
            "safe",
            fields(&[("player_name", json!("private"))]),
        );
        assert_eq!(logger.entries()[0].fields["player_name"], "[redacted]");
    }

    #[test]
    fn redacts_account_identifier_field_values_by_key() {
        let logger = Logger::new_in_memory(16);
        log(
            &logger,
            Level::Info,
            "safe",
            fields(&[("account_id", json!(12345))]),
        );
        assert_eq!(logger.entries()[0].fields["account_id"], "[redacted]");
    }

    #[test]
    fn shortens_the_windows_user_profile_segment() {
        assert_eq!(
            redact(r"failed at C:\Users\alice\AppData\Local\Kohbor"),
            r"failed at C:\Users\<user>\AppData\Local\Kohbor"
        );
    }

    #[test]
    fn host_payload_trees_are_dropped_before_queueing() {
        let logger = Logger::new_in_memory(16);
        log(
            &logger,
            Level::Info,
            "safe",
            fields(&[
                ("nested", json!({"caption": "76561197960265728", "safe": 3})),
                ("count", json!(3)),
            ]),
        );
        assert!(logger.entries()[0].fields.get("nested").is_none());
        assert_eq!(logger.entries()[0].fields["count"], 3);
    }

    #[test]
    fn file_line_has_the_required_json_shape() {
        let root = temp_dir("json-shape");
        let logger = Logger::new_with_limits(Some(root), 4096, 5, 16);
        log(
            &logger,
            Level::Info,
            "ready",
            fields(&[("count", json!(2))]),
        );
        logger.flush();
        let line = fs::read_to_string(logger.current_file().unwrap()).unwrap();
        let entry: Value = serde_json::from_str(line.trim()).unwrap();
        let keys: Vec<_> = entry
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["commit", "event", "fields", "level", "module", "msg", "ts", "v"]
        );
        let timestamp = entry["ts"].as_str().unwrap();
        assert_eq!(timestamp.len(), 24);
        assert_eq!(&timestamp[10..11], "T");
        assert_eq!(&timestamp[19..20], ".");
        assert_eq!(&timestamp[23..24], "Z");
    }

    #[test]
    fn file_line_includes_version_and_nullable_commit() {
        let root = temp_dir("metadata");
        let logger = Logger::new_with_limits(Some(root), 4096, 5, 16);
        log(&logger, Level::Info, "ready", Fields::new());
        logger.flush();
        let entry: Value = serde_json::from_str(
            fs::read_to_string(logger.current_file().unwrap())
                .unwrap()
                .trim(),
        )
        .unwrap();
        assert!(entry["v"].as_str().is_some());
        assert!(entry["commit"].is_string() || entry["commit"].is_null());
    }

    #[test]
    fn defaults_to_info_level() {
        let logger = Logger::new_in_memory(16);
        log(&logger, Level::Debug, "hidden", Fields::new());
        log(&logger, Level::Info, "visible", Fields::new());
        assert_eq!(logger.entries().len(), 1);
        assert_eq!(logger.entries()[0].msg, "visible");
    }

    #[test]
    fn runtime_debug_level_enables_debug_entries() {
        let logger = Logger::new_in_memory(16);
        logger.set_level(Level::Debug);
        log(&logger, Level::Debug, "visible", Fields::new());
        assert_eq!(logger.entries().len(), 1);
    }

    #[test]
    fn ring_sequence_is_monotonic() {
        let logger = Logger::new_in_memory(16);
        for _ in 0..4 {
            log(&logger, Level::Info, "x", Fields::new());
        }
        assert_eq!(
            logger
                .entries()
                .iter()
                .map(|entry| entry.seq)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
    }

    #[test]
    fn ring_wraps_at_its_capacity() {
        let logger = Logger::new_in_memory(3);
        for index in 0..5 {
            log(&logger, Level::Info, &index.to_string(), Fields::new());
        }
        assert_eq!(
            logger
                .entries()
                .iter()
                .map(|entry| entry.seq)
                .collect::<Vec<_>>(),
            [3, 4, 5]
        );
    }

    #[test]
    fn tail_filters_entries_after_the_requested_sequence() {
        let logger = Logger::new_in_memory(16);
        for _ in 0..3 {
            log(&logger, Level::Info, "x", Fields::new());
        }
        assert_eq!(logger.tail_value(Some(1), Some(1))["entries"][0]["seq"], 2);
    }

    #[test]
    fn limited_tail_cursor_does_not_skip_unread_entries() {
        let logger = Logger::new_in_memory(16);
        for _ in 0..3 {
            log(&logger, Level::Info, "x", Fields::new());
        }
        let first_page = logger.tail_value(Some(1), Some(1));
        assert_eq!(first_page["last_seq"], 2);
        let next_page = logger.tail_value(first_page["last_seq"].as_u64(), Some(1));
        assert_eq!(next_page["entries"][0]["seq"], 3);
        assert_eq!(next_page["last_seq"], 3);
    }

    #[test]
    fn initial_tail_shows_the_newest_entries() {
        let logger = Logger::new_in_memory(16);
        for _ in 0..3 {
            log(&logger, Level::Info, "x", Fields::new());
        }
        let value = logger.tail_value(None, Some(1));
        assert_eq!(value["entries"][0]["seq"], 3);
        assert_eq!(value["last_seq"], 3);
    }

    #[test]
    fn tail_reports_dropped_entries_after_ring_wrap() {
        let logger = Logger::new_in_memory(2);
        for _ in 0..3 {
            log(&logger, Level::Info, "x", Fields::new());
        }
        let value = logger.tail_value(Some(0), Some(10));
        assert_eq!(value["dropped"], true);
        assert_eq!(value["last_seq"], 3);
    }

    #[test]
    fn rolls_before_a_line_crosses_the_configured_size_boundary() {
        let root = temp_dir("roll-boundary");
        let logger = Logger::new_with_limits(Some(root), 512, 5, 16);
        log(&logger, Level::Info, &"x ".repeat(160), Fields::new());
        logger.flush();
        let first = logger.current_file().unwrap();
        assert!(fs::metadata(&first).unwrap().len() <= 512);
        log(&logger, Level::Info, &"y ".repeat(160), Fields::new());
        logger.flush();
        assert_ne!(logger.current_file().unwrap(), first);
    }

    #[test]
    fn oversized_records_remain_valid_json_inside_the_file_cap() {
        let root = temp_dir("oversized-record");
        let logger = Logger::new_with_limits(Some(root), 512, 5, 16);
        log(
            &logger,
            Level::Error,
            &"x ".repeat(5000),
            fields(&[("details", json!("y ".repeat(5000)))]),
        );
        let contents = fs::read_to_string(logger.current_file().unwrap()).unwrap();
        assert!(contents.len() <= 512);
        assert!(serde_json::from_str::<Value>(contents.trim()).is_ok());
        assert!(!logger.is_degraded());
    }

    #[test]
    fn retention_keeps_at_most_five_matching_log_files() {
        let root = temp_dir("retention");
        let logs = root.join("logs");
        fs::create_dir_all(&logs).unwrap();
        for second in 0..7 {
            fs::write(
                logs.join(format!("kohbor-20200101-0000{second:02}.log")),
                "old\n",
            )
            .unwrap();
        }
        let _logger = Logger::new_with_limits(Some(root.clone()), 4096, 5, 16);
        assert!(_logger.current_file().is_some());
        let names = file_names(&root);
        assert_eq!(
            names
                .iter()
                .filter(|name| name.starts_with("kohbor-"))
                .count(),
            5
        );
        for second in 0..3 {
            assert!(!names.contains(&format!("kohbor-20200101-0000{second:02}.log")));
        }
    }

    #[test]
    fn retention_preserves_unrelated_files_in_logs_folder() {
        let root = temp_dir("retention-unrelated");
        let logs = root.join("logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join("notes.txt"), "keep").unwrap();
        for second in 0..7 {
            fs::write(
                logs.join(format!("kohbor-20200101-0000{second:02}.log")),
                "old\n",
            )
            .unwrap();
        }
        let _logger = Logger::new_with_limits(Some(root.clone()), 4096, 5, 16);
        assert!(_logger.current_file().is_some());
        assert_eq!(fs::read_to_string(logs.join("notes.txt")).unwrap(), "keep");
    }

    #[test]
    fn retention_does_not_delete_near_match_filenames() {
        let root = temp_dir("retention-pattern");
        let logs = root.join("logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join("kohbor-old.log"), "keep").unwrap();
        for second in 0..7 {
            fs::write(
                logs.join(format!("kohbor-20200101-0000{second:02}.log")),
                "old\n",
            )
            .unwrap();
        }
        let _logger = Logger::new_with_limits(Some(root), 4096, 5, 16);
        assert!(_logger.current_file().is_some());
        assert_eq!(
            fs::read_to_string(logs.join("kohbor-old.log")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn uncreatable_logs_directory_keeps_an_in_memory_ring() {
        let root = temp_dir("degraded").join("file-blocker");
        fs::write(&root, "not a directory").unwrap();
        let logger = Logger::new_with_limits(Some(root), 4096, 5, 16);
        log(&logger, Level::Info, "still available", Fields::new());
        assert!(logger.is_degraded());
        assert_eq!(logger.entries().len(), 2);
    }

    #[test]
    fn storage_degradation_is_reported_once() {
        let root = temp_dir("degraded-once").join("file-blocker");
        fs::write(&root, "not a directory").unwrap();
        let logger = Logger::new_with_limits(Some(root), 4096, 5, 16);
        assert_eq!(
            logger
                .entries()
                .iter()
                .filter(|entry| entry.event == "log_storage_unavailable")
                .count(),
            1
        );
    }

    #[test]
    fn frontend_rate_limiter_accepts_twenty_calls_per_second() {
        let logger = Logger::new_in_memory(64);
        let start = Instant::now();
        let mut accepted = 0;
        for _ in 0..21 {
            accepted += usize::from(logger.log_frontend_at(
                start,
                Level::Info,
                "renderer",
                "error",
                "message",
                Fields::new(),
            ));
        }
        assert_eq!(accepted, 20);
    }

    #[test]
    fn frontend_rate_limiter_emits_one_warning_for_excess_calls() {
        let logger = Logger::new_in_memory(64);
        let start = Instant::now();
        for _ in 0..24 {
            logger.log_frontend_at(
                start,
                Level::Info,
                "renderer",
                "error",
                "message",
                Fields::new(),
            );
        }
        assert_eq!(
            logger
                .entries()
                .iter()
                .filter(|entry| entry.event == "frontend_rate_limited")
                .count(),
            1
        );
    }

    #[test]
    fn frontend_rate_limiter_resets_on_the_next_second() {
        let logger = Logger::new_in_memory(64);
        let start = Instant::now();
        for _ in 0..20 {
            assert!(logger.log_frontend_at(
                start,
                Level::Info,
                "renderer",
                "error",
                "message",
                Fields::new()
            ));
        }
        assert!(logger.log_frontend_at(
            start + Duration::from_secs(1),
            Level::Info,
            "renderer",
            "error",
            "message",
            Fields::new(),
        ));
    }

    #[test]
    fn tail_command_shape_contains_entries_last_sequence_and_drop_flag() {
        let logger = Logger::new_in_memory(16);
        log(&logger, Level::Info, "ready", Fields::new());
        let value = logger.tail_value(None, None);
        assert!(value["entries"].is_array());
        assert!(value["last_seq"].is_u64());
        assert!(value["dropped"].is_boolean());
    }

    #[test]
    fn tail_entries_include_sequence_and_all_display_fields() {
        let logger = Logger::new_in_memory(16);
        log(&logger, Level::Warn, "stale", Fields::new());
        let value = logger.tail_value(None, None);
        let entry = &value["entries"][0];
        for key in [
            "seq", "ts", "level", "module", "event", "msg", "fields", "v", "commit",
        ] {
            assert!(entry.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn log_dir_command_value_is_a_path_string() {
        let root = temp_dir("log-dir-shape");
        let logger = Logger::new_with_limits(Some(root.clone()), 4096, 5, 16);
        assert_eq!(
            logger.log_dir_value(),
            root.join("logs").to_string_lossy().into_owned()
        );
    }

    #[test]
    fn panic_hook_records_a_redacted_message_and_location() {
        let root = temp_dir("panic-hook");
        super::initialize(Some(&root));
        super::install_panic_hook();
        let secret = token(44);
        let logger = std::sync::Arc::new(Logger::new_in_memory(16));
        let result = super::with_test_logger(logger.clone(), || {
            std::panic::catch_unwind(|| panic!("logging panic hook probe {secret}"))
        });
        assert!(result.is_err());
        let entries = logger.entries();
        let entry = entries
            .iter()
            .find(|entry| {
                entry.event == "panic" && entry.msg.starts_with("logging panic hook probe")
            })
            .unwrap();
        assert!(!entry.msg.contains(&secret));
        assert!(entry.fields.get("location").is_some());
        assert!(entry.fields["line"].as_u64().is_some_and(|line| line > 0));
    }

    #[test]
    fn adversarial_inputs_are_redacted_in_file_and_ring() {
        let root = temp_dir("adversarial");
        let logger = Logger::new_with_limits(Some(root), 4096, 5, 16);
        let secret = token(44);
        let long_token = token(48);
        let message = format!("{secret} 76561197960265728 {long_token}");
        log(
            &logger,
            Level::Error,
            &message,
            fields(&[("token", json!(secret)), ("note", json!(token(40)))]),
        );
        logger.flush();
        let ring = serde_json::to_string(&logger.entries()).unwrap();
        let file = fs::read_to_string(logger.current_file().unwrap()).unwrap();
        for raw in [&secret, "76561197960265728", &long_token, &token(40)] {
            assert!(!ring.contains(raw));
            assert!(!file.contains(raw));
        }
    }

    #[test]
    fn filename_matcher_accepts_only_the_required_log_pattern() {
        assert!(super::is_log_filename("kohbor-20261114-123456.log"));
        assert!(!super::is_log_filename("kohbor-2026-11-14-123456.log"));
        assert!(!super::is_log_filename("kohbor-20261114-123456.log.bak"));
        assert!(!super::is_log_filename("kohbor-old.log"));
    }
}
