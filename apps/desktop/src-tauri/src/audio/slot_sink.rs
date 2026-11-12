//! The sink slot owned by the audio owner (design section 1 and 8). API STUB until
//! S3-5f. Admission-path code: pure, no I/O, no lock, no panic on any input.

#![allow(dead_code, unused_variables)]

use kohbor_audio::{PlaybackId, Sink, SinkError, SinkEvent};

use kohbor_audio::pcm::Pcm;

/// A `Sink` that forwards to an optional boxed sink. With an empty slot `submit`
/// returns `Err(SinkError)` and `cancel` does nothing; `poll_event` returns `None`.
/// When the inner sink reports `SinkEvent::DeviceChanged`, the slot records that fact
/// (readable once through `take_changed`) and still passes the event on.
pub struct SlotSink {
    inner: Option<Box<dyn Sink + Send>>,
    changed: bool,
}

impl SlotSink {
    pub fn empty() -> SlotSink {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// Install `sink`; returns the previous one (so the caller can hand it off for dropping).
    pub fn install(&mut self, sink: Box<dyn Sink + Send>) -> Option<Box<dyn Sink + Send>> {
        todo!()
    }

    /// Remove and return the sink (the slot becomes empty).
    pub fn take(&mut self) -> Option<Box<dyn Sink + Send>> {
        todo!()
    }

    /// True once after the inner sink reported `DeviceChanged`; resets on read.
    pub fn take_changed(&mut self) -> bool {
        todo!()
    }
}

impl Sink for SlotSink {
    fn submit(&mut self, id: PlaybackId, pcm: Pcm) -> Result<(), SinkError> {
        todo!()
    }

    fn cancel(&mut self, id: PlaybackId) {
        todo!()
    }

    fn poll_event(&mut self) -> Option<SinkEvent> {
        todo!()
    }
}
