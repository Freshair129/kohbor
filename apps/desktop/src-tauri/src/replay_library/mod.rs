mod engine;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const INDEX_SCHEMA_VERSION: u32 = 1;
const MAX_INDEX_BYTES: u64 = 4 * 1024 * 1024;
const WATCH_INTERVAL: Duration = Duration::from_secs(30);
const MATCH_IDLE_POLL: Duration = Duration::from_millis(250);
const MAX_JOB_HISTORY: usize = 50;
pub(crate) const EVENT_NAME: &str = "replay-library://state-changed";
static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TOMBSTONE_ID: AtomicU64 = AtomicU64::new(1);

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
    pub bundle_bytes: u64,
    pub storage_bytes: u64,
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
    #[serde(default)]
    auto_queued: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayJob {
    pub job_id: String,
    pub status: ReplayStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayLibraryList {
    pub matches: Vec<ReplayLibraryEntry>,
    pub jobs: Vec<ReplayJob>,
    pub watched_replay_folder_path: Option<String>,
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
}

struct Inner {
    root: PathBuf,
    staging: PathBuf,
    bundled_engine: Option<PathBuf>,
    allow_development_engine: bool,
    limits: Mutex<ReplayLibraryLimits>,
    match_active: Arc<dyn Fn() -> bool + Send + Sync>,
    emit_state_changed: EventSink,
    state: Mutex<StoreState>,
    jobs: Mutex<VecDeque<JobRecord>>,
    operation: Mutex<()>,
    worker_available: AtomicBool,
    auto_analyse: AtomicBool,
    auto_backlog_queued: AtomicBool,
    watch_initialized: AtomicBool,
}

#[derive(Clone)]
struct ParseRequest {
    job_id: String,
    match_id: Option<String>,
    source: ReplaySource,
    source_path: PathBuf,
    automatic: bool,
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
                limits: Mutex::new(limits),
                match_active,
                emit_state_changed,
                state: Mutex::new(StoreState { index, unavailable }),
                jobs: Mutex::new(VecDeque::new()),
                operation: Mutex::new(()),
                worker_available: AtomicBool::new(false),
                auto_analyse: AtomicBool::new(true),
                auto_backlog_queued: AtomicBool::new(false),
                watch_initialized: AtomicBool::new(false),
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
            .spawn(move || loop {
                if let Ok(game) = crate::intake::locate_game() {
                    let replay_dir = game.join("game").join("dota").join("replays");
                    // Errors are quiet because a local Dota install is optional.
                    let _ = library.watch_replay_folder(&replay_dir);
                }
                thread::sleep(WATCH_INTERVAL);
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
        let state = lock(&self.inner.state);
        if let Some(reason) = state.unavailable {
            return Err(reason);
        }
        let mut matches = state.index.entries.clone();
        for entry in &mut matches {
            enrich_list_entry(&self.inner.root, entry);
        }
        matches.sort_by(|left, right| right.created_at_unix_ms.cmp(&left.created_at_unix_ms));
        let jobs = lock(&self.inner.jobs)
            .iter()
            .map(|record| record.public.clone())
            .collect();
        let watched_replay_folder_path = crate::intake::locate_game().ok().map(|game| {
            game.join("game")
                .join("dota")
                .join("replays")
                .to_string_lossy()
                .into_owned()
        });
        Ok(ReplayLibraryList {
            matches,
            jobs,
            watched_replay_folder_path,
        })
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
            self.inner.auto_backlog_queued.store(false, Ordering::Release);
            return;
        }

        if self.inner.watch_initialized.load(Ordering::Acquire) {
            let _ = self.queue_available_auto_backlog_locked();
            self.inner.auto_backlog_queued.store(true, Ordering::Release);
        }
    }

    pub fn set_limits(&self, limits: ReplayLibraryLimits) -> Result<(), &'static str> {
        if !(5..=100).contains(&limits.max_matches) || limits.max_bytes == 0 {
            return Err("invalid_library_limits");
        }
        let _operation = lock(&self.inner.operation);
        let mut state = lock(&self.inner.state);
        let mut next = state.index.clone();
        let evicted = self.trim_to_limits_with(&mut next, "", limits)?;
        let tombstones = evicted.iter().filter_map(|id| move_to_tombstone(&self.inner.root, id)).collect::<Vec<_>>();
        if write_index(&self.inner.root, &next).is_err() {
            restore_tombstones(&tombstones);
            return Err("storage_unavailable");
        }
        state.index = next;
        drop(state);
        for (_, tombstone) in tombstones { let _ = fs::remove_dir_all(tombstone); }
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
        let tombstones = evicted.iter().filter_map(|id| move_to_tombstone(&self.inner.root, id)).collect::<Vec<_>>();
        let Some(updated) = next.entries.iter().find(|entry| entry.match_id == match_id).cloned() else {
            restore_tombstones(&tombstones);
            return Err("match_not_found");
        };
        let mut updated = updated;
        updated.storage_bytes = updated.bundle_bytes + entry_meta_bytes(&updated)?.len() as u64;
        if write_meta(&self.inner.root.join(match_id), &updated).is_err() {
            restore_tombstones(&tombstones);
            return Err("storage_unavailable");
        }
        if let Some(entry) = next.entries.iter_mut().find(|entry| entry.match_id == match_id) { entry.storage_bytes = updated.storage_bytes; }
        if write_index(&self.inner.root, &next).is_err() {
            restore_tombstones(&tombstones);
            if let Some(old) = state.index.entries.iter().find(|entry| entry.match_id == match_id) { let _ = write_meta(&self.inner.root.join(match_id), old); }
            return Err("storage_unavailable");
        }
        state.index = next;
        drop(state);
        for (_, tombstone) in tombstones { let _ = fs::remove_dir_all(tombstone); }
        self.emit_state_changed();
        Ok(())
    }

    pub fn open_bundle(&self, match_id: &str) -> Result<ReplayBundle, &'static str> {
        validate_match_id(match_id).map_err(|_| "invalid_match_id")?;
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
        if is_match_id(id_or_path) {
            self.queue_existing_match(sender, id_or_path)
        } else {
            self.queue_manual_path(sender, Path::new(id_or_path))
        }
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
        self.queue_existing_match_locked(sender, &match_id, automatic)
    }

    fn queue_existing_match_locked(
        &self,
        sender: &Sender<ParseRequest>,
        match_id: &str,
        automatic: bool,
    ) -> Result<ReplayJob, &'static str> {
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
            if automatic
                && (entry.source != ReplaySource::DotaReplaysFolder
                    || entry.status != ReplayStatus::Available
                    || state.index.suppressed_watched_match_ids.iter().any(|id| id == match_id))
            {
                return Err("auto_parse_not_eligible");
            }
            match entry.status {
                ReplayStatus::Ready => return Err("already_ready"),
                ReplayStatus::Queued | ReplayStatus::Parsing => return Err("already_queued"),
                _ => {}
            }
            (PathBuf::from(&entry.source_path), entry.source)
        };
        engine::validate_input_path(&source_path).map_err(|failure| failure.code())?;
        let job = ReplayJob {
            job_id: next_job_id(),
            status: ReplayStatus::Queued,
        };
        self.set_entry_auto_status_locked(&match_id, ReplayStatus::Queued, automatic)?;
        self.record_job(&job, Some(match_id.to_owned()));
        self.emit_state_changed();
        if sender
            .send(ParseRequest {
                job_id: job.job_id.clone(),
                match_id: Some(match_id.to_owned()),
                source,
                source_path,
                automatic,
            })
            .is_err()
        {
            let failed = ReplayStatus::Failed {
                reason: "worker_unavailable".to_owned(),
            };
            let _ = self.set_entry_auto_status_locked(&match_id, failed.clone(), false);
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
            status: ReplayStatus::Queued,
        };
        let available_job = ReplayJob {
            job_id,
            status: ReplayStatus::Available,
        };
        self.record_job(&available_job, None);
        self.emit_state_changed();
        self.set_job_status(&job.job_id, ReplayStatus::Queued);
        self.emit_state_changed();
        if sender
            .send(ParseRequest {
                job_id: job.job_id.clone(),
                match_id: None,
                source: ReplaySource::ManualPick,
                source_path,
                automatic: false,
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
            if entry.status == ReplayStatus::Parsing {
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

    fn watch_replay_folder(&self, folder: &Path) -> Result<usize, &'static str> {
        let entries = fs::read_dir(folder).map_err(|_| "replay_folder_unavailable")?;
        let first_poll = !self.inner.watch_initialized.swap(true, Ordering::AcqRel);
        let mut watched = Vec::new();
        for item in entries {
            let item = item.map_err(|_| "replay_folder_unavailable")?;
            if !item
                .file_type()
                .map_err(|_| "replay_folder_unavailable")?
                .is_file()
            {
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
            let metadata = item.metadata().map_err(|_| "replay_folder_unavailable")?;
            if metadata.len() == 0 || metadata.len() > engine::MAX_DEM_BYTES {
                continue;
            }
            let modified = metadata.modified().ok().and_then(system_time_unix_ms);
            watched.push((modified, match_id, path));
        }
        watched.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| right.1.cmp(&left.1))
        });

        let mut discovered = 0;
        let mut newly_discovered = Vec::new();
        for (modified, match_id, path) in watched {
            if self.add_available(match_id.clone(), &path, ReplaySource::DotaReplaysFolder)? {
                discovered += 1;
                newly_discovered.push((modified, match_id));
            }
        }

        if self.inner.auto_analyse.load(Ordering::Acquire) {
            if first_poll || !self.inner.auto_backlog_queued.load(Ordering::Acquire) {
                let _operation = lock(&self.inner.operation);
                let _ = self.queue_available_auto_backlog_locked();
                self.inner.auto_backlog_queued.store(true, Ordering::Release);
            } else {
                self.queue_newly_discovered_auto(&newly_discovered);
            }
        }
        Ok(discovered)
    }

    fn queue_newly_discovered_auto(&self, discovered: &[(Option<u64>, String)]) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        let _operation = lock(&self.inner.operation);
        if !self.inner.auto_analyse.load(Ordering::Acquire) {
            return;
        }
        let available_slots = self.remaining_auto_slots();
        let mut ordered = discovered.to_vec();
        ordered.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| right.1.cmp(&left.1))
        });
        let mut queued_any = false;
        for (_, match_id) in ordered.into_iter().take(available_slots) {
            if self
                .queue_existing_match_locked(sender, &match_id, true)
                .is_ok()
            {
                queued_any = true;
            }
        }
        if queued_any {
            self.emit_state_changed();
        }
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
                    entry.source == ReplaySource::DotaReplaysFolder
                        && entry.status == ReplayStatus::Available
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
                    )
                })
                .collect::<Vec<_>>()
        };
        available.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| right.1.cmp(&left.1))
        });
        let slots = self.remaining_auto_slots();
        let mut queued = 0;
        for (_, match_id) in available.into_iter().take(slots) {
            if self
                .queue_existing_match_locked(sender, &match_id, true)
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
        let mut jobs = lock(&self.inner.jobs);
        for record in jobs.iter_mut().filter(|record| {
            record.match_id.as_deref() == Some(match_id)
                && record.public.status == ReplayStatus::Queued
        }) {
            record.public.status = ReplayStatus::Failed {
                reason: "cancelled".to_owned(),
            };
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
        let mut tombstones = Vec::new();
        for old_id in evicted {
            if let Some(tombstone) = move_to_tombstone(&self.inner.root, &old_id) {
                tombstones.push(tombstone);
            }
        }
        if fs::rename(&stage, &target).is_err() {
            restore_tombstones(&tombstones);
            let _ = fs::remove_dir_all(stage);
            return Err("storage_unavailable");
        }
        if write_index(&self.inner.root, &next).is_err() {
            let _ = fs::remove_dir_all(&target);
            restore_tombstones(&tombstones);
            return Err("storage_unavailable");
        }
        state.index = next;
        drop(state);
        for (_, tombstone) in tombstones {
            let _ = fs::remove_dir_all(tombstone);
        }
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
            let result = request.match_id.as_deref().map_or(Ok(()), |match_id| {
                self.set_entry_auto_status_locked(
                    match_id,
                    ReplayStatus::Parsing,
                    request.automatic,
                )
            });
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
        engine::validate_input_path(&request.source_path)?;
        let dem_sha256 = engine::sha256_file(&request.source_path)
            .map_err(|_| engine::EngineFailure::InvalidInput)?;
        let stage = self
            .create_staging_dir(&request.job_id)
            .map_err(|_| engine::EngineFailure::StorageConflict)?;
        let development_engine = development_engine_path(self.inner.allow_development_engine);
        let run = engine::run_pinned_engine(
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
                return Err(failure);
            }
        };
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
        if let Err(failure) = self.install_parsed_bundle(&stage, entry.clone()) {
            let _ = fs::remove_dir_all(stage);
            return Err(failure);
        }
        Ok(validated.match_id)
    }

    fn install_parsed_bundle(
        &self,
        stage: &Path,
        entry: ReplayLibraryEntry,
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
        if existing
            .as_ref()
            .is_some_and(|candidate| candidate.status == ReplayStatus::Ready)
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
        for old_id in evicted {
            if let Some(tombstone) = move_to_tombstone(&self.inner.root, &old_id) {
                tombstones.push(tombstone);
            }
        }
        if fs::rename(stage, &target).is_err() {
            restore_tombstones(&tombstones);
            return Err(engine::EngineFailure::StorageConflict);
        }
        if write_index(&self.inner.root, &next).is_err() {
            let _ = fs::remove_dir_all(&target);
            restore_tombstones(&tombstones);
            return Err(engine::EngineFailure::StorageConflict);
        }
        state.index = next;
        drop(state);
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
        let pinned_usage = index.entries.iter().filter(|entry| entry.pinned)
            .fold(0u64, |total, entry| total.saturating_add(entry.storage_bytes));
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
                .filter(|entry| !entry.pinned && entry.match_id != protected_match_id
                    && (!over_count || entry.status == ReplayStatus::Ready))
                .min_by_key(|entry| entry.created_at_unix_ms)
                .map(|entry| entry.match_id.clone()) else {
                    // Pinned bundles are user-retained data. If they alone exceed the cap,
                    // preserve them and expose actual usage so the UI can show over-cap state.
                    if pinned_usage > limits.max_bytes { break; }
                    return Err("output_too_large");
                };
            if index
                .entries
                .iter()
                .find(|entry| entry.match_id == oldest)
                .is_some_and(|entry| entry.source == ReplaySource::DotaReplaysFolder)
                && !index.suppressed_watched_match_ids.contains(&oldest)
            {
                index.suppressed_watched_match_ids.push(oldest.clone());
            }
            index.entries.retain(|entry| entry.match_id != oldest);
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

    fn record_job(&self, job: &ReplayJob, match_id: Option<String>) {
        let mut jobs = lock(&self.inner.jobs);
        if jobs.len() >= MAX_JOB_HISTORY {
            jobs.pop_front();
        }
        jobs.push_back(JobRecord {
            public: job.clone(),
            match_id,
        });
    }

    fn set_job_match_id(&self, job_id: &str, match_id: &str) {
        if let Some(record) = lock(&self.inner.jobs)
            .iter_mut()
            .find(|record| record.public.job_id == job_id)
        {
            record.match_id = Some(match_id.to_owned());
        }
    }

    fn set_job_status(&self, job_id: &str, status: ReplayStatus) {
        if let Some(record) = lock(&self.inner.jobs)
            .iter_mut()
            .find(|record| record.public.job_id == job_id)
        {
            record.public.status = status;
        }
    }

    fn request_is_queued(&self, request: &ParseRequest) -> bool {
        if let Some(match_id) = &request.match_id {
            if request.automatic && !self.inner.auto_analyse.load(Ordering::Acquire) {
                return false;
            }
            return lock(&self.inner.state)
                .index
                .entries
                .iter()
                .any(|entry| {
                    entry.match_id == *match_id
                        && entry.status == ReplayStatus::Queued
                        && (!request.automatic || entry.auto_queued)
                });
        }
        lock(&self.inner.jobs).iter().any(|record| {
            record.public.job_id == request.job_id && record.public.status == ReplayStatus::Queued
        })
    }

    fn finish_job_failure(&self, request: &ParseRequest, status: ReplayStatus) {
        if let Some(match_id) = &request.match_id {
            let _ = self.set_entry_auto_status_locked(match_id, status.clone(), false);
        }
        self.set_job_status(&request.job_id, status);
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
            return (ReplayIndex::default(), Some("index_invalid"));
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
        return (ReplayIndex::default(), Some("index_invalid"));
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
        return (ReplayIndex::default(), Some("index_invalid"));
    }
    (index, None)
}

