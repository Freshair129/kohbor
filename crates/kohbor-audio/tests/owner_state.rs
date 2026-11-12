mod common;
use common::*;
use kohbor_audio::{
    AudioClass::*, CancelReason, Detail, Notice as EventNotice, Outcome, PlaybackId, VisualPhase,
};
#[test]
fn mute_before_stale() {
    let mut o = owner();
    o.set_muted(0, true);
    let mut r = request(Voice);
    r.session_epoch = 2;
    assert_eq!(o.request(0, r).unwrap().outcome, Outcome::Muted);
}
#[test]
fn stale_before_revision() {
    let mut o = owner();
    let mut r = request(Critical);
    r.session_epoch = 2;
    r.package_revision = Some(2);
    assert_eq!(o.request(0, r).unwrap().outcome, Outcome::StaleSession);
}
#[test]
fn revision_before_device() {
    let mut o = owner();
    o.set_device_available(0, false);
    let mut r = request(Critical);
    r.package_revision = Some(2);
    assert_eq!(
        o.request(0, r).unwrap().outcome,
        Outcome::SupersededRevision
    );
}
#[test]
fn device_before_matrix() {
    let mut o = owner();
    o.request(0, request(Critical));
    o.set_device_available(1, false);
    let d = o.request(2, request(Voice)).unwrap();
    assert_eq!(
        (d.outcome, d.detail),
        (Outcome::ChannelBusy, Some(Detail::DeviceUnavailable))
    );
}
macro_rules! cancel_test {
    ($name:ident,$action:expr,$reason:expr) => {
        #[test]
        fn $name() {
            let mut o = owner();
            let log = o.sink().calls.clone();
            let mut r = request(Critical);
            r.visual_tail_ms = 100;
            o.request(0, r);
            ($action)(&mut o);
            assert_eq!(o.visual(1), None);
            assert_eq!(
                o.drain_notices(),
                vec![EventNotice::Cancelled {
                    id: PlaybackId(1),
                    reason: $reason
                }]
            );
            assert_eq!(log.borrow().last(), Some(&Call::Cancel(PlaybackId(1))));
        }
    };
}
cancel_test!(
    mute_cancels,
    |o: &mut kohbor_audio::AudioOwner<FakeSink>| o.set_muted(1, true),
    CancelReason::Muted
);
cancel_test!(
    session_cancels,
    |o: &mut kohbor_audio::AudioOwner<FakeSink>| o.set_session(1, 2),
    CancelReason::SessionFence
);
cancel_test!(
    revision_cancels,
    |o: &mut kohbor_audio::AudioOwner<FakeSink>| o.set_revision(1, 2),
    CancelReason::RevisionFence
);
cancel_test!(
    device_cancels,
    |o: &mut kohbor_audio::AudioOwner<FakeSink>| o.set_device_available(1, false),
    CancelReason::DeviceLost
);
#[test]
fn preemption_cancels_visual() {
    let mut o = owner();
    o.request(0, request(Voice));
    o.request(1, request(Critical));
    assert!(o.drain_notices().contains(&EventNotice::Cancelled {
        id: PlaybackId(1),
        reason: CancelReason::Preempted
    }));
    assert_ne!(o.visual(1).map(|v| v.id), Some(PlaybackId(1)));
}
#[test]
fn mute_suppresses_and_unmute_restores_nothing() {
    let mut o = owner();
    let log = o.sink().calls.clone();
    o.request(0, request(Voice));
    o.set_muted(1, true);
    let n = log.borrow().len();
    assert_eq!(
        o.request(2, request(Voice)).unwrap().outcome,
        Outcome::Muted
    );
    assert_eq!(log.borrow().len(), n);
    o.set_muted(3, false);
    assert_eq!(o.active(3), None);
}
#[test]
fn initial_session_zero() {
    let mut o = kohbor_audio::AudioOwner::new(FakeSink::default(), Default::default());
    let mut r = request(Critical);
    r.session_epoch = 0;
    r.package_revision = Some(0);
    assert_eq!(o.request(0, r).unwrap().outcome, Outcome::Admitted);
}
#[test]
fn stale_session_rejected() {
    let mut o = owner();
    let mut r = request(Voice);
    r.session_epoch = 2;
    let d = o.request(0, r).unwrap();
    assert_eq!(d.outcome, Outcome::StaleSession);
    assert_eq!(d.playback_id, None, "rejection must not have a playback ID");
}
#[test]
fn stale_revision_rejected() {
    let mut o = owner();
    let mut r = request(Critical);
    r.package_revision = Some(2);
    assert_eq!(
        o.request(0, r).unwrap().outcome,
        Outcome::SupersededRevision
    );
}
#[test]
fn voice_without_revision_and_event_is_admitted_and_echoed() {
    let mut o = owner();
    let d = o.request(0, request(Voice)).unwrap();
    assert_eq!(d.outcome, Outcome::Admitted);
    assert_eq!(d.package_revision, None);
    assert_eq!(d.event_type, None);
}
#[test]
fn non_voice_without_revision_is_superseded() {
    let mut o = owner();
    let mut r = request(Critical);
    r.package_revision = None;
    let d = o.request(0, r).unwrap();
    assert_eq!(d.outcome, Outcome::SupersededRevision);
    assert_eq!(d.playback_id, None);
}
#[test]
fn revision_change_does_not_cancel_voice() {
    let mut o = owner();
    let log = o.sink().calls.clone();
    let d = o.request(0, request(Voice)).unwrap();
    assert_eq!(d.outcome, Outcome::Admitted);
    o.set_revision(1, 2);
    assert_eq!(o.active(1), d.playback_id);
    assert!(o.drain_notices().is_empty());
    assert_eq!(*log.borrow(), vec![Call::Submit(PlaybackId(1))]);
}
#[test]
fn voice_skips_revision_check_even_if_revision_changes() {
    for revision in [None, Some(0), Some(2), Some(u64::MAX)] {
        let mut o = owner();
        o.set_revision(0, 3);
        let mut r = request(Voice);
        // Some is malformed for the contract, but the owner still skips this check.
        r.package_revision = revision;
        let d = o.request(1, r).unwrap();
        assert_eq!(d.outcome, Outcome::Admitted, "voice revision {revision:?}");
    }
}
#[test]
fn non_voice_event_type_is_echoed_even_if_missing() {
    let mut o = owner();
    let mut r = request(Ordinary);
    r.event_type = None;
    let d = o.request(0, r).unwrap();
    assert_eq!(d.outcome, Outcome::Admitted);
    assert_eq!(
        d.event_type, None,
        "owner echoes input; caller must satisfy schema"
    );
}
#[test]
fn device_restores_service() {
    let mut o = owner();
    let log = o.sink().calls.clone();
    o.set_device_available(0, false);
    let d = o.request(1, request(Voice)).unwrap();
    assert_eq!(
        (d.outcome, d.detail),
        (Outcome::ChannelBusy, Some(Detail::DeviceUnavailable))
    );
    assert!(log.borrow().is_empty());
    o.set_device_available(2, true);
    assert_eq!(
        o.request(3, request(Voice)).unwrap().outcome,
        Outcome::Admitted
    );
}
#[test]
fn sink_failure_no_active() {
    let s = FakeSink::default();
    s.fail.set(true);
    let mut o = kohbor_audio::AudioOwner::new(s, Default::default());
    o.set_session(0, 1);
    o.set_revision(0, 1);
    let d = o.request(0, request(Voice)).unwrap();
    assert_eq!(
        (d.outcome, d.detail),
        (Outcome::ChannelBusy, Some(Detail::SinkFailed))
    );
    assert_eq!(o.active(0), None);
}
#[test]
fn generations_never_reused() {
    let mut o = owner();
    let a = o.request(0, request(Voice)).unwrap().playback_id;
    let b = o.request(1, request(Critical)).unwrap().playback_id;
    assert_eq!((a, b), (Some(PlaybackId(1)), Some(PlaybackId(2))));
}
#[test]
fn late_completion_ignored() {
    let mut o = owner();
    o.request(0, request(Voice));
    o.request(1, request(Critical));
    o.drain_notices();
    o.on_sink_finished(2, PlaybackId(1));
    assert_eq!(o.active(2), Some(PlaybackId(2)));
    assert!(o.drain_notices().is_empty());
}
#[test]
fn current_completion_once() {
    let mut o = owner();
    o.request(0, request(Voice));
    o.on_sink_finished(100, PlaybackId(1));
    assert_eq!(o.active(100), None);
    assert_eq!(
        o.drain_notices(),
        vec![EventNotice::Finished { id: PlaybackId(1) }]
    );
    o.on_sink_finished(101, PlaybackId(1));
    assert!(o.drain_notices().is_empty());
}
#[test]
fn natural_end_tail_and_finish() {
    let mut o = owner();
    let mut r = request(Voice);
    r.visual_tail_ms = 20;
    o.request(0, r);
    assert_eq!(o.visual(99).unwrap().phase, VisualPhase::Audio);
    o.tick(100);
    assert_eq!(o.active(100), None);
    assert_eq!(o.visual(100).unwrap().phase, VisualPhase::Tail);
    assert!(o.drain_notices().is_empty());
    o.tick(120);
    assert_eq!(o.visual(120), None);
    assert_eq!(
        o.drain_notices(),
        vec![EventNotice::Finished { id: PlaybackId(1) }]
    );
}
#[test]
fn tail_does_not_hold_channel() {
    let mut o = owner();
    let mut r = request(Voice);
    r.visual_tail_ms = 100;
    o.request(0, r);
    assert_eq!(
        o.request(100, request(Ordinary)).unwrap().outcome,
        Outcome::Admitted
    );
}
// Earlier caller timestamps are tolerated without panic or arithmetic underflow; ordering is otherwise unspecified.
#[test]
fn earlier_time_does_not_panic() {
    let mut o = owner();
    o.request(100, request(Voice));
    o.request(1, request(Critical));
}
// Zero samples have zero duration; saturation is required near the time limit.
#[test]
fn zero_duration_and_max_time() {
    let mut o = owner();
    let mut r = request(Voice);
    r.pcm = ms(0);
    o.request(u64::MAX, r);
    o.tick(u64::MAX);
    assert_eq!(o.active(u64::MAX), None);
}
#[test]
fn nonzero_duration_at_max_time_does_not_overflow() {
    let mut o = owner();
    o.request(u64::MAX - 1, request(Voice));
    o.tick(u64::MAX);
}
#[test]
fn notices_drain_in_order() {
    let mut o = owner();
    o.request(0, request(Voice));
    o.request(1, request(Ordinary));
    o.set_muted(2, true);
    assert_eq!(
        o.drain_notices(),
        vec![
            EventNotice::Cancelled {
                id: PlaybackId(1),
                reason: CancelReason::Preempted
            },
            EventNotice::Cancelled {
                id: PlaybackId(2),
                reason: CancelReason::Muted
            }
        ]
    );
    assert!(o.drain_notices().is_empty());
}
#[test]
fn decision_echoes_request() {
    let mut o = owner();
    let r = request(Ordinary);
    let d = o.request(7, r.clone()).unwrap();
    assert_eq!(
        (
            d.instance_id,
            d.event_type,
            d.session_epoch,
            d.class,
            d.package_revision,
            d.decided_at_ms,
            d.playback_id
        ),
        (
            r.instance_id,
            r.event_type,
            r.session_epoch,
            r.class,
            r.package_revision,
            7,
            Some(PlaybackId(1))
        )
    );
}

