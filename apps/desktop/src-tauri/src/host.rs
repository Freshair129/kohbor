//! Tauri-independent host glue for the critical media and audio paths.

use crate::audio::device::{spawn_device_supervisor, DeviceCmd, Opener, Sleeper};
use crate::audio::engine::Engine;
use crate::audio::lanes::{Control, EngineOut, EngineShared, EngineTx, OUT_LANE};
use crate::audio::publisher::{self, HistoryHandle};
use crate::clock::MonoClock;
use crate::containment::Sample;
use crate::intake::{self, lock, Runtime};
use crate::media::inventory::media_status;
use crate::media::prewarm::{new_visual_index_handle, VisualIndexHandle};
use crate::media::protocol::{self, ProtocolRequest, ProtocolResponse};
use crate::media::service::{self, FactsHandle, MediaHandle, ServiceConfig};
use kohbor_pack::catalog::Catalog;
use kohbor_pack::store::ActivationPolicy;
use serde_json::{json, Value};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const PROTOCOL_QUEUE_CAPACITY: usize = 8;
const PROTOCOL_EVENT: &str = "media://probe-request";
const EVENT_MARKER: &str = "__kohbor_publisher_event";
const EVENT_PAYLOAD: &str = "payload";
#[cfg(not(test))]
const SHUTDOWN_JOIN_GRACE: Duration = Duration::from_secs(5);
#[cfg(test)]
const SHUTDOWN_JOIN_GRACE: Duration = Duration::from_millis(100);

/// The only application-specific dependency used by the publisher glue.
pub trait Emitter: Send + Sync {
    fn emit(&self, event: &str, payload: Value);
}

/// A testable boundary for the media-service command queue.
pub trait MediaJobs: Send + Sync {
    fn inspect(&self, source_path: &str) -> Result<Value, String>;
    fn install(&self, request: &[u8]) -> Result<Value, String>;
    fn activate(
        &self,
        request_id: &str,
        install_id: &str,
        policy: ActivationPolicy,
    ) -> Result<Value, String>;
    fn list(&self) -> Result<Value, String>;
    fn probe_report(
        &self,
        install_id: &str,
        revision: u64,
        path_sha256: &str,
        ok: bool,
        duration_ms: u64,
    ) -> Result<Value, String>;
    fn probe_pending(&self) -> Result<Value, String>;
}

impl MediaJobs for MediaHandle {
    fn inspect(&self, source_path: &str) -> Result<Value, String> {
        MediaHandle::inspect(self, source_path)
    }

    fn install(&self, request: &[u8]) -> Result<Value, String> {
        MediaHandle::install(self, request)
    }

    fn activate(
        &self,
        request_id: &str,
        install_id: &str,
        policy: ActivationPolicy,
    ) -> Result<Value, String> {
        MediaHandle::activate(self, request_id, install_id, policy)
    }

    fn list(&self) -> Result<Value, String> {
        MediaHandle::list(self)
    }

    fn probe_report(
        &self,
        install_id: &str,
        revision: u64,
        path_sha256: &str,
        ok: bool,
        duration_ms: u64,
    ) -> Result<Value, String> {
        MediaHandle::probe_report(self, install_id, revision, path_sha256, ok, duration_ms)
    }

