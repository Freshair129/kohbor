mod engine;

use serde::{ser::SerializeStruct, Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const INDEX_SCHEMA_VERSION: u32 = 1;
const MAX_INDEX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_META_BYTES: u64 = 64 * 1024;
const WATCH_INTERVAL: Duration = Duration::from_secs(30);
const MATCH_IDLE_POLL: Duration = Duration::from_millis(250);
const MAX_JOB_HISTORY: usize = 50;
pub(crate) const EVENT_NAME: &str = "replay-library://state-changed";
static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TOMBSTONE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_PICK_TOKEN: AtomicU64 = AtomicU64::new(1);

pub(crate) type EventSink = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Copy, Debug)]
pub struct ReplayLibraryLimits {
    pub max_matches: usize,
    pub max_bytes: u64,
}

impl Default for ReplayLibraryLimits {
    fn default() -> Self {
        Self {
            max_matches: 20,
            max_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ReplayStatus {
    Available,
    Queued,
    Parsing,
    Ready,
    Evicted,
    Failed { reason: String },
    NeedsPack,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplaySource {
    DotaReplaysFolder,
    ManualPick,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayLibraryEntry {
    pub match_id: String,
    pub source: ReplaySource,
    pub source_path: String,
    pub dem_sha256: Option<String>,
    pub engine_version: Option<String>,
    pub engine_sha256: Option<String>,
    pub parse_time_unix_ms: Option<u64>,
    pub status: ReplayStatus,
    #[serde(default)]
    pub bundle_bytes: u64,
    #[serde(default)]
    pub storage_bytes: u64,
    #[serde(default)]
    pub created_at_unix_ms: u64,
    #[serde(default)]
    pub match_date_utc: Option<String>,
    #[serde(default)]
    pub own_hero: Option<String>,
    #[serde(default)]
    pub duration_s: Option<u64>,
    #[serde(default)]
    pub patch: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    // True for automatic queue work, including a pending retry after interruption.
    #[serde(default)]
    pub(crate) auto_queued: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayJob {
    pub job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_id: Option<String>,
    pub operation: ReplayOperation,
    pub status: ReplayStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayOperation {
    Parse,
    Reparse,
}

#[derive(Clone, Debug)]
pub struct ReplayLibraryList {
    pub matches: Vec<ReplayLibraryEntry>,
    pub jobs: Vec<ReplayJob>,
    source_exists: Vec<bool>,
    bundle_schemas: Vec<Option<String>>,
    bundle_hashes: Vec<Option<String>>,
}

#[derive(Serialize)]
struct ReplayLibraryEntryView<'a> {
    match_id: &'a str,
    source: ReplaySource,
    engine_version: &'a Option<String>,
    parse_time_unix_ms: Option<u64>,
    status: &'a ReplayStatus,
    bundle_bytes: u64,
    storage_bytes: u64,
    created_at_unix_ms: u64,
    match_date_utc: &'a Option<String>,
    own_hero: &'a Option<String>,
    duration_s: Option<u64>,
    patch: &'a Option<String>,
    pinned: bool,
    auto_queued: bool,
    source_exists: bool,
    bundle_schema: Option<&'a str>,
    bundle_hash: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReplayWatcherState {
    pub folder_found: Option<bool>,
    pub last_scan_unix_ms: Option<u64>,
    pub last_error_code: Option<String>,
}

impl Serialize for ReplayLibraryList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let entries = self
            .matches
            .iter()
            .enumerate()
            .map(|(index, entry)| ReplayLibraryEntryView {
                match_id: &entry.match_id,
                source: entry.source,
                engine_version: &entry.engine_version,
                parse_time_unix_ms: entry.parse_time_unix_ms,
                status: &entry.status,
                bundle_bytes: entry.bundle_bytes,
                storage_bytes: entry.storage_bytes,
                created_at_unix_ms: entry.created_at_unix_ms,
                match_date_utc: &entry.match_date_utc,
                own_hero: &entry.own_hero,
                duration_s: entry.duration_s,
                patch: &entry.patch,
                pinned: entry.pinned,
                auto_queued: entry.auto_queued,
                source_exists: self.source_exists.get(index).copied().unwrap_or(false),
                bundle_schema: self.bundle_schemas.get(index).and_then(Option::as_deref),
                bundle_hash: self.bundle_hashes.get(index).and_then(Option::as_deref),
            })
            .collect::<Vec<_>>();
        let mut state = serializer.serialize_struct("ReplayLibraryList", 2)?;
        state.serialize_field("matches", &entries)?;
        state.serialize_field("jobs", &self.jobs)?;
        state.end()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayBundle {
    pub manifest: Value,
    pub match_data: Value,
    pub world: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ReplayIndex {
    schema_version: u32,
    entries: Vec<ReplayLibraryEntry>,
    #[serde(default)]
    suppressed_watched_match_ids: Vec<String>,
}

impl Default for ReplayIndex {
    fn default() -> Self {
        Self {
            schema_version: INDEX_SCHEMA_VERSION,
            entries: Vec::new(),
            suppressed_watched_match_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct ReplayMeta<'a> {
    match_id: &'a str,
    source: ReplaySource,
    source_path: &'a str,
    dem_sha256: &'a Option<String>,
    engine_version: &'a Option<String>,
    engine_sha256: &'a Option<String>,
    parse_time_unix_ms: Option<u64>,
    status: &'a ReplayStatus,
    match_date_utc: &'a Option<String>,
    own_hero: &'a Option<String>,
    duration_s: Option<u64>,
    patch: &'a Option<String>,
    pinned: bool,
    auto_queued: bool,
    created_at_unix_ms: u64,
}

impl ReplayLibraryEntry {
    fn meta(&self) -> ReplayMeta<'_> {
        ReplayMeta {
            match_id: &self.match_id,
            source: self.source,
            source_path: &self.source_path,
            dem_sha256: &self.dem_sha256,
            engine_version: &self.engine_version,
            engine_sha256: &self.engine_sha256,
            parse_time_unix_ms: self.parse_time_unix_ms,
            status: &self.status,
            match_date_utc: &self.match_date_utc,
            own_hero: &self.own_hero,
            duration_s: self.duration_s,
            patch: &self.patch,
            pinned: self.pinned,
            auto_queued: self.auto_queued,
            created_at_unix_ms: self.created_at_unix_ms,
        }
    }
}

struct StoreState {
    index: ReplayIndex,
    unavailable: Option<&'static str>,
}

struct JobRecord {
    public: ReplayJob,
    match_id: Option<String>,
    mode: ParseMode,
    queued_at: Instant,
}

trait EngineRunner: Send + Sync {
    fn run(
        &self,
        bundled_path: Option<&Path>,
        development_path: Option<&Path>,
        allow_development_path: bool,
        dem_path: &Path,
        output_dir: &Path,
        match_active: &dyn Fn() -> bool,
    ) -> Result<engine::EngineRun, engine::EngineFailure>;
}

struct PinnedEngineRunner;

impl EngineRunner for PinnedEngineRunner {
    fn run(
        &self,
        bundled_path: Option<&Path>,
        development_path: Option<&Path>,
        allow_development_path: bool,
        dem_path: &Path,
        output_dir: &Path,
        match_active: &dyn Fn() -> bool,
    ) -> Result<engine::EngineRun, engine::EngineFailure> {
        engine::run_pinned_engine(
            bundled_path,
            development_path,
            allow_development_path,
            dem_path,
            output_dir,
            match_active,
        )
    }
}

struct Inner {
    root: PathBuf,
    staging: PathBuf,
    bundled_engine: Option<PathBuf>,
    allow_development_engine: bool,
    engine_runner: Arc<dyn EngineRunner>,
    limits: Mutex<ReplayLibraryLimits>,
    match_active: Arc<dyn Fn() -> bool + Send + Sync>,
    emit_state_changed: EventSink,
    state: Mutex<StoreState>,
    jobs: Mutex<VecDeque<JobRecord>>,
    operation: Mutex<()>,
    worker_available: AtomicBool,
    auto_analyse: AtomicBool,
    watcher_state: Mutex<ReplayWatcherState>,
    watched_sizes: Mutex<HashMap<String, (u64, u8)>>,
    stable_watched_match_ids: Mutex<HashSet<String>>,
    picked_files: Mutex<HashMap<String, PathBuf>>,
}

#[derive(Clone)]
struct ParseRequest {
    job_id: String,
    match_id: Option<String>,
    source: ReplaySource,
    source_path: PathBuf,
    mode: ParseMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParseMode {
    Manual,
    Automatic,
    Reparse,
    Retry,
}

impl ParseMode {
    fn automatic(self) -> bool {
        self == Self::Automatic
    }

    fn automatic_retry(self) -> bool {
        matches!(self, Self::Automatic | Self::Retry)
    }
}

#[derive(Clone)]
pub struct ReplayLibrary {
    inner: Arc<Inner>,
    sender: Option<Sender<ParseRequest>>,
}

impl ReplayLibrary {
    pub fn open(
        root: PathBuf,
        bundled_engine: Option<PathBuf>,
        match_active: Arc<dyn Fn() -> bool + Send + Sync>,
        emit_state_changed: EventSink,
        limits: ReplayLibraryLimits,
    ) -> Self {
        Self::open_with_auto_analyse(
            root,
            bundled_engine,
            match_active,
            emit_state_changed,
            limits,
            true,
        )
    }

    pub fn open_with_auto_analyse(
        root: PathBuf,
        bundled_engine: Option<PathBuf>,
        match_active: Arc<dyn Fn() -> bool + Send + Sync>,
        emit_state_changed: EventSink,
        limits: ReplayLibraryLimits,
        auto_analyse: bool,
    ) -> Self {
        let library = Self::new_configured(
            root,
            bundled_engine,
            cfg!(any(debug_assertions, feature = "demo", feature = "lab")),
            match_active,
            emit_state_changed,
            limits,
            auto_analyse,
        );
        library.start_background_workers();
        library
    }

    fn new_configured(
        root: PathBuf,
        bundled_engine: Option<PathBuf>,
        allow_development_engine: bool,
        match_active: Arc<dyn Fn() -> bool + Send + Sync>,
        emit_state_changed: EventSink,
        limits: ReplayLibraryLimits,
        auto_analyse: bool,
    ) -> Self {
        let library = Self::new(
            root,
            bundled_engine,
            allow_development_engine,
            match_active,
            emit_state_changed,
            limits,
        );
        library
            .inner
            .auto_analyse
            .store(auto_analyse, Ordering::Release);
        if auto_analyse {
            let _operation = lock(&library.inner.operation);
            let _ = library.queue_available_auto_backlog_locked();
        }
        library
    }

    fn new(
        root: PathBuf,
        bundled_engine: Option<PathBuf>,
        allow_development_engine: bool,
        match_active: Arc<dyn Fn() -> bool + Send + Sync>,
        emit_state_changed: EventSink,
        limits: ReplayLibraryLimits,
    ) -> Self {
        Self::new_with_engine_runner(
            root,
            bundled_engine,
            allow_development_engine,
            match_active,
            emit_state_changed,
            limits,
            Arc::new(PinnedEngineRunner),
        )
    }

    fn new_with_engine_runner(
        root: PathBuf,
        bundled_engine: Option<PathBuf>,
        allow_development_engine: bool,
        match_active: Arc<dyn Fn() -> bool + Send + Sync>,
        emit_state_changed: EventSink,
        limits: ReplayLibraryLimits,
        engine_runner: Arc<dyn EngineRunner>,
    ) -> Self {
        let staging = root.join(".staging");
        let storage_failure = if limits.max_matches == 0 || limits.max_bytes == 0 {
            Some("invalid_library_limits")
        } else if fs::create_dir_all(&staging).is_err() {
            Some("storage_unavailable")
        } else {
            None
        };
        let (mut index, load_failure) = load_index(&root);
        let mut unavailable = storage_failure.or(load_failure);
        if unavailable.is_none() {
            if let Err(reason) = recover_interrupted_entries(&root, &mut index) {
                unavailable = Some(reason);
            }
        }
        let (sender, receiver) = if unavailable.is_none() {
            let (sender, receiver) = mpsc::channel();
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };
        let library = Self {
            inner: Arc::new(Inner {
                root,
                staging,
                bundled_engine,
                allow_development_engine,
                engine_runner,
                limits: Mutex::new(limits),
                match_active,
                emit_state_changed,
                state: Mutex::new(StoreState { index, unavailable }),
                jobs: Mutex::new(VecDeque::new()),
                operation: Mutex::new(()),
                worker_available: AtomicBool::new(false),
                auto_analyse: AtomicBool::new(true),
                watcher_state: Mutex::new(ReplayWatcherState {
                    folder_found: None,
                    last_scan_unix_ms: None,
                    last_error_code: None,
                }),
                watched_sizes: Mutex::new(HashMap::new()),
                stable_watched_match_ids: Mutex::new(HashSet::new()),
                picked_files: Mutex::new(HashMap::new()),
            }),
            sender,
        };
        if let Some(receiver) = receiver {
            library.start_worker(receiver);
        }
        library
    }

    fn start_background_workers(&self) {
        if !self.inner.worker_available.load(Ordering::Relaxed) {
            return;
        }
        let library = self.clone();
        let _ = thread::Builder::new()
            .name("replay-folder-watch".to_owned())
            .spawn(move || {
                loop {
                    match crate::intake::locate_game() {
                        Ok(game) => {
                    let replay_dir = game.join("game").join("dota").join("replays");
                    let _ = library.watch_replay_folder(&replay_dir);
                }
                        Err(_) => {
                            library.record_watcher_state(false, Some("game_install_not_found"));
                            eprintln!("{}", watcher_summary_log_line(0, 0, 0, Some("game_install_not_found")));
                        }
                    }
                thread::sleep(WATCH_INTERVAL);
                }
            });
    }

    fn start_worker(&self, receiver: Receiver<ParseRequest>) {
        let library = self.clone();
        if thread::Builder::new()
            .name("replay-engine-worker".to_owned())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    library.process_request(request);
                }
            })
            .is_ok()
        {
            self.inner.worker_available.store(true, Ordering::Relaxed);
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(root: PathBuf, limits: ReplayLibraryLimits) -> Self {
        Self::new_for_tests_with_phase(root, limits, false)
    }

    #[cfg(test)]
    fn new_for_tests_with_phase(root: PathBuf, limits: ReplayLibraryLimits, match_active: bool) -> Self {
        Self::new(
            root,
            None,
            false,
            Arc::new(move || match_active),
            Arc::new(|| {}),
            limits,
        )
    }

    pub fn list(&self) -> Result<ReplayLibraryList, &'static str> {
        let _operation = lock(&self.inner.operation);
        let state = lock(&self.inner.state);
        if let Some(reason) = state.unavailable {
            return Err(reason);
        }
        let mut matches = state.index.entries.clone();
        drop(state);
        matches.sort_by(|left, right| replay_recency(right).cmp(&replay_recency(left)));
        let mut source_exists = Vec::with_capacity(matches.len());
        let mut bundle_schemas = Vec::with_capacity(matches.len());
        let mut bundle_hashes = Vec::with_capacity(matches.len());
        for entry in &mut matches {
            enrich_list_entry(&self.inner.root, entry);
            source_exists.push(
                source_path_from_meta(&self.inner.root, &entry.match_id)
                    .is_some_and(|path| path.is_file()),
            );
            let manifest = read_json_limited(
                &self.inner.root.join(&entry.match_id).join("manifest.json"),
                engine::MAX_MANIFEST_BYTES,
            )
            .ok();
            bundle_schemas.push(manifest.as_ref().and_then(|manifest| {
                manifest
                    .get("schema")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .filter(|schema| schema == "3.0" || schema == "3.1")
            }));
            bundle_hashes.push(manifest.as_ref().and_then(|manifest| {
                validated_library_bundle_hash(&self.inner.root, entry, manifest)
            }));
        }
        let jobs = lock(&self.inner.jobs)
            .iter()
            .map(|record| record.public.clone())
            .collect();
        Ok(ReplayLibraryList {
            matches,
            jobs,
            source_exists,
            bundle_schemas,
            bundle_hashes,
        })
    }

    pub fn watcher_state(&self) -> ReplayWatcherState {
        lock(&self.inner.watcher_state).clone()
    }

    /// Return only watched replay rows for the post-game status lookup. Unlike `list`, this does
    /// not inspect or hash every bundle because the caller needs only source identity and state.
    pub fn watched_entries(&self) -> Result<Vec<ReplayLibraryEntry>, &'static str> {
        let state = lock(&self.inner.state);
        if let Some(reason) = state.unavailable {
            return Err(reason);
        }
        Ok(state.index.entries.iter()
            .filter(|entry| entry.source == ReplaySource::DotaReplaysFolder)
            .cloned()
            .collect())
    }

    pub fn register_picked_file(&self, path: PathBuf) -> String {
        let token = format!(
            "picked-file-{}-{}",
            std::process::id(),
            NEXT_PICK_TOKEN.fetch_add(1, Ordering::Relaxed)
        );
        lock(&self.inner.picked_files).insert(token.clone(), path);
        token
    }

    pub fn preview_retention(&self, limits: ReplayLibraryLimits) -> Result<usize, &'static str> {
        if !(5..=100).contains(&limits.max_matches) || limits.max_bytes == 0 {
            return Err("invalid_library_limits");
        }
        let _operation = lock(&self.inner.operation);
        let state = lock(&self.inner.state);
        if let Some(reason) = state.unavailable {
            return Err(reason);
        }
        let mut preview = state.index.clone();
        drop(state);
        Ok(self.trim_to_limits_with(&mut preview, "", limits)?.len())
    }

    pub fn bundle_folder(&self, match_id: &str) -> Result<PathBuf, &'static str> {
        validate_match_id(match_id).map_err(|_| "invalid_match_id")?;
        if !lock(&self.inner.state).index.entries.iter().any(|entry| entry.match_id == match_id) {
            return Err("match_not_found");
        }
        Ok(self.inner.root.join(match_id))
    }

    pub fn set_auto_analyse(&self, enabled: bool) {
        let _operation = lock(&self.inner.operation);
        let was_enabled = self.inner.auto_analyse.swap(enabled, Ordering::AcqRel);
        if was_enabled == enabled {
            return;
        }

        if !enabled {
            let mut state = lock(&self.inner.state);
            if state.unavailable.is_none() {
                let mut next = state.index.clone();
                let mut cancelled = Vec::new();
                for entry in &mut next.entries {
                    if entry.auto_queued && entry.status == ReplayStatus::Queued {
                        entry.status = ReplayStatus::Available;
                        entry.auto_queued = false;
                        entry.storage_bytes = entry.bundle_bytes
                            + entry_meta_bytes(entry).map(|bytes| bytes.len() as u64).unwrap_or(0);
                        cancelled.push(entry.match_id.clone());
                        let _ = write_meta(&self.inner.root.join(&entry.match_id), entry);
                    }
                }
                if !cancelled.is_empty() {
                    let _ = write_index(&self.inner.root, &next);
                    state.index = next;
                    drop(state);
                    for match_id in cancelled {
                        self.cancel_queued_auto_job(&match_id);
                    }
                    self.emit_state_changed();
                }
            }
            return;
        }

            let _ = self.queue_available_auto_backlog_locked();
    }

    /// Read the existing automatic-parse preference without exposing it to the library UI model.
    pub fn auto_analyse_enabled(&self) -> bool {
        self.inner.auto_analyse.load(Ordering::Acquire)
    }

    pub fn set_limits(&self, limits: ReplayLibraryLimits) -> Result<(), &'static str> {
        self.apply_limits(limits, None)
    }

    pub fn set_limits_previewed(
        &self,
        limits: ReplayLibraryLimits,
        expected_evictions: usize,
    ) -> Result<(), &'static str> {
        self.apply_limits(limits, Some(expected_evictions))
    }

    fn apply_limits(
        &self,
        limits: ReplayLibraryLimits,
        expected_evictions: Option<usize>,
    ) -> Result<(), &'static str> {
        if !(5..=100).contains(&limits.max_matches) || limits.max_bytes == 0 {
            return Err("invalid_library_limits");
        }
        let _operation = lock(&self.inner.operation);
        let mut state = lock(&self.inner.state);
        let mut next = state.index.clone();
        let evicted = self.trim_to_limits_with(&mut next, "", limits)?;
        if expected_evictions.is_some_and(|expected| expected != evicted.len()) {
            return Err("retention_preview_stale");
        }
        let staged = stage_bundle_evictions(&self.inner.root, &evicted)?;
        if write_evicted_metadata(&self.inner.root, &next, &evicted).is_err() {
            restore_bundle_evictions(&self.inner.root, &staged);
            for id in &evicted {
                if let Some(old) = state
                    .index
                    .entries
                    .iter()
                    .find(|entry| &entry.match_id == id)
                {
                    let _ = write_meta(&self.inner.root.join(id), old);
                }
            }
            return Err("storage_unavailable");
        }
        if write_index(&self.inner.root, &next).is_err() {
            restore_bundle_evictions(&self.inner.root, &staged);
            for id in &evicted {
                if let Some(old) = state
                    .index
                    .entries
                    .iter()
                    .find(|entry| &entry.match_id == id)
                {
                    let _ = write_meta(&self.inner.root.join(id), old);
                }
            }
            return Err("storage_unavailable");
        }
        state.index = next;
        drop(state);
        finish_bundle_evictions(&staged);
        log_evictions(evicted.len());
        *lock(&self.inner.limits) = limits;
        self.emit_state_changed();
        Ok(())
    }

    pub fn set_pinned(&self, match_id: &str, pinned: bool) -> Result<(), &'static str> {
        validate_match_id(match_id).map_err(|_| "invalid_match_id")?;
        let _operation = lock(&self.inner.operation);
        let mut state = lock(&self.inner.state);
        let mut next = state.index.clone();
        let entry = next.entries.iter_mut().find(|entry| entry.match_id == match_id).ok_or("match_not_found")?;
        entry.pinned = pinned;
        let evicted = self.trim_to_limits(&mut next, "")?;
        let staged = stage_bundle_evictions(&self.inner.root, &evicted)?;
        if write_evicted_metadata(&self.inner.root, &next, &evicted).is_err() {
            restore_bundle_evictions(&self.inner.root, &staged);
            for id in &evicted {
                if let Some(old) = state
                    .index
                    .entries
                    .iter()
                    .find(|entry| &entry.match_id == id)
                {
                    let _ = write_meta(&self.inner.root.join(id), old);
                }
            }
            return Err("storage_unavailable");
        }
        if let Some(updated) = next
            .entries
            .iter()
            .find(|entry| entry.match_id == match_id)
            .cloned()
        {
        if write_meta(&self.inner.root.join(match_id), &updated).is_err() {
                restore_bundle_evictions(&self.inner.root, &staged);
                if let Some(old) = state
                    .index
                    .entries
                    .iter()
                    .find(|entry| entry.match_id == match_id)
                {
                    let _ = write_meta(&self.inner.root.join(match_id), old);
                }
            return Err("storage_unavailable");
        }
        }
        if write_index(&self.inner.root, &next).is_err() {
            restore_bundle_evictions(&self.inner.root, &staged);
            for id in &evicted {
                if let Some(old) = state
                    .index
                    .entries
                    .iter()
                    .find(|entry| &entry.match_id == id)
                {
                    let _ = write_meta(&self.inner.root.join(id), old);
                }
            }
            if let Some(old) = state
                .index
                .entries
                .iter()
                .find(|entry| entry.match_id == match_id)
            {
                let _ = write_meta(&self.inner.root.join(match_id), old);
            }
            return Err("storage_unavailable");
        }
        state.index = next;
        drop(state);
        finish_bundle_evictions(&staged);
        log_evictions(evicted.len());
        self.emit_state_changed();
        Ok(())
    }

    pub fn open_bundle(&self, match_id: &str) -> Result<ReplayBundle, &'static str> {
        validate_match_id(match_id).map_err(|_| "invalid_match_id")?;
        let _operation = lock(&self.inner.operation);
        let entry = {
            let state = lock(&self.inner.state);
            if let Some(reason) = state.unavailable {
                return Err(reason);
            }
            state
                .index
                .entries
                .iter()
                .find(|entry| entry.match_id == match_id)
                .cloned()
                .ok_or("match_not_found")?
        };
        if entry.status != ReplayStatus::Ready {
            return Err("bundle_not_ready");
        }
        let expected_hash = entry.dem_sha256.as_deref().ok_or("bundle_invalid")?;
        engine::validate_library_bundle_dir(&self.inner.root.join(match_id), expected_hash)
            .map_err(|_| "bundle_invalid")?;
        let folder = self.inner.root.join(match_id);
        let manifest =
            read_json_limited(&folder.join("manifest.json"), engine::MAX_MANIFEST_BYTES)?;
        let match_data = read_json_limited(&folder.join("match.json"), engine::MAX_MATCH_BYTES)?;
        let world = read_json_limited(&folder.join("world.json"), engine::MAX_WORLD_BYTES)?;
        Ok(ReplayBundle {
            manifest,
            match_data,
            world,
        })
    }

