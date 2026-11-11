---
title: "Sprint Roadmap"
doc_id: "SPRINT-ROADMAP"
status: draft
version: "0.3.6"
updated: "2026-11-11"
authority: "SDP milestones; planning metadata only. Scope, priority and acceptance stay in PRD/SRS and the registry"
owner_role: "Engineering lead"
approval_status: "not_approved"
---

# Sprint Roadmap

This page orders the work of the SDP milestones into sprints and names the model for each task. Roles, patterns (T-L-R, L-R, L, Q), risk tiers (R0–R2) and the gate are defined in AGENT-WORKFLOW.md.

- A sprint is a unit of scope with an exit gate. It has **no calendar length**: the SDP forbids deriving dates before the feasibility spikes.
- Requirement IDs come from `contracts/requirements.json` (`target_milestone`). This page changes none of them.
- Sprints 0–3 are planned to task level. Later sprints are planned to slice level and are broken into tasks when their entry dependencies are met.
- "Owner input" lists what only the owner can supply. A missing input blocks only the tasks that name it; other tasks continue.
- Decision IDs below are the entries that are `open` or `proposed` in `contracts/decisions.json` on 2026-11-11. The lead confirms the exact dependency of each task at sprint planning.

Model column: **Luna** = `gpt-6-luna`, effort `max` (general worker). **Terra** = `gpt-6.1-terra`, effort `high` (test worker). **Lead** = Claude. **Owner** = product owner.

## Phase overview

| Phase | Sprint | Milestone | Outcome | Main model mix |
| --- | --- | --- | --- | --- |
| A. Foundation | 0 | M1 close-out | The workflow is proven on real tasks; M1 gaps closed in source | Luna tooling, Terra tests |
| B. Events and audio | 1 | M2 | Deterministic event detector and arbitration | Terra first, then Luna |
| B. Events and audio | 2 | M2 | Verified package cache: bounded, hostile-input safe, atomic | Terra first, then Luna, Lead reads R2 |
| B. Events and audio | 3 | M2 | Native audio owner and the independent critical warning path | Luna, Terra harness, Owner measures |
| C. Vision and overlay | 4 | M3 | Calibrated capture admission and observation boundary | Luna spike, then Terra first |
| C. Vision and overlay | 5 | M3 | Risk heuristic and alert policy | Terra first, then Luna |
| C. Vision and overlay | 6 | M3 | Overlay window, banner stage, Session Monitor | Luna, Terra state tests |
| D. Demonstration | 7 | M4a | One recorded live evidence match | Owner, Luna fixes |
| D. Demonstration | 8 | M4b | Local OBS highlights | Terra first, then Luna |
| E. Later | 9+ | M5–M8 | Voice, memory and advice; optional cloud and peer; premium; release | Replanned after the model feasibility spike |

## Sprint 0 — Pilot and M1 close-out

**Goal.** Run the workflow end to end on small real tasks, and close the M1 gaps that source work can close.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S0-1 | `tools/gate.py`: one command for format, lint, tests, frontend build and specification validation; one line per check; full logs to a file | Luna | L | R0 | source |
| S0-2 | `tools/update_manifest.py`: refresh `package-manifest.json` from the tree (the repository has no generator today) | Luna | L | R0 | source |
| S0-3 | Failing tests from GSI-INTAKE-SPEC for the known M1 gaps: hero-name mismatch during a session, ownership state after returning to the menu, a client that stalls mid-request | Terra | T-L-R 1 | R2 | synthetic |
| S0-4 | Make S0-3 pass | Luna | T-L-R 2 | R2 | synthetic |
| S0-5 | Review of S0-4 | Terra | T-L-R 3 | R2 | source |
| S0-6 | Failure-injection test for containment (CORE-002 exit code 1) and a cold offline startup test | Terra | L-R | R2 | synthetic |
| S0-7 | Settings UI (locale) through the `settings-patch` contract | Luna | L-R | R1 | synthetic |
| S0-8 | Record the lead's cost per task and adjust the limits in AGENT-WORKFLOW section 7 | Lead | n/a | R0 | n/a |

- **Owner input.** Commit authority for the sprint. A decision on the uncommitted `m2/event-tiers` specification change. Release build of the hardened revision and the live re-test in M1-STATUS section 4 item 1, including assumption A-10. Resource runs per the NFR protocol.
- **Exit gate.** Every pattern ran at least once through the gate. `tools/gate.py` passes on the branch. M1-STATUS states which M1 exit items are met in source and which wait for native or live evidence.

## Sprint 1 — Event detector (M2)

