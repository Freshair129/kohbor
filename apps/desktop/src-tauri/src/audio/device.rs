#![allow(dead_code)] // The host starts this supervisor in the following integration slice.

use super::lanes::{Control, EngineTx};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceCmd {
    Need,
    Shutdown,
}

pub fn backoff_ms(attempt: u32) -> u64 {
    match attempt {
        0 => 250,
        1 => 500,
        2 => 1_000,
        3 => 2_000,
        4 => 4_000,
        _ => 5_000,
    }
}

pub type Opener = Box<dyn FnMut() -> Result<Box<dyn kohbor_audio::Sink + Send>, ()> + Send>;
pub type Sleeper = Box<dyn FnMut(u64) + Send>;

/// Runs a user supplied sleeper without allowing its panic to unwind the supervisor.
/// A failed sleeper is terminal so retries cannot accidentally become a busy loop.
fn sleep_safely(sleeper: &mut Sleeper, milliseconds: u64) -> bool {
    catch_unwind(AssertUnwindSafe(|| sleeper(milliseconds))).is_ok()
}

/// Consume queued Need requests while an open or hand-over attempt is active.
/// Return true when the supervisor should stop.
fn stop_requested(rx: &Receiver<DeviceCmd>) -> bool {
    loop {
        match rx.try_recv() {
            Ok(DeviceCmd::Need) => {}
            Ok(DeviceCmd::Shutdown) | Err(TryRecvError::Disconnected) => return true,
            Err(TryRecvError::Empty) => return false,
        }
    }
}