    pub fn parse(&self, id_or_path: &str) -> Result<ReplayJob, &'static str> {
        let sender = self.sender.as_ref().ok_or("worker_unavailable")?;
        if !self.inner.worker_available.load(Ordering::Relaxed) {
            return Err("worker_unavailable");
        }
        if let Some(path) = lock(&self.inner.picked_files).remove(id_or_path) {
            self.queue_manual_path(sender, &path)
        } else if is_match_id(id_or_path) {
            self.queue_existing_match(sender, id_or_path)
        } else {
            self.queue_manual_path(sender, Path::new(id_or_path))
        }
    }

    pub fn reparse(&self, match_id: &str) -> Result<ReplayJob, &'static str> {
        let sender = self.sender.as_ref().ok_or("worker_unavailable")?;
        if !self.inner.worker_available.load(Ordering::Relaxed) {
            return Err("worker_unavailable");
        }
        let match_id = canonical_match_id(match_id).ok_or("invalid_match_id")?;
        let _operation = lock(&self.inner.operation);
        self.queue_existing_match_locked(sender, &match_id, ParseMode::Reparse)
    }

    fn queue_existing_match(
        &self,
        sender: &Sender<ParseRequest>,
        match_id: &str,
    ) -> Result<ReplayJob, &'static str> {
        self.queue_existing_match_with_mode(sender, match_id, false)
    }

    fn queue_existing_match_with_mode(
        &self,
        sender: &Sender<ParseRequest>,
        match_id: &str,
        automatic: bool,
    ) -> Result<ReplayJob, &'static str> {
        let match_id = canonical_match_id(match_id).ok_or("invalid_match_id")?;
        let _operation = lock(&self.inner.operation);
        self.queue_existing_match_locked(
            sender,
            &match_id,
            if automatic {
                ParseMode::Automatic
            } else {
                ParseMode::Manual
            },
        )
    }

    fn queue_existing_match_locked(
        &self,
        sender: &Sender<ParseRequest>,
        match_id: &str,
        mode: ParseMode,
    ) -> Result<ReplayJob, &'static str> {
        let automatic = mode.automatic_retry();
        if automatic && !self.inner.auto_analyse.load(Ordering::Acquire) {
            return Err("auto_parse_disabled");
        }
        let (source_path, source) = {
            let state = lock(&self.inner.state);
            if let Some(reason) = state.unavailable {
                return Err(reason);
            }
            let entry = state
                .index
                .entries
                .iter()
                .find(|entry| entry.match_id == match_id)
                .ok_or("match_not_found")?;
            if mode == ParseMode::Automatic
                && (entry.source != ReplaySource::DotaReplaysFolder
                    || entry.status != ReplayStatus::Available
                    || state.index.suppressed_watched_match_ids.iter().any(|id| id == match_id))
            {
                return Err("auto_parse_not_eligible");
            }
            if mode == ParseMode::Retry
                && (!entry.auto_queued || entry.status != ReplayStatus::Available)
            {
                return Err("retry_not_eligible");
            }
            match entry.status {
                ReplayStatus::Ready if mode == ParseMode::Reparse => {}
                ReplayStatus::Ready => return Err("already_ready"),
                ReplayStatus::Queued | ReplayStatus::Parsing => return Err("already_queued"),
                _ if mode == ParseMode::Reparse => return Err("bundle_not_ready"),
                _ => {}
            }
            if mode == ParseMode::Reparse && self.has_active_reparse(match_id) {
                return Err("already_queued");
            }
            let path = if mode == ParseMode::Reparse {
                let path =
                    source_path_from_meta(&self.inner.root, match_id).ok_or("source_missing")?;
                if !path.is_file() {
                    return Err("source_missing");
                }
                path
            } else {
                PathBuf::from(&entry.source_path)
            };
            (path, entry.source)
        };
        engine::validate_input_path(&source_path).map_err(|failure| {
            if mode == ParseMode::Reparse && !source_path.is_file() {
                "source_missing"
            } else {
                failure.code()
            }
        })?;
        let job = ReplayJob {
            job_id: next_job_id(),
            match_id: Some(match_id.to_owned()),
            operation: if mode == ParseMode::Reparse {
                ReplayOperation::Reparse
            } else {
                ReplayOperation::Parse
            },
            status: ReplayStatus::Queued,
        };
        if mode != ParseMode::Reparse {
            self.set_entry_auto_status_locked(&match_id, ReplayStatus::Queued, automatic)?;
        }
        self.record_job(&job, Some(match_id.to_owned()), mode);
        self.emit_state_changed();
        if sender
            .send(ParseRequest {
                job_id: job.job_id.clone(),
                match_id: Some(match_id.to_owned()),
                source,
                source_path,
                mode,
            })
            .is_err()
        {
            let failed = ReplayStatus::Failed {
                reason: "worker_unavailable".to_owned(),
            };
            if mode != ParseMode::Reparse {
                let _ = self.set_entry_auto_status_locked(&match_id, failed.clone(), false);
            }
            self.set_job_status(&job.job_id, failed);
            self.emit_state_changed();
            return Err("worker_unavailable");
        }
        Ok(job)
    }

    fn queue_manual_path(
        &self,
        sender: &Sender<ParseRequest>,
        path: &Path,
    ) -> Result<ReplayJob, &'static str> {
        engine::validate_input_path(path).map_err(|failure| failure.code())?;
        let source_path = path.canonicalize().map_err(|_| "invalid_input")?;
        let _operation = lock(&self.inner.operation);
        let job_id = next_job_id();
        let job = ReplayJob {
            job_id: job_id.clone(),
            match_id: None,
            operation: ReplayOperation::Parse,
            status: ReplayStatus::Queued,
        };
        let available_job = ReplayJob {
            job_id,
            match_id: None,
            operation: ReplayOperation::Parse,
            status: ReplayStatus::Available,
        };
        self.record_job(&available_job, None, ParseMode::Manual);
        self.emit_state_changed();
        self.set_job_status(&job.job_id, ReplayStatus::Queued);
        self.emit_state_changed();
        if sender
            .send(ParseRequest {
                job_id: job.job_id.clone(),
                match_id: None,
                source: ReplaySource::ManualPick,
                source_path,
                mode: ParseMode::Manual,
            })
            .is_err()
        {
            self.set_job_status(
                &job.job_id,
                ReplayStatus::Failed {
                    reason: "worker_unavailable".to_owned(),
                },
            );
            self.emit_state_changed();
            return Err("worker_unavailable");
        }
        Ok(job)
    }

    pub fn remove(&self, match_id: &str) -> Result<(), &'static str> {
        validate_match_id(match_id).map_err(|_| "invalid_match_id")?;
        let _operation = lock(&self.inner.operation);
        let removed = {
            let mut state = lock(&self.inner.state);
            if let Some(reason) = state.unavailable {
                return Err(reason);
            }
            let entry = state
                .index
                .entries
                .iter()
                .find(|entry| entry.match_id == match_id)
                .cloned()
                .ok_or("match_not_found")?;
            if entry.status == ReplayStatus::Parsing || self.reparse_is_parsing(match_id) {
                return Err("parse_in_progress");
            }
            let path = self.inner.root.join(match_id);
            let tombstone = self.inner.root.join(format!(
                ".remove-{}",
                NEXT_TOMBSTONE_ID.fetch_add(1, Ordering::Relaxed)
            ));
            if path.exists() {
                fs::rename(&path, &tombstone).map_err(|_| "storage_unavailable")?;
            }
            let mut next = state.index.clone();
            if entry.source == ReplaySource::DotaReplaysFolder
                && !next.suppressed_watched_match_ids.iter().any(|id| id == match_id)
            {
                next.suppressed_watched_match_ids.push(match_id.to_owned());
            }
            next.entries
                .retain(|candidate| candidate.match_id != match_id);
            if write_index(&self.inner.root, &next).is_err() {
                if tombstone.exists() {
                    let _ = fs::rename(&tombstone, &path);
                }
                return Err("storage_unavailable");
            }
            state.index = next.clone();
            tombstone
        };
        cancel_jobs_for_match(&self.inner.jobs, match_id);
        if removed.exists() {
            let _ = fs::remove_dir_all(removed);
        }
        self.emit_state_changed();
        Ok(())
    }

    pub fn clear(&self) -> Result<(), &'static str> {
        let _operation = lock(&self.inner.operation);
        let tombstones = {
            let mut state = lock(&self.inner.state);
            if let Some(reason) = state.unavailable {
                return Err(reason);
            }
            let parsing_job = lock(&self.inner.jobs)
                .iter()
                .any(|record| record.public.status == ReplayStatus::Parsing);
            if parsing_job
                || state
                    .index
                    .entries
                    .iter()
                    .any(|entry| entry.status == ReplayStatus::Parsing)
            {
                return Err("parse_in_progress");
            }
            let old_entries = state.index.entries.clone();
            let mut next = ReplayIndex::default();
            next.suppressed_watched_match_ids = state
                .index
                .suppressed_watched_match_ids
                .clone();
            for entry in &old_entries {
                if entry.source == ReplaySource::DotaReplaysFolder
                    && !next.suppressed_watched_match_ids.contains(&entry.match_id)
                {
                    next.suppressed_watched_match_ids.push(entry.match_id.clone());
                }
            }
            let mut tombstones = Vec::new();
            for entry in &old_entries {
                let path = self.inner.root.join(&entry.match_id);
                if path.exists() {
                    let tombstone = self.inner.root.join(format!(
                        ".clear-{}",
                        NEXT_TOMBSTONE_ID.fetch_add(1, Ordering::Relaxed)
                    ));
                    if fs::rename(&path, &tombstone).is_err() {
                        restore_tombstones(&tombstones);
                        return Err("storage_unavailable");
                    }
                    tombstones.push((path, tombstone));
                }
            }
            if write_index(&self.inner.root, &next).is_err() {
                restore_tombstones(&tombstones);
                return Err("storage_unavailable");
            }
            state.index = next;
            tombstones
        };
        for (job_id, _) in lock(&self.inner.jobs)
            .iter()
            .filter(|record| record.public.status == ReplayStatus::Queued)
            .map(|record| (record.public.job_id.clone(), record.match_id.clone()))
            .collect::<Vec<_>>()
        {
            self.set_job_status(
                &job_id,
                ReplayStatus::Failed {
                    reason: "cancelled".to_owned(),
                },
            );
        }
        for (_, tombstone) in tombstones {
            let _ = fs::remove_dir_all(tombstone);
        }
        self.emit_state_changed();
        Ok(())
    }

    fn record_watcher_state(&self, folder_found: bool, error: Option<&'static str>) {
        *lock(&self.inner.watcher_state) = ReplayWatcherState {
            folder_found: Some(folder_found),
            last_scan_unix_ms: Some(now_unix_ms()),
            last_error_code: error.map(str::to_owned),
        };
    }

    fn watch_replay_folder(&self, folder: &Path) -> Result<usize, &'static str> {
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(_) => {
                lock(&self.inner.watched_sizes).clear();
                lock(&self.inner.stable_watched_match_ids).clear();
                self.record_watcher_state(false, Some("replay_folder_unavailable"));
                eprintln!(
                    "{}",
                    watcher_summary_log_line(0, 0, 0, Some("replay_folder_unavailable"))
                );
                return Err("replay_folder_unavailable");
            }
        };
        let mut watched = Vec::new();
        let mut observed = HashSet::new();
        let mut stable = HashSet::new();
        let mut scan_error = None;
        for item in entries {
            let item = match item {
                Ok(item) => item,
                Err(_) => {
                    scan_error.get_or_insert("replay_folder_entry_unavailable");
                    continue;
                }
            };
            let is_file = match item.file_type() {
                Ok(file_type) => file_type.is_file(),
                Err(_) => {
                    scan_error.get_or_insert("replay_file_metadata_unavailable");
                    continue;
                }
            };
            if !is_file {
                continue;
            }
            let path = item.path();
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("dem"))
            {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Some(match_id) = canonical_match_id(stem) else {
                continue;
            };
            let metadata = match item.metadata() {
                Ok(metadata) => metadata,
                Err(_) => {
                    scan_error.get_or_insert("replay_file_metadata_unavailable");
                    continue;
                }
            };
            if metadata.len() == 0 || metadata.len() > engine::MAX_DEM_BYTES {
                continue;
            }
            observed.insert(match_id.clone());
            let stable_now = {
                let mut sizes = lock(&self.inner.watched_sizes);
                let polls = match sizes.get(&match_id) {
                    Some((previous_size, previous_polls)) if *previous_size == metadata.len() => {
                        previous_polls.saturating_add(1)
                    }
                    _ => 1,
                };
                sizes.insert(match_id.clone(), (metadata.len(), polls));
                polls >= 2
            };
            if stable_now {
                stable.insert(match_id.clone());
            }
            let modified = metadata.modified().ok().and_then(system_time_unix_ms);
            watched.push((modified, match_id, path));
        }
        {
            let mut sizes = lock(&self.inner.watched_sizes);
            sizes.retain(|match_id, _| observed.contains(match_id));
            if scan_error.is_some() {
                for (match_id, (_, polls)) in sizes.iter_mut() {
                    if observed.contains(match_id) {
                        *polls = 1;
                    }
            }
                stable.clear();
        }
        }
        *lock(&self.inner.stable_watched_match_ids) = stable.clone();
        watched.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));

        let mut discovered = 0;
        for (_, match_id, path) in watched {
            match self.add_available(match_id, &path, ReplaySource::DotaReplaysFolder) {
                Ok(true) => discovered += 1,
                Ok(false) => {}
                Err(error) => {
                    scan_error.get_or_insert(error);
            }
        }
    }

        let mut queued = 0;
        if self.inner.auto_analyse.load(Ordering::Acquire) && scan_error.is_none() {
        let _operation = lock(&self.inner.operation);
            match self.queue_available_auto_backlog_locked() {
                Ok(count) => queued = count,
                Err(error) => {
                    scan_error.get_or_insert(error);
        }
            }
        }
        if scan_error.is_some() {
            let mut sizes = lock(&self.inner.watched_sizes);
            for (match_id, (_, polls)) in sizes.iter_mut() {
                if observed.contains(match_id) {
                    *polls = 1;
                }
        }
            lock(&self.inner.stable_watched_match_ids).clear();
        }
        self.record_watcher_state(true, scan_error);
        eprintln!(
            "{}",
            watcher_summary_log_line(discovered, stable.len(), queued, scan_error)
        );
        Ok(discovered)
    }

    fn queue_available_auto_backlog_locked(&self) -> Result<usize, &'static str> {
        if !self.inner.auto_analyse.load(Ordering::Acquire) {
            return Ok(0);
        }
        let Some(sender) = self.sender.as_ref() else {
            return Ok(0);
        };
        let mut available = {
            let state = lock(&self.inner.state);
            if let Some(reason) = state.unavailable {
                return Err(reason);
            }
            state
                .index
                .entries
                .iter()
                .filter(|entry| {
                    entry.status == ReplayStatus::Available
                        && ((entry.source == ReplaySource::DotaReplaysFolder
                            && lock(&self.inner.stable_watched_match_ids)
                                .contains(&entry.match_id))
                            || (entry.source == ReplaySource::ManualPick && entry.auto_queued))
                        && !state
                            .index
                            .suppressed_watched_match_ids
                            .iter()
                            .any(|id| id == &entry.match_id)
                })
                .map(|entry| {
                    (
                        source_modified_unix_ms(&entry.source_path)
                            .or(Some(entry.created_at_unix_ms)),
                        entry.match_id.clone(),
                        entry.auto_queued,
                    )
                })
                .collect::<Vec<_>>()
        };
        available.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
        let slots = self.remaining_auto_slots();
        let mut queued = 0;
        for (_, match_id, retry) in available.into_iter().take(slots) {
            if self
                .queue_existing_match_locked(
                    sender,
                    &match_id,
                    if retry {
                        ParseMode::Retry
                    } else {
                        ParseMode::Automatic
                    },
                )
                .is_ok()
            {
                queued += 1;
            }
        }
        if queued > 0 {
            self.emit_state_changed();
        }
        Ok(queued)
    }

    fn remaining_auto_slots(&self) -> usize {
        let limit = lock(&self.inner.limits).max_matches;
        let active_auto = lock(&self.inner.state)
            .index
            .entries
            .iter()
            .filter(|entry| {
                entry.auto_queued
                    && matches!(entry.status, ReplayStatus::Queued | ReplayStatus::Parsing)
            })
            .count();
        limit.saturating_sub(active_auto)
    }

    fn cancel_queued_auto_job(&self, match_id: &str) {
        let job_ids = lock(&self.inner.jobs)
            .iter()
            .filter(|record| {
            record.match_id.as_deref() == Some(match_id)
                && record.public.status == ReplayStatus::Queued
            })
            .map(|record| record.public.job_id.clone())
            .collect::<Vec<_>>();
        for job_id in job_ids {
            self.set_job_status(
                &job_id,
                ReplayStatus::Failed {
                reason: "cancelled".to_owned(),
                },
            );
        }
    }

    fn add_available(
        &self,
        match_id: String,
        path: &Path,
        source: ReplaySource,
    ) -> Result<bool, &'static str> {
        let _operation = lock(&self.inner.operation);
        let mut state = lock(&self.inner.state);
        if let Some(reason) = state.unavailable {
            return Err(reason);
        }
        if state
            .index
            .entries
            .iter()
            .any(|entry| entry.match_id == match_id)
        {
            return Ok(false);
        }
        if source == ReplaySource::DotaReplaysFolder
            && state
                .index
                .suppressed_watched_match_ids
                .iter()
                .any(|id| id == &match_id)
        {
            return Ok(false);
        }
        let mut entry = ReplayLibraryEntry {
            match_id: match_id.clone(),
            source,
            source_path: path.to_string_lossy().into_owned(),
            dem_sha256: None,
            engine_version: None,
            engine_sha256: None,
            parse_time_unix_ms: None,
            status: ReplayStatus::Available,
            bundle_bytes: 0,
            storage_bytes: 0,
            created_at_unix_ms: now_unix_ms(),
            match_date_utc: None,
            own_hero: None,
            duration_s: None,
            patch: None,
            pinned: false,
            auto_queued: false,
        };
        let target = self.inner.root.join(&match_id);
        if target.exists() {
            return Err("storage_conflict");
        }
        let stage = self.create_staging_dir("available")?;
        entry.storage_bytes = match entry_meta_bytes(&entry) {
            Ok(bytes) => bytes.len() as u64,
            Err(error) => {
                let _ = fs::remove_dir_all(&stage);
                return Err(error);
            }
        };
        if let Err(error) = write_meta(&stage, &entry) {
            let _ = fs::remove_dir_all(&stage);
            return Err(error);
        }
        let mut next = state.index.clone();
        next.entries.push(entry.clone());
        let evicted = match self.trim_to_limits(&mut next, &match_id) {
            Ok(evicted) => evicted,
            Err(error) => {
                let _ = fs::remove_dir_all(&stage);
                return Err(error);
            }
        };
        let staged = match stage_bundle_evictions(&self.inner.root, &evicted) {
            Ok(staged) => staged,
            Err(error) => {
                let _ = fs::remove_dir_all(&stage);
                return Err(error);
            }
        };
        if write_evicted_metadata(&self.inner.root, &next, &evicted).is_err() {
            restore_bundle_evictions(&self.inner.root, &staged);
            for id in &evicted {
                if let Some(old) = state
                    .index
                    .entries
                    .iter()
                    .find(|entry| &entry.match_id == id)
                {
                    let _ = write_meta(&self.inner.root.join(id), old);
            }
        }
            let _ = fs::remove_dir_all(&stage);
            return Err("storage_unavailable");
        }
        if fs::rename(&stage, &target).is_err() {
            restore_bundle_evictions(&self.inner.root, &staged);
            for id in &evicted {
                if let Some(old) = state
                    .index
                    .entries
                    .iter()
                    .find(|entry| &entry.match_id == id)
                {
                    let _ = write_meta(&self.inner.root.join(id), old);
                }
            }
            let _ = fs::remove_dir_all(stage);
            return Err("storage_unavailable");
        }
        if write_index(&self.inner.root, &next).is_err() {
            let _ = fs::remove_dir_all(&target);
            restore_bundle_evictions(&self.inner.root, &staged);
            for id in &evicted {
                if let Some(old) = state
                    .index
                    .entries
                    .iter()
                    .find(|entry| &entry.match_id == id)
                {
                    let _ = write_meta(&self.inner.root.join(id), old);
                }
            }
            return Err("storage_unavailable");
        }
        state.index = next;
        drop(state);
        finish_bundle_evictions(&staged);
        log_evictions(evicted.len());
        self.emit_state_changed();
        Ok(true)
    }

    fn process_request(&self, request: ParseRequest) {
        if !self.request_is_queued(&request) {
            return;
        }
        while (self.inner.match_active)() {
            if !self.request_is_queued(&request) {
                return;
            }
            thread::sleep(MATCH_IDLE_POLL);
        }
        if !self.request_is_queued(&request) {
            return;
        }
        let transition = {
            let _operation = lock(&self.inner.operation);
            if !self.request_is_queued(&request) {
                return;
            }
            let result = if request.mode == ParseMode::Reparse {
                Ok(())
            } else {
                request.match_id.as_deref().map_or(Ok(()), |match_id| {
                    self.set_entry_auto_status_locked(
                        match_id,
                        ReplayStatus::Parsing,
                        request.mode.automatic(),
                    )
                })
            };
            if result.is_ok() {
                self.set_job_status(&request.job_id, ReplayStatus::Parsing);
            }
            result
        };
        if let Err(reason) = transition {
            self.finish_job_failure(
                &request,
                ReplayStatus::Failed {
                    reason: reason.to_owned(),
                },
            );
            self.emit_state_changed();
            return;
        }
        self.emit_state_changed();

        let result = self.parse_and_store(&request);
        match result {
            Ok(match_id) => {
                self.set_job_match_id(&request.job_id, &match_id);
                self.set_job_status(&request.job_id, ReplayStatus::Ready);
            }
            Err(engine::EngineFailure::NeedsPack) => {
                self.finish_job_failure(&request, ReplayStatus::NeedsPack);
            }
            Err(engine::EngineFailure::MatchBecameActive) => {
                self.retry_after_match_start(&request);
            }
            Err(failure) => {
                self.finish_job_failure(
                    &request,
                    ReplayStatus::Failed {
                        reason: failure.code().to_owned(),
                    },
                );
            }
        }
        self.emit_state_changed();
    }

    fn parse_and_store(&self, request: &ParseRequest) -> Result<String, engine::EngineFailure> {
        if request.mode == ParseMode::Reparse && !request.source_path.is_file() {
            return Err(engine::EngineFailure::SourceMissing);
        }
        engine::validate_input_path(&request.source_path).map_err(|failure| {
            if request.mode == ParseMode::Reparse && !request.source_path.is_file() {
                engine::EngineFailure::SourceMissing
            } else {
                failure
            }
        })?;
        let dem_sha256 = engine::sha256_file(&request.source_path).map_err(|_| {
            if request.mode == ParseMode::Reparse && !request.source_path.is_file() {
                engine::EngineFailure::SourceMissing
            } else {
                engine::EngineFailure::InvalidInput
            }
        })?;
        let stage = self
            .create_staging_dir(&request.job_id)
            .map_err(|_| engine::EngineFailure::StorageConflict)?;
        let development_engine = development_engine_path(self.inner.allow_development_engine);
        let run = self.inner.engine_runner.run(
            self.inner.bundled_engine.as_deref(),
            development_engine.as_deref(),
            self.inner.allow_development_engine,
            &request.source_path,
            &stage,
            &*self.inner.match_active,
        );
        let run = match run {
            Ok(run) => run,
            Err(failure) => {
                let _ = fs::remove_dir_all(stage);
                return Err(if request.mode == ParseMode::Reparse
                    && !request.source_path.is_file()
                {
                    engine::EngineFailure::SourceMissing
                } else {
                    failure
                    },
                );
            }
        };
        if request.mode == ParseMode::Reparse && !request.source_path.is_file() {
            let _ = fs::remove_dir_all(stage);
            return Err(engine::EngineFailure::SourceMissing);
        }
        if let Some(code) = run.stderr_code {
            crate::log_info!(
                "replay",
                "engine_stderr",
                "replay engine emitted stderr",
                code = code
            );
        }
        let validated = match engine::validate_bundle_dir(&stage, &dem_sha256) {
            Ok(validated) => validated,
            Err(failure) => {
                let _ = fs::remove_dir_all(stage);
                return Err(failure);
            }
        };
        if validated.engine_version != run.engine_version
            || request
                .match_id
                .as_deref()
                .is_some_and(|expected| expected != validated.match_id)
        {
            let _ = fs::remove_dir_all(stage);
            return Err(engine::EngineFailure::InvalidBundle);
        }
        let match_data = read_json_limited(&stage.join("match.json"), engine::MAX_MATCH_BYTES)
            .map_err(|_| engine::EngineFailure::InvalidBundle)?;
        let (match_date_utc, own_hero, duration_s, patch) = library_match_metadata(&match_data);

        let existing = lock(&self.inner.state)
            .index
            .entries
            .iter()
            .find(|entry| entry.match_id == validated.match_id)
            .cloned();
        let created_at = existing
            .as_ref()
            .map(|entry| entry.created_at_unix_ms)
            .unwrap_or_else(now_unix_ms);
        let mut entry = ReplayLibraryEntry {
            match_id: validated.match_id.clone(),
            source: request.source,
            source_path: request.source_path.to_string_lossy().into_owned(),
            dem_sha256: Some(dem_sha256),
            engine_version: Some(run.engine_version),
            engine_sha256: Some(run.engine_sha256),
            parse_time_unix_ms: Some(now_unix_ms()),
            status: ReplayStatus::Ready,
            bundle_bytes: validated.bundle_bytes,
            storage_bytes: 0,
            created_at_unix_ms: created_at,
            match_date_utc,
            own_hero,
            duration_s,
            patch,
            pinned: false,
            auto_queued: false,
        };
        entry.pinned = existing.as_ref().is_some_and(|entry| entry.pinned);
        entry.storage_bytes = entry.bundle_bytes
            + entry_meta_bytes(&entry)
                .map_err(|_| engine::EngineFailure::StorageConflict)?
                .len() as u64;
        if write_meta(&stage, &entry).is_err() {
            let _ = fs::remove_dir_all(stage);
            return Err(engine::EngineFailure::StorageConflict);
        }
        if let Err(failure) =
            self.install_parsed_bundle(&stage, entry.clone(), request.mode == ParseMode::Reparse)
        {
            let _ = fs::remove_dir_all(stage);
            return Err(failure);
        }
        Ok(validated.match_id)
    }

    fn install_parsed_bundle(
        &self,
        stage: &Path,
        entry: ReplayLibraryEntry,
        replace_ready: bool,
    ) -> Result<(), engine::EngineFailure> {
        let _operation = lock(&self.inner.operation);
        let mut state = lock(&self.inner.state);
        if state.unavailable.is_some() {
            return Err(engine::EngineFailure::StorageConflict);
        }
        let existing = state
            .index
            .entries
            .iter()
            .find(|candidate| candidate.match_id == entry.match_id)
            .cloned();
        if existing.as_ref().is_some_and(|candidate| {
            (candidate.status == ReplayStatus::Ready && !replace_ready)
                || (replace_ready && candidate.status != ReplayStatus::Ready)
        }) || (replace_ready && existing.is_none())
        {
            return Err(engine::EngineFailure::StorageConflict);
        }
        let target = self.inner.root.join(&entry.match_id);
        if existing.is_none() && target.exists() {
            return Err(engine::EngineFailure::StorageConflict);
        }

        let mut next = state.index.clone();
        next.entries
            .retain(|candidate| candidate.match_id != entry.match_id);
        next.entries.push(entry.clone());
        let evicted = self
            .trim_to_limits(&mut next, &entry.match_id)
            .map_err(|_| engine::EngineFailure::OutputTooLarge)?;

        let mut tombstones = Vec::new();
        if existing.is_some() && target.exists() {
            if let Some(tombstone) = move_path_to_tombstone(&target, &self.inner.root) {
                tombstones.push(tombstone);
            } else {
                return Err(engine::EngineFailure::StorageConflict);
            }
        }
        let staged = match stage_bundle_evictions(&self.inner.root, &evicted) {
            Ok(staged) => staged,
            Err(_) => {
                restore_tombstones(&tombstones);
                return Err(engine::EngineFailure::StorageConflict);
            }
        };
        if write_evicted_metadata(&self.inner.root, &next, &evicted).is_err() {
            restore_bundle_evictions(&self.inner.root, &staged);
            restore_tombstones(&tombstones);
            for id in &evicted {
                if let Some(old) = state
                    .index
                    .entries
                    .iter()
                    .find(|candidate| &candidate.match_id == id)
                {
                    let _ = write_meta(&self.inner.root.join(id), old);
            }
        }
            return Err(engine::EngineFailure::StorageConflict);
        }
        if fs::rename(stage, &target).is_err() {
            restore_bundle_evictions(&self.inner.root, &staged);
            restore_tombstones(&tombstones);
            return Err(engine::EngineFailure::StorageConflict);
        }
        if write_index(&self.inner.root, &next).is_err() {
            let _ = fs::remove_dir_all(&target);
            restore_bundle_evictions(&self.inner.root, &staged);
            restore_tombstones(&tombstones);
            for id in &evicted {
                if let Some(old) = state
                    .index
                    .entries
                    .iter()
                    .find(|candidate| &candidate.match_id == id)
                {
                    let _ = write_meta(&self.inner.root.join(id), old);
                }
            }
            return Err(engine::EngineFailure::StorageConflict);
        }
        state.index = next;
        drop(state);
        finish_bundle_evictions(&staged);
        log_evictions(evicted.len());
        for (_, tombstone) in tombstones {
            let _ = fs::remove_dir_all(tombstone);
        }
        Ok(())
    }

    fn trim_to_limits(
        &self,
        index: &mut ReplayIndex,
        protected_match_id: &str,
    ) -> Result<Vec<String>, &'static str> {
        self.trim_to_limits_with(index, protected_match_id, *lock(&self.inner.limits))
    }

    fn trim_to_limits_with(
        &self,
        index: &mut ReplayIndex,
        protected_match_id: &str,
        limits: ReplayLibraryLimits,
    ) -> Result<Vec<String>, &'static str> {
        if index
            .entries
            .iter()
            .find(|entry| entry.match_id == protected_match_id)
            .is_some_and(|entry| entry.bundle_bytes > limits.max_bytes)
        {
            return Err("output_too_large");
        }
        let pinned_usage = index
            .entries
            .iter()
            .filter(|entry| entry.pinned && entry.status == ReplayStatus::Ready)
            .fold(0u64, |total, entry| {
                total.saturating_add(entry.bundle_bytes)
            });
        let mut evicted = Vec::new();
        loop {
            let unpinned_ready = index.entries.iter()
                .filter(|entry| !entry.pinned && entry.status == ReplayStatus::Ready).count();
            let over_count = unpinned_ready > limits.max_matches;
            let over_size = index_usage(index) > limits.max_bytes;
            if !over_count && !over_size { break; }
            let Some(oldest) = index
                .entries
                .iter()
                .filter(|entry| {
                    !entry.pinned
                        && entry.match_id != protected_match_id
                        && !self.has_active_reparse(&entry.match_id)
                        && entry.status == ReplayStatus::Ready
                })
                .min_by_key(|entry| replay_recency(entry))
                .map(|entry| entry.match_id.clone())
            else {
                    // Pinned bundles are user-retained data. If they alone exceed the cap,
                    // preserve them and expose actual usage so the UI can show over-cap state.
                    if pinned_usage > limits.max_bytes { break; }
                    return Err("output_too_large");
                };
            if let Some(entry) = index
                .entries
                .iter_mut()
                .find(|entry| entry.match_id == oldest)
            {
                entry.status = ReplayStatus::Evicted;
                entry.bundle_bytes = 0;
                entry.auto_queued = false;
                entry.storage_bytes = entry_meta_bytes(entry)?.len() as u64;
            }
            evicted.push(oldest);
        }
        Ok(evicted)
    }

    fn create_staging_dir(&self, suffix: &str) -> Result<PathBuf, &'static str> {
        let safe_suffix: String = suffix
            .bytes()
            .filter(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
            .map(char::from)
            .take(64)
            .collect();
        let path = self.inner.staging.join(format!(
            "job-{}-{}-{}",
            std::process::id(),
            NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed),
            safe_suffix
        ));
        fs::create_dir(&path).map_err(|_| "storage_unavailable")?;
        Ok(path)
    }

    fn set_entry_status(&self, match_id: &str, status: ReplayStatus) -> Result<(), &'static str> {
        let _operation = lock(&self.inner.operation);
        self.set_entry_status_locked(match_id, status)
    }

    fn set_entry_status_locked(
        &self,
        match_id: &str,
        status: ReplayStatus,
    ) -> Result<(), &'static str> {
        self.set_entry_auto_status_locked(match_id, status, false)
    }

    fn set_entry_auto_status_locked(
        &self,
        match_id: &str,
        status: ReplayStatus,
        auto_queued: bool,
    ) -> Result<(), &'static str> {
        let mut state = lock(&self.inner.state);
        if let Some(reason) = state.unavailable {
            return Err(reason);
        }
        let mut next = state.index.clone();
        let entry = next
            .entries
            .iter_mut()
            .find(|entry| entry.match_id == match_id)
            .ok_or("match_not_found")?;
        entry.status = status;
        entry.auto_queued = auto_queued;
        entry.storage_bytes = entry.bundle_bytes + entry_meta_bytes(entry)?.len() as u64;
        write_meta(&self.inner.root.join(match_id), entry)?;
        write_index(&self.inner.root, &next)?;
        state.index = next;
        Ok(())
    }

    fn record_job(&self, job: &ReplayJob, match_id: Option<String>, mode: ParseMode) {
        let mut jobs = lock(&self.inner.jobs);
        if jobs.len() >= MAX_JOB_HISTORY {
            jobs.pop_front();
        }
        let mut public = job.clone();
        public.match_id = match_id.clone();
        public.operation = if mode == ParseMode::Reparse {
            ReplayOperation::Reparse
        } else {
            ReplayOperation::Parse
        };
        let reason = match mode {
            ParseMode::Automatic => "automatic_queue",
            ParseMode::Retry => "automatic_retry",
            ParseMode::Reparse => "reparse_queue",
            ParseMode::Manual => "manual_queue",
        };
        eprintln!(
            "{}",
            job_transition_log_line(
                "none",
                state_code(&public.status),
                reason,
                0,
                operation_code(public.operation)
            )
        );
        jobs.push_back(JobRecord {
            public,
            match_id,
            mode,
            queued_at: Instant::now(),
        });
    }

    fn set_job_match_id(&self, job_id: &str, match_id: &str) {
        if let Some(record) = lock(&self.inner.jobs)
            .iter_mut()
            .find(|record| record.public.job_id == job_id)
        {
            record.match_id = Some(match_id.to_owned());
            record.public.match_id = Some(match_id.to_owned());
        }
    }

    fn set_job_status(&self, job_id: &str, status: ReplayStatus) {
        let transition = {
            let mut jobs = lock(&self.inner.jobs);
            let Some(record) = jobs
            .iter_mut()
            .find(|record| record.public.job_id == job_id)
            else {
                return;
            };
            if record.public.status == status {
                return;
            }
            let from = state_code(&record.public.status).to_owned();
            let to = state_code(&status).to_owned();
            let reason = status_reason_code(&status).to_owned();
            let duration = record
                .queued_at
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64;
            let operation = operation_code(record.public.operation);
            record.public.status = status;
            Some((from, to, reason, duration, operation))
        };
        if let Some((from, to, reason, duration, operation)) = transition {
            eprintln!(
                "{}",
                job_transition_log_line(&from, &to, &reason, duration, operation)
            );
        }
    }

    fn request_is_queued(&self, request: &ParseRequest) -> bool {
        if let Some(match_id) = &request.match_id {
            if request.mode.automatic_retry() && !self.inner.auto_analyse.load(Ordering::Acquire) {
                return false;
            }
            if request.mode == ParseMode::Reparse {
                let job_queued = lock(&self.inner.jobs).iter().any(|record| {
                    record.public.job_id == request.job_id
                        && record.mode == ParseMode::Reparse
                        && record.public.status == ReplayStatus::Queued
                });
                return job_queued
                    && lock(&self.inner.state).index.entries.iter().any(|entry| {
                        entry.match_id == *match_id && entry.status == ReplayStatus::Ready
                    });
            }
            return lock(&self.inner.state).index.entries.iter().any(|entry| {
                entry.match_id == *match_id
                    && entry.status == ReplayStatus::Queued
                    && (!request.mode.automatic_retry() || entry.auto_queued)
            });
        }
        lock(&self.inner.jobs).iter().any(|record| {
            record.public.job_id == request.job_id && record.public.status == ReplayStatus::Queued
        })
    }

    fn finish_job_failure(&self, request: &ParseRequest, status: ReplayStatus) {
        if request.mode != ParseMode::Reparse {
            if let Some(match_id) = &request.match_id {
                let _ = self.set_entry_auto_status_locked(match_id, status.clone(), false);
            }
        }
        self.set_job_status(&request.job_id, status);
    }

    fn retry_after_match_start(&self, request: &ParseRequest) {
        let Some(sender) = self.sender.as_ref() else {
            self.set_job_status(
                &request.job_id,
                ReplayStatus::Failed {
                    reason: "worker_unavailable".to_owned(),
                },
            );
            return;
        };
        let _operation = lock(&self.inner.operation);
        if let Some(match_id) = &request.match_id {
            if request.mode != ParseMode::Reparse {
                let _ = self.set_entry_auto_status_locked(match_id, ReplayStatus::Available, false);
                let retry_later = request.mode.automatic_retry()
                    && !self.inner.auto_analyse.load(Ordering::Acquire);
                if retry_later {
                    self.set_retry_pending_locked(match_id);
                }
            }
        }
        self.set_job_status(
            &request.job_id,
            ReplayStatus::Failed {
                reason: "match_active".to_owned(),
            },
        );
        self.set_job_status(&request.job_id, ReplayStatus::Available);
        if request.mode.automatic_retry() && !self.inner.auto_analyse.load(Ordering::Acquire) {
            self.emit_state_changed();
            return;
        }
        if let Some(match_id) = &request.match_id {
            if request.mode != ParseMode::Reparse {
                if self
                    .set_entry_auto_status_locked(
                        match_id,
                        ReplayStatus::Queued,
                        request.mode.automatic_retry(),
                    )
                    .is_err()
                {
                    self.set_job_status(
                        &request.job_id,
                        ReplayStatus::Failed {
                            reason: "storage_unavailable".to_owned(),
                        },
                    );
                    self.emit_state_changed();
                    return;
                }
            }
        }
        self.set_job_status(&request.job_id, ReplayStatus::Queued);
        if sender.send(request.clone()).is_err() {
            if let Some(match_id) = &request.match_id {
                if request.mode != ParseMode::Reparse {
                    let _ =
                        self.set_entry_auto_status_locked(match_id, ReplayStatus::Available, false);
                }
            }
            self.set_job_status(
                &request.job_id,
                ReplayStatus::Failed {
                    reason: "worker_unavailable".to_owned(),
                },
            );
        }
        self.emit_state_changed();
    }

    fn set_retry_pending_locked(&self, match_id: &str) {
        let mut state = lock(&self.inner.state);
        if state.unavailable.is_some() {
            return;
        }
        let mut next = state.index.clone();
        let Some(entry) = next
            .entries
            .iter_mut()
            .find(|entry| entry.match_id == match_id)
        else {
            return;
        };
        entry.auto_queued = true;
        entry.storage_bytes = entry.bundle_bytes
            + entry_meta_bytes(entry)
                .map(|bytes| bytes.len() as u64)
                .unwrap_or(0);
        if write_meta(&self.inner.root.join(match_id), entry).is_err()
            || write_index(&self.inner.root, &next).is_err()
        {
            return;
        }
        state.index = next;
    }

    fn has_active_reparse(&self, match_id: &str) -> bool {
        lock(&self.inner.jobs).iter().any(|record| {
            record.mode == ParseMode::Reparse
                && record.match_id.as_deref() == Some(match_id)
                && matches!(
                    record.public.status,
                    ReplayStatus::Queued | ReplayStatus::Parsing
                )
        })
    }

    fn reparse_is_parsing(&self, match_id: &str) -> bool {
        lock(&self.inner.jobs).iter().any(|record| {
            record.mode == ParseMode::Reparse
                && record.match_id.as_deref() == Some(match_id)
                && record.public.status == ReplayStatus::Parsing
        })
    }

    fn emit_state_changed(&self) {
        (self.inner.emit_state_changed)();
    }

    fn watch_replay_folder_for_test(&self, folder: &Path) -> Result<usize, &'static str> {
        self.watch_replay_folder(folder)
    }
}

