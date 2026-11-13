//! Bounded command bridge and single-owner package-store thread.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use kohbor_events::EventId;
use kohbor_pack::catalog::Catalog;
use kohbor_pack::container;
use kohbor_pack::store::{
    ActivationPolicy, ActivationState, InstallRequest, NoHook, Status, Store,
};
use kohbor_pack::{limits::MAX_CONTAINER_BYTES, Limits};
use serde_json::{json, Value};

use crate::audio::lanes::{Control, EngineShared, EngineTx};
use crate::audio::resolve::PrewarmTable;
use crate::media::inventory::{facts_from_table, MediaFacts};
use crate::media::prewarm::{build_table, VisualIndexHandle};
use crate::media::probe::{ProbeItem, ProbeRequest, ProbeState};

pub type ProbeEmitter = Arc<dyn Fn(Value) + Send + Sync>;

pub type FactsHandle = Arc<Mutex<MediaFacts>>;

pub const DEFAULT_RESUME_INTERVAL_MS: u64 = 2_000;
const JOB_CAPACITY: usize = 4;
const JOB_TIMEOUT: Duration = Duration::from_millis(10_000);
const MAX_INSPECT_PATH_CHARS: usize = 1_024;

pub fn new_facts_handle() -> FactsHandle {
    Arc::new(Mutex::new(MediaFacts {
        store_ok: false,
        critical: Vec::new(),
    }))
}

pub struct ServiceConfig {
    pub root: PathBuf,
    pub catalog: Catalog,
    pub engine: EngineTx,
    pub engine_shared: Arc<EngineShared>,
    pub visual_index: VisualIndexHandle,
    pub facts: FactsHandle,
    pub probe: Arc<dyn Fn(EventId) -> Option<bool> + Send + Sync>,
    pub match_active: Arc<dyn Fn() -> bool + Send + Sync>,
    pub sleeper: Box<dyn FnMut(u64) + Send>,
    /// Set to `DEFAULT_RESUME_INTERVAL_MS` in production; injectable for tests.
    pub resume_interval_ms: u64,
    /// Zero disables loader sleeps in tests. A positive value enables the 25%
    /// duty-cycle sleep and acts as its minimum millisecond delay.
    pub pause_ms_per_key: u64,
}

#[derive(Clone)]
pub struct MediaHandle {
    jobs: SyncSender<Job>,
    shutdown_requested: Arc<AtomicBool>,
    service_stopped: Arc<AtomicBool>,
    probe_emitter: Arc<Mutex<Option<ProbeEmitter>>>,
}

impl MediaHandle {
    pub fn inspect(&self, source_path: &str) -> Result<Value, String> {
        self.request(JobKind::Inspect(source_path.to_owned()))
    }

    pub fn install(&self, request_json: &[u8]) -> Result<Value, String> {
        self.request(JobKind::Install(request_json.to_vec()))
    }

    pub fn activate(
        &self,
        request_id: &str,
        install_id: &str,
        policy: ActivationPolicy,
    ) -> Result<Value, String> {
        self.request(JobKind::Activate {
            request_id: request_id.to_owned(),
            install_id: install_id.to_owned(),
            policy,
        })
    }

    pub fn list(&self) -> Result<Value, String> {
        self.request(JobKind::List)
    }

    pub fn probe_report(
        &self,
        install_id: &str,
        revision: u64,
        path_sha256: &str,
        ok: bool,
        duration_ms: u64,
    ) -> Result<Value, String> {
        self.request(JobKind::ProbeReport {
            install_id: install_id.to_owned(),
            revision,
            path_sha256: path_sha256.to_owned(),
            ok,
            duration_ms,
        })
    }

    pub fn probe_pending(&self) -> Result<Value, String> {
        self.request(JobKind::ProbePending)
    }

    pub fn reemit_probe(&self) {
        if self.shutdown_requested.load(Ordering::Acquire)
            || self.service_stopped.load(Ordering::Acquire)
        {
            return;
        }
        let _ = self.jobs.try_send(Job {
            kind: JobKind::ReemitProbe,
            reply: None,
        });
    }

    /// Install the host's nonblocking publisher bridge after the service starts.
    /// A renderer can recover an early startup request with `probe_pending`.
    pub fn set_probe_emitter(&self, emitter: ProbeEmitter) {
        let mut guard = lock_recover(&self.probe_emitter);
        *guard = Some(emitter);
    }

    pub fn clear_probe_emitter(&self) {
        let mut guard = lock_recover(&self.probe_emitter);
        *guard = None;
    }

    /// Stop accepting work and wake the service thread for an orderly exit.
    pub fn shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
        let job = Job {
            kind: JobKind::Shutdown,
            reply: None,
        };
        let _ = self.jobs.try_send(job);
    }

    fn request(&self, kind: JobKind) -> Result<Value, String> {
        if self.shutdown_requested.load(Ordering::Acquire) {
            return Err("CANCELLED".to_owned());
        }
        if self.service_stopped.load(Ordering::Acquire) {
            return Err("CAPABILITY_UNAVAILABLE".to_owned());
        }
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let job = Job {
            kind,
            reply: Some(reply_tx),
        };
        match self.jobs.try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err("CAPABILITY_UNAVAILABLE".to_owned()),
            Err(TrySendError::Disconnected(_)) => {
                return Err(if self.shutdown_requested.load(Ordering::Acquire) {
                    "CANCELLED".to_owned()
                } else {
                    "CAPABILITY_UNAVAILABLE".to_owned()
                });
            }
        }
        match reply_rx.recv_timeout(JOB_TIMEOUT) {
            Ok(_) if self.shutdown_requested.load(Ordering::Acquire) => Err("CANCELLED".to_owned()),
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if self.shutdown_requested.load(Ordering::Acquire) {
                    Err("CANCELLED".to_owned())
                } else {
                    Err("CAPABILITY_UNAVAILABLE".to_owned())
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Err("CAPABILITY_UNAVAILABLE".to_owned()),
        }
    }
}

pub fn spawn_media_service(
    config: ServiceConfig,
) -> std::io::Result<(MediaHandle, JoinHandle<()>)> {
    let (jobs_tx, jobs_rx) = mpsc::sync_channel(JOB_CAPACITY);
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let service_stopped = Arc::new(AtomicBool::new(false));
    let probe_emitter = Arc::new(Mutex::new(None));
    let handle = MediaHandle {
        jobs: jobs_tx,
        shutdown_requested: shutdown_requested.clone(),
        service_stopped: service_stopped.clone(),
        probe_emitter: probe_emitter.clone(),
    };
    let join = thread::Builder::new()
        .name("media-service".to_owned())
        .spawn(move || {
            run_service(
                config,
                jobs_rx,
                shutdown_requested,
                service_stopped,
                probe_emitter,
            );
        })?;
    Ok((handle, join))
}

enum JobKind {
    Inspect(String),
    Install(Vec<u8>),
    Activate {
        request_id: String,
        install_id: String,
        policy: ActivationPolicy,
    },
    List,
    ProbeReport {
        install_id: String,
        revision: u64,
        path_sha256: String,
        ok: bool,
        duration_ms: u64,
    },
    ProbePending,
    ReemitProbe,
    Shutdown,
}

struct Job {
    kind: JobKind,
    reply: Option<SyncSender<Result<Value, String>>>,
}

struct ServiceState {
    config: ServiceConfig,
    store: Option<Store<NoHook>>,
    table: Arc<PrewarmTable>,
    probe_state: ProbeState,
    probe_request: Option<ProbeRequest>,
    probe_emitter: Arc<Mutex<Option<ProbeEmitter>>>,
    stripped_visuals: Vec<EventId>,
}

