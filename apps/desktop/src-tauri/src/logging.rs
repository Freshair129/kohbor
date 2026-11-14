use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
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
const MAX_FIELD_NODES: usize = 256;
const MAX_FIELD_DEPTH: usize = 8;
const MAX_FIELD_STRING_CHARS: usize = 256;
const MAX_FIELD_KEY_CHARS: usize = 128;
const MAX_MESSAGE_CHARS: usize = 4096;

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
    requested_logs_dir: Option<PathBuf>,
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

#[derive(Default)]
struct FrontendRateState {
    window_started: Option<Instant>,
    accepted: u32,
    warning_emitted: bool,
}

pub struct Logger {
    state: Mutex<LoggerState>,
    level: AtomicU8,
    flush_each_record: AtomicU8,
    max_file_bytes: u64,
    max_files: usize,
    ring_capacity: usize,
    frontend_rate: Mutex<FrontendRateState>,
}

impl Logger {
    fn with_limits(
        data_dir: Option<PathBuf>,
        max_file_bytes: u64,
        max_files: usize,
        ring_capacity: usize,
        report_degradation: bool,
    ) -> Self {
        let requested_logs_dir = data_dir.map(|dir| dir.join("logs"));
        let mut logger = Self {
            state: Mutex::new(LoggerState {
                requested_logs_dir: requested_logs_dir.clone(),
                logs_dir: None,
                current_file: None,
                writer: None,
                file_len: 0,
                ring: VecDeque::with_capacity(ring_capacity.max(1)),
                next_seq: 0,
                last_flush: Instant::now(),
                degraded: requested_logs_dir.is_none(),
                degradation_reported: false,
            }),
            level: AtomicU8::new(Level::Info.as_u8()),
            flush_each_record: AtomicU8::new(0),
            max_file_bytes: max_file_bytes.max(512),
            max_files: max_files.max(1),
            ring_capacity: ring_capacity.max(1),
            frontend_rate: Mutex::new(FrontendRateState::default()),
        };

        let setup_result = requested_logs_dir.as_ref().map_or_else(
            || {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "data directory unavailable",
                ))
            },
            |logs_dir| {
                fs::create_dir_all(logs_dir).and_then(|()| {
                    let mut state = logger
                        .state
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner());
                    state.logs_dir = Some(logs_dir.clone());
                    Self::open_new_file(&mut state, logger.max_files)
                })
            },
        );
        if setup_result.is_err() {
            logger.mark_degraded(report_degradation);
        }
        logger
    }

    #[cfg(test)]
    fn new_with_limits(
        data_dir: Option<PathBuf>,
        max_file_bytes: u64,
        max_files: usize,
        ring_capacity: usize,
    ) -> Self {
        Self::with_limits(data_dir, max_file_bytes, max_files, ring_capacity, false)
    }

    #[cfg(test)]
    fn new_in_memory(ring_capacity: usize) -> Self {
        let logger = Self::with_limits(None, MAX_FILE_BYTES, MAX_FILES, ring_capacity, false);
        let mut state = logger
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.ring.clear();
        state.next_seq = 0;
        state.degradation_reported = true;
        drop(state);
        logger
    }

    fn mark_degraded(&mut self, report_stderr: bool) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.degraded = true;
        state.logs_dir = None;
        state.current_file = None;
        state.writer = None;
        if !state.degradation_reported {
            state.degradation_reported = true;
            if report_stderr {
                eprintln!("Kohbor log storage unavailable; keeping the in-memory log ring only.");
            }
            self.append_locked(
                &mut state,
                Level::Warn,
                "logging",
                "log_storage_unavailable",
                "log directory unavailable; using the in-memory ring",
                Fields::new(),
            );
        }
    }

    fn open_new_file(state: &mut LoggerState, max_files: usize) -> io::Result<()> {
        let logs_dir = state
            .logs_dir
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "logs directory unavailable"))?;
        let now = SystemTime::now();
        let mut seconds = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let (mut file, path) = loop {
            let name = log_filename_at(seconds);
            let path = logs_dir.join(name);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => break (file, path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    seconds = seconds.saturating_add(1);
                }
                Err(error) => return Err(error),
            }
        };
        file.flush()?;
        state.file_len = 0;
        state.current_file = Some(path);
        state.writer = Some(BufWriter::new(file));
        retain_logs(logs_dir, max_files, state.current_file.as_deref())?;
        state.last_flush = Instant::now();
        Ok(())
    }

    fn write_line(&self, state: &mut LoggerState, line: &[u8]) -> io::Result<()> {
        if state.logs_dir.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "logs directory unavailable",
            ));
        }
        let should_roll = state.writer.is_none()
            || (state.file_len > 0
                && state.file_len.saturating_add(line.len() as u64) > self.max_file_bytes);
        if should_roll {
            if let Some(writer) = state.writer.as_mut() {
                writer.flush()?;
            }
            Self::open_new_file(state, self.max_files)?;
        }
        if line.len() as u64 > self.max_file_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "log record exceeds the file size limit",
            ));
        }
        let writer = state
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "log file unavailable"))?;
        writer.write_all(line)?;
        state.file_len = state.file_len.saturating_add(line.len() as u64);
        Ok(())
    }

    fn append_locked(
        &self,
        state: &mut LoggerState,
        level: Level,
        module: &str,
        event: &str,
        msg: &str,
        fields: Fields,
    ) -> LogEntry {
        state.next_seq = state.next_seq.saturating_add(1);
        let app_version = crate::version::AppVersion::current();
        let mut entry = LogEntry {
            seq: state.next_seq,
            ts: utc_timestamp(SystemTime::now()),
            level,
            module: redact_and_limit(module, 128),
            event: redact_and_limit(event, 128),
            msg: redact_and_limit(msg, MAX_MESSAGE_CHARS),
            fields: redact_fields(fields),
            v: app_version.version.to_owned(),
            commit: app_version
                .git_commit
                .map(|commit| commit.chars().take(7).collect()),
        };
        fit_entry_to_file_limit(&mut entry, self.max_file_bytes);
        state.ring.push_back(entry.clone());
        while state.ring.len() > self.ring_capacity {
            state.ring.pop_front();
        }
        entry
    }

    pub fn log(&self, level: Level, module: &str, event: &str, msg: &str, fields: Fields) {
        if level.as_u8() > self.level.load(Ordering::Relaxed) {
            return;
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let entry = self.append_locked(&mut state, level, module, event, msg, fields);
        if !state.degraded {
            let mut bytes = match serde_json::to_vec(&entry.file_value()) {
                Ok(bytes) => bytes,
                Err(_) => return,
            };
            bytes.push(b'\n');
            if let Err(_error) = self.write_line(&mut state, &bytes) {
                self.degrade_locked(&mut state, true);
                self.append_locked(
                    &mut state,
                    Level::Warn,
                    "logging",
                    "log_storage_unavailable",
                    "log write failed; using the in-memory ring",
                    Fields::new(),
                );
                return;
            }
        }
        if matches!(level, Level::Error | Level::Warn) {
            if let Some(writer) = state.writer.as_mut() {
                if writer.flush().is_err() {
                    self.degrade_locked(&mut state, true);
                    self.append_locked(
                        &mut state,
                        Level::Warn,
                        "logging",
                        "log_storage_unavailable",
                        "log flush failed; using the in-memory ring",
                        Fields::new(),
                    );
                    return;
                }
                state.last_flush = Instant::now();
            }
        } else if self.flush_each_record.load(Ordering::Relaxed) != 0
            || state.last_flush.elapsed() >= Duration::from_secs(1)
        {
            if let Some(writer) = state.writer.as_mut() {
                if writer.flush().is_err() {
                    self.degrade_locked(&mut state, true);
                    return;
                }
            }
            state.last_flush = Instant::now();
        }
    }

    fn degrade_locked(&self, state: &mut LoggerState, report_stderr: bool) {
        state.degraded = true;
        state.logs_dir = None;
        state.current_file = None;
        state.writer = None;
        if !state.degradation_reported {
            state.degradation_reported = true;
            if report_stderr {
                eprintln!("Kohbor log storage unavailable; keeping the in-memory log ring only.");
            }
        }
    }

    pub fn flush(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(writer) = state.writer.as_mut() {
            if writer.flush().is_err() {
                self.degrade_locked(&mut state, true);
            }
        }
        state.last_flush = Instant::now();
    }

    fn flush_if_due(&self) {
        let due = {
            let state = self
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.last_flush.elapsed() >= Duration::from_secs(1)
        };
        if due {
            self.flush();
        }
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .ring
            .iter()
            .cloned()
            .collect()
    }

    pub fn tail_value(&self, since_seq: Option<u64>, limit: Option<u32>) -> Value {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let limit = limit
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_TAIL_LIMIT)
            .clamp(1, MAX_TAIL_LIMIT);
        let dropped = since_seq.is_some_and(|since| {
            state
                .ring
                .front()
                .is_some_and(|first| first.seq > since.saturating_add(1))
        });
        let matching: Vec<&LogEntry> = state
            .ring
            .iter()
            .filter(|entry| since_seq.is_none_or(|since| entry.seq > since))
            .collect();
        // Initial reads show the newest entries. A supplied cursor pages forward
        // without skipping unread entries when the result reaches its limit.
        let start = if since_seq.is_some() {
            0
        } else {
            matching.len().saturating_sub(limit)
        };
        let selected = &matching[start..matching.len().min(start.saturating_add(limit))];
        let last_seq = selected.last().map_or(state.next_seq, |entry| entry.seq);
        let entries: Vec<Value> = selected.iter().map(|entry| entry.tail_value()).collect();
        json!({ "entries": entries, "last_seq": last_seq, "dropped": dropped })
    }

    pub fn set_level(&self, level: Level) {
        self.level.store(level.as_u8(), Ordering::Relaxed);
    }

    pub fn is_degraded(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .degraded
    }

    pub fn current_file(&self) -> Option<PathBuf> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .current_file
            .clone()
    }

    pub fn log_dir_value(&self) -> String {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .requested_logs_dir
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
        let warn = {
            let mut rate = self
                .frontend_rate
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if rate.window_started.is_none_or(|started| {
                now.saturating_duration_since(started) >= Duration::from_secs(1)
            }) {
                rate.window_started = Some(now);
                rate.accepted = 0;
                rate.warning_emitted = false;
            }
            if rate.accepted < 20 {
                rate.accepted += 1;
                self.log(level, module, event, msg, fields);
                return true;
            }
            if rate.warning_emitted {
                false
            } else {
                rate.warning_emitted = true;
                true
            }
        };
        if warn {
            self.log(
                Level::Warn,
                "logging",
                "frontend_rate_limited",
                "renderer log rate limit exceeded",
                Fields::new(),
            );
        }
        false
    }
}

