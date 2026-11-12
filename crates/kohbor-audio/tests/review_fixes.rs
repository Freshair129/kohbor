use kohbor_audio::pcm::{decode_wav, duration_ms, Pcm};
use kohbor_audio::{
    AudioClass, AudioOwner, CancelReason, Config, Detail, Notice, Outcome, PlaybackId, Request,
    Sink, SinkError, VisualPhase,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Submit(PlaybackId),
    Cancel(PlaybackId),
}

#[derive(Default)]
struct RecordingSink {
    calls: Rc<RefCell<Vec<Call>>>,
    fail_next: Rc<Cell<bool>>,
    current: Rc<Cell<Option<PlaybackId>>>,
}

impl Sink for RecordingSink {
    fn submit(&mut self, id: PlaybackId, _: Pcm) -> Result<(), SinkError> {
        self.calls.borrow_mut().push(Call::Submit(id));
        if self.fail_next.replace(false) {
            self.current.set(None);
            Err(SinkError)
        } else {
            self.current.set(Some(id));
            Ok(())
        }
    }

    fn cancel(&mut self, id: PlaybackId) {
        self.calls.borrow_mut().push(Call::Cancel(id));
        if self.current.get() == Some(id) {
            self.current.set(None);
        }
    }
}

fn samples(count: usize) -> Pcm {
    Arc::from(vec![123; count])
}

fn ms(count: usize) -> Pcm {
    samples(count.saturating_mul(48))
}

fn request(class: AudioClass) -> Request {
    Request {
        instance_id: "instance_1".into(),
        event_type: (class != AudioClass::Voice).then(|| "event".into()),
        session_epoch: 1,
        class,
        episode: None,
        package_revision: (class != AudioClass::Voice).then_some(1),
        pcm: ms(100),
        visual_tail_ms: 0,
    }
}

fn owner() -> AudioOwner<RecordingSink> {
    let mut owner = AudioOwner::new(RecordingSink::default(), Config::default());
    owner.set_session(0, 1);
    owner.set_revision(0, 1);
    owner
}

