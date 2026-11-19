mod common;
use common::*;
use kohbor_audio::{AudioClass::*, CancelReason, Notice as EventNotice, Outcome, PlaybackId};

fn ranked_ordinary(instance_id: &str, rank: u32) -> kohbor_audio::Request {
    let mut request = request(Ordinary);
    request.instance_id = instance_id.to_owned();
    request.ordinary_rank = rank;
    request
}

macro_rules! matrix {
    ($name:ident, $incoming:expr, $active:expr, $expected:expr, $preempt:expr) => {
        #[test]
        fn $name() {
            let mut o = owner();
            let log = o.sink().calls.clone();
            let active: Option<kohbor_audio::AudioClass> = $active;
            if let Some(class) = active {
                let mut r = request(class);
                r.pcm = ms(20_000);
                r.episode = Some("old".into());
                assert_eq!(
                    o.request(0, r).unwrap().outcome,
                    Outcome::Admitted,
                    "{} active",
                    stringify!($name)
                );
            }
            let mut r = request($incoming);
            r.episode = Some("new".into());
            let got = o.request(10_000, r);
            assert_eq!(
                got.as_ref().map(|d| d.outcome),
                $expected,
                "{} outcome",
                stringify!($name)
            );
            let expected = match (active, $preempt) {
                (None, _) => vec![Call::Submit(PlaybackId(1))],
                (Some(_), true) => vec![
                    Call::Submit(PlaybackId(1)),
                    Call::Cancel(PlaybackId(1)),
                    Call::Submit(PlaybackId(2)),
                ],
                _ => vec![Call::Submit(PlaybackId(1))],
            };
            assert_eq!(*log.borrow(), expected, "{} sink order", stringify!($name));
            let notices = o.drain_notices();
            if $preempt {
                assert_eq!(
                    notices,
                    vec![EventNotice::Cancelled {
                        id: PlaybackId(1),
                        reason: CancelReason::Preempted,
                        at_ms: 10_000,
                    }],
                    "{} notice",
                    stringify!($name)
                );
            } else {
                assert!(
                    notices.is_empty(),
                    "{} unexpected notice",
                    stringify!($name)
                );
            }
        }
    };
}
macro_rules! row {
    ($name:ident,$in:ident,$active:expr,$out:expr,$pre:expr) => {
        matrix!($name, $in, $active, $out, $pre);
    };
}
row!(c1_idle, Critical, None, Some(Outcome::Admitted), false);
row!(
    c1_c1_different,
    Critical,
    Some(Critical),
    Some(Outcome::Admitted),
    true
);
row!(
    c1_c2,
    Critical,
    Some(Correction),
    Some(Outcome::Admitted),
    true
);
row!(c1_c3, Critical, Some(Notice), Some(Outcome::Admitted), true);
row!(
    c1_c4,
    Critical,
    Some(Ordinary),
    Some(Outcome::Admitted),
    true
);
row!(c1_c5, Critical, Some(Voice), Some(Outcome::Admitted), true);
row!(c2_idle, Correction, None, Some(Outcome::Admitted), false);
row!(c2_c1, Correction, Some(Critical), None, false);
row!(
    c2_c2,
    Correction,
    Some(Correction),
    Some(Outcome::Admitted),
    true
);
row!(
    c2_c3,
    Correction,
    Some(Notice),
    Some(Outcome::Admitted),
    true
);
row!(
    c2_c4,
    Correction,
    Some(Ordinary),
    Some(Outcome::Admitted),
    true
);
row!(
    c2_c5,
    Correction,
    Some(Voice),
    Some(Outcome::Admitted),
    true
);
row!(c3_idle, Notice, None, Some(Outcome::Admitted), false);
row!(
    c3_c1,
    Notice,
    Some(Critical),
    Some(Outcome::CriticalActive),
    false
);
row!(
    c3_c2,
    Notice,
    Some(Correction),
    Some(Outcome::CriticalActive),
    false
);
row!(
    c3_c3,
    Notice,
    Some(Notice),
    Some(Outcome::ChannelBusy),
    false
);
row!(c3_c4, Notice, Some(Ordinary), Some(Outcome::Admitted), true);
row!(c3_c5, Notice, Some(Voice), Some(Outcome::Admitted), true);
row!(c4_idle, Ordinary, None, Some(Outcome::Admitted), false);
row!(c4_c1, Ordinary, Some(Critical), None, false);
row!(c4_c2, Ordinary, Some(Correction), None, false);
row!(
    c4_c3,
    Ordinary,
    Some(Notice),
    Some(Outcome::ChannelBusy),
    false
);
row!(
    c4_c4,
    Ordinary,
    Some(Ordinary),
    Some(Outcome::ChannelBusy),
    false
);
row!(c4_c5, Ordinary, Some(Voice), Some(Outcome::Admitted), true);
row!(c5_idle, Voice, None, Some(Outcome::Admitted), false);
row!(
    c5_c1,
    Voice,
    Some(Critical),
    Some(Outcome::ChannelBusy),
    false
);
row!(
    c5_c2,
    Voice,
    Some(Correction),
    Some(Outcome::ChannelBusy),
    false
);
row!(
    c5_c3,
    Voice,
    Some(Notice),
    Some(Outcome::ChannelBusy),
    false
);
row!(
    c5_c4,
    Voice,
    Some(Ordinary),
    Some(Outcome::ChannelBusy),
    false
);
row!(c5_c5, Voice, Some(Voice), Some(Outcome::Admitted), true);

