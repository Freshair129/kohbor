---
title: "S4 Observation Boundary and Capture Admission (design for the no-hardware slice)"
doc_id: "S4-OBSERVATION-BOUNDARY"
status: draft
version: "0.1.0"
updated: "2026-11-13"
authority: "Lead decision under the owner's Sprint 4 start for parts that need no hardware and no open decision. Elaborates VISION-001..004, DATA-005, DATA-006 and REMAINING-SYSTEM-DESIGN sections 1.5 to 1.7. Changes no requirement statement."
approval_status: "agent_decision_not_owner_approved"
---

# S4 Observation Boundary and Capture Admission

Labels: **SPEC** = stated by SRS or a governing design section (id given). **PROPOSED** = this document's choice, inside engineering freedom, changeable without an owner. **UNVERIFIED** = nothing measured; the confirming step is named. No label means a plain restatement of the SPEC row above it.

## 1. Scope

In scope: pure Rust logic with an injected clock and no I/O, in two new crates (`crates/kohbor-capture`, `crates/kohbor-vision`, the names and split come from REMAINING-SYSTEM-DESIGN section 4). Out of scope, not started: screen capture code, any OS capture API, frame transport, calibration captures, the rect table (spike V3), detector or model inference, risk score, settings or runtime-status contract changes, UI. Evidence class of everything here: **source and synthetic only**. Nothing here proves a native or live claim; QUALITY and VISION rows stay NOT_RUN for native and live classes.

## 2. What is testable today

| Item | Requirement | Label | Where |
| --- | --- | --- | --- |
| Rect validity against output bounds (non-empty, inside the output, square within 1 px) | VISION-001 | SPEC: square region (RSD 1.5 step 3). The 1 px tolerance is PROPOSED | capture |
| Calibration admission: candidate, structural result stream (30 consecutive passes to confirm), single confirmation, record | VISION-001 | SPEC (RSD 1.5); the "30 consecutive" reading of "≥30 frames" is PROPOSED | capture |
| Calibration generation fence: every invalidation and every new candidate bumps it; frames of an older generation, session, output are rejected | VISION-001 (AC) | SPEC | capture |
| Invalidation causes: output size, DPI, monitor, game window rect, minimap layout, access lost, 60 consecutive structural failures, user action | VISION-001 | SPEC (RSD 1.5) | capture |
| Crop shown only when calibration is `validated` | VISION-001 | SPEC (RSD 1.5) | capture |
| Freshness: fresh ≤2000 ms, stale ≥2001 ms, coverage unavailable >5000 ms (5000 is still available), measured from the last healthy acquire cycle; a healthy timeout counts as healthy | VISION-001/002 | SPEC (RSD 1.3 and 1.6, tests at 2000/2001) | capture |
| Phase derivation: `live`, `lite`, `stale`, `coverage_unavailable`, `calibrating`, `recalibration_needed`, `lost`, `unsupported` | VISION-001 | SPEC names; precedence order is PROPOSED | capture |
| Capture-source separation: the selected critical source is explicit; OBS recording being on never changes it; frames from any other source are rejected | VISION-004 | SPEC (AC-VISION-004). Admission of OBS or WGC as a critical source needs a separate measured decision, so it cannot be set in this slice | capture |
| Observation boundary: an enemy field exists only through an admitted frame; unobserved is `unknown` with no value, never zero, default, predicted or inferred; no hidden-stat fields exist | DATA-005 | SPEC | vision |
| Class certification: a class outside the certified set is `unverified`, not accepted; the set is empty until DEC-GAME-DATA closes | VISION-003 | SPEC | vision |
| Missing requires continuing coverage: absence accrues only over consecutive admitted frames; any gap suspends it and yields neither missing nor all-clear | VISION-002 | SPEC; the accrual rule is PROPOSED | vision |
| Catalog provenance label: partial or unsupported, never "latest" | DATA-006 | SPEC; the shape is PROPOSED | vision |

UNVERIFIED and not testable here: any real frame, rect, DPI, monitor, timing under load, capture cost, detector quality, structural check on pixels (the pixel test itself needs frames; this slice consumes only its boolean result).

## 3. Decisions that block the rest of Sprint 4 (real status, checked 2026-11-13 in `contracts/decisions.json`)

