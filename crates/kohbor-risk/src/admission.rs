//! Proposed, unwired risk-admission evaluator.
//!
//! DEC-RISK-DETAIL is still open. This module can produce synthetic proposal
//! events for replay, but the audible decision remains unavailable.

use kohbor_capture::Coverage;
use std::collections::{BTreeMap, BTreeSet};

/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 1.
/// Keep false until the owner approves the decision.
pub const AUDIBLE_MISSING_GANK_APPROVED: bool = false;

/// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.1.
const DEFAULT_RAMP_START_S: f64 = 5.0;
/// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.1.
const DEFAULT_PEAK_S: f64 = 12.0;
/// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.1.
const DEFAULT_PEAK_RISK: f64 = 0.70;
/// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.1.
const DEFAULT_DECAY_PER_S: f64 = 0.03;
/// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.1.
const DEFAULT_FLOOR: f64 = 0.10;
/// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.3.
const DEFAULT_MULTI_BOOST: f64 = 1.15;
/// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.2.
const DEFAULT_HEADING_AMP: f64 = 0.22;
/// INHERITED: ALERT-003, medium sensitivity, section 6.3.
const DEFAULT_DANGER: f64 = 0.65;
/// INHERITED: SDD.md section 6, correction hysteresis, section 7.
const DEFAULT_REVISION: f64 = 0.40;
/// INHERITED: ALERT-004 global 30-second interval, section 7.
const DEFAULT_GLOBAL_COOLDOWN_MS: u64 = 30_000;
/// INHERITED: ALERT-004 per-contributor 60-second interval, section 7.
const DEFAULT_CONTRIBUTOR_COOLDOWN_MS: u64 = 60_000;
/// INHERITED: VISION-002 observation gap, section 3.4.
const DEFAULT_OBSERVATION_GAP_MS: u64 = 2_000;
/// INHERITED: GSI freshness limit, section 2.
const DEFAULT_GSI_FRESH_MS: u64 = 2_000;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 3.2.
const DEFAULT_REAPPEAR_MIN_FRAMES: u64 = 3;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 3.2.
const DEFAULT_REAPPEAR_MIN_MS: u64 = 500;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 4.2.
const DEFAULT_HEADING_WINDOW_MS: u64 = 1_500;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 4.2.
const DEFAULT_HEADING_MIN_SIGHTINGS: u64 = 3;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 4.2.
const DEFAULT_HEADING_MIN_SPAN_MS: u64 = 500;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 4.2.
const DEFAULT_HEADING_MIN_DISPLACEMENT: f64 = 20.0;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 4.2.
const DEFAULT_CENTER_DEADZONE: f64 = 20.0;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 5.1.
const DEFAULT_OWN_RESPAWN_GRACE_MS: u64 = 5_000;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 5.2.
const DEFAULT_KILL_PARTICIPATION_WINDOW_MS: u64 = 3_000;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.2.
const DEFAULT_COORD_MIN_MEMBERS: u64 = 2;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.2.
const DEFAULT_COORD_VANISH_WINDOW_MS: u64 = 3_000;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.2.
const DEFAULT_COORD_RADIUS: f64 = 120.0;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.2.
const DEFAULT_COORD_HEADING_COS: f64 = 0.5;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.2.
const DEFAULT_COORD_MAX_AGE_MS: u64 = 15_000;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.4.
const DEFAULT_PLAYER_RELATIVE_GATE: bool = true;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.4.
const DEFAULT_TOWARD_PLAYER_COS: f64 = 0.5;
/// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.4.
const DEFAULT_PROXIMITY_RADIUS: f64 = 250.0;
/// INHERITED: existing low-sensitivity danger threshold, section 6.3.
const DEFAULT_HYSTERESIS_LOW_DANGER: f64 = 0.85;
/// INHERITED: existing low-sensitivity revision threshold, section 7.
const DEFAULT_HYSTERESIS_LOW_REVISION: f64 = 0.50;
/// INHERITED: existing high-sensitivity danger threshold, section 6.3.
const DEFAULT_HYSTERESIS_HIGH_DANGER: f64 = 0.50;
/// INHERITED: existing high-sensitivity revision threshold, section 7.
const DEFAULT_HYSTERESIS_HIGH_REVISION: f64 = 0.30;
/// INHERITED: five-minute visible history, section 3.4.
const DEFAULT_HISTORY_MS: u64 = 300_000;

