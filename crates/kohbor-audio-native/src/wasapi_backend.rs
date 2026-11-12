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
use crate::engine::{DeviceEventSender, Engine, EngineHandle};
use kohbor_audio::pcm::Pcm;
use kohbor_audio::{PlaybackId, Sink, SinkError, SinkEvent};
use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use wasapi::{
    AudioClient, AudioRenderClient, BufferFlags, DeviceEnumerator, DeviceEventCallbacks,
    DeviceEventRegistration, Direction, SampleType, StreamMode, WaveFormat,
};

const SHUTDOWN_POLL_MS: u32 = 100;

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
}

impl WasapiSink {
    /// Open the current default render endpoint with shared event-driven output.
    pub fn start() -> Result<Self, StartError> {
        let (engine, coordinator) = Engine::new();
        let events = coordinator.device_event_sender();
        let backend = WasapiBackend::start(engine, events)?;
        Ok(Self {
            coordinator,
            backend,
        })
    }

    #[cfg(test)]
    fn start_with_first_service_gate() -> Result<Self, StartError> {
        let (engine, coordinator) = Engine::new();
        let events = coordinator.device_event_sender();
        let backend = WasapiBackend::start_with_first_service_gate(engine, events)?;
        Ok(Self {
            coordinator,
            backend,
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
            Ok(Ok(())) => Ok(Self {
                shutdown: Some(shutdown_sender),
                thread: Some(thread),
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
        let (default_period_hns, _) = client
            .get_device_period()
            .map_err(|_| StartError::WasapiInitialization)?;
        let format = WaveFormat::new(16, 16, &SampleType::Int, 48_000, 1, Some(0));
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: default_period_hns,
        };
        client
            .initialize_client(&format, &Direction::Render, &mode)
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
        })
    }

    fn service(&mut self, engine: &mut Engine) -> Result<(), ()> {
        let frames = self
            .client
            .get_available_space_in_frames()
            .map_err(|_| ())?;
        let frame_count = usize::try_from(frames).map_err(|_| ())?;
        if frame_count == 0 {
            return Ok(());
        }
        if frame_count > self.buffer_frames {
            return Err(());
        }
        let sample_count = frame_count;
        let byte_count = sample_count.checked_mul(self.block_align).ok_or(())?;
        if sample_count > self.sample_buffer.len() || byte_count > self.byte_buffer.len() {
            return Err(());
        }

        engine.fill(&mut self.sample_buffer[..sample_count]);
        for (index, sample) in self.sample_buffer[..sample_count].iter().enumerate() {
            let offset = index.checked_mul(self.block_align).ok_or(())?;
            let end = offset.checked_add(self.block_align).ok_or(())?;
            let bytes = sample.to_le_bytes();
            let Some(destination) = self.byte_buffer.get_mut(offset..end) else {
                return Err(());
            };
            destination.copy_from_slice(&bytes);
        }
        self.renderer
            .write_to_device(
                frame_count,
                &self.byte_buffer[..byte_count],
                Some(BufferFlags {
                    data_discontinuity: false,
                    silent: false,
                    timestamp_error: false,
                }),
            )
            .map_err(|_| ())
    }
}

fn render_thread(
    mut engine: Engine,
    events: DeviceEventSender,
    shutdown: Receiver<()>,
    first_service_gate: Option<Receiver<()>>,
    ready: SyncSender<Result<(), StartError>>,
) {
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
    if ready.send(Ok(())).is_err() {
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

    loop {
        match shutdown.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }
        if events.stop_requested() {
            break;
        }
        match stream.event.wait_for_event(SHUTDOWN_POLL_MS) {
            Ok(()) => {}
            Err(wasapi::WasapiError::EventTimeout) => continue,
            Err(_) => {
                events.device_lost();
                break;
            }
        }
        match shutdown.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }
        if events.stop_requested() {
            break;
        }
        if stream.service(&mut engine).is_err() {
            events.device_lost();
            break;
        }
    }
    let _ = stream.client.stop_stream();
}
