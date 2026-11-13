#![cfg(test)]

use super::{
    engine::Engine,
    lanes::{self, EngineOut, EngineShared, EngineTx, TickMsg},
    resolve::{Clip, Entry, PrewarmTable},
};
use crate::clock::MonoClock;
use kohbor_audio::{pcm::Pcm, PlaybackId, Sink, SinkError, SinkEvent};
use kohbor_events::{Event, EventId};
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Submit(PlaybackId, usize),
    Cancel(PlaybackId),
}
#[derive(Default)]
pub struct Script {
    pub calls: Vec<Call>,
    pub events: VecDeque<SinkEvent>,
    pub fail_next: bool,
    pub panic_next: bool,
    pub panic_next_poll: bool,
}
pub type Log = Arc<Mutex<Script>>;
pub struct FakeSink(pub Log);
impl FakeSink {
    pub fn new() -> (Self, Log) {
        let log = Arc::new(Mutex::new(Script::default()));
        (Self(log.clone()), log)
    }
}
impl Sink for FakeSink {
    fn submit(&mut self, id: PlaybackId, pcm: Pcm) -> Result<(), SinkError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push(Call::Submit(id, pcm.len()));
        let panic_next = std::mem::take(&mut s.panic_next);
        let fail = std::mem::take(&mut s.fail_next);
        drop(s);
        assert!(!panic_next, "scripted submit panic");
        if fail {
            Err(SinkError)
        } else {
            Ok(())
        }
    }
    fn cancel(&mut self, id: PlaybackId) {
        self.0.lock().unwrap().calls.push(Call::Cancel(id));
    }
    fn poll_event(&mut self) -> Option<SinkEvent> {
        let mut script = self.0.lock().unwrap();
        let should_panic = std::mem::take(&mut script.panic_next_poll);
        let event = script.events.pop_front();
        drop(script);
        assert!(!should_panic, "scripted poll panic");
        event
    }
}
pub type Row<'a> = (EventId, Option<u8>, usize, u32, Option<&'a str>);
pub fn table(fence: u64, rows: &[Row<'_>]) -> Arc<PrewarmTable> {
    Arc::new(PrewarmTable::new(
        fence,
        Some(Arc::from(format!("ins_{fence:010}"))),
        rows.iter()
            .map(|(id, tier, count, tail, url)| {
                (
                    *id,
                    *tier,
                    Entry::Ready(Clip {
                        pcm: Arc::from(vec![0_i16; *count]),
                        bound_tier: *tier,
                        audio_ms: ((*count as u64 * 1000).div_ceil(48000)) as u32,
                        visual_tail_ms: *tail,
                        visual_url: url.map(Arc::from),
                    }),
                )
            })
            .collect(),
    ))
}
pub fn tick(
    origin: Instant,
    epoch: u64,
    generation: u64,
    sequence: u64,
    events: &[(EventId, Option<u8>)],
) -> TickMsg {
    TickMsg {
        arrival: origin,
        detected: origin + Duration::from_micros(50),
        at_ms: 0,
        session_epoch: epoch,
        generation,
        sequence,
        events: events
            .iter()
            .map(|(id, tier)| Event {
                id: *id,
                tier: *tier,
                at_ms: 0,
            })
            .collect(),
    }
}
pub struct Harness {
    pub tx: EngineTx,
    pub engine: Engine,
    pub out: mpsc::Receiver<EngineOut>,
    pub shared: Arc<EngineShared>,
    pub origin: Instant,
    pub clock: MonoClock,
}
impl Harness {
    pub fn new(sink: Option<Box<dyn Sink + Send>>, latency: bool, capacity: usize) -> Self {
        let origin = Instant::now() - Duration::from_millis(1);
        let clock = MonoClock::from_origin(origin);
        let shared = Arc::new(EngineShared::new());
        let (tx, rx) = lanes::lanes(thread::current(), shared.clone());
        let (out_tx, out) = mpsc::sync_channel(capacity);
        let engine = Engine::new(clock, shared.clone(), rx, sink, out_tx, latency);
        Self {
            tx,
            engine,
            out,
            shared,
            origin,
            clock,
        }
    }
    pub fn drain(&self) -> Vec<EngineOut> {
        let outputs: Vec<_> = self.out.try_iter().collect();
        super::tests_engine::check_outputs(&outputs);
        outputs
    }
}
