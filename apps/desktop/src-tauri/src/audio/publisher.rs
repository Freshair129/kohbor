#![allow(dead_code)] // Public host integration points are wired in the following slice.

use super::device::DeviceCmd;
use super::lanes::{ClipVariantLog, EngineOut};
use super::outcome::{is_valid_record, PlaybackEnd};
use super::recap::{MatchRecapStore, MATCH_RECAP_SCHEMA_VERSION};
use kohbor_events::EventId;
use serde_json::{json, Map, Value};
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
    recap: Option<MatchRecapStore>,
}

impl HistoryHandle {
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(VecDeque::with_capacity(HISTORY_CAP))),
            invalid_dropped: Arc::new(AtomicU64::new(0)),
            publisher_counters: Arc::new(PublisherCounters::default()),
            recap: None,
        }
    }

    pub fn with_match_recap(path: impl Into<std::path::PathBuf>) -> Self {
        let mut history = Self::new();
        history.recap = Some(MatchRecapStore::open(path));
        history
    }

    pub fn push(&self, record: Value) {
        if !is_valid_record(&record) {
            increment(&self.invalid_dropped);
            return;
        }

        let recap_record = record.clone();
        let mut records = lock_recover(&self.records);
        if records.len() == HISTORY_CAP {
            let _ = records.pop_front();
        }
        records.push_back(record);
        drop(records);
        if let Some(recap) = &self.recap {
            recap.record(&recap_record);
        }
    }

    pub fn finish_playback(
        &self,
        playback_id: kohbor_audio::PlaybackId,
        instance_id: &str,
        end: PlaybackEnd,
    ) {
        let contract_id = playback_id.to_contract_id();
        {
            let mut records = lock_recover(&self.records);
            for record in records.iter_mut() {
                if record.get("playback_id").and_then(Value::as_str) == Some(&contract_id)
                    && record.get("instance_id").and_then(Value::as_str) == Some(instance_id)
                {
                    if let Some(object) = record.as_object_mut() {
                        object.insert("playback_end".to_owned(), json!(end.as_str()));
                    }
                }
            }
        }
        if let Some(recap) = &self.recap {
            recap.finish_playback(&contract_id, instance_id, end);
        }
    }

    pub fn match_recap(&self) -> Value {
        self.recap.as_ref().map_or_else(
            || json!({"schema_version": MATCH_RECAP_SCHEMA_VERSION, "matches": []}),
            MatchRecapStore::snapshot,
        )
    }

    pub fn clear_match_recap(&self) -> Result<(), String> {
        self.recap.as_ref().map_or(Ok(()), MatchRecapStore::clear)
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
                        if catch_unwind(AssertUnwindSafe(|| {
                            emitter.emit("playback://event", &payload)
                        }))
                        .is_err()
                        {
                            increment(&history.publisher_counters.emit_panics);
                        }
                    }
                    EngineOut::PlaybackEnd {
                        playback_id,
                        instance_id,
                        end,
                    } => {
                        history.finish_playback(playback_id, &instance_id, end);
                    }
                    EngineOut::ClipPlayed { event_id, variant } => {
                        log_clip_played(&event_id, &variant);
                    }
                    EngineOut::ClipVariantInvalid {
                        event_id,
                        tier,
                        variant_id,
                        clip_id,
                        reason,
                    } => {
                        log_invalid_variant(event_id, tier, &variant_id, clip_id.as_deref(), reason)
                    }
                    EngineOut::ClipQueued {
                        event_id,
                        tier,
                        variant,
                    } => {
                        log_clip_queued(&event_id, tier, &variant);
                    }
                    EngineOut::ClipDropped {
                        event_id,
                        tier,
                        outcome,
                        variant,
                    } => log_clip_dropped(&event_id, tier, outcome.as_str(), variant.as_ref()),
                    EngineOut::ClipFinished {
                        event_id,
                        reason,
                        expected_frames,
                        native_snapshot,
                        render_thread_priority,
                    } => crate::logging::try_clip_finished(
                        &event_id,
                        &reason,
                        expected_frames,
                        native_snapshot.as_deref(),
                        render_thread_priority,
                    ),
                    EngineOut::Outcome(record) => {
                        if record["event_type"].as_str() == Some("audition") {
                            crate::log_info!(
                                "media",
                                "audition",
                                "pack clip audition outcome",
                                outcome = record["outcome"].as_str().unwrap_or("unknown")
                            );
                        }
                        history.push(record);
                    }
                    EngineOut::AuditionMatchChanged(active) => {
                        if catch_unwind(AssertUnwindSafe(|| {
                            emitter.emit("media://audition-match", &Value::Bool(active))
                        }))
                        .is_err()
                        {
                            increment(&history.publisher_counters.emit_panics);
                        }
                    }
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

fn log_invalid_variant(
    event_id: EventId,
    tier: Option<u8>,
    variant_id: &str,
    clip_id: Option<&str>,
    reason: &'static str,
) {
    let mut fields = Map::from_iter([
        ("event_id".to_owned(), Value::from(event_id.as_str())),
        ("tier".to_owned(), tier.map_or(Value::Null, Value::from)),
        ("variant_id".to_owned(), Value::from(variant_id)),
        ("reason".to_owned(), Value::from(reason)),
    ]);
    if let Some(clip_id) = clip_id {
        fields.insert("clip_id".to_owned(), Value::from(clip_id));
    }
    crate::logging::emit(
        crate::logging::Level::Warn,
        "audio",
        "clip_variant_invalid",
        "announcer variant skipped",
        fields,
    );
}

