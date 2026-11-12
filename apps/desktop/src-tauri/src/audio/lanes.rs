#![allow(dead_code)]

use kohbor_events::Event;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::Thread;
use std::time::Instant;

pub const CRITICAL_LANE: usize = 8;
pub const ORDINARY_LANE: usize = 64;
pub const CONTROL_LANE: usize = 32;
pub const OUT_LANE: usize = 256;

#[derive(Debug)]
pub struct TickMsg {
    pub arrival: Instant,
    pub detected: Instant,
    pub at_ms: u64,
    pub session_epoch: u64,
    pub generation: u64,
    pub sequence: u64,
    pub events: Vec<Event>,
}

pub enum Control {
    Session {
        epoch: u64,
    },
    SetMuted {
        muted: bool,
        ack: SyncSender<bool>,
    },
    SwapTable {
        table: Arc<super::resolve::PrewarmTable>,
    },
    SinkReady(Box<dyn kohbor_audio::Sink + Send>),
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    Sent,
    DroppedOrdinary,
    CriticalOverflow,
}

#[derive(Clone)]
pub struct EngineTx {
    critical: SyncSender<TickMsg>,
    ordinary: SyncSender<TickMsg>,
    control: SyncSender<Control>,
    thread: Thread,
    shared: Arc<EngineShared>,
}

pub struct EngineRx {
    pub(crate) critical: Receiver<TickMsg>,
    pub(crate) ordinary: Receiver<TickMsg>,
    pub(crate) control: Receiver<Control>,
}

pub struct EngineShared {
    pub heartbeat_ms: AtomicU64,
    pub muted: AtomicBool,
    pub device_available: AtomicBool,
    pub lease_active: AtomicBool,
    pub fence: AtomicU64,
    pub dropped_ordinary: AtomicU64,
    pub critical_overflow: AtomicU64,
    pub out_dropped: AtomicU64,
    pub rebuilds: AtomicU32,
    pub degraded: AtomicBool,
}

impl EngineShared {
    pub fn new() -> Self {
        Self {
            heartbeat_ms: AtomicU64::new(0),
            muted: AtomicBool::new(false),
            device_available: AtomicBool::new(false),
            lease_active: AtomicBool::new(false),
            fence: AtomicU64::new(0),
            dropped_ordinary: AtomicU64::new(0),
            critical_overflow: AtomicU64::new(0),
            out_dropped: AtomicU64::new(0),
            rebuilds: AtomicU32::new(0),
            degraded: AtomicBool::new(false),
        }
    }
}

pub enum EngineOut {
    Playback(serde_json::Value),
    Outcome(serde_json::Value),
    DeviceLost,
    /// The engine has no usable sink (startup, device loss, rebuild after a panic):
    /// the audio-device supervisor should open one and answer `Control::SinkReady`.
    NeedSink,
    /// A sink the engine no longer uses, handed over so it is dropped (and its render
    /// thread joined) off the engine thread.
    ReleaseSink(Box<dyn kohbor_audio::Sink + Send>),
    /// The previous prewarm table, handed over so the large free happens off the
    /// engine thread.
    ReleaseTable(Arc<super::resolve::PrewarmTable>),
    Latency {
        instance_id: String,
        event_type: Option<String>,
        t0_ms: u64,
        t1_us: u64,
        t2_us: u64,
        t3p_us: u64,
    },
}

impl EngineTx {
    pub fn offer_tick(&self, msg: TickMsg) -> Offer {
        let critical = msg.events.iter().any(|event| event.id.is_critical());
        let result = if critical {
            self.critical.try_send(msg)
        } else {
            self.ordinary.try_send(msg)
        };

        match result {
            Ok(()) => {
                self.thread.unpark();
                Offer::Sent
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) if critical => {
                self.shared
                    .critical_overflow
                    .fetch_add(1, Ordering::Relaxed);
                Offer::CriticalOverflow
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.shared.dropped_ordinary.fetch_add(1, Ordering::Relaxed);
                Offer::DroppedOrdinary
            }
        }
    }

    pub fn control(&self, message: Control) -> Result<(), Control> {
        match self.control.try_send(message) {
            Ok(()) => {
                self.thread.unpark();
                Ok(())
            }
            Err(TrySendError::Full(message)) | Err(TrySendError::Disconnected(message)) => {
                Err(message)
            }
        }
    }
}