fn development_engine_path(allowed: bool) -> Option<PathBuf> {
    if !allowed {
        return None;
    }
    std::env::var_os("KOHBOR_REPLAY_ENGINE").map(PathBuf::from)
}

fn load_index(root: &Path) -> (ReplayIndex, Option<&'static str>) {
    let path = root.join("index.json");
    let bytes = match fs::metadata(&path) {
        Ok(metadata) if metadata.len() > MAX_INDEX_BYTES => {
            return rebuild_corrupt_index(root, &path);
        }
        Ok(_) => match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => return (ReplayIndex::default(), Some("storage_unavailable")),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (ReplayIndex::default(), None);
        }
        Err(_) => return (ReplayIndex::default(), Some("storage_unavailable")),
    };
    let Ok(index) = serde_json::from_slice::<ReplayIndex>(&bytes) else {
        return rebuild_corrupt_index(root, &path);
    };
    let mut seen = HashSet::new();
    let mut seen_suppressed = HashSet::new();
    if index.schema_version != INDEX_SCHEMA_VERSION
        || index.entries.iter().any(|entry| {
            canonical_match_id(&entry.match_id).as_deref() != Some(entry.match_id.as_str())
                || !seen.insert(entry.match_id.as_str())
        })
        || index.suppressed_watched_match_ids.iter().any(|match_id| {
            canonical_match_id(match_id).as_deref() != Some(match_id.as_str())
                || !seen_suppressed.insert(match_id.as_str())
        })
    {
        return rebuild_corrupt_index(root, &path);
    }
    (index, None)
}

