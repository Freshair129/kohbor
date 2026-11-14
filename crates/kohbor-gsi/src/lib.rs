//! Kohbor game-state intake core.
//!
//! Pure logic for docs/design/GSI-INTAKE-SPEC.md: strict body parsing, owned
//! configuration rendering, source admission and ownership, the own-snapshot
//! read model (contracts/own-snapshot.schema.json 2.1.0) and session continuity.
//! Network, file and registry access live in the callers.

pub mod admission;
pub mod config;
pub mod session;
pub mod snapshot;
pub mod strict_json;

pub use admission::{admit, steamid64_from_account_id, Outcome, Ownership, Phase};
pub use snapshot::{build as build_snapshot, SourceState, FRESH_MAX_AGE_MS};