fn run_service(
    config: ServiceConfig,
    jobs: Receiver<Job>,
    shutdown_requested: Arc<AtomicBool>,
    service_stopped: Arc<AtomicBool>,
    probe_emitter: Arc<Mutex<Option<ProbeEmitter>>>,
) {
    let mut state = ServiceState {
        config,
        store: None,
        table: PrewarmTable::empty(),
        probe_state: ProbeState::new(),
        probe_request: None,
        probe_emitter,
        stripped_visuals: Vec::new(),
    };
    state.publish_empty_visual();

    match Store::open(
        &state.config.root,
        state.config.catalog.clone(),
        Limits::default(),
    ) {
        Ok(store) => {
            state.store = Some(store);
            let initialized = catch_unwind(AssertUnwindSafe(|| state.initialize()));
            if initialized.is_err() {
                state.publish_current_facts(true);
            }
        }
        Err(_) => {
            replace_mutex_value(
                &state.config.facts,
                MediaFacts {
                    store_ok: false,
                    critical: Vec::new(),
                },
            );
        }
    }

    let interval_ms = if state.config.resume_interval_ms == 0 {
        DEFAULT_RESUME_INTERVAL_MS
    } else {
        state.config.resume_interval_ms
    };
    let interval = Duration::from_millis(interval_ms);

    loop {
        if shutdown_requested.load(Ordering::Acquire) {
            break;
        }
        match jobs.recv_timeout(interval) {
            Ok(job) => {
                if shutdown_requested.load(Ordering::Acquire)
                    || matches!(&job.kind, JobKind::Shutdown)
                {
                    shutdown_requested.store(true, Ordering::Release);
                    if let Some(reply) = job.reply {
                        let _ = reply.send(Err("CANCELLED".to_owned()));
                    }
                    break;
                }
                run_job(job, &mut state);
            }
            Err(RecvTimeoutError::Timeout) => {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    state.tick_probes();
                    state.resume_deferred();
                }));
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    while let Ok(job) = jobs.try_recv() {
        if let Some(reply) = job.reply {
            let _ = reply.send(Err("CANCELLED".to_owned()));
        }
    }
    service_stopped.store(true, Ordering::Release);
}

fn run_job(job: Job, state: &mut ServiceState) {
    let Job { kind, reply } = job;
    let Some(reply) = reply else {
        if matches!(&kind, JobKind::ReemitProbe) {
            let _ = catch_unwind(AssertUnwindSafe(|| state.execute(kind)));
        }
        return;
    };
    let result = match catch_unwind(AssertUnwindSafe(|| state.execute(kind))) {
        Ok(result) => result,
        Err(_) => Err("CAPABILITY_UNAVAILABLE".to_owned()),
    };
    let _ = reply.send(result);
}

impl ServiceState {
    fn initialize(&mut self) {
        let active = self.store.as_ref().and_then(Store::active);
        if active.is_some() {
            if self.prewarm_and_swap().is_err() {
                self.publish_current_facts(true);
            }
        } else {
            self.publish_current_facts(true);
        }
    }

    fn execute(&mut self, kind: JobKind) -> Result<Value, String> {
        if let JobKind::ProbeReport {
            install_id,
            revision,
            path_sha256,
            ok,
            duration_ms,
        } = &kind
        {
            let accepted = self.probe_state.report(
                install_id,
                *revision,
                path_sha256,
                *ok,
                *duration_ms,
                self.now_ms(),
            );
            if accepted {
                self.publish_current_facts(self.store.is_some());
                self.sync_visual_filter();
            }
            return Ok(json!({"accepted": accepted}));
        }
        if matches!(&kind, JobKind::ProbePending) {
            return Ok(self.pending_probe_json());
        }
        if matches!(&kind, JobKind::ReemitProbe) {
            self.emit_probe_request();
            return Ok(Value::Null);
        }
        if let JobKind::Inspect(source_path) = &kind {
            return inspect(source_path);
        }
        if self.store.is_none() {
            return Err("MEDIA_INVALID".to_owned());
        }
        match kind {
            JobKind::Install(request_bytes) => {
                let request = InstallRequest::from_json(&request_bytes)
                    .map_err(|_| "MEDIA_INVALID".to_owned())?;
                let store = self
                    .store
                    .as_mut()
                    .ok_or_else(|| "MEDIA_INVALID".to_owned())?;
                let result = store
                    .install(&request)
                    .map_err(|_| "MEDIA_INVALID".to_owned())?;
                Ok(result.to_json())
            }
            JobKind::Activate {
                request_id,
                install_id,
                policy,
            } => {
                let activation_state = self.activation_state();
                let result = {
                    let store = self
                        .store
                        .as_mut()
                        .ok_or_else(|| "MEDIA_INVALID".to_owned())?;
                    store
                        .activate(&request_id, &install_id, policy, &activation_state)
                        .map_err(|_| "MEDIA_INVALID".to_owned())?
                };
                if result.status == Status::Active {
                    self.prewarm_and_swap()?;
                }
                Ok(result.to_json())
            }
            JobKind::List => {
                let store = self
                    .store
                    .as_ref()
                    .ok_or_else(|| "MEDIA_INVALID".to_owned())?;
                Ok(list_json(store))
            }
            JobKind::Inspect(_)
            | JobKind::ProbeReport { .. }
            | JobKind::ProbePending
            | JobKind::ReemitProbe => Ok(Value::Null),
            JobKind::Shutdown => Err("CANCELLED".to_owned()),
        }
    }

    fn now_ms(&self) -> u64 {
        self.config
            .engine_shared
            .heartbeat_ms
            .load(Ordering::Acquire)
    }

    fn pending_probe_json(&mut self) -> Value {
        let items = self.probe_state.pending(self.now_ms());
        self.publish_current_facts(self.store.is_some());
        self.sync_visual_filter();
        let Some(request) = self.probe_request.as_ref() else {
            return Value::Array(Vec::new());
        };
        Value::Array(
            items
                .into_iter()
                .map(|item| {
                    json!({
                        "install_id": request.install_id,
                        "revision": request.revision,
                        "path_sha256": item.path_sha256,
                        "url": item.url,
                        "duration_ms": item.duration_ms,
                        "visual_tail_ms": item.visual_tail_ms,
                    })
                })
                .collect(),
        )
    }

    fn tick_probes(&mut self) {
        self.probe_state.tick(self.now_ms());
        self.publish_current_facts(self.store.is_some());
        self.sync_visual_filter();
    }

