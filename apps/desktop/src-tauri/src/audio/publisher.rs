#![allow(dead_code)] // Public host integration points are wired in the following slice.

use super::device::DeviceCmd;
use super::lanes::EngineOut;
use serde_json::Value;
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

pub trait Emit: Send + Sync {
    fn emit(&self, event: &str, payload: &Value);
}

pub const HISTORY_CAP: usize = 256;

const OUTCOME_KEYS: [&str; 8] = [
    "instance_id",
    "event_type",
    "session_epoch",
    "audio_class",
    "outcome",
    "playback_id",
    "package_revision",
    "decided_monotonic_ms",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PublisherStats {
    pub device_lost: u64,
    pub emit_panics: u64,
}

#[derive(Default)]
struct PublisherCounters {
    device_lost: AtomicU64,
    emit_panics: AtomicU64,
}

#[derive(Clone)]
pub struct HistoryHandle {
    records: Arc<Mutex<VecDeque<Value>>>,
    invalid_dropped: Arc<AtomicU64>,
    publisher_counters: Arc<PublisherCounters>,
}

impl HistoryHandle {
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(VecDeque::with_capacity(HISTORY_CAP))),
            invalid_dropped: Arc::new(AtomicU64::new(0)),
            publisher_counters: Arc::new(PublisherCounters::default()),
        }
    }

    pub fn push(&self, record: Value) {
        let valid = record.as_object().is_some_and(|object| {
            object.get("schema_version").and_then(Value::as_str) == Some("1.0.0")
                && OUTCOME_KEYS.iter().all(|key| object.contains_key(*key))
        });
        if !valid {
            increment(&self.invalid_dropped);
            return;
        }

        let mut records = lock_recover(&self.records);
        if records.len() == HISTORY_CAP {
            let _ = records.pop_front();
        }
        records.push_back(record);
    }

    pub fn snapshot(&self, limit: Option<u32>) -> Vec<Value> {
        let records = lock_recover(&self.records);
        let limit = limit.map_or(HISTORY_CAP, |value| (value as usize).min(HISTORY_CAP));
        let count = records.len().min(limit);
        records.iter().take(count).cloned().collect()
    }

    pub fn len(&self) -> usize {
        lock_recover(&self.records).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn invalid_dropped(&self) -> u64 {
        self.invalid_dropped.load(Ordering::Relaxed)
    }

    pub fn publisher_stats(&self) -> PublisherStats {
        PublisherStats {
            device_lost: self.publisher_counters.device_lost.load(Ordering::Relaxed),
            emit_panics: self.publisher_counters.emit_panics.load(Ordering::Relaxed),
        }
    }
}

impl Default for HistoryHandle {
    fn default() -> Self {
        Self::new()
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(1))
    });
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatencySample {
    pub instance_id: String,
    pub event_type: Option<String>,
    pub t0_ms: u64,
    pub t1_us: u64,
    pub t2_us: u64,
    pub t3p_us: u64,
}

pub fn spawn_publisher(
    rx: Receiver<EngineOut>,
    emitter: Arc<dyn Emit>,
    history: HistoryHandle,
    device: Option<Sender<DeviceCmd>>,
    latency: Option<Sender<LatencySample>>,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(String::from("audio-publisher"))
        .spawn(move || {
            while let Ok(output) = rx.recv() {
                match output {
                    EngineOut::Playback(payload) => {
                        // Bounded try_send only; the logging writer owns all
                        // scanning, locks, and file I/O. Never log the payload.
                        if payload["kind"] == "started" {
                            crate::logging::try_clip_played(
                                payload["event_type"].as_str().unwrap_or("unknown"),
                            );
                        }
                        if catch_unwind(AssertUnwindSafe(|| {
                            emitter.emit("playback://event", &payload)
                        }))
                        .is_err()
                        {
                            increment(&history.publisher_counters.emit_panics);
                        }
                    }
                    EngineOut::Outcome(record) => history.push(record),
                    EngineOut::DeviceLost => {
                        increment(&history.publisher_counters.device_lost);
                        crate::log_warn!(
                            "audio",
                            "device_lost",
                            "audio output device was lost",
                            code = "AUDIO_DEVICE_LOST"
                        );
                    }
                    EngineOut::NeedSink => {
                        if let Some(device) = &device {
                            let _ = device.send(DeviceCmd::Need);
                        }
                    }
                    EngineOut::ReleaseSink(sink) => {
                        let _ = catch_unwind(AssertUnwindSafe(|| drop(sink)));
                    }
                    EngineOut::ReleaseTable(table) => {
                        let _ = catch_unwind(AssertUnwindSafe(|| drop(table)));
                    }
                    EngineOut::Latency {
                        instance_id,
                        event_type,
                        t0_ms,
                        t1_us,
                        t2_us,
                        t3p_us,
                    } => {
                        if let Some(latency) = &latency {
                            let _ = latency.send(LatencySample {
                                instance_id,
                                event_type,
                                t0_ms,
                                t1_us,
                                t2_us,
                                t3p_us,
                            });
                        }
                    }
                }
            }
        })
}

