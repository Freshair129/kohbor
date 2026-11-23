use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub const ENGINE_VERSION: &str = "0.2.0";
pub const ENGINE_SHA256: &str = "a05f91761a8bacd84ceec24522568e526e6a35a27525c3dfbfaf5e7f3192dcb6";
pub const ENGINE_SOURCE_COMMIT: &str = "a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9";
pub const BUNDLE_SCHEMA: &str = "3.0";
pub const MAX_DEM_BYTES: u64 = 400 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: u64 = 1 * 1024 * 1024;
pub const MAX_MATCH_BYTES: u64 = 48 * 1024 * 1024;
pub const MAX_WORLD_BYTES: u64 = 24 * 1024 * 1024;
pub const MAX_BUNDLE_BYTES: u64 = MAX_MANIFEST_BYTES + MAX_MATCH_BYTES + MAX_WORLD_BYTES;
pub const ENGINE_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_CAPTURE_BYTES: usize = 1024 * 1024;
const PROCESS_POLL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineFailure {
    Unavailable,
    HashMismatch,
    SpawnFailed,
    Timeout,
    OutputTooLarge,
    CaptureTooLarge,
    MatchBecameActive,
    NeedsPack,
    InvalidInput,
    EngineFailed,
    InvalidProtocol,
    InvalidBundle,
    StorageConflict,
}

impl EngineFailure {
    pub fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "engine_unavailable",
            Self::HashMismatch => "engine_hash_mismatch",
            Self::SpawnFailed => "engine_spawn_failed",
            Self::Timeout => "timeout",
            Self::OutputTooLarge => "output_too_large",
            Self::CaptureTooLarge => "engine_output_too_large",
            Self::MatchBecameActive => "match_active",
            Self::NeedsPack => "needs_pack",
            Self::InvalidInput => "invalid_input",
            Self::EngineFailed => "engine_failed",
            Self::InvalidProtocol => "engine_protocol_invalid",
            Self::InvalidBundle => "invalid_bundle",
            Self::StorageConflict => "storage_conflict",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnginePin {
    pub path: PathBuf,
    pub version: &'static str,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedBundle {
    pub match_id: String,
    pub engine_version: String,
    pub bundle_bytes: u64,
}

#[derive(Debug)]
pub struct EngineRun {
    pub engine_version: String,
    pub engine_sha256: String,
    pub stderr_code: Option<&'static str>,
}

struct CapturedChild {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

struct BoundedCapture {
    bytes: Vec<u8>,
    overflowed: bool,
}

fn pinned_hash(hash: &str) -> bool {
    hash.eq_ignore_ascii_case(ENGINE_SHA256)
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn resolve_engine_path(
    bundled_path: Option<&Path>,
    development_path: Option<&Path>,
    allow_development_path: bool,
) -> Result<EnginePin, EngineFailure> {
    let candidate = preferred_engine_path(bundled_path, development_path, allow_development_path)?;

    let canonical = candidate
        .canonicalize()
        .map_err(|_| EngineFailure::Unavailable)?;
    let digest = sha256_file(&canonical).map_err(|_| EngineFailure::Unavailable)?;
    if !pinned_hash(&digest) {
        return Err(EngineFailure::HashMismatch);
    }
    Ok(EnginePin {
        path: canonical,
        version: ENGINE_VERSION,
        sha256: digest,
    })
}

fn preferred_engine_path<'a>(
    bundled_path: Option<&'a Path>,
    development_path: Option<&'a Path>,
    allow_development_path: bool,
) -> Result<&'a Path, EngineFailure> {
    bundled_path
        .filter(|path| path.is_file())
        .or_else(|| {
            allow_development_path
                .then_some(development_path)
                .flatten()
                .filter(|path| path.is_file())
        })
        .ok_or(EngineFailure::Unavailable)
}

pub fn validate_input_path(path: &Path) -> Result<u64, EngineFailure> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dem"))
    {
        return Err(EngineFailure::InvalidInput);
    }
    let metadata = fs::metadata(path).map_err(|_| EngineFailure::InvalidInput)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_DEM_BYTES {
        return Err(EngineFailure::InvalidInput);
    }
    let mut file = File::open(path).map_err(|_| EngineFailure::InvalidInput)?;
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)
        .map_err(|_| EngineFailure::InvalidInput)?;
    if &magic != b"PBDEMS2\0" {
        return Err(EngineFailure::InvalidInput);
    }
    Ok(metadata.len())
}

