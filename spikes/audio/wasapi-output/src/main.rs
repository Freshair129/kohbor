use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use wasapi::{
    AudioClient, BufferFlags, DeviceEnumerator, DeviceEventCallbacks, Direction, SampleType,
    StreamMode, WaveFormat, initialize_mta,
};

const ITERATIONS: usize = 200;
const FORMAT_RATE: usize = 48_000;

fn main() -> Result<(), Box<dyn Error>> {
    let watch = std::env::args().any(|arg| arg == "--watch");
    initialize_mta().ok()?;
    let origin = Instant::now();
    let enumerator = DeviceEnumerator::new()?;
    let device = enumerator.get_default_device(&Direction::Render)?;
    let device_name = device.get_friendlyname()?;
    let device_id = device.get_id()?;
    let mut notifications = DeviceEventCallbacks::new();
    notifications.set_device_added_callback({
        move |id| {
            println!(
                "event=DeviceAdded t_ms={} id={id}",
                origin.elapsed().as_millis()
            )
        }
    });
    notifications.set_device_removed_callback({
        move |id| {
            println!(
                "event=DeviceRemoved t_ms={} id={id}",
                origin.elapsed().as_millis()
            )
        }
    });
    notifications.set_device_state_callback({
        move |id, state| {
            println!(
                "event=DeviceStateChanged t_ms={} id={id} state={state:?}",
                origin.elapsed().as_millis()
            )
        }
    });
    notifications.set_default_device_callback({
        move |direction, role, id| {
            println!(
                "event=DefaultDeviceChanged t_ms={} direction={direction:?} role={role:?} id={}",
                origin.elapsed().as_millis(),
                id.as_deref().unwrap_or("none")
            )
        }
    });
    let _registration = enumerator.register_notification_callback(notifications)?;

    let mut client = device.get_iaudioclient()?;
    let mix = client.get_mixformat()?;
    let (default_period_hns, minimum_period_hns) = client.get_device_period()?;
    let desired = WaveFormat::new(16, 16, &SampleType::Int, FORMAT_RATE, 1, Some(0));
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: default_period_hns,
    };
    let requested_format = match client.initialize_client(&desired, &Direction::Render, &mode) {
        Ok(()) => {
            println!("requested_pcm=mono/48000/pcm16 initialize=accepted autoconvert=true");
            desired
        }
        Err(error) => {
            println!("requested_pcm=mono/48000/pcm16 initialize=not_accepted detail={error}");
            client = device.get_iaudioclient()?;
            let fallback_mode = StreamMode::EventsShared {
                autoconvert: false,
                buffer_duration_hns: default_period_hns,
            };
            client.initialize_client(&mix, &Direction::Render, &fallback_mode)?;
            mix.clone()
        }
    };

    let event = client.set_get_eventhandle()?;
    let renderer = client.get_audiorenderclient()?;
    let buffer_frames = client.get_buffer_size()?;
    let stream_latency_hns = unsafe { client.as_iaudioclient().GetStreamLatency()? };
    let block_align = requested_format.get_blockalign() as usize;
    let silence = vec![0_u8; buffer_frames as usize * block_align];
    let initial_frames = client.get_available_space_in_frames()? as usize;
    renderer.write_to_device(
        initial_frames,
        &silence[..initial_frames * block_align],
        Some(BufferFlags {
            data_discontinuity: false,
            silent: true,
            timestamp_error: false,
        }),
    )?;
    client.start_stream()?;

    println!(
        "api=wasapi version=0.26.0 device_name={device_name} device_id={device_id} stream=shared event_driven=true stream_config={}Hz/{}ch/{}bit {:?} buffer_frames={buffer_frames} buffer_ms={:.3} device_period_ms={:.3} min_period_ms={:.3} stream_latency_ms={:.3}",
        requested_format.get_samplespersec(),
        requested_format.get_nchannels(),
        requested_format.get_bitspersample(),
        requested_format.get_subformat()?,
        frames_ms(buffer_frames, requested_format.get_samplespersec()),
        hns_ms(default_period_hns),
        hns_ms(minimum_period_hns),
        hns_ms(stream_latency_hns)
    );
    println!(
        "mix_format={}Hz/{}ch/{}bit {:?}",
        mix.get_samplespersec(),
        mix.get_nchannels(),
        mix.get_bitspersample(),
        mix.get_subformat()?
    );

    if watch {
        println!("event=WatchStarted t_ms=0 stream=silent; Ctrl+C ends watch");
        let mut stream_valid = true;
        loop {
            if !stream_valid {
                thread::sleep(Duration::from_millis(250));
                continue;
            }
            if let Err(error) = event.wait_for_event(250) {
                println!(
                    "event=StreamInvalidated t_ms={} detail={error}",
                    origin.elapsed().as_millis()
                );
                stream_valid = false;
                continue;
            }
            if let Err(error) = service_silence(&client, &renderer, &silence, block_align) {
                println!(
                    "event=StreamInvalidated t_ms={} detail={error}",
                    origin.elapsed().as_millis()
                );
                stream_valid = false;
            }
        }
    }

    // Leave the WASAPI owner on this thread; a second thread submits an atomic
    // marker while the owner waits for the next render event.
    event.wait_for_event(2_000)?;
    service_silence(&client, &renderer, &silence, block_align)?;
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
        event.wait_for_event(2_000)?;
        service_silence(&client, &renderer, &silence, block_align)?;
        let started_ns = submit_ns.swap(0, Ordering::AcqRel);
        submit_ms.push((origin.elapsed().as_nanos() as u64 - started_ns) as f64 / 1_000_000.0);
    }
    producer.join().map_err(|_| "submit producer panicked")?;
    print_stats("submit_to_event_render_ms", &submit_ms);

    let mut stop_start_ms = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        client.stop_stream()?;
        client.start_stream()?;
        stop_start_ms.push(start.elapsed().as_secs_f64() * 1_000.0);
    }
    print_stats("stop_start_call_pair_ms", &stop_start_ms);
    Ok(())
}

fn service_silence(
    client: &AudioClient,
    renderer: &wasapi::AudioRenderClient,
    silence: &[u8],
    block_align: usize,
) -> Result<(), Box<dyn Error>> {
    let available = client.get_available_space_in_frames()? as usize;
    if available == 0 {
        return Ok(());
    }
    renderer.write_to_device(
        available,
        &silence[..available * block_align],
        Some(BufferFlags {
            data_discontinuity: false,
            silent: true,
            timestamp_error: false,
        }),
    )?;
    Ok(())
}

fn frames_ms(frames: u32, sample_rate: u32) -> f64 {
    frames as f64 * 1_000.0 / sample_rate as f64
}

fn hns_ms(value: i64) -> f64 {
    value as f64 / 10_000.0
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