fn clip_fields(
    event_id: &str,
    tier: Option<u8>,
    outcome: Option<&str>,
    variant: Option<&ClipVariantLog>,
    has_visual: Option<bool>,
) -> Map<String, Value> {
    let mut fields = Map::from_iter([(
        "event_id".to_owned(),
        Value::String(event_id.chars().take(80).collect()),
    )]);
    if let Some(tier) = tier {
        fields.insert("tier".to_owned(), Value::from(tier));
    } else if has_visual.is_none() {
        fields.insert("tier".to_owned(), Value::Null);
    }
    if let Some(outcome) = outcome {
        fields.insert(
            "outcome".to_owned(),
            Value::String(outcome.chars().take(80).collect()),
        );
    }
    if let Some(variant) = variant {
        fields.insert(
            "variant_id".to_owned(),
            Value::String(variant.variant_id.chars().take(32).collect()),
        );
        if let Some(clip_id) = variant.clip_id.as_deref() {
            fields.insert(
                "clip_id".to_owned(),
                Value::String(clip_id.chars().take(48).collect()),
            );
        }
        fields.insert(
            "selection".to_owned(),
            variant
                .selection
                .as_ref()
                .map(|selection| Value::String(selection.chars().take(16).collect()))
                .unwrap_or(Value::Null),
        );
    }
    if let Some(has_visual) = has_visual {
        fields.insert("has_visual".to_owned(), Value::Bool(has_visual));
    }
    fields
}

fn log_clip_played(event_id: &str, variant: &ClipVariantLog) {
    if event_id == "audition" {
        return;
    }
    crate::logging::emit(
        crate::logging::Level::Info,
        "audio",
        "clip_played",
        "announcer clip started",
        clip_fields(
            event_id,
            None,
            Some("admitted"),
            Some(variant),
            Some(variant.has_visual),
        ),
    );
}

fn log_clip_queued(event_id: &str, tier: Option<u8>, variant: &ClipVariantLog) {
    if event_id == "audition" {
        return;
    }
    crate::logging::emit(
        crate::logging::Level::Info,
        "audio",
        "clip_queued",
        "announcer clip queued",
        clip_fields(event_id, tier, None, Some(variant), None),
    );
}

fn log_clip_dropped(
    event_id: &str,
    tier: Option<u8>,
    outcome: &str,
    variant: Option<&ClipVariantLog>,
) {
    if event_id == "audition" {
        return;
    }
    crate::logging::emit(
        crate::logging::Level::Info,
        "audio",
        "clip_dropped",
        "announcer clip dropped",
        clip_fields(event_id, tier, Some(outcome), variant, None),
    );
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
            "schema_version": "1.1.0",
            "instance_id": format!("ev_g3_s{id}_0"),
            "event_type": "hero_kill",
            "session_epoch": 1,
            "audio_class": "ordinary",
            "outcome": "not_voiced",
            "playback_id": null,
            "package_revision": 1,
            "decided_monotonic_ms": id,
            "tier": null,
            "variant_id": null,
            "clip_id": null,
            "selection": null,
            "game_clock_seconds": null,
            "match_id": "local_match_1",
            "playback_end": null,
        })
    }

    #[test]
    fn invalid_variant_log_contains_only_binding_provenance_and_reason() {
        let entries = crate::logging::collect_test_logs(|| {
            super::log_invalid_variant(
                EventId::Streak,
                Some(3),
                "broken",
                Some("vrzo-th-0019"),
                "digest_mismatch",
            );
            super::log_invalid_variant(EventId::Streak, None, "broken", None, "digest_mismatch");
        });
        assert_eq!(entries.len(), 2);
        assert!(entries
            .iter()
            .all(|entry| entry.module == "audio" && entry.event == "clip_variant_invalid"));
        assert_eq!(
            entries[0].fields,
            json!({
                "event_id": "streak",
                "tier": 3,
                "variant_id": "broken",
                "clip_id": "vrzo-th-0019",
                "reason": "digest_mismatch"
            })
        );
        assert_eq!(
            entries[1].fields,
            json!({
                "event_id": "streak", "tier": null, "variant_id": "broken", "reason": "digest_mismatch"
            })
        );
    }

    #[test]
    fn clip_logs_keep_variant_provenance_and_visual_presence() {
        let variant = ClipVariantLog {
            variant_id: "kill-02".to_owned(),
            clip_id: Some("vrzo-th-0019".to_owned()),
            selection: Some("shuffle".to_owned()),
            has_visual: false,
        };
        let played = super::clip_fields(
            "hero_kill",
            None,
            Some("admitted"),
            Some(&variant),
            Some(variant.has_visual),
        );
        assert_eq!(played["variant_id"], "kill-02");
        assert_eq!(played["clip_id"], "vrzo-th-0019");
        assert_eq!(played["selection"], "shuffle");
        assert_eq!(played["has_visual"], false);

        let mut with_visual = variant.clone();
        with_visual.has_visual = true;
        let visually_played = super::clip_fields(
            "hero_kill",
            None,
            Some("admitted"),
            Some(&with_visual),
            Some(with_visual.has_visual),
        );
        assert_eq!(visually_played["has_visual"], true);

        let queued = super::clip_fields("hero_kill", Some(2), None, Some(&variant), None);
        assert_eq!(queued["variant_id"], "kill-02");
        assert_eq!(queued["clip_id"], "vrzo-th-0019");
        assert_eq!(queued["selection"], "shuffle");

        let dropped = super::clip_fields(
            "hero_kill",
            Some(2),
            Some("channel_busy"),
            Some(&variant),
            None,
        );
        assert_eq!(dropped["variant_id"], "kill-02");
        assert_eq!(dropped["selection"], "shuffle");

        let tierless_dropped =
            super::clip_fields("hero_kill", None, Some("channel_busy"), None, None);
        assert_eq!(tierless_dropped["tier"], Value::Null);
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