/// Parameters for the proposed evaluator. Defaults are either inherited
/// baselines or explicitly marked proposals above.
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    /// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.1.
    pub ramp_start_s: f64,
    /// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.1.
    pub peak_s: f64,
    /// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.1.
    pub peak_risk: f64,
    /// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.1.
    pub decay_per_s: f64,
    /// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.1.
    pub floor: f64,
    /// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.3.
    pub multi_boost: f64,
    /// INHERITED: FEATURE-CATALOG.md, “Missing-time heuristic”, section 4.2.
    pub heading_amp: f64,
    /// INHERITED: ALERT-003, medium sensitivity, section 6.3.
    pub danger: f64,
    /// INHERITED: SDD.md section 6, correction hysteresis, section 7.
    pub revision: f64,
    /// INHERITED: ALERT-004 global 30-second interval, section 7.
    pub global_cooldown_ms: u64,
    /// INHERITED: ALERT-004 per-contributor 60-second interval, section 7.
    pub contributor_cooldown_ms: u64,
    /// INHERITED: VISION-002 observation gap, section 3.4.
    pub observation_gap_ms: u64,
    /// INHERITED: GSI freshness limit, section 2.
    pub gsi_fresh_ms: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 3.2.
    pub reappear_min_frames: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 3.2.
    pub reappear_min_ms: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 4.2.
    pub heading_window_ms: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 4.2.
    pub heading_min_sightings: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 4.2.
    pub heading_min_span_ms: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 4.2.
    pub heading_min_displacement: f64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 4.2.
    pub center_deadzone: f64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 5.1.
    pub own_respawn_grace_ms: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 5.2.
    pub kill_participation_window_ms: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.2.
    pub coord_min_members: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.2.
    pub coord_vanish_window_ms: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.2.
    pub coord_radius: f64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.2.
    pub coord_heading_cos: f64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.2.
    pub coord_max_age_ms: u64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.4.
    pub player_relative_gate: bool,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.4.
    pub toward_player_cos: f64,
    /// PROPOSED (DEC-RISK-DETAIL, not approved): RISK-ADMISSION-SPEC section 6.4.
    pub proximity_radius: f64,
    /// INHERITED: existing low-sensitivity danger threshold, section 6.3.
    pub hysteresis_low_danger: f64,
    /// INHERITED: existing low-sensitivity revision threshold, section 7.
    pub hysteresis_low_revision: f64,
    /// INHERITED: existing high-sensitivity danger threshold, section 6.3.
    pub hysteresis_high_danger: f64,
    /// INHERITED: existing high-sensitivity revision threshold, section 7.
    pub hysteresis_high_revision: f64,
    /// INHERITED: five-minute visible history, section 3.4.
    pub history_ms: u64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            ramp_start_s: DEFAULT_RAMP_START_S,
            peak_s: DEFAULT_PEAK_S,
            peak_risk: DEFAULT_PEAK_RISK,
            decay_per_s: DEFAULT_DECAY_PER_S,
            floor: DEFAULT_FLOOR,
            multi_boost: DEFAULT_MULTI_BOOST,
            heading_amp: DEFAULT_HEADING_AMP,
            danger: DEFAULT_DANGER,
            revision: DEFAULT_REVISION,
            global_cooldown_ms: DEFAULT_GLOBAL_COOLDOWN_MS,
            contributor_cooldown_ms: DEFAULT_CONTRIBUTOR_COOLDOWN_MS,
            observation_gap_ms: DEFAULT_OBSERVATION_GAP_MS,
            gsi_fresh_ms: DEFAULT_GSI_FRESH_MS,
            reappear_min_frames: DEFAULT_REAPPEAR_MIN_FRAMES,
            reappear_min_ms: DEFAULT_REAPPEAR_MIN_MS,
            heading_window_ms: DEFAULT_HEADING_WINDOW_MS,
            heading_min_sightings: DEFAULT_HEADING_MIN_SIGHTINGS,
            heading_min_span_ms: DEFAULT_HEADING_MIN_SPAN_MS,
            heading_min_displacement: DEFAULT_HEADING_MIN_DISPLACEMENT,
            center_deadzone: DEFAULT_CENTER_DEADZONE,
            own_respawn_grace_ms: DEFAULT_OWN_RESPAWN_GRACE_MS,
            kill_participation_window_ms: DEFAULT_KILL_PARTICIPATION_WINDOW_MS,
            coord_min_members: DEFAULT_COORD_MIN_MEMBERS,
            coord_vanish_window_ms: DEFAULT_COORD_VANISH_WINDOW_MS,
            coord_radius: DEFAULT_COORD_RADIUS,
            coord_heading_cos: DEFAULT_COORD_HEADING_COS,
            coord_max_age_ms: DEFAULT_COORD_MAX_AGE_MS,
            player_relative_gate: DEFAULT_PLAYER_RELATIVE_GATE,
            toward_player_cos: DEFAULT_TOWARD_PLAYER_COS,
            proximity_radius: DEFAULT_PROXIMITY_RADIUS,
            hysteresis_low_danger: DEFAULT_HYSTERESIS_LOW_DANGER,
            hysteresis_low_revision: DEFAULT_HYSTERESIS_LOW_REVISION,
            hysteresis_high_danger: DEFAULT_HYSTERESIS_HIGH_DANGER,
            hysteresis_high_revision: DEFAULT_HYSTERESIS_HIGH_REVISION,
            history_ms: DEFAULT_HISTORY_MS,
        }
    }
}

/// A typed override value for [`Params::set`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    Number(f64),
    Bool(bool),
}

impl Params {
    /// Set a parameter by its exact uppercase fixture key.
    ///
    /// Unknown names, wrong value types, non-finite numbers, values outside
    /// their supported ranges, and numeric values that cannot be represented
    /// by an integer parameter return an error without changing the parameter
    /// set.
    pub fn set(&mut self, name: &str, value: ParamValue) -> Result<(), String> {
        let mut candidate = self.clone();
        candidate.set_unchecked(name, value)?;
        if !candidate.is_valid() {
            return Err(format!("out-of-range value for {name}"));
        }
        *self = candidate;
        Ok(())
    }

