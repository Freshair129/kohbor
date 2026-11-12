use kohbor_audio::pcm::Pcm;
use kohbor_audio::{PlaybackId, Sink, SinkError};
use std::sync::Arc;

#[derive(Default)]
struct FakeSlotSink {
    current: Option<PlaybackId>,
}

impl Sink for FakeSlotSink {
    fn submit(&mut self, id: PlaybackId, _: Pcm) -> Result<(), SinkError> {
        self.current = Some(id);
        Ok(())
    }

    fn cancel(&mut self, id: PlaybackId) {
        if self.current == Some(id) {
            self.current = None;
        }
    }
}

fn pcm() -> Pcm {
    Arc::from(vec![1_i16].into_boxed_slice())
}

#[test]
fn submit_replaces_the_single_current_playback() {
    let mut sink = FakeSlotSink::default();
    assert_eq!(sink.submit(PlaybackId(1), pcm()), Ok(()));
    assert_eq!(sink.current, Some(PlaybackId(1)));

    assert_eq!(sink.submit(PlaybackId(2), pcm()), Ok(()));
    assert_eq!(sink.current, Some(PlaybackId(2)));
}

#[test]
fn cancelling_a_stale_id_is_a_no_op() {
    let mut sink = FakeSlotSink::default();
    sink.submit(PlaybackId(1), pcm()).expect("first submit");
    sink.submit(PlaybackId(2), pcm())
        .expect("replacement submit");

    sink.cancel(PlaybackId(1));
    assert_eq!(sink.current, Some(PlaybackId(2)));

    sink.cancel(PlaybackId(2));
    assert_eq!(sink.current, None);
}
