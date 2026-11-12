//! Native audio owner (M2, Sprint 3). API STUB: every body is `todo!()`; the red
//! tests of S3-2 and S3-4 are written against these signatures and S3-3 implements
//! them. Do not change a public name or signature without the lead.
//!
//! Sources: docs/design/EVENT-AUDIO-SPEC.md sections 4 (priority and preemption,
//! prewarm, mute) and 5 (outcome reasons); MEDIA-PACK-SPEC MP-02, MP-04, MP-08;
//! SRS MEDIA-005, ALERT-001, ALERT-007; contracts/audio-outcome.schema.json 1.0.0.
//!
//! Purity: no file, network, device, thread or clock access anywhere in this crate.
//! Time is the `at_ms` argument (monotonic milliseconds supplied by the caller).
//! The real output device is behind the [`Sink`] trait implemented elsewhere.

#![allow(unused_variables, dead_code)]

pub mod owner;
pub mod pcm;
pub mod readiness;

pub use owner::{
    AudioClass, AudioOwner, CancelReason, Config, Decision, Detail, Notice, Outcome, PlaybackId,
    Request, Sink, SinkError, VisualPhase, VisualState,
};
pub use readiness::{assess, CriticalMedia, Readiness, ReadinessInput, ReadinessState};