pub(super) fn ensure_idle(match_active: &dyn Fn() -> bool) -> Result<(), EngineFailure> {
    if match_active() {
        Err(EngineFailure::MatchBecameActive)
    } else {
        Ok(())
    }
}

pub fn run_pinned_engine(
    bundled_path: Option<&Path>,
    development_path: Option<&Path>,
    allow_development_path: bool,
    dem_path: &Path,
    output_dir: &Path,
    match_active: &dyn Fn() -> bool,
) -> Result<EngineRun, EngineFailure> {
    let _dem_size = validate_input_path(dem_path)?;
    let pin = resolve_engine_path(bundled_path, development_path, allow_development_path)?;
    ensure_idle(match_active)?;

    let mut command = Command::new(&pin.path);
    command
        .arg("analyze")
        .arg(dem_path)
        .arg("--out")
        .arg(output_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(BELOW_NORMAL_PRIORITY_CLASS | CREATE_NO_WINDOW);
    }

    let capture = run_child(
        command,
        ENGINE_TIMEOUT,
        || ensure_output_sizes(output_dir),
        || match_active(),
    )?;

    if !capture.status.success() {
        return match engine_error_code(&capture.stdout).as_deref() {
            Some("unknown_patch") => Err(EngineFailure::NeedsPack),
            Some("not_a_replay" | "truncated") => Err(EngineFailure::InvalidInput),
            _ => Err(EngineFailure::EngineFailed),
        };
    }

    let summary: Value =
        serde_json::from_slice(&capture.stdout).map_err(|_| EngineFailure::InvalidProtocol)?;
    if summary.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(EngineFailure::InvalidProtocol);
    }

    Ok(EngineRun {
        engine_version: pin.version.to_owned(),
        engine_sha256: pin.sha256,
        stderr_code: (!capture.stderr.is_empty()).then_some("ENGINE_STDERR_CAPTURED"),
    })
}

fn engine_error_code(stdout: &[u8]) -> Option<String> {
    let response: Value = serde_json::from_slice(stdout).ok()?;
    let code = response.get("error")?.get("code")?.as_str()?;
    if code.is_empty()
        || code.len() > 64
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return None;
    }
    Some(code.to_owned())
}

fn run_child(
    mut command: Command,
    timeout: Duration,
    check_output: impl Fn() -> Result<(), EngineFailure>,
    should_cancel: impl Fn() -> bool,
) -> Result<CapturedChild, EngineFailure> {
    if should_cancel() {
        return Err(EngineFailure::MatchBecameActive);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|_| EngineFailure::SpawnFailed)?;
    let stdout = child.stdout.take().ok_or(EngineFailure::SpawnFailed)?;
    let stderr = child.stderr.take().ok_or(EngineFailure::SpawnFailed)?;
    let stdout_overflow = Arc::new(AtomicBool::new(false));
    let stderr_overflow = Arc::new(AtomicBool::new(false));
    let stdout_thread = capture_thread(stdout, stdout_overflow.clone());
    let stderr_thread = capture_thread(stderr, stderr_overflow.clone());
    let started = Instant::now();

    let status = loop {
        if should_cancel() {
            kill_and_wait(&mut child);
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(EngineFailure::MatchBecameActive);
        }
        if started.elapsed() >= timeout {
            kill_and_wait(&mut child);
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(EngineFailure::Timeout);
        }
        if stdout_overflow.load(Ordering::Relaxed) || stderr_overflow.load(Ordering::Relaxed) {
            kill_and_wait(&mut child);
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(EngineFailure::CaptureTooLarge);
        }
        if let Err(error) = check_output() {
            kill_and_wait(&mut child);
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(PROCESS_POLL),
            Err(_) => {
                kill_and_wait(&mut child);
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(EngineFailure::EngineFailed);
            }
        }
    };

    let stdout = stdout_thread
        .join()
        .map_err(|_| EngineFailure::EngineFailed)?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| EngineFailure::EngineFailed)?;
    if stdout.overflowed || stderr.overflowed {
        return Err(EngineFailure::CaptureTooLarge);
    }
    Ok(CapturedChild {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}

fn capture_thread<R: Read + Send + 'static>(
    mut reader: R,
    overflow_flag: Arc<AtomicBool>,
) -> thread::JoinHandle<BoundedCapture> {
    thread::spawn(move || {
        let mut bytes = Vec::with_capacity(MAX_CAPTURE_BYTES.min(16 * 1024));
        let mut chunk = [0u8; 8192];
        let mut overflowed = false;
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let remaining = MAX_CAPTURE_BYTES.saturating_sub(bytes.len());
                    let copied = count.min(remaining);
                    bytes.extend_from_slice(&chunk[..copied]);
                    if copied < count {
                        overflowed = true;
                        overflow_flag.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
        BoundedCapture { bytes, overflowed }
    })
}

fn kill_and_wait(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn ensure_output_sizes(output_dir: &Path) -> Result<(), EngineFailure> {
    let Ok(entries) = fs::read_dir(output_dir) else {
        return Ok(());
    };
    let mut total = 0u64;
    for entry in entries {
        let entry = entry.map_err(|_| EngineFailure::OutputTooLarge)?;
        let kind = entry
            .file_type()
            .map_err(|_| EngineFailure::OutputTooLarge)?;
        if !kind.is_file() {
            return Err(EngineFailure::OutputTooLarge);
        }
        let name = entry.file_name();
        let limit = match name.to_str() {
            Some("manifest.json") => MAX_MANIFEST_BYTES,
            Some("match.json") => MAX_MATCH_BYTES,
            Some("world.json") => MAX_WORLD_BYTES,
            _ => return Err(EngineFailure::InvalidBundle),
        };
        let size = entry
            .metadata()
            .map_err(|_| EngineFailure::OutputTooLarge)?
            .len();
        if size > limit {
            return Err(EngineFailure::OutputTooLarge);
        }
        total = total
            .checked_add(size)
            .ok_or(EngineFailure::OutputTooLarge)?;
        if total > MAX_BUNDLE_BYTES {
            return Err(EngineFailure::OutputTooLarge);
        }
    }
    Ok(())
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, EngineFailure> {
    let file = File::open(path).map_err(|_| EngineFailure::InvalidBundle)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| EngineFailure::InvalidBundle)?;
    if bytes.len() as u64 > limit {
        return Err(EngineFailure::OutputTooLarge);
    }
    Ok(bytes)
}

fn object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    value.as_object()
}

fn string_field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name)?.as_str()
}

