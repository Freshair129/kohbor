use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE,
    EDataFlow, ERole, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, IMMNotificationClient,
    MMDeviceEnumerator, WAVE_FORMAT_PCM, WAVEFORMATEX, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows_core::{PCWSTR, implement};

const ITERATIONS: usize = 200;
const HNS_PER_MILLISECOND: f64 = 10_000.0;

fn main() -> Result<(), Box<dyn Error>> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let result = run();
    unsafe { CoUninitialize() };
    result
}

fn run() -> Result<(), Box<dyn Error>> {
    let watch = std::env::args().any(|arg| arg == "--watch");
    let origin = Instant::now();
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let device = unsafe {
        enumerator.GetDefaultAudioEndpoint(eRender, windows::Win32::Media::Audio::eConsole)?
    };
    let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None)? };
    let mix_ptr = unsafe { client.GetMixFormat()? };
    let mix = unsafe { *mix_ptr };
    let mut default_period_hns = 0_i64;
    let mut minimum_period_hns = 0_i64;
    unsafe {
        client.GetDevicePeriod(Some(&mut default_period_hns), Some(&mut minimum_period_hns))?;
    }

    let desired = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 1,
        nSamplesPerSec: 48_000,
        nAvgBytesPerSec: 96_000,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    let mut closest: *mut WAVEFORMATEX = std::ptr::null_mut();
    let exact_status =
        unsafe { client.IsFormatSupported(AUDCLNT_SHAREMODE_SHARED, &desired, Some(&mut closest)) };
    let exact_supported = exact_status.0 == 0;
    if !closest.is_null() {
        unsafe { CoTaskMemFree(Some(closest.cast())) };
    }

    let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    let initialized_as_pcm = unsafe {
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                flags,
                default_period_hns,
                0,
                &desired,
                None,
            )
            .is_ok()
    };
    let (client, active_format) = if initialized_as_pcm {
        (client, desired)
    } else {
        let fallback: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None)? };
        let fallback_flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
        unsafe {
            fallback.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                fallback_flags,
                default_period_hns,
                0,
                mix_ptr,
                None,
            )?;
        }
        (fallback, mix)
    };

    let event = unsafe { CreateEventW(None, false, false, PCWSTR::null())? };
    unsafe { client.SetEventHandle(event)? };
    let renderer: IAudioRenderClient = unsafe { client.GetService()? };
    let buffer_frames = unsafe { client.GetBufferSize()? };
    let stream_latency_hns = unsafe { client.GetStreamLatency()? };
    let active_rate = active_format.nSamplesPerSec;
    let active_channels = active_format.nChannels;
    let active_bits = active_format.wBitsPerSample;
    let active_tag = active_format.wFormatTag;
    let mix_rate = mix.nSamplesPerSec;
    let mix_channels = mix.nChannels;
    let mix_bits = mix.wBitsPerSample;
    let mix_tag = mix.wFormatTag;
    let initial_buffer = unsafe { renderer.GetBuffer(buffer_frames)? };
    let _ = initial_buffer;
    unsafe {
        renderer.ReleaseBuffer(buffer_frames, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
        client.Start()?;
    }

    let (notification_tx, notification_rx) = mpsc::channel::<String>();
    let notification: IMMNotificationClient = NotificationSink {
        tx: notification_tx,
        origin,
    }
    .into();
    unsafe { enumerator.RegisterEndpointNotificationCallback(&notification)? };

    println!(
        "api=windows version=0.62.2 stream=shared event_driven=true stream_config={}Hz/{}ch/{}bit tag={} pcm16_mono48k_exact_supported={exact_supported} autoconvert_initialize={initialized_as_pcm} buffer_frames={buffer_frames} buffer_ms={:.3} device_period_ms={:.3} min_period_ms={:.3} stream_latency_ms={:.3}",
        active_rate,
        active_channels,
        active_bits,
        active_tag,
        frames_ms(buffer_frames, active_rate),
        hns_ms(default_period_hns),
        hns_ms(minimum_period_hns),
        hns_ms(stream_latency_hns)
    );
    println!(
        "mix_format={}Hz/{}ch/{}bit tag={}",
        mix_rate, mix_channels, mix_bits, mix_tag
    );

    if watch {
        println!("event=WatchStarted t_ms=0 stream=silent; Ctrl+C ends watch");
        let mut stream_valid = true;
        loop {
            let wait = unsafe { WaitForSingleObject(event, 250) }.0;
            print_notifications(&notification_rx);
            if !stream_valid {
                thread::sleep(Duration::from_millis(250));
            } else if wait == 0 {
                if let Err(error) = render_silence(&client, &renderer, buffer_frames) {
                    println!(
                        "event=StreamInvalidated t_ms={} detail={error}",
                        elapsed_ms(origin)
                    );
                    stream_valid = false;
                }
            } else if wait == u32::MAX {
                println!(
                    "event=StreamInvalidated t_ms={} wait=WAIT_FAILED",
                    elapsed_ms(origin)
                );
                stream_valid = false;
            }
        }
    }

    // Leave the WASAPI owner on this thread; a second thread submits an atomic
    // marker while the owner waits for the next render event.
    let wait = unsafe { WaitForSingleObject(event, 2_000) }.0;
    if wait != 0 {
        return Err("initial render event wait failed".into());
    }
    render_silence(&client, &renderer, buffer_frames)?;
    let request = Arc::new(AtomicUsize::new(0));
    let submitted = Arc::new(AtomicUsize::new(0));
    let submit_ns = Arc::new(AtomicU64::new(0));
    let producer_request = Arc::clone(&request);
    let producer_submitted = Arc::clone(&submitted);
    let producer_submit_ns = Arc::clone(&submit_ns);
    let producer_origin = origin;
    let producer = thread::spawn(move || {
        for sequence in 1..=ITERATIONS {
            while producer_request.load(Ordering::Acquire) < sequence {
                thread::yield_now();
            }
            producer_submit_ns.store(
                producer_origin.elapsed().as_nanos() as u64,
                Ordering::Release,
            );
            producer_submitted.store(sequence, Ordering::Release);
        }
    });

    let mut submit_ms = Vec::with_capacity(ITERATIONS);
    for sequence in 1..=ITERATIONS {
        request.store(sequence, Ordering::Release);
        while submitted.load(Ordering::Acquire) < sequence {
            thread::yield_now();
        }
        let wait = unsafe { WaitForSingleObject(event, 2_000) }.0;
        if wait != 0 {
            return Err("render event wait failed".into());
        }
        render_silence(&client, &renderer, buffer_frames)?;
        let started_ns = submit_ns.swap(0, Ordering::AcqRel);
        submit_ms.push((origin.elapsed().as_nanos() as u64 - started_ns) as f64 / 1_000_000.0);
    }
    producer.join().map_err(|_| "submit producer panicked")?;
    print_stats("submit_to_event_render_ms", &submit_ms);

    let mut stop_start_ms = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        unsafe {
            client.Stop()?;
            client.Start()?;
        }
        stop_start_ms.push(start.elapsed().as_secs_f64() * 1_000.0);
    }
    print_stats("stop_start_call_pair_ms", &stop_start_ms);
    print_notifications(&notification_rx);

    unsafe {
        enumerator.UnregisterEndpointNotificationCallback(&notification)?;
        client.Stop()?;
        CloseHandle(HANDLE(event.0))?;
        CoTaskMemFree(Some(mix_ptr.cast()));
    }
    Ok(())
}

