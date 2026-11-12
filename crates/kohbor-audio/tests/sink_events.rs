use kohbor_audio::pcm::Pcm;
use kohbor_audio::{
    AudioClass, AudioOwner, CancelReason, Config, Detail, Notice, Outcome, PlaybackId, Request,
    Sink, SinkError, SinkEvent,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

struct ScriptedSink {
    events: Rc<RefCell<VecDeque<SinkEvent>>>,
    poll_calls: Rc<Cell<usize>>,
}

impl Sink for ScriptedSink {
    fn submit(&mut self, _: PlaybackId, _: Pcm) -> Result<(), SinkError> {
        Ok(())
    }

    fn cancel(&mut self, _: PlaybackId) {}

    fn poll_event(&mut self) -> Option<SinkEvent> {
        self.poll_calls.set(self.poll_calls.get() + 1);
        self.events.borrow_mut().pop_front()
    }
}

fn scripted(events: impl IntoIterator<Item = SinkEvent>) -> (ScriptedSink, Rc<Cell<usize>>) {
    let events = Rc::new(RefCell::new(events.into_iter().collect()));
    let poll_calls = Rc::new(Cell::new(0));
    (
        ScriptedSink {
            events,
            poll_calls: Rc::clone(&poll_calls),
        },
        poll_calls,
    )
}

fn request() -> Request {
    Request {
        instance_id: "instance_1".to_owned(),
        event_type: Some("health_low".to_owned()),
        session_epoch: 1,
        class: AudioClass::Ordinary,
        episode: None,
        package_revision: Some(1),
        pcm: Arc::from(vec![7_i16; 4_800]),
        visual_tail_ms: 0,
    }
}

fn owner_with_events(
    events: impl IntoIterator<Item = SinkEvent>,
) -> (AudioOwner<ScriptedSink>, Rc<Cell<usize>>) {
    let (sink, poll_calls) = scripted(events);
    let mut owner = AudioOwner::new(sink, Config::default());
    owner.set_session(0, 1);
    owner.set_revision(0, 1);
    (owner, poll_calls)
}

fn start(owner: &mut AudioOwner<ScriptedSink>) -> PlaybackId {
    let decision = owner
        .request(0, request())
        .expect("ordinary request is decided immediately");
    assert_eq!(decision.outcome, Outcome::Admitted);
    decision.playback_id.expect("admitted playback has an id")
}

#[test]
fn current_finished_event_ends_playback_once_and_drains() {
    let (mut owner, poll_calls) = owner_with_events([SinkEvent::Finished { id: PlaybackId(1) }]);
    let current = start(&mut owner);

    owner.pump_sink_events(10);
    assert_eq!(owner.active(10), None);
    assert_eq!(
        owner.drain_notices(),
        vec![Notice::Finished {
            id: current,
            at_ms: 10,
        }]
    );

    owner.pump_sink_events(11);
    assert!(owner.drain_notices().is_empty());
    assert_eq!(poll_calls.get(), 3);
}

#[test]
fn stale_finished_event_is_inert() {
    let (mut owner, poll_calls) = owner_with_events([SinkEvent::Finished { id: PlaybackId(99) }]);
    let current = start(&mut owner);

    owner.pump_sink_events(10);

    assert_eq!(owner.active(10), Some(current));
    assert!(owner.drain_notices().is_empty());
    assert_eq!(poll_calls.get(), 2);
}

#[test]
fn device_lost_cancels_playback_and_marks_device_unavailable() {
    let (mut owner, _) = owner_with_events([SinkEvent::DeviceLost]);
    let current = start(&mut owner);

    owner.pump_sink_events(10);

    assert_eq!(owner.active(10), None);
    assert_eq!(
        owner.drain_notices(),
        vec![Notice::Cancelled {
            id: current,
            reason: CancelReason::DeviceLost,
            at_ms: 10,
        }]
    );
    let decision = owner.request(11, request()).expect("request is decided");
    assert_eq!(decision.outcome, Outcome::ChannelBusy);
    assert_eq!(decision.detail, Some(Detail::DeviceUnavailable));
}

#[test]
fn device_changed_keeps_current_playback_unchanged() {
    let (mut owner, _) = owner_with_events([SinkEvent::DeviceChanged]);
    let current = start(&mut owner);

    owner.pump_sink_events(10);

    assert_eq!(owner.active(10), Some(current));
    assert!(owner.drain_notices().is_empty());
}

#[test]
fn event_order_is_applied_in_queue_order_and_drain_reaches_empty() {
    let (mut owner, poll_calls) = owner_with_events([
        SinkEvent::DeviceChanged,
        SinkEvent::DeviceLost,
        SinkEvent::Finished { id: PlaybackId(1) },
        SinkEvent::DeviceChanged,
    ]);
    let current = start(&mut owner);

    owner.pump_sink_events(10);

    assert_eq!(owner.active(10), None);
    assert_eq!(
        owner.drain_notices(),
        vec![Notice::Cancelled {
            id: current,
            reason: CancelReason::DeviceLost,
            at_ms: 10,
        }]
    );
    assert_eq!(poll_calls.get(), 5);
}

#[test]
fn no_sink_events_have_no_effect() {
    let (mut owner, poll_calls) = owner_with_events([]);
    let current = start(&mut owner);

    owner.pump_sink_events(10);

    assert_eq!(owner.active(10), Some(current));
    assert!(owner.drain_notices().is_empty());
    assert_eq!(poll_calls.get(), 1);
}