fn number_field(value: &Value, name: &str) -> bool {
    value.get(name).is_some_and(Value::is_number)
}

fn integer_field(value: &Value, name: &str) -> bool {
    value
        .get(name)
        .is_some_and(|field| field.as_i64().is_some() || field.as_u64().is_some())
}

fn uint_field(value: &Value, name: &str) -> Option<u64> {
    value.get(name)?.as_u64()
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn has_object(value: &Value, name: &str) -> bool {
    value.get(name).is_some_and(Value::is_object)
}

fn has_array(value: &Value, name: &str) -> bool {
    value.get(name).is_some_and(Value::is_array)
}

fn number_array(value: &Value, name: &str) -> bool {
    value
        .get(name)
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().all(Value::is_number))
}

fn three_number_arrays(value: &Value) -> bool {
    value.as_array().is_some_and(|arrays| {
        arrays.len() == 3
            && arrays.iter().all(|array| {
                array
                    .as_array()
                    .is_some_and(|values| values.iter().all(Value::is_number))
            })
    })
}

fn string_array(value: &Value, name: &str) -> bool {
    value
        .get(name)
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().all(Value::is_string))
}

fn validate_manifest(value: &Value) -> Result<String, EngineFailure> {
    object(value).ok_or(EngineFailure::InvalidBundle)?;
    let schema = string_field(value, "schema").ok_or(EngineFailure::InvalidBundle)?;
    if schema != BUNDLE_SCHEMA
        || !string_field(value, "engine_version").is_some_and(|field| !field.is_empty())
        || !string_field(value, "patch").is_some_and(|field| !field.is_empty())
        || !uint_field(value, "match_id").is_some_and(|id| id > 0)
        || !has_object(value, "pack")
        || !has_object(value, "files")
        || !has_object(value, "map")
        || !has_object(value, "icons")
        || !has_object(value, "run")
    {
        return Err(EngineFailure::InvalidBundle);
    }
    let dem_sha = string_field(value, "dem_sha256").ok_or(EngineFailure::InvalidBundle)?;
    if !valid_sha256(dem_sha) {
        return Err(EngineFailure::InvalidBundle);
    }
    let pack = value.get("pack").ok_or(EngineFailure::InvalidBundle)?;
    if string_field(pack, "id").is_none() || string_field(pack, "source").is_none() {
        return Err(EngineFailure::InvalidBundle);
    }
    let map = value.get("map").ok_or(EngineFailure::InvalidBundle)?;
    if string_field(map, "image").is_none()
        || string_field(map, "pack_path").is_none()
        || !uint_field(map, "size").is_some_and(|size| size > 0)
        || string_field(map, "coord").is_none()
        || !has_object(map, "calibration")
    {
        return Err(EngineFailure::InvalidBundle);
    }
    let calibration = map.get("calibration").ok_or(EngineFailure::InvalidBundle)?;
    if !number_array(calibration, "px")
        || !number_array(calibration, "py")
        || string_field(calibration, "formula").is_none()
    {
        return Err(EngineFailure::InvalidBundle);
    }
    let icons = value.get("icons").ok_or(EngineFailure::InvalidBundle)?;
    if !has_object(icons, "dirs") || !has_object(icons, "needed") {
        return Err(EngineFailure::InvalidBundle);
    }
    let needed = icons.get("needed").ok_or(EngineFailure::InvalidBundle)?;
    if !string_array(needed, "heroes")
        || !string_array(needed, "items")
        || !string_array(needed, "abilities")
    {
        return Err(EngineFailure::InvalidBundle);
    }
    let run = value.get("run").ok_or(EngineFailure::InvalidBundle)?;
    if !uint_field(run, "parser_passes").is_some_and(|passes| passes > 0)
        || !has_object(run, "timing_s")
    {
        return Err(EngineFailure::InvalidBundle);
    }

    let files = value
        .get("files")
        .and_then(Value::as_object)
        .ok_or(EngineFailure::InvalidBundle)?;
    if files.len() != 2 || !files.contains_key("match.json") || !files.contains_key("world.json") {
        return Err(EngineFailure::InvalidBundle);
    }
    for name in ["match.json", "world.json"] {
        let record = files.get(name).ok_or(EngineFailure::InvalidBundle)?;
        if !record.is_object()
            || uint_field(record, "bytes").is_none()
            || !string_field(record, "sha256").is_some_and(valid_sha256)
        {
            return Err(EngineFailure::InvalidBundle);
        }
    }
    let match_id = uint_field(value, "match_id").ok_or(EngineFailure::InvalidBundle)?;
    let engine_version =
        string_field(value, "engine_version").ok_or(EngineFailure::InvalidBundle)?;
    Ok(format!("{match_id}:{engine_version}:{dem_sha}"))
}