fn rebuild_corrupt_index(root: &Path, index_path: &Path) -> (ReplayIndex, Option<&'static str>) {
    if index_path.exists() && backup_invalid_index(index_path).is_err() {
        return (ReplayIndex::default(), Some("index_invalid"));
    }
    let mut index = match rebuild_index_from_metadata(root) {
        Ok(index) => index,
        Err(reason) => return (ReplayIndex::default(), Some(reason)),
    };
    if write_index(root, &index).is_err() {
        return (ReplayIndex::default(), Some("storage_unavailable"));
    }
    index.schema_version = INDEX_SCHEMA_VERSION;
    (index, None)
}

fn backup_invalid_index(index_path: &Path) -> Result<(), &'static str> {
    let parent = index_path.parent().ok_or("storage_unavailable")?;
    let mut backup = parent.join(format!("index.invalid-{}.json", now_unix_ms()));
    while backup.exists() {
        backup = parent.join(format!(
            "index.invalid-{}-{}.json",
            now_unix_ms(),
            NEXT_TOMBSTONE_ID.fetch_add(1, Ordering::Relaxed)
        ));
    }
    fs::rename(index_path, backup).map_err(|_| "storage_unavailable")
}

fn rebuild_index_from_metadata(root: &Path) -> Result<ReplayIndex, &'static str> {
    let mut index = ReplayIndex::default();
    let directories = fs::read_dir(root).map_err(|_| "storage_unavailable")?;
    let mut seen = HashSet::new();
    for item in directories {
        let item = item.map_err(|_| "storage_unavailable")?;
        let kind = item.file_type().map_err(|_| "storage_unavailable")?;
        if !kind.is_dir() {
            continue;
        }
        let name = item.file_name();
        let Some(name) = name.to_str() else { continue };
        if canonical_match_id(name).as_deref() != Some(name) || !seen.insert(name.to_owned()) {
            continue;
        }
        let directory = item.path();
        let meta_path = directory.join("meta.json");
        if !meta_path.exists() {
            continue;
        }
        let mut entry: ReplayLibraryEntry = read_json_limited(&meta_path, MAX_META_BYTES)
            .and_then(|value| serde_json::from_value(value).map_err(|_| "index_invalid"))?;
        if entry.match_id != name {
            return Err("index_invalid");
        }
        let metadata = fs::metadata(&meta_path).map_err(|_| "storage_unavailable")?;
        if entry.created_at_unix_ms == 0 {
            entry.created_at_unix_ms = metadata
                .modified()
                .ok()
                .and_then(system_time_unix_ms)
                .unwrap_or(0);
        }
        let mut bundle_bytes = 0u64;
        let mut complete_bundle = true;
        for name in ["manifest.json", "match.json", "world.json"] {
            match fs::metadata(directory.join(name)) {
                Ok(metadata) => bundle_bytes = bundle_bytes.saturating_add(metadata.len()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    complete_bundle = false
                }
                Err(_) => return Err("storage_unavailable"),
            }
        }
        if entry.status == ReplayStatus::Ready && !complete_bundle {
            entry.status = ReplayStatus::Evicted;
            bundle_bytes = 0;
        }
        entry.bundle_bytes = if entry.status == ReplayStatus::Ready {
            bundle_bytes
        } else {
            0
        };
        entry.storage_bytes = entry.bundle_bytes.saturating_add(metadata.len());
        if matches!(entry.status, ReplayStatus::Queued | ReplayStatus::Parsing) {
            entry.status = ReplayStatus::Available;
            entry.auto_queued = true;
            write_meta(&directory, &entry)?;
        }
        index.entries.push(entry);
    }
    Ok(index)
}