**Goal.** A pure crate turns admitted game snapshots into catalog events, with tiers and arbitration, under injected monotonic time. Requirements: MEDIA-001, MEDIA-002, MEDIA-003.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S1-1 | Table-driven tests from `contracts/event-catalog.json` and EVENT-AUDIO-SPEC section 2: taxonomy, tier ranges, the 18-second chain, streak count and reset, re-arm, respawn unvoiced, unknown input produces no event | Terra | T-L-R 1 | R1 | synthetic |
| S1-2 | Arbitration tests: one winner per admitted tick, rank order, ties, suppression | Terra | T-L-R 1 | R1 | synthetic |
| S1-3 | `crates/kohbor-events`: detector and arbiter | Luna | T-L-R 2 | R1 | synthetic |
| S1-4 | Review of S1-3 | Terra | T-L-R 3 | R1 | source |
| S1-5 | Replay tool: run recorded captures through the detector and print the event timeline | Luna | L-R | R0 | synthetic |
| S1-6 | Wire the detector into the host behind the read model; expose event availability in runtime status | Luna | L-R | R1 | synthetic |

- **Owner input.** A capture with a double kill, a triple kill, two kills more than 18 s apart, and a streak of three or more ended by a death (assumptions E-3, E-4; also E-1, E-2). Acknowledgement status of DEC-EVENT-AUDIO.
- **Exit gate.** All detector tests pass. The replay of the owner's capture matches the game's announcements, or the affected event is reported unavailable and the assumption stays open. No event is emitted from stale or unowned data.

## Sprint 2 — Verified package cache (M2)

**Goal.** A media package is parsed, bounded, verified and installed atomically; activation is a separate step. Requirements: MEDIA-004, MEDIA-006, MEDIA-007, MEDIA-008.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S2-1 | Hostile-package tests from MEDIA-PACK-SPEC and the manifest fixtures: path traversal, oversize entry, expansion bomb, wrong hash, wrong media format, unpaired binding, tier rule violations | Terra | T-L-R 1 | R2 | synthetic |
| S2-2 | Install and activation tests: interruption between every two steps leaves the previous package active and no partial files | Terra | T-L-R 1 | R2 | synthetic |
| S2-3 | `crates/kohbor-pack`: manifest parsing, bounded extraction, verification, staging, atomic install, activation pointer | Luna | T-L-R 2 | R2 | synthetic |
| S2-4 | Review of S2-3; the lead reads the full diff | Terra, Lead | T-L-R 3 | R2 | source |
| S2-5 | Trust-domain separation tests and code: community packages never satisfy a check that belongs to another domain | Terra, then Luna | T-L-R | R2 | synthetic |
| S2-6 | Package authoring tool for a clip folder plus mapping file: resample, build the manifest with tiers, write provenance. Output stays outside the repository | Luna | L-R | R0 | synthetic |

- **Owner input.** DEC-PACK-CRYPTO (signature scheme and trust roots): until it is decided, S2-3 implements hash verification and the signature step reports "unavailable". Rights documents for any package that will be distributed. A decision on the AGENTS.md rule about source-brand assets and real-person voices before such a package ships with the product.
- **Exit gate.** Every hostile fixture is rejected with its specific reason. A corrupt or interrupted install never becomes active.

## Sprint 3 — Native audio and the critical warning path (M2)

**Goal.** One native audio owner plays package audio with deterministic preemption, and the critical warning path works without the web view. Requirements: MEDIA-005, ALERT-001, ALERT-007, QUALITY-001.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S3-1 | Feasibility spike: native output API, device loss and device change behavior; written result with a recommendation | Luna | L | R0 | native observation |
| S3-2 | Tests against a fake sink: one owner, priority and preemption order, mute semantics, no overlap, behavior when the device disappears | Terra | T-L-R 1 | R1 | synthetic |
| S3-3 | Audio owner and queue | Luna | T-L-R 2 | R1 | synthetic |
| S3-4 | Critical-media readiness: state tests, then the readiness row in runtime status | Terra, then Luna | T-L-R | R1 | synthetic |
| S3-5 | Critical warning path independent of the web view | Luna | L-R | R2 | synthetic |
| S3-6 | Latency harness: event admission to first audio frame submitted, with receipts | Terra | L-R | R1 | synthetic (engineering fixture) |
| S3-7 | Review of S3-3 and S3-5 | Terra, Lead | T-L-R 3 | R2 | source |

- **Owner input.** DEC-CRITICAL-MEDIA and authorized critical-alert media (neither existing clip set contains warning lines). Acoustic latency measurement on hardware for QUALITY-001: the harness result is an engineering fixture, not acoustic evidence.
- **Exit gate (SDP M2).** Preemption, corrupt-package and latency engineering fixtures pass. QUALITY-001 is reported as measured or NOT_RUN, never inferred.