    fn set_unchecked(&mut self, name: &str, value: ParamValue) -> Result<(), String> {
        if name == "PLAYER_RELATIVE_GATE" {
            return match value {
                ParamValue::Bool(value) => {
                    self.player_relative_gate = value;
                    Ok(())
                }
                ParamValue::Number(_) => Err(format!("wrong type for {name}")),
            };
        }
        let number = match value {
            ParamValue::Number(value) if value.is_finite() => value,
            ParamValue::Number(_) => return Err(format!("non-finite value for {name}")),
            ParamValue::Bool(_) => return Err(format!("wrong type for {name}")),
        };
        macro_rules! set_float {
            ($field:ident) => {{
                self.$field = number;
                Ok(())
            }};
        }
        macro_rules! set_integer {
            ($field:ident) => {{
                let Some(integer) = exact_u64(number) else {
                    return Err(format!("{name} must be a non-negative integer"));
                };
                self.$field = integer;
                Ok(())
            }};
        }
        match name {
            "RAMP_START_S" => set_float!(ramp_start_s),
            "PEAK_S" => set_float!(peak_s),
            "PEAK_RISK" => set_float!(peak_risk),
            "DECAY_PER_S" => set_float!(decay_per_s),
            "FLOOR" => set_float!(floor),
            "MULTI_BOOST" => set_float!(multi_boost),
            "HEADING_AMP" => set_float!(heading_amp),
            "DANGER" => set_float!(danger),
            "REVISION" => set_float!(revision),
            "GLOBAL_COOLDOWN_MS" => set_integer!(global_cooldown_ms),
            "CONTRIBUTOR_COOLDOWN_MS" => set_integer!(contributor_cooldown_ms),
            "OBSERVATION_GAP_MS" => set_integer!(observation_gap_ms),
            "GSI_FRESH_MS" => set_integer!(gsi_fresh_ms),
            "REAPPEAR_MIN_FRAMES" => set_integer!(reappear_min_frames),
            "REAPPEAR_MIN_MS" => set_integer!(reappear_min_ms),
            "HEADING_WINDOW_MS" => set_integer!(heading_window_ms),
            "HEADING_MIN_SIGHTINGS" => set_integer!(heading_min_sightings),
            "HEADING_MIN_SPAN_MS" => set_integer!(heading_min_span_ms),
            "HEADING_MIN_DISPLACEMENT" => set_float!(heading_min_displacement),
            "CENTER_DEADZONE" => set_float!(center_deadzone),
            "OWN_RESPAWN_GRACE_MS" => set_integer!(own_respawn_grace_ms),
            "KILL_PARTICIPATION_WINDOW_MS" => set_integer!(kill_participation_window_ms),
            "COORD_MIN_MEMBERS" => set_integer!(coord_min_members),
            "COORD_VANISH_WINDOW_MS" => set_integer!(coord_vanish_window_ms),
            "COORD_RADIUS" => set_float!(coord_radius),
            "COORD_HEADING_COS" => set_float!(coord_heading_cos),
            "COORD_MAX_AGE_MS" => set_integer!(coord_max_age_ms),
            "TOWARD_PLAYER_COS" => set_float!(toward_player_cos),
            "PROXIMITY_RADIUS" => set_float!(proximity_radius),
            "HYSTERESIS_LOW_DANGER" => set_float!(hysteresis_low_danger),
            "HYSTERESIS_LOW_REVISION" => set_float!(hysteresis_low_revision),
            "HYSTERESIS_HIGH_DANGER" => set_float!(hysteresis_high_danger),
            "HYSTERESIS_HIGH_REVISION" => set_float!(hysteresis_high_revision),
            "HISTORY_MS" => set_integer!(history_ms),
            _ => Err(format!("unknown parameter {name}")),
        }
    }

    fn is_valid(&self) -> bool {
        let floats = [
            self.ramp_start_s,
            self.peak_s,
            self.peak_risk,
            self.decay_per_s,
            self.floor,
            self.multi_boost,
            self.heading_amp,
            self.danger,
            self.revision,
            self.heading_min_displacement,
            self.center_deadzone,
            self.coord_radius,
            self.coord_heading_cos,
            self.toward_player_cos,
            self.proximity_radius,
            self.hysteresis_low_danger,
            self.hysteresis_low_revision,
            self.hysteresis_high_danger,
            self.hysteresis_high_revision,
        ];
        floats.iter().all(|value| value.is_finite())
            && self.ramp_start_s >= 0.0
            && self.peak_s >= 0.0
            && self.peak_s > self.ramp_start_s
            && (0.0..=1.0).contains(&self.danger)
            && (0.0..=1.0).contains(&self.revision)
            && self.revision < self.danger
            && (0.0..=1.0).contains(&self.coord_heading_cos)
            && (0.0..=1.0).contains(&self.toward_player_cos)
            && (0.0..=1.0).contains(&self.hysteresis_low_danger)
            && (0.0..=1.0).contains(&self.hysteresis_low_revision)
            && self.hysteresis_low_revision < self.hysteresis_low_danger
            && (0.0..=1.0).contains(&self.hysteresis_high_danger)
            && (0.0..=1.0).contains(&self.hysteresis_high_revision)
            && self.hysteresis_high_revision < self.hysteresis_high_danger
            && self.peak_risk >= 0.0
            && self.floor >= 0.0
            && self.multi_boost >= 0.0
            && self.heading_min_displacement >= 0.0
            && self.center_deadzone >= 0.0
            && self.coord_radius >= 0.0
            && self.proximity_radius >= 0.0
    }
}

fn exact_u64(value: f64) -> Option<u64> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value >= u64::MAX as f64 {
        None
    } else {
        Some(value as u64)
    }
}

/// A minimap position in milli-units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapPos {
    pub x: f64,
    pub y: f64,
}

impl MapPos {
    fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && (0.0..=1_000.0).contains(&self.x)
            && (0.0..=1_000.0).contains(&self.y)
    }
}

/// Current local-player state supplied by GSI.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnState {
    pub fresh: bool,
    pub in_match: bool,
    pub alive: bool,
    pub paused: Option<bool>,
    pub kills: Option<u32>,
    pub assists: Option<u32>,
    pub map_pos: Option<MapPos>,
}

/// One certified or anonymous vision observation.
#[derive(Debug, Clone, PartialEq)]
pub struct Sighting {
    pub track: u64,
    pub hero: Option<String>,
    pub pos: MapPos,
}

/// A single admitted capture frame and its associated session data.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub t_ms: u64,
    pub coverage: Coverage,
    pub own: Option<OwnState>,
    pub media_ready: bool,
    pub sightings: Vec<Sighting>,
}

/// A gate failure reason, in the order specified by section 6.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    NotInMatch,
    GsiStale,
    OwnDeadOrGrace,
    PausedOrUnknown,
    CoverageInvalid,
    OwnPositionUnknown,
    KillDataUnknown,
    NoGroup,
    NotTowardPlayer,
    TooFar,
    KillParticipation,
    BelowDanger,
    GlobalCooldown,
    ContributorCooldown,
    MediaUnready,
}

impl Reason {
    /// Return the stable snake-case fixture name for this reason.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotInMatch => "not_in_match",
            Self::GsiStale => "gsi_stale",
            Self::OwnDeadOrGrace => "own_dead_or_grace",
            Self::PausedOrUnknown => "paused_or_unknown",
            Self::CoverageInvalid => "coverage_invalid",
            Self::OwnPositionUnknown => "own_position_unknown",
            Self::KillDataUnknown => "kill_data_unknown",
            Self::NoGroup => "no_group",
            Self::NotTowardPlayer => "not_toward_player",
            Self::TooFar => "too_far",
            Self::KillParticipation => "kill_participation",
            Self::BelowDanger => "below_danger",
            Self::GlobalCooldown => "global_cooldown",
            Self::ContributorCooldown => "contributor_cooldown",
            Self::MediaUnready => "media_unready",
        }
    }
}