fn recover_interrupted_entries(root: &Path, index: &mut ReplayIndex) -> Result<(), &'static str> {
    let mut changed = false;
    for entry in &mut index.entries {
        if matches!(entry.status, ReplayStatus::Queued | ReplayStatus::Parsing) {
            entry.status = ReplayStatus::Failed {
                reason: "interrupted".to_owned(),
            };
            entry.auto_queued = false;
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

fn entry_meta_bytes(entry: &ReplayLibraryEntry) -> Result<Vec<u8>, &'static str> {
    serde_json::to_vec_pretty(&entry.meta()).map_err(|_| "storage_unavailable")
}

fn write_meta(directory: &Path, entry: &ReplayLibraryEntry) -> Result<(), &'static str> {
    fs::create_dir_all(directory).map_err(|_| "storage_unavailable")?;
    let bytes = entry_meta_bytes(entry)?;
    crate::atomic::write(&directory.join("meta.json"), &bytes).map_err(|_| "storage_unavailable")
}

fn index_usage(index: &ReplayIndex) -> u64 {
    let index_bytes = serde_json::to_vec(index)
        .map(|bytes| bytes.len() as u64)
        .unwrap_or(u64::MAX);
    index.entries.iter().fold(index_bytes, |total, entry| {
        total.saturating_add(entry.storage_bytes)
    })
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

fn move_to_tombstone(root: &Path, match_id: &str) -> Option<(PathBuf, PathBuf)> {
    move_path_to_tombstone(&root.join(match_id), root)
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
        job.public.status = ReplayStatus::Failed {
            reason: "cancelled".to_owned(),
        };
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
    use std::sync::atomic::{AtomicU64, Ordering};
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
    fn persisted_interrupted_jobs_reopen_as_failed() {
        let root = temp_dir("interrupted");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        let dem = source.join("204.dem");
        fs::write(&dem, b"PBDEMS2\0fixture").unwrap();
        library
            .add_available("204".to_owned(), &dem, ReplaySource::ManualPick)
            .unwrap();
        library
            .set_entry_status("204", ReplayStatus::Parsing)
            .unwrap();

        let reopened = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits::default());
        let reopened_list = reopened.list().unwrap();
        let entry = &reopened_list.matches[0];
        assert_eq!(
            entry.status,
            ReplayStatus::Failed {
                reason: "interrupted".to_owned()
            }
        );
        let meta: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("204").join("meta.json")).unwrap()).unwrap();
        assert_eq!(meta["status"]["state"], "failed");
        assert_eq!(meta["status"]["reason"], "interrupted");
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
        for (id, file) in [("101", "one.dem"), ("102", "two.dem"), ("103", "three.dem")] {
            let path = source.join(file);
            fs::write(&path, b"PBDEMS2\0fixture").unwrap();
            library
                .add_available(id.to_owned(), &path, ReplaySource::DotaReplaysFolder)
                .unwrap();
            library.set_entry_status(id, ReplayStatus::Ready).unwrap();
        }
        library.set_pinned("103", false).unwrap();
        let ids = library
            .list()
            .unwrap()
            .matches
            .into_iter()
            .map(|entry| entry.match_id)
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"102".to_owned()));
        assert!(ids.contains(&"103".to_owned()));
        assert!(!root.join("101").exists());
        assert!(source.join("one.dem").exists(), "retention must never delete the user's .dem");
        library.remove("102").unwrap();
        assert!(!root.join("102").exists());
        assert_eq!(library.list().unwrap().matches.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retention_keeps_newest_unpinned_ready_bundles_and_pins_and_preserves_notes_and_dem() {
        let root = temp_dir("retention-pins");
        let data = temp_dir("retention-notes");
        let library = ReplayLibrary::new_for_tests(root.clone(), ReplayLibraryLimits { max_matches: 100, max_bytes: 1024 * 1024 });
        let source = root.join("source"); fs::create_dir_all(&source).unwrap();
        let notes = crate::analytics::AnalyticsStore::open(&data, Some([9; 32]));
        for (index, id) in ["101", "102", "103", "104", "105", "106", "107"].into_iter().enumerate() {
            let dem = source.join(format!("{id}.dem")); fs::write(&dem, b"PBDEMS2\0fixture").unwrap();
            library.add_available(id.to_owned(), &dem, ReplaySource::DotaReplaysFolder).unwrap();
            library.set_entry_status(id, ReplayStatus::Ready).unwrap();
            let mut state = lock(&library.inner.state);
            if let Some(entry) = state.index.entries.iter_mut().find(|entry| entry.match_id == id) {
                entry.created_at_unix_ms = index as u64 + 1;
                entry.bundle_bytes = 100;
                entry.storage_bytes = 200;
                write_meta(&root.join(id), entry).unwrap();
                write_index(&root, &state.index).unwrap();
            }
        }
        notes.write_note("102", "Coach note", "kept after replay eviction").unwrap();
        library.set_pinned("101", true).unwrap();
        library.set_limits(ReplayLibraryLimits { max_matches: 5, max_bytes: 1024 * 1024 }).unwrap();
        let ids = library.list().unwrap().matches.into_iter().map(|entry| entry.match_id).collect::<HashSet<_>>();
        assert_eq!(ids, ["101", "103", "104", "105", "106", "107"].into_iter().map(str::to_owned).collect());
        assert!(source.join("102.dem").exists());
        assert_eq!(notes.read_note("102", "Coach note.md").unwrap(), "kept after replay eviction");
        assert!(!root.join("102").exists());
        let _ = fs::remove_dir_all(root); let _ = fs::remove_dir_all(data);
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
        library.set_limits(ReplayLibraryLimits { max_matches: 100, max_bytes: 1500 }).unwrap();
        let list = library.list().unwrap();
        assert_eq!(list.matches.len(), 1); assert_eq!(list.matches[0].match_id, "203");
        for id in ["201", "202", "203"] { assert!(source.join(format!("{id}.dem")).exists()); }
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
        library.set_limits(ReplayLibraryLimits { max_matches: 20, max_bytes: 800 }).unwrap();
        let list = library.list().unwrap();
        assert_eq!(list.matches.iter().map(|entry| entry.match_id.as_str()).collect::<HashSet<_>>(), ["301", "302"].into_iter().collect());
        assert_eq!(list.matches.iter().map(|entry| entry.storage_bytes).sum::<u64>(), 1200);
        for id in ["301", "302", "303"] { assert!(source.join(format!("{id}.dem")).exists()); }
        assert!(root.join("301").exists() && root.join("302").exists());
        assert!(!root.join("303").exists());
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
        let (parsing_tx, parsing_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = mpsc::channel();
        let continue_rx = Arc::new(Mutex::new(continue_rx));
        let continue_rx_for_sink = continue_rx.clone();
        let event_count = Arc::new(AtomicU64::new(0));
        let event_count_for_sink = event_count.clone();
        let sink: EventSink = Arc::new(move || {
            if event_count_for_sink.fetch_add(1, Ordering::SeqCst) == 3 {
                let _ = parsing_tx.send(());
                let _ = lock(&continue_rx_for_sink).recv_timeout(Duration::from_secs(3));
            }
        });
        let library = controlled_library(
            root.clone(),
            ReplayLibraryLimits::default(),
            active.clone(),
            sink,
        );

        assert_eq!(library.watch_replay_folder_for_test(&folder).unwrap(), 1);
        wait_until(|| library.list().unwrap().matches[0].status == ReplayStatus::Queued);
        assert_eq!(library.list().unwrap().jobs[0].status, ReplayStatus::Queued);
        assert!(dem.exists());
        assert!(parsing_rx.try_recv().is_err());

        active.store(false, Ordering::Release);
        parsing_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(library.list().unwrap().matches[0].status, ReplayStatus::Parsing);
        continue_tx.send(()).unwrap();
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
        assert_ne!(library.list().unwrap().matches[0].status, ReplayStatus::Ready);
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
        queued_library.watch_replay_folder_for_test(&queued_folder).unwrap();
        wait_until(|| queued_library.list().unwrap().matches.iter().all(|entry| entry.status == ReplayStatus::Queued));

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
            ReplayLibraryLimits { max_matches: 2, max_bytes: 1024 * 1024 },
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

        library.set_auto_analyse(true);
        wait_until(|| library.list().unwrap().matches.iter().filter(|entry| entry.status == ReplayStatus::Queued).count() == 2);
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
                status: ReplayStatus::Parsing,
            },
            None,
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
