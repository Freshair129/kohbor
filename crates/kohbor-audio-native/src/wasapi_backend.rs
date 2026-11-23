#[cfg(test)]
mod smoke_tests {
    use super::{StartError, WasapiSink};
    use kohbor_audio::{PlaybackId, Sink, SinkEvent};
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    fn silent_buffer() -> Arc<[i16]> {
        let samples = vec![0_i16; 4_800];
        assert_eq!(samples.len(), 4_800);
        assert!(samples.iter().all(|sample| *sample == 0));
        Arc::from(samples)
    }

    fn start_or_report_no_endpoint() -> Option<WasapiSink> {
        match WasapiSink::start() {
            Ok(sink) => Some(sink),
            Err(StartError::NoRenderEndpoint) => {
                println!("no active endpoint");
                None
            }
            Err(error) => {
                assert_eq!(error, StartError::NoRenderEndpoint, "WASAPI start failed");
                None
            }
        }
    }

    fn wait_for_finish(sink: &mut WasapiSink, id: PlaybackId, timeout: Duration) {
        let started = Instant::now();
        loop {
            let event = sink.poll_event();
            if event == Some(SinkEvent::Finished { id }) {
                return;
            }
            assert_ne!(
                event,
                Some(SinkEvent::DeviceLost),
                "render endpoint was lost"
            );
            assert!(
                started.elapsed() < timeout,
                "timed out waiting for Finished"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "opens the default render endpoint and submits digital silence only"]
    fn default_endpoint_renders_one_hundred_ms_of_silence_to_finished() {
        let Some(mut sink) = start_or_report_no_endpoint() else {
            return;
        };
        let pcm = silent_buffer();
        assert!(pcm.iter().all(|sample| *sample == 0));
        assert_eq!(sink.submit(PlaybackId(1), pcm), Ok(()));
        wait_for_finish(&mut sink, PlaybackId(1), Duration::from_secs(2));
    }

    #[test]
    #[ignore = "opens and closes the default render endpoint twice; silence only"]
    fn start_and_shutdown_twice() {
        for _ in 0..2 {
            let Some(sink) = start_or_report_no_endpoint() else {
                return;
            };
            drop(sink);
        }
    }

    #[test]
    #[ignore = "opens the default render endpoint; queued digital silence is cancelled before first service"]
    fn cancel_before_first_service_keeps_the_stream_silent() {
        let mut sink = match WasapiSink::start_with_first_service_gate() {
            Ok(sink) => sink,
            Err(StartError::NoRenderEndpoint) => {
                println!("no active endpoint");
                return;
            }
            Err(error) => {
                assert_eq!(error, StartError::NoRenderEndpoint, "WASAPI start failed");
                return;
            }
        };
        let pcm = silent_buffer();
        assert!(pcm.iter().all(|sample| *sample == 0));
        assert_eq!(sink.submit(PlaybackId(3), pcm), Ok(()));
        sink.cancel(PlaybackId(3));
        sink.release_first_service_gate();

        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(250) {
            let event = sink.poll_event();
            assert_ne!(event, Some(SinkEvent::Finished { id: PlaybackId(3) }));
            assert_ne!(event, Some(SinkEvent::DeviceLost));
            thread::sleep(Duration::from_millis(10));
        }
    }
}
#[cfg(feature = "latency-probe")]
use crate::engine::FirstFillProbe;
use crate::engine::{
    DeviceEventSender, Engine, EngineHandle, PlaybackMetricsSnapshot, RenderObservation,
    SharedDiagnostics,
};
use kohbor_audio::pcm::Pcm;
use kohbor_audio::{PlaybackId, Sink, SinkError, SinkEvent};
use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use wasapi::{
    AudioClient, AudioRenderClient, BufferFlags, DeviceEnumerator, DeviceEventCallbacks,
    DeviceEventRegistration, Direction, SampleType, StreamMode, WaveFormat,
};

const SHUTDOWN_POLL_MS: u32 = 100;
const REQUESTED_SAMPLE_RATE_HZ: u32 = 48_000;
const REQUESTED_CHANNELS: u16 = 1;
const REQUESTED_BITS_PER_SAMPLE: u16 = 16;
const THREAD_PRIORITY_ERROR_RETURN: i32 = 0x7fff_ffff;
const THREAD_PRIORITY_TIME_CRITICAL: i32 = 15;
const SHARED_BUFFER_MULTIPLIER: i64 = 4;
const PRO_AUDIO_TASK: [u16; 10] = [0x50, 0x72, 0x6f, 0x20, 0x41, 0x75, 0x64, 0x69, 0x6f, 0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceMixFormat {
    sample_rate_hz: u32,
    channels: u16,
    bits_per_sample: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceOpenInfo {
    mix_format: Option<DeviceMixFormat>,
    default_period_hns: Option<i64>,
    min_period_hns: Option<i64>,
    buffer_frames: u64,
    buffer_requested_hns: Option<i64>,
    buffer_fallback: bool,
    mmcss_task_index: Option<u32>,
    mmcss_ok: bool,
    mmcss_error_code: Option<u32>,
    priority_fallback_error_code: Option<u32>,
    render_thread_priority: Option<i32>,
}

static CURRENT_DIAGNOSTICS: OnceLock<Mutex<Option<Arc<SharedDiagnostics>>>> = OnceLock::new();
static CURRENT_DEVICE_INFO: OnceLock<Mutex<Option<DeviceOpenInfo>>> = OnceLock::new();

fn diagnostics_registry() -> &'static Mutex<Option<Arc<SharedDiagnostics>>> {
    CURRENT_DIAGNOSTICS.get_or_init(|| Mutex::new(None))
}

fn device_info_registry() -> &'static Mutex<Option<DeviceOpenInfo>> {
    CURRENT_DEVICE_INFO.get_or_init(|| Mutex::new(None))
}

fn publish_current_diagnostics(diagnostics: Arc<SharedDiagnostics>) {
    if let Ok(mut current) = diagnostics_registry().lock() {
        *current = Some(diagnostics);
    }
}

fn publish_current_device_info(info: DeviceOpenInfo) {
    if let Ok(mut current) = device_info_registry().lock() {
        *current = Some(info);
    }
}

fn current_device_info() -> Option<DeviceOpenInfo> {
    device_info_registry()
        .lock()
        .ok()
        .and_then(|current| *current)
}

fn current_playback_snapshot(playback_id: u64) -> Option<PlaybackMetricsSnapshot> {
    let diagnostics = diagnostics_registry()
        .lock()
        .ok()
        .and_then(|current| current.as_ref().map(Arc::clone))?;
    diagnostics.snapshot(playback_id)
}

fn json_u64(value: Option<u64>) -> String {
    value.map_or_else(|| String::from("null"), |value| value.to_string())
}

fn json_u32(value: Option<u32>) -> String {
    value.map_or_else(|| String::from("null"), |value| value.to_string())
}

fn json_i64(value: Option<i64>) -> String {
    value.map_or_else(|| String::from("null"), |value| value.to_string())
}

fn json_i32(value: Option<i32>) -> String {
    value.map_or_else(|| String::from("null"), |value| value.to_string())
}

fn json_duration_ms(value_hns: Option<i64>) -> String {
    value_hns.filter(|value| *value >= 0).map_or_else(
        || String::from("null"),
        |value| format!("{:.3}", value as f64 / 10_000.0),
    )
}

impl DeviceOpenInfo {
    fn with_thread_scheduling(mut self, scheduling: ThreadSchedulingReport) -> Self {
        self.mmcss_task_index = scheduling.mmcss_task_index;
        self.mmcss_ok = scheduling.mmcss_ok;
        self.mmcss_error_code = scheduling.mmcss_error_code;
        self.priority_fallback_error_code = scheduling.priority_fallback_error_code;
        self.render_thread_priority = scheduling.render_thread_priority;
        self
    }

    fn fields_json(self) -> String {
        let mix = self.mix_format;
        let mix_sample_rate_hz = mix.map(|format| format.sample_rate_hz);
        let mix_channels = mix.map(|format| u32::from(format.channels));
        let mix_bits_per_sample = mix.map(|format| u32::from(format.bits_per_sample));
        let buffer_ms = self.buffer_frames as f64 * 1_000.0 / f64::from(REQUESTED_SAMPLE_RATE_HZ);
        format!(
            "{{\"device_mix_sample_rate_hz\":{},\"device_mix_channels\":{},\"device_mix_bits_per_sample\":{},\"default_period_ms\":{},\"min_period_ms\":{},\"buffer_frames\":{},\"buffer_ms\":{:.3},\"buffer_requested_ms\":{},\"buffer_fallback\":{},\"mmcss_task_index\":{},\"mmcss_ok\":{},\"mmcss_error_code\":{},\"priority_fallback_error_code\":{},\"render_thread_priority\":{}}}",
            json_u32(mix_sample_rate_hz),
            json_u32(mix_channels),
            json_u32(mix_bits_per_sample),
            json_duration_ms(self.default_period_hns),
            json_duration_ms(self.min_period_hns),
            self.buffer_frames,
            buffer_ms,
            json_duration_ms(self.buffer_requested_hns),
            self.buffer_fallback,
            json_u32(self.mmcss_task_index),
            self.mmcss_ok,
            json_u32(self.mmcss_error_code),
            json_u32(self.priority_fallback_error_code),
            json_i32(self.render_thread_priority),
        )
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> *mut std::ffi::c_void;
    fn GetThreadPriority(thread: *mut std::ffi::c_void) -> i32;
    fn SetThreadPriority(thread: *mut std::ffi::c_void, priority: i32) -> i32;
    fn GetLastError() -> u32;
}

#[link(name = "avrt")]
unsafe extern "system" {
    fn AvSetMmThreadCharacteristicsW(
        task_name: *const u16,
        task_index: *mut u32,
    ) -> *mut std::ffi::c_void;
    fn AvRevertMmThreadCharacteristics(task_handle: *mut std::ffi::c_void) -> i32;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ThreadSchedulingReport {
    mmcss_task_index: Option<u32>,
    mmcss_ok: bool,
    mmcss_error_code: Option<u32>,
    priority_fallback_error_code: Option<u32>,
    render_thread_priority: Option<i32>,
}

struct ThreadSchedulingOutcome<H> {
    report: ThreadSchedulingReport,
    mmcss_handle: Option<H>,
}

trait ThreadSchedulingApi {
    type Handle;

    fn av_set_mm_thread_characteristics(&mut self) -> Result<(Self::Handle, u32), u32>;
    fn set_time_critical_priority(&mut self) -> Result<(), u32>;
    fn current_priority(&mut self) -> Option<i32>;
}

fn configure_thread_scheduling<A: ThreadSchedulingApi>(
    api: &mut A,
) -> ThreadSchedulingOutcome<A::Handle> {
    match api.av_set_mm_thread_characteristics() {
        Ok((handle, task_index)) => ThreadSchedulingOutcome {
            report: ThreadSchedulingReport {
                mmcss_task_index: Some(task_index),
                mmcss_ok: true,
                mmcss_error_code: None,
                priority_fallback_error_code: None,
                render_thread_priority: api.current_priority(),
            },
            mmcss_handle: Some(handle),
        },
        Err(mmcss_error_code) => {
            let priority_fallback_error_code = api.set_time_critical_priority().err();
            ThreadSchedulingOutcome {
                report: ThreadSchedulingReport {
                    mmcss_task_index: None,
                    mmcss_ok: false,
                    mmcss_error_code: Some(mmcss_error_code),
                    priority_fallback_error_code,
                    render_thread_priority: api.current_priority(),
                },
                mmcss_handle: None,
            }
        }
    }
}

struct Win32ThreadSchedulingApi;

impl ThreadSchedulingApi for Win32ThreadSchedulingApi {
    type Handle = *mut std::ffi::c_void;

    fn av_set_mm_thread_characteristics(&mut self) -> Result<(Self::Handle, u32), u32> {
        let mut task_index = 0;
        let handle =
            unsafe { AvSetMmThreadCharacteristicsW(PRO_AUDIO_TASK.as_ptr(), &mut task_index) };
        if handle.is_null() {
            Err(unsafe { GetLastError() })
        } else {
            Ok((handle, task_index))
        }
    }

    fn set_time_critical_priority(&mut self) -> Result<(), u32> {
        let changed =
            unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) };
        if changed == 0 {
            Err(unsafe { GetLastError() })
        } else {
            Ok(())
        }
    }

    fn current_priority(&mut self) -> Option<i32> {
        current_thread_priority()
    }
}

struct MmcssRegistration(Option<*mut std::ffi::c_void>);

impl Drop for MmcssRegistration {
    fn drop(&mut self) {
        if let Some(handle) = self.0 {
            let _ = unsafe { AvRevertMmThreadCharacteristics(handle) };
        }
    }
}

fn requested_buffer_duration_hns(default_period_hns: i64) -> Option<i64> {
    (default_period_hns > 0)
        .then(|| default_period_hns.checked_mul(SHARED_BUFFER_MULTIPLIER))
        .flatten()
}

fn initialize_with_buffer_fallback<E>(
    requested_hns: i64,
    fallback_hns: i64,
    mut initialize: impl FnMut(i64) -> Result<(), E>,
) -> Result<bool, E> {
    if initialize(requested_hns).is_ok() {
        Ok(false)
    } else {
        initialize(fallback_hns).map(|()| true)
    }
}

fn period_ns_from_hns(period_hns: Option<i64>) -> Option<u64> {
    u64::try_from(period_hns?).ok()?.checked_mul(100)
}

#[derive(Debug)]
struct WakeLatenessTracker {
    event_period: Option<Duration>,
    previous_wake: Option<Instant>,
}

impl WakeLatenessTracker {
    fn new(event_period: Option<Duration>) -> Self {
        Self {
            event_period,
            previous_wake: None,
        }
    }

    fn observe(&mut self, actual_wake: Instant) -> Option<i64> {
        let lateness_ns = self.previous_wake.and_then(|previous_wake| {
            let expected_wake = previous_wake.checked_add(self.event_period?)?;
            let (delta, positive) = if actual_wake >= expected_wake {
                (actual_wake.duration_since(expected_wake), true)
            } else {
                (expected_wake.duration_since(actual_wake), false)
            };
            let magnitude = delta.as_nanos().min(i64::MAX as u128) as i64;
            Some(if positive { magnitude } else { -magnitude })
        });
        self.previous_wake = Some(actual_wake);
        lateness_ns
    }
}

fn current_thread_priority() -> Option<i32> {
    let priority = unsafe { GetThreadPriority(GetCurrentThread()) };
    (priority != THREAD_PRIORITY_ERROR_RETURN).then_some(priority)
}

/// Why the WASAPI sink could not start. Device names, IDs, and native error
/// details are intentionally not included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartError {
    /// Windows has no default render endpoint for the current console role.
    NoRenderEndpoint,
    /// The dedicated owner thread could not initialize its MTA.
    ComInitialization,
    /// WASAPI refused the requested shared event stream or its startup buffer.
    WasapiInitialization,
    /// The operating system could not create the dedicated owner thread.
    ThreadSpawn,
    /// The owner thread exited before it reported startup completion.
    ThreadStartFailed,
}

impl fmt::Display for StartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NoRenderEndpoint => "no default render endpoint is available",
            Self::ComInitialization => "the audio owner could not initialize COM MTA",
            Self::WasapiInitialization => "WASAPI refused the shared render stream",
            Self::ThreadSpawn => "the audio owner thread could not be started",
            Self::ThreadStartFailed => "the audio owner exited before startup completed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for StartError {}

/// Shared-mode Windows output sink. One MTA thread owns the endpoint, stream,
/// render client, event handle, and device notification registration. A default
/// endpoint change reports `DeviceChanged` and leaves this stream running. Stream
/// failure reports `DeviceLost` and stops; this adapter does not reopen or retry,
/// so recovery belongs to the host.
pub struct WasapiSink {
    coordinator: EngineHandle,
    backend: WasapiBackend,
    #[cfg(feature = "latency-probe")]
    first_fill_probe: FirstFillProbe,
}

impl WasapiSink {
    /// Open the current default render endpoint with shared event-driven output.
    pub fn start() -> Result<Self, StartError> {
        #[cfg(feature = "latency-probe")]
        let first_fill_probe = FirstFillProbe::new();
        let (engine, coordinator) = Engine::new();
        #[cfg(feature = "latency-probe")]
        let engine = {
            let mut engine = engine;
            engine.set_first_fill_probe(Arc::clone(&first_fill_probe.inner));
            engine
        };
        let events = coordinator.device_event_sender();
        let backend = WasapiBackend::start(engine, events)?;
        publish_current_diagnostics(coordinator.diagnostics());
        publish_current_device_info(backend.device_info);
        Ok(Self {
            coordinator,
            backend,
            #[cfg(feature = "latency-probe")]
            first_fill_probe,
        })
    }