fn recover_interrupted_entries(root: &Path, index: &mut ReplayIndex) -> Result<(), &'static str> {
    let mut changed = false;
    for entry in &mut index.entries {
        if matches!(entry.status, ReplayStatus::Queued | ReplayStatus::Parsing) {
            entry.status = ReplayStatus::Available;
            entry.auto_queued = true;
            entry.storage_bytes = entry.bundle_bytes + entry_meta_bytes(entry)?.len() as u64;
            write_meta(&root.join(&entry.match_id), entry)?;
            changed = true;
        }
    }
    if changed {
        write_index(root, index)?;
    }
    Ok(())
}

fn write_index(root: &Path, index: &ReplayIndex) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec_pretty(index).map_err(|_| "storage_unavailable")?;
    if bytes.len() as u64 > MAX_INDEX_BYTES {
        return Err("storage_unavailable");
    }
    crate::atomic::write(&root.join("index.json"), &bytes).map_err(|_| "storage_unavailable")
}

fn read_json_limited(path: &Path, limit: u64) -> Result<Value, &'static str> {
    let mut file = File::open(path).map_err(|_| "bundle_invalid")?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| "bundle_invalid")?;
    if bytes.len() as u64 > limit {
        return Err("bundle_invalid");
    }
    serde_json::from_slice(&bytes).map_err(|_| "bundle_invalid")
}

