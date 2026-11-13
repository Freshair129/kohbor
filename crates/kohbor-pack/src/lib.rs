//! Verified media package cache (M2, Sprint 2).
//!
//! API STUB. Every body is `todo!()`; the red tests of S2-1, S2-2 and S2-5 are
//! written against these signatures and S2-3 implements them. Do not change a
//! public name or signature without the lead.
//!
//! Sources: docs/design/MEDIA-PACK-SPEC.md (MP-03, MP-05, MP-06, MP-07, MP-09),
//! docs/design/EVENT-AUDIO-SPEC.md sections 6 and 7, contracts/media-manifest.schema.json
//! 1.3.0, media-install-request/-result.schema.json 1.0.0.
//!
//! Layering (all in this crate, no network, no clock, no threads):
//! - [`catalog`]: the event catalog as the package reader needs it.
//! - [`descriptor`]: strict descriptor parsing and semantic rules (no archive I/O).
//! - [`media`]: WAV and WebM profile inspection of in-memory bytes.
//! - [`container`]: strict indexed-ZIP reading and bounded extraction into staging.
//! - [`trust`]: authority, publisher claim and signature status (DEC-PACK-CRYPTO is open).
//! - [`store`]: install, activation and registry transactions with crash-injection steps.

#![allow(unused_variables, dead_code)]

pub mod catalog;
pub mod container;
pub mod descriptor;
pub mod error;
mod fsutil;
pub mod limits;
pub mod media;
pub mod store;
pub mod trust;

pub use error::RejectReason;
pub use limits::Limits;