    /// Compact device metrics for the host's non-render-thread `device_opened` log.
    pub fn device_opened_fields_json() -> Option<String> {
        current_device_info().map(DeviceOpenInfo::fields_json)
    }

    /// Playback metrics are read by the host after its owner reports clip end.
    pub fn playback_snapshot_json(playback_id: u64) -> Option<String> {
        let snapshot = current_playback_snapshot(playback_id)?;
        Some(format!(
            "{{\"pcm_frames\":{},\"cursor_frames\":{},\"clip_start_latency_ns\":{},\"frames_rendered\":{},\"frames_written\":{},\"write_error_count\":{},\"event_timeout_count\":{},\"snapshot_pending\":{},\"rendered_duration_known\":{},\"frames_available_total\":{},\"frames_requested_total\":{},\"late_fills\":{},\"max_lateness_ns\":{},\"min_padding_frames\":{}}}",
            snapshot.pcm_frames,
            snapshot.cursor_frames,
            json_u64(snapshot.first_fill_latency_ns),
            json_u64(snapshot.frames_rendered),
            json_u64(snapshot.frames_rendered),
            json_u64(snapshot.write_error_count),
            snapshot.event_timeout_count,
            snapshot.snapshot_pending,
            snapshot.rendered_duration_known,
            json_u64(snapshot.frames_available_total),
            json_u64(snapshot.frames_requested_total),
            json_u64(snapshot.late_fills),
            json_i64(snapshot.max_lateness_ns),
            json_u64(snapshot.min_padding_frames),
        ))
    }