fn retain_logs(logs_dir: &Path, max_files: usize, current_file: Option<&Path>) -> io::Result<()> {
    let mut files: Vec<PathBuf> = fs::read_dir(logs_dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() || !is_log_filename(&entry.file_name().to_string_lossy()) {
                return None;
            }
            Some(entry.path())
        })
        .collect();
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    while files.len() > max_files {
        let index = files
            .iter()
            .position(|path| Some(path.as_path()) != current_file)
            .unwrap_or(0);
        let oldest = files.remove(index);
        fs::remove_file(oldest)?;
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

fn redact(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        if let Some((_, username_end)) = profile_user_segment(bytes, index) {
            output.push_str("C:\\Users\\<user>");
            if username_end < bytes.len() {
                output.push('\\');
                index = username_end + 1;
            } else {
                index = username_end;
            }
            continue;
        }
        if bytes[index..].starts_with(b"7656119") && bytes[index].is_ascii_digit() {
            let end = digit_run_end(bytes, index);
            if end.saturating_sub(index) >= 17 {
                output.push_str(REDACTED);
                index = end;
                continue;
            }
        }
        if is_base64url(bytes[index]) {
            let end = base64url_run_end(bytes, index);
            if end.saturating_sub(index) >= 40 {
                output.push_str(REDACTED);
                index = end;
                continue;
            }
        }
        let ch = input[index..].chars().next().expect("valid UTF-8 boundary");
        output.push(ch);
        index += ch.len_utf8();
    }
    output
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

fn redact_fields(fields: Fields) -> Value {
    let mut budget = MAX_FIELD_NODES;
    redact_fields_with_budget(fields, &mut budget, 0)
}

fn redact_fields_with_budget(fields: Fields, budget: &mut usize, depth: usize) -> Value {
    let mut redacted = Map::new();
    for (key, value) in fields {
        if *budget == 0 {
            redacted.insert("_truncated".to_owned(), Value::Bool(true));
            break;
        }
        *budget -= 1;
        let sensitive = sensitive_field_name(&key);
        let key = redact_and_limit(&key, MAX_FIELD_KEY_CHARS);
        let value = if sensitive {
            Value::String(REDACTED.to_owned())
        } else {
            redact_value(value, budget, depth + 1)
        };
        redacted.insert(key, value);
    }
    Value::Object(redacted)
}

fn redact_value(value: Value, budget: &mut usize, depth: usize) -> Value {
    if *budget == 0 || depth >= MAX_FIELD_DEPTH {
        return Value::String("[truncated]".to_owned());
    }
    *budget -= 1;
    match value {
        Value::String(text) => Value::String(redact_and_limit(&text, MAX_FIELD_STRING_CHARS)),
        Value::Array(values) => {
            let mut redacted = Vec::new();
            for value in values {
                if *budget == 0 {
                    redacted.push(Value::String("[truncated]".to_owned()));
                    break;
                }
                redacted.push(redact_value(value, budget, depth + 1));
            }
            Value::Array(redacted)
        }
        Value::Object(values) => {
            let fields: Fields = values.into_iter().collect();
            redact_fields_with_budget(fields, budget, depth + 1)
        }
        Value::Number(number) => {
            let text = number.to_string();
            let cleaned = redact(&text);
            if cleaned != text {
                Value::String(cleaned)
            } else {
                Value::Number(number)
            }
        }
        other => other,
    }
}

fn redact_and_limit(input: &str, max_chars: usize) -> String {
    let mut redacted = redact(input);
    if redacted.chars().count() > max_chars {
        let mut end = redacted
            .char_indices()
            .nth(max_chars)
            .map_or(redacted.len(), |(index, _)| index);
        while !redacted.is_char_boundary(end) {
            end -= 1;
        }
        redacted.truncate(end);
    }
    redacted
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

#[cfg(test)]
thread_local! {
    static SCOPED_TEST_LOGGER: std::cell::RefCell<Option<Arc<Logger>>> = const {
        std::cell::RefCell::new(None)
    };
}

/// Isolate fake-worker logging from other concurrently running unit tests.
#[cfg(test)]
pub(crate) fn collect_test_logs(action: impl FnOnce()) -> Vec<LogEntry> {
    struct Restore(Option<Arc<Logger>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SCOPED_TEST_LOGGER.with(|active| active.replace(self.0.take()));
        }
    }
    let logger = Arc::new(Logger::new_in_memory(RING_CAPACITY));
    let _restore = Restore(SCOPED_TEST_LOGGER.with(|active| active.replace(Some(logger.clone()))));
    action();
    logger.entries()
}

pub fn initialize(data_dir: Option<&Path>) {
    let logger = GLOBAL.get_or_init(|| {
        let logger = Arc::new(Logger::with_limits(
            data_dir.map(Path::to_path_buf),
            MAX_FILE_BYTES,
            MAX_FILES,
            RING_CAPACITY,
            true,
        ));
        if std::env::var("KOHBOR_LOG").is_ok_and(|value| value.eq_ignore_ascii_case("debug")) {
            logger.set_level(Level::Debug);
        }
        start_flush_thread(&logger);
        logger
    });
    install_panic_hook();
    let _ = logger;
}

fn start_flush_thread(logger: &Arc<Logger>) {
    let weak = Arc::downgrade(logger);
    if let Err(error) = std::thread::Builder::new()
        .name("log-flusher".to_owned())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let Some(logger) = weak.upgrade() else {
                break;
            };
            logger.flush_if_due();
        })
    {
        logger.flush_each_record.store(1, Ordering::Relaxed);
        logger.log(
            Level::Warn,
            "logging",
            "log_flusher_unavailable",
            &format!(
                "periodic log flush worker unavailable: {}",
                redact(&error.to_string())
            ),
            Fields::new(),
        );
    }
}