fn source_path_from_meta(root: &Path, match_id: &str) -> Option<PathBuf> {
    read_json_limited(&root.join(match_id).join("meta.json"), MAX_META_BYTES)
        .ok()?
        .get("source_path")?
        .as_str()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

fn library_match_metadata(
    match_data: &Value,
) -> (Option<String>, Option<String>, Option<u64>, Option<String>) {
    let meta = match_data.get("meta");
    let match_date_utc = meta
        .and_then(|meta| meta.get("end_time_utc"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .map(str::to_owned);
    let own_hero = meta
        .and_then(|meta| meta.get("own_hero"))
        .and_then(Value::as_str)
        .or_else(|| {
            match_data
                .get("heroes")
                .and_then(Value::as_array)
                .and_then(|heroes| {
                    heroes.iter().find_map(|hero| {
                        (hero.get("own").and_then(Value::as_bool) == Some(true)
                            || hero.get("is_own").and_then(Value::as_bool) == Some(true))
                        .then(|| hero.get("id").and_then(Value::as_str))
                        .flatten()
                    })
                })
        })
        .filter(|value| {
            value.starts_with("npc_dota_hero_")
                && value.len() <= 96
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
        .map(str::to_owned);
    let duration_s = meta
        .and_then(|meta| meta.get("duration_s"))
        .and_then(Value::as_u64);
    let patch = meta
        .and_then(|meta| meta.get("patch"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 32)
        .map(str::to_owned);
    (match_date_utc, own_hero, duration_s, patch)
}

fn enrich_list_entry(root: &Path, entry: &mut ReplayLibraryEntry) {
    if !matches!(entry.status, ReplayStatus::Ready)
        || (entry.match_date_utc.is_some()
            && entry.own_hero.is_some()
            && entry.duration_s.is_some()
            && entry.patch.is_some())
    {
        return;
    }
    let directory = root.join(&entry.match_id);
    if let Ok(meta) = read_json_limited(&directory.join("meta.json"), MAX_INDEX_BYTES) {
        if entry.match_date_utc.is_none() {
            entry.match_date_utc = meta
                .get("match_date_utc")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if entry.own_hero.is_none() {
            entry.own_hero = meta
                .get("own_hero")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if entry.duration_s.is_none() {
            entry.duration_s = meta.get("duration_s").and_then(Value::as_u64);
        }
        if entry.patch.is_none() {
            entry.patch = meta.get("patch").and_then(Value::as_str).map(str::to_owned);
        }
    }
    if entry.match_date_utc.is_some()
        && entry.own_hero.is_some()
        && entry.duration_s.is_some()
        && entry.patch.is_some()
    {
        return;
    }
    if let Ok(bundle) = read_json_limited(&directory.join("match.json"), engine::MAX_MATCH_BYTES) {
        let (date, hero, duration, patch) = library_match_metadata(&bundle);
        entry.match_date_utc = entry.match_date_utc.take().or(date);
        entry.own_hero = entry.own_hero.take().or(hero);
        entry.duration_s = entry.duration_s.or(duration);
        entry.patch = entry.patch.take().or(patch);
    }
}

fn validated_library_bundle_hash(
    root: &Path,
    entry: &ReplayLibraryEntry,
    manifest: &Value,
) -> Option<String> {
    if entry.status != ReplayStatus::Ready {
        return None;
    }
    let expected_dem_sha256 = entry.dem_sha256.as_deref()?;
    let directory = root.join(&entry.match_id);
    let validated = engine::validate_library_bundle_dir(&directory, expected_dem_sha256).ok()?;
    if validated.match_id != entry.match_id {
        return None;
    }
    let manifest_sha256 = engine::sha256_file(&directory.join("manifest.json")).ok()?;
    let files = manifest.get("files")?.as_object()?;
    let match_sha256 = files.get("match.json")?.get("sha256")?.as_str()?;
    let world_sha256 = files.get("world.json")?.get("sha256")?.as_str()?;
    let mut identity = Vec::with_capacity(180);
    for (name, sha256) in [
        ("manifest.json", manifest_sha256.as_str()),
        ("match.json", match_sha256),
        ("world.json", world_sha256),
    ] {
        identity.extend_from_slice(name.as_bytes());
        identity.push(0);
        identity.extend_from_slice(sha256.as_bytes());
        identity.push(0);
    }
    Some(engine::sha256_bytes(&identity))
}

fn entry_meta_bytes(entry: &ReplayLibraryEntry) -> Result<Vec<u8>, &'static str> {
    serde_json::to_vec_pretty(&entry.meta()).map_err(|_| "storage_unavailable")
}

fn write_meta(directory: &Path, entry: &ReplayLibraryEntry) -> Result<(), &'static str> {
    fs::create_dir_all(directory).map_err(|_| "storage_unavailable")?;
    let bytes = entry_meta_bytes(entry)?;
    crate::atomic::write(&directory.join("meta.json"), &bytes).map_err(|_| "storage_unavailable")
}

fn index_usage(index: &ReplayIndex) -> u64 {
    index
        .entries
        .iter()
        .filter(|entry| entry.status == ReplayStatus::Ready)
        .fold(0u64, |total, entry| {
            total.saturating_add(entry.bundle_bytes)
    })
}

struct StagedBundleEviction {
    match_id: String,
    directory: PathBuf,
}

fn stage_bundle_evictions(
    root: &Path,
    match_ids: &[String],
) -> Result<Vec<StagedBundleEviction>, &'static str> {
    let staging = root.join(".staging");
    fs::create_dir_all(&staging).map_err(|_| "storage_unavailable")?;
    let mut staged = Vec::new();
    for match_id in match_ids {
        let source = root.join(match_id);
        if !source.is_dir() {
            continue;
        }
        let directory = staging.join(format!(
            "evict-{}-{}",
            std::process::id(),
            NEXT_TOMBSTONE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        if fs::create_dir(&directory).is_err() {
            restore_bundle_evictions(root, &staged);
            return Err("storage_unavailable");
        }
        let mut moved = false;
        for name in ["manifest.json", "match.json", "world.json"] {
            let from = source.join(name);
            if !from.exists() {
                continue;
            }
            if fs::rename(&from, directory.join(name)).is_err() {
                staged.push(StagedBundleEviction {
                    match_id: match_id.clone(),
                    directory,
                });
                restore_bundle_evictions(root, &staged);
                return Err("storage_unavailable");
            }
            moved = true;
        }
        if moved {
            staged.push(StagedBundleEviction {
                match_id: match_id.clone(),
                directory,
            });
        } else {
            let _ = fs::remove_dir(&directory);
        }
    }
    Ok(staged)
}

fn restore_bundle_evictions(root: &Path, staged: &[StagedBundleEviction]) {
    for item in staged.iter().rev() {
        let destination = root.join(&item.match_id);
        let _ = fs::create_dir_all(&destination);
        for name in ["manifest.json", "match.json", "world.json"] {
            let from = item.directory.join(name);
            if from.exists() {
                let _ = fs::rename(from, destination.join(name));
            }
        }
        let _ = fs::remove_dir(&item.directory);
    }
}

fn finish_bundle_evictions(staged: &[StagedBundleEviction]) {
    for item in staged {
        let _ = fs::remove_dir_all(&item.directory);
    }
}

fn log_evictions(count: usize) {
    for _ in 0..count {
        eprintln!("{}", eviction_log_line());
    }
}

fn write_evicted_metadata(
    root: &Path,
    index: &ReplayIndex,
    match_ids: &[String],
) -> Result<(), &'static str> {
    for match_id in match_ids {
        if let Some(entry) = index
            .entries
            .iter()
            .find(|entry| &entry.match_id == match_id)
        {
            write_meta(&root.join(match_id), entry)?;
        }
    }
    Ok(())
}

fn replay_recency(entry: &ReplayLibraryEntry) -> (Option<&str>, Option<u64>, u64) {
    replay_recency_key(
        entry.match_date_utc.as_deref(),
        source_modified_unix_ms(&entry.source_path),
        entry.created_at_unix_ms,
    )
}

fn replay_recency_key(
    match_date_utc: Option<&str>,
    dem_modified_unix_ms: Option<u64>,
    discovered_unix_ms: u64,
) -> (Option<&str>, Option<u64>, u64) {
    (match_date_utc, dem_modified_unix_ms, discovered_unix_ms)
}

fn state_code(status: &ReplayStatus) -> &str {
    match status {
        ReplayStatus::Available => "available",
        ReplayStatus::Queued => "queued",
        ReplayStatus::Parsing => "parsing",
        ReplayStatus::Ready => "ready",
        ReplayStatus::Evicted => "evicted",
        ReplayStatus::Failed { .. } => "failed",
        ReplayStatus::NeedsPack => "needs_pack",
    }
}

fn status_reason_code(status: &ReplayStatus) -> &str {
    match status {
        ReplayStatus::Failed { reason } => safe_reason_code(reason),
        _ => state_code(status),
    }
}

fn safe_reason_code(reason: &str) -> &str {
    if !reason.is_empty()
        && reason.len() <= 64
        && reason
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        reason
    } else {
        "unknown"
    }
}

fn operation_code(operation: ReplayOperation) -> &'static str {
    match operation {
        ReplayOperation::Parse => "parse",
        ReplayOperation::Reparse => "reparse",
    }
}

fn job_transition_log_line(
    from: &str,
    to: &str,
    reason: &str,
    duration_ms: u64,
    operation: &str,
) -> String {
    format!(
        "replay_job_transition from={} to={} reason={} duration_ms={} operation={}",
        safe_reason_code(from),
        safe_reason_code(to),
        safe_reason_code(reason),
        duration_ms,
        safe_reason_code(operation),
    )
}

fn watcher_summary_log_line(
    discovered: usize,
    stable: usize,
    queued: usize,
    error: Option<&str>,
) -> String {
    format!(
        "replay_watch_pass discovered={discovered} stable={stable} queued={queued} error={}",
        error.map(safe_reason_code).unwrap_or("none"),
    )
}

fn eviction_log_line() -> &'static str {
    "replay_eviction reason=retention_policy"
}

fn validate_match_id(match_id: &str) -> Result<(), ()> {
    canonical_match_id(match_id)
        .filter(|canonical| canonical == match_id)
        .map(|_| ())
        .ok_or(())
}

fn canonical_match_id(value: &str) -> Option<String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number = value.parse::<u64>().ok()?;
    if number == 0 {
        return None;
    }
    Some(number.to_string())
}

fn is_match_id(value: &str) -> bool {
    canonical_match_id(value).is_some_and(|canonical| canonical == value)
}

fn next_job_id() -> String {
    format!(
        "job-{}-{}",
        std::process::id(),
        NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed)
    )
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn system_time_unix_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
}

fn source_modified_unix_ms(source_path: &str) -> Option<u64> {
    fs::metadata(source_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_unix_ms)
}

fn auto_parse_enabled_or_default(setting: Result<bool, &'static str>) -> bool {
    setting.unwrap_or(true)
}

fn move_path_to_tombstone(path: &Path, root: &Path) -> Option<(PathBuf, PathBuf)> {
    if !path.exists() {
        return None;
    }
    let tombstone = root.join(format!(
        ".retired-{}",
        NEXT_TOMBSTONE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::rename(path, &tombstone)
        .ok()
        .map(|_| (path.to_owned(), tombstone))
}

fn restore_tombstones(tombstones: &[(PathBuf, PathBuf)]) {
    for (original, tombstone) in tombstones.iter().rev() {
        if tombstone.exists() {
            let _ = fs::rename(tombstone, original);
        }
    }
}

fn cancel_jobs_for_match(jobs: &Mutex<VecDeque<JobRecord>>, match_id: &str) {
    for job in lock(jobs).iter_mut().filter(|job| {
        job.match_id.as_deref() == Some(match_id) && job.public.status == ReplayStatus::Queued
    }) {
        let duration = job
            .queued_at
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let operation = operation_code(job.public.operation);
        job.public.status = ReplayStatus::Failed {
            reason: "cancelled".to_owned(),
        };
        eprintln!(
            "{}",
            job_transition_log_line("queued", "failed", "cancelled", duration, operation)
        );
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::Instant;

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "kohbor-replay-library-test-{label}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn controlled_library(
        root: PathBuf,
        limits: ReplayLibraryLimits,
        active: Arc<AtomicBool>,
        emit_state_changed: EventSink,
    ) -> ReplayLibrary {
        let active_for_gate = active.clone();
        ReplayLibrary::new(
            root,
            None,
            false,
            Arc::new(move || active_for_gate.load(Ordering::Acquire)),
            emit_state_changed,
            limits,
        )
    }

    fn wait_until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if predicate() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(predicate(), "condition did not become true before timeout");
    }

    fn write_demo(folder: &Path, match_id: &str) -> PathBuf {
        fs::create_dir_all(folder).unwrap();
        let path = folder.join(format!("{match_id}.dem"));
        fs::write(&path, b"PBDEMS2\0fixture").unwrap();
        path
    }

    struct FixtureEngineRunner {
        calls: AtomicUsize,
        fail: AtomicBool,
        remove_source_before_failure: AtomicBool,
        match_active_after_first_call: Option<Arc<AtomicBool>>,
    }

    impl FixtureEngineRunner {
        fn new(fail: bool) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                fail: AtomicBool::new(fail),
                remove_source_before_failure: AtomicBool::new(false),
                match_active_after_first_call: None,
            }
        }

        fn removing_source_before_failure() -> Self {
            let runner = Self::new(true);
            runner
                .remove_source_before_failure
                .store(true, Ordering::Release);
            runner
        }

        fn starting_match_after_first_call(active: Arc<AtomicBool>) -> Self {
            let mut runner = Self::new(false);
            runner.match_active_after_first_call = Some(active);
            runner
        }
    }

    impl EngineRunner for FixtureEngineRunner {
        fn run(
            &self,
            _bundled_path: Option<&Path>,
            _development_path: Option<&Path>,
            _allow_development_path: bool,
            dem_path: &Path,
            output_dir: &Path,
            _match_active: &dyn Fn() -> bool,
        ) -> Result<engine::EngineRun, engine::EngineFailure> {
            let call = self.calls.fetch_add(1, Ordering::AcqRel);
            if call == 0 {
                if let Some(active) = &self.match_active_after_first_call {
                    active.store(true, Ordering::Release);
                    return Err(engine::EngineFailure::MatchBecameActive);
                }
            }
            if self.fail.load(Ordering::Acquire) {
                if self.remove_source_before_failure.load(Ordering::Acquire) {
                    fs::remove_file(dem_path).unwrap();
                }
                return Err(engine::EngineFailure::EngineFailed);
            }
            let dem_sha =
                engine::sha256_file(dem_path).map_err(|_| engine::EngineFailure::InvalidInput)?;
            write_valid_bundle(output_dir, "321", "3.1", &dem_sha);
            Ok(engine::EngineRun {
                engine_version: engine::ENGINE_VERSION.to_owned(),
                engine_sha256: engine::ENGINE_SHA256.to_owned(),
                stderr_code: None,
            })
        }
    }

    fn write_valid_bundle(directory: &Path, match_id: &str, schema: &str, dem_sha: &str) {
        let engine_version = if schema == "3.0" {
            "0.2.0"
        } else {
            engine::ENGINE_VERSION
        };
        let numeric_match_id = match_id.parse::<u64>().unwrap();
        let mut match_doc = json!({
            "meta": {
                "schema": schema, "engine_version": engine_version, "patch": "7.42", "pack": "7.42",
                "server_build": "dota_v123", "build_num": 123, "match_id": numeric_match_id,
                "end_time_utc": null, "game_mode": 22, "duration_s": 1200, "dem_sha256": dem_sha,
                "dem_file": "fixture.dem", "parse_error": null
            },
            "t0": 0, "t1": 2, "dtick": 30,
            "heroes": [{
                "id": "npc_dota_hero_alpha", "name": "Alpha", "team": 2, "pid": 0,
                "color": "#3375FF", "pos": [], "hp": [], "mhp": [], "mp": [], "mmp": [],
                "lvl": [], "alive": [], "eco": [], "abil": [], "inv": [], "icd": {}, "mods": [],
                "casts": [], "buys": [], "finds": [], "st": {}, "prim": 0, "din": [], "hin": []
            }],
            "deaths": [], "names": {}, "statdb": {}, "sampleT": []
        });
        if schema == "3.1" {
            match_doc["meta"]["winner"] = Value::Null;
            match_doc["meta"]["winner_reason"] = json!("not recorded in the replay");
            match_doc["meta"]["draft"] = Value::Null;
            match_doc["meta"]["draft_reason"] = json!("draft not present in the replay");
            match_doc["heroes"][0]["gold_earned"] = json!([null, 250, 400]);
            match_doc["heroes"][0]["gold_earned_final"] = Value::Null;
            match_doc["heroes"][0]["dmg_out"] = json!({ "total": 75, "p": 25, "m": 40, "u": 10 });
        }
        let world_doc =
            json!({ "v": 1, "qh": 8, "qu": 30, "t0": 0, "heroes": {}, "tracks": [], "runes": [] });
        let match_bytes = serde_json::to_vec(&match_doc).unwrap();
        let world_bytes = serde_json::to_vec(&world_doc).unwrap();
        let manifest = json!({
            "schema": schema,
            "engine_version": engine_version,
            "match_id": numeric_match_id,
            "patch": "7.42",
            "pack": { "id": "7.42", "source": "embedded" },
            "dem_sha256": dem_sha,
            "files": {
                "match.json": { "bytes": match_bytes.len(), "sha256": engine::sha256_bytes(&match_bytes) },
                "world.json": { "bytes": world_bytes.len(), "sha256": engine::sha256_bytes(&world_bytes) }
            },
            "map": {
                "image": "map.png", "pack_path": "7.42/map.png", "size": 1, "coord": "world",
                "calibration": { "px": [], "py": [], "formula": "linear" }
            },
            "icons": { "dirs": {}, "needed": { "heroes": [], "items": [], "abilities": [] } },
            "run": { "parser_passes": 1, "timing_s": {} }
        });
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(directory.join("match.json"), match_bytes).unwrap();
        fs::write(directory.join("world.json"), world_bytes).unwrap();
    }

    fn seed_ready_bundle(
        library: &ReplayLibrary,
        source_path: &Path,
        match_id: &str,
        source: ReplaySource,
        schema: &str,
    ) {
        library
            .add_available(match_id.to_owned(), source_path, source)
            .unwrap();
        let dem_sha = engine::sha256_file(source_path).unwrap();
        let stage = library.create_staging_dir("seed").unwrap();
        write_valid_bundle(&stage, match_id, schema, &dem_sha);
        let validated = engine::validate_bundle_dir(&stage, &dem_sha).unwrap();
        let mut entry = lock(&library.inner.state)
            .index
            .entries
            .iter()
            .find(|entry| entry.match_id == match_id)
            .unwrap()
            .clone();
        entry.dem_sha256 = Some(dem_sha);
        entry.engine_version = Some(
            if schema == "3.0" {
                "0.2.0"
            } else {
                engine::ENGINE_VERSION
            }
            .to_owned(),
        );
        entry.engine_sha256 = Some("previous-engine-hash".to_owned());
        entry.parse_time_unix_ms = Some(now_unix_ms());
        entry.status = ReplayStatus::Ready;
        entry.bundle_bytes = validated.bundle_bytes;
        entry.storage_bytes = entry.bundle_bytes + entry_meta_bytes(&entry).unwrap().len() as u64;
        write_meta(&stage, &entry).unwrap();
        library.install_parsed_bundle(&stage, entry, false).unwrap();
    }

    fn reparse_test_library(
        root: PathBuf,
        active: Arc<AtomicBool>,
        runner: Arc<FixtureEngineRunner>,
    ) -> ReplayLibrary {
        let active_for_gate = active.clone();
        ReplayLibrary::new_with_engine_runner(
            root,
            None,
            false,
            Arc::new(move || active_for_gate.load(Ordering::Acquire)),
            Arc::new(|| {}),
            ReplayLibraryLimits::default(),
            runner,
        )
    }

    #[test]
    fn library_list_metadata_uses_only_explicit_local_bundle_fields() {
        let bundle = serde_json::json!({
            "meta": {"end_time_utc": "2026-11-23T21:40:00Z", "duration_s": 1800},
            "heroes": [{"id": "npc_dota_hero_axe", "pid": 0}]
        });
        assert_eq!(
            library_match_metadata(&bundle),
            (
                Some("2026-11-23T21:40:00Z".to_owned()),
                None,
                Some(1800),
                None
            )
        );

        let explicit_own = serde_json::json!({
            "meta": {"own_hero": "npc_dota_hero_lion"},
            "heroes": [{"id": "npc_dota_hero_axe", "is_own": true}]
        });
        assert_eq!(
            library_match_metadata(&explicit_own),
            (None, Some("npc_dota_hero_lion".to_owned()), None, None)
        );
    }

    #[test]
    fn library_index_round_trips_atomically() {
        let root = temp_dir("index");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        let dem = source.join("sample.dem");
        fs::write(&dem, b"PBDEMS2\0fixture").unwrap();
        assert!(library
            .add_available("42".to_owned(), &dem, ReplaySource::ManualPick)
            .unwrap());

        let reopened = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let list = reopened.list().unwrap();
        assert_eq!(list.matches.len(), 1);
        assert_eq!(list.matches[0].match_id, "42");
        assert_eq!(list.matches[0].status, ReplayStatus::Available);
        let on_disk: ReplayIndex =
            serde_json::from_slice(&fs::read(root.join("index.json")).unwrap()).unwrap();
        assert_eq!(on_disk.entries.len(), 1);
        assert!(root.join("42").join("meta.json").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn list_reply_keeps_match_id_and_drops_source_paths_and_hashes() {
        let root = temp_dir("list-redaction");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let dem = write_demo(&root.join("source"), "424242");
        library
            .add_available("424242".to_owned(), &dem, ReplaySource::ManualPick)
            .unwrap();
        {
            let mut state = lock(&library.inner.state);
            let entry = state
                .index
                .entries
                .iter_mut()
                .find(|entry| entry.match_id == "424242")
                .unwrap();
            entry.dem_sha256 = Some("private-dem-hash".to_owned());
            entry.engine_sha256 = Some("private-engine-hash".to_owned());
            write_meta(&root.join("424242"), entry).unwrap();
            write_index(&root, &state.index).unwrap();
        }
        library.record_job(
            &ReplayJob {
                job_id: "job-private-test".to_owned(),
                match_id: Some("424242".to_owned()),
                operation: ReplayOperation::Parse,
                status: ReplayStatus::Queued,
            },
            Some("424242".to_owned()),
            ParseMode::Manual,
        );
        let reply = serde_json::to_value(library.list().unwrap()).unwrap();
        let row = &reply["matches"][0];
        for key in ["source_path", "dem_sha256", "engine_sha256"] {
            assert!(row.get(key).is_none(), "{key} must not be serialized");
        }
        assert_eq!(row["match_id"], "424242");
        assert_eq!(row["source_exists"], true);
        assert!(row.get("bundle_schema").is_some());
        assert_eq!(row["bundle_hash"], Value::Null);
        assert_eq!(reply["jobs"][0]["match_id"], "424242");
        let serialized = serde_json::to_string(&reply).unwrap();
        assert!(serialized.contains("424242"));
        assert!(!serialized.contains(dem.to_string_lossy().as_ref()));
        assert!(!serialized.contains("private-dem-hash"));
        assert!(!serialized.contains("private-engine-hash"));
        assert!(reply.get("watched_replay_folder_path").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn list_bundle_hash_changes_only_for_a_validated_bundle() {
        let root = temp_dir("list-bundle-hash");
        let source = write_demo(&root.join("source"), "424243");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        seed_ready_bundle(
            &library,
            &source,
            "424243",
            ReplaySource::ManualPick,
            "3.1",
        );

        let first_hash = serde_json::to_value(library.list().unwrap()).unwrap()["matches"][0]
            ["bundle_hash"]
            .as_str()
            .expect("a validated ready bundle should have a hash")
            .to_owned();
        assert_eq!(first_hash.len(), 64);

        let directory = root.join("424243");
        let mut manifest: Value = read_json_limited(
            &directory.join("manifest.json"),
            engine::MAX_MANIFEST_BYTES,
        )
        .unwrap();
        let mut world: Value = read_json_limited(
            &directory.join("world.json"),
            engine::MAX_WORLD_BYTES,
        )
        .unwrap();
        world["qh"] = json!(9);
        let world_bytes = serde_json::to_vec(&world).unwrap();
        fs::write(directory.join("world.json"), &world_bytes).unwrap();
        manifest["files"]["world.json"]["bytes"] = json!(world_bytes.len());
        manifest["files"]["world.json"]["sha256"] =
            json!(engine::sha256_bytes(&world_bytes));
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        engine::validate_library_bundle_dir(
            &directory,
            library.list().unwrap().matches[0]
                .dem_sha256
                .as_deref()
                .unwrap(),
        )
        .expect("updated bundle fixture remains valid");

        let second_hash = serde_json::to_value(library.list().unwrap()).unwrap()["matches"][0]
            ["bundle_hash"]
            .as_str()
            .expect("a validated ready bundle should have a hash")
            .to_owned();
        assert_ne!(first_hash, second_hash);

        fs::write(directory.join("world.json"), b"not a valid bundle").unwrap();
        let invalid_hash = serde_json::to_value(library.list().unwrap()).unwrap()["matches"][0]
            ["bundle_hash"]
            .clone();
        assert_eq!(invalid_hash, Value::Null);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn replay_log_formatters_do_not_accept_identity_or_path_fields() {
        let pass = watcher_summary_log_line(3, 2, 1, Some("replay_folder_unavailable"));
        let transition = job_transition_log_line("queued", "failed", "engine_failed", 42, "parse");
        let eviction = eviction_log_line();
        for line in [&pass, &transition, eviction] {
            assert!(!line.contains("424242"));
            assert!(!line.contains("C:\\"));
            assert!(!line.contains("\\replays\\"));
        }
        assert!(transition.contains("reason=engine_failed"));
        assert!(transition.contains("duration_ms=42"));
    }

    #[test]
    fn persisted_interrupted_automatic_jobs_return_available_then_requeue_when_stable() {
        let root = temp_dir("interrupted");
        let source = root.join("dota-replays");
        let dem_queued = write_demo(&source, "204");
        let dem_parsing = write_demo(&source, "205");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        library
            .add_available("204".to_owned(), &dem_queued, ReplaySource::DotaReplaysFolder)
            .unwrap();
        library
            .set_entry_auto_status_locked("204", ReplayStatus::Queued, true)
            .unwrap();
        library
            .add_available("205".to_owned(), &dem_parsing, ReplaySource::DotaReplaysFolder)
            .unwrap();
        library
            .set_entry_auto_status_locked("205", ReplayStatus::Parsing, true)
            .unwrap();

        let reopened = ReplayLibrary::new_for_tests_with_phase(
            root.clone(),
            ReplayLibraryLimits::default(),
            true,
        );
        let reopened_list = reopened.list().unwrap();
        assert_eq!(reopened_list.matches.len(), 2);
        for entry in &reopened_list.matches {
            assert_eq!(entry.status, ReplayStatus::Available);
            assert!(entry.auto_queued);
            let meta: serde_json::Value = serde_json::from_slice(
                &fs::read(root.join(&entry.match_id).join("meta.json")).unwrap(),
            ).unwrap();
            assert_eq!(meta["status"]["state"], "available");
            assert_eq!(meta["auto_queued"], true);
        }
        reopened.watch_replay_folder_for_test(&source).unwrap();
        assert!(reopened.list().unwrap().matches.iter().all(|entry| entry.status == ReplayStatus::Available));
        reopened.watch_replay_folder_for_test(&source).unwrap();
        assert!(reopened.list().unwrap().matches.iter().all(|entry| entry.status == ReplayStatus::Queued));
        assert_eq!(reopened.list().unwrap().jobs.len(), 2);
        assert!(reopened.list().unwrap().jobs.iter().all(|job| job.status == ReplayStatus::Queued));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_library_index_is_backed_up_and_rebuilt_from_match_metadata() {
        let root = temp_dir("index-rebuild");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let dem = write_demo(&root.join("source"), "205");
        library
            .add_available("205".to_owned(), &dem, ReplaySource::ManualPick)
            .unwrap();
        let bad_index = b"{ this is not valid json";
        fs::write(root.join("index.json"), bad_index).unwrap();

        let rebuilt = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let list = rebuilt.list().unwrap();
        assert_eq!(list.matches.len(), 1);
        assert_eq!(list.matches[0].match_id, "205");
        assert_eq!(list.matches[0].status, ReplayStatus::Available);
        assert_eq!(list.matches[0].source, ReplaySource::ManualPick);
        let backup = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("index.invalid-") && name.ends_with(".json")
                    })
            })
            .expect("invalid index backup should remain present");
        assert_eq!(fs::read(backup).unwrap(), bad_index);
        let valid: ReplayIndex =
            serde_json::from_slice(&fs::read(root.join("index.json")).unwrap()).unwrap();
        assert_eq!(valid.entries.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn configured_match_cap_evicts_oldest_first() {
        let root = temp_dir("cap");
        let limits = ReplayLibraryLimits {
            max_matches: 2,
            max_bytes: 1024 * 1024,
        };
        let library = ReplayLibrary::new_for_tests(root.clone(), limits);
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        for (index, (id, file)) in [("101", "one.dem"), ("102", "two.dem"), ("103", "three.dem")]
            .into_iter()
            .enumerate()
        {
            let path = source.join(file);
            fs::write(&path, b"PBDEMS2\0fixture").unwrap();
            library
                .add_available(id.to_owned(), &path, ReplaySource::DotaReplaysFolder)
                .unwrap();
            library.set_entry_status(id, ReplayStatus::Ready).unwrap();
            let mut state = lock(&library.inner.state);
            let entry = state
                .index
                .entries
                .iter_mut()
                .find(|entry| entry.match_id == id)
                .unwrap();
            entry.source_path.clear();
            entry.created_at_unix_ms = index as u64 + 1;
            write_meta(&root.join(id), entry).unwrap();
            write_index(&root, &state.index).unwrap();
        }
        library.set_pinned("103", false).unwrap();
        let rows = library.list().unwrap().matches;
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter()
                .find(|entry| entry.match_id == "101")
            .unwrap()
                .status,
            ReplayStatus::Evicted
        );
        assert_eq!(
            rows.iter()
                .find(|entry| entry.match_id == "102")
                .unwrap()
                .status,
            ReplayStatus::Ready
        );
        assert_eq!(
            rows.iter()
                .find(|entry| entry.match_id == "103")
                .unwrap()
                .status,
            ReplayStatus::Ready
        );
        assert!(root.join("101").join("meta.json").is_file());
        assert!(!root.join("101").join("manifest.json").exists());
        assert!(
            source.join("one.dem").exists(),
            "retention must never delete the user's .dem"
        );
        library.remove("102").unwrap();
        assert!(!root.join("102").exists());
        assert_eq!(library.list().unwrap().matches.len(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retention_order_uses_match_end_then_dem_time_then_discovery() {
        let root = temp_dir("retention-order");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let source = root.join("source");
        let fixtures = [
            ("401", "2026-11-20T00:00:00Z", 300, 4),
            ("402", "2026-11-21T00:00:00Z", 100, 3),
            ("403", "2026-11-21T00:00:00Z", 200, 1),
            ("404", "2026-11-21T00:00:00Z", 200, 2),
        ];
        let mut recency_keys = fixtures
            .iter()
            .map(|(id, ended_at, dem_modified, discovered)| {
                (
                    *id,
                    replay_recency_key(
                        Some(ended_at),
                        Some(*dem_modified),
                        *discovered,
                    ),
                )
            })
            .collect::<Vec<_>>();
        recency_keys.sort_by_key(|(_, recency)| *recency);
        assert_eq!(
            recency_keys.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            ["401", "402", "403", "404"]
        );
        for (match_id, ended_at, _dem_modified, discovered) in fixtures {
            let dem = write_demo(&source, match_id);
            library
                .add_available(match_id.to_owned(), &dem, ReplaySource::DotaReplaysFolder)
                .unwrap();
            library
                .set_entry_status(match_id, ReplayStatus::Ready)
                .unwrap();
            let mut state = lock(&library.inner.state);
            let entry = state
                .index
                .entries
                .iter_mut()
                .find(|entry| entry.match_id == match_id)
                .unwrap();
            entry.source_path.clear();
            entry.match_date_utc = Some(ended_at.to_owned());
            entry.created_at_unix_ms = discovered;
            entry.bundle_bytes = 1;
            write_meta(&root.join(match_id), entry).unwrap();
            write_index(&root, &state.index).unwrap();
        }
        let mut projection = lock(&library.inner.state).index.clone();
        let evicted = library
            .trim_to_limits_with(
                &mut projection,
                "",
                ReplayLibraryLimits {
                    max_matches: 1,
                    max_bytes: 1024,
                },
            )
            .unwrap();
        assert_eq!(evicted, ["401", "403", "404"]);
        assert_eq!(
            projection
                .entries
                .iter()
                .find(|entry| entry.match_id == "402")
                .unwrap()
                .status,
            ReplayStatus::Ready
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retention_keeps_newest_unpinned_ready_bundles_and_pins_and_preserves_notes_and_dem() {
        let root = temp_dir("retention-pins");
        let data = temp_dir("retention-notes");
        let library = ReplayLibrary::new_for_tests(
            root.clone(),
            ReplayLibraryLimits {
                max_matches: 100,
                max_bytes: 1024 * 1024,
            },
        );
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        let notes = crate::analytics::AnalyticsStore::open(&data, Some([9; 32]));
        for (index, id) in ["101", "102", "103", "104", "105", "106", "107"].into_iter().enumerate() {
            let dem = source.join(format!("{id}.dem")); fs::write(&dem, b"PBDEMS2\0fixture").unwrap();
            library.add_available(id.to_owned(), &dem, ReplaySource::DotaReplaysFolder).unwrap();
            library.set_entry_status(id, ReplayStatus::Ready).unwrap();
            let mut state = lock(&library.inner.state);
            if let Some(entry) = state.index.entries.iter_mut().find(|entry| entry.match_id == id) {
                entry.created_at_unix_ms = index as u64 + 1;
                entry.match_date_utc = Some(format!("2026-11-{:02}T00:00:00Z", index + 1));
                entry.bundle_bytes = 100;
                entry.storage_bytes = 200;
                write_meta(&root.join(id), entry).unwrap();
                write_index(&root, &state.index).unwrap();
            }
        }
        notes.write_note("102", "Coach note", "kept after replay eviction").unwrap();
        library.set_pinned("101", true).unwrap();
        library
            .set_limits(ReplayLibraryLimits {
                max_matches: 5,
                max_bytes: 1024 * 1024,
            })
            .unwrap();
        let ids = library
            .list()
            .unwrap()
            .matches
            .into_iter()
            .map(|entry| entry.match_id)
            .collect::<HashSet<_>>();
        assert_eq!(
            ids,
            ["101", "102", "103", "104", "105", "106", "107"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        assert_eq!(
            library
                .list()
                .unwrap()
                .matches
                .iter()
                .find(|entry| entry.match_id == "102")
                .unwrap()
                .status,
            ReplayStatus::Evicted
        );
        assert!(source.join("102.dem").exists());
        assert_eq!(
            notes.read_note("102", "Coach note.md").unwrap(),
            "kept after replay eviction"
        );
        assert!(root.join("102").join("meta.json").is_file());
        assert!(!root.join("102").join("manifest.json").exists());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(data);
    }

    #[test]
    fn size_cap_evicts_oldest_eligible_bundle_only() {
        let root = temp_dir("retention-size");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let source = root.join("source"); fs::create_dir_all(&source).unwrap();
        for (index, id) in ["201", "202", "203"].into_iter().enumerate() {
            let dem = source.join(format!("{id}.dem")); fs::write(&dem, b"PBDEMS2\0fixture").unwrap();
            library.add_available(id.to_owned(), &dem, ReplaySource::DotaReplaysFolder).unwrap();
            library.set_entry_status(id, ReplayStatus::Ready).unwrap();
            let mut state = lock(&library.inner.state);
            let entry = state.index.entries.iter_mut().find(|entry| entry.match_id == id).unwrap();
            entry.created_at_unix_ms = index as u64 + 1; entry.bundle_bytes = 600; entry.storage_bytes = 700;
            write_meta(&root.join(id), entry).unwrap(); write_index(&root, &state.index).unwrap();
        }
        library
            .set_limits(ReplayLibraryLimits {
                max_matches: 100,
                max_bytes: 1500,
            })
            .unwrap();
        let list = library.list().unwrap();
        assert_eq!(list.matches.len(), 3);
        assert_eq!(
            list.matches
                .iter()
                .find(|entry| entry.match_id == "201")
                .unwrap()
                .status,
            ReplayStatus::Evicted
        );
        assert_eq!(
            list.matches
                .iter()
                .find(|entry| entry.match_id == "202")
                .unwrap()
                .status,
            ReplayStatus::Ready
        );
        assert_eq!(
            list.matches
                .iter()
                .find(|entry| entry.match_id == "203")
                .unwrap()
                .status,
            ReplayStatus::Ready
        );
        for id in ["201", "202", "203"] {
            assert!(source.join(format!("{id}.dem")).exists());
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pinned_bundles_survive_a_size_cap_reduction_and_report_actual_usage() {
        let root = temp_dir("retention-pinned-over-cap");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let source = root.join("source"); fs::create_dir_all(&source).unwrap();
        for (id, pinned) in [("301", true), ("302", true), ("303", false)] {
            let dem = source.join(format!("{id}.dem")); fs::write(&dem, b"PBDEMS2\\0fixture").unwrap();
            library.add_available(id.to_owned(), &dem, ReplaySource::DotaReplaysFolder).unwrap();
            library.set_entry_status(id, ReplayStatus::Ready).unwrap();
            if pinned { library.set_pinned(id, true).unwrap(); }
            let mut state = lock(&library.inner.state);
            let entry = state.index.entries.iter_mut().find(|entry| entry.match_id == id).unwrap();
            entry.storage_bytes = 600; entry.bundle_bytes = 500;
            write_meta(&root.join(id), entry).unwrap(); write_index(&root, &state.index).unwrap();
        }
        library
            .set_limits(ReplayLibraryLimits {
                max_matches: 20,
                max_bytes: 800,
            })
            .unwrap();
        let list = library.list().unwrap();
        assert_eq!(list.matches.len(), 3);
        assert_eq!(
            list.matches.iter().filter(|entry| entry.status == ReplayStatus::Ready)
                .map(|entry| entry.match_id.as_str()).collect::<HashSet<_>>(),
            ["301", "302"].into_iter().collect()
        );
        assert_eq!(
            list.matches.iter().filter(|entry| entry.status == ReplayStatus::Evicted)
                .map(|entry| entry.match_id.as_str()).collect::<HashSet<_>>(),
            ["303"].into_iter().collect()
        );
        assert_eq!(
            list.matches.iter().filter(|entry| entry.status == ReplayStatus::Ready)
                .map(|entry| entry.storage_bytes).sum::<u64>(),
            1200
        );
        for id in ["301", "302", "303"] { assert!(source.join(format!("{id}.dem")).exists()); }
        assert!(root.join("301").exists() && root.join("302").exists());
        assert!(root.join("303").join("meta.json").is_file());
        assert!(!root.join("303").join("manifest.json").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn auto_analysis_off_keeps_watched_dems_available_without_queueing() {
        let root = temp_dir("auto-off");
        let folder = root.join("dota-replays");
        let dem = write_demo(&folder, "9001");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        library.set_auto_analyse(false);

        assert_eq!(library.watch_replay_folder_for_test(&folder).unwrap(), 1);

        let list = library.list().unwrap();
        assert!(list.jobs.is_empty());
        assert_eq!(list.matches[0].status, ReplayStatus::Available);
        assert!(dem.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn auto_analysis_queues_new_watched_replay_and_waits_for_match_barrier() {
        let root = temp_dir("auto-barrier");
        let folder = root.join("dota-replays");
        let dem = write_demo(&folder, "9101");
        let active = Arc::new(AtomicBool::new(true));
        let library = controlled_library(
            root.clone(),
            ReplayLibraryLimits::default(),
            active.clone(),
            Arc::new(|| {}),
        );

        assert_eq!(library.watch_replay_folder_for_test(&folder).unwrap(), 1);
        assert_eq!(library.watch_replay_folder_for_test(&folder).unwrap(), 0);
        wait_until(|| library.list().unwrap().matches[0].status == ReplayStatus::Queued);
        assert_eq!(library.list().unwrap().jobs[0].status, ReplayStatus::Queued);
        assert!(dem.exists());
        thread::sleep(Duration::from_millis(30));
        assert_eq!(
            library.list().unwrap().matches[0].status,
            ReplayStatus::Queued
        );

        active.store(false, Ordering::Release);
        wait_until(|| {
            !matches!(
                library.list().unwrap().matches[0].status,
                ReplayStatus::Queued | ReplayStatus::Parsing
            )
        });
        assert!(matches!(
            library.list().unwrap().matches[0].status,
            ReplayStatus::Failed { .. } | ReplayStatus::NeedsPack
        ));
        assert_ne!(
            library.list().unwrap().matches[0].status,
            ReplayStatus::Ready
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn growing_watched_file_waits_for_two_equal_size_polls() {
        let root = temp_dir("watch-stable-size");
        let folder = root.join("dota-replays");
        let dem = write_demo(&folder, "9119");
        let active = Arc::new(AtomicBool::new(true));
        let library = controlled_library(
            root.clone(),
            ReplayLibraryLimits::default(),
            active,
            Arc::new(|| {}),
        );

        library.watch_replay_folder_for_test(&folder).unwrap();
        assert_eq!(
            library.list().unwrap().matches[0].status,
            ReplayStatus::Available
        );
        assert!(library.list().unwrap().jobs.is_empty());

        fs::OpenOptions::new()
            .append(true)
            .open(&dem)
            .unwrap()
            .write_all(b"growing")
            .unwrap();
        library.watch_replay_folder_for_test(&folder).unwrap();
        assert_eq!(
            library.list().unwrap().matches[0].status,
            ReplayStatus::Available
        );
        assert!(library.list().unwrap().jobs.is_empty());

        library.watch_replay_folder_for_test(&folder).unwrap();
        assert_eq!(
            library.list().unwrap().matches[0].status,
            ReplayStatus::Queued
        );
        assert_eq!(library.list().unwrap().jobs[0].status, ReplayStatus::Queued);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn match_start_during_parse_returns_available_and_requeues_after_barrier() {
        let root = temp_dir("match-start-retry");
        let source = root.join("source");
        let dem = write_demo(&source, "321");
        let active = Arc::new(AtomicBool::new(false));
        let runner = Arc::new(FixtureEngineRunner::starting_match_after_first_call(
            active.clone(),
        ));
        let active_for_gate = active.clone();
        let library = ReplayLibrary::new_with_engine_runner(
            root.clone(),
            None,
            false,
            Arc::new(move || active_for_gate.load(Ordering::Acquire)),
            Arc::new(|| {}),
            ReplayLibraryLimits::default(),
            runner.clone(),
        );
        library
            .add_available("321".to_owned(), &dem, ReplaySource::DotaReplaysFolder)
            .unwrap();
        library.parse("321").unwrap();

        wait_until(|| runner.calls.load(Ordering::Acquire) == 1);
        wait_until(|| library.list().unwrap().matches[0].status == ReplayStatus::Queued);
        assert_eq!(library.list().unwrap().jobs[0].status, ReplayStatus::Queued);
        assert!(active.load(Ordering::Acquire));
        assert_eq!(runner.calls.load(Ordering::Acquire), 1);

        active.store(false, Ordering::Release);
        wait_until(|| library.list().unwrap().matches[0].status == ReplayStatus::Ready);
        assert_eq!(runner.calls.load(Ordering::Acquire), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn removed_watched_entry_is_not_rediscovered_from_the_persistent_dota_file() {
        let root = temp_dir("removed-watched");
        let folder = root.join("dota-replays");
        let dem = write_demo(&folder, "9102");
        let active = Arc::new(AtomicBool::new(true));
        let library = controlled_library(
            root.clone(),
            ReplayLibraryLimits::default(),
            active,
            Arc::new(|| {}),
        );

        library.watch_replay_folder_for_test(&folder).unwrap();
        library.watch_replay_folder_for_test(&folder).unwrap();
        wait_until(|| library.list().unwrap().matches[0].status == ReplayStatus::Queued);
        library.remove("9102").unwrap();
        assert_eq!(library.watch_replay_folder_for_test(&folder).unwrap(), 0);

        assert!(library.list().unwrap().matches.is_empty());
        assert!(dem.exists());
        assert!(library
            .list()
            .unwrap()
            .jobs
            .iter()
            .all(|job| job.status != ReplayStatus::Queued));
        let reopened = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        assert_eq!(reopened.watch_replay_folder_for_test(&folder).unwrap(), 0);
        assert!(reopened.list().unwrap().matches.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_and_needs_pack_watched_entries_are_not_auto_requeued() {
        let root = temp_dir("auto-no-retry");
        let folder = root.join("dota-replays");
        write_demo(&folder, "9103");
        write_demo(&folder, "9104");
        let active = Arc::new(AtomicBool::new(true));
        let library = controlled_library(
            root.clone(),
            ReplayLibraryLimits::default(),
            active,
            Arc::new(|| {}),
        );
        library.set_auto_analyse(false);
        library.watch_replay_folder_for_test(&folder).unwrap();
        library
            .set_entry_status(
                "9103",
                ReplayStatus::Failed {
                    reason: "fixture_failure".to_owned(),
                },
            )
            .unwrap();
        library
            .set_entry_status("9104", ReplayStatus::NeedsPack)
            .unwrap();

        library.set_auto_analyse(true);
        library.watch_replay_folder_for_test(&folder).unwrap();
        thread::sleep(Duration::from_millis(30));

        assert!(library
            .list()
            .unwrap()
            .jobs
            .iter()
            .all(|job| job.status != ReplayStatus::Queued));
        assert_eq!(
            library.list().unwrap().matches.iter().find(|entry| entry.match_id == "9103").unwrap().status,
            ReplayStatus::Failed { reason: "fixture_failure".to_owned() }
        );
        assert_eq!(
            library.list().unwrap().matches.iter().find(|entry| entry.match_id == "9104").unwrap().status,
            ReplayStatus::NeedsPack
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn disabling_auto_parse_cancels_queued_jobs_but_not_an_already_parsing_job() {
        let queued_root = temp_dir("auto-cancel-queued");
        let queued_folder = queued_root.join("dota-replays");
        write_demo(&queued_folder, "9105");
        write_demo(&queued_folder, "9106");
        let active = Arc::new(AtomicBool::new(true));
        let queued_library = controlled_library(
            queued_root.clone(),
            ReplayLibraryLimits::default(),
            active.clone(),
            Arc::new(|| {}),
        );
        queued_library
            .watch_replay_folder_for_test(&queued_folder)
            .unwrap();
        queued_library
            .watch_replay_folder_for_test(&queued_folder)
            .unwrap();
        wait_until(|| {
            queued_library
                .list()
                .unwrap()
                .matches
                .iter()
                .all(|entry| entry.status == ReplayStatus::Queued)
        });

        queued_library.set_auto_analyse(false);

        assert!(queued_library
            .list()
            .unwrap()
            .matches
            .iter()
            .all(|entry| entry.status == ReplayStatus::Available));
        assert!(queued_library
            .list()
            .unwrap()
            .jobs
            .iter()
            .all(|job| job.status != ReplayStatus::Queued));
        active.store(false, Ordering::Release);
        thread::sleep(Duration::from_millis(30));
        assert!(queued_library
            .list()
            .unwrap()
            .matches
            .iter()
            .all(|entry| entry.status == ReplayStatus::Available));
        let _ = fs::remove_dir_all(queued_root);

        let running_root = temp_dir("auto-off-running");
        let running_folder = running_root.join("dota-replays");
        write_demo(&running_folder, "9107");
        let (started_tx, started_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = mpsc::channel();
        let continue_rx = Arc::new(Mutex::new(continue_rx));
        let event_count = Arc::new(AtomicU64::new(0));
        let event_count_for_sink = event_count.clone();
        let continue_rx_for_sink = continue_rx.clone();
        let sink: EventSink = Arc::new(move || {
            if event_count_for_sink.fetch_add(1, Ordering::SeqCst) == 3 {
                let _ = started_tx.send(());
                let _ = lock(&continue_rx_for_sink).recv_timeout(Duration::from_secs(3));
            }
        });
        let running_library = controlled_library(
            running_root.clone(),
            ReplayLibraryLimits::default(),
            Arc::new(AtomicBool::new(false)),
            sink,
        );
        running_library.watch_replay_folder_for_test(&running_folder).unwrap();
        running_library.watch_replay_folder_for_test(&running_folder).unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(running_library.list().unwrap().matches[0].status, ReplayStatus::Parsing);

        running_library.set_auto_analyse(false);
        assert_eq!(running_library.list().unwrap().matches[0].status, ReplayStatus::Parsing);

        continue_tx.send(()).unwrap();
        wait_until(|| running_library.list().unwrap().matches[0].status != ReplayStatus::Parsing);
        assert!(matches!(running_library.list().unwrap().matches[0].status, ReplayStatus::Failed { .. }));
        let _ = fs::remove_dir_all(running_root);
    }

    #[test]
    fn enabling_auto_parse_queues_available_backlog_newest_first_with_retention_cap() {
        let root = temp_dir("auto-backlog");
        let folder = root.join("dota-replays");
        let active = Arc::new(AtomicBool::new(true));
        let library = controlled_library(
            root.clone(),
            ReplayLibraryLimits {
                max_matches: 2,
                max_bytes: 1024 * 1024,
            },
            active,
            Arc::new(|| {}),
        );
        library.set_auto_analyse(false);
        for (index, match_id) in ["9201", "9202", "9203", "9204"].into_iter().enumerate() {
            let path = write_demo(&folder, match_id);
            fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(SystemTime::now() + Duration::from_secs(index as u64 * 60))
                .unwrap();
        }
        assert_eq!(library.watch_replay_folder_for_test(&folder).unwrap(), 4);
        assert_eq!(library.watch_replay_folder_for_test(&folder).unwrap(), 0);

        library.set_auto_analyse(true);
        wait_until(|| {
            library
                .list()
                .unwrap()
                .matches
                .iter()
                .filter(|entry| entry.status == ReplayStatus::Queued)
                .count()
                == 2
        });
        let queued = lock(&library.inner.jobs)
            .iter()
            .filter(|record| record.public.status == ReplayStatus::Queued)
            .filter_map(|record| record.match_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(queued, ["9204", "9203"]);
        assert_eq!(library.list().unwrap().matches.iter().filter(|entry| entry.status == ReplayStatus::Available).count(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn auto_parse_preference_persists_defaults_on_and_uses_default_for_corrupt_settings() {
        let data = temp_dir("auto-pref-store");
        let store = crate::analytics::AnalyticsStore::open(&data, None);
        let defaults = store.preferences().unwrap();
        assert!(auto_parse_enabled_or_default(Ok(defaults.auto_analyse)));

        let mut changed = defaults;
        changed.auto_analyse = false;
        store.write_preferences(&changed).unwrap();
        assert!(!crate::analytics::AnalyticsStore::open(&data, None)
            .preferences()
            .map(|preferences| preferences.auto_analyse)
            .unwrap());

        fs::write(store.root().join("settings.json"), b"{ corrupt").unwrap();
        assert!(auto_parse_enabled_or_default(
            crate::analytics::AnalyticsStore::open(&data, None)
                .preferences()
                .map(|preferences| preferences.auto_analyse)
        ));
        let _ = fs::remove_dir_all(data);
    }

    #[test]
    fn saved_auto_parse_off_is_applied_before_the_first_startup_watch_poll() {
        let settings_dir = temp_dir("startup-auto-pref");
        let store = crate::analytics::AnalyticsStore::open(&settings_dir, None);
        let mut preferences = store.preferences().unwrap();
        preferences.auto_analyse = false;
        store.write_preferences(&preferences).unwrap();
        let saved_auto_analyse = crate::analytics::AnalyticsStore::open(&settings_dir, None)
            .preferences()
            .unwrap()
            .auto_analyse;
        assert!(!saved_auto_analyse);

        let root = temp_dir("startup-auto-off");
        let folder = root.join("dota-replays");
        write_demo(&folder, "9876543211");
        let active = Arc::new(AtomicBool::new(true));
        let active_for_gate = active.clone();
        let library = ReplayLibrary::new_configured(
            root.clone(),
            None,
            false,
            Arc::new(move || active_for_gate.load(Ordering::Acquire)),
            Arc::new(|| {}),
            ReplayLibraryLimits::default(),
            saved_auto_analyse,
        );

        assert_eq!(library.watch_replay_folder_for_test(&folder).unwrap(), 1);
        let list = library.list().unwrap();
        assert_eq!(list.matches[0].status, ReplayStatus::Available);
        assert!(list.jobs.is_empty());

        let _ = fs::remove_dir_all(settings_dir);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn replay_library_logs_never_include_a_match_id_or_replay_path() {
        let root = temp_dir("private-replay-log");
        let folder = root.join("dota-replays");
        let match_id = "9876543210";
        let dem = write_demo(&folder, match_id);
        let logs = crate::logging::collect_test_logs(|| {
            let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
            library.set_auto_analyse(false);
            library.watch_replay_folder_for_test(&folder).unwrap();
            library.remove(match_id).unwrap();
        });
        let output = serde_json::to_string(&logs).unwrap();
        assert!(!output.contains(match_id));
        assert!(!output.contains(&dem.to_string_lossy().to_string()));
        assert!(dem.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manual_pick_remains_an_explicit_parse_even_when_auto_parse_is_off() {
        let root = temp_dir("manual-pick-off");
        let manual_folder = root.join("manual");
        let dem = write_demo(&manual_folder, "manual");
        let active = Arc::new(AtomicBool::new(true));
        let library = controlled_library(
            root.clone(),
            ReplayLibraryLimits::default(),
            active,
            Arc::new(|| {}),
        );
        library.set_auto_analyse(false);

        let job = library.parse(&dem.to_string_lossy()).unwrap();

        assert_eq!(job.status, ReplayStatus::Queued);
        assert!(library.list().unwrap().matches.is_empty());
        assert_eq!(library.list().unwrap().jobs.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reparse_upgrades_a_schema_30_entry_to_schema_31() {
        let root = temp_dir("reparse-upgrade");
        let source_folder = temp_dir("reparse-upgrade-source");
        let dem = write_demo(&source_folder, "321");
        let runner = Arc::new(FixtureEngineRunner::new(false));
        let library = reparse_test_library(
            root.clone(),
            Arc::new(AtomicBool::new(false)),
            runner.clone(),
        );
        seed_ready_bundle(&library, &dem, "321", ReplaySource::ManualPick, "3.0");

        let before = library.list().unwrap();
        let before_json = serde_json::to_value(&before).unwrap();
        assert_eq!(before_json["matches"][0]["bundle_schema"], "3.0");
        assert_eq!(before_json["matches"][0]["source_exists"], true);
        let job = library.reparse("321").unwrap();
        wait_until(|| {
            library.list().unwrap().jobs.iter().any(|candidate| {
                candidate.job_id == job.job_id && candidate.status == ReplayStatus::Ready
            })
        });

        let after = library.list().unwrap();
        assert_eq!(after.matches[0].status, ReplayStatus::Ready);
        let after_json = serde_json::to_value(&after).unwrap();
        assert_eq!(after_json["matches"][0]["bundle_schema"], "3.1");
        assert_eq!(
            library.open_bundle("321").unwrap().manifest["schema"],
            "3.1"
        );
        assert_eq!(runner.calls.load(Ordering::Acquire), 1);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(source_folder);
    }

    #[test]
    fn missing_reparse_source_returns_stable_reason_without_changing_bundle_or_job_history() {
        let root = temp_dir("reparse-missing");
        let source_folder = temp_dir("reparse-missing-source");
        let dem = write_demo(&source_folder, "321");
        let runner = Arc::new(FixtureEngineRunner::new(false));
        let library = reparse_test_library(
            root.clone(),
            Arc::new(AtomicBool::new(false)),
            runner.clone(),
        );
        seed_ready_bundle(&library, &dem, "321", ReplaySource::ManualPick, "3.0");
        let before = ["manifest.json", "match.json", "world.json", "meta.json"]
            .map(|name| fs::read(root.join("321").join(name)).unwrap());
        let index_before = fs::read(root.join("index.json")).unwrap();
        let jobs_before = library.list().unwrap().jobs.len();
        fs::remove_file(&dem).unwrap();

        assert_eq!(library.reparse("321").unwrap_err(), "source_missing");
        let after = ["manifest.json", "match.json", "world.json", "meta.json"]
            .map(|name| fs::read(root.join("321").join(name)).unwrap());
        assert_eq!(after, before);
        assert_eq!(fs::read(root.join("index.json")).unwrap(), index_before);
        assert_eq!(library.list().unwrap().jobs.len(), jobs_before);
        assert_eq!(runner.calls.load(Ordering::Acquire), 0);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(source_folder);
    }

    #[test]
    fn failed_reparse_preserves_the_previous_ready_bundle() {
        let root = temp_dir("reparse-failure");
        let source_folder = temp_dir("reparse-failure-source");
        let dem = write_demo(&source_folder, "321");
        let runner = Arc::new(FixtureEngineRunner::new(true));
        let library = reparse_test_library(
            root.clone(),
            Arc::new(AtomicBool::new(false)),
            runner.clone(),
        );
        seed_ready_bundle(&library, &dem, "321", ReplaySource::ManualPick, "3.0");
        let before = ["manifest.json", "match.json", "world.json", "meta.json"]
            .map(|name| fs::read(root.join("321").join(name)).unwrap());
        let job = library.reparse("321").unwrap();
        wait_until(|| {
            library.list().unwrap().jobs.iter().any(|candidate| {
                candidate.job_id == job.job_id
                    && candidate.status
                        == (ReplayStatus::Failed {
                            reason: "engine_failed".to_owned(),
                        })
            })
        });

        let after = ["manifest.json", "match.json", "world.json", "meta.json"]
            .map(|name| fs::read(root.join("321").join(name)).unwrap());
        assert_eq!(after, before);
        assert_eq!(
            library.list().unwrap().matches[0].status,
            ReplayStatus::Ready
        );
        assert_eq!(
            library.open_bundle("321").unwrap().manifest["schema"],
            "3.0"
        );
        assert_eq!(runner.calls.load(Ordering::Acquire), 1);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(source_folder);
    }

    #[test]
    fn source_removed_during_reparse_reports_source_missing_and_preserves_bundle() {
        let root = temp_dir("reparse-source-removed");
        let source_folder = temp_dir("reparse-source-removed-source");
        let dem = write_demo(&source_folder, "321");
        let runner = Arc::new(FixtureEngineRunner::removing_source_before_failure());
        let library = reparse_test_library(
            root.clone(),
            Arc::new(AtomicBool::new(false)),
            runner.clone(),
        );
        seed_ready_bundle(&library, &dem, "321", ReplaySource::ManualPick, "3.0");
        let before = ["manifest.json", "match.json", "world.json", "meta.json"]
            .map(|name| fs::read(root.join("321").join(name)).unwrap());
        let job = library.reparse("321").unwrap();
        wait_until(|| {
            library.list().unwrap().jobs.iter().any(|candidate| {
                candidate.job_id == job.job_id
                    && candidate.status
                        == (ReplayStatus::Failed {
                            reason: "source_missing".to_owned(),
                        })
            })
        });

        let after = ["manifest.json", "match.json", "world.json", "meta.json"]
            .map(|name| fs::read(root.join("321").join(name)).unwrap());
        assert_eq!(after, before);
        assert_eq!(
            library.list().unwrap().matches[0].status,
            ReplayStatus::Ready
        );
        assert_eq!(runner.calls.load(Ordering::Acquire), 1);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(source_folder);
    }

    #[test]
    fn reparse_waits_for_the_match_barrier_to_clear() {
        let root = temp_dir("reparse-match-barrier");
        let source_folder = temp_dir("reparse-match-barrier-source");
        let dem = write_demo(&source_folder, "321");
        let active = Arc::new(AtomicBool::new(true));
        let runner = Arc::new(FixtureEngineRunner::new(false));
        let library = reparse_test_library(root.clone(), active.clone(), runner.clone());
        seed_ready_bundle(&library, &dem, "321", ReplaySource::ManualPick, "3.0");
        let job = library.reparse("321").unwrap();
        thread::sleep(Duration::from_millis(50));
        let queued = library
            .list()
            .unwrap()
            .jobs
            .into_iter()
            .find(|candidate| candidate.job_id == job.job_id)
            .unwrap();
        assert_eq!(queued.status, ReplayStatus::Queued);
        assert_eq!(runner.calls.load(Ordering::Acquire), 0);

        active.store(false, Ordering::Release);
        wait_until(|| {
            library.list().unwrap().jobs.iter().any(|candidate| {
                candidate.job_id == job.job_id && candidate.status == ReplayStatus::Ready
            })
        });
        assert_eq!(runner.calls.load(Ordering::Acquire), 1);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(source_folder);
    }

    #[test]
    fn automatic_queue_does_not_reparse_ready_entries() {
        let root = temp_dir("reparse-not-automatic");
        let source_folder = temp_dir("reparse-not-automatic-source");
        let dem = write_demo(&source_folder, "321");
        let runner = Arc::new(FixtureEngineRunner::new(false));
        let library = reparse_test_library(
            root.clone(),
            Arc::new(AtomicBool::new(false)),
            runner.clone(),
        );
        seed_ready_bundle(
            &library,
            &dem,
            "321",
            ReplaySource::DotaReplaysFolder,
            "3.0",
        );
        library.set_auto_analyse(true);
        assert_eq!(
            library
                .watch_replay_folder_for_test(&source_folder)
                .unwrap(),
            0
        );
        let job = library.reparse("321").unwrap();
        wait_until(|| {
            library.list().unwrap().jobs.iter().any(|candidate| {
                candidate.job_id == job.job_id && candidate.status == ReplayStatus::Ready
            })
        });
        assert_eq!(
            library
                .watch_replay_folder_for_test(&source_folder)
                .unwrap(),
            0
        );
        thread::sleep(Duration::from_millis(50));
        assert_eq!(runner.calls.load(Ordering::Acquire), 1);
        assert_eq!(library.list().unwrap().jobs.len(), 1);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(source_folder);
    }

    #[test]
    fn clear_removes_all_library_entries_and_match_folders() {
        let root = temp_dir("clear");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        for id in ["201", "202"] {
            let dem = source.join(format!("{id}.dem"));
            fs::write(&dem, b"PBDEMS2\0fixture").unwrap();
            library
                .add_available(id.to_owned(), &dem, ReplaySource::ManualPick)
                .unwrap();
        }
        library.clear().unwrap();
        assert!(library.list().unwrap().matches.is_empty());
        assert!(!root.join("201").exists());
        assert!(!root.join("202").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clear_refuses_while_a_manual_parse_job_is_running() {
        let root = temp_dir("clear-running-manual");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        let dem = source.join("203.dem");
        fs::write(&dem, b"PBDEMS2\0fixture").unwrap();
        library
            .add_available("203".to_owned(), &dem, ReplaySource::ManualPick)
            .unwrap();
        library.record_job(
            &ReplayJob {
                job_id: "job-test-running".to_owned(),
                match_id: None,
                operation: ReplayOperation::Parse,
                status: ReplayStatus::Parsing,
            },
            None,
            ParseMode::Manual,
        );
        assert_eq!(library.clear(), Err("parse_in_progress"));
        assert_eq!(library.list().unwrap().matches.len(), 1);
        assert!(root.join("203").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_watch_detects_a_new_replay_file() {
        let root = temp_dir("watch-store");
        let folder = temp_dir("watch-folder");
        let dem = folder.join("987654.dem");
        fs::write(&dem, b"PBDEMS2\0fixture").unwrap();
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        library.set_auto_analyse(false);
        assert_eq!(library.watch_replay_folder_for_test(&folder).unwrap(), 1);
        assert_eq!(library.watch_replay_folder_for_test(&folder).unwrap(), 0);
        let list = library.list().unwrap();
        assert_eq!(list.matches.len(), 1);
        assert_eq!(list.matches[0].status, ReplayStatus::Available);
        assert_eq!(list.matches[0].source, ReplaySource::DotaReplaysFolder);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(folder);
    }

    #[test]
    fn active_match_gate_rejects_parse_before_sidecar_invocation() {
        let mut spawn_called = false;
        let match_active = || true;
        let result = engine::ensure_idle(&match_active);
        if result.is_ok() {
            spawn_called = true;
        }
        assert_eq!(result, Err(engine::EngineFailure::MatchBecameActive));
        assert!(!spawn_called);
    }
}
