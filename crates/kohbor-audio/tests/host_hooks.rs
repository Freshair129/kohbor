use kohbor_audio::pcm::Pcm;
use kohbor_audio::{
    AudioClass, AudioOwner, CancelReason, Config, Detail, Notice, Outcome, PlaybackId, Request,
    Sink, SinkError, SinkEvent,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
enum SinkCall {
    Submit(&'static str, PlaybackId),
    Cancel(&'static str, PlaybackId),
    Poll(&'static str),
}

#[derive(Clone, Default)]
struct SinkProbe {
    calls: Rc<RefCell<Vec<SinkCall>>>,
    events: Rc<RefCell<VecDeque<SinkEvent>>>,
}

struct FakeSink {
    name: &'static str,
    probe: SinkProbe,
}

impl FakeSink {
    fn new(name: &'static str) -> (Self, SinkProbe) {
        let probe = SinkProbe::default();
        (
            Self {
                name,
                probe: probe.clone(),
            },
            probe,
        )
    }
}

impl Sink for FakeSink {
    fn submit(&mut self, id: PlaybackId, _: Pcm) -> Result<(), SinkError> {
        self.probe
            .calls
            .borrow_mut()
            .push(SinkCall::Submit(self.name, id));
        Ok(())
    }

    fn cancel(&mut self, id: PlaybackId) {
        self.probe
            .calls
            .borrow_mut()
            .push(SinkCall::Cancel(self.name, id));
    }

    fn poll_event(&mut self) -> Option<SinkEvent> {
        self.probe
            .calls
            .borrow_mut()
            .push(SinkCall::Poll(self.name));
        self.probe.events.borrow_mut().pop_front()
    }
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

fn owner(first_playback_id: u64, sink: FakeSink) -> AudioOwner<FakeSink> {
    let mut owner = AudioOwner::with_first_playback_id(sink, Config::default(), first_playback_id);
    owner.set_session(0, 1);
    owner.set_revision(0, 1);
    owner
}

fn admitted_id(owner: &mut AudioOwner<FakeSink>, at_ms: u64) -> PlaybackId {
    let decision = owner
        .request(at_ms, request())
        .expect("ordinary request is decided immediately");
    assert_eq!(decision.outcome, Outcome::Admitted);
    decision.playback_id.expect("admitted playback has an id")
}

#[test]
fn configured_first_id_is_used_once_then_increments() {
    let (sink, probe) = FakeSink::new("configured");
    let mut owner = owner(41, sink);

    assert_eq!(admitted_id(&mut owner, 0), PlaybackId(41));
    assert_eq!(admitted_id(&mut owner, 100), PlaybackId(42));
    assert_eq!(admitted_id(&mut owner, 200), PlaybackId(43));
    assert_eq!(
        *probe.calls.borrow(),
        vec![
            SinkCall::Submit("configured", PlaybackId(41)),
            SinkCall::Submit("configured", PlaybackId(42)),
            SinkCall::Submit("configured", PlaybackId(43)),
        ]
    );
}

#[test]
fn zero_first_id_is_treated_as_one() {
    let (sink, _) = FakeSink::new("zero");
    let mut owner = owner(0, sink);

    assert_eq!(admitted_id(&mut owner, 0), PlaybackId(1));
}

#[test]
fn max_id_is_admitted_once_then_exhaustion_is_reported_without_wrapping() {
    let (sink, probe) = FakeSink::new("near-max");
    let mut owner = owner(u64::MAX - 1, sink);

    assert_eq!(admitted_id(&mut owner, 0), PlaybackId(u64::MAX - 1));
    assert_eq!(admitted_id(&mut owner, 100), PlaybackId(u64::MAX));

    let exhausted = owner.request(200, request()).expect("request is decided");
    assert_eq!(exhausted.outcome, Outcome::ChannelBusy);
    assert_eq!(exhausted.playback_id, None);
    assert_eq!(exhausted.detail, Some(Detail::SinkFailed));

    let still_exhausted = owner.request(300, request()).expect("request is decided");
    assert_eq!(still_exhausted.outcome, Outcome::ChannelBusy);
    assert_eq!(still_exhausted.detail, Some(Detail::SinkFailed));
    assert_eq!(
        *probe.calls.borrow(),
        vec![
            SinkCall::Submit("near-max", PlaybackId(u64::MAX - 1)),
            SinkCall::Submit("near-max", PlaybackId(u64::MAX)),
        ]
    );
}

#[test]
fn replacement_sink_receives_requests_after_device_recovery() {
    let (initial_sink, initial_probe) = FakeSink::new("initial");
    let (replacement_sink, replacement_probe) = FakeSink::new("replacement");
    let mut owner = owner(1, initial_sink);

    owner.set_device_available(0, false);
    *owner.sink_mut() = replacement_sink;
    let unavailable = owner.request(1, request()).expect("request is decided");
    assert_eq!(unavailable.outcome, Outcome::ChannelBusy);
    assert_eq!(unavailable.detail, Some(Detail::DeviceUnavailable));

    owner.set_device_available(2, true);
    assert_eq!(admitted_id(&mut owner, 2), PlaybackId(1));
    assert!(initial_probe.calls.borrow().is_empty());
    assert_eq!(
        *replacement_probe.calls.borrow(),
        vec![SinkCall::Submit("replacement", PlaybackId(1))]
    );
}

#[test]
fn state_getters_follow_muted_and_device_transitions() {
    let (sink, probe) = FakeSink::new("state");
    let mut owner = owner(1, sink);
    probe.events.borrow_mut().push_back(SinkEvent::DeviceLost);

    assert!(!owner.is_muted());
    assert!(owner.device_available());
    owner.set_muted(0, true);
    assert!(owner.is_muted());
    owner.set_muted(1, false);
    assert!(!owner.is_muted());

    owner.set_device_available(2, false);
    assert!(!owner.device_available());
    owner.set_device_available(3, true);
    assert!(owner.device_available());
    owner.pump_sink_events(4);
    assert!(!owner.device_available());
}

#[test]
fn getters_do_not_poll_sink_change_notices_or_mutate_owner_state() {
    let (sink, probe) = FakeSink::new("pure-getters");
    let mut owner = owner(1, sink);
    probe.events.borrow_mut().push_back(SinkEvent::DeviceLost);
    assert_eq!(admitted_id(&mut owner, 0), PlaybackId(1));

    let calls_before = probe.calls.borrow().clone();
    assert!(!owner.is_muted());
    assert!(owner.device_available());
    assert_eq!(*probe.calls.borrow(), calls_before);
    assert!(owner.drain_notices().is_empty());

    owner.pump_sink_events(1);
    assert!(!owner.device_available());
    assert_eq!(
        owner.drain_notices(),
        vec![Notice::Cancelled {
            id: PlaybackId(1),
            reason: CancelReason::DeviceLost,
            at_ms: 1,
        }]
    );

    let calls_before = probe.calls.borrow().clone();
    assert!(!owner.is_muted());
    assert!(!owner.device_available());
    assert_eq!(*probe.calls.borrow(), calls_before);
    assert!(owner.drain_notices().is_empty());
}
