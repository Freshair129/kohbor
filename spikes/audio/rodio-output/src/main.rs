use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{OutputStream, Sink, Source};

const ITERATIONS: usize = 200;

fn main() -> Result<(), Box<dyn Error>> {
    let watch = std::env::args().any(|arg| arg == "--watch");
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default render device")?;
    let config = device.default_output_config()?;
    let sample_format = config.sample_format();
    let channels = config.channels();
    let sample_rate = config.sample_rate().0;
    let buffer = Arc::<[i16]>::from(vec![0_i16; 48_000]);
    let (_stream, handle) = OutputStream::try_from_device(&device)?;
    let sink = Sink::try_new(&handle)?;
    let pending_ns = Arc::new(AtomicU64::new(0));
    let consumed_ns = Arc::new(AtomicU64::new(0));
    let same_thread_consumptions = Arc::new(AtomicUsize::new(0));
    let origin = Instant::now();
    let submit_thread = thread::current().id();

    println!(
        "api=rodio version=0.19.0 stream=shared config={}Hz/{}ch/{:?} buffer=default source=mono/48000/i16 converted_to_f32_mixer=true",
        sample_rate, channels, sample_format
    );
    println!("device_period=not_exposed_by_rodio output_latency=not_exposed_by_rodio");

    if watch {
        sink.append(WatchSilence::new(Arc::clone(&buffer)));
        watch_default_changes(&host, &device, origin);
    }

    let mut latencies = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        consumed_ns.store(0, Ordering::Release);
        let start_ns = origin.elapsed().as_nanos() as u64;
        pending_ns.store(start_ns, Ordering::Release);
        // Sink::append boxes the source and locks its queue on this submit path.
        sink.append(ProbeSilence::new(
            Arc::clone(&buffer),
            Arc::clone(&pending_ns),
            Arc::clone(&consumed_ns),
            Arc::clone(&same_thread_consumptions),
            origin,
            submit_thread,
        ));
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let consumed = consumed_ns.load(Ordering::Acquire);
            if consumed > start_ns {
                latencies.push((consumed - start_ns) as f64 / 1_000_000.0);
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::yield_now();
        }
    }
    print_stats("submit_to_first_source_next_ms", &latencies);
    println!(
        "metric=source_next_on_submit_thread n={}",
        same_thread_consumptions.load(Ordering::Acquire)
    );

    let mut pause_play = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        sink.pause();
        sink.play();
        pause_play.push(start.elapsed().as_secs_f64() * 1_000.0);
    }
    print_stats("sink_pause_play_call_pair_ms", &pause_play);
    Ok(())
}

struct ProbeSilence {
    pcm: Arc<[i16]>,
    pending_ns: Arc<AtomicU64>,
    consumed_ns: Arc<AtomicU64>,
    same_thread_consumptions: Arc<AtomicUsize>,
    origin: Instant,
    submit_thread: thread::ThreadId,
    emitted: bool,
}

impl ProbeSilence {
    fn new(
        pcm: Arc<[i16]>,
        pending_ns: Arc<AtomicU64>,
        consumed_ns: Arc<AtomicU64>,
        same_thread_consumptions: Arc<AtomicUsize>,
        origin: Instant,
        submit_thread: thread::ThreadId,
    ) -> Self {
        Self {
            pcm,
            pending_ns,
            consumed_ns,
            same_thread_consumptions,
            origin,
            submit_thread,
            emitted: false,
        }
    }
}

impl Iterator for ProbeSilence {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.emitted {
            return None;
        }
        self.emitted = true;
        let start_ns = self.pending_ns.swap(0, Ordering::AcqRel);
        if start_ns != 0 {
            if thread::current().id() == self.submit_thread {
                self.same_thread_consumptions
                    .fetch_add(1, Ordering::Relaxed);
            } else {
                self.consumed_ns
                    .store(self.origin.elapsed().as_nanos() as u64, Ordering::Release);
            }
        }
        Some(self.pcm[0] as f32 / 32_768.0)
    }
}

impl Source for ProbeSilence {
    fn current_frame_len(&self) -> Option<usize> {
        Some(usize::from(!self.emitted))
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        48_000
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(1.0 / 48_000.0))
    }
}

struct WatchSilence {
    pcm: Arc<[i16]>,
}

impl WatchSilence {
    fn new(pcm: Arc<[i16]>) -> Self {
        Self { pcm }
    }
}

impl Iterator for WatchSilence {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.pcm[0] as f32 / 32_768.0)
    }
}

impl Source for WatchSilence {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        48_000
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

fn watch_default_changes(host: &cpal::Host, current: &cpal::Device, origin: Instant) -> ! {
    let mut previous = current.name().ok();
    println!("event=WatchStarted t_ms=0 stream=silent");
    loop {
        let name = host
            .default_output_device()
            .and_then(|device| device.name().ok());
        if name != previous {
            println!(
                "event=DefaultDeviceChanged t_ms={} name={}",
                origin.elapsed().as_millis(),
                name.as_deref().unwrap_or("none")
            );
            previous = name;
        }
        thread::sleep(Duration::from_millis(250));
    }
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