#[test]
fn c1_same_episode_cooldown() {
    let mut o = owner();
    let log = o.sink().calls.clone();
    let mut r = request(Critical);
    r.episode = Some("x".into());
    o.request(0, r.clone());
    assert_eq!(o.request(1, r).unwrap().outcome, Outcome::Cooldown);
    assert_eq!(
        *log.borrow(),
        vec![Call::Submit(PlaybackId(1))],
        "same episode does not preempt"
    );
    assert!(
        o.drain_notices().is_empty(),
        "same episode emits no cancellation"
    );
}
#[test]
fn c3_cooldown_before_channel() {
    let mut o = owner();
    o.request(0, request(Notice));
    o.request(100, request(Critical));
    assert_eq!(
        o.request(101, request(Notice)).unwrap().outcome,
        Outcome::Cooldown
    );
}
#[test]
fn c3_cooldown_boundary() {
    let mut o = owner();
    o.request(0, request(Notice));
    assert_eq!(
        o.request(10_000, request(Notice)).unwrap().outcome,
        Outcome::Admitted
    );
}
#[test]
fn c3_cooldown_one_ms_before_boundary() {
    let mut o = owner();
    o.request(0, request(Notice));
    assert_eq!(
        o.request(9_999, request(Notice)).unwrap().outcome,
        Outcome::Cooldown
    );
}
#[test]
fn queued_c2_starts_after_c1() {
    let mut o = owner();
    let log = o.sink().calls.clone();
    o.request(0, request(Critical));
    assert!(o.request(1, request(Correction)).is_none());
    o.tick(100);
    assert!(o
        .drain_notices()
        .iter()
        .any(|n| matches!(n,EventNotice::Decided(d) if d.outcome==Outcome::Admitted)));
    assert_eq!(log.borrow().len(), 2);
}
#[test]
fn queued_c2_latest_replaces_first() {
    let mut o = owner();
    o.request(0, request(Critical));
    let mut first = request(Correction);
    first.instance_id = "first".into();
    let mut second = request(Correction);
    second.instance_id = "second".into();
    assert!(o.request(1, first).is_none());
    assert!(o.request(2, second).is_none());
    assert!(o.drain_notices().iter().any(
        |n| matches!(n,EventNotice::Decided(d) if d.instance_id=="first" && d.outcome==Outcome::CriticalActive && d.detail==Some(kohbor_audio::Detail::QueueReplaced))
    ));
    o.tick(100);
    assert!(o.drain_notices().iter().any(|n| matches!(n,EventNotice::Decided(d) if d.instance_id=="second" && d.outcome==Outcome::Admitted)), "latest request must start");
}
#[test]
fn queued_c2_expired() {
    let mut o = owner();
    let log = o.sink().calls.clone();
    let mut r = request(Critical);
    r.pcm = ms(5_000);
    o.request(0, r);
    o.request(1, request(Correction));
    o.tick(3_002);
    assert!(o.drain_notices().iter().any(|n| matches!(n,EventNotice::Decided(d) if d.outcome==Outcome::CriticalActive && d.detail==Some(kohbor_audio::Detail::QueueExpired))));
    assert_eq!(
        *log.borrow(),
        vec![Call::Submit(PlaybackId(1))],
        "expired C2 must not submit"
    );
}
// "Older than TTL" means exactly 3,000 ms remains eligible; 3,001 expires.
#[test]
fn queued_c2_ttl_inclusive() {
    let mut o = owner();
    let mut r = request(Critical);
    r.pcm = ms(3_001);
    o.request(0, r);
    o.request(1, request(Correction));
    o.tick(3_001);
    assert!(o
        .drain_notices()
        .iter()
        .any(|n| matches!(n,EventNotice::Decided(d) if d.outcome==Outcome::Admitted)));
}