fn validate_match(value: &Value) -> Result<(String, String), EngineFailure> {
    object(value).ok_or(EngineFailure::InvalidBundle)?;
    if !number_field(value, "t0")
        || !number_field(value, "t1")
        || !number_field(value, "dtick")
        || !has_array(value, "heroes")
        || !has_array(value, "deaths")
        || !has_object(value, "names")
        || !has_object(value, "statdb")
        || !has_array(value, "sampleT")
    {
        return Err(EngineFailure::InvalidBundle);
    }
    if !number_array(value, "sampleT") {
        return Err(EngineFailure::InvalidBundle);
    }
    let meta = value.get("meta").ok_or(EngineFailure::InvalidBundle)?;
    if string_field(meta, "schema") != Some(BUNDLE_SCHEMA)
        || !string_field(meta, "engine_version").is_some_and(|field| !field.is_empty())
        || !string_field(meta, "patch").is_some_and(|field| !field.is_empty())
        || string_field(meta, "pack").is_none()
        || string_field(meta, "server_build").is_none()
        || !integer_field(meta, "build_num")
        || !uint_field(meta, "match_id").is_some_and(|id| id > 0)
        || !meta
            .get("end_time_utc")
            .is_some_and(|time| time.is_null() || time.is_string())
        || !integer_field(meta, "game_mode")
        || !number_field(meta, "duration_s")
        || !string_field(meta, "dem_sha256").is_some_and(valid_sha256)
        || !meta
            .get("dem_file")
            .is_some_and(|file| file.is_null() || file.is_string())
        || !meta
            .get("parse_error")
            .is_some_and(|error| error.is_null() || error.is_object())
    {
        return Err(EngineFailure::InvalidBundle);
    }
    let heroes = value
        .get("heroes")
        .and_then(Value::as_array)
        .ok_or(EngineFailure::InvalidBundle)?;
    if heroes.iter().any(|hero| {
        !hero.is_object()
            || string_field(hero, "id").is_none()
            || string_field(hero, "name").is_none()
            || !integer_field(hero, "team")
            || !integer_field(hero, "pid")
            || string_field(hero, "color").is_none()
            || ![
                "pos", "hp", "mhp", "mp", "mmp", "lvl", "alive", "eco", "abil", "inv", "mods",
                "casts", "buys", "finds", "din", "hin",
            ]
            .iter()
            .all(|name| has_array(hero, name))
            || !["icd", "st"].iter().all(|name| has_object(hero, name))
            || !integer_field(hero, "prim")
    }) {
        return Err(EngineFailure::InvalidBundle);
    }
    let deaths = value
        .get("deaths")
        .and_then(Value::as_array)
        .ok_or(EngineFailure::InvalidBundle)?;
    if deaths.iter().any(|death| {
        !death.is_object()
            || !number_field(death, "t")
            || string_field(death, "hero").is_none()
            || !death
                .get("killer")
                .is_some_and(|killer| killer.is_null() || killer.is_string())
            || !["last", "by", "win", "hits", "heals"]
                .iter()
                .all(|name| has_array(death, name))
            || !has_object(death, "seen")
            || !integer_field(death, "out")
    }) {
        return Err(EngineFailure::InvalidBundle);
    }
    if value
        .get("names")
        .and_then(Value::as_object)
        .is_some_and(|names| names.values().any(|name| !name.is_string()))
    {
        return Err(EngineFailure::InvalidBundle);
    }
    Ok((
        uint_field(meta, "match_id")
            .ok_or(EngineFailure::InvalidBundle)?
            .to_string(),
        string_field(meta, "engine_version")
            .ok_or(EngineFailure::InvalidBundle)?
            .to_owned(),
    ))
}