pub fn lanes(thread: Thread, shared: Arc<EngineShared>) -> (EngineTx, EngineRx) {
    let (critical_tx, critical_rx) = mpsc::sync_channel(CRITICAL_LANE);
    let (ordinary_tx, ordinary_rx) = mpsc::sync_channel(ORDINARY_LANE);
    let (control_tx, control_rx) = mpsc::sync_channel(CONTROL_LANE);

    (
        EngineTx {
            critical: critical_tx,
            ordinary: ordinary_tx,
            control: control_tx,
            thread,
            shared,
        },
        EngineRx {
            critical: critical_rx,
            ordinary: ordinary_rx,
            control: control_rx,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{
        lanes, Control, EngineOut, EngineShared, Offer, TickMsg, CONTROL_LANE, CRITICAL_LANE,
        ORDINARY_LANE,
    };
    use kohbor_events::{Event, EventId};
    use std::sync::atomic::Ordering;
    use std::sync::{mpsc, Arc};
    use std::thread;
    use std::time::{Duration, Instant};

    fn event(id: EventId, at_ms: u64) -> Event {
        Event {
            id,
            tier: None,
            at_ms,
        }
    }

    fn tick(sequence: u64, events: Vec<Event>) -> TickMsg {
        let now = Instant::now();
        TickMsg {
            arrival: now,
            detected: now,
            at_ms: sequence,
            session_epoch: 1,
            generation: 1,
            sequence,
            events,
        }
    }

    fn lanes_for_current_thread() -> (super::EngineTx, super::EngineRx, Arc<EngineShared>) {
        let shared = Arc::new(EngineShared::new());
        let (tx, rx) = lanes(thread::current(), shared.clone());
        (tx, rx, shared)
    }

    #[test]
    fn ordinary_overflow_drops_the_newest_tick_and_counts_it() {
        let (tx, rx, shared) = lanes_for_current_thread();
        for sequence in 0..ORDINARY_LANE as u64 {
            assert_eq!(tx.offer_tick(tick(sequence, vec![])), Offer::Sent);
        }

        assert_eq!(
            tx.offer_tick(tick(ORDINARY_LANE as u64, vec![])),
            Offer::DroppedOrdinary
        );
        assert_eq!(shared.dropped_ordinary.load(Ordering::Relaxed), 1);

        let queued: Vec<_> = rx.ordinary.try_iter().map(|msg| msg.sequence).collect();
        assert_eq!(queued, (0..ORDINARY_LANE as u64).collect::<Vec<_>>());
    }

    #[test]
    fn critical_tick_is_delivered_when_the_ordinary_lane_is_full() {
        let (tx, rx, _) = lanes_for_current_thread();
        for sequence in 0..ORDINARY_LANE as u64 {
            assert_eq!(tx.offer_tick(tick(sequence, vec![])), Offer::Sent);
        }

        assert_eq!(
            tx.offer_tick(tick(900, vec![event(EventId::HealthDanger, 900)],)),
            Offer::Sent
        );
        let critical = rx.critical.try_recv().unwrap();
        assert_eq!(critical.sequence, 900);
        assert_eq!(critical.events[0].id, EventId::HealthDanger);
        assert_eq!(rx.ordinary.try_iter().count(), ORDINARY_LANE);
    }

    #[test]
    fn mixed_critical_tick_travels_whole_and_keeps_event_order() {
        let (tx, rx, _) = lanes_for_current_thread();
        let ordered = vec![
            event(EventId::HeroKill, 11),
            event(EventId::HealthDanger, 11),
            event(EventId::MultiKill, 11),
        ];

        assert_eq!(tx.offer_tick(tick(11, ordered)), Offer::Sent);
        let delivered = rx.critical.try_recv().unwrap();
        assert_eq!(
            delivered
                .events
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            [EventId::HeroKill, EventId::HealthDanger, EventId::MultiKill]
        );
        assert!(rx.ordinary.try_recv().is_err());
    }

    #[test]
    fn critical_overflow_is_counted_and_reported() {
        let (tx, rx, shared) = lanes_for_current_thread();
        for sequence in 0..CRITICAL_LANE as u64 {
            assert_eq!(
                tx.offer_tick(tick(sequence, vec![event(EventId::HealthDanger, sequence)],)),
                Offer::Sent
            );
        }

        assert_eq!(
            tx.offer_tick(tick(
                CRITICAL_LANE as u64,
                vec![event(EventId::HealthDanger, CRITICAL_LANE as u64)],
            )),
            Offer::CriticalOverflow
        );
        assert_eq!(shared.critical_overflow.load(Ordering::Relaxed), 1);
        assert_eq!(rx.critical.try_iter().count(), CRITICAL_LANE);
    }

    #[test]
    fn full_control_lane_returns_the_original_message() {
        let (tx, _rx, _) = lanes_for_current_thread();
        for _ in 0..CONTROL_LANE {
            let (ack, _ack_rx) = mpsc::sync_channel(1);
            assert!(tx.control(Control::SetMuted { muted: false, ack }).is_ok());
        }

        let (ack, ack_rx) = mpsc::sync_channel(1);
        let returned = match tx.control(Control::SetMuted { muted: true, ack }) {
            Err(message) => message,
            Ok(()) => panic!("full control lane accepted another message"),
        };
        match returned {
            Control::SetMuted { muted, ack } => {
                assert!(muted);
                ack.send(true).unwrap();
                assert_eq!(ack_rx.try_recv(), Ok(true));
            }
            _ => panic!("control lane returned a different message"),
        }
    }

    #[test]
    fn successful_tick_send_unparks_the_registered_thread() {
        let (tx, _rx, _) = lanes_for_current_thread();
        assert_eq!(tx.offer_tick(tick(1, vec![])), Offer::Sent);

        let started = Instant::now();
        thread::park_timeout(Duration::from_secs(5));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn successful_control_send_unparks_the_registered_thread() {
        let (tx, _rx, _) = lanes_for_current_thread();
        let (ack, _ack_rx) = mpsc::sync_channel(1);
        assert!(tx.control(Control::SetMuted { muted: false, ack }).is_ok());

        let started = Instant::now();
        thread::park_timeout(Duration::from_secs(5));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn offering_to_a_full_lane_returns_without_waiting() {
        let (tx, rx, _) = lanes_for_current_thread();
        for sequence in 0..ORDINARY_LANE as u64 {
            assert_eq!(tx.offer_tick(tick(sequence, vec![])), Offer::Sent);
        }

        let (done_tx, done_rx) = mpsc::channel();
        let sender = tx.clone();
        let worker = thread::spawn(move || {
            done_tx.send(sender.offer_tick(tick(1000, vec![]))).unwrap();
        });
        let result = done_rx.recv_timeout(Duration::from_secs(1));
        if result.is_err() {
            drop(rx);
            let _ = worker.join();
            panic!("offering to a full lane did not return promptly");
        }
        assert_eq!(result.unwrap(), Offer::DroppedOrdinary);
        drop(rx);
        worker.join().unwrap();
    }

    #[test]
    fn disconnected_tick_receivers_are_treated_as_full_lanes() {
        let (tx, rx, shared) = lanes_for_current_thread();
        drop(rx.ordinary);
        assert_eq!(tx.offer_tick(tick(1, vec![])), Offer::DroppedOrdinary);
        assert_eq!(shared.dropped_ordinary.load(Ordering::Relaxed), 1);

        let (tx, rx, shared) = lanes_for_current_thread();
        drop(rx.critical);
        assert_eq!(
            tx.offer_tick(tick(2, vec![event(EventId::HealthDanger, 2)])),
            Offer::CriticalOverflow
        );
        assert_eq!(shared.critical_overflow.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn engine_shared_starts_with_only_device_unavailable() {
        let shared = EngineShared::new();
        assert_eq!(shared.heartbeat_ms.load(Ordering::Relaxed), 0);
        assert!(!shared.muted.load(Ordering::Relaxed));
        assert!(!shared.device_available.load(Ordering::Relaxed));
        assert!(!shared.lease_active.load(Ordering::Relaxed));
        assert_eq!(shared.fence.load(Ordering::Relaxed), 0);
        assert_eq!(shared.dropped_ordinary.load(Ordering::Relaxed), 0);
        assert_eq!(shared.critical_overflow.load(Ordering::Relaxed), 0);
        assert_eq!(shared.out_dropped.load(Ordering::Relaxed), 0);
        assert_eq!(shared.rebuilds.load(Ordering::Relaxed), 0);
        assert!(!shared.degraded.load(Ordering::Relaxed));
    }

    #[test]
    fn tick_control_and_outbound_types_have_the_required_thread_contracts() {
        fn assert_send<T: Send>() {}

        assert_send::<TickMsg>();
        assert_send::<Control>();
        let _ = std::mem::size_of::<EngineOut>();
    }

    // This lexical scan strips `//` comments and test code; it does not parse Rust.
    fn production_region(source: &str) -> String {
        let mut result = String::new();
        for line in source.lines() {
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            let code = line.split_once("//").map_or(line, |(code, _)| code);
            result.push_str(code);
            result.push('\n');
        }
        result
    }

    #[test]
    fn critical_path_source_guard_is_lexical() {
        let sources = [
            ("clock.rs", include_str!("../clock.rs")),
            ("lanes.rs", include_str!("lanes.rs")),
        ];
        let forbidden = [
            "std::fs",
            "std::net",
            "Mutex",
            "RwLock",
            ".lock(",
            ".recv()",
            "thread::sleep",
            ".unwrap(",
            ".expect(",
            "panic!(",
        ];

        for (path, source) in sources {
            let production = production_region(source);
            for token in forbidden {
                assert!(
                    !production.contains(token),
                    "lexical source guard found {token} in {path}"
                );
            }
        }
    }
}