pub fn install_panic_hook() {
    PANIC_HOOK.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            let location = info.location().map_or_else(
                || "unknown".to_owned(),
                |location| {
                    format!(
                        "{}:{}:{}",
                        location.file(),
                        location.line(),
                        location.column()
                    )
                },
            );
            let mut fields = Fields::new();
            fields.insert("location".to_owned(), Value::String(location));
            emit(Level::Error, "panic", "panic", message, fields);
            flush();
            previous(info);
        }));
    });
}

pub fn emit(level: Level, module: &str, event: &str, msg: &str, fields: Fields) {
    #[cfg(test)]
    if let Some(logger) = SCOPED_TEST_LOGGER.with(|active| active.borrow().clone()) {
        logger.log(level, module, event, msg, fields);
        return;
    }
    if let Some(logger) = GLOBAL.get() {
        logger.log(level, module, event, msg, fields);
    } else {
        eprintln!(
            "[{}] {} {}: {}",
            level.as_str(),
            redact(module),
            redact(event),
            redact(msg)
        );
    }
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
        .unwrap_or_else(|| json!({ "entries": [], "last_seq": 0, "dropped": false }))
}

pub fn set_level(value: &str) -> Result<(), &'static str> {
    let level = Level::parse(value)?;
    if let Some(logger) = GLOBAL.get() {
        logger.set_level(level);
        Ok(())
    } else {
        Err("LOGGER_UNAVAILABLE")
    }
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
        .map_or_else(String::new, |logger| logger.log_dir_value());
    if dir.is_empty() {
        return Err("LOG_DIRECTORY_UNAVAILABLE".to_owned());
    }
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

#[cfg(test)]
fn global_entries_for_test() -> Vec<LogEntry> {
    GLOBAL
        .get()
        .map_or_else(Vec::new, |logger| logger.entries())
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
    fn redacts_nested_values_recursively() {
        let logger = Logger::new_in_memory(16);
        log(
            &logger,
            Level::Info,
            "safe",
            fields(&[("nested", json!({"caption": "76561197960265728", "safe": 3}))]),
        );
        assert_eq!(
            logger.entries()[0].fields["nested"]["caption"],
            "[redacted]"
        );
        assert_eq!(logger.entries()[0].fields["nested"]["safe"], 3);
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
        let result = std::panic::catch_unwind(|| panic!("logging panic hook probe {secret}"));
        assert!(result.is_err());
        let entries = super::global_entries_for_test();
        let entry = entries
            .iter()
            .find(|entry| {
                entry.event == "panic" && entry.msg.starts_with("logging panic hook probe")
            })
            .unwrap();
        assert!(!entry.msg.contains(&secret));
        assert!(entry.fields.get("location").is_some());
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