    /// Current priority of the render thread, or null when Windows reports failure.
    pub fn render_thread_priority() -> Option<i32> {
        current_device_info().and_then(|info| info.render_thread_priority)
    }

    #[cfg(feature = "latency-probe")]
    pub fn first_fill_probe(&self) -> FirstFillProbe {
        self.first_fill_probe.clone()
    }

    #[cfg(test)]
    fn start_with_first_service_gate() -> Result<Self, StartError> {
        #[cfg(feature = "latency-probe")]
        let first_fill_probe = FirstFillProbe::new();
        let (engine, coordinator) = Engine::new();
        #[cfg(feature = "latency-probe")]
        let engine = {
            let mut engine = engine;
            engine.set_first_fill_probe(Arc::clone(&first_fill_probe.inner));
            engine
        };
        let events = coordinator.device_event_sender();
        let backend = WasapiBackend::start_with_first_service_gate(engine, events)?;
        publish_current_diagnostics(coordinator.diagnostics());
        publish_current_device_info(backend.device_info);
        Ok(Self {
            coordinator,
            backend,
            #[cfg(feature = "latency-probe")]
            first_fill_probe,
        })
    }

    #[cfg(test)]
    fn release_first_service_gate(&mut self) {
        self.backend.release_first_service_gate();
    }
}

impl Sink for WasapiSink {
    fn submit(&mut self, id: PlaybackId, pcm: Pcm) -> Result<(), SinkError> {
        self.coordinator.submit(id, pcm)
    }