fn validate_world(value: &Value) -> Result<(), EngineFailure> {
    if !object(value).is_some_and(|data| {
        ["v", "qh", "qu", "t0", "heroes", "tracks", "runes"]
            .iter()
            .all(|name| data.contains_key(*name))
    }) || !integer_field(value, "v")
        || !number_field(value, "qh")
        || !number_field(value, "qu")
        || !number_field(value, "t0")
        || !has_object(value, "heroes")
        || !has_array(value, "tracks")
        || !has_array(value, "runes")
    {
        return Err(EngineFailure::InvalidBundle);
    }
    let heroes = value.get("heroes").and_then(Value::as_object).unwrap();
    if heroes.values().any(|hero| {
        !number_array(hero, "t")
            || !number_array(hero, "x")
            || !number_array(hero, "y")
            || !has_array(hero, "brk")
            || !has_array(hero, "dead")
            || !has_array(hero, "vis")
    }) {
        return Err(EngineFailure::InvalidBundle);
    }
    let tracks = value.get("tracks").and_then(Value::as_array).unwrap();
    if tracks.iter().any(|track| {
        !track.is_object()
            || string_field(track, "k").is_none()
            || !integer_field(track, "team")
            || !number_field(track, "t0")
            || !has_array(track, "vis")
            || !number_field(track, "t1")
            || !track.get("p").is_some_and(three_number_arrays)
            || track.get("hp").is_some_and(|hp| !hp.is_array())
            || track.get("n").is_some_and(|name| !name.is_string())
    }) {
        return Err(EngineFailure::InvalidBundle);
    }
    let runes = value.get("runes").and_then(Value::as_array).unwrap();
    if runes.iter().any(|rune| {
        !rune.is_object()
            || !number_field(rune, "t0")
            || !number_field(rune, "t1")
            || string_field(rune, "type").is_none()
            || !number_field(rune, "x")
            || !number_field(rune, "y")
            || string_field(rune, "out").is_none()
            || !rune
                .get("by")
                .is_some_and(|hero| hero.is_null() || hero.is_string())
    }) {
        return Err(EngineFailure::InvalidBundle);
    }
    Ok(())
}