#[allow(dead_code)] // Wired to the Tauri command in the following host integration slice.
pub fn audio_history_command(history: &HistoryHandle, limit: Option<u32>) -> Vec<Value> {
    if history.is_empty() {
        Vec::new()
    } else {
        history.snapshot(limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::device::DeviceCmd;
    use crate::audio::lanes::EngineOut;
    use crate::audio::resolve::PrewarmTable;
    use kohbor_audio::{pcm::Pcm, PlaybackId, SinkError};
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    fn record(id: usize) -> Value {
        json!({
            "schema_version": "1.0.0",
            "instance_id": format!("ev_g3_s{id}_0"),
            "event_type": "hero_kill",
            "session_epoch": 1,
            "audio_class": "ordinary",
            "outcome": "not_voiced",
            "playback_id": null,
            "package_revision": 1,
            "decided_monotonic_ms": id,
        })
    }

    fn publisher(
        rx: Receiver<EngineOut>,
        emitter: Arc<dyn Emit>,
        history: HistoryHandle,
    ) -> JoinHandle<()> {
        spawn_publisher(rx, emitter, history, None, None)
            .unwrap_or_else(|error| panic!("could not spawn publisher: {error}"))
    }

    fn join_with_deadline(worker: JoinHandle<()>) {
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = worker.join();
            let _ = done_tx.send(());
        });
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).is_ok());
    }

    struct RecordingEmitter(SyncSender<(String, Value)>);

    impl Emit for RecordingEmitter {
        fn emit(&self, event: &str, payload: &Value) {
            let _ = self.0.send((event.to_owned(), payload.clone()));
        }
    }

    fn recording_emitter() -> (Arc<dyn Emit>, Receiver<(String, Value)>) {
        let (tx, rx) = mpsc::sync_channel(16);
        (Arc::new(RecordingEmitter(tx)), rx)
    }

    #[test]
    fn playback_is_emitted_in_order_and_outcomes_only_go_to_history() {
        let (emitter, emitted) = recording_emitter();
        let history = HistoryHandle::new();
        let (tx, rx) = mpsc::sync_channel(8);
        let worker = publisher(rx, emitter, history.clone());

        for id in 0..3 {
            assert!(tx.send(EngineOut::Playback(json!({"id": id}))).is_ok());
        }
        assert!(tx.send(EngineOut::Outcome(record(10))).is_ok());
        assert!(tx.send(EngineOut::Outcome(record(11))).is_ok());
        drop(tx);
        join_with_deadline(worker);

        let delivered: Vec<_> = emitted.try_iter().collect();
        assert_eq!(
            delivered,
            [
                ("playback://event".to_owned(), json!({"id": 0})),
                ("playback://event".to_owned(), json!({"id": 1})),
                ("playback://event".to_owned(), json!({"id": 2})),
            ]
        );
        assert_eq!(
            history
                .snapshot(None)
                .iter()
                .map(|value| value["instance_id"].clone())
                .collect::<Vec<_>>(),
            [json!("ev_g3_s10_0"), json!("ev_g3_s11_0")]
        );
    }

    #[test]
    fn history_is_bounded_and_snapshots_are_oldest_first_with_limits() {
        let history = HistoryHandle::new();
        for id in 0..300 {
            history.push(record(id));
        }

        assert_eq!(history.len(), HISTORY_CAP);
        let all = history.snapshot(None);
        assert_eq!(all.len(), HISTORY_CAP);
        assert_eq!(
            all.first().map(|value| &value["instance_id"]),
            Some(&json!("ev_g3_s44_0"))
        );
        assert_eq!(
            all.last().map(|value| &value["instance_id"]),
            Some(&json!("ev_g3_s299_0"))
        );
        assert!(history.snapshot(Some(0)).is_empty());
        assert_eq!(history.snapshot(Some(1))[0]["instance_id"], "ev_g3_s44_0");
        let ten = history.snapshot(Some(10));
        assert_eq!(
            ten.first().map(|value| &value["instance_id"]),
            Some(&json!("ev_g3_s44_0"))
        );
        assert_eq!(
            ten.last().map(|value| &value["instance_id"]),
            Some(&json!("ev_g3_s53_0"))
        );
        assert_eq!(history.snapshot(Some(1000)), all);
        assert_eq!(audio_history_command(&history, Some(10)), ten);
        assert!(all.windows(2).all(|pair| {
            pair[0]["decided_monotonic_ms"].as_u64() < pair[1]["decided_monotonic_ms"].as_u64()
        }));
    }

    #[test]
    fn history_recovers_a_poisoned_mutex() {
        let history = HistoryHandle::new();
        let records = history.records.clone();
        let poisoner = thread::spawn(move || {
            let _guard = records
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            panic!("intentional poison for recovery test");
        });
        let (poisoned_tx, poisoned_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = poisoned_tx.send(poisoner.join().is_err());
        });
        assert_eq!(poisoned_rx.recv_timeout(Duration::from_secs(2)), Ok(true));

        history.push(record(1));
        assert_eq!(history.len(), 1);
        assert_eq!(history.snapshot(None), [record(1)]);
    }

    #[test]
    fn invalid_outcomes_are_counted_and_not_stored() {
        let history = HistoryHandle::new();
        history.push(Value::Null);
        history.push(json!({"schema_version": "1.0.0"}));
        let mut invalid_version = record(1);
        invalid_version["schema_version"] = json!("2.0.0");
        history.push(invalid_version);

        assert_eq!(history.invalid_dropped(), 3);
        assert!(history.is_empty());
    }

    struct BlockingEmitter {
        entered: SyncSender<()>,
        release: Mutex<Receiver<()>>,
        emitted: SyncSender<Value>,
    }

    impl Emit for BlockingEmitter {
        fn emit(&self, _event: &str, payload: &Value) {
            if payload["block"] == true {
                let _ = self.entered.send(());
                let released = self
                    .release
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .recv();
                assert!(released.is_ok());
            }
            let _ = self.emitted.send(payload.clone());
        }
    }

    #[test]
    fn blocked_emitter_does_not_block_a_try_send_producer() {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (emitted_tx, emitted_rx) = mpsc::sync_channel(8);
        let emitter = Arc::new(BlockingEmitter {
            entered: entered_tx,
            release: Mutex::new(release_rx),
            emitted: emitted_tx,
        });
        let history = HistoryHandle::new();
        let (tx, rx) = mpsc::sync_channel(4);
        let worker = publisher(rx, emitter, history);

        assert!(tx.send(EngineOut::Playback(json!({"block": true}))).is_ok());
        assert!(entered_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        for id in 0..4 {
            assert!(tx.try_send(EngineOut::Playback(json!({"id": id}))).is_ok());
        }
        assert!(matches!(
            tx.try_send(EngineOut::Playback(json!({"id": 4}))),
            Err(TrySendError::Full(_))
        ));

        assert!(release_tx.send(()).is_ok());
        assert!(emitted_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        assert!(emitted_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        assert!(tx.try_send(EngineOut::Playback(json!({"id": 4}))).is_ok());
        drop(tx);
        join_with_deadline(worker);
    }

    struct PanickingEmitter(Arc<AtomicUsize>);

    impl Emit for PanickingEmitter {
        fn emit(&self, _event: &str, _payload: &Value) {
            self.0.fetch_add(1, Ordering::Relaxed);
            panic!("intentional emitter panic");
        }
    }

    #[test]
    fn a_panicking_emitter_does_not_kill_the_thread_and_is_counted() {
        let calls = Arc::new(AtomicUsize::new(0));
        let history = HistoryHandle::new();
        let (tx, rx) = mpsc::sync_channel(2);
        let worker = publisher(
            rx,
            Arc::new(PanickingEmitter(calls.clone())),
            history.clone(),
        );
        assert!(tx.send(EngineOut::Playback(json!({"id": 1}))).is_ok());
        assert!(tx.send(EngineOut::Playback(json!({"id": 2}))).is_ok());
        drop(tx);
        join_with_deadline(worker);

        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert_eq!(history.publisher_stats().emit_panics, 2);
    }

    struct DropThreadSink(SyncSender<String>);

    impl Drop for DropThreadSink {
        fn drop(&mut self) {
            let name = thread::current().name().unwrap_or("unnamed").to_owned();
            let _ = self.0.send(name);
        }
    }

    impl kohbor_audio::Sink for DropThreadSink {
        fn submit(&mut self, _id: PlaybackId, _pcm: Pcm) -> Result<(), SinkError> {
            Ok(())
        }

        fn cancel(&mut self, _id: PlaybackId) {}
    }

    #[test]
    fn released_sink_and_table_are_dropped_by_the_publisher() {
        let (emitter, emitted) = recording_emitter();
        let (drop_tx, drop_rx) = mpsc::sync_channel(1);
        let history = HistoryHandle::new();
        let (tx, rx) = mpsc::sync_channel(3);
        let worker = publisher(rx, emitter, history);
        let table = PrewarmTable::empty();
        let weak_table = Arc::downgrade(&table);

        assert!(tx
            .send(EngineOut::ReleaseSink(Box::new(DropThreadSink(drop_tx))))
            .is_ok());
        assert!(tx.send(EngineOut::ReleaseTable(table)).is_ok());
        assert!(tx
            .send(EngineOut::Playback(json!({"after_release": true})))
            .is_ok());
        drop(tx);

        assert_eq!(
            drop_rx.recv_timeout(Duration::from_secs(2)).ok().as_deref(),
            Some("audio-publisher")
        );
        assert_eq!(
            emitted.recv_timeout(Duration::from_secs(2)).ok(),
            Some((
                "playback://event".to_owned(),
                json!({"after_release": true})
            ))
        );
        assert!(weak_table.upgrade().is_none());
        join_with_deadline(worker);
    }

    #[test]
    fn need_sink_and_latency_are_forwarded_and_device_loss_is_counted() {
        let (emitter, emitted) = recording_emitter();
        let history = HistoryHandle::new();
        let (device_tx, device_rx) = mpsc::channel();
        let (latency_tx, latency_rx) = mpsc::channel();
        let (tx, rx) = mpsc::sync_channel(4);
        let worker = spawn_publisher(
            rx,
            emitter,
            history.clone(),
            Some(device_tx),
            Some(latency_tx),
        )
        .unwrap_or_else(|error| panic!("could not spawn publisher: {error}"));

        assert!(tx.send(EngineOut::NeedSink).is_ok());
        assert!(tx.send(EngineOut::NeedSink).is_ok());
        assert!(tx.send(EngineOut::DeviceLost).is_ok());
        assert!(tx
            .send(EngineOut::Latency {
                instance_id: "ev_g3_s1_0".to_owned(),
                event_type: Some("hero_kill".to_owned()),
                t0_ms: 10,
                t1_us: 20,
                t2_us: 30,
                t3p_us: 40,
            })
            .is_ok());
        drop(tx);
        join_with_deadline(worker);

        assert_eq!(
            device_rx.try_iter().collect::<Vec<_>>(),
            [DeviceCmd::Need, DeviceCmd::Need]
        );
        assert!(emitted.try_recv().is_err());
        assert_eq!(
            latency_rx.try_recv(),
            Ok(LatencySample {
                instance_id: "ev_g3_s1_0".to_owned(),
                event_type: Some("hero_kill".to_owned()),
                t0_ms: 10,
                t1_us: 20,
                t2_us: 30,
                t3p_us: 40,
            })
        );
        assert_eq!(history.publisher_stats().device_lost, 1);
    }

    #[test]
    fn publisher_exits_when_the_output_channel_disconnects() {
        let (emitter, _emitted) = recording_emitter();
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = publisher(rx, emitter, HistoryHandle::new());
        drop(tx);
        join_with_deadline(worker);
    }
}