    fn activation_state(&self) -> ActivationState {
        let match_active = (self.config.match_active)();
        let leased_install_ids = if self
            .config
            .engine_shared
            .lease_active
            .load(Ordering::Acquire)
        {
            self.table
                .install_id
                .as_deref()
                .map(str::to_owned)
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        ActivationState {
            match_active,
            leased_install_ids,
        }
    }

    fn resume_deferred(&mut self) {
        let state = self.activation_state();
        let results = {
            let Some(store) = self.store.as_mut() else {
                return;
            };
            if store.pending_activations().is_empty() {
                return;
            }
            store.resume_deferred(&state)
        };
        if let Ok(results) = results {
            if results.iter().any(|result| result.status == Status::Active) {
                let _ = self.prewarm_and_swap();
            }
        }
    }

    fn prewarm_and_swap(&mut self) -> Result<(), String> {
        let Some(store) = self.store.as_ref() else {
            return Err("MEDIA_INVALID".to_owned());
        };
        let Some(active) = store.active() else {
            let table = PrewarmTable::empty();
            let visuals = Arc::new(crate::media::prewarm::VisualIndex::default());
            self.probe_state.clear();
            self.probe_request = None;
            self.stripped_visuals.clear();
            let facts = self.facts_from_table(&table, true);
            self.publish(table, visuals, facts);
            return Ok(());
        };

        let (table, visuals) = self.build_active_table();
        if table.install_id.as_deref() != Some(active.install_id.as_str()) {
            return Err("CAPABILITY_UNAVAILABLE".to_owned());
        }
        let still_active = self
            .store
            .as_ref()
            .and_then(Store::active)
            .is_some_and(|current| current.install_id == active.install_id);
        if !still_active {
            return Err("CAPABILITY_UNAVAILABLE".to_owned());
        }
        // Evaluate injected probe/test dependencies before queuing a swap, so a
        // contained service failure cannot leave the engine on a table whose
        // readiness facts were never published.
        let preflight_probe_state = ProbeState::new();
        let facts = self.facts_from_table_with_probe(&table, true, &preflight_probe_state);
        let mut message = Some(Control::SwapTable {
            table: table.clone(),
        });
        let mut sent = false;
        for attempt in 0..=20 {
            let Some(control) = message.take() else {
                break;
            };
            match self.config.engine.control(control) {
                Ok(()) => {
                    sent = true;
                    break;
                }
                Err(returned) => {
                    message = Some(returned);
                    if attempt < 20 {
                        (self.config.sleeper)(250);
                    }
                }
            }
        }
        if !sent {
            return Err("CAPABILITY_UNAVAILABLE".to_owned());
        }

        self.probe_state.clear();
        self.probe_request = probe_request(&table).map(|request| {
            self.probe_state.begin(
                &request.install_id,
                request.revision,
                request.items.clone(),
                self.now_ms(),
            )
        });
        self.stripped_visuals.clear();
        self.publish(table, Arc::new(visuals), facts);
        self.emit_probe_request();
        Ok(())
    }

    fn build_active_table(&mut self) -> (Arc<PrewarmTable>, crate::media::prewarm::VisualIndex) {
        let Some(store) = self.store.as_ref() else {
            return (
                PrewarmTable::empty(),
                crate::media::prewarm::VisualIndex::default(),
            );
        };
        let catalog = &self.config.catalog;
        let minimum_pause = self.config.pause_ms_per_key;
        let sleeper = &mut self.config.sleeper;
        let mut previous_key_started: Option<Instant> = None;
        let mut pause = || {
            if let Some(previous_started) = previous_key_started {
                if minimum_pause > 0 {
                    let nanos = previous_started.elapsed().as_nanos().saturating_mul(3);
                    let delay_ms = nanos.saturating_add(999_999) / 1_000_000;
                    let dynamic_delay = u64::try_from(delay_ms).unwrap_or(u64::MAX);
                    (sleeper)(minimum_pause.max(dynamic_delay));
                }
            }
            previous_key_started = Some(Instant::now());
        };
        build_table(store, catalog, &mut pause)
    }

    fn publish(
        &mut self,
        table: Arc<PrewarmTable>,
        visuals: Arc<crate::media::prewarm::VisualIndex>,
        facts: MediaFacts,
    ) {
        self.table = table;
        replace_mutex_value(&self.config.visual_index, visuals);
        replace_mutex_value(&self.config.facts, facts);
    }

    fn publish_current_facts(&mut self, store_ok: bool) {
        let table = self.table.clone();
        let facts = self.facts_from_table(&table, store_ok);
        replace_mutex_value(&self.config.facts, facts);
    }

    fn publish_empty_visual(&self) {
        replace_mutex_value(
            &self.config.visual_index,
            Arc::new(crate::media::prewarm::VisualIndex::default()),
        );
    }

    fn facts_from_table(&self, table: &PrewarmTable, store_ok: bool) -> MediaFacts {
        self.facts_from_table_with_probe(table, store_ok, &self.probe_state)
    }

    fn facts_from_table_with_probe(
        &self,
        table: &PrewarmTable,
        store_ok: bool,
        probe_state: &ProbeState,
    ) -> MediaFacts {
        let injected_probe = &self.config.probe;
        let mut facts = facts_from_table(
            table,
            &|id| {
                probe_token(table, id)
                    .and_then(|token| probe_state.result(&token))
                    .or_else(|| injected_probe(id))
            },
            store_ok,
        );
        // A failed visual remains a failed readiness fact even after the engine
        // receives an audio-only table copy for that event.
        for media in &mut facts.critical {
            let Some(id) = crate::media::inventory::available_critical()
                .into_iter()
                .find(|id| id.as_str() == media.event_id)
            else {
                continue;
            };
            if probe_token(table, id).and_then(|token| probe_state.result(&token)) == Some(false) {
                media.probe_ok = Some(false);
            }
        }
        facts
    }

    fn emit_probe_request(&self) {
        let Some(request) = self.probe_request.as_ref() else {
            return;
        };
        let guard = lock_recover(&self.probe_emitter);
        if let Some(emitter) = guard.as_ref() {
            emitter(request.to_json());
        }
    }

    fn sync_visual_filter(&mut self) {
        let failed = failed_visual_events(&self.table, &self.probe_state);
        if failed == self.stripped_visuals {
            return;
        }
        let table = self.table.without_visuals(&failed);
        let mut message = Some(Control::SwapTable { table });
        let mut sent = false;
        for attempt in 0..=20 {
            let Some(control) = message.take() else {
                break;
            };
            match self.config.engine.control(control) {
                Ok(()) => {
                    sent = true;
                    break;
                }
                Err(returned) => {
                    message = Some(returned);
                    if attempt < 20 {
                        (self.config.sleeper)(250);
                    }
                }
            }
        }
        if sent {
            self.stripped_visuals = failed;
        }
    }
}

fn probe_request(table: &PrewarmTable) -> Option<ProbeRequest> {
    let install_id = table.install_id.as_deref()?;
    if table.fence == 0 {
        return None;
    }
    let mut items = Vec::new();
    for id in crate::media::inventory::available_critical() {
        let crate::audio::resolve::Lookup::Ready(clip) = table.lookup(id, None) else {
            continue;
        };
        let Some(url) = clip.visual_url.as_deref() else {
            continue;
        };
        let Some(path_sha256) = url.rsplit('/').next() else {
            continue;
        };
        if path_sha256.len() != 64
            || !path_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || clip.audio_ms == 0
        {
            continue;
        }
        items.push(ProbeItem {
            path_sha256: path_sha256.to_owned(),
            url: url.to_owned(),
            duration_ms: clip.audio_ms,
            visual_tail_ms: clip.visual_tail_ms,
        });
    }
    if items.is_empty() {
        return None;
    }
    Some(ProbeRequest {
        install_id: install_id.to_owned(),
        revision: table.fence,
        items,
    })
}

fn probe_token(table: &PrewarmTable, id: EventId) -> Option<String> {
    let crate::audio::resolve::Lookup::Ready(clip) = table.lookup(id, None) else {
        return None;
    };
    let url = clip.visual_url.as_deref()?;
    let token = url.rsplit('/').next()?;
    (token.len() == 64
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| token.to_owned())
}

fn failed_visual_events(table: &PrewarmTable, probes: &ProbeState) -> Vec<EventId> {
    crate::media::inventory::available_critical()
        .into_iter()
        .filter(|id| probe_token(table, *id).and_then(|token| probes.result(&token)) == Some(false))
        .collect()
}

fn lock_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn replace_mutex_value<T>(handle: &Mutex<T>, value: T) {
    let mut guard = match handle.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = value;
}

fn inspect(source_path: &str) -> Result<Value, String> {
    inspect_with_limit(source_path, MAX_CONTAINER_BYTES)
}

/// Follows symlinks because the path is user-selected; installation re-verifies
/// the package hash before committing it.
fn inspect_with_limit(source_path: &str, limit: u64) -> Result<Value, String> {
    if source_path.chars().count() > MAX_INSPECT_PATH_CHARS {
        return Err("MEDIA_INVALID".to_owned());
    }
    let path = Path::new(source_path);
    let before = std::fs::metadata(path).map_err(|_| "MEDIA_INVALID".to_owned())?;
    if !before.is_file() || before.len() > limit {
        return Err("MEDIA_INVALID".to_owned());
    }
    let (container_sha256, bytes_hashed) =
        container::sha256_file_limited(path, limit).map_err(|_| "MEDIA_INVALID".to_owned())?;
    let after = std::fs::metadata(path).map_err(|_| "MEDIA_INVALID".to_owned())?;
    if !after.is_file()
        || after.len() > limit
        || after.len() != before.len()
        || bytes_hashed != before.len()
        || bytes_hashed != after.len()
    {
        return Err("MEDIA_INVALID".to_owned());
    }
    Ok(json!({
        "container_sha256": container_sha256,
        "size_bytes": after.len(),
    }))
}

fn list_json(store: &Store<NoHook>) -> Value {
    let active = store.active().map(|active| {
        json!({
            "install_id": active.install_id,
            "package_id": active.package_id,
            "revision": active.revision,
        })
    });
    let records: Vec<Value> = store
        .records()
        .into_iter()
        .map(|record| {
            json!({
                "install_id": record.install_id,
                "package_id": record.package_id,
                "revision": record.revision,
                "container_sha256": record.container_sha256,
                "authority_class": record.authority_class.as_str(),
                "publisher_claim": record.publisher_claim.as_str(),
            })
        })
        .collect();
    json!({
        "active": active,
        "records": records,
        "pending": store.pending_activations(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc::{self, TryRecvError};
    use std::sync::{Arc, Barrier};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use kohbor_pack::store::ActivationPolicy;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};

    use super::{
        new_facts_handle, spawn_media_service, FactsHandle, MediaHandle, ServiceConfig,
        DEFAULT_RESUME_INTERVAL_MS,
    };
    use crate::audio::lanes::{lanes, Control, EngineRx, EngineShared, CONTROL_LANE};
    use crate::audio::resolve::PrewarmTable;
    use crate::media::inventory::{media_status, MediaFacts};
    use crate::media::prewarm::test_support::{
        activate, catalog, install, install_request_json, package, webm, BindingSpec, Package,
        TempDir,
    };
    use crate::media::prewarm::{new_visual_index_handle, VisualIndexHandle};

    struct RunningService {
        handle: MediaHandle,
        join: JoinHandle<()>,
        shared: Arc<EngineShared>,
        facts: FactsHandle,
        visuals: VisualIndexHandle,
        engine_rx: EngineRx,
    }

    fn setup(
        root: &std::path::Path,
        match_active: Arc<dyn Fn() -> bool + Send + Sync>,
        probe: Arc<dyn Fn(kohbor_events::EventId) -> Option<bool> + Send + Sync>,
        sleeper: Box<dyn FnMut(u64) + Send>,
        resume_interval_ms: u64,
        pause_ms_per_key: u64,
    ) -> RunningService {
        let shared = Arc::new(EngineShared::new());
        let (engine, engine_rx) = lanes(thread::current(), shared.clone());
        let facts = new_facts_handle();
        let visuals = new_visual_index_handle();
        let config = ServiceConfig {
            root: root.to_path_buf(),
            catalog: catalog(),
            engine,
            engine_shared: shared.clone(),
            visual_index: visuals.clone(),
            facts: facts.clone(),
            probe,
            match_active,
            sleeper,
            resume_interval_ms,
            pause_ms_per_key,
        };
        let (handle, join) = spawn_media_service(config).expect("spawn named media service thread");
        RunningService {
            handle,
            join,
            shared,
            facts,
            visuals,
            engine_rx,
        }
    }

    fn idle_match() -> Arc<dyn Fn() -> bool + Send + Sync> {
        Arc::new(|| false)
    }

    fn passing_probe() -> Arc<dyn Fn(kohbor_events::EventId) -> Option<bool> + Send + Sync> {
        Arc::new(|_| Some(true))
    }

    fn default_service(root: &std::path::Path) -> RunningService {
        setup(
            root,
            idle_match(),
            passing_probe(),
            Box::new(|_| {}),
            DEFAULT_RESUME_INTERVAL_MS,
            0,
        )
    }

    fn install_via_service(service: &MediaHandle, package: &Package, request_id: &str) -> String {
        let request = install_request_json(package, request_id, "never");
        let result = service
            .install(&request)
            .expect("service installs generated package");
        assert_eq!(
            result["status"], "installed",
            "install command never activates the revision"
        );
        result["record"]["install_id"]
            .as_str()
            .expect("install result carries install id")
            .to_owned()
    }

    fn receive_swap(receiver: &EngineRx) -> Arc<PrewarmTable> {
        match receiver
            .control
            .recv_timeout(Duration::from_secs(2))
            .expect("service sends a control message")
        {
            Control::SwapTable { table } => table,
            _ => panic!("service sends a table swap for active media"),
        }
    }

    fn snapshot_facts(handle: &FactsHandle) -> MediaFacts {
        handle.lock().expect("facts lock is available").clone()
    }

    fn finish(service: RunningService) {
        service.handle.shutdown();
        assert_eq!(
            service.handle.list(),
            Err("CANCELLED".to_owned()),
            "calls after shutdown are cancelled"
        );
        let (joined_tx, joined_rx) = mpsc::sync_channel(1);
        let waiter = thread::spawn(move || {
            let joined = service.join.join().is_ok();
            let _ = joined_tx.send(joined);
        });
        assert_eq!(
            joined_rx.recv_timeout(Duration::from_secs(2)),
            Ok(true),
            "media service joins before its two-second deadline"
        );
        assert!(
            waiter.join().is_ok(),
            "bounded join waiter exits after service thread"
        );
    }

    fn wav_fixture(temp: &TempDir, package_id: &str, event_id: &'static str) -> Package {
        package(
            temp,
            package_id,
            &[BindingSpec {
                event_id,
                tier: None,
                samples: 480,
                visual: false,
                visual_tail_ms: 0,
            }],
        )
    }

    #[test]
    fn facts_handle_starts_unavailable_and_empty() {
        let facts = new_facts_handle();
        let snapshot = snapshot_facts(&facts);
        assert!(!snapshot.store_ok, "facts start with store unavailable");
        assert!(
            snapshot.critical.is_empty(),
            "facts start without fabricated critical rows"
        );
    }

    #[test]
    fn production_resume_cadence_constant_is_two_seconds() {
        assert_eq!(
            DEFAULT_RESUME_INTERVAL_MS, 2_000,
            "production deferred-resume cadence is two seconds"
        );
    }

    #[test]
    fn available_store_publishes_store_ok_with_empty_initial_inventory() {
        let temp = TempDir::new("initial-facts");
        let service = default_service(&temp.path().join("store"));
        let listing = service
            .handle
            .list()
            .expect("list waits behind successful store open");
        assert!(
            listing["active"].is_null(),
            "new store has no active revision"
        );
        let facts = snapshot_facts(&service.facts);
        assert!(facts.store_ok, "successful Store::open publishes store_ok");
        assert_eq!(
            facts.critical.first().map(|fact| fact.mapped),
            Some(false),
            "no active table publishes an unmapped critical fact"
        );
        finish(service);
    }

    #[test]
    fn failing_store_returns_media_invalid_for_every_command() {
        let temp = TempDir::new("unavailable-store");
        let file = temp.path().join("not-a-directory");
        fs::write(&file, b"store root is a file").expect("create invalid store-root file");
        let service = default_service(&file);
        assert_eq!(
            service.handle.inspect("missing.media"),
            Err("MEDIA_INVALID".to_owned()),
            "unavailable store rejects inspect command"
        );
        assert_eq!(
            service.handle.install(b"{}"),
            Err("MEDIA_INVALID".to_owned()),
            "unavailable store rejects install command"
        );
        assert_eq!(
            service.handle.activate(
                "req-activate-001",
                "ins_0000000001",
                ActivationPolicy::NowIfIdle
            ),
            Err("MEDIA_INVALID".to_owned()),
            "unavailable store rejects activation command"
        );
        assert_eq!(
            service.handle.list(),
            Err("MEDIA_INVALID".to_owned()),
            "unavailable store rejects list command"
        );
        let facts = snapshot_facts(&service.facts);
        assert!(!facts.store_ok, "failed store open keeps store_ok false");
        assert!(
            facts.critical.is_empty(),
            "failed store open publishes no critical facts"
        );
        service.shared.heartbeat_ms.store(10, Ordering::Release);
        service
            .shared
            .device_available
            .store(true, Ordering::Release);
        assert_eq!(
            media_status(&facts, &service.shared, 10),
            ("not_ready", Some("MEDIA_INVALID")),
            "healthy engine reports the unavailable media store as not ready"
        );
        finish(service);
    }

    #[test]
    fn inspect_returns_hash_and_size_without_installing_package() {
        let temp = TempDir::new("inspect-valid");
        let pkg = wav_fixture(&temp, "inspect-valid-pack", "health_danger");
        let service = default_service(&temp.path().join("store"));
        let result = service
            .handle
            .inspect(pkg.package_path.to_str().expect("test path is Unicode"))
            .expect("bounded inspect reads package container");
        assert_eq!(
            result["container_sha256"], pkg.digest,
            "inspect reports the package container hash"
        );
        assert_eq!(
            result["size_bytes"].as_u64(),
            fs::metadata(&pkg.package_path)
                .ok()
                .map(|metadata| metadata.len()),
            "inspect reports actual package size"
        );
        let listing = service
            .handle
            .list()
            .expect("list confirms inspect has no store side effect");
        assert!(
            listing["records"]
                .as_array()
                .expect("records is array")
                .is_empty(),
            "inspect does not install a record"
        );
        finish(service);
    }

    #[test]
    fn inspect_of_missing_file_returns_media_invalid() {
        let temp = TempDir::new("inspect-missing");
        let service = default_service(&temp.path().join("store"));
        assert_eq!(
            service.handle.inspect(
                temp.path()
                    .join("missing.zip")
                    .to_str()
                    .expect("path is Unicode")
            ),
            Err("MEDIA_INVALID".to_owned()),
            "missing source cannot be inspected"
        );
        finish(service);
    }

    #[test]
    fn inspect_with_limit_accepts_files_at_and_under_limit_and_rejects_over() {
        let temp = TempDir::new("inspect-limit");
        let limit = 4;
        for (name, bytes, expected_size) in [
            ("under.bin", &b"abc"[..], Some(3)),
            ("at.bin", &b"abcd"[..], Some(4)),
            ("over.bin", &b"abcde"[..], None),
        ] {
            let path = temp.path().join(name);
            fs::write(&path, bytes).expect("write small inspect fixture");
            let result =
                super::inspect_with_limit(path.to_str().expect("test path is Unicode"), limit);

            match expected_size {
                Some(size) => assert_eq!(
                    result.expect("file at or under the limit is inspectable")["size_bytes"]
                        .as_u64(),
                    Some(size)
                ),
                None => assert_eq!(result, Err("MEDIA_INVALID".to_owned())),
            }
        }
    }

    #[test]
    fn inspect_with_limit_rejects_a_directory() {
        let temp = TempDir::new("inspect-directory");
        let directory = temp.path().join("directory");
        fs::create_dir(&directory).expect("create inspect directory fixture");

        assert_eq!(
            super::inspect_with_limit(directory.to_str().expect("test path is Unicode"), 4),
            Err("MEDIA_INVALID".to_owned())
        );
    }

    #[test]
    fn inspect_refuses_paths_longer_than_1024_characters() {
        let temp = TempDir::new("inspect-long");
        let service = default_service(&temp.path().join("store"));
        let too_long = "x".repeat(1_025);
        assert_eq!(
            service.handle.inspect(&too_long),
            Err("MEDIA_INVALID".to_owned()),
            "path character limit is enforced before file access"
        );
        finish(service);
    }

    #[test]
    fn install_stages_package_without_swapping_engine_table() {
        let temp = TempDir::new("install-only");
        let pkg = wav_fixture(&temp, "install-only-pack", "health_danger");
        let service = default_service(&temp.path().join("store"));
        let install_id = install_via_service(&service.handle, &pkg, "req-install-only-001");
        assert!(
            install_id.starts_with("ins_"),
            "install returns a generated install id"
        );
        assert!(
            matches!(
                service.engine_rx.control.try_recv(),
                Err(TryRecvError::Empty)
            ),
            "install never activates or swaps the table"
        );
        let listing = service
            .handle
            .list()
            .expect("installed package is listable");
        assert!(
            listing["active"].is_null(),
            "install leaves active selection empty"
        );
        finish(service);
    }

    #[test]
    fn install_with_hostile_digest_returns_rejected_without_table_change() {
        let temp = TempDir::new("hostile-digest");
        let pkg = wav_fixture(&temp, "hostile-digest-pack", "health_danger");
        let service = default_service(&temp.path().join("store"));
        let mut request: Value =
            serde_json::from_slice(&install_request_json(&pkg, "req-hostile-001", "never"))
                .expect("valid request fixture parses");
        request["expected_container_sha256"] = json!("0".repeat(64));
        let result = service
            .handle
            .install(&serde_json::to_vec(&request).expect("hostile request serializes"))
            .expect("store returns rejected result as JSON");
        assert_eq!(
            result["status"], "rejected",
            "digest mismatch is a rejected result"
        );
        let listing = service
            .handle
            .list()
            .expect("list follows rejected install");
        assert!(
            listing["active"].is_null(),
            "hostile package does not activate a revision"
        );
        assert!(
            listing["records"]
                .as_array()
                .expect("records are an array")
                .is_empty(),
            "hostile package is absent from installed records"
        );
        assert!(
            matches!(
                service.engine_rx.control.try_recv(),
                Err(TryRecvError::Empty)
            ),
            "rejected install leaves engine table untouched"
        );
        finish(service);
    }

    #[test]
    fn activation_sends_one_consistent_table_swap_with_install_fence() {
        let temp = TempDir::new("activate-swap");
        let pkg = wav_fixture(&temp, "activate-swap-pack", "health_danger");
        let service = default_service(&temp.path().join("store"));
        let install_id = install_via_service(&service.handle, &pkg, "req-activate-install-001");
        let result = service
            .handle
            .activate(
                "req-activate-swap-001",
                &install_id,
                ActivationPolicy::NowIfIdle,
            )
            .expect("activation prewarms and queues table");
        assert_eq!(
            result["status"], "active",
            "activation result reports active status"
        );
        let table = receive_swap(&service.engine_rx);
        assert_eq!(table.fence, 1, "swap message fence equals install number");
        assert_eq!(
            table.install_id.as_deref(),
            Some(install_id.as_str()),
            "swap message holds the activated revision id"
        );
        assert!(
            matches!(
                service.engine_rx.control.try_recv(),
                Err(TryRecvError::Empty)
            ),
            "activation sends exactly one swap control message"
        );
        finish(service);
    }

    #[test]
    fn active_startup_revision_is_preloaded_and_swapped_before_commands() {
        let temp = TempDir::new("startup-active");
        let pkg = wav_fixture(&temp, "startup-active-pack", "health_danger");
        let mut store = kohbor_pack::store::Store::open(
            &temp.path().join("store"),
            catalog(),
            kohbor_pack::Limits::default(),
        )
        .expect("open startup fixture store");
        let installed = install(&mut store, &pkg, "req-startup-install-001");
        let install_id = installed
            .record
            .expect("startup fixture returns install record")
            .install_id;
        let active = activate(&mut store, "req-startup-active-001", &install_id);
        assert_eq!(
            active.status.as_str(),
            "active",
            "startup fixture revision activates before service spawn"
        );
        drop(store);
        let service = default_service(&temp.path().join("store"));
        let table = receive_swap(&service.engine_rx);
        assert_eq!(
            table.install_id.as_deref(),
            Some(install_id.as_str()),
            "startup prewarm uses the active store revision"
        );
        assert!(
            service
                .handle
                .list()
                .expect("service is open after startup")["active"]
                .is_object(),
            "active startup revision is visible to list command"
        );
        assert!(
            snapshot_facts(&service.facts).store_ok,
            "successful startup publishes store readiness"
        );
        finish(service);
    }

    #[test]
    fn list_exposes_host_local_active_records_and_pending_shape() {
        let temp = TempDir::new("list-shape");
        let pkg = wav_fixture(&temp, "list-shape-pack", "health_danger");
        let service = default_service(&temp.path().join("store"));
        let install_id = install_via_service(&service.handle, &pkg, "req-list-install-001");
        let listing = service
            .handle
            .list()
            .expect("installed store lists successfully");
        assert!(
            listing["active"].is_null(),
            "active is null before activation"
        );
        assert_eq!(
            listing["records"].as_array().map(Vec::len),
            Some(1),
            "list includes the installed record"
        );
        assert_eq!(
            listing["records"][0]["install_id"], install_id,
            "record identifies generated install"
        );
        assert!(
            listing["records"][0]["package_id"].is_string(),
            "record includes package id"
        );
        assert!(
            listing["records"][0]["revision"].is_number(),
            "record includes descriptor revision"
        );
        assert!(
            listing["pending"]
                .as_array()
                .expect("pending is an array")
                .is_empty(),
            "pending list starts empty"
        );
        finish(service);
    }

    #[test]
    fn deferred_match_activation_resumes_after_match_barrier_clears() {
        let temp = TempDir::new("deferred-match");
        let pkg = wav_fixture(&temp, "deferred-match-pack", "health_danger");
        let match_flag = Arc::new(AtomicBool::new(true));
        let match_probe = match_flag.clone();
        let service = setup(
            &temp.path().join("store"),
            Arc::new(move || match_probe.load(Ordering::Acquire)),
            passing_probe(),
            Box::new(|_| {}),
            20,
            0,
        );
        let install_id = install_via_service(&service.handle, &pkg, "req-deferred-install-001");
        let result = service
            .handle
            .activate(
                "req-deferred-match-001",
                &install_id,
                ActivationPolicy::NowIfIdle,
            )
            .expect("store records deferred activation");
        assert_eq!(
            result["status"], "deferred",
            "active match barrier defers package activation"
        );
        assert!(
            matches!(
                service.engine_rx.control.try_recv(),
                Err(TryRecvError::Empty)
            ),
            "deferred activation does not swap the table"
        );
        match_flag.store(false, Ordering::Release);
        let table = receive_swap(&service.engine_rx);
        assert_eq!(
            table.install_id.as_deref(),
            Some(install_id.as_str()),
            "deferred resume swaps the newly active revision"
        );
        let listing = service
            .handle
            .list()
            .expect("resumed activation remains listable");
        assert_eq!(
            listing["active"]["install_id"], install_id,
            "resume updates Store active pointer"
        );
        finish(service);
    }

    #[test]
    fn lease_defers_replacement_until_the_old_revision_is_released() {
        let temp = TempDir::new("deferred-lease");
        let first = wav_fixture(&temp, "lease-first-pack", "health_danger");
        let second = package(
            &temp,
            "lease-second-pack",
            &[BindingSpec {
                event_id: "hero_kill",
                tier: None,
                samples: 480,
                visual: false,
                visual_tail_ms: 0,
            }],
        );
        let store_root = temp.path().join("store");
        let mut store =
            kohbor_pack::store::Store::open(&store_root, catalog(), kohbor_pack::Limits::default())
                .expect("open lease fixture store");
        let first_installed = install(&mut store, &first, "req-lease-first-inst-001");
        let first_id = first_installed
            .record
            .expect("first install returns record")
            .install_id;
        let _ = activate(&mut store, "req-lease-first-act-001", &first_id);
        drop(store);
        let service = setup(
            &store_root,
            idle_match(),
            passing_probe(),
            Box::new(|_| {}),
            20,
            0,
        );
        let initial = receive_swap(&service.engine_rx);
        assert_eq!(
            initial.install_id.as_deref(),
            Some(first_id.as_str()),
            "startup table begins with leased revision"
        );
        service.shared.lease_active.store(true, Ordering::Release);
        let second_id = install_via_service(&service.handle, &second, "req-lease-second-inst-001");
        let deferred = service
            .handle
            .activate(
                "req-lease-second-act-001",
                &second_id,
                ActivationPolicy::NowIfIdle,
            )
            .expect("active playback lease defers replacement");
        assert_eq!(
            deferred["status"], "deferred",
            "lease on current install blocks revision switch"
        );
        let pending = service
            .handle
            .list()
            .expect("lease-deferred activation remains listable");
        assert_eq!(
            pending["pending"],
            json!([second_id.clone()]),
            "lease-deferred install is recorded for resume"
        );
        assert!(
            matches!(
                service.engine_rx.control.try_recv(),
                Err(TryRecvError::Empty)
            ),
            "leased old revision remains the only sent table"
        );
        service.shared.lease_active.store(false, Ordering::Release);
        let resumed = receive_swap(&service.engine_rx);
        assert_eq!(
            resumed.install_id.as_deref(),
            Some(second_id.as_str()),
            "released lease allows deferred replacement"
        );
        finish(service);
    }

    #[test]
    fn activation_publishes_facts_and_visual_index_only_after_swap_is_queued() {
        let temp = TempDir::new("publish-after-swap");
        let pkg = wav_fixture(&temp, "publish-after-swap-pack", "health_danger");
        let service = default_service(&temp.path().join("store"));
        let install_id = install_via_service(&service.handle, &pkg, "req-publish-install-001");
        let _ = service
            .handle
            .activate(
                "req-publish-active-001",
                &install_id,
                ActivationPolicy::NowIfIdle,
            )
            .expect("activate and prewarm");
        let _ = receive_swap(&service.engine_rx);
        let facts = snapshot_facts(&service.facts);
        assert!(
            facts.store_ok,
            "published facts retain successful store state"
        );
        assert!(
            facts
                .critical
                .iter()
                .any(|fact| fact.event_id == "health_danger" && fact.prewarmed),
            "facts follow the newly prewarmed table"
        );
        let visuals = service
            .visuals
            .lock()
            .expect("visual handle lock is available")
            .clone();
        assert_eq!(
            visuals.install_id.as_deref(),
            Some(install_id.as_str()),
            "visual index is swapped with the table revision"
        );
        finish(service);
    }

    #[test]
    fn healthy_engine_can_report_prepared_critical_audio_ready() {
        let temp = TempDir::new("healthy-readiness");
        let pkg = wav_fixture(&temp, "healthy-readiness-pack", "health_danger");
        let service = default_service(&temp.path().join("store"));
        let install_id = install_via_service(&service.handle, &pkg, "req-healthy-install-001");
        let _ = service
            .handle
            .activate(
                "req-healthy-active-001",
                &install_id,
                ActivationPolicy::NowIfIdle,
            )
            .expect("activation prewarms critical audio");
        let _ = receive_swap(&service.engine_rx);
        service.shared.heartbeat_ms.store(50, Ordering::Release);
        service
            .shared
            .device_available
            .store(true, Ordering::Release);
        let facts = snapshot_facts(&service.facts);
        assert_eq!(
            media_status(&facts, &service.shared, 50),
            ("ready", None),
            "prepared critical clip is ready only with a healthy engine and device"
        );
        finish(service);
    }

    #[test]
    fn full_engine_control_lane_retries_then_preserves_unswapped_facts() {
        let temp = TempDir::new("control-full");
        let pkg = wav_fixture(&temp, "control-full-pack", "health_danger");
        let shared = Arc::new(EngineShared::new());
        let (engine, engine_rx) = lanes(thread::current(), shared.clone());
        for epoch in 0..CONTROL_LANE {
            assert!(
                engine
                    .control(Control::Session {
                        epoch: epoch as u64
                    })
                    .is_ok(),
                "pre-fill engine control lane slot {epoch}"
            );
        }
        let sleeps = Arc::new(AtomicUsize::new(0));
        let sleep_counter = sleeps.clone();
        let facts = new_facts_handle();
        let visuals = new_visual_index_handle();
        let config = ServiceConfig {
            root: temp.path().join("store"),
            catalog: catalog(),
            engine,
            engine_shared: shared,
            visual_index: visuals.clone(),
            facts: facts.clone(),
            probe: passing_probe(),
            match_active: idle_match(),
            sleeper: Box::new(move |_| {
                sleep_counter.fetch_add(1, Ordering::Relaxed);
            }),
            resume_interval_ms: 20,
            pause_ms_per_key: 0,
        };
        let (handle, join) =
            spawn_media_service(config).expect("spawn service with saturated control lane");
        let install_id = install_via_service(&handle, &pkg, "req-control-install-001");
        assert_eq!(
            handle.activate(
                "req-control-active-001",
                &install_id,
                ActivationPolicy::NowIfIdle
            ),
            Err("CAPABILITY_UNAVAILABLE".to_owned()),
            "failed control send reports capability unavailable"
        );
        assert_eq!(
            sleeps.load(Ordering::Relaxed),
            20,
            "failed swap waits through exactly twenty injected retry intervals"
        );
        let listing = handle
            .list()
            .expect("store remains queryable after swap-lane failure");
        assert_eq!(
            listing["active"]["install_id"], install_id,
            "store active pointer is preserved after the engine lane stays full"
        );
        assert_eq!(
            snapshot_facts(&facts)
                .critical
                .first()
                .map(|fact| fact.mapped),
            Some(false),
            "facts still describe the empty engine-held table"
        );
        for _ in 0..CONTROL_LANE {
            assert!(
                matches!(engine_rx.control.try_recv(), Ok(Control::Session { .. })),
                "full lane contains only its original control messages"
            );
        }
        assert!(
            matches!(engine_rx.control.try_recv(), Err(TryRecvError::Empty)),
            "failed swap did not append a table control message"
        );
        finish(RunningService {
            handle,
            join,
            shared: Arc::new(EngineShared::new()),
            facts,
            visuals,
            engine_rx,
        });
    }

    #[test]
    fn full_job_channel_returns_capability_unavailable() {
        let temp = TempDir::new("job-full");
        let pkg = wav_fixture(&temp, "job-full-pack", "health_danger");
        let (sleep_started_tx, sleep_started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let entered = Arc::new(AtomicBool::new(false));
        let entered_sleep = entered.clone();
        let sleeper = Box::new(move |_| {
            if !entered_sleep.swap(true, Ordering::AcqRel) {
                let _ = sleep_started_tx.send(());
                let _ = release_rx.recv();
            }
        });
        let service = setup(
            &temp.path().join("store"),
            idle_match(),
            passing_probe(),
            sleeper,
            20,
            1,
        );
        let install_id = install_via_service(&service.handle, &pkg, "req-jobfull-install-001");
        let activation_handle = service.handle.clone();
        let activation_id = install_id.clone();
        let (activation_tx, activation_rx) = mpsc::sync_channel(1);
        let activation_thread = thread::spawn(move || {
            let result = activation_handle.activate(
                "req-jobfull-active-001",
                &activation_id,
                ActivationPolicy::NowIfIdle,
            );
            let _ = activation_tx.send(result);
        });
        assert_eq!(
            sleep_started_rx.recv_timeout(Duration::from_secs(2)),
            Ok(()),
            "activation reaches injected prewarm sleeper"
        );

        let barrier = Arc::new(Barrier::new(6));
        let mut callers = Vec::new();
        let (caller_tx, caller_rx) = mpsc::channel();
        for _ in 0..5 {
            let barrier = barrier.clone();
            let handle = service.handle.clone();
            let tx = caller_tx.clone();
            callers.push(thread::spawn(move || {
                barrier.wait();
                let result = handle.list();
                let _ = tx.send(result);
            }));
        }
        barrier.wait();
        match caller_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Err(code)) if code == "CAPABILITY_UNAVAILABLE" => {}
            other => {
                panic!("one caller observes the full bounded channel before release: {other:?}")
            }
        }
        let _ = release_tx.send(());
        for _ in 0..4 {
            match caller_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Ok(_)) => {}
                other => panic!(
                    "four queued commands finish within two seconds after release: {other:?}"
                ),
            }
        }
        for caller in callers {
            assert!(
                caller.join().is_ok(),
                "job-channel caller exits after receiving a reply"
            );
        }
        assert_eq!(
            activation_rx
                .recv_timeout(Duration::from_secs(2))
                .map(|result| result.map(|value| value["status"].clone())),
            Ok(Ok(json!("active"))),
            "unblocked activation completes"
        );
        assert!(
            activation_thread.join().is_ok(),
            "activation caller exits after result"
        );
        let _ = receive_swap(&service.engine_rx);
        finish(service);
    }