#[test]
fn queued_c4_starts_after_c1_ends() {
    let mut o = owner();
    o.request(0, request(Critical));

    assert!(
        o.request(1, request(Ordinary)).is_none(),
        "C4 waits in the ordinary slot while C1 owns the channel"
    );
    o.tick(100);

    let decisions: Vec<_> = o
        .drain_notices()
        .into_iter()
        .filter_map(|notice| match notice {
            EventNotice::Decided(decision) => Some(decision),
            _ => None,
        })
        .collect();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].outcome, Outcome::Admitted);
    assert_eq!(decisions[0].playback_id, Some(PlaybackId(2)));
}

#[test]
fn queued_c4_expires_after_its_ttl() {
    let mut o = owner();
    let mut critical = request(Critical);
    critical.pcm = ms(5_000);
    o.request(0, critical);
    assert!(o.request(1, ranked_ordinary("expired", 90)).is_none());

    o.tick(3_002);

    assert!(o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "expired"
                && decision.outcome == Outcome::CriticalActive
                && decision.detail == Some(kohbor_audio::Detail::QueueExpired)
    )));
    assert_eq!(o.active_class(3_002), Some(Critical));
}

#[test]
fn higher_rank_replaces_lower_rank_in_c4_queue() {
    let mut o = owner();
    o.request(0, request(Critical));
    assert!(o.request(1, ranked_ordinary("hero_kill", 60)).is_none());
    assert!(o.request(2, ranked_ordinary("multi_kill", 90)).is_none());

    assert!(o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "hero_kill"
                && decision.outcome == Outcome::CriticalActive
                && decision.detail == Some(kohbor_audio::Detail::QueueReplaced)
    )));
    o.tick(100);
    assert!(o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "multi_kill" && decision.outcome == Outcome::Admitted
    )));
}

#[test]
fn lower_rank_loses_to_existing_c4_queue_entry() {
    let mut o = owner();
    o.request(0, request(Critical));
    assert!(o.request(1, ranked_ordinary("multi_kill", 90)).is_none());
    let loser = o.request(2, ranked_ordinary("hero_kill", 60)).unwrap();
    assert_eq!(loser.outcome, Outcome::CriticalActive);
    assert_eq!(loser.detail, Some(kohbor_audio::Detail::QueueReplaced));
    o.tick(100);
    assert!(o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "multi_kill" && decision.outcome == Outcome::Admitted
    )));
}

#[test]
fn newer_equal_rank_replaces_older_c4_queue_entry() {
    let mut o = owner();
    o.request(0, request(Critical));
    assert!(o.request(1, ranked_ordinary("first", 90)).is_none());
    assert!(o.request(2, ranked_ordinary("second", 90)).is_none());

    assert!(o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "first"
                && decision.outcome == Outcome::CriticalActive
                && decision.detail == Some(kohbor_audio::Detail::QueueReplaced)
    )));
    o.tick(100);
    assert!(o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "second" && decision.outcome == Outcome::Admitted
    )));
}