pub fn spawn_device_supervisor(
    rx: Receiver<DeviceCmd>,
    engine: EngineTx,
    mut opener: Opener,
    mut sleeper: Sleeper,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(String::from("audio-device"))
        .spawn(move || {
            let mut attempt = 0_u32;
            'supervisor: while let Ok(DeviceCmd::Need) = rx.recv() {
                'attempts: loop {
                    if stop_requested(&rx) {
                        break 'supervisor;
                    }

                    let opened = catch_unwind(AssertUnwindSafe(&mut opener));
                    if stop_requested(&rx) {
                        drop(opened);
                        break 'supervisor;
                    }

                    let sink = match opened {
                        Ok(Ok(sink)) => sink,
                        Ok(Err(())) | Err(_) => {
                            let delay = backoff_ms(attempt);
                            if !sleep_safely(&mut sleeper, delay) {
                                break 'supervisor;
                            }
                            attempt = attempt.saturating_add(1);
                            if stop_requested(&rx) {
                                break 'supervisor;
                            }
                            continue 'attempts;
                        }
                    };

                    let mut sink = sink;
                    let mut retries = 0_u8;
                    let mut handed_over = false;
                    loop {
                        if stop_requested(&rx) {
                            drop(sink);
                            break 'supervisor;
                        }

                        match engine.control(Control::SinkReady(sink)) {
                            Ok(()) => {
                                handed_over = true;
                                break;
                            }
                            Err(Control::SinkReady(returned_sink)) => {
                                sink = returned_sink;
                                if retries == 20 {
                                    drop(sink);
                                    attempt = 0;
                                    break;
                                }
                                retries += 1;
                                if !sleep_safely(&mut sleeper, 250) {
                                    drop(sink);
                                    break 'supervisor;
                                }
                                if stop_requested(&rx) {
                                    drop(sink);
                                    break 'supervisor;
                                }
                            }
                            Err(other) => {
                                drop(other);
                                break 'supervisor;
                            }
                        }
                    }

                    if handed_over {
                        attempt = 0;
                        if stop_requested(&rx) {
                            break 'supervisor;
                        }
                        break 'attempts;
                    }
                }
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::lanes::{lanes, Control, EngineShared, CONTROL_LANE};
    use kohbor_audio::{pcm::Pcm, PlaybackId, SinkError};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    fn lanes_for_test() -> (crate::audio::lanes::EngineTx, Receiver<Control>) {
        let shared = Arc::new(EngineShared::new());
        let (engine, rx) = lanes(thread::current(), shared);
        (engine, rx.control)
    }

    fn join_with_deadline(worker: JoinHandle<()>) {
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = worker.join();
            let _ = done_tx.send(());
        });
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).is_ok());
    }

    fn sink() -> Box<dyn kohbor_audio::Sink + Send> {
        Box::new(TestSink)
    }

    struct TestSink;

    impl kohbor_audio::Sink for TestSink {
        fn submit(&mut self, _id: PlaybackId, _pcm: Pcm) -> Result<(), SinkError> {
            Ok(())
        }

        fn cancel(&mut self, _id: PlaybackId) {}
    }

    fn receive_sink(rx: &Receiver<Control>) {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Control::SinkReady(_)) => {}
            Ok(_) => panic!("unexpected control message"),
            Err(error) => panic!("sink was not handed over: {error}"),
        }
    }

    #[test]
    fn backoff_caps_at_five_seconds_without_overflow() {
        assert_eq!(
            (0..=6).map(backoff_ms).collect::<Vec<_>>(),
            [250, 500, 1_000, 2_000, 4_000, 5_000, 5_000]
        );
        assert_eq!(backoff_ms(u32::MAX), 5_000);
    }

    #[test]
    fn need_opens_once_and_a_later_need_opens_again() {
        let (engine, control_rx) = lanes_for_test();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let opens = Arc::new(AtomicUsize::new(0));
        let open_counter = opens.clone();
        let worker = spawn_device_supervisor(
            cmd_rx,
            engine,
            Box::new(move || {
                open_counter.fetch_add(1, Ordering::Relaxed);
                Ok(sink())
            }),
            Box::new(|_| {}),
        )
        .unwrap_or_else(|error| panic!("could not spawn device supervisor: {error}"));

        assert!(cmd_tx.send(DeviceCmd::Need).is_ok());
        receive_sink(&control_rx);
        assert_eq!(opens.load(Ordering::Relaxed), 1);
        assert!(cmd_tx.send(DeviceCmd::Need).is_ok());
        receive_sink(&control_rx);
        assert_eq!(opens.load(Ordering::Relaxed), 2);
        assert!(cmd_tx.send(DeviceCmd::Shutdown).is_ok());
        join_with_deadline(worker);
    }

    #[test]
    fn open_failures_follow_the_bounded_backoff_sequence() {
        let (engine, control_rx) = lanes_for_test();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let opens = Arc::new(AtomicUsize::new(0));
        let open_counter = opens.clone();
        let sleeps = Arc::new(Mutex::new(Vec::new()));
        let recorded_sleeps = sleeps.clone();
        let worker = spawn_device_supervisor(
            cmd_rx,
            engine,
            Box::new(move || {
                let attempt = open_counter.fetch_add(1, Ordering::Relaxed);
                if attempt < 7 {
                    Err(())
                } else {
                    Ok(sink())
                }
            }),
            Box::new(move |milliseconds| {
                recorded_sleeps
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(milliseconds);
            }),
        )
        .unwrap_or_else(|error| panic!("could not spawn device supervisor: {error}"));

        assert!(cmd_tx.send(DeviceCmd::Need).is_ok());
        receive_sink(&control_rx);
        assert_eq!(
            *sleeps
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            [250, 500, 1_000, 2_000, 4_000, 5_000, 5_000]
        );
        assert_eq!(opens.load(Ordering::Relaxed), 8);
        assert!(cmd_tx.send(DeviceCmd::Shutdown).is_ok());
        join_with_deadline(worker);
    }

    #[test]
    fn needs_arriving_during_a_retry_are_coalesced() {
        let (engine, control_rx) = lanes_for_test();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let retry_tx = cmd_tx.clone();
        let opens = Arc::new(AtomicUsize::new(0));
        let open_counter = opens.clone();
        let slept = Arc::new(AtomicUsize::new(0));
        let sleep_counter = slept.clone();
        let worker = spawn_device_supervisor(
            cmd_rx,
            engine,
            Box::new(move || {
                if open_counter.fetch_add(1, Ordering::Relaxed) == 0 {
                    Err(())
                } else {
                    Ok(sink())
                }
            }),
            Box::new(move |_| {
                sleep_counter.fetch_add(1, Ordering::Relaxed);
                for _ in 0..5 {
                    let _ = retry_tx.send(DeviceCmd::Need);
                }
            }),
        )
        .unwrap_or_else(|error| panic!("could not spawn device supervisor: {error}"));

        assert!(cmd_tx.send(DeviceCmd::Need).is_ok());
        receive_sink(&control_rx);
        assert_eq!(opens.load(Ordering::Relaxed), 2);
        assert_eq!(slept.load(Ordering::Relaxed), 1);
        assert!(cmd_tx.send(DeviceCmd::Shutdown).is_ok());
        join_with_deadline(worker);
    }

    #[test]
    fn full_control_lane_retries_then_drops_the_sink_and_starts_a_new_series() {
        let (engine, control_rx) = lanes_for_test();
        for epoch in 0..CONTROL_LANE {
            assert!(engine
                .control(Control::Session {
                    epoch: epoch as u64
                })
                .is_ok());
        }

        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (drop_tx, drop_rx) = mpsc::channel();
        let (second_open_tx, second_open_rx) = mpsc::channel();
        let (allow_second_tx, allow_second_rx) = mpsc::channel();
        let opens = Arc::new(AtomicUsize::new(0));
        let open_counter = opens.clone();
        let sleeps = Arc::new(Mutex::new(Vec::new()));
        let recorded_sleeps = sleeps.clone();
        let worker = spawn_device_supervisor(
            cmd_rx,
            engine,
            Box::new(move || {
                let open = open_counter.fetch_add(1, Ordering::Relaxed);
                if open == 0 {
                    Ok(Box::new(DropSink {
                        id: 1,
                        dropped: drop_tx.clone(),
                    }) as Box<dyn kohbor_audio::Sink + Send>)
                } else {
                    let _ = second_open_tx.send(());
                    let _ = allow_second_rx.recv();
                    Ok(Box::new(DropSink {
                        id: 2,
                        dropped: drop_tx.clone(),
                    }) as Box<dyn kohbor_audio::Sink + Send>)
                }
            }),
            Box::new(move |milliseconds| {
                recorded_sleeps
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(milliseconds);
            }),
        )
        .unwrap_or_else(|error| panic!("could not spawn device supervisor: {error}"));

        assert!(cmd_tx.send(DeviceCmd::Need).is_ok());
        assert_eq!(
            drop_rx
                .recv_timeout(Duration::from_secs(2))
                .ok()
                .map(|item| item.0),
            Some(1)
        );
        assert!(second_open_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        assert_eq!(opens.load(Ordering::Relaxed), 2);
        assert_eq!(
            *sleeps
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            vec![250; 20]
        );
        while control_rx.try_recv().is_ok() {}
        assert!(allow_second_tx.send(()).is_ok());
        receive_sink(&control_rx);
        assert!(cmd_tx.send(DeviceCmd::Shutdown).is_ok());
        join_with_deadline(worker);
    }

    struct DropSink {
        id: u32,
        dropped: Sender<(u32, String)>,
    }

    impl Drop for DropSink {
        fn drop(&mut self) {
            let thread_name = thread::current().name().unwrap_or("unnamed").to_owned();
            let _ = self.dropped.send((self.id, thread_name));
        }
    }

    impl kohbor_audio::Sink for DropSink {
        fn submit(&mut self, _id: PlaybackId, _pcm: Pcm) -> Result<(), SinkError> {
            Ok(())
        }

        fn cancel(&mut self, _id: PlaybackId) {}
    }

    #[test]
    fn shutdown_stops_during_an_open_retry() {
        let (engine, _control_rx) = lanes_for_test();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let retry_tx = cmd_tx.clone();
        let opens = Arc::new(AtomicUsize::new(0));
        let open_counter = opens.clone();
        let sleeps = Arc::new(Mutex::new(Vec::new()));
        let recorded_sleeps = sleeps.clone();
        let worker = spawn_device_supervisor(
            cmd_rx,
            engine,
            Box::new(move || {
                open_counter.fetch_add(1, Ordering::Relaxed);
                Err(())
            }),
            Box::new(move |milliseconds| {
                recorded_sleeps
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(milliseconds);
                let _ = retry_tx.send(DeviceCmd::Shutdown);
            }),
        )
        .unwrap_or_else(|error| panic!("could not spawn device supervisor: {error}"));

        assert!(cmd_tx.send(DeviceCmd::Need).is_ok());
        join_with_deadline(worker);
        assert_eq!(opens.load(Ordering::Relaxed), 1);
        assert_eq!(
            *sleeps
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            [250]
        );
    }

    #[test]
    fn a_panicking_opener_is_treated_as_a_failed_attempt() {
        let (engine, control_rx) = lanes_for_test();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let opens = Arc::new(AtomicUsize::new(0));
        let open_counter = opens.clone();
        let sleeps = Arc::new(Mutex::new(Vec::new()));
        let recorded_sleeps = sleeps.clone();
        let worker = spawn_device_supervisor(
            cmd_rx,
            engine,
            Box::new(move || {
                if open_counter.fetch_add(1, Ordering::Relaxed) == 0 {
                    panic!("intentional opener panic");
                }
                Ok(sink())
            }),
            Box::new(move |milliseconds| {
                recorded_sleeps
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(milliseconds);
            }),
        )
        .unwrap_or_else(|error| panic!("could not spawn device supervisor: {error}"));

        assert!(cmd_tx.send(DeviceCmd::Need).is_ok());
        receive_sink(&control_rx);
        assert_eq!(opens.load(Ordering::Relaxed), 2);
        assert_eq!(
            *sleeps
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            [250]
        );
        assert!(cmd_tx.send(DeviceCmd::Shutdown).is_ok());
        join_with_deadline(worker);
    }

    #[test]
    fn a_disconnected_command_channel_ends_the_supervisor() {
        let (engine, _control_rx) = lanes_for_test();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let opens = Arc::new(AtomicUsize::new(0));
        let open_counter = opens.clone();
        let worker = spawn_device_supervisor(
            cmd_rx,
            engine,
            Box::new(move || {
                open_counter.fetch_add(1, Ordering::Relaxed);
                Ok(sink())
            }),
            Box::new(|_| {}),
        )
        .unwrap_or_else(|error| panic!("could not spawn device supervisor: {error}"));
        drop(cmd_tx);
        join_with_deadline(worker);
        assert_eq!(opens.load(Ordering::Relaxed), 0);
    }
}