    #[test]
    fn one_visual_package_covers_prepared_url_index_probe_and_job_panic() {
        let temp = TempDir::new("visual-service");
        assert!(
            kohbor_pack::media::inspect_webm(&webm()).is_ok(),
            "hand-built fixture has one valid VP9 video track"
        );
        let visual = package(
            &temp,
            "visual-service-pack",
            &[BindingSpec {
                event_id: "health_danger",
                tier: None,
                samples: 480,
                visual: true,
                visual_tail_ms: 900,
            }],
        );
        let panic_probe = Arc::new(AtomicBool::new(false));
        let probe_state = panic_probe.clone();
        let service = setup(
            &temp.path().join("store"),
            idle_match(),
            Arc::new(move |_| {
                if probe_state.load(Ordering::Acquire) {
                    panic!("injected renderer probe panic");
                }
                Some(false)
            }),
            Box::new(|_| {}),
            20,
            0,
        );
        let install_id = install_via_service(&service.handle, &visual, "req-visual-inst-001");
        let initial_index = service
            .visuals
            .lock()
            .expect("visual index lock is available before activation")
            .clone();
        assert!(
            initial_index.install_id.is_none() && initial_index.entries.is_empty(),
            "installing the visual package does not preload renderer files"
        );

        let revision = temp
            .path()
            .join("store")
            .join("revisions")
            .join(&install_id);
        let descriptor: Value = serde_json::from_slice(
            &fs::read(revision.join("descriptor.json")).expect("installed descriptor is readable"),
        )
        .expect("installed descriptor parses");
        let relative = descriptor["bindings"][0]["silent_visual_path"]
            .as_str()
            .expect("descriptor binds its silent visual");
        let token = format!("{:x}", Sha256::digest(relative.as_bytes()));
        let visual_path = revision.join(relative);

        let activated = service
            .handle
            .activate(
                "req-visual-active-001",
                &install_id,
                ActivationPolicy::NowIfIdle,
            )
            .expect("service activates and prewarms visual package");
        assert_eq!(activated["status"], "active", "visual package is active");
        let table = receive_swap(&service.engine_rx);
        assert_eq!(table.fence, 1, "visual table carries the install fence");
        assert_eq!(
            table.install_id.as_deref(),
            Some(install_id.as_str()),
            "visual table and fence identify one active revision"
        );
        match table.lookup(kohbor_events::EventId::HealthDanger, None) {
            crate::audio::resolve::Lookup::Ready(clip) => {
                assert_eq!(clip.pcm.len(), 480, "prewarm decodes the generated audio");
                assert_eq!(clip.visual_tail_ms, 900, "binding visual tail is preserved");
                let expected_url = format!("kohbor-media://localhost/v1/{install_id}/{token}");
                assert_eq!(
                    clip.visual_url.as_deref(),
                    Some(expected_url.as_str()),
                    "clip contains logical URL with independently hashed descriptor path"
                );
            }
            crate::audio::resolve::Lookup::Invalid | crate::audio::resolve::Lookup::Unmapped => {
                panic!("valid visual fixture should have a ready clip")
            }
        }
        let index = service
            .visuals
            .lock()
            .expect("visual index lock is available after activation")
            .clone();
        assert_eq!(
            index.install_id.as_deref(),
            Some(install_id.as_str()),
            "visual index is scoped to the active install"
        );
        let visual_entry = index
            .entries
            .get(&token)
            .expect("SHA-256 token indexes the visual file");
        assert_eq!(
            visual_entry.path, visual_path,
            "visual index points at the descriptor-relative file"
        );
        assert_eq!(
            visual_entry.size_bytes,
            fs::metadata(&visual_path)
                .expect("visual file metadata is readable")
                .len(),
            "visual index captures current file size"
        );
        let facts = snapshot_facts(&service.facts);
        assert_eq!(
            facts
                .critical
                .iter()
                .find(|fact| fact.event_id == "health_danger")
                .map(|fact| fact.probe_ok),
            Some(Some(false)),
            "visual readiness preserves the injected failed probe result"
        );

        panic_probe.store(true, Ordering::Release);
        assert_eq!(
            service.handle.activate(
                "req-visual-panic-001",
                &install_id,
                ActivationPolicy::NowIfIdle
            ),
            Err("CAPABILITY_UNAVAILABLE".to_owned()),
            "panicking visual probe fails only the current activation job"
        );
        assert!(
            service.handle.list().is_ok(),
            "service remains alive and accepts a command after a panic"
        );
        assert!(
            matches!(
                service.engine_rx.control.try_recv(),
                Err(TryRecvError::Empty)
            ),
            "probe panic occurs before a second table swap"
        );

        panic_probe.store(false, Ordering::Release);
        fs::remove_file(&visual_path).expect("remove visual file to simulate post-install loss");
        let rebuilt = service
            .handle
            .activate(
                "req-visual-missing-001",
                &install_id,
                ActivationPolicy::NowIfIdle,
            )
            .expect("active revision remains available after visual file loss");
        assert_eq!(
            rebuilt["status"], "active",
            "active revision is returned while unavailable visual is rebuilt as invalid"
        );
        let invalid_table = receive_swap(&service.engine_rx);
        assert!(
            matches!(
                invalid_table.lookup(kohbor_events::EventId::HealthDanger, None),
                crate::audio::resolve::Lookup::Invalid
            ),
            "missing visual metadata makes its clip invalid"
        );
        assert!(
            service
                .visuals
                .lock()
                .expect("visual index lock remains available")
                .entries
                .is_empty(),
            "missing visual is omitted from the served allowlist"
        );
        service.shared.heartbeat_ms.store(50, Ordering::Release);
        service
            .shared
            .device_available
            .store(true, Ordering::Release);
        assert_eq!(
            media_status(&snapshot_facts(&service.facts), &service.shared, 50),
            ("not_ready", Some("MEDIA_INVALID")),
            "missing critical visual metadata prevents media readiness"
        );
        finish(service);
    }