    fn probe_pending(&self) -> Result<Value, String> {
        MediaHandle::probe_pending(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadPriority {
    AboveNormal,
    Normal,
    BelowNormal,
}

enum PriorityTarget<'a> {
    Current,
    Joined(&'a JoinHandle<()>),
}

#[cfg(windows)]
fn set_thread_priority(target: PriorityTarget<'_>, priority: ThreadPriority) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
        THREAD_PRIORITY_BELOW_NORMAL, THREAD_PRIORITY_NORMAL,
    };

    let handle = match target {
        PriorityTarget::Current => unsafe { GetCurrentThread() },
        PriorityTarget::Joined(join) => HANDLE(join.as_raw_handle()),
    };
    let priority = match priority {
        ThreadPriority::AboveNormal => THREAD_PRIORITY_ABOVE_NORMAL,
        ThreadPriority::Normal => THREAD_PRIORITY_NORMAL,
        ThreadPriority::BelowNormal => THREAD_PRIORITY_BELOW_NORMAL,
    };
    let _ = unsafe { SetThreadPriority(handle, priority) };
}

#[cfg(not(windows))]
fn set_thread_priority(_target: PriorityTarget<'_>, _priority: ThreadPriority) {}

pub(crate) fn set_current_thread_priority(priority: ThreadPriority) {
    set_thread_priority(PriorityTarget::Current, priority);
}

fn set_join_priority(join: &JoinHandle<()>, priority: ThreadPriority) {
    set_thread_priority(PriorityTarget::Joined(join), priority);
}

#[derive(Clone)]
pub struct CommandApi {
    media: Arc<dyn MediaJobs>,
    engine: EngineTx,
    history: HistoryHandle,
    shutting_down: Arc<AtomicBool>,
}

impl CommandApi {
    fn ensure_running(&self) -> Result<(), String> {
        if self.shutting_down.load(Ordering::Acquire) {
            Err("CANCELLED".to_owned())
        } else {
            Ok(())
        }
    }

    pub fn media_inspect(&self, source_path: &str) -> Result<Value, String> {
        self.ensure_running()?;
        self.media.inspect(source_path)
    }

    pub fn media_install(&self, request: &Value) -> Result<Value, String> {
        self.ensure_running()?;
        let bytes = serde_json::to_vec(request).map_err(|_| "MEDIA_INVALID".to_owned())?;
        self.media.install(&bytes)
    }

    pub fn media_activate(
        &self,
        request_id: &str,
        install_id: &str,
        policy: &str,
    ) -> Result<Value, String> {
        self.ensure_running()?;
        let policy = match policy {
            "never" => ActivationPolicy::Never,
            "now_if_idle" => ActivationPolicy::NowIfIdle,
            "after_match" => ActivationPolicy::AfterMatch,
            _ => return Err("MEDIA_INVALID".to_owned()),
        };
        self.media.activate(request_id, install_id, policy)
    }

    pub fn media_list(&self) -> Result<Value, String> {
        self.ensure_running()?;
        self.media.list()
    }

    pub fn media_probe_report(
        &self,
        install_id: &str,
        revision: u64,
        path_sha256: &str,
        ok: bool,
        duration_ms: u64,
    ) -> Result<Value, String> {
        self.ensure_running()?;
        self.media
            .probe_report(install_id, revision, path_sha256, ok, duration_ms)
    }

    pub fn media_probe_pending(&self) -> Result<Value, String> {
        self.ensure_running()?;
        self.media.probe_pending()
    }

    pub fn audio_history(&self, limit: Option<u32>) -> Result<Vec<Value>, String> {
        self.ensure_running()?;
        Ok(publisher::audio_history_command(&self.history, limit))
    }

    pub fn set_muted(&self, muted: bool) -> Result<Value, String> {
        self.ensure_running()?;
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self
            .engine
            .control(Control::SetMuted { muted, ack: ack_tx })
            .is_err()
        {
            return Err("CAPABILITY_UNAVAILABLE".to_owned());
        }
        match ack_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(acknowledged) => {
                if self.shutting_down.load(Ordering::Acquire) {
                    Err("CANCELLED".to_owned())
                } else {
                    Ok(json!({"muted": acknowledged}))
                }
            }
            Err(_) if self.shutting_down.load(Ordering::Acquire) => Err("CANCELLED".to_owned()),
            Err(_) => Err("CAPABILITY_UNAVAILABLE".to_owned()),
        }
    }
}

struct PublisherEmitter(Arc<dyn Emitter>);

impl publisher::Emit for PublisherEmitter {
    fn emit(&self, event: &str, payload: &Value) {
        if let Some(object) = payload.as_object() {
            if let (Some(routed_event), Some(routed_payload)) = (
                object.get(EVENT_MARKER).and_then(Value::as_str),
                object.get(EVENT_PAYLOAD),
            ) {
                self.0.emit(routed_event, routed_payload.clone());
                return;
            }
        }
        self.0.emit(event, payload.clone());
    }
}

struct ProtocolJob {
    request: ProtocolRequest,
    complete: Box<dyn FnOnce(ProtocolResponse) + Send + 'static>,
}

struct ProtocolServer {
    sender: Option<SyncSender<ProtocolJob>>,
    join: Option<JoinHandle<()>>,
    shutting_down: Arc<AtomicBool>,
    #[cfg(test)]
    stop_hook: Option<SyncSender<()>>,
}

#[derive(Clone)]
struct ProtocolDispatch {
    sender: Option<SyncSender<ProtocolJob>>,
    shutting_down: Arc<AtomicBool>,
}

impl ProtocolDispatch {
    fn dispatch(
        &self,
        request: ProtocolRequest,
        complete: impl FnOnce(ProtocolResponse) + Send + 'static,
    ) {
        if self.shutting_down.load(Ordering::Acquire) {
            complete(unavailable_protocol_response());
            return;
        }
        let Some(sender) = self.sender.as_ref() else {
            complete(unavailable_protocol_response());
            return;
        };
        let job = ProtocolJob {
            request,
            complete: Box::new(complete),
        };
        match sender.try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(job)) | Err(TrySendError::Disconnected(job)) => {
                (job.complete)(unavailable_protocol_response());
            }
        }
    }
}

impl ProtocolServer {
    fn spawn(index: VisualIndexHandle, shutting_down: Arc<AtomicBool>) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<ProtocolJob>(PROTOCOL_QUEUE_CAPACITY);
        let worker_index = index.clone();
        let join = thread::Builder::new()
            .name("media-serve".to_owned())
            .spawn(move || {
                set_current_thread_priority(ThreadPriority::Normal);
                while let Ok(job) = receiver.recv() {
                    let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let index = lock(&worker_index).clone();
                        protocol::serve(&job.request, &index)
                    }))
                    .unwrap_or_else(|_| unavailable_protocol_response());
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (job.complete)(response)
                    }));
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            join: Some(join),
            shutting_down,
            #[cfg(test)]
            stop_hook: None,
        })
    }

    fn dispatch_handle(&self) -> ProtocolDispatch {
        ProtocolDispatch {
            sender: self.sender.clone(),
            shutting_down: self.shutting_down.clone(),
        }
    }

    fn stop(&mut self) {
        self.sender.take();
        #[cfg(test)]
        if let Some(hook) = self.stop_hook.take() {
            let _ = hook.try_send(());
        }
        if let Some(join) = self.join.take() {
            join_with_deadline(join, SHUTDOWN_JOIN_GRACE);
        }
    }
}

