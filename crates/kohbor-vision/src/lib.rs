//! Pure enemy observation boundary. See docs/design/S4-OBSERVATION-BOUNDARY.md section 4.2.

use std::collections::{BTreeMap, BTreeSet};

use kohbor_capture::{AdmittedFrame, FRESH_MAX_MS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Fresh,
    Stale,
    Unknown,
    Invalid,
    Unsupported,
}

/// A value with an explicit availability state; unknown states never manufacture a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<T> {
    value: Option<T>,
    availability: Availability,
}

impl<T> Field<T> {
    pub fn fresh(value: T) -> Self {
        Self::with_value(value, Availability::Fresh)
    }

    pub fn stale(value: T) -> Self {
        Self::with_value(value, Availability::Stale)
    }

    pub fn unknown() -> Self {
        Self::without_value(Availability::Unknown)
    }

    pub fn invalid() -> Self {
        Self::without_value(Availability::Invalid)
    }

    pub fn unsupported() -> Self {
        Self::without_value(Availability::Unsupported)
    }

    pub fn value(&self) -> Option<&T> {
        self.value.as_ref()
    }

    pub fn availability(&self) -> Availability {
        self.availability
    }

    fn with_value(value: T, availability: Availability) -> Self {
        Self {
            value: Some(value),
            availability,
        }
    }

    fn without_value(availability: Availability) -> Self {
        Self {
            value: None,
            availability,
        }
    }
}

/// Integer thousandths (0..=1000) of the calibrated rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapPoint {
    pub x_milli: u16,
    pub y_milli: u16,
}

