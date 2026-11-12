use std::error::Error;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};

const ITERATIONS: usize = 200;
const PCM_RATE: u32 = 48_000;

fn main() -> Result<(), Box<dyn Error>> {
    let watch = std::env::args().any(|arg| arg == "--watch");
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default render device")?;

    let (config, sample_format, source_exact) = choose_config(&device)?;
    let pcm: Arc<[i16]> = Arc::from(vec![0_i16; PCM_RATE as usize]);
    let pcm_buffers = [Arc::clone(&pcm), pcm];
    let pending_ns = Arc::new(AtomicU64::new(0));
    let pending_slot = Arc::new(AtomicUsize::new(0));
    let consumed_ns = Arc::new(AtomicU64::new(0));
    let callback_frames = Arc::new(AtomicUsize::new(0));
    let origin = Instant::now();
    let (error_tx, error_rx) = mpsc::channel::<String>();

    let stream = build_stream_for_format(
        &device,
        &config,
        sample_format,
        pcm_buffers,
        Arc::clone(&pending_ns),
        Arc::clone(&pending_slot),
        Arc::clone(&consumed_ns),
        Arc::clone(&callback_frames),
        origin,
        error_tx,
    )?;
    stream.play()?;

    println!(
        "api=cpal version=0.16.0 host={:?} stream=shared config={}Hz/{}ch/{:?} buffer={:?} pcm=mono/48000/i16 exact_pcm={source_exact}",
        host.id(),
        config.sample_rate.0,
        config.channels,
        sample_format,
        config.buffer_size
    );
    println!("device_period=not_exposed_by_cpal output_latency=not_exposed_by_cpal");

    if watch {
        watch_default_changes(&host, &device, origin, &error_rx);
        return Ok(());
    }

    wait_for_callbacks(&consumed_ns, origin);
    let observed_frames = callback_frames.load(Ordering::Acquire);
    println!(
        "observed_callback_block_frames={observed_frames} observed_callback_block_ms={:.3}",
        observed_frames as f64 * 1_000.0 / config.sample_rate.0 as f64
    );
    let callback_ms = measure_submits(&pending_ns, &pending_slot, &consumed_ns, origin);
    print_stats("submit_to_first_callback_ms", &callback_ms);

    let restart_ms = measure_pause_play(&stream);
    print_stats("pause_play_call_pair_ms", &restart_ms);
    Ok(())
}

