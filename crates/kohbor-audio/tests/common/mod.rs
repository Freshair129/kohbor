#![allow(dead_code)]
use kohbor_audio::pcm::Pcm;
use kohbor_audio::{AudioClass, AudioOwner, Config, PlaybackId, Request, Sink, SinkError};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Submit(PlaybackId),
    Cancel(PlaybackId),
}
#[derive(Default)]
pub struct FakeSink {
    pub calls: Rc<RefCell<Vec<Call>>>,
    pub fail: Rc<Cell<bool>>,
}
impl Sink for FakeSink {
    fn submit(&mut self, id: PlaybackId, _: Pcm) -> Result<(), SinkError> {
        self.calls.borrow_mut().push(Call::Submit(id));
        if self.fail.replace(false) {
            Err(SinkError)
        } else {
            Ok(())
        }
    }
    fn cancel(&mut self, id: PlaybackId) {
        self.calls.borrow_mut().push(Call::Cancel(id));
    }
}
pub fn samples(n: usize) -> Pcm {
    Arc::from(vec![123; n])
}
pub fn ms(n: usize) -> Pcm {
    samples(n * 48)
}
pub fn request(class: AudioClass) -> Request {
    Request {
        instance_id: "instance_1".into(),
        event_type: if class == AudioClass::Voice {
            None
        } else {
            Some("event".into())
        },
        session_epoch: 1,
        class,
        episode: None,
        package_revision: if class == AudioClass::Voice {
            None
        } else {
            Some(1)
        },
        pcm: ms(100),
        visual_tail_ms: 0,
    }
}
pub fn owner() -> AudioOwner<FakeSink> {
    let mut owner = AudioOwner::new(FakeSink::default(), Config::default());
    owner.set_session(0, 1);
    owner.set_revision(0, 1);
    owner
}
pub fn wav(samples: &[i16]) -> Vec<u8> {
    let len = (samples.len() * 2) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&96_000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}
pub fn schema(path: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts")
            .join(path),
    )
    .unwrap()
}
pub fn enum_values(text: &str, property: &str) -> Vec<String> {
    let start = text.find(&format!("\"{property}\": {{")).expect("property");
    let rest = &text[start..];
    let start = rest.find("\"enum\"").expect("enum");
    let rest = &rest[start..];
    let start = rest.find('[').unwrap();
    let end = rest.find(']').unwrap();
    rest[start + 1..end]
        .split(',')
        .filter_map(|s| {
            let s = s.trim();
            s.strip_prefix('"')
                .and_then(|x| x.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect()
}
