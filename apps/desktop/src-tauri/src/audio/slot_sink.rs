//! The sink slot owned by the audio owner. It forwards bounded calls to its
//! optional sink and retains only the most recent default-device change flag.

#![allow(dead_code, unused_variables)]

use kohbor_audio::pcm::Pcm;
use kohbor_audio::{PlaybackId, Sink, SinkError, SinkEvent};

/// A `Sink` that forwards to an optional boxed sink. An empty slot returns
/// `Err(SinkError)` from `submit`, ignores `cancel`, and returns `None` from
/// `poll_event`. A `DeviceChanged` event is recorded once and still forwarded.
pub struct SlotSink {
    inner: Option<Box<dyn Sink + Send>>,
    changed: bool,
}

impl SlotSink {
    /// Create an empty slot. Its `submit` fails until a sink is installed.
    pub fn empty() -> SlotSink {
        SlotSink {
            inner: None,
            changed: false,
        }
    }

    /// Whether the slot currently has an inner sink.
    pub fn is_empty(&self) -> bool {
        self.inner.is_none()
    }

    /// Install `sink`; return the previous sink so it can be handed off for dropping.
    pub fn install(&mut self, sink: Box<dyn Sink + Send>) -> Option<Box<dyn Sink + Send>> {
        self.changed = false;
        self.inner.replace(sink)
    }

    /// Remove and return the inner sink.
    pub fn take(&mut self) -> Option<Box<dyn Sink + Send>> {
        self.changed = false;
        self.inner.take()
    }

    /// Return the default-device change flag once, then clear it.
    pub fn take_changed(&mut self) -> bool {
        std::mem::replace(&mut self.changed, false)
    }
}

impl Sink for SlotSink {
    fn submit(&mut self, id: PlaybackId, pcm: Pcm) -> Result<(), SinkError> {
        if let Some(inner) = self.inner.as_mut() {
            inner.submit(id, pcm)
        } else {
            Err(SinkError)
        }
    }

    fn cancel(&mut self, id: PlaybackId) {
        if let Some(inner) = self.inner.as_mut() {
            inner.cancel(id);
        }
    }

    fn poll_event(&mut self) -> Option<SinkEvent> {
        let event = self.inner.as_mut().and_then(|inner| inner.poll_event());
        if event == Some(SinkEvent::DeviceChanged) {
            self.changed = true;
        }
        event
    }
}