fn dispatch_protocol_from(
    protocol: &Mutex<Option<ProtocolServer>>,
    request: ProtocolRequest,
    complete: impl FnOnce(ProtocolResponse) + Send + 'static,
) {
    let dispatch = {
        let protocol = lock(protocol);
        protocol.as_ref().map(ProtocolServer::dispatch_handle)
    };
    if let Some(dispatch) = dispatch {
        dispatch.dispatch(request, complete);
    } else {
        complete(unavailable_protocol_response());
    }
}

fn stop_protocol(protocol: &Mutex<Option<ProtocolServer>>) {
    let server = lock(protocol).take();
    if let Some(mut server) = server {
        server.stop();
    }
}

fn unavailable_protocol_response() -> ProtocolResponse {
    ProtocolResponse {
        status: 503,
        headers: vec![("Cache-Control".to_owned(), "no-store".to_owned())],
        body: b"Service Unavailable".to_vec(),
    }
}

struct Threads {
    engine: Option<JoinHandle<()>>,
    device_sender: Option<mpsc::Sender<DeviceCmd>>,
    device: Option<JoinHandle<()>>,
    media: Option<JoinHandle<()>>,
    publisher: Option<JoinHandle<()>>,
}

pub struct Host {
    engine_shared: Arc<EngineShared>,
    facts: FactsHandle,
    clock: MonoClock,
    pub(crate) commands: CommandApi,
    media: MediaHandle,
    shutting_down: Arc<AtomicBool>,
    threads: Mutex<Threads>,
    protocol: Mutex<Option<ProtocolServer>>,
}

impl Host {
    pub fn start(
        app_data_dir: PathBuf,
        runtime: Arc<Runtime>,
        sample: Arc<Mutex<(Sample, bool)>>,
        emitter: Arc<dyn Emitter>,
    ) -> io::Result<Self> {
        let catalog = catalog()?;
        let clock = lock(&runtime.shared).clock();
        let engine_shared = Arc::new(EngineShared::new());
        let (out_tx, out_rx) = mpsc::sync_channel(OUT_LANE);
        let (engine_tx, engine) =
            Engine::spawn(clock, engine_shared.clone(), out_tx.clone(), false)?;
        set_join_priority(&engine, ThreadPriority::AboveNormal);

        let history = HistoryHandle::new();
        let (device_sender, device_receiver) = mpsc::channel();
        let publisher_join = match publisher::spawn_publisher(
            out_rx,
            Arc::new(PublisherEmitter(emitter)),
            history.clone(),
            Some(device_sender.clone()),
            None,
        ) {
            Ok(join) => join,
            Err(error) => {
                stop_engine(&engine_tx);
                join_with_deadline(engine, SHUTDOWN_JOIN_GRACE);
                return Err(error);
            }
        };
        set_join_priority(&publisher_join, ThreadPriority::Normal);

        let device_engine = engine_tx.clone();
        let opener: Opener = Box::new(open_default_sink);
        let sleeper: Sleeper = Box::new(|milliseconds| {
            thread::sleep(Duration::from_millis(milliseconds));
        });
        let device_join =
            match spawn_device_supervisor(device_receiver, device_engine, opener, sleeper) {
                Ok(join) => join,
                Err(error) => {
                    let _ = device_sender.send(DeviceCmd::Shutdown);
                    stop_engine(&engine_tx);
                    join_with_deadline(engine, SHUTDOWN_JOIN_GRACE);
                    drop(out_tx);
                    join_with_deadline(publisher_join, SHUTDOWN_JOIN_GRACE);
                    return Err(error);
                }
            };
        set_join_priority(&device_join, ThreadPriority::Normal);

        let shutting_down = Arc::new(AtomicBool::new(false));
        let visual_index = new_visual_index_handle();
        let protocol = match ProtocolServer::spawn(visual_index.clone(), shutting_down.clone()) {
            Ok(server) => server,
            Err(error) => {
                let _ = device_sender.send(DeviceCmd::Shutdown);
                join_with_deadline(device_join, SHUTDOWN_JOIN_GRACE);
                stop_engine(&engine_tx);
                join_with_deadline(engine, SHUTDOWN_JOIN_GRACE);
                drop(out_tx);
                join_with_deadline(publisher_join, SHUTDOWN_JOIN_GRACE);
                return Err(error);
            }
        };

        let facts = service::new_facts_handle();
        let media_config = ServiceConfig {
            root: app_data_dir.join("media"),
            catalog,
            engine: engine_tx.clone(),
            engine_shared: engine_shared.clone(),
            visual_index: visual_index.clone(),
            facts: facts.clone(),
            probe: Arc::new(|_| None),
            match_active: {
                let runtime = runtime.clone();
                Arc::new(move || {
                    let active_phase = matches!(
                        lock(&runtime.shared).phase,
                        Some(kohbor_gsi::admission::Phase::Draft)
                            | Some(kohbor_gsi::admission::Phase::Pregame)
                            | Some(kohbor_gsi::admission::Phase::Live)
                    );
                    active_phase && !matches!(intake::game_running(), Some(false))
                })
            },
            sleeper: {
                let throttle = sample.clone();
                Box::new(move |milliseconds| {
                    while lock(&throttle).1 {
                        thread::sleep(Duration::from_millis(100));
                    }
                    if milliseconds > 0 {
                        thread::sleep(Duration::from_millis(milliseconds));
                    }
                })
            },
            resume_interval_ms: service::DEFAULT_RESUME_INTERVAL_MS,
            pause_ms_per_key: 1,
        };
        let (media, media_join) = match service::spawn_media_service(media_config) {
            Ok(value) => value,
            Err(error) => {
                let mut protocol = protocol;
                protocol.stop();
                let _ = device_sender.send(DeviceCmd::Shutdown);
                join_with_deadline(device_join, SHUTDOWN_JOIN_GRACE);
                stop_engine(&engine_tx);
                join_with_deadline(engine, SHUTDOWN_JOIN_GRACE);
                drop(out_tx);
                join_with_deadline(publisher_join, SHUTDOWN_JOIN_GRACE);
                return Err(error);
            }
        };
        set_join_priority(&media_join, ThreadPriority::BelowNormal);

        let probe_sender = out_tx.clone();
        let probe_shared = engine_shared.clone();
        media.set_probe_emitter(Arc::new(move |payload| {
            let routed = json!({EVENT_MARKER: PROTOCOL_EVENT, EVENT_PAYLOAD: payload});
            if probe_sender.try_send(EngineOut::Playback(routed)).is_err() {
                let _ = probe_shared.out_dropped.fetch_update(
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                    |current| Some(current.saturating_add(1)),
                );
            }
        }));
        media.reemit_probe();
        drop(out_tx);

        lock(&runtime.shared).attach_engine(engine_tx.clone());
        runtime.start();

        let commands = CommandApi {
            media: Arc::new(media.clone()),
            engine: engine_tx,
            history,
            shutting_down: shutting_down.clone(),
        };
        Ok(Self {
            engine_shared,
            facts,
            clock,
            commands,
            media,
            shutting_down,
            threads: Mutex::new(Threads {
                engine: Some(engine),
                device_sender: Some(device_sender),
                device: Some(device_join),
                media: Some(media_join),
                publisher: Some(publisher_join),
            }),
            protocol: Mutex::new(Some(protocol)),
        })
    }