| Decision | Status | Blocks | Safe interim (from the decision) |
| --- | --- | --- | --- |
| DEC-MODELS | open | Detector, identity, anything that produces sightings (S5) | Model capability unavailable |
| DEC-GAME-DATA | open | The certified class list (the set stays empty), catalog content, any game-derived template | Unknown unsupported fields |
| DEC-NATIVE-EVIDENCE | open | Every native or live claim: capture cost, FPS pairs, rect table, calibration on real frames | NOT_RUN or INDETERMINATE, not PASS |
| DEC-RISK-DETAIL | open | The missing threshold and any score. This slice takes the threshold as a caller argument and invents none | Informational only |

Spikes V1 to V3 (owner machine, running game, stills) and S4-0 contract changes (runtime-status 1.2.0 capture phases, calibration record in ICD, settings categories) are not done here. The capture phase enum below is a proposal for S4-0, not a contract.

## 4. Design

Time is an argument (`now_ms: u64`, monotonic, from the caller). No function reads a clock, a file or a socket.

### 4.1 `kohbor-capture` (public surface for this slice, PROPOSED)

- Constants: `FRESH_MAX_MS = 2000`, `COVERAGE_MAX_MS = 5000`, `STRUCTURAL_PASS_FRAMES = 30`, `STRUCTURAL_FAIL_FRAMES = 60`.
- `Rect { x: i32, y: i32, width: u32, height: u32 }`; `OutputGeometry { output_id: u64, width: u32, height: u32, dpi: u32 }`; `Environment { output: OutputGeometry, monitor_id: u64, window_rect: Rect, minimap_layout: String }`; `validate_rect(&Rect, &OutputGeometry) -> Result<(), GeometryError>` with `GeometryError::{ZeroOutput, EmptyRect, OutOfBounds, NotSquare}`.
- `Calibrator`: phases `Uncalibrated | Calibrating | Validated | RecalibrationNeeded`; `begin(env, candidate_rect, profile_id)` (validates the rect, bumps generation, enters Calibrating); `structural_frame(passed: bool)` (a failure resets the consecutive-pass count; while Validated, 60 consecutive failures invalidate); `confirm(method, now_ms)` (needs 30 consecutive passes, else `ConfirmError`; creates the `CalibrationRecord`); `observe_environment(&Environment)` (any difference from the record's environment invalidates with the matching `InvalidationReason`); `invalidate(reason)`; `generation()`, `phase()`, `record()`. Generation starts at 0 and only grows.
- `CaptureController`: owns the `Calibrator`, the selected `CaptureSource` (`Dxgi | Wgc | Obs`), the session epoch, the lite flag and the OBS-recording flag. `record_cycle(now_ms, CycleOutcome::{HealthyTimeout, AccessLost, Failed, Unsupported})`; `admit(&FrameHeader, now_ms) -> Result<AdmittedFrame, Reject>` with `Reject::{NotValidated, StaleGeneration, WrongSession, WrongOutput, SourceNotSelected, OutOfOrder}`; `phase(now_ms) -> CapturePhase`; `coverage(now_ms) -> Coverage::{Valid, Stale, Unavailable}`; `crop_visible() -> bool`.
- `AdmittedFrame` has private fields and no public constructor. Only `CaptureController::admit` makes one. This is the type-level half of the observation boundary: downstream code cannot fabricate a frame.
- Phase precedence (PROPOSED): `unsupported`, `lost`, `calibrating`/`recalibration_needed` (and `Uncalibrated`, reported as `calibrating`), `coverage_unavailable`, `stale`, `lite`, `live`. Coverage `Valid` is live or lite; `Stale` is 2001 to 5000 ms; `Unavailable` is more than 5000 ms or any phase above stale. A failed cycle (`AccessLost`, `Failed`) never refreshes the clock; a `HealthyTimeout` does (a paused game is not stale).
- A successful `admit` is itself a healthy cycle and refreshes the clock at `header.acquired_at_ms` (a rejected frame refreshes nothing); `record_cycle` carries the other outcomes (`HealthyTimeout`, `AccessLost`, `Failed`, `Unsupported`). `acquired_at_ms` must not be after `now_ms` and not before the previous admitted frame (`OutOfOrder`). PROPOSED.

### 4.2 `kohbor-vision` (depends on `kohbor-capture`, PROPOSED)

- `Availability::{Fresh, Stale, Unknown, Invalid, Unsupported}` and `Field<T>` with private members: `value()` is `Some` exactly when availability is `Fresh` or `Stale`. There is no `Default`, no `From<T>` and no zero constructor. Constructors: `Field::fresh`, `Field::stale`, `Field::unknown`, `Field::invalid`, `Field::unsupported` (mirrors `contracts/own-snapshot.schema.json`; DDD section on availability).
- `MapPoint { x_milli: u16, y_milli: u16 }` (0 to 1000 of the calibrated rect, integers so equality is exact). `TrackId(u64)` is supplied by the detector layer (S5); this slice invents no identity.
- `ClassId(String)`, `CoverageStatus::{Verified, Partial, Unsupported}`, `CatalogProvenance { patch: Option<String>, source_hash: Option<String>, coverage: CoverageStatus }`, `fact_label(&CatalogProvenance, requested_patch: &str) -> FactLabel::{Verified, Partial, Unsupported}`: no patch, or a different patch, gives Unsupported; partial coverage or a missing hash gives Partial; there is no "latest" value. `CertifiedClasses::none()` and `CertifiedClasses::from_provenance(&CatalogProvenance, classes)` (an empty set unless the provenance is Verified).
- `IdentityState::{Unknown, Unverified, Certified(ClassId)}` via `classify(Option<&ClassId>, &CertifiedClasses)`. An unverified class never carries the class value.
- `EnemyObservation { track, position: Field<MapPoint>, identity: IdentityState, last_seen_ms: Option<u64>, presence: Presence }`, `Presence::{NeverSeen, Visible, Absent { covered_ms }, Interrupted}`. The struct has no hp, mana, item, level or ability field on purpose (AC-DATA-005); a test destructures it exhaustively so adding one breaks the build and forces a review. `EnemyObservation::unobserved(track)` is the only way to get one without an admitted sighting.
- `Tracker::new(CertifiedClasses)`; `on_admitted_frame(&AdmittedFrame, &[Sighting])`; `on_coverage_lost(now_ms)`; `observation(TrackId) -> EnemyObservation` (unobserved for a track never sighted); `absent_beyond(min_covered_ms) -> Vec<TrackId>` (strictly greater). Rules:
  - Position is `fresh` only in a frame that sighted the track. Afterwards it is `stale` with the last measured point (a real measurement, labelled; no prediction, no drift). Never sighted: `unknown`.
  - Absence accrues only across consecutive admitted frames at most `FRESH_MAX_MS` apart. A larger gap, or `on_coverage_lost`, makes seen tracks `Interrupted` and adds nothing to `covered_ms`; the tracker never reports missing or all-clear across a gap. After resumption the clock restarts from zero (rebuilt baseline, SDD risk section). PROPOSED, pending DEC-RISK-DETAIL.
  - A different session epoch or calibration generation on an admitted frame clears all tracks (old coordinates are never kept).
  - A track is `Absent` only after it was `Visible` before; a track never sighted is never absent or missing (VISION-002 prior presence).
  - The threshold is an argument; this crate holds no risk constant.

### 4.3 Provenance of the labels above and what could be wrong

The structural 30/60 counts, the 1 px square tolerance, the stale-admission rule and the gap-restart rule are PROPOSED values. They are not measurements. If a native run or DEC-RISK-DETAIL contradicts one, change the constant and its test together and record why; do not weaken a threshold to pass a failing run (AGENTS.md).

## 5. Review and risk

R2 for the observation boundary (roadmap Sprint 4 row): read-only review by the test worker, then the lead reads the full diff. Reviews are `agent_review`, never owner approval.

## 6. Owner must provide before the rest of Sprint 4 and Sprint 5

Calibration stills for the certified cell and both minimap options (spike V3); a hardware session with the game for spikes V1 to V3 and FPS pairs (DEC-NATIVE-EVIDENCE); the certified class list and asset provenance (DEC-GAME-DATA); the detector configuration decision (DEC-MODELS); a contract decision for S4-0 (runtime-status capture phases, ICD frame header and calibration record, settings categories), which is owner-reserved.