    #[test]
    fn visual_probe_request_timeout_pending_and_report_use_engine_time() {
        let temp = TempDir::new("probe-flow");
        let visual = package(
            &temp,
            "probe-flow-pack",
            &[BindingSpec {
                event_id: "health_danger",
                tier: None,
                samples: 480,
                visual: true,
                visual_tail_ms: 300,
            }],
        );
        let service = setup(
            &temp.path().join("store"),
            idle_match(),
            Arc::new(|_| None),
            Box::new(|_| {}),
            DEFAULT_RESUME_INTERVAL_MS,
            0,
        );
        let (probe_tx, probe_rx) = mpsc::sync_channel(1);
        service.handle.set_probe_emitter(Arc::new(move |request| {
            let _ = probe_tx.try_send(request);
        }));
        let install_id = install_via_service(&service.handle, &visual, "req-probe-flow-install");
        let activated = service
            .handle
            .activate(
                "req-probe-flow-active",
                &install_id,
                ActivationPolicy::NowIfIdle,
            )
            .expect("visual revision activates before decode probe");
        assert_eq!(activated["status"], "active");
        let _ = receive_swap(&service.engine_rx);

        let request = probe_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("table swap emits one renderer probe request");
        let items = request["items"].as_array().expect("probe items are listed");
        assert_eq!(request["install_id"], install_id);
        assert_eq!(request["revision"], 1);
        assert_eq!(items.len(), 1);
        let token = items[0]["path_sha256"]
            .as_str()
            .expect("probe item includes path hash")
            .to_owned();

        service.shared.heartbeat_ms.store(5_001, Ordering::Release);
        let pending = service
            .handle
            .probe_pending()
            .expect("timed out probe can be reissued");
        let pending_items = pending.as_array().expect("pending response is an array");
        assert_eq!(pending_items.len(), 1);
        assert_eq!(pending_items[0]["install_id"], install_id);
        assert_eq!(pending_items[0]["revision"], 1);
        assert_eq!(pending_items[0]["path_sha256"], token);
        assert_eq!(pending_items[0]["duration_ms"], 10);
        assert_eq!(pending_items[0]["visual_tail_ms"], 300);

        let report = service
            .handle
            .probe_report(&install_id, 1, &token, true, 10)
            .expect("current decode report is accepted");
        assert_eq!(report, json!({"accepted": true}));
        let facts = snapshot_facts(&service.facts);
        assert_eq!(
            facts
                .critical
                .iter()
                .find(|fact| fact.event_id == "health_danger")
                .map(|fact| fact.probe_ok),
            Some(Some(true))
        );
        assert_eq!(
            service
                .handle
                .probe_pending()
                .expect("success is not reissued"),
            json!([])
        );
        finish(service);
    }