pub fn validate_bundle_dir(
    directory: &Path,
    expected_dem_sha256: &str,
) -> Result<ValidatedBundle, EngineFailure> {
    ensure_output_sizes(directory)?;
    let manifest_bytes = read_limited(&directory.join("manifest.json"), MAX_MANIFEST_BYTES)?;
    let match_bytes = read_limited(&directory.join("match.json"), MAX_MATCH_BYTES)?;
    let world_bytes = read_limited(&directory.join("world.json"), MAX_WORLD_BYTES)?;

    for entry in fs::read_dir(directory).map_err(|_| EngineFailure::InvalidBundle)? {
        let entry = entry.map_err(|_| EngineFailure::InvalidBundle)?;
        let kind = entry
            .file_type()
            .map_err(|_| EngineFailure::InvalidBundle)?;
        if !kind.is_file()
            || !matches!(
                entry.file_name().to_str(),
                Some("manifest.json" | "match.json" | "world.json")
            )
        {
            return Err(EngineFailure::InvalidBundle);
        }
    }

    let manifest: Value =
        serde_json::from_slice(&manifest_bytes).map_err(|_| EngineFailure::InvalidBundle)?;
    let match_data: Value =
        serde_json::from_slice(&match_bytes).map_err(|_| EngineFailure::InvalidBundle)?;
    let world: Value =
        serde_json::from_slice(&world_bytes).map_err(|_| EngineFailure::InvalidBundle)?;

    let manifest_identity = validate_manifest(&manifest)?;
    let (match_id, match_engine_version) = validate_match(&match_data)?;
    validate_world(&world)?;

    let mut identity_parts = manifest_identity.splitn(3, ':');
    let manifest_match_id = identity_parts.next().unwrap_or_default();
    let manifest_engine_version = identity_parts.next().unwrap_or_default();
    let manifest_dem_sha = identity_parts.next().unwrap_or_default();
    if manifest_match_id != match_id
        || manifest_engine_version != match_engine_version
        || manifest_dem_sha != expected_dem_sha256
        || string_field(&match_data["meta"], "dem_sha256") != Some(expected_dem_sha256)
    {
        return Err(EngineFailure::InvalidBundle);
    }

    let file_records = manifest["files"]
        .as_object()
        .ok_or(EngineFailure::InvalidBundle)?;
    for (name, bytes) in [
        ("match.json", match_bytes.as_slice()),
        ("world.json", world_bytes.as_slice()),
    ] {
        let record = file_records.get(name).ok_or(EngineFailure::InvalidBundle)?;
        if uint_field(record, "bytes") != Some(bytes.len() as u64)
            || string_field(record, "sha256") != Some(sha256_bytes(bytes).as_str())
        {
            return Err(EngineFailure::InvalidBundle);
        }
    }

    let engine_version = string_field(&manifest, "engine_version")
        .ok_or(EngineFailure::InvalidBundle)?
        .to_owned();
    let bundle_bytes = (manifest_bytes.len() + match_bytes.len() + world_bytes.len()) as u64;
    Ok(ValidatedBundle {
        match_id,
        engine_version,
        bundle_bytes,
    })
}

