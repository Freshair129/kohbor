mod common;
use common::*;
use kohbor_audio::{AudioClass::*, CancelReason, Notice as EventNotice, Outcome, PlaybackId};

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
row!(
    c4_c1,
    Ordinary,
    Some(Critical),
    Some(Outcome::CriticalActive),
    false
);
row!(
    c4_c2,
    Ordinary,
    Some(Correction),
    Some(Outcome::CriticalActive),
    false
);
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