// A failed new submit does not resurrect the playback already cancelled.
#[test]
fn preemption_submit_failure_keeps_old_cancelled() {
    let mut o = owner();
    let log = o.sink().calls.clone();
    let fail = o.sink().fail.clone();
    o.request(0, request(Voice));
    fail.set(true);
    let d = o.request(1, request(Critical)).unwrap();
    assert_eq!(
        (d.outcome, d.detail),
        (Outcome::ChannelBusy, Some(Detail::SinkFailed))
    );
    assert_eq!(o.active(1), None);
    assert_eq!(
        *log.borrow(),
        vec![
            Call::Submit(PlaybackId(1)),
            Call::Cancel(PlaybackId(1)),
            Call::Submit(PlaybackId(2))
        ]
    );
    assert_eq!(
        o.drain_notices(),
        vec![EventNotice::Cancelled {
            id: PlaybackId(1),
            reason: CancelReason::Preempted
        }]
    );
}

macro_rules! queue_cleared {
    ($name:ident, $action:expr) => {
        #[test] fn $name() {
            let mut o = owner(); let log = o.sink().calls.clone();
            o.request(0,request(Critical)); assert!(o.request(1,request(Correction)).is_none());
            ($action)(&mut o); o.tick(200);
            assert_eq!(*log.borrow(),vec![Call::Submit(PlaybackId(1)),Call::Cancel(PlaybackId(1))],stringify!($name));
            assert!(!o.drain_notices().iter().any(|n| matches!(n,EventNotice::Decided(d) if d.outcome==Outcome::Admitted)),stringify!($name));
        }
    };
}
queue_cleared!(mute_clears_queue, |o: &mut kohbor_audio::AudioOwner<
    FakeSink,
>| o.set_muted(2, true));
queue_cleared!(session_clears_queue, |o: &mut kohbor_audio::AudioOwner<
    FakeSink,
>| o.set_session(2, 2));
queue_cleared!(revision_clears_queue, |o: &mut kohbor_audio::AudioOwner<
    FakeSink,
>| o.set_revision(2, 2));
queue_cleared!(device_clears_queue, |o: &mut kohbor_audio::AudioOwner<
    FakeSink,
>| o.set_device_available(2, false));