    #[test]
    fn shutdown_stops_thread_and_returns_cancelled_to_later_jobs() {
        let temp = TempDir::new("shutdown");
        let service = default_service(&temp.path().join("store"));
        let handle = service.handle.clone();
        finish(service);
        assert_eq!(
            handle.list(),
            Err("CANCELLED".to_owned()),
            "retained handle is cancelled after service shutdown"
        );
    }

    #[test]
    fn list_records_deferred_activation_until_resume() {
        let temp = TempDir::new("pending-list");
        let pkg = wav_fixture(&temp, "pending-list-pack", "health_danger");
        let active = Arc::new(AtomicBool::new(true));
        let flag = active.clone();
        let service = setup(
            &temp.path().join("store"),
            Arc::new(move || flag.load(Ordering::Acquire)),
            passing_probe(),
            Box::new(|_| {}),
            20,
            0,
        );
        let install_id = install_via_service(&service.handle, &pkg, "req-pending-install-001");
        let deferred = service
            .handle
            .activate(
                "req-pending-act-001",
                &install_id,
                ActivationPolicy::NowIfIdle,
            )
            .expect("match-active activation defers");
        assert_eq!(
            deferred["status"], "deferred",
            "activation reports deferred status"
        );
        let listing = service.handle.list().expect("pending state is listable");
        assert_eq!(
            listing["pending"],
            json!([install_id]),
            "list exposes deferred install id"
        );
        assert!(
            listing["active"].is_null(),
            "deferred revision is not active until resumed"
        );
        active.store(false, Ordering::Release);
        let _ = receive_swap(&service.engine_rx);
        finish(service);
    }

