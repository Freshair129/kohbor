//! Pure enemy observation boundary. See docs/design/S4-OBSERVATION-BOUNDARY.md section 4.2.
//! STUB: signatures are the contract for the S4-T1 tests; bodies are `todo!()` until S4-L1.
#![allow(unused_variables, dead_code)]

use kohbor_capture::AdmittedFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Fresh,
    Stale,
    Unknown,
    Invalid,
    Unsupported,
}

/// `value()` is Some exactly when availability is Fresh or Stale. No Default, no From<T>.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<T> {
    _private: Option<T>,
}

impl<T> Field<T> {
    pub fn fresh(value: T) -> Self {
        todo!()
    }
    pub fn stale(value: T) -> Self {
        todo!()
    }
    pub fn unknown() -> Self {
        todo!()
    }
    pub fn invalid() -> Self {
        todo!()
    }
    pub fn unsupported() -> Self {
        todo!()
    }
    pub fn value(&self) -> Option<&T> {
        todo!()
    }
    pub fn availability(&self) -> Availability {
        todo!()
    }
}

/// Integer thousandths (0..=1000) of the calibrated rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapPoint {
    pub x_milli: u16,
    pub y_milli: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrackId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageStatus {
    Verified,
    Partial,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogProvenance {
    pub patch: Option<String>,
    pub source_hash: Option<String>,
    pub coverage: CoverageStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactLabel {
    Verified,
    Partial,
    Unsupported,
}

/// No patch or a different patch => Unsupported; else coverage Unsupported => Unsupported;
/// Partial coverage or missing hash => Partial; else Verified.
pub fn fact_label(provenance: &CatalogProvenance, requested_patch: &str) -> FactLabel {
    todo!()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertifiedClasses {
    _private: Vec<ClassId>,
}

impl CertifiedClasses {
    pub fn none() -> Self {
        todo!()
    }
    /// Empty unless provenance.coverage is Verified and a patch and a hash are present.
    pub fn from_provenance(provenance: &CatalogProvenance, classes: &[ClassId]) -> Self {
        todo!()
    }
    pub fn contains(&self, class: &ClassId) -> bool {
        todo!()
    }
    pub fn len(&self) -> usize {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityState {
    Unknown,
    Unverified,
    Certified(ClassId),
}

/// None => Unknown; in set => Certified; otherwise Unverified (carries no class).
pub fn classify(detected: Option<&ClassId>, certified: &CertifiedClasses) -> IdentityState {
    todo!()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    NeverSeen,
    Visible,
    Absent { covered_ms: u64 },
    Interrupted,
}

/// Deliberately no hp, mana, item, level or ability field (AC-DATA-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnemyObservation {
    pub track: TrackId,
    pub position: Field<MapPoint>,
    pub identity: IdentityState,
    pub last_seen_ms: Option<u64>,
    pub presence: Presence,
}

impl EnemyObservation {
    /// position unknown, identity Unknown, last_seen None, presence NeverSeen.
    pub fn unobserved(track: TrackId) -> Self {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    pub track: TrackId,
    pub position: MapPoint,
    pub class: Option<ClassId>,
}

#[derive(Debug)]
pub struct Tracker {
    _private: (),
}

impl Tracker {
    pub fn new(classes: CertifiedClasses) -> Self {
        todo!()
    }
    /// A frame whose session_epoch or generation differs from the tracker's current pair clears
    /// all tracks first (the first frame ever just sets the pair). Sighted tracks: Visible,
    /// position Fresh, last_seen_ms = frame time, identity = classify(). Seen tracks not sighted:
    /// if frame time - previous frame time <= 2000 (and no coverage loss since) the track becomes
    /// Absent{covered_ms += that delta} with position Stale (last point); if the delta is larger
    /// the track becomes Interrupted and covered_ms restarts at 0 from this frame.
    pub fn on_admitted_frame(&mut self, frame: &AdmittedFrame, sightings: &[Sighting]) {
        todo!()
    }
    /// Every seen track becomes Interrupted (position Stale), covered_ms reset, and the next
    /// admitted frame is a fresh baseline (no accrual for the interval up to it).
    pub fn on_coverage_lost(&mut self, now_ms: u64) {
        todo!()
    }
    pub fn observation(&self, track: TrackId) -> EnemyObservation {
        todo!()
    }
    pub fn tracks(&self) -> Vec<TrackId> {
        todo!()
    }
    /// Tracks that are Absent with covered_ms strictly greater than min_covered_ms.
    pub fn absent_beyond(&self, min_covered_ms: u64) -> Vec<TrackId> {
        todo!()
    }
}