/// Result of the proposed gate for a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Admit,
    Blocked(Reason),
}

/// Kind of proposed, non-audible event emitted by the evaluator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    WouldAdmit,
    CorrectionWouldAdmit,
    LatchClearedSilent,
}

impl EventKind {
    /// Return the stable snake-case fixture name for this event kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WouldAdmit => "would_admit",
            Self::CorrectionWouldAdmit => "correction_would_admit",
            Self::LatchClearedSilent => "latch_cleared_silent",
        }
    }
}

/// A proposed event. Members are sorted lexicographically.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub t_ms: u64,
    pub kind: EventKind,
    pub members: Vec<String>,
    pub score: Option<f64>,
    pub eta_s: Option<f64>,
}

/// The proposed gate, informational score, and events for one tick.
#[derive(Debug, Clone, PartialEq)]
pub struct TickOutput {
    pub gate: Gate,
    pub score_all: Option<f64>,
    pub events: Vec<Event>,
}

/// Production decision for this proposal. It is pinned unavailable while the
/// owner decision is open, independent of the evaluator's proposed output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudibleDecision {
    UnavailableDecisionOpen,
    Play,
}

/// Return the production audible decision. The cue is unreachable by default.
pub fn audible_decision(_out: &TickOutput) -> AudibleDecision {
    if AUDIBLE_MISSING_GANK_APPROVED {
        AudibleDecision::Play
    } else {
        AudibleDecision::UnavailableDecisionOpen
    }
}

/// Compute the inherited per-hero missing-time score.
pub fn base_risk(seconds: f64, params: &Params) -> Option<f64> {
    if !seconds.is_finite() || seconds < 0.0 || !params.is_valid() {
        return None;
    }
    let score = if seconds < params.ramp_start_s {
        0.0
    } else if seconds <= params.peak_s {
        ((seconds - params.ramp_start_s) / (params.peak_s - params.ramp_start_s)) * params.peak_risk
    } else {
        (params.peak_risk - (seconds - params.peak_s) * params.decay_per_s).max(params.floor)
    };
    score.is_finite().then_some(score)
}

/// Derive a pre-vanish displacement from ordered sightings.
///
/// The last history item is the vanish sighting. Invalid or insufficient
/// history returns `None`.
pub fn heading(history: &[(u64, MapPos)], params: &Params) -> Option<(f64, f64)> {
    if !params.is_valid() {
        return None;
    }
    let (vanish_ms, _) = history.last().copied()?;
    let earliest_allowed = vanish_ms.saturating_sub(params.heading_window_ms);
    let mut earliest: Option<(u64, MapPos)> = None;
    let mut latest: Option<(u64, MapPos)> = None;
    let mut count = 0_u64;
    for &(at_ms, pos) in history {
        if at_ms < earliest_allowed || at_ms > vanish_ms {
            continue;
        }
        if !pos.is_valid() {
            return None;
        }
        count = count.saturating_add(1);
        if earliest.is_none_or(|(at, _)| at_ms < at) {
            earliest = Some((at_ms, pos));
        }
        if latest.is_none_or(|(at, _)| at_ms >= at) {
            latest = Some((at_ms, pos));
        }
    }
    if count < params.heading_min_sightings {
        return None;
    }
    let (first_ms, first_pos) = earliest?;
    let (last_ms, last_pos) = latest?;
    if last_ms.checked_sub(first_ms)? < params.heading_min_span_ms {
        return None;
    }
    let vector = (last_pos.x - first_pos.x, last_pos.y - first_pos.y);
    if !vector.0.is_finite()
        || !vector.1.is_finite()
        || vector.0.hypot(vector.1) < params.heading_min_displacement
    {
        return None;
    }
    Some(vector)
}

/// Return the heading multiplier, using neutral weight when heading evidence is
/// unavailable or invalid.
pub fn heading_multiplier(heading: Option<(f64, f64)>, p_last: MapPos, params: &Params) -> f64 {
    let Some((hx, hy)) = heading else {
        return 1.0;
    };
    if !params.is_valid() || !p_last.is_valid() || !hx.is_finite() || !hy.is_finite() {
        return 1.0;
    }
    let center = (500.0 - p_last.x, 500.0 - p_last.y);
    if center.0.hypot(center.1) < params.center_deadzone {
        return 1.0;
    }
    match cosine((hx, hy), center) {
        Some(cos) => (1.0 + cos * params.heading_amp).clamp(0.0, 2.0),
        None => 1.0,
    }
}