    pub fn media_status(&self) -> (&'static str, Option<&'static str>) {
        let facts = lock(&self.facts);
        media_status(&facts, &self.engine_shared, self.clock.now_ms())
    }

    /// Tauri's protocol callback only converts the request and enqueues this job.
    pub fn dispatch_protocol(
        &self,
        request: ProtocolRequest,
        complete: impl FnOnce(ProtocolResponse) + Send + 'static,
    ) {
        dispatch_protocol_from(&self.protocol, request, complete);
    }

    pub fn shutdown_started(&self) -> bool {
        shutdown_flag_started(&self.shutting_down)
    }

    pub fn shutdown(&self) {
        if !begin_shutdown(&self.shutting_down) {
            return;
        }
        let mut threads = lock(&self.threads);
        let mut engine = threads.engine.take();
        let mut device_sender = threads.device_sender.take();
        let mut device = threads.device.take();
        let mut media = threads.media.take();
        let mut publisher = threads.publisher.take();
        run_shutdown_steps(
            || {},
            || {
                stop_engine(&self.commands.engine);
                if let Some(worker) = engine.take() {
                    join_with_deadline(worker, SHUTDOWN_JOIN_GRACE);
                }
                if let Some(sender) = device_sender.take() {
                    let _ = sender.send(DeviceCmd::Shutdown);
                }
                if let Some(worker) = device.take() {
                    join_with_deadline(worker, SHUTDOWN_JOIN_GRACE);
                }
            },
            || {
                self.media.shutdown();
                if let Some(worker) = media.take() {
                    join_with_deadline(worker, SHUTDOWN_JOIN_GRACE);
                }
                self.media.clear_probe_emitter();
            },
            || {
                if let Some(worker) = publisher.take() {
                    join_with_deadline(worker, SHUTDOWN_JOIN_GRACE);
                }
            },
            || {
                stop_protocol(&self.protocol);
                // Runtime's gsi-intake thread intentionally remains detached, as before.
            },
        );
    }
}

fn shutdown_flag_started(shutting_down: &AtomicBool) -> bool {
    shutting_down.load(Ordering::Acquire)
}

fn begin_shutdown(shutting_down: &AtomicBool) -> bool {
    !shutting_down.swap(true, Ordering::AcqRel)
}

fn run_shutdown_steps(
    shutting_down: impl FnOnce(),
    engine_and_device: impl FnOnce(),
    media_service: impl FnOnce(),
    publisher: impl FnOnce(),
    protocol_and_detached_intake: impl FnOnce(),
) {
    shutting_down();
    engine_and_device();
    media_service();
    publisher();
    protocol_and_detached_intake();
}

/// Bounded: a full control lane is retried for at most `STOP_ENGINE_ATTEMPTS`
/// tries, and a dead engine (disconnected lane) cannot hang shutdown.
const STOP_ENGINE_ATTEMPTS: u32 = 100;