    #[test]
    fn inspect_path_limit_counts_unicode_characters() {
        let temp = TempDir::new("inspect-unicode-limit");
        let service = default_service(&temp.path().join("store"));
        let path = "é".repeat(1_025);
        assert_eq!(
            service.handle.inspect(&path),
            Err("MEDIA_INVALID".to_owned()),
            "path length is measured in characters rather than UTF-8 bytes"
        );
        finish(service);
    }

    #[test]
    fn inspect_is_available_when_store_open_fails() {
        let temp = TempDir::new("inspect-without-store");
        let source = temp.path().join("bounded-source.bin");
        let bytes = b"inspection does not require an open package store";
        fs::write(&source, bytes).expect("write bounded inspection input");
        let store_path = temp.path().join("store-is-a-file");
        fs::write(&store_path, b"not a store directory").expect("block Store::open");
        let service = setup(
            &store_path,
            idle_match(),
            passing_probe(),
            Box::new(|_| {}),
            DEFAULT_RESUME_INTERVAL_MS,
            0,
        );

        let result = service
            .handle
            .inspect(source.to_str().unwrap_or_default())
            .expect("bounded inspect does not depend on Store::open");
        assert_eq!(result["size_bytes"], bytes.len() as u64);
        assert_eq!(
            result["container_sha256"],
            format!("{:x}", Sha256::digest(bytes))
        );
        assert!(!snapshot_facts(&service.facts).store_ok);
        assert_eq!(service.handle.list(), Err("MEDIA_INVALID".to_owned()));
        finish(service);
    }