fn wav(samples: &[i16]) -> Vec<u8> {
    let byte_len = u32::try_from(samples.len())
        .unwrap_or(u32::MAX)
        .saturating_mul(2);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&36u32.saturating_add(byte_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&96_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&byte_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

#[test]
fn earlier_visual_tail_is_cancelled_by_idle_channel_admission() {
    let mut owner = owner();
    let mut first = request(AudioClass::Voice);
    first.pcm = samples(48);
    first.visual_tail_ms = 100;
    owner.request(0, first);
    owner.tick(1);
    assert_eq!(owner.visual(50).unwrap().id, PlaybackId(1));

    assert_eq!(
        owner
            .request(50, request(AudioClass::Ordinary))
            .unwrap()
            .outcome,
        Outcome::Admitted
    );
    assert_eq!(
        owner.drain_notices(),
        vec![Notice::Cancelled {
            id: PlaybackId(1),
            reason: CancelReason::Preempted,
            at_ms: 50,
        }]
    );
}

#[test]
fn queued_correction_starts_at_critical_deadline_and_finishes_old_audio_first() {
    let mut owner = owner();
    let mut critical = request(AudioClass::Critical);
    critical.pcm = ms(5);
    owner.request(0, critical);
    let mut correction = request(AudioClass::Correction);
    correction.instance_id = "correction_1".into();
    assert!(owner.request(1, correction).is_none());

    owner.tick(100);
    let notices = owner.drain_notices();
    assert_eq!(
        notices.first(),
        Some(&Notice::Finished {
            id: PlaybackId(1),
            at_ms: 5,
        })
    );
    assert!(matches!(
        notices.get(1),
        Some(Notice::Decided(decision))
            if decision.instance_id == "correction_1"
                && decision.outcome == Outcome::Admitted
                && decision.decided_at_ms == 5
    ));
    assert_eq!(owner.active(100), Some(PlaybackId(2)));
}

#[test]
fn queued_correction_that_reaches_strict_expiry_does_not_start() {
    let mut owner = owner();
    let mut critical = request(AudioClass::Critical);
    critical.pcm = ms(3_002);
    owner.request(0, critical);
    assert!(owner.request(1, request(AudioClass::Correction)).is_none());

    owner.tick(5_000);
    let notices = owner.drain_notices();
    assert_eq!(
        notices.first(),
        Some(&Notice::Finished {
            id: PlaybackId(1),
            at_ms: 3_002,
        })
    );
    assert!(matches!(
        notices.get(1),
        Some(Notice::Decided(decision))
            if decision.outcome == Outcome::CriticalActive
                && decision.detail == Some(Detail::QueueExpired)
                && decision.decided_at_ms == 3_002
    ));
    assert_eq!(owner.active(5_000), None);
}

#[test]
fn queued_correction_does_not_expire_if_critical_frees_channel_before_ttl() {
    let mut owner = owner();
    let mut critical = request(AudioClass::Critical);
    critical.pcm = ms(3_001);
    owner.request(0, critical);
    let mut correction = request(AudioClass::Correction);
    correction.pcm = ms(10_000);
    assert!(owner.request(1, correction).is_none());

    owner.tick(5_000);
    assert!(matches!(
        owner.drain_notices().as_slice(),
        [
            Notice::Finished {
                id: PlaybackId(1),
                at_ms: 3_001,
            },
            Notice::Decided(decision),
        ] if decision.outcome == Outcome::Admitted && decision.decided_at_ms == 3_001
    ));
    assert_eq!(owner.active(5_000), Some(PlaybackId(2)));
}

#[test]
fn stale_sink_callback_does_not_advance_owner_time() {
    let mut owner = owner();
    owner.request(0, request(AudioClass::Voice));
    owner.on_sink_finished(10_000, PlaybackId(999));

    let decision = owner.request(50, request(AudioClass::Critical)).unwrap();
    assert_eq!(decision.decided_at_ms, 50);
    assert!(matches!(
        owner.drain_notices().as_slice(),
        [Notice::Cancelled {
            id: PlaybackId(1),
            reason: CancelReason::Preempted,
            at_ms: 50,
        }]
    ));
}

#[test]
fn failed_preemption_leaves_queued_correction_until_next_processing_call() {
    let mut owner = owner();
    let fail_next = owner.sink().fail_next.clone();
    let calls = owner.sink().calls.clone();
    let mut critical = request(AudioClass::Critical);
    critical.episode = Some("first".into());
    critical.pcm = ms(10_000);
    owner.request(0, critical);
    assert!(owner.request(1, request(AudioClass::Correction)).is_none());

    fail_next.set(true);
    let mut next_critical = request(AudioClass::Critical);
    next_critical.episode = Some("second".into());
    let failed = owner.request(2, next_critical).unwrap();
    assert_eq!(failed.outcome, Outcome::ChannelBusy);
    assert_eq!(failed.detail, Some(Detail::SinkFailed));
    assert_eq!(owner.active(2), None);
    assert!(owner.drain_notices().iter().all(|notice| !matches!(
        notice,
        Notice::Decided(decision) if decision.outcome == Outcome::Admitted
    )));

    owner.tick(3);
    assert_eq!(owner.active(3), Some(PlaybackId(3)));
    assert!(matches!(
        owner.drain_notices().as_slice(),
        [Notice::Decided(decision)]
            if decision.outcome == Outcome::Admitted && decision.decided_at_ms == 3
    ));
    assert_eq!(
        *calls.borrow(),
        vec![
            Call::Submit(PlaybackId(1)),
            Call::Cancel(PlaybackId(1)),
            Call::Submit(PlaybackId(2)),
            Call::Submit(PlaybackId(3)),
        ]
    );
}

#[test]
fn voice_decisions_normalize_request_identity_fields() {
    let mut owner = owner();
    let mut voice = request(AudioClass::Voice);
    voice.event_type = Some("carried_event".into());
    voice.package_revision = Some(99);
    let decision = owner.request(0, voice).unwrap();

    assert_eq!(decision.outcome, Outcome::Admitted);
    assert_eq!(decision.event_type, None);
    assert_eq!(decision.package_revision, None);
}

#[test]
fn visual_tail_is_clamped_to_three_point_two_seconds() {
    let mut owner = owner();
    let mut voice = request(AudioClass::Voice);
    voice.pcm = samples(48);
    voice.visual_tail_ms = 9_999;
    owner.request(0, voice);
    owner.tick(1);

    assert_eq!(owner.visual(3_200).unwrap().phase, VisualPhase::Tail);
    owner.tick(3_201);
    assert_eq!(owner.visual(3_201), None);
    assert_eq!(
        owner.drain_notices(),
        vec![Notice::Finished {
            id: PlaybackId(1),
            at_ms: 3_201,
        }]
    );
}

#[test]
fn short_nonempty_clips_hold_channel_for_one_millisecond() {
    assert_eq!(duration_ms(&[0; 47]), 0);
    for sample_count in [1, 47, 48] {
        let mut owner = owner();
        let mut voice = request(AudioClass::Voice);
        voice.pcm = samples(sample_count);
        owner.request(0, voice);

        assert_eq!(
            owner.active(0),
            Some(PlaybackId(1)),
            "{sample_count} samples"
        );
        owner.tick(1);
        assert_eq!(owner.active(1), None, "{sample_count} samples");
        assert_eq!(
            owner.drain_notices(),
            vec![Notice::Finished {
                id: PlaybackId(1),
                at_ms: 1,
            }],
            "{sample_count} samples"
        );
    }
}

#[test]
fn backwards_time_uses_the_last_observed_time() {
    let mut owner = owner();
    owner.request(100, request(AudioClass::Voice));
    assert_eq!(owner.active(150), Some(PlaybackId(1)));
    let decision = owner.request(1, request(AudioClass::Critical)).unwrap();

    assert_eq!(decision.decided_at_ms, 150);
    assert!(matches!(
        owner.drain_notices().as_slice(),
        [Notice::Cancelled {
            id: PlaybackId(1),
            reason: CancelReason::Preempted,
            at_ms: 150,
        }]
    ));
}

#[test]
fn nonempty_clip_at_max_time_has_no_overflowed_deadline() {
    let mut owner = owner();
    let mut voice = request(AudioClass::Voice);
    voice.pcm = samples(1);
    owner.request(u64::MAX, voice);
    owner.tick(u64::MAX);

    assert_eq!(owner.active(u64::MAX), Some(PlaybackId(1)));
    assert!(owner.drain_notices().is_empty());
}

#[test]
fn empty_wav_and_empty_request_are_rejected_without_submit() {
    assert!(decode_wav(&wav(&[])).is_none());

    let mut owner = owner();
    let calls = owner.sink().calls.clone();
    let mut voice = request(AudioClass::Voice);
    voice.pcm = samples(0);
    let decision = owner.request(0, voice).unwrap();
    assert_eq!(decision.outcome, Outcome::ChannelBusy);
    assert_eq!(decision.detail, Some(Detail::EmptyMedia));
    assert!(calls.borrow().is_empty());
    assert_eq!(owner.active(0), None);
}

#[test]
fn notice_queue_is_bounded_and_drop_count_survives_drain() {
    let mut owner = owner();
    for at_ms in 0..300 {
        owner.request(at_ms, request(AudioClass::Voice));
    }

    assert_eq!(owner.drain_notices().len(), 256);
    assert_eq!(owner.dropped_notices(), 43);
    assert!(owner.drain_notices().is_empty());
    assert_eq!(owner.dropped_notices(), 43);
}
