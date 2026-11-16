//! Kohbor Lab: the detector profile (docs/design/KOHBOR-LAB.md sections 2 to 4).
//!
//! Pure logic. A profile is a bounded set of the catalog tunables of the existing native
//! detectors. The embedded default ships in every build; a dev-only override file can
//! replace it only in builds that compile the `dev_override` module in (feature `lab` or
//! debug assertions) and only when the environment asks for it. Release builds contain
//! no code that reads an override (tests/release_gate.rs).

mod profile;

#[cfg(any(feature = "lab", debug_assertions))]
pub mod dev_override;

pub mod sandbox;

pub use profile::{
    embedded, embedded_sha256, merge_override, BannerLayout, Effective, HudLayout, Layout,
    ProfileError, HUD_MODULE_IDS, PERCENT_BOUNDS, SCALE_BOUNDS,
};

use std::path::PathBuf;

/// Where the effective profile came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Embedded,
    Override(PathBuf),
}

/// The effective profile plus why it is what it is.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub effective: Effective,
    pub source: Source,
    /// An override was asked for but refused: the embedded default is in force.
    pub rejected: Option<ProfileError>,
}

impl Resolved {
    pub fn embedded() -> Self {
        Self {
            effective: embedded(),
            source: Source::Embedded,
            rejected: None,
        }
    }
}

#[cfg(any(feature = "lab", debug_assertions))]
pub fn resolve_effective() -> Resolved {
    dev_override::resolve_from_env()
}

#[cfg(not(any(feature = "lab", debug_assertions)))]
pub fn resolve_effective() -> Resolved {
    Resolved::embedded()
}