fn choose_config(
    device: &cpal::Device,
) -> Result<(cpal::StreamConfig, SampleFormat, bool), Box<dyn Error>> {
    let mut exact = None;
    for range in device.supported_output_configs()? {
        if range.channels() == 1
            && range.sample_format() == SampleFormat::I16
            && range.min_sample_rate().0 <= PCM_RATE
            && range.max_sample_rate().0 >= PCM_RATE
        {
            exact = Some(range.with_sample_rate(cpal::SampleRate(PCM_RATE)));
            break;
        }
    }

    if let Some(config) = exact {
        let format = config.sample_format();
        return Ok((config.config(), format, true));
    }

    let config = device.default_output_config()?;
    let format = config.sample_format();
    Ok((config.config(), format, false))
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    pcm: [Arc<[i16]>; 2],
    pending_ns: Arc<AtomicU64>,
    pending_slot: Arc<AtomicUsize>,
    consumed_ns: Arc<AtomicU64>,
    callback_frames: Arc<AtomicUsize>,
    origin: Instant,
    error_tx: mpsc::Sender<String>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: Sample + SizedSample + FromSample<i16>,
{
    let channels = config.channels as usize;
    let output_rate = config.sample_rate.0 as u64;
    let mut active_slot = 0_usize;
    device.build_output_stream(
        config,
        move |output: &mut [T], _| {
            let _ = callback_frames.compare_exchange(
                0,
                output.len() / channels.max(1),
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
            let start_ns = pending_ns.swap(0, Ordering::AcqRel);
            if start_ns != 0 {
                // Select one of the immutable PCM slots built before opening.
                active_slot = pending_slot.load(Ordering::Acquire);
            }
            let mut source_frame = 0_usize;
            let mut phase = 0_u64;
            for frame in output.chunks_mut(channels.max(1)) {
                let sample = T::from_sample(pcm[active_slot][source_frame]);
                frame.fill(sample);
                phase += PCM_RATE as u64;
                while phase >= output_rate {
                    source_frame += 1;
                    if source_frame == pcm[active_slot].len() {
                        source_frame = 0;
                    }
                    phase -= output_rate;
                }
            }
            if start_ns != 0 {
                consumed_ns.store(origin.elapsed().as_nanos() as u64, Ordering::Release);
            }
        },
        move |error| {
            let _ = error_tx.send(format!("{error:?}"));
        },
        Some(Duration::from_millis(100)),
    )
}

fn build_stream_for_format(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: SampleFormat,
    pcm: [Arc<[i16]>; 2],
    pending_ns: Arc<AtomicU64>,
    pending_slot: Arc<AtomicUsize>,
    consumed_ns: Arc<AtomicU64>,
    callback_frames: Arc<AtomicUsize>,
    origin: Instant,
    error_tx: mpsc::Sender<String>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    macro_rules! build {
        ($sample:ty) => {
            build_stream::<$sample>(
                device,
                config,
                pcm,
                pending_ns,
                pending_slot,
                consumed_ns,
                callback_frames,
                origin,
                error_tx,
            )
        };
    }
    match format {
        SampleFormat::I8 => build!(i8),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I32 => build!(i32),
        SampleFormat::I64 => build!(i64),
        SampleFormat::U8 => build!(u8),
        SampleFormat::U16 => build!(u16),
        SampleFormat::U32 => build!(u32),
        SampleFormat::U64 => build!(u64),
        SampleFormat::F32 => build!(f32),
        SampleFormat::F64 => build!(f64),
        SampleFormat::I24 => build!(cpal::I24),
        _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
    }
}

fn wait_for_callbacks(consumed_ns: &AtomicU64, origin: Instant) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while consumed_ns.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    let _ = origin;
}

fn measure_submits(
    pending_ns: &AtomicU64,
    pending_slot: &AtomicUsize,
    consumed_ns: &AtomicU64,
    origin: Instant,
) -> Vec<f64> {
    let mut samples = Vec::with_capacity(ITERATIONS);
    for sequence in 0..ITERATIONS {
        consumed_ns.store(0, Ordering::Release);
        let start_ns = origin.elapsed().as_nanos() as u64;
        // The two zeroed Arc buffers are prebuilt; submit only publishes a slot and timestamp.
        pending_slot.store(sequence % 2, Ordering::Release);
        pending_ns.store(start_ns, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let consumed = consumed_ns.load(Ordering::Acquire);
            if consumed > start_ns {
                samples.push((consumed - start_ns) as f64 / 1_000_000.0);
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::yield_now();
        }
    }
    samples
}

fn measure_pause_play(stream: &cpal::Stream) -> Vec<f64> {
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        if stream.pause().is_err() || stream.play().is_err() {
            continue;
        }
        samples.push(start.elapsed().as_secs_f64() * 1_000.0);
    }
    samples
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

fn watch_default_changes(
    host: &cpal::Host,
    current_device: &cpal::Device,
    origin: Instant,
    errors: &mpsc::Receiver<String>,
) {
    let mut previous = current_device.name().ok();
    println!("event=WatchStarted t_ms=0 stream=silent");
    loop {
        if let Ok(error) = errors.try_recv() {
            println!(
                "event=StreamError t_ms={} detail={error}",
                elapsed_ms(origin)
            );
        }
        let current = host
            .default_output_device()
            .and_then(|device| device.name().ok());
        if current != previous {
            println!(
                "event=DefaultDeviceChanged t_ms={} name={}",
                elapsed_ms(origin),
                current.as_deref().unwrap_or("none")
            );
            previous = current;
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn elapsed_ms(origin: Instant) -> u128 {
    origin.elapsed().as_millis()
}