fn render_silence(
    client: &IAudioClient,
    renderer: &IAudioRenderClient,
    buffer_frames: u32,
) -> windows::core::Result<()> {
    let padding = unsafe { client.GetCurrentPadding()? };
    let available = buffer_frames.saturating_sub(padding);
    if available == 0 {
        return Ok(());
    }
    let buffer = unsafe { renderer.GetBuffer(available)? };
    let _ = buffer;
    unsafe { renderer.ReleaseBuffer(available, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) }
}

fn print_notifications(receiver: &Receiver<String>) {
    while let Ok(event) = receiver.try_recv() {
        println!("{event}");
    }
}

fn frames_ms(frames: u32, sample_rate: u32) -> f64 {
    frames as f64 * 1_000.0 / sample_rate as f64
}

fn hns_ms(value: i64) -> f64 {
    value as f64 / HNS_PER_MILLISECOND
}

fn elapsed_ms(origin: Instant) -> u128 {
    origin.elapsed().as_millis()
}

fn print_stats(label: &str, values: &[f64]) {
    if values.is_empty() {
        println!("metric={label} n=0 result=NOT_RUN");
        return;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| sorted[((sorted.len() - 1) as f64 * p).ceil() as usize];
    println!(
        "metric={label} n={} min={:.6} p50={:.6} p95={:.6} max={:.6}",
        sorted.len(),
        sorted[0],
        percentile(0.50),
        percentile(0.95),
        sorted[sorted.len() - 1]
    );
}

#[implement(IMMNotificationClient)]
struct NotificationSink {
    tx: Sender<String>,
    origin: Instant,
}

impl NotificationSink {
    fn push(&self, event: &str, id: &PCWSTR, extra: &str) {
        let device_id = if id.is_null() {
            "none".to_owned()
        } else {
            unsafe { id.to_string() }.unwrap_or_else(|_| "unreadable".to_owned())
        };
        let _ = self.tx.send(format!(
            "event={event} t_ms={} id={device_id} {extra}",
            elapsed_ms(self.origin)
        ));
    }
}

impl windows::Win32::Media::Audio::IMMNotificationClient_Impl for NotificationSink_Impl {
    fn OnDeviceStateChanged(&self, id: &PCWSTR, state: DEVICE_STATE) -> windows::core::Result<()> {
        self.push("DeviceStateChanged", id, &format!("state={}", state.0));
        Ok(())
    }

    fn OnDeviceAdded(&self, id: &PCWSTR) -> windows::core::Result<()> {
        self.push("DeviceAdded", id, "");
        Ok(())
    }

    fn OnDeviceRemoved(&self, id: &PCWSTR) -> windows::core::Result<()> {
        self.push("DeviceRemoved", id, "");
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        id: &PCWSTR,
    ) -> windows::core::Result<()> {
        self.push(
            "DefaultDeviceChanged",
            id,
            &format!("flow={} role={}", flow.0, role.0),
        );
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        id: &PCWSTR,
        _key: &windows::Win32::Foundation::PROPERTYKEY,
    ) -> windows::core::Result<()> {
        self.push("DevicePropertyChanged", id, "");
        Ok(())
    }
}