## Sprint 4 — Capture and observation boundary (M3)

Requirements: VISION-001, VISION-002, VISION-003, VISION-004, DATA-005, DATA-006.

| Slice | Work | Model |
| --- | --- | --- |
| Spike | Screen capture feasibility on the supported matrix; cost per frame; exclusive-fullscreen behavior | Luna |
| Tests | Geometry, calibration admission, freshness, "missing requires continuing coverage", capture-source separation | Terra |
| Code | Capture, calibration and the observation boundary (no unobserved enemy field is ever filled) | Luna |
| Review | R2 for the observation boundary | Terra, Lead |

- **Owner input.** DEC-MODELS, DEC-GAME-DATA, DEC-NATIVE-EVIDENCE; calibration captures; hardware access.

## Sprint 5 — Risk and alert policy (M3)

Requirements: ALERT-002, ALERT-003, ALERT-004, ALERT-005, ALERT-006, QUALITY-005.

| Slice | Work | Model |
| --- | --- | --- |
| Tests | Risk is labeled a heuristic; audible gank needs evidence; absence-episode deduplication; correction; missing-notification policy | Terra |
| Code | Risk model and alert policy on top of Sprint 1 events and Sprint 4 observations | Luna |
| Harness | Warning-quality scoring over recorded sessions | Terra |

- **Owner input.** DEC-RISK-DETAIL; recorded sessions with labeled outcomes.

## Sprint 6 — Overlay, banner stage and Session Monitor (M3)

Requirements: UI-001, UI-002, UI-003, UI-004, UI-005, UI-006, UI-008, QUALITY-003, QUALITY-004.

| Slice | Work | Model |
| --- | --- | --- |
| Code | Independent overlay window; reserved banner stage (720 × 220) that plays the package's silent video with the audio owner; companion chip; no HP or mana row | Luna |
| Code | Session Monitor field topology, unknown cards, inspector, keyboard and focus | Luna |
| Tests | State tests for every honest-state chip; overlay never covers the protected game regions; focus order | Terra |
| Harness | FPS preservation measurement protocol | Terra |

- **Owner input.** DEC-VISUALS and DEC-INTERFACES; the overlay decisions already made in the mock-up (no HP/mana row, banner stage, pre-rendered video) recorded in UX-SPEC; FPS runs on hardware.
- **Exit gate (SDP M3).** Geometry, freshness and unknown tests pass; initial native vision and acoustic evidence exists.

## Sprint 7 — First demonstration (M4a)

| Slice | Work | Model |
| --- | --- | --- |
| Prepare | Evidence-recording method; demo checklist; defect fixes found in rehearsal | Luna |
| Verify | Regression tests for each rehearsal defect before its fix | Terra |
| Run | One recorded live evidence match | Owner |

- **Exit gate (SDP M4a).** The recorded match meets demo acceptance. It certifies the demonstration only.

## Sprint 8 — Local OBS highlights (M4b)

Requirements: OBS-001, OBS-002, OBS-003, OBS-004, OBS-005.

| Slice | Work | Model |
| --- | --- | --- |
| Tests | Explicit session start; observe before audio suppression; measured clip coverage; recorder failure isolation | Terra |
| Code | OBS integration and clip trimming | Luna |
| Run | Native OBS evidence | Owner |

- **Owner input.** DEC-OBS-BUDGET; OBS prerequisites.

## Phase E — Later milestones

| Milestone | Scope | Planning note |
| --- | --- | --- |
| M5 | VOICE-001..007, MEM-001..005, ADVICE-001, QUALITY-006, QUALITY-007 | Starts with a local-model feasibility spike (Luna) and per-language benchmarks (Terra harness). Needs DEC-MODELS, DEC-VOICE-BUFFER, DEC-RETENTION |
| M6 | CORE-003, ADVICE-002..004, PEER-001..010, SOCIAL-001..003 | Optional services; separate consent and service designs first. Needs DEC-SOCIAL, DEC-SESSION |
| M7 | UI-007, PREMIUM-001..003 | Only if scheduled; DEC-PREMIUM is deferred |
| M8 | UPDATE-001..004, QUALITY-008 | Release candidate; signing and promotion need explicit owner authorization |

The routing rule does not change in later phases: tests, harnesses and reviews go to Terra; everything else goes to Luna.

## Backlog status

The lead updates this table as tasks pass the gate. `Commit` is empty until the owner has authorized the commit.

| Task | Status | Report disposition | Commit |
| --- | --- | --- | --- |
| S0-1 … S0-8 | not started | n/a | |