#[test]
fn queued_c2_starts_before_waiting_c4() {
    let mut o = owner();
    let mut critical = request(Critical);
    critical.pcm = ms(500);
    o.request(0, critical);
    assert!(o.request(1, ranked_ordinary("ordinary", 90)).is_none());
    let mut correction = request(Correction);
    correction.instance_id = "correction".into();
    assert!(o.request(2, correction).is_none());

    o.tick(500);
    let first = o.drain_notices();
    assert!(first.iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "correction"
                && decision.class == Correction
                && decision.outcome == Outcome::Admitted
                && decision.playback_id == Some(PlaybackId(2))
    )));
    assert_eq!(o.active_class(500), Some(Correction));

    o.tick(600);
    assert!(o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "ordinary"
                && decision.class == Ordinary
                && decision.outcome == Outcome::Admitted
                && decision.playback_id == Some(PlaybackId(3))
    )));
}

#[test]
fn queued_c4_is_cleared_by_session_revision_and_mute_fences() {
    let mut session_owner = owner();
    session_owner.request(0, request(Critical));
    assert!(session_owner
        .request(1, ranked_ordinary("session", 90))
        .is_none());
    session_owner.set_session(2, 2);
    assert!(session_owner.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "session" && decision.outcome == Outcome::StaleSession
    )));

    let mut revision_owner = owner();
    revision_owner.request(0, request(Critical));
    assert!(revision_owner
        .request(1, ranked_ordinary("revision", 90))
        .is_none());
    revision_owner.set_revision(2, 2);
    assert!(revision_owner.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "revision"
                && decision.outcome == Outcome::SupersededRevision
    )));

    let mut muted_owner = owner();
    muted_owner.request(0, request(Critical));
    assert!(muted_owner
        .request(1, ranked_ordinary("muted", 90))
        .is_none());
    muted_owner.set_muted(2, true);
    assert!(muted_owner.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "muted" && decision.outcome == Outcome::Muted
    )));
}

#[test]
fn queued_c4_is_cleared_when_device_is_lost_or_owner_stops() {
    let mut device_owner = owner();
    device_owner.request(0, request(Critical));
    assert!(device_owner
        .request(1, ranked_ordinary("device", 90))
        .is_none());
    device_owner.set_device_available(2, false);
    assert!(device_owner.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "device"
                && decision.outcome == Outcome::ChannelBusy
                && decision.detail == Some(kohbor_audio::Detail::DeviceUnavailable)
    )));

    let mut stopped_owner = owner();
    stopped_owner.request(0, request(Critical));
    assert!(stopped_owner
        .request(1, ranked_ordinary("stopped", 90))
        .is_none());
    stopped_owner.stop(2);
    assert!(stopped_owner.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "stopped"
                && decision.outcome == Outcome::ChannelBusy
                && decision.detail.is_none()
    )));
}

#[test]
fn incoming_c1_does_not_start_waiting_c4_before_admission() {
    let mut o = owner();
    o.request(0, request(Critical));
    assert!(o.request(1, ranked_ordinary("ordinary", 90)).is_none());

    assert_eq!(
        o.request(100, request(Critical)).unwrap().outcome,
        Outcome::Admitted
    );
    assert_eq!(o.active_class(100), Some(Critical));
    assert!(!o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "ordinary" && decision.outcome == Outcome::Admitted
    )));

    o.tick(200);
    assert!(o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "ordinary" && decision.outcome == Outcome::Admitted
    )));
}

#[test]
fn incoming_c3_does_not_start_waiting_c4_before_admission() {
    let mut o = owner();
    let mut critical = request(Critical);
    critical.pcm = ms(100);
    o.request(0, critical);
    assert!(o.request(1, ranked_ordinary("ordinary", 90)).is_none());

    let notice = o.request(100, request(Notice)).unwrap();
    assert_eq!(notice.outcome, Outcome::Admitted);
    assert_eq!(o.active_class(100), Some(Notice));
    assert!(!o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "ordinary" && decision.outcome == Outcome::Admitted
    )));

    o.tick(200);
    assert!(o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(decision)
            if decision.instance_id == "ordinary" && decision.outcome == Outcome::Admitted
    )));
}