    fn cancel(&mut self, id: PlaybackId) {
        self.coordinator.cancel(id);
    }

    fn cancel_with_fade(&mut self, id: PlaybackId, fade_samples: usize) {
        self.coordinator.cancel_with_fade(id, fade_samples);
    }

    fn poll_event(&mut self) -> Option<SinkEvent> {
        self.coordinator.poll_event()
    }
}

impl Drop for WasapiSink {
    fn drop(&mut self) {
        self.backend.shutdown();
    }
}

struct WasapiBackend {
    shutdown: Option<SyncSender<()>>,
    thread: Option<JoinHandle<()>>,
    device_info: DeviceOpenInfo,
    #[cfg(test)]
    first_service_gate: Option<SyncSender<()>>,
}

impl WasapiBackend {
    fn start(engine: Engine, events: DeviceEventSender) -> Result<Self, StartError> {
        Self::start_inner(engine, events, None, None)
    }

    #[cfg(test)]
    fn start_with_first_service_gate(
        engine: Engine,
        events: DeviceEventSender,
    ) -> Result<Self, StartError> {
        let (gate_sender, gate_receiver) = mpsc::sync_channel(1);
        Self::start_inner(engine, events, Some(gate_receiver), Some(gate_sender))
    }

    fn start_inner(
        engine: Engine,
        events: DeviceEventSender,
        first_service_gate_receiver: Option<Receiver<()>>,
        _first_service_gate_sender: Option<SyncSender<()>>,
    ) -> Result<Self, StartError> {
        let (shutdown_sender, shutdown_receiver) = mpsc::sync_channel(1);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .spawn(move || {
                render_thread(
                    engine,
                    events,
                    shutdown_receiver,
                    first_service_gate_receiver,
                    ready_sender,
                );
            })
            .map_err(|_| StartError::ThreadSpawn)?;

        match ready_receiver.recv() {
            Ok(Ok(device_info)) => Ok(Self {
                shutdown: Some(shutdown_sender),
                thread: Some(thread),
                device_info,
                #[cfg(test)]
                first_service_gate: _first_service_gate_sender,
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(StartError::ThreadStartFailed)
            }
        }
    }

    #[cfg(test)]
    fn release_first_service_gate(&mut self) {
        if let Some(gate) = self.first_service_gate.take() {
            let _ = gate.try_send(());
        }
    }

    fn shutdown(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.try_send(());
        }
        #[cfg(test)]
        self.release_first_service_gate();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for WasapiBackend {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct MtaGuard;

impl MtaGuard {
    fn initialize() -> Result<Self, StartError> {
        if wasapi::initialize_mta().is_err() {
            return Err(StartError::ComInitialization);
        }
        Ok(Self)
    }
}

impl Drop for MtaGuard {
    fn drop(&mut self) {
        wasapi::deinitialize();
    }
}

struct OpenedStream {
    client: AudioClient,
    event: wasapi::Handle,
    renderer: AudioRenderClient,
    _registration: DeviceEventRegistration,
    sample_buffer: Vec<i16>,
    byte_buffer: Vec<u8>,
    buffer_frames: usize,
    block_align: usize,
    info: DeviceOpenInfo,
}

impl OpenedStream {
    fn open(events: &DeviceEventSender) -> Result<Self, StartError> {
        let enumerator = DeviceEnumerator::new().map_err(|_| StartError::WasapiInitialization)?;
        let device = enumerator
            .get_default_device(&Direction::Render)
            .map_err(|_| StartError::NoRenderEndpoint)?;
        let endpoint_id = device
            .get_id()
            .map_err(|_| StartError::WasapiInitialization)?;

        let mut callbacks = DeviceEventCallbacks::new();
        let removed_endpoint = endpoint_id.clone();
        let removed_events = events.clone_for_callback();
        callbacks.set_device_removed_callback(move |id| {
            if id == removed_endpoint {
                removed_events.device_lost();
            }
        });
        let state_endpoint = endpoint_id.clone();
        let state_events = events.clone_for_callback();
        callbacks.set_device_state_callback(move |id, _state| {
            if id == state_endpoint {
                state_events.device_lost();
            }
        });
        let default_events = events.clone_for_callback();
        callbacks.set_default_device_callback(move |direction, _role, _id| {
            if direction == Direction::Render {
                default_events.device_changed();
            }
        });
        let registration = enumerator
            .register_notification_callback(callbacks)
            .map_err(|_| StartError::WasapiInitialization)?;

        let mut client = device
            .get_iaudioclient()
            .map_err(|_| StartError::WasapiInitialization)?;
        let (default_period_hns, min_period_hns) = client
            .get_device_period()
            .map_err(|_| StartError::WasapiInitialization)?;
        let mix_format = client.get_mixformat().ok().map(|format| DeviceMixFormat {
            sample_rate_hz: format.get_samplespersec(),
            channels: format.get_nchannels(),
            bits_per_sample: format.get_bitspersample(),
        });
        let sample_rate = usize::try_from(REQUESTED_SAMPLE_RATE_HZ)
            .map_err(|_| StartError::WasapiInitialization)?;
        let format = WaveFormat::new(
            usize::from(REQUESTED_BITS_PER_SAMPLE),
            usize::from(REQUESTED_BITS_PER_SAMPLE),
            &SampleType::Int,
            sample_rate,
            usize::from(REQUESTED_CHANNELS),
            Some(0),
        );
        let buffer_requested_hns = requested_buffer_duration_hns(default_period_hns)
            .ok_or(StartError::WasapiInitialization)?;
        let buffer_fallback = initialize_with_buffer_fallback(
            buffer_requested_hns,
            default_period_hns,
            |buffer_duration_hns| {
                let mode = StreamMode::EventsShared {
                    autoconvert: true,
                    buffer_duration_hns,
                };
                client.initialize_client(&format, &Direction::Render, &mode)
            },
        )
        .map_err(|_| StartError::WasapiInitialization)?;
        let event = client
            .set_get_eventhandle()
            .map_err(|_| StartError::WasapiInitialization)?;
        let renderer = client
            .get_audiorenderclient()
            .map_err(|_| StartError::WasapiInitialization)?;
        let buffer_frames = usize::try_from(
            client
                .get_buffer_size()
                .map_err(|_| StartError::WasapiInitialization)?,
        )
        .map_err(|_| StartError::WasapiInitialization)?;
        let block_align = usize::try_from(format.get_blockalign())
            .map_err(|_| StartError::WasapiInitialization)?;
        if buffer_frames == 0 || block_align != 2 {
            return Err(StartError::WasapiInitialization);
        }
        let byte_capacity = buffer_frames
            .checked_mul(block_align)
            .ok_or(StartError::WasapiInitialization)?;
        let mut sample_buffer = Vec::new();
        sample_buffer
            .try_reserve_exact(buffer_frames)
            .map_err(|_| StartError::WasapiInitialization)?;
        sample_buffer.resize(buffer_frames, 0_i16);
        let mut byte_buffer = Vec::new();
        byte_buffer
            .try_reserve_exact(byte_capacity)
            .map_err(|_| StartError::WasapiInitialization)?;
        byte_buffer.resize(byte_capacity, 0_u8);

        let initial_frames = usize::try_from(
            client
                .get_available_space_in_frames()
                .map_err(|_| StartError::WasapiInitialization)?,
        )
        .map_err(|_| StartError::WasapiInitialization)?;
        let initial_bytes = initial_frames
            .checked_mul(block_align)
            .ok_or(StartError::WasapiInitialization)?;
        if initial_frames > buffer_frames || initial_bytes > byte_buffer.len() {
            return Err(StartError::WasapiInitialization);
        }
        if initial_frames > 0 {
            byte_buffer[..initial_bytes].fill(0);
            renderer
                .write_to_device(
                    initial_frames,
                    &byte_buffer[..initial_bytes],
                    Some(BufferFlags {
                        data_discontinuity: false,
                        silent: true,
                        timestamp_error: false,
                    }),
                )
                .map_err(|_| StartError::WasapiInitialization)?;
        }
        client
            .start_stream()
            .map_err(|_| StartError::WasapiInitialization)?;

        Ok(Self {
            client,
            event,
            renderer,
            _registration: registration,
            sample_buffer,
            byte_buffer,
            buffer_frames,
            block_align,
            info: DeviceOpenInfo {
                mix_format,
                default_period_hns: Some(default_period_hns),
                min_period_hns: Some(min_period_hns),
                buffer_frames: buffer_frames as u64,
                buffer_requested_hns: Some(buffer_requested_hns),
                buffer_fallback,
                mmcss_task_index: None,
                mmcss_ok: false,
                mmcss_error_code: None,
                priority_fallback_error_code: None,
                render_thread_priority: None,
            },
        })
    }

    fn service(
        &mut self,
        engine: &mut Engine,
        lateness_ns: Option<i64>,
        period_ns: Option<u64>,
    ) -> Result<(), ()> {
        let padding = self.client.get_current_padding().map_err(|_| ())?;
        let padding_frames = usize::try_from(padding).map_err(|_| ())?;
        if padding_frames > self.buffer_frames {
            return Err(());
        }
        let frame_count = self.buffer_frames - padding_frames;
        // `available_frames` is WASAPI's current free capacity; `requested_frames`
        // is one complete opened event buffer period, so their sum exposes shortages.
        // Render has no capture-side DATA_DISCONTINUITY signal; padding == 0 is the
        // possible-underrun signal used for this shared-mode stream.
        let observation = RenderObservation {
            lateness_ns,
            period_ns,
            available_frames: u32::try_from(frame_count).map_err(|_| ())?,
            requested_frames: u32::try_from(self.buffer_frames).map_err(|_| ())?,
            padding_frames: u32::try_from(padding_frames).map_err(|_| ())?,
        };
        if frame_count == 0 {
            engine.record_no_space_callback(observation);
            return Ok(());
        }
        let sample_count = frame_count;
        let byte_count = sample_count.checked_mul(self.block_align).ok_or(())?;
        if sample_count > self.sample_buffer.len() || byte_count > self.byte_buffer.len() {
            return Err(());
        }

        let rendered =
            engine.fill_observed(&mut self.sample_buffer[..sample_count], Some(observation));
        for (sample, destination) in self.sample_buffer[..sample_count]
            .iter()
            .zip(self.byte_buffer[..byte_count].chunks_exact_mut(self.block_align))
        {
            let bytes = sample.to_le_bytes();
            destination.copy_from_slice(&bytes);
        }
        match self.renderer.write_to_device(
            frame_count,
            &self.byte_buffer[..byte_count],
            Some(BufferFlags {
                data_discontinuity: false,
                silent: false,
                timestamp_error: false,
            }),
        ) {
            Ok(()) => {
                if let Some(rendered) = rendered {
                    engine.record_rendered_frames(rendered.playback_id, rendered.frames);
                }
                Ok(())
            }
            Err(_) => {
                if let Some(rendered) = rendered {
                    engine.record_write_error(rendered.playback_id);
                }
                Err(())
            }
        }
    }
}

fn render_thread(
    mut engine: Engine,
    events: DeviceEventSender,
    shutdown: Receiver<()>,
    first_service_gate: Option<Receiver<()>>,
    ready: SyncSender<Result<DeviceOpenInfo, StartError>>,
) {
    let scheduling = configure_thread_scheduling(&mut Win32ThreadSchedulingApi);
    let ThreadSchedulingOutcome {
        report: scheduling_report,
        mmcss_handle,
    } = scheduling;
    let _mmcss_registration = MmcssRegistration(mmcss_handle);

    let _mta = match MtaGuard::initialize() {
        Ok(guard) => guard,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let mut stream = match OpenedStream::open(&events) {
        Ok(stream) => stream,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let device_info = stream.info.with_thread_scheduling(scheduling_report);
    stream.info = device_info;
    if ready.send(Ok(device_info)).is_err() {
        let _ = stream.client.stop_stream();
        return;
    }

    if let Some(gate) = first_service_gate {
        loop {
            match gate.recv_timeout(Duration::from_millis(u64::from(SHUTDOWN_POLL_MS))) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => match shutdown.try_recv() {
                    Ok(()) | Err(TryRecvError::Disconnected) => {
                        let _ = stream.client.stop_stream();
                        return;
                    }
                    Err(TryRecvError::Empty) => {}
                },
            }
        }
    }

    // Event cadence follows the endpoint's default device period. The returned
    // buffer size is larger and is not the event period for lateness accounting.
    let period_ns = period_ns_from_hns(stream.info.default_period_hns);
    let mut wake_lateness = WakeLatenessTracker::new(period_ns.map(Duration::from_nanos));

    loop {
        match shutdown.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }
        if events.stop_requested() {
            break;
        }
        let actual_wake = match stream.event.wait_for_event(SHUTDOWN_POLL_MS) {
            Ok(()) => Instant::now(),
            Err(wasapi::WasapiError::EventTimeout) => {
                // A timeout is counted as one possible missed fill while a clip is active.
                engine.record_event_timeout();
                continue;
            }
            Err(_) => {
                events.device_lost();
                break;
            }
        };
        match shutdown.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }
        if events.stop_requested() {
            break;
        }
        let lateness_ns = wake_lateness.observe(actual_wake);
        if stream.service(&mut engine, lateness_ns, period_ns).is_err() {
            events.device_lost();
            break;
        }
    }
    let _ = stream.client.stop_stream();
}

#[cfg(test)]
mod logic_tests {
    use super::{
        configure_thread_scheduling, initialize_with_buffer_fallback, period_ns_from_hns,
        requested_buffer_duration_hns, ThreadSchedulingApi, WakeLatenessTracker,
        THREAD_PRIORITY_TIME_CRITICAL,
    };
    use std::time::{Duration, Instant};

    struct FakeSchedulingApi {
        mmcss_result: Result<(u32, u32), u32>,
        priority_result: Result<(), u32>,
        priority: i32,
        priority_after_mmcss: i32,
        mmcss_calls: usize,
        priority_calls: usize,
    }

    impl ThreadSchedulingApi for FakeSchedulingApi {
        type Handle = u32;

        fn av_set_mm_thread_characteristics(&mut self) -> Result<(Self::Handle, u32), u32> {
            self.mmcss_calls += 1;
            if self.mmcss_result.is_ok() {
                self.priority = self.priority_after_mmcss;
            }
            self.mmcss_result
        }

        fn set_time_critical_priority(&mut self) -> Result<(), u32> {
            self.priority_calls += 1;
            if self.priority_result.is_ok() {
                self.priority = THREAD_PRIORITY_TIME_CRITICAL;
            }
            self.priority_result
        }

        fn current_priority(&mut self) -> Option<i32> {
            Some(self.priority)
        }
    }

    #[test]
    fn first_render_callback_lateness_is_unmeasured() {
        let mut timing = WakeLatenessTracker::new(Some(Duration::from_millis(10)));
        assert_eq!(timing.observe(Instant::now()), None);
    }

    #[test]
    fn render_callback_exactly_on_deadline_reports_zero_lateness() {
        let start = Instant::now();
        let mut timing = WakeLatenessTracker::new(Some(Duration::from_millis(10)));
        assert_eq!(timing.observe(start), None);
        assert_eq!(timing.observe(start + Duration::from_millis(10)), Some(0));
    }

    #[test]
    fn render_callback_after_deadline_reports_positive_lateness() {
        let start = Instant::now();
        let mut timing = WakeLatenessTracker::new(Some(Duration::from_millis(10)));
        timing.observe(start);
        assert_eq!(
            timing.observe(start + Duration::from_millis(16)),
            Some(6_000_000)
        );
    }

    #[test]
    fn render_callback_before_deadline_reports_negative_delta() {
        let start = Instant::now();
        let mut timing = WakeLatenessTracker::new(Some(Duration::from_millis(10)));
        timing.observe(start);
        assert_eq!(
            timing.observe(start + Duration::from_millis(8)),
            Some(-2_000_000)
        );
    }

    #[test]
    fn expected_deadline_advances_from_the_previous_actual_wake() {
        let start = Instant::now();
        let mut timing = WakeLatenessTracker::new(Some(Duration::from_millis(10)));
        timing.observe(start);
        assert_eq!(
            timing.observe(start + Duration::from_millis(12)),
            Some(2_000_000)
        );
        assert_eq!(
            timing.observe(start + Duration::from_millis(27)),
            Some(5_000_000)
        );
    }

    #[test]
    fn wake_lateness_remains_unmeasured_without_an_event_period() {
        let start = Instant::now();
        let mut timing = WakeLatenessTracker::new(None);
        assert_eq!(timing.observe(start), None);
        assert_eq!(timing.observe(start + Duration::from_millis(10)), None);
    }

    #[test]
    fn device_period_hns_converts_to_nanoseconds() {
        assert_eq!(period_ns_from_hns(Some(100_000)), Some(10_000_000));
        assert_eq!(period_ns_from_hns(None), None);
    }

    #[test]
    fn requested_shared_buffer_is_four_device_periods() {
        let requested = requested_buffer_duration_hns(100_000).unwrap();
        assert_eq!(requested, 400_000);
        assert_eq!(requested as f64 / 10_000.0, 40.0);
    }

    #[test]
    fn larger_buffer_failure_retries_once_at_the_default_period() {
        let mut requests = Vec::new();
        let fallback = initialize_with_buffer_fallback(400_000, 100_000, |duration| {
            requests.push(duration);
            if duration == 400_000 {
                Err("large request rejected")
            } else {
                Ok(())
            }
        })
        .unwrap();

        assert!(fallback);
        assert_eq!(requests, [400_000, 100_000]);
    }

    #[test]
    fn accepted_larger_buffer_does_not_retry() {
        let mut requests = Vec::new();
        let fallback = initialize_with_buffer_fallback(400_000, 100_000, |duration| {
            requests.push(duration);
            Ok::<(), ()>(())
        })
        .unwrap();

        assert!(!fallback);
        assert_eq!(requests, [400_000]);
    }

    #[test]
    fn successful_mmcss_setup_skips_priority_fallback() {
        let mut api = FakeSchedulingApi {
            mmcss_result: Ok((42, 7)),
            priority_result: Err(87),
            priority: 0,
            priority_after_mmcss: 6,
            mmcss_calls: 0,
            priority_calls: 0,
        };

        let result = configure_thread_scheduling(&mut api);

        assert_eq!(result.mmcss_handle, Some(42));
        assert_eq!(result.report.mmcss_task_index, Some(7));
        assert!(result.report.mmcss_ok);
        assert_eq!(result.report.mmcss_error_code, None);
        assert_eq!(result.report.priority_fallback_error_code, None);
        assert_eq!(result.report.render_thread_priority, Some(6));
        assert_eq!(api.mmcss_calls, 1);
        assert_eq!(api.priority_calls, 0);
    }

    #[test]
    fn mmcss_failure_uses_time_critical_priority_fallback() {
        let mut api = FakeSchedulingApi {
            mmcss_result: Err(5),
            priority_result: Ok(()),
            priority: 0,
            priority_after_mmcss: 0,
            mmcss_calls: 0,
            priority_calls: 0,
        };

        let result = configure_thread_scheduling(&mut api);

        assert_eq!(result.mmcss_handle, None);
        assert_eq!(result.report.mmcss_task_index, None);
        assert!(!result.report.mmcss_ok);
        assert_eq!(result.report.mmcss_error_code, Some(5));
        assert_eq!(result.report.priority_fallback_error_code, None);
        assert_eq!(
            result.report.render_thread_priority,
            Some(THREAD_PRIORITY_TIME_CRITICAL)
        );
        assert_eq!(api.priority_calls, 1);
    }

    #[test]
    fn both_scheduling_paths_failing_preserves_priority_and_reports_both_codes() {
        let mut api = FakeSchedulingApi {
            mmcss_result: Err(5),
            priority_result: Err(87),
            priority: 2,
            priority_after_mmcss: 0,
            mmcss_calls: 0,
            priority_calls: 0,
        };

        let result = configure_thread_scheduling(&mut api);

        assert_eq!(result.report.mmcss_error_code, Some(5));
        assert_eq!(result.report.priority_fallback_error_code, Some(87));
        assert_eq!(result.report.render_thread_priority, Some(2));
        assert_eq!(api.priority, 2);
        assert_eq!(api.mmcss_calls, 1);
        assert_eq!(api.priority_calls, 1);
    }
}