    #[test]
    fn service_visual_index_stays_empty_until_revision_is_activated() {
        let temp = TempDir::new("audio-install-only");
        let pkg = wav_fixture(&temp, "audio-install-only-pack", "health_danger");
        let service = default_service(&temp.path().join("store"));
        let _ = install_via_service(&service.handle, &pkg, "req-visual-install-only-001");
        let index = service
            .visuals
            .lock()
            .expect("visual index lock remains available")
            .clone();
        assert!(
            index.install_id.is_none(),
            "installation without activation leaves visual index unbound"
        );
        assert!(
            index.entries.is_empty(),
            "audio-only installation does not create visual rows"
        );
        finish(service);
    }

    #[test]
    fn command_reply_preserves_media_install_result_schema_version() {
        let temp = TempDir::new("install-json-version");
        let pkg = wav_fixture(&temp, "install-json-version-pack", "health_danger");
        let service = default_service(&temp.path().join("store"));
        let result = service
            .handle
            .install(&install_request_json(&pkg, "req-json-version-001", "never"))
            .expect("install result is returned as JSON");
        assert_eq!(
            result["schema_version"], "1.0.0",
            "host returns the Store install result contract version"
        );
        finish(service);
    }

    #[test]
    fn request_after_service_gone_reports_capability_unavailable() {
        let temp = TempDir::new("gone-service");
        let service = default_service(&temp.path().join("store"));
        let handle = service.handle.clone();
        finish(service);
        assert_eq!(
            handle.list(),
            Err("CANCELLED".to_owned()),
            "intentional shutdown is distinguished from an unexpected disconnect"
        );
    }
}