#[test]
fn delayed_tick_does_not_submit_expired_queued_audio() {
    for class in [Correction, Ordinary] {
        for tick_at_ms in [3_002, 4_000] {
            let mut o = owner();
            o.request(0, request(Critical)).unwrap();
            let mut queued = request(class);
            queued.instance_id = "queued".into();
            assert!(o.request(1, queued).is_none());

            o.tick(tick_at_ms);

            assert_eq!(
                o.sink().calls.borrow().as_slice(),
                &[Call::Submit(PlaybackId(1))],
                "{class:?} must not reach the sink at {tick_at_ms}"
            );
            assert!(o.drain_notices().iter().any(|notice| matches!(
                notice,
                EventNotice::Decided(d)
                    if d.instance_id == "queued"
                        && d.outcome == Outcome::CriticalActive
                        && d.detail == Some(kohbor_audio::Detail::QueueExpired)
                        && d.playback_id.is_none()
            )));
        }
    }
}

#[test]
fn incoming_c1_does_not_submit_queued_c2_before_admission() {
    let mut o = owner();
    o.request(0, request(Critical)).unwrap();
    let mut correction = request(Correction);
    correction.instance_id = "correction".into();
    assert!(o.request(1, correction).is_none());

    let critical = o.request(100, request(Critical)).unwrap();
    assert_eq!(critical.outcome, Outcome::Admitted);
    assert_eq!(critical.playback_id, Some(PlaybackId(2)));
    assert_eq!(
        o.sink().calls.borrow().as_slice(),
        &[Call::Submit(PlaybackId(1)), Call::Submit(PlaybackId(2))]
    );
    assert!(!o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(d) if d.instance_id == "correction"
    )));

    o.tick(200);
    assert!(o.drain_notices().iter().any(|notice| matches!(
        notice,
        EventNotice::Decided(d)
            if d.instance_id == "correction"
                && d.outcome == Outcome::Admitted
                && d.playback_id == Some(PlaybackId(3))
    )));
}

#[test]
fn fences_at_audio_end_decide_queued_audio_without_submitting_it() {
    type Fence = fn(&mut kohbor_audio::AudioOwner<FakeSink>, u64);
    let fences: [(Fence, Outcome, Option<kohbor_audio::Detail>); 5] = [
        (|o, at| o.set_muted(at, true), Outcome::Muted, None),
        (|o, at| o.set_session(at, 2), Outcome::StaleSession, None),
        (
            |o, at| o.set_revision(at, 2),
            Outcome::SupersededRevision,
            None,
        ),
        (
            |o, at| o.set_device_available(at, false),
            Outcome::ChannelBusy,
            Some(kohbor_audio::Detail::DeviceUnavailable),
        ),
        (|o, at| o.stop(at), Outcome::ChannelBusy, None),
    ];
    for (fence, outcome, detail) in fences {
        let mut o = owner();
        o.request(0, request(Critical)).unwrap();
        let mut correction = request(Correction);
        correction.instance_id = "correction".into();
        assert!(o.request(1, correction).is_none());
        assert!(o.request(1, ranked_ordinary("ordinary", 90)).is_none());

        fence(&mut o, 100);
        o.tick(200);

        assert_eq!(
            o.sink().calls.borrow().as_slice(),
            &[Call::Submit(PlaybackId(1))],
            "{outcome:?} must not submit queued audio"
        );
        let notices = o.drain_notices();
        for id in ["correction", "ordinary"] {
            assert!(notices.iter().any(|notice| matches!(
                notice,
                EventNotice::Decided(d)
                    if d.instance_id == id
                        && d.outcome == outcome
                        && d.detail == detail
                        && d.playback_id.is_none()
            )));
        }
    }
}