/// Aggregate independent hero scores using the inherited heuristic.
pub fn aggregate(risks: &[f64], p: &Params) -> f64 {
    if risks.is_empty() {
        return 0.0;
    }
    let valid: Vec<f64> = risks
        .iter()
        .copied()
        .filter(|risk| risk.is_finite() && *risk > 0.0)
        .map(|risk| risk.clamp(0.0, 1.0))
        .collect();
    if valid.is_empty() {
        return 0.0;
    }
    let mut remaining = 1.0;
    for risk in &valid {
        remaining *= 1.0 - risk;
    }
    let mut combined = 1.0 - remaining;
    if valid.len() >= 2 {
        let boost = if p.multi_boost.is_finite() && p.multi_boost >= 0.0 {
            p.multi_boost
        } else {
            1.0
        };
        combined *= boost;
    }
    if combined.is_finite() {
        combined.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Return the inherited time-since-vanish ETA in seconds.
pub fn eta_s(seconds: f64) -> f64 {
    if !seconds.is_finite() || seconds < 0.0 {
        f64::NAN
    } else {
        (12.0 - seconds).max(1.0)
    }
}

fn cosine(left: (f64, f64), right: (f64, f64)) -> Option<f64> {
    if !left.0.is_finite() || !left.1.is_finite() || !right.0.is_finite() || !right.1.is_finite() {
        return None;
    }
    let left_norm = left.0.hypot(left.1);
    let right_norm = right.0.hypot(right.1);
    if left_norm == 0.0 || right_norm == 0.0 || !left_norm.is_finite() || !right_norm.is_finite() {
        return None;
    }
    let value = (left.0 / left_norm) * (right.0 / right_norm)
        + (left.1 / left_norm) * (right.1 / right_norm);
    value.is_finite().then_some(value.clamp(-1.0, 1.0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeroPhase {
    NeverSeen,
    Visible,
    Missing,
    Interrupted,
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    at_ms: u64,
    pos: MapPos,
}

#[derive(Debug, Default)]
struct HeroState {
    phase: Option<HeroPhase>,
    history: Vec<Sample>,
    vanish_ms: Option<u64>,
    heading: Option<(f64, f64)>,
    last_pos: Option<MapPos>,
    notified: bool,
    reappearance: Vec<Sample>,
}

impl HeroState {
    fn phase(&self) -> HeroPhase {
        self.phase.unwrap_or(HeroPhase::NeverSeen)
    }
}

#[derive(Debug, Clone, Copy)]
struct DeathWindow {
    start_ms: u64,
    end_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct Candidate {
    hero: String,
    vanish_ms: u64,
    position: MapPos,
    heading: (f64, f64),
    risk: f64,
}

#[derive(Debug, Clone)]
struct SelectedGroup {
    members: Vec<String>,
    score: f64,
}

/// Stateful, pure evaluator for proposed risk-admission behavior.
pub struct ProposedEvaluator {
    params: Params,
    heroes: BTreeMap<String, HeroState>,
    last_tick_ms: Option<u64>,
    death_windows: Vec<DeathWindow>,
    respawn_until_ms: Option<u64>,
    previous_kill_total: Option<u64>,
    kill_change_times: Vec<u64>,
    kill_unknown_times: Vec<u64>,
    last_admitted_ms: Option<u64>,
    contributor_admitted_ms: BTreeMap<String, u64>,
    latch: BTreeSet<String>,
}

type RiskByHero = BTreeMap<String, Option<f64>>;
type MissingSecondsByHero = BTreeMap<String, f64>;

impl ProposedEvaluator {
    /// Construct an evaluator with the provided proposed parameters.
    pub fn new(params: Params) -> Self {
        Self {
            params,
            heroes: BTreeMap::new(),
            last_tick_ms: None,
            death_windows: Vec::new(),
            respawn_until_ms: None,
            previous_kill_total: None,
            kill_change_times: Vec::new(),
            kill_unknown_times: Vec::new(),
            last_admitted_ms: None,
            contributor_admitted_ms: BTreeMap::new(),
            latch: BTreeSet::new(),
        }
    }

    /// Evaluate one frame. A bad frame interrupts all active hero evidence and
    /// is never used to establish a new baseline.
    pub fn tick(&mut self, tick: &Tick) -> TickOutput {
        let time_interrupted = self.last_tick_ms.is_some_and(|last| {
            tick.t_ms
                .checked_sub(last)
                .is_none_or(|elapsed| elapsed == 0 || elapsed > self.params.observation_gap_ms)
        });
        let paused_or_missing = tick
            .own
            .as_ref()
            .is_none_or(|own| own.paused != Some(false));
        let invalid_position = tick
            .own
            .as_ref()
            .and_then(|own| own.map_pos)
            .is_some_and(|pos| !pos.is_valid())
            || tick
                .sightings
                .iter()
                .any(|sighting| !sighting.pos.is_valid());
        let invalid_params = !self.params.is_valid();
        let interrupted = tick.coverage != Coverage::Valid
            || time_interrupted
            || paused_or_missing
            || invalid_position
            || invalid_params;

        self.update_session_history(tick);

        let mut interrupted_latched = false;
        let mut reappeared = BTreeSet::new();
        if interrupted {
            for (hero, state) in &mut self.heroes {
                match state.phase() {
                    HeroPhase::Visible | HeroPhase::Missing => {
                        if self.latch.contains(hero) {
                            interrupted_latched = true;
                        }
                        state.phase = Some(HeroPhase::Interrupted);
                        state.history.clear();
                        state.vanish_ms = None;
                        state.heading = None;
                        state.last_pos = None;
                        state.reappearance.clear();
                    }
                    HeroPhase::Interrupted => state.reappearance.clear(),
                    HeroPhase::NeverSeen => {}
                }
            }
        } else {
            self.apply_observations(tick, &mut reappeared);
        }

        if tick.own.as_ref().is_some_and(|own| own.alive)
            && self
                .respawn_until_ms
                .is_some_and(|until| tick.t_ms >= until)
        {
            self.respawn_until_ms = None;
        }

        let (risks, missing_seconds) = self.current_risks(tick.t_ms);
        let score_all = if tick.coverage == Coverage::Valid {
            Some(aggregate(
                &risks.values().filter_map(|risk| *risk).collect::<Vec<_>>(),
                &self.params,
            ))
        } else {
            None
        };

        let mut events = Vec::new();
        self.update_latch(
            tick,
            interrupted_latched,
            &reappeared,
            score_all,
            &mut events,
        );

        let (gate, selected) = self.evaluate_gate(tick, &risks);
        if let (Gate::Admit, Some(group)) = (gate, selected) {
            let eta = group
                .members
                .iter()
                .filter_map(|hero| missing_seconds.get(hero).copied())
                .map(eta_s)
                .reduce(f64::min);
            events.push(Event {
                t_ms: tick.t_ms,
                kind: EventKind::WouldAdmit,
                members: group.members.clone(),
                score: Some(group.score),
                eta_s: eta.filter(|value| value.is_finite()),
            });
            for hero in &group.members {
                if let Some(state) = self.heroes.get_mut(hero) {
                    state.notified = true;
                }
                self.contributor_admitted_ms.insert(hero.clone(), tick.t_ms);
                self.latch.insert(hero.clone());
            }
            self.last_admitted_ms = Some(tick.t_ms);
        }

        self.last_tick_ms = Some(tick.t_ms);
        self.prune_history(tick.t_ms);
        TickOutput {
            gate,
            score_all,
            events,
        }
    }

    fn update_session_history(&mut self, tick: &Tick) {
        if let Some(own) = tick.own.as_ref() {
            if own.fresh && own.in_match {
                let has_open_death = self
                    .death_windows
                    .iter()
                    .any(|window| window.end_ms.is_none());
                if !own.alive && !has_open_death {
                    self.death_windows.push(DeathWindow {
                        start_ms: tick.t_ms,
                        end_ms: None,
                    });
                    self.respawn_until_ms = None;
                } else if own.alive && has_open_death {
                    let until = tick.t_ms.saturating_add(self.params.own_respawn_grace_ms);
                    if let Some(window) = self
                        .death_windows
                        .iter_mut()
                        .rev()
                        .find(|window| window.end_ms.is_none())
                    {
                        window.end_ms = Some(until);
                    }
                    self.respawn_until_ms = Some(until);
                }
            }

            match (own.kills, own.assists) {
                (Some(kills), Some(assists)) => {
                    let total = u64::from(kills) + u64::from(assists);
                    if self
                        .previous_kill_total
                        .is_some_and(|previous| total > previous)
                    {
                        self.kill_change_times.push(tick.t_ms);
                    }
                    self.previous_kill_total = Some(total);
                }
                _ => self.kill_unknown_times.push(tick.t_ms),
            }
        } else {
            self.kill_unknown_times.push(tick.t_ms);
        }
    }

    fn apply_observations(&mut self, tick: &Tick, reappeared: &mut BTreeSet<String>) {
        let mut chosen: BTreeMap<String, Sighting> = BTreeMap::new();
        for sighting in &tick.sightings {
            let Some(hero) = sighting.hero.as_ref() else {
                continue;
            };
            self.heroes.entry(hero.clone()).or_default();
            let replace = chosen.get(hero).is_none_or(|current| {
                (sighting.track, sighting.pos.x, sighting.pos.y)
                    .cmp_by_total((current.track, current.pos.x, current.pos.y))
                    .is_lt()
            });
            if replace {
                chosen.insert(hero.clone(), sighting.clone());
            }
        }

        for (hero, state) in &mut self.heroes {
            let sighting = chosen.get(hero);
            match state.phase() {
                HeroPhase::NeverSeen => {
                    if let Some(sighting) = sighting {
                        state.phase = Some(HeroPhase::Visible);
                        state.history = vec![Sample {
                            at_ms: tick.t_ms,
                            pos: sighting.pos,
                        }];
                    }
                }
                HeroPhase::Visible => {
                    if let Some(sighting) = sighting {
                        state.history.push(Sample {
                            at_ms: tick.t_ms,
                            pos: sighting.pos,
                        });
                        let oldest = tick.t_ms.saturating_sub(self.params.history_ms);
                        state.history.retain(|sample| sample.at_ms >= oldest);
                    } else if let Some(last) = state.history.last().copied() {
                        let history: Vec<_> = state
                            .history
                            .iter()
                            .map(|sample| (sample.at_ms, sample.pos))
                            .collect();
                        state.phase = Some(HeroPhase::Missing);
                        state.vanish_ms = Some(last.at_ms);
                        state.heading = heading(&history, &self.params);
                        state.last_pos = Some(last.pos);
                        state.reappearance.clear();
                    } else {
                        state.phase = Some(HeroPhase::Interrupted);
                    }
                }
                HeroPhase::Missing | HeroPhase::Interrupted => {
                    if let Some(sighting) = sighting {
                        state.reappearance.push(Sample {
                            at_ms: tick.t_ms,
                            pos: sighting.pos,
                        });
                        let frames = state.reappearance.len() as u64;
                        let span = state
                            .reappearance
                            .last()
                            .zip(state.reappearance.first())
                            .and_then(|(last, first)| last.at_ms.checked_sub(first.at_ms));
                        if frames >= self.params.reappear_min_frames.max(1)
                            && span.is_some_and(|elapsed| elapsed >= self.params.reappear_min_ms)
                        {
                            if state.phase() == HeroPhase::Missing {
                                reappeared.insert(hero.clone());
                            }
                            state.phase = Some(HeroPhase::Visible);
                            state.history.clone_from(&state.reappearance);
                            state.vanish_ms = None;
                            state.heading = None;
                            state.last_pos = None;
                            state.notified = false;
                            state.reappearance.clear();
                        }
                    } else {
                        state.reappearance.clear();
                    }
                }
            }
        }
    }

    fn current_risks(&self, now_ms: u64) -> (RiskByHero, MissingSecondsByHero) {
        let mut risks = BTreeMap::new();
        let mut seconds_by_hero = BTreeMap::new();
        for (hero, state) in &self.heroes {
            if state.phase() != HeroPhase::Missing {
                risks.insert(hero.clone(), None);
                continue;
            }
            let Some(vanish_ms) = state.vanish_ms else {
                risks.insert(hero.clone(), None);
                continue;
            };
            let Some(elapsed_ms) = now_ms.checked_sub(vanish_ms) else {
                risks.insert(hero.clone(), None);
                continue;
            };
            let seconds = elapsed_ms as f64 / 1_000.0;
            let risk = base_risk(seconds, &self.params).and_then(|base| {
                let multiplier = state.last_pos.map_or(1.0, |position| {
                    heading_multiplier(state.heading, position, &self.params)
                });
                let value = (base * multiplier).clamp(0.0, 1.0);
                value.is_finite().then_some(value)
            });
            seconds_by_hero.insert(hero.clone(), seconds);
            risks.insert(hero.clone(), risk);
        }
        (risks, seconds_by_hero)
    }

    fn update_latch(
        &mut self,
        tick: &Tick,
        interrupted_latched: bool,
        reappeared: &BTreeSet<String>,
        score_all: Option<f64>,
        events: &mut Vec<Event>,
    ) {
        if self.latch.is_empty() {
            return;
        }
        if interrupted_latched {
            self.clear_latch(tick.t_ms, EventKind::LatchClearedSilent, score_all, events);
            return;
        }
        let cleared_members: Vec<_> = self.latch.intersection(reappeared).cloned().collect();
        if !cleared_members.is_empty() {
            for hero in &cleared_members {
                self.latch.remove(hero);
            }
        }
        if !cleared_members.is_empty() && self.latch.is_empty() {
            let can_correct = self.base_gate_reason(tick).is_none()
                && score_all.is_some_and(|score| score < self.params.revision);
            let kind = if can_correct {
                EventKind::CorrectionWouldAdmit
            } else {
                EventKind::LatchClearedSilent
            };
            events.push(Event {
                t_ms: tick.t_ms,
                kind,
                members: cleared_members,
                score: score_all,
                eta_s: None,
            });
        }
    }

    fn clear_latch(
        &mut self,
        t_ms: u64,
        kind: EventKind,
        score: Option<f64>,
        events: &mut Vec<Event>,
    ) {
        let members = std::mem::take(&mut self.latch).into_iter().collect();
        events.push(Event {
            t_ms,
            kind,
            members,
            score,
            eta_s: None,
        });
    }

    fn base_gate_reason(&self, tick: &Tick) -> Option<Reason> {
        let own = tick.own.as_ref()?;
        if !own.in_match {
            return Some(Reason::NotInMatch);
        }
        if !own.fresh {
            return Some(Reason::GsiStale);
        }
        if !own.alive || self.respawn_until_ms.is_some_and(|until| tick.t_ms < until) {
            return Some(Reason::OwnDeadOrGrace);
        }
        if own.paused != Some(false) {
            return Some(Reason::PausedOrUnknown);
        }
        if tick.coverage != Coverage::Valid {
            return Some(Reason::CoverageInvalid);
        }
        None
    }

    fn evaluate_gate(
        &self,
        tick: &Tick,
        risks: &BTreeMap<String, Option<f64>>,
    ) -> (Gate, Option<SelectedGroup>) {
        let blocked = |reason| (Gate::Blocked(reason), None);
        let Some(own) = tick.own.as_ref() else {
            return blocked(Reason::NotInMatch);
        };
        if !own.in_match {
            return blocked(Reason::NotInMatch);
        }
        if !own.fresh {
            return blocked(Reason::GsiStale);
        }
        if !own.alive || self.respawn_until_ms.is_some_and(|until| tick.t_ms < until) {
            return blocked(Reason::OwnDeadOrGrace);
        }
        if own.paused != Some(false) {
            return blocked(Reason::PausedOrUnknown);
        }
        if tick.coverage != Coverage::Valid {
            return blocked(Reason::CoverageInvalid);
        }
        if self.params.player_relative_gate
            && own.map_pos.is_none_or(|position| !position.is_valid())
        {
            return blocked(Reason::OwnPositionUnknown);
        }
        if own.kills.is_none() || own.assists.is_none() {
            return blocked(Reason::KillDataUnknown);
        }

        let Some(group) = self.choose_group(tick.t_ms, risks) else {
            return blocked(Reason::NoGroup);
        };
        let Some(earliest_vanish) = group
            .members
            .iter()
            .filter_map(|hero| self.heroes.get(hero).and_then(|state| state.vanish_ms))
            .min()
        else {
            return blocked(Reason::NoGroup);
        };
        let earliest_unknown =
            earliest_vanish.saturating_sub(self.params.kill_participation_window_ms);
        if self
            .kill_unknown_times
            .iter()
            .any(|unknown_ms| earliest_unknown <= *unknown_ms && *unknown_ms <= tick.t_ms)
        {
            return blocked(Reason::KillDataUnknown);
        }
        if self.params.player_relative_gate {
            let Some(position) = own.map_pos else {
                return blocked(Reason::OwnPositionUnknown);
            };
            match player_gate(&group, &self.heroes, position, &self.params) {
                Some(Reason::NotTowardPlayer) => {
                    return (Gate::Blocked(Reason::NotTowardPlayer), Some(group));
                }
                Some(Reason::TooFar) => return (Gate::Blocked(Reason::TooFar), Some(group)),
                _ => {}
            }
        }
        if self.kill_change_times.iter().any(|changed_at| {
            group.members.iter().any(|hero| {
                self.heroes
                    .get(hero)
                    .and_then(|state| state.vanish_ms)
                    .is_some_and(|vanish| {
                        changed_at.abs_diff(vanish) <= self.params.kill_participation_window_ms
                    })
            })
        }) {
            return (Gate::Blocked(Reason::KillParticipation), Some(group));
        }
        if group.score < self.params.danger {
            return (Gate::Blocked(Reason::BelowDanger), Some(group));
        }
        if self.last_admitted_ms.is_some_and(|last| {
            tick.t_ms
                .checked_sub(last)
                .is_none_or(|elapsed| elapsed < self.params.global_cooldown_ms)
        }) {
            return (Gate::Blocked(Reason::GlobalCooldown), Some(group));
        }
        if group.members.iter().any(|hero| {
            self.contributor_admitted_ms.get(hero).is_some_and(|last| {
                tick.t_ms
                    .checked_sub(*last)
                    .is_none_or(|elapsed| elapsed < self.params.contributor_cooldown_ms)
            })
        }) {
            return (Gate::Blocked(Reason::ContributorCooldown), Some(group));
        }
        if !tick.media_ready {
            return (Gate::Blocked(Reason::MediaUnready), Some(group));
        }
        (Gate::Admit, Some(group))
    }

    fn choose_group(
        &self,
        now_ms: u64,
        risks: &BTreeMap<String, Option<f64>>,
    ) -> Option<SelectedGroup> {
        let mut candidates = Vec::new();
        for (hero, state) in &self.heroes {
            if state.phase() != HeroPhase::Missing || state.notified {
                continue;
            }
            let (Some(vanish_ms), Some(heading), Some(position)) =
                (state.vanish_ms, state.heading, state.last_pos)
            else {
                continue;
            };
            if self.inside_own_death(vanish_ms) {
                continue;
            }
            let Some(elapsed_ms) = now_ms.checked_sub(vanish_ms) else {
                continue;
            };
            let Some(risk) = risks.get(hero).copied().flatten() else {
                continue;
            };
            let missing_seconds = elapsed_ms as f64 / 1_000.0;
            if !missing_seconds.is_finite() {
                continue;
            }
            candidates.push(Candidate {
                hero: hero.clone(),
                vanish_ms,
                position,
                heading,
                risk,
            });
        }
        select_group(&candidates, now_ms, &self.params)
    }

    fn inside_own_death(&self, vanish_ms: u64) -> bool {
        self.death_windows.iter().any(|window| {
            window.start_ms <= vanish_ms && window.end_ms.is_none_or(|end| vanish_ms < end)
        })
    }

    fn prune_history(&mut self, now_ms: u64) {
        let oldest_kill = now_ms.saturating_sub(
            self.params
                .coord_max_age_ms
                .saturating_add(self.params.coord_vanish_window_ms)
                .saturating_add(self.params.kill_participation_window_ms),
        );
        self.kill_change_times
            .retain(|changed_at| *changed_at >= oldest_kill);
        self.kill_unknown_times
            .retain(|unknown_at| *unknown_at >= oldest_kill);

        let oldest_relevant_vanish = self
            .heroes
            .values()
            .filter(|state| state.phase() == HeroPhase::Missing)
            .filter_map(|state| state.vanish_ms)
            .min();
        self.death_windows
            .retain(|window| match (window.end_ms, oldest_relevant_vanish) {
                (Some(end), Some(oldest)) => end > oldest,
                (Some(end), None) => end > now_ms,
                (None, _) => true,
            });
    }
}

fn select_group(candidates: &[Candidate], now_ms: u64, params: &Params) -> Option<SelectedGroup> {
    let min_members = params.coord_min_members.max(2);
    if min_members > candidates.len() as u64 {
        return None;
    }
    let min_members = usize::try_from(min_members).ok()?;
    let mut best: Option<SelectedGroup> = None;
    for size in min_members..=candidates.len() {
        let mut indexes: Vec<usize> = (0..size).collect();
        loop {
            let group = indexes
                .iter()
                .filter_map(|index| candidates.get(*index))
                .collect::<Vec<_>>();
            if group.len() == size && group_is_valid(&group, now_ms, params) {
                let score = aggregate(
                    &group
                        .iter()
                        .map(|candidate| candidate.risk)
                        .collect::<Vec<_>>(),
                    params,
                );
                let members = group
                    .iter()
                    .map(|candidate| candidate.hero.clone())
                    .collect::<Vec<_>>();
                let replace = best.as_ref().is_none_or(|current| {
                    members.len() > current.members.len()
                        || (members.len() == current.members.len()
                            && (score > current.score
                                || (score.total_cmp(&current.score).is_eq()
                                    && members.as_slice() < current.members.as_slice())))
                });
                if replace {
                    best = Some(SelectedGroup { members, score });
                }
            }
            let Some(position) = (0..size)
                .rev()
                .find(|position| indexes[*position] < candidates.len() - size + *position)
            else {
                break;
            };
            indexes[position] += 1;
            for next in (position + 1)..size {
                indexes[next] = indexes[next - 1] + 1;
            }
        }
    }
    best
}

fn group_is_valid(group: &[&Candidate], now_ms: u64, params: &Params) -> bool {
    let Some(max_vanish) = group.iter().map(|candidate| candidate.vanish_ms).max() else {
        return false;
    };
    if now_ms
        .checked_sub(max_vanish)
        .is_none_or(|age| age > params.coord_max_age_ms)
    {
        return false;
    }
    for (left_index, left) in group.iter().enumerate() {
        for right in group.iter().skip(left_index + 1) {
            if left.vanish_ms.abs_diff(right.vanish_ms) > params.coord_vanish_window_ms
                || distance(left.position, right.position) > params.coord_radius
                || cosine(left.heading, right.heading)
                    .is_none_or(|value| value < params.coord_heading_cos)
            {
                return false;
            }
        }
    }
    true
}

fn player_gate(
    group: &SelectedGroup,
    heroes: &BTreeMap<String, HeroState>,
    own_position: MapPos,
    params: &Params,
) -> Option<Reason> {
    let mut unit_sum = (0.0, 0.0);
    let mut position_sum = (0.0, 0.0);
    let mut count = 0.0;
    for hero in &group.members {
        let Some(state) = heroes.get(hero) else {
            return Some(Reason::NotTowardPlayer);
        };
        let (Some(heading), Some(position)) = (state.heading, state.last_pos) else {
            return Some(Reason::NotTowardPlayer);
        };
        let norm = heading.0.hypot(heading.1);
        if norm == 0.0 || !norm.is_finite() {
            return Some(Reason::NotTowardPlayer);
        }
        unit_sum.0 += heading.0 / norm;
        unit_sum.1 += heading.1 / norm;
        position_sum.0 += position.x;
        position_sum.1 += position.y;
        count += 1.0;
    }
    let heading_norm = unit_sum.0.hypot(unit_sum.1);
    if count == 0.0 || heading_norm == 0.0 || !heading_norm.is_finite() {
        return Some(Reason::NotTowardPlayer);
    }
    let centroid = (position_sum.0 / count, position_sum.1 / count);
    let direction = (own_position.x - centroid.0, own_position.y - centroid.1);
    let distance_to_player = direction.0.hypot(direction.1);
    if !distance_to_player.is_finite() {
        return Some(Reason::TooFar);
    }
    if distance_to_player >= params.center_deadzone
        && cosine(unit_sum, direction).is_none_or(|value| value < params.toward_player_cos)
    {
        return Some(Reason::NotTowardPlayer);
    }
    if distance_to_player > params.proximity_radius {
        return Some(Reason::TooFar);
    }
    None
}

fn distance(left: MapPos, right: MapPos) -> f64 {
    (left.x - right.x).hypot(left.y - right.y)
}

trait TupleTotalCmp {
    fn cmp_by_total(self, other: Self) -> std::cmp::Ordering;
}

impl TupleTotalCmp for (u64, f64, f64) {
    fn cmp_by_total(self, other: Self) -> std::cmp::Ordering {
        self.0
            .cmp(&other.0)
            .then_with(|| self.1.total_cmp(&other.1))
            .then_with(|| self.2.total_cmp(&other.2))
    }
}