fn stop_engine(engine: &EngineTx) {
    for _ in 0..STOP_ENGINE_ATTEMPTS {
        match engine.control(Control::Shutdown) {
            Ok(()) => return,
            Err(control) => {
                if !matches!(control, Control::Shutdown) {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn join_with_deadline(join: JoinHandle<()>, grace: Duration) {
    let deadline = Instant::now() + grace;
    while !join.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if join.is_finished() {
        let _ = join.join();
    } else {
        // The blocking owner is allowed to finish independently after its bounded grace.
        drop(join);
    }
}

fn catalog() -> io::Result<Catalog> {
    let value: Value =
        serde_json::from_str(include_str!("../../../../contracts/event-catalog.json"))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Catalog::from_json(&value)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "event catalog is invalid"))
}

fn open_default_sink() -> Result<Box<dyn kohbor_audio::Sink + Send>, ()> {
    #[cfg(windows)]
    {
        kohbor_audio_native::WasapiSink::start()
            .map(|sink| Box::new(sink) as Box<dyn kohbor_audio::Sink + Send>)
            .map_err(|_| ())
    }
    #[cfg(not(windows))]
    {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::fake_sink::FakeSink;
    use crate::audio::lanes;
    use crate::audio::publisher::HistoryHandle;
    use crate::audio::resolve::{Clip, Entry, PrewarmTable};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, SyncSender};

    struct BlockingEmitter {
        entered: SyncSender<()>,
        release: Mutex<Receiver<()>>,
    }

    impl Emitter for BlockingEmitter {
        fn emit(&self, _event: &str, _payload: Value) {
            let _ = self.entered.try_send(());
            let _ = lock(&self.release).recv();
        }
    }

    struct FakeMediaJobs {
        inspect: Result<Value, String>,
        calls: AtomicUsize,
    }

    impl MediaJobs for FakeMediaJobs {
        fn inspect(&self, _source_path: &str) -> Result<Value, String> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.inspect.clone()
        }
        fn install(&self, _request: &[u8]) -> Result<Value, String> {
            Ok(Value::Null)
        }
        fn activate(
            &self,
            _request_id: &str,
            _install_id: &str,
            _policy: ActivationPolicy,
        ) -> Result<Value, String> {
            Ok(Value::Null)
        }
        fn list(&self) -> Result<Value, String> {
            Ok(Value::Null)
        }
        fn probe_report(
            &self,
            _install_id: &str,
            _revision: u64,
            _path_sha256: &str,
            _ok: bool,
            _duration_ms: u64,
        ) -> Result<Value, String> {
            Ok(json!({"accepted": false}))
        }
        fn probe_pending(&self) -> Result<Value, String> {
            Ok(json!([]))
        }
    }

    fn command_api(
        inspect: Result<Value, String>,
    ) -> (
        CommandApi,
        Arc<FakeMediaJobs>,
        EngineTx,
        mpsc::Receiver<Control>,
    ) {
        let shared = Arc::new(EngineShared::new());
        let (engine, rx) = lanes::lanes(thread::current(), shared);
        let backend = Arc::new(FakeMediaJobs {
            inspect,
            calls: AtomicUsize::new(0),
        });
        let api = CommandApi {
            media: backend.clone(),
            engine: engine.clone(),
            history: HistoryHandle::new(),
            shutting_down: Arc::new(AtomicBool::new(false)),
        };
        (api, backend, engine, rx.control)
    }

    #[test]
    fn stop_engine_is_bounded_when_the_engine_is_gone_or_the_lane_stays_full() {
        // Disconnected lane: the engine thread died and dropped its receivers.
        let shared = Arc::new(EngineShared::new());
        let (engine, receivers) = lanes::lanes(thread::current(), shared.clone());
        drop(receivers);
        let started = Instant::now();
        stop_engine(&engine);
        assert!(started.elapsed() < Duration::from_secs(3));

        // Full lane that nobody drains: still bounded.
        let (engine, receivers) = lanes::lanes(thread::current(), shared);
        while engine.control(Control::Session { epoch: 1 }).is_ok() {}
        let started = Instant::now();
        stop_engine(&engine);
        assert!(started.elapsed() < Duration::from_secs(3));
        drop(receivers);
    }

    #[test]
    fn media_job_lane_full_is_returned_as_capability_unavailable() {
        let (api, backend, _, _) = command_api(Err("CAPABILITY_UNAVAILABLE".to_owned()));
        assert_eq!(
            api.media_inspect("package.khm"),
            Err("CAPABILITY_UNAVAILABLE".to_owned())
        );
        assert_eq!(backend.calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn set_muted_times_out_without_an_engine_ack() {
        let (api, _, _, control) = command_api(Ok(Value::Null));
        let started = Instant::now();
        assert_eq!(
            api.set_muted(true),
            Err("CAPABILITY_UNAVAILABLE".to_owned())
        );
        assert!(started.elapsed() < Duration::from_millis(400));
        assert!(matches!(
            control.try_recv(),
            Ok(Control::SetMuted { muted: true, .. })
        ));
    }

    #[test]
    fn command_calls_are_cancelled_after_shutdown_starts() {
        let (mut api, backend, _, _) = command_api(Ok(json!({"container_sha256": "x"})));
        let shutting_down = Arc::new(AtomicBool::new(false));
        api.shutting_down = shutting_down.clone();
        assert!(api.ensure_running().is_ok());
        shutting_down.store(true, Ordering::Release);
        assert_eq!(api.ensure_running(), Err("CANCELLED".to_owned()));
        assert_eq!(
            api.media_inspect("package.khm"),
            Err("CANCELLED".to_owned())
        );
        assert_eq!(api.audio_history(None), Err("CANCELLED".to_owned()));
        assert_eq!(backend.calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn shutdown_started_flag_seam_flips_when_shutdown_begins() {
        // No Tauri-independent Host builder exists; exercise the exact atomic seam
        // shared by Host::shutdown and Host::shutdown_started.
        let shutting_down = AtomicBool::new(false);

        assert!(!shutdown_flag_started(&shutting_down));
        assert!(begin_shutdown(&shutting_down));
        assert!(shutdown_flag_started(&shutting_down));
    }

    #[test]
    fn protocol_worker_answers_from_its_bounded_worker() {
        let visual_index = new_visual_index_handle();
        let server = ProtocolServer::spawn(visual_index, Arc::new(AtomicBool::new(false)))
            .expect("protocol worker starts");
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        server.dispatch_handle().dispatch(
            ProtocolRequest {
                method: "GET".to_owned(),
                path: "/v1/ins_0000000001/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                query: None,
                range: None,
            },
            move |response| {
                let _ = response_tx.send(response);
            },
        );
        assert_eq!(
            response_rx
                .recv_timeout(Duration::from_secs(1))
                .map(|response| response.status),
            Ok(404)
        );
        let mut server = server;
        server.stop();
    }

    #[test]
    fn protocol_dispatch_does_not_wait_for_slow_shutdown() {
        let (stop_started_tx, stop_started_rx) = mpsc::sync_channel(1);
        let mut server =
            ProtocolServer::spawn(new_visual_index_handle(), Arc::new(AtomicBool::new(false)))
                .expect("protocol worker starts");
        server.stop_hook = Some(stop_started_tx);
        let protocol = Arc::new(Mutex::new(Some(server)));
        let request = ProtocolRequest {
            method: "GET".to_owned(),
            path: "/v1/ins_0000000001/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_owned(),
            query: None,
            range: None,
        };
        let (worker_entered_tx, worker_entered_rx) = mpsc::sync_channel(1);
        let (release_worker_tx, release_worker_rx) = mpsc::sync_channel(1);
        dispatch_protocol_from(&protocol, request.clone(), move |_| {
            let _ = worker_entered_tx.try_send(());
            let _ = release_worker_rx.recv();
        });
        let worker_blocked = worker_entered_rx
            .recv_timeout(Duration::from_secs(1))
            .is_ok();

        let stopping_protocol = protocol.clone();
        let stop_thread = thread::spawn(move || stop_protocol(&stopping_protocol));
        let stop_started = stop_started_rx.recv_timeout(Duration::from_secs(1)).is_ok();

        let dispatch_protocol = protocol.clone();
        let (dispatch_result_tx, dispatch_result_rx) = mpsc::sync_channel(1);
        let dispatch_thread = thread::spawn(move || {
            let started = Instant::now();
            let callback_protocol = dispatch_protocol.clone();
            let (callback_result_tx, callback_result_rx) = mpsc::sync_channel(1);
            dispatch_protocol_from(&dispatch_protocol, request, move |response| {
                let _ = callback_result_tx
                    .try_send((response.status, callback_protocol.try_lock().is_ok()));
            });
            let result = callback_result_rx
                .recv_timeout(Duration::from_millis(300))
                .ok();
            let _ = dispatch_result_tx.try_send((result, started.elapsed()));
        });
        let dispatch_result = dispatch_result_rx
            .recv_timeout(Duration::from_millis(300))
            .ok();

        let _ = release_worker_tx.try_send(());
        let dispatch_joined = dispatch_thread.join().is_ok();
        let stop_joined = stop_thread.join().is_ok();

        assert!(
            worker_blocked,
            "protocol worker is held in its completion callback"
        );
        assert!(stop_started, "shutdown reached the slow stop/join path");
        assert_eq!(
            dispatch_result.map(|(result, _)| result),
            Some(Some((503, true))),
            "a request during shutdown gets 503 after the protocol mutex is released"
        );
        assert!(
            dispatch_result.is_some_and(|(_, elapsed)| elapsed < Duration::from_millis(300)),
            "dispatch returns within the bounded shutdown window"
        );
        assert!(dispatch_joined, "protocol dispatch thread exits");
        assert!(stop_joined, "protocol shutdown thread exits after release");
    }

    #[test]
    fn protocol_stop_returns_after_grace_when_completion_callback_never_returns() {
        let server =
            ProtocolServer::spawn(new_visual_index_handle(), Arc::new(AtomicBool::new(false)))
                .expect("protocol worker starts");
        let request = ProtocolRequest {
            method: "GET".to_owned(),
            path: "/v1/ins_0000000001/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_owned(),
            query: None,
            range: None,
        };
        let (worker_entered_tx, worker_entered_rx) = mpsc::sync_channel(1);
        server.dispatch_handle().dispatch(request, move |_| {
            let _ = worker_entered_tx.try_send(());
            loop {
                thread::park();
            }
        });
        assert!(worker_entered_rx
            .recv_timeout(Duration::from_secs(1))
            .is_ok());

        let (stopped_tx, stopped_rx) = mpsc::sync_channel(1);
        let started = Instant::now();
        let stop_thread = thread::spawn(move || {
            let mut server = server;
            server.stop();
            let _ = stopped_tx.send(started.elapsed());
        });
        let elapsed = stopped_rx
            .recv_timeout(Duration::from_millis(500))
            .expect("protocol stop returns within its bounded grace");

        assert!(elapsed < Duration::from_millis(500));
        assert!(stop_thread.join().is_ok());
    }

    #[test]
    fn full_media_serve_lane_returns_503_without_waiting_for_the_worker() {
        let server =
            ProtocolServer::spawn(new_visual_index_handle(), Arc::new(AtomicBool::new(false)))
                .expect("protocol worker starts");
        let request = ProtocolRequest {
            method: "GET".to_owned(),
            path: "/v1/ins_0000000001/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            query: None,
            range: None,
        };
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        server
            .dispatch_handle()
            .dispatch(request.clone(), move |_| {
                let _ = entered_tx.try_send(());
                let _ = release_rx.recv();
            });
        let worker_blocked = entered_rx.recv_timeout(Duration::from_secs(1)).is_ok();
        for _ in 0..PROTOCOL_QUEUE_CAPACITY {
            server.dispatch_handle().dispatch(request.clone(), |_| {});
        }
        let (status_tx, status_rx) = mpsc::sync_channel(1);
        server.dispatch_handle().dispatch(request, move |response| {
            let _ = status_tx.try_send(response.status);
        });
        let overflow_status = status_rx.recv_timeout(Duration::from_millis(100)).ok();

        let _ = release_tx.try_send(());
        let mut server = server;
        server.stop();

        assert!(
            worker_blocked,
            "test callback holds the only media-serve worker"
        );
        assert_eq!(overflow_status, Some(503));
    }

    #[test]
    fn shutdown_runs_the_five_host_steps_in_order() {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let action = |name| {
            let observed = observed.clone();
            move || lock(&observed).push(name)
        };
        run_shutdown_steps(
            action("cancel commands"),
            action("engine and device"),
            action("media service"),
            action("publisher"),
            action("protocol and detached intake"),
        );
        assert_eq!(
            *lock(&observed),
            [
                "cancel commands",
                "engine and device",
                "media service",
                "publisher",
                "protocol and detached intake",
            ]
        );
    }

    #[test]
    fn blocked_emitter_does_not_stop_tick_admission_and_counts_dropped_output() {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let emitter: Arc<dyn Emitter> = Arc::new(BlockingEmitter {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        });
        let (out_tx, out_rx) = mpsc::sync_channel(1);
        let first_queued = out_tx
            .try_send(EngineOut::Playback(json!({"kind": "started"})))
            .is_ok();
        let history = HistoryHandle::new();
        let publisher = publisher::spawn_publisher(
            out_rx,
            Arc::new(PublisherEmitter(emitter)),
            history,
            None,
            None,
        )
        .expect("publisher thread starts");
        let emitter_blocked = entered_rx.recv_timeout(Duration::from_secs(2)).is_ok();
        let queued_behind_blocked_emitter =
            out_tx.try_send(EngineOut::Outcome(Value::Null)).is_ok();

        let engine_shared = Arc::new(EngineShared::new());
        let (engine_tx, engine_rx) = lanes::lanes(thread::current(), engine_shared.clone());
        let mut engine = Engine::new(
            MonoClock::from_origin(Instant::now()),
            engine_shared.clone(),
            engine_rx,
            None,
            out_tx.clone(),
            false,
        );
        let _ = engine.step(0);
        let admission = engine_tx.offer_tick(crate::audio::lanes::TickMsg {
            arrival: Instant::now(),
            detected: Instant::now(),
            at_ms: 1,
            session_epoch: 1,
            generation: 1,
            sequence: 1,
            events: Vec::new(),
        });
        let dropped = engine_shared.out_dropped.load(Ordering::Relaxed);

        let _ = release_tx.try_send(());
        drop(engine);
        drop(out_tx);
        let joined = publisher.join().is_ok();

        assert!(first_queued);
        assert!(emitter_blocked, "publisher reached the blocking emitter");
        assert!(
            queued_behind_blocked_emitter,
            "bounded publisher lane filled"
        );
        assert_eq!(admission, crate::audio::lanes::Offer::Sent);
        assert!(dropped > 0, "engine counts a full publisher lane");
        assert!(joined, "publisher exits after its lane closes");
    }

    fn wait_for(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            thread::sleep(Duration::from_millis(2));
        }
        condition()
    }

    fn receive_started(receiver: &mpsc::Receiver<EngineOut>) -> Option<String> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(EngineOut::Playback(payload)) if payload["kind"] == "started" => {
                    return payload["playback_id"].as_str().map(str::to_owned);
                }
                Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
        None
    }

    fn engine_tick(engine: &EngineTx, shared: &EngineShared, sequence: u64) -> bool {
        let now = Instant::now();
        engine.offer_tick(crate::audio::lanes::TickMsg {
            arrival: now,
            detected: now,
            at_ms: shared.heartbeat_ms.load(Ordering::Acquire),
            session_epoch: 1,
            generation: 1,
            sequence,
            events: vec![kohbor_events::Event {
                id: kohbor_events::EventId::HealthDanger,
                tier: None,
                at_ms: shared.heartbeat_ms.load(Ordering::Acquire),
            }],
        }) == crate::audio::lanes::Offer::Sent
    }

    #[test]
    fn engine_thread_rebuilds_fake_sink_and_degrades_after_four_panics() {
        let origin = Instant::now();
        let clock = MonoClock::from_origin(origin);
        let shared = Arc::new(EngineShared::new());
        let (out_tx, out_rx) = mpsc::sync_channel(64);
        let (engine_tx, engine_join) =
            Engine::spawn(clock, shared.clone(), out_tx, false).expect("engine thread starts");
        let table = Arc::new(PrewarmTable::new(
            1,
            Some(Arc::from("ins_0000000001")),
            vec![(
                kohbor_events::EventId::HealthDanger,
                None,
                Entry::Ready(Clip {
                    pcm: Arc::from(vec![0_i16; 480]),
                    bound_tier: None,
                    audio_ms: 10,
                    visual_tail_ms: 0,
                    visual_url: None,
                }),
            )],
        ));
        let _ = engine_tx.control(Control::Session { epoch: 1 });
        let _ = engine_tx.control(Control::SwapTable { table });
        let (initial_sink, initial_log) = FakeSink::new();
        let initial_installed = engine_tx
            .control(Control::SinkReady(Box::new(initial_sink)))
            .is_ok();
        let first_ready = wait_for(Duration::from_secs(1), || {
            shared.device_available.load(Ordering::Acquire)
                && shared.fence.load(Ordering::Acquire) == 1
        });
        let first_accepted = first_ready && engine_tick(&engine_tx, &shared, 1);
        let first_id = if first_accepted {
            receive_started(&out_rx)
        } else {
            None
        };
        let first_ended = wait_for(Duration::from_secs(1), || {
            !shared.lease_active.load(Ordering::Acquire)
        });

        if let Ok(mut script) = initial_log.lock() {
            script.panic_next = true;
        }
        let second_accepted = first_ended && engine_tick(&engine_tx, &shared, 2);
        let first_rebuild = wait_for(Duration::from_secs(1), || {
            shared.rebuilds.load(Ordering::Acquire) >= 1
        });

        let mut continuous_id = false;
        let mut fourth_rebuild = false;
        if first_rebuild {
            let (replacement, _) = FakeSink::new();
            let _ = engine_tx.control(Control::SinkReady(Box::new(replacement)));
            let replacement_ready = wait_for(Duration::from_secs(1), || {
                shared.device_available.load(Ordering::Acquire)
            });
            if replacement_ready {
                let third_accepted = engine_tick(&engine_tx, &shared, 3);
                let second_id = receive_started(&out_rx);
                continuous_id = third_accepted
                    && match (
                        first_id
                            .as_deref()
                            .and_then(|value| value.strip_prefix("pb_"))
                            .and_then(|value| value.parse::<u64>().ok()),
                        second_id
                            .as_deref()
                            .and_then(|value| value.strip_prefix("pb_"))
                            .and_then(|value| value.parse::<u64>().ok()),
                    ) {
                        (Some(first), Some(second)) => second > first,
                        _ => false,
                    };
                let _ = wait_for(Duration::from_secs(1), || {
                    !shared.lease_active.load(Ordering::Acquire)
                });
                for expected_rebuilds in 2..=4 {
                    let (failing_sink, log) = FakeSink::new();
                    if let Ok(mut script) = log.lock() {
                        script.panic_next_poll = true;
                    }
                    let accepted = engine_tx
                        .control(Control::SinkReady(Box::new(failing_sink)))
                        .is_ok();
                    if !accepted {
                        break;
                    }
                    if !wait_for(Duration::from_secs(1), || {
                        shared.rebuilds.load(Ordering::Acquire) >= expected_rebuilds
                    }) {
                        break;
                    }
                }
                fourth_rebuild = shared.rebuilds.load(Ordering::Acquire) >= 4;
            }
        }
        let degraded = shared.degraded.load(Ordering::Acquire);
        let shutdown_sent = engine_tx.control(Control::Shutdown).is_ok();
        let engine_joined = engine_join.join().is_ok();
        drop(out_rx);

        assert!(
            initial_installed,
            "fake startup sink is queued on the engine lane"
        );
        assert!(first_accepted, "critical intake remains admitted");
        assert!(first_ready, "thread installs table and fake sink");
        assert!(first_id.is_some(), "first fake-sink playback starts");
        assert!(
            second_accepted,
            "second critical request reaches the engine"
        );
        assert!(
            first_rebuild,
            "fake sink panic rebuilds the threaded engine"
        );
        assert!(
            continuous_id,
            "playback id remains continuous after rebuild"
        );
        assert!(
            fourth_rebuild,
            "four fake-sink panics are observed within 60 seconds"
        );
        assert!(degraded, "more than three rebuilds publish degraded mode");
        assert!(
            shutdown_sent && engine_joined,
            "engine shutdown is acknowledged and joined"
        );
    }

    #[test]
    fn admission_source_guard_rejects_blocking_and_io_primitives() {
        const FORBIDDEN: [&str; 14] = [
            "std::fs",
            "std::net",
            "File::",
            "Mutex",
            "RwLock",
            ".lock(",
            ".recv()",
            ".send(",
            "thread::sleep",
            "unwrap(",
            "expect(",
            "panic!(",
            "kohbor_pack",
            "tauri",
        ];
        let sources = [
            ("clock.rs", include_str!("clock.rs")),
            ("audio/lanes.rs", include_str!("audio/lanes.rs")),
            ("audio/resolve.rs", include_str!("audio/resolve.rs")),
            ("audio/outcome.rs", include_str!("audio/outcome.rs")),
            ("audio/engine.rs", include_str!("audio/engine.rs")),
            ("audio/slot_sink.rs", include_str!("audio/slot_sink.rs")),
        ];
        for (path, source) in sources {
            let admission = source.split("#[cfg(test)]").next().unwrap_or(source);
            for forbidden in FORBIDDEN {
                assert!(
                    !admission.contains(forbidden),
                    "{path} admission source contains forbidden token {forbidden:?}"
                );
            }
        }
    }
}