impl MapPoint {
    fn is_within_minimap(&self) -> bool {
        self.x_milli <= 1000 && self.y_milli <= 1000
    }
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

/// Labels only facts tied to the requested patch and sufficient catalog provenance.
pub fn fact_label(provenance: &CatalogProvenance, requested_patch: &str) -> FactLabel {
    if provenance.patch.as_deref() != Some(requested_patch)
        || provenance.coverage == CoverageStatus::Unsupported
    {
        return FactLabel::Unsupported;
    }
    if provenance.coverage == CoverageStatus::Partial || provenance.source_hash.is_none() {
        return FactLabel::Partial;
    }
    FactLabel::Verified
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertifiedClasses {
    classes: BTreeSet<ClassId>,
}

impl CertifiedClasses {
    pub fn none() -> Self {
        Self {
            classes: BTreeSet::new(),
        }
    }

    /// Uses classes only when patch, hash, and coverage provenance are all verified.
    pub fn from_provenance(provenance: &CatalogProvenance, classes: &[ClassId]) -> Self {
        if provenance.coverage != CoverageStatus::Verified
            || provenance.patch.is_none()
            || provenance.source_hash.is_none()
        {
            return Self::none();
        }
        Self {
            classes: classes.iter().cloned().collect(),
        }
    }

    pub fn contains(&self, class: &ClassId) -> bool {
        self.classes.contains(class)
    }

    pub fn len(&self) -> usize {
        self.classes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityState {
    Unknown,
    Unverified,
    Certified(ClassId),
}

/// Leaves absent classes unknown and unlisted classes unverified without carrying their value.
pub fn classify(detected: Option<&ClassId>, certified: &CertifiedClasses) -> IdentityState {
    match detected {
        None => IdentityState::Unknown,
        Some(class) if certified.contains(class) => IdentityState::Certified(class.clone()),
        Some(_) => IdentityState::Unverified,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    NeverSeen,
    Visible,
    Absent { covered_ms: u64 },
    Interrupted,
}

/// Deliberately omits hp, mana, items, levels, and abilities (AC-DATA-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnemyObservation {
    pub track: TrackId,
    pub position: Field<MapPoint>,
    pub identity: IdentityState,
    pub last_seen_ms: Option<u64>,
    pub presence: Presence,
}

impl EnemyObservation {
    /// Creates the explicit unknown state for a track with no admitted sighting.
    pub fn unobserved(track: TrackId) -> Self {
        Self {
            track,
            position: Field::unknown(),
            identity: IdentityState::Unknown,
            last_seen_ms: None,
            presence: Presence::NeverSeen,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    pub track: TrackId,
    pub position: MapPoint,
    pub class: Option<ClassId>,
}

#[derive(Debug, Clone)]
struct TrackState {
    position: MapPoint,
    identity: IdentityState,
    last_seen_ms: u64,
    presence: Presence,
}

#[derive(Debug)]
pub struct Tracker {
    classes: CertifiedClasses,
    tracks: BTreeMap<TrackId, TrackState>,
    frame_identity: Option<(u64, u64)>,
    frame_stream_id: Option<u64>,
    previous_frame_ms: Option<u64>,
    last_frame_token: Option<(u64, u64, u64, u64, u64, u64)>,
    coverage_lost_at_ms: Option<u64>,
}

impl Tracker {
    pub fn new(classes: CertifiedClasses) -> Self {
        Self {
            classes,
            tracks: BTreeMap::new(),
            frame_identity: None,
            frame_stream_id: None,
            previous_frame_ms: None,
            last_frame_token: None,
            coverage_lost_at_ms: None,
        }
    }

    /// Applies only sightings supplied alongside an admitted frame.
    pub fn on_admitted_frame(&mut self, frame: &AdmittedFrame, sightings: &[Sighting]) {
        let frame_identity = (frame.session_epoch(), frame.generation());
        let stream_id = frame.stream_id();
        let admission_order = frame.admission_order();
        let frame_ms = frame.acquired_at_ms();
        let frame_sequence = frame.sequence();
        let identity_changed =
            self.frame_identity != Some(frame_identity) || self.frame_stream_id != Some(stream_id);

        if let Some((
            previous_stream_id,
            previous_session,
            previous_generation,
            previous_sequence,
            previous_ms,
            previous_order,
        )) = self.last_frame_token
        {
            if frame_ms < previous_ms {
                return;
            }
            if stream_id == previous_stream_id
                && (admission_order <= previous_order
                    || (frame_identity == (previous_session, previous_generation)
                        && frame_sequence <= previous_sequence))
            {
                return;
            }
            if stream_id != previous_stream_id && frame_ms == previous_ms {
                // Controller changes at the same millisecond are ambiguous; clear old state but
                // wait for a later frame before admitting new observations.
                self.tracks.clear();
                self.frame_identity = Some(frame_identity);
                self.frame_stream_id = Some(stream_id);
                self.previous_frame_ms = None;
                self.last_frame_token = Some((
                    stream_id,
                    frame_identity.0,
                    frame_identity.1,
                    frame_sequence,
                    frame_ms,
                    admission_order,
                ));
                return;
            }
        }

        if self
            .coverage_lost_at_ms
            .is_some_and(|lost_at_ms| frame_ms <= lost_at_ms)
        {
            if identity_changed {
                self.tracks.clear();
                self.frame_identity = Some(frame_identity);
                self.frame_stream_id = Some(stream_id);
                self.previous_frame_ms = None;
                self.last_frame_token = Some((
                    stream_id,
                    frame_identity.0,
                    frame_identity.1,
                    frame_sequence,
                    frame_ms,
                    admission_order,
                ));
            }
            return;
        }

        if identity_changed {
            self.tracks.clear();
            self.frame_identity = Some(frame_identity);
            self.frame_stream_id = Some(stream_id);
            self.previous_frame_ms = None;
        }

        let delta_ms = self
            .previous_frame_ms
            .and_then(|previous_ms| frame_ms.checked_sub(previous_ms));
        let valid_sightings: Vec<_> = sightings
            .iter()
            .filter(|sighting| sighting.position.is_within_minimap())
            .collect();
        let valid_track_ids: BTreeSet<_> = valid_sightings
            .iter()
            .map(|sighting| sighting.track)
            .collect();
        let invalid_track_ids: BTreeSet<_> = sightings
            .iter()
            .filter(|sighting| !sighting.position.is_within_minimap())
            .map(|sighting| sighting.track)
            .collect();

        for (track_id, state) in &mut self.tracks {
            if valid_track_ids.contains(track_id) {
                continue;
            }
            if invalid_track_ids.contains(track_id) {
                state.presence = Presence::Interrupted;
                continue;
            }

            match delta_ms {
                Some(delta) if delta <= FRESH_MAX_MS => {
                    let previous_coverage = match state.presence {
                        Presence::Absent { covered_ms } => covered_ms,
                        Presence::Visible | Presence::Interrupted | Presence::NeverSeen => 0,
                    };
                    let covered_ms = previous_coverage.saturating_add(delta);
                    state.presence = Presence::Absent { covered_ms };
                }
                Some(_) => state.presence = Presence::Interrupted,
                None if self.previous_frame_ms.is_some() => {
                    // A regressed injected timestamp interrupts coverage and establishes a new
                    // baseline for subsequent frames.
                    state.presence = Presence::Interrupted;
                }
                None => {}
            }
        }

        for sighting in valid_sightings {
            self.tracks.insert(
                sighting.track,
                TrackState {
                    position: sighting.position,
                    identity: classify(sighting.class.as_ref(), &self.classes),
                    last_seen_ms: frame_ms,
                    presence: Presence::Visible,
                },
            );
        }

        self.previous_frame_ms = Some(frame_ms);
        self.last_frame_token = Some((
            stream_id,
            frame_identity.0,
            frame_identity.1,
            frame_sequence,
            frame_ms,
            admission_order,
        ));
        self.coverage_lost_at_ms = None;
    }

    /// Interrupts all seen tracks and makes the next admitted frame a new coverage baseline.
    pub fn on_coverage_lost(&mut self, now_ms: u64) {
        for state in self.tracks.values_mut() {
            state.presence = Presence::Interrupted;
        }
        self.previous_frame_ms = None;
        let last_frame_ms = self
            .last_frame_token
            .map(|(_, _, _, _, frame_ms, _)| frame_ms);
        self.coverage_lost_at_ms = Some(
            self.coverage_lost_at_ms
                .into_iter()
                .chain(last_frame_ms)
                .fold(now_ms, u64::max),
        );
    }

    pub fn observation(&self, track: TrackId) -> EnemyObservation {
        let Some(state) = self.tracks.get(&track) else {
            return EnemyObservation::unobserved(track);
        };
        let position = match state.presence {
            Presence::Visible => Field::fresh(state.position),
            Presence::NeverSeen | Presence::Absent { .. } | Presence::Interrupted => {
                Field::stale(state.position)
            }
        };
        EnemyObservation {
            track,
            position,
            identity: state.identity.clone(),
            last_seen_ms: Some(state.last_seen_ms),
            presence: state.presence,
        }
    }

    pub fn tracks(&self) -> Vec<TrackId> {
        self.tracks.keys().copied().collect()
    }

    /// Returns only tracks with strictly more than the requested continuous coverage.
    pub fn absent_beyond(&self, min_covered_ms: u64) -> Vec<TrackId> {
        self.tracks
            .iter()
            .filter_map(|(track_id, state)| match state.presence {
                Presence::Absent { covered_ms } if covered_ms > min_covered_ms => Some(*track_id),
                Presence::NeverSeen
                | Presence::Visible
                | Presence::Absent { .. }
                | Presence::Interrupted => None,
            })
            .collect()
    }
}