fn run_child_test_adapter(
    command: Command,
    timeout: Duration,
    should_cancel: impl Fn() -> bool,
) -> Result<ExitStatus, EngineFailure> {
    run_child(command, timeout, || Ok(()), should_cancel).map(|capture| capture.status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "kohbor-replay-engine-test-{label}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn wrong_executable_hash_is_refused_before_spawn() {
        let directory = temp_dir("wrong-pin");
        let executable = directory.join("wrong.exe");
        fs::write(&executable, b"not the pinned executable").unwrap();
        let result = resolve_engine_path(Some(&executable), None, false);
        assert_eq!(result, Err(EngineFailure::HashMismatch));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn bundled_engine_path_takes_precedence_over_development_path() {
        let directory = temp_dir("precedence");
        let bundled = directory.join("bundled.exe");
        let development = directory.join("dev.exe");
        fs::write(&bundled, b"bad").unwrap();
        fs::write(&development, b"also bad").unwrap();
        assert_eq!(
            preferred_engine_path(Some(&bundled), Some(&development), true),
            Ok(bundled.as_path())
        );
        let result = resolve_engine_path(Some(&bundled), Some(&development), true);
        // A present but unpinned bundled executable fails closed; it never
        // falls through to a development override.
        assert_eq!(result, Err(EngineFailure::HashMismatch));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn development_path_is_used_only_when_allowed_and_no_bundle_exists() {
        let directory = temp_dir("dev-path");
        let development = directory.join("dev.exe");
        fs::write(&development, b"dev").unwrap();
        assert_eq!(
            preferred_engine_path(None, Some(&development), true),
            Ok(development.as_path())
        );
        assert_eq!(
            preferred_engine_path(None, Some(&development), false),
            Err(EngineFailure::Unavailable)
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn timeout_kills_the_child_process() {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("timeout_helper_child_process")
            .arg("--nocapture")
            .env("KOHBOR_REPLAY_TEST_TIMEOUT_CHILD", "1");
        let result = run_child_test_adapter(command, Duration::from_millis(150), || false);
        assert_eq!(result, Err(EngineFailure::Timeout));
    }

    #[test]
    fn timeout_helper_child_process() {
        if std::env::var_os("KOHBOR_REPLAY_TEST_TIMEOUT_CHILD").is_some() {
            thread::sleep(Duration::from_secs(10));
        }
    }

    #[test]
    fn structural_validation_rejects_an_incomplete_match_document() {
        let directory = temp_dir("invalid-bundle");
        let dem_sha = "a".repeat(64);
        let match_bytes = br#"{"meta":{"schema":"3.0"}}"#;
        let world_bytes = br#"{"v":1,"qh":2,"qu":8,"t0":0,"heroes":{},"tracks":[],"runes":[]}"#;
        let manifest = json!({
            "schema": "3.0",
            "engine_version": ENGINE_VERSION,
            "match_id": 55,
            "patch": "7.42",
            "pack": {"id": "7.42", "source": "embedded"},
            "dem_sha256": dem_sha,
            "files": {
                "match.json": {"bytes": match_bytes.len(), "sha256": sha256_bytes(match_bytes)},
                "world.json": {"bytes": world_bytes.len(), "sha256": sha256_bytes(world_bytes)}
            },
            "map": {"image": "map.png", "pack_path": "7.42/map.png", "size": 1, "coord": "world", "calibration": {}},
            "icons": {"dirs": {}, "needed": {}},
            "run": {"parser_passes": 1, "timing_s": {}}
        });
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(directory.join("match.json"), match_bytes).unwrap();
        fs::write(directory.join("world.json"), world_bytes).unwrap();
        assert_eq!(
            validate_bundle_dir(&directory, &dem_sha),
            Err(EngineFailure::InvalidBundle)
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn structural_validation_accepts_a_complete_schema3_bundle_fixture() {
        let directory = temp_dir("valid-bundle");
        let dem_sha = "a".repeat(64);
        let match_doc = json!({
            "meta": {
                "schema": BUNDLE_SCHEMA,
                "engine_version": ENGINE_VERSION,
                "patch": "7.42",
                "pack": "7.42",
                "server_build": "dota_v123",
                "build_num": 123,
                "match_id": 55,
                "end_time_utc": null,
                "game_mode": 22,
                "duration_s": 1200,
                "dem_sha256": dem_sha,
                "dem_file": "55.dem",
                "parse_error": null
            },
            "t0": 0,
            "t1": 1200,
            "dtick": 30,
            "heroes": [{
                "id": "npc_dota_hero_axe",
                "name": "Axe",
                "team": 2,
                "pid": 0,
                "color": "#3375FF",
                "pos": [], "hp": [], "mhp": [], "mp": [], "mmp": [], "lvl": [],
                "alive": [], "eco": [], "abil": [], "inv": [], "icd": {}, "mods": [],
                "casts": [], "buys": [], "finds": [], "st": {}, "prim": 0, "din": [], "hin": []
            }],
            "deaths": [{
                "t": 900,
                "hero": "Axe",
                "killer": null,
                "last": [], "by": [], "win": [], "hits": [], "heals": [],
                "seen": {},
                "out": 0
            }],
            "names": {},
            "statdb": {},
            "sampleT": [],
        });
        let world_doc = json!({
            "v": 1,
            "qh": 8,
            "qu": 30,
            "t0": 0,
            "heroes": {
                "npc_dota_hero_axe": { "t": [], "x": [], "y": [], "brk": [], "dead": [], "vis": [] }
            },
            "tracks": [
                { "k": "ward", "team": 2, "t0": 10, "vis": [], "t1": 20, "p": [[], [], []] },
                { "k": "rosh", "team": 3, "t0": 11, "vis": [], "hp": [[11, 100]], "n": "roshan", "t1": 21, "p": [[], [], []] }
            ],
            "runes": [
                { "t0": 10, "t1": 11, "type": "bounty", "x": 1, "y": 2, "out": "collected", "by": "Axe" },
                { "t0": 20, "t1": 21, "type": "wisdom", "x": 3, "y": 4, "out": "expired", "by": null }
            ]
        });
        let match_bytes = serde_json::to_vec(&match_doc).unwrap();
        let world_bytes = serde_json::to_vec(&world_doc).unwrap();
        let manifest = json!({
            "schema": BUNDLE_SCHEMA,
            "engine_version": ENGINE_VERSION,
            "match_id": 55,
            "patch": "7.42",
            "pack": { "id": "7.42", "source": "embedded" },
            "dem_sha256": dem_sha,
            "files": {
                "match.json": { "bytes": match_bytes.len(), "sha256": sha256_bytes(&match_bytes) },
                "world.json": { "bytes": world_bytes.len(), "sha256": sha256_bytes(&world_bytes) }
            },
            "map": {
                "image": "map.png", "pack_path": "7.42/map.png", "size": 1, "coord": "world",
                "calibration": { "px": [], "py": [], "formula": "linear" }
            },
            "icons": {
                "dirs": {},
                "needed": { "heroes": [], "items": [], "abilities": [] }
            },
            "run": { "parser_passes": 1, "timing_s": {} }
        });
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(directory.join("match.json"), match_bytes).unwrap();
        fs::write(directory.join("world.json"), world_bytes).unwrap();

        let result = validate_bundle_dir(&directory, &dem_sha).unwrap();
        assert_eq!(result.match_id, "55");
        assert_eq!(result.engine_version, ENGINE_VERSION);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn world_tracks_allow_absent_optional_health_and_name_but_reject_wrong_types() {
        let mut world = json!({
            "v": 1, "qh": 8, "qu": 30, "t0": 0,
            "heroes": {},
            "tracks": [{ "k": "lane", "team": 2, "t0": 1, "vis": [], "t1": 2, "p": [[], [], []] }],
            "runes": []
        });
        assert!(validate_world(&world).is_ok());

        world["tracks"][0]["hp"] = json!("unknown");
        assert_eq!(validate_world(&world), Err(EngineFailure::InvalidBundle));
        world["tracks"][0].as_object_mut().unwrap().remove("hp");
        world["tracks"][0]["n"] = json!(false);
        assert_eq!(validate_world(&world), Err(EngineFailure::InvalidBundle));
    }

    #[test]
    fn non_pinned_hash_is_not_in_the_allowlist() {
        assert_eq!(
            ENGINE_SHA256,
            "a05f91761a8bacd84ceec24522568e526e6a35a27525c3dfbfaf5e7f3192dcb6"
        );
        assert_eq!(
            ENGINE_SOURCE_COMMIT,
            "a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9"
        );
        assert!(!pinned_hash(&sha256_bytes(b"wrong engine")));
        assert!(pinned_hash(ENGINE_SHA256));
        assert!(!pinned_hash(
            "5955ddc1b2c8bc5881876ca29e6fb73d1be7c449a6715fe76ede849f5828b46a"
        ));
    }

    #[test]
    fn active_match_gate_cancels_before_spawning_the_engine() {
        let command = Command::new("this-replay-engine-must-not-be-spawned.exe");
        let result = run_child_test_adapter(command, Duration::from_secs(1), || true);
        assert_eq!(result, Err(EngineFailure::MatchBecameActive));
    }

    #[test]
    fn opt_in_bundled_engine_parses_one_saved_demo() {
        if std::env::var_os("KOHBOR_REPLAY_ENGINE").is_none() {
            return;
        }
        let engine_path = PathBuf::from(std::env::var_os("KOHBOR_REPLAY_ENGINE").unwrap());
        let demo_root = Path::new(r"F:\gsi-log\demos");
        let demo = fs::read_dir(demo_root)
            .expect("the configured replay demo folder must be readable")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("dem"))
            })
            .expect("one saved demo is required for opt-in integration");
        let directory = temp_dir("dev-integration");
        let started = Instant::now();
        let run = run_pinned_engine(Some(&engine_path), None, false, &demo, &directory, &|| {
            false
        })
        .expect("the pinned bundled replay engine should parse the selected demo");
        let dem_sha = sha256_file(&demo).expect("the saved demo should be readable");
        let bundle = validate_bundle_dir(&directory, &dem_sha)
            .expect("the bundled engine output should satisfy Match Bundle 3.0");
        assert_eq!(bundle.engine_version, run.engine_version);
        eprintln!(
            "replay_integration engine=bundled elapsed_ms={} result=ready",
            started.elapsed().as_millis()
        );
        let _ = fs::remove_dir_all(directory);
    }
}
