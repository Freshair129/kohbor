---
title: "Software Requirements Specification"
doc_id: "SRS"
status: "draft"
version: "0.3.8"
updated: "2026-11-21"
owner: "Product Owner"
approval_status: "not_approved"
---

# Software Requirements Specification (SRS)

## 1. Introduction

### 1.1 Purpose

This SRS defines observable behavior, data boundaries, quality attributes and constraints for **Kohbor / เกาะเบาะ**, a new Windows desktop companion for Dota 2. It guides engineering, verification and acceptance.

The [PRD](../product/PRD.md) defines needs, outcomes, scope and priorities; this SRS translates them into verifiable obligations. The owner selected the product name. The builder defines its visual identity and unresolved stable technical identifiers without changing functionality, permissions or acceptance.

### 1.2 Scope and document status

The full MVP covers local telemetry, admitted visible-screen observations, deterministic warnings, event media, control window and passive overlay, manual advice, local voice conversation, local history/memory, bounded summaries and explicit local highlights recording. Guest team-status sharing, private social coordination, the edge quick panel (UI-009), the hero pick card with consented public statistics (UI-010, STATS-001–003), and the deck draft strip (UI-012) are extensions. Premium authority and advanced services remain deferred unless explicitly scheduled.

The first demonstration is a limited delivery milestone, not completion of the full MVP. Action-damage analysis remains subject to its recorded scope/data decision. Game injection, hidden enemy data, automatic gameplay actions, cloud voice/memory processing and inherited product compatibility are outside this baseline.

Version 0.3.7 is a reviewable draft. The owner-selected name is recorded; the requirements baseline, implementation and native/live acceptance have not been approved. Its organization follows conventional requirements-engineering practice; no formal standards-conformance certification is asserted.

### 1.3 References and document relationships

| Document | Responsibility |
|---|---|
| [PRD](../product/PRD.md) | Product objectives, scope, priorities and success |
| [SAD](../design/SAD.md) | System boundaries, components and architectural decisions |
| [SDD](../design/SDD.md) | Detailed algorithms, state machines and design |
| [ICD](../design/ICD.md) | Interfaces, messages, errors and compatibility |
| [DDD](../design/DDD.md) | Logical data, ownership, persistence and migrations |
| [NFR](../quality/NFR.md) | Quality budgets, environments and measurement definitions |
| [Test Plan](../quality/TEST-PLAN.md) | Verification procedures and evidence |
| [RTM](RTM.csv) | Requirement-to-design/test/evidence traceability |

Accepted owner decisions and PRD scope constrain this SRS. The [requirements registry](../../contracts/requirements.json) is the structured representation of its detailed requirement register, maintained at the same baseline. It has no independent authority over the PRD or accepted decisions. Generated tables and fixtures cannot expand permissions. A conflict stops the affected work until the governing decision and documents are reconciled; unrelated approved work may continue.

### 1.4 Terms and conventions

| Term | Meaning |
|---|---|
| SHALL / MUST | Required behavior within the stated scope |
| SHOULD / MAY | Recommendation / permitted implementation choice |
| GSI | Game State Integration: permitted local game telemetry |
| CV | Computer vision applied to admitted visible images |
| PTT / STT / TTS | Push-to-talk / speech recognition / speech synthesis |
| Admitted observation | Input passing source, identity, session and validity checks |
| Fresh / stale / unknown | Current admitted value / expired prior value / no admitted value |
| Estimated | Derived through a declared method; not directly observed |
| Session epoch | Native generation fencing work belonging to a match context |
| Complete session | Session satisfying the finalized completion-evidence contract |
| Base / premium | Presentation capabilities, not additional data permissions |
| Certified configuration | Exact environment supported by recorded acceptance evidence |
| RTM | Requirements Traceability Matrix |

## 2. Overall description

### 2.1 Product perspective and context

The native host owns admission, critical decisions, audio, cancellation, protected operations and resource accounting. The control window presents configuration and inspection; the transparent overlay and HUD present passive gameplay information and remain click-through under UI-001. UI-010 uses the passive overlay during the draft. UI-009 permits one separate orb window with bounded mouse hits and no keyboard focus.

Local gameplay is independent of account, cloud, premium and relay availability. Missing coverage remains unavailable. Read-only integration neither controls the game nor obtains hidden enemy state.

### 2.2 User classes

| User class | Principal need |
|---|---|
| Guest player | Start and use local functions offline |
| Active player | Receive timely, non-disruptive evidence-based information |
| Voice/advice user | Obtain local answers with explicit capability/failure states |
| Returning player | Inspect local history and control retained memory |
| Consenting teammate | Share limited own vitals with authorized peers |
| Signed-in social user | Manage private relationships and invitations |
| Maintainer/validator | Produce reproducible builds and verifiable releases |

Roles do not merge consent, source ownership, account authorization or subscription authority.

### 2.3 Operating environment

The initial acceptance target is Windows x64: Windows 10 build 19045 and Windows 11 build 26100; live 5v5 All Pick Ranked/Unranked; vision at 1920×1080, 16:9, primary display, 100% DPI, default bottom-left minimap and borderless fullscreen. Additional configurations require their own evidence.

Tauri/Rust, React/TypeScript and WebView2 form the baseline architecture. Exact versions are locked during initialization. The NFR document defines reference hardware and measurement conditions. A second-screen control window does not certify capture across arbitrary monitors or DPI settings.

### 2.4 Assumptions and dependencies

Affected capabilities depend on telemetry, capture permissions, supported devices, licensed media and reproducible model/catalog artifacts. Models need feasibility/quality evidence; optional services need authorization and operational contracts.

Missing prerequisites produce unavailable states, not simulated success. Dependencies and safe interim behavior belong in the [decision register](../../contracts/decisions.json).

## 3. External interface requirements

| Interface | Boundary and governing detail |
|---|---|
| Game input | Loopback telemetry and owned integration configuration; read-only screen capture |
| Human interaction | Control window, passive overlay, keyboard, microphone and audio output |
| Optional public hero statistics | V1 suggestions fetch fixed public tables at most four times/day with no user- or match-specific data; filter locally. The separate pick-card request carries own hero ID and optional manually selected bracket only |
| Local storage/media | Settings, journals, snapshots, verified packages and clips |
| Local recording | Explicitly started, authenticated loopback OBS control |
| Advice services | On-device processing; separate consent and closed projection for optional cloud |
| Peer/social services | Native-owned team transport; independently authorized account broker |
| Updates/authoring tools | Trusted signed updates and bounded local installation requests |

The ICD freezes endpoints, payloads, units, authentication, limits, timeouts, idempotency and versioning. Draft schemas are incomplete protocols; UI controls cannot grant native/server authority.

## 4. Logical data requirements

The DDD defines settings, sessions, observations, events, catalogs, summaries, memory, media, clips, memberships and account records, including ownership, keys, lifetime, consumers and deletion.

Applicable facts carry source, availability, age and generation. Unknown differs from zero/known-empty. Match time, monotonic durations and durable timestamps are distinct. Transient peer/voice data cannot enter history or cloud through incidental serialization.

Persistence requires versioning, atomic updates, recovery and retention. Memory erasure survives restart without rebuilding forgotten aggregates. Logical entities do not each require a database table.

## 5. Requirement organization

The register contains **92 stable IDs**, with scope, priority, obligation, failure behavior, acceptance scenario, evidence class, contracts and dependencies. Moving documents does not change IDs.

| Classification | Interpretation |
|---|---|
| Functional | Observable behavior, state transitions and failure responses |
| Quality | Measurable latency, resources, recognition quality, usability and reliability |
| Constraint | Required environment, technology boundary, protocol or permitted source |
| Security/privacy | Authorization, consent, confidentiality, integrity and lifecycle invariants |

Classification may overlap; prefixes identify domains. Scope differs from priority; requirement acceptance differs from implementation evidence. Behavior/scope changes require a decision and impact analysis.

## 6. Verification and traceability

Use inspection, analysis, demonstration and testing as specified in the Test Plan. Schema, unit, socket, browser, native, live-game, acoustic and resource evidence establish different claims.

The RTM connects PRD needs, SRS IDs, design/interfaces, tests and evidence bidirectionally. Changes update this chain to expose unsupported implementation or untested obligations.

Record NOT_RUN, PASS, FAIL, SKIP or INDETERMINATE with revision, environment, method, expected/actual results and limitations. Missing evidence cannot pass. Quality thresholds and sampling definitions come from the accepted baseline and NFR, not retrospective test tuning.

## 7. Open decisions, review and approval

Open model, interface, session, retention, media-trust, voice-buffer, risk-admission, recording-budget and service decisions identify affected requirements and closure evidence. Resolve them before affected integration/acceptance, without weakening defaults.

Baseline approval requires product scope, engineering feasibility/interfaces and verification coverage reviews. Record actual reviewers, dates and decisions. Release approval additionally requires applicable native/live and operational evidence; none is claimed here.

## Diagram correspondences

The [diagram catalog](../diagrams/README.md) supplies controlled architectural, data and behavioral views. Its mappings appear in the RTM and detailed rows below. DGM-002, DGM-004, DGM-005 and DGM-017 predate DEC-EDGE-ORB and DEC-PICK-STATS. Use the revised text for those extensions until the four views are regenerated. The figures supplement the same baseline; their arrows and short guards cannot broaden a permission or replace a complete textual requirement.

## 8. Atomic software requirements

<!-- BEGIN GENERATED REQUIREMENTS -->

This controlled section and the structured registry represent the same requirements baseline. Edit an atomic row through the registry, regenerate, and review the complete SRS context for consistency. The registry is an authoring mechanism, not a higher authority. Acceptance scenarios are test designs; new-application evidence is NOT_RUN until actual execution is recorded.

### CORE

#### CORE-001 — Account-independent local startup

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-001 · **Primary design:** CMP-STARTUP · **Design components:** CMP-STARTUP

The local core shall start without account, network or premium authority.

**Failure behavior:** Do not lock gameplay functionality behind sign-in or an entitlement check. Account work runs on its own worker and is never awaited by intake, overlay or alert paths.

**AC-CORE-001:** Given a cold installed application with no account and no network; when the player starts the application; then local setup, permitted intake, overlay and available local functions are usable; account services show unavailable; account work runs on its own worker and intake, overlay and alert paths never await it.

**Verification:** synthetic, native, live; procedures TP-01, UAT-01; status NOT_RUN.

**Elaboration:** [PRD.md](../product/PRD.md), [SAD.md](../design/SAD.md)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-007](../diagrams/source/DGM-007-startup-sequence.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd), [DGM-018](../diagrams/source/DGM-018-release-update.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### CORE-002 — Resource containment before UI

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-STARTUP · **Design components:** CMP-STARTUP, CMP-PERFORMANCE

The application shall establish the 10% Windows CPU Job Object cap before creating UI or sidecars.

**Failure behavior:** Do not continue with an uncontained runtime.

**AC-CORE-002:** Given job creation, configuration or assignment fails; when startup is attempted; then the application records a safe error and exits with code 1 before WebView or sidecar creation.

**Verification:** synthetic, native, live; procedures TP-01, TP-13, UAT-05; status NOT_RUN.

**Elaboration:** [PRD.md](../product/PRD.md), [SAD.md](../design/SAD.md)

**Contracts:** [runtime-status.schema.json](../../contracts/runtime-status.schema.json)

**Diagram views:** [DGM-003](../diagrams/source/DGM-003-deployment.mmd), [DGM-007](../diagrams/source/DGM-007-startup-sequence.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### CORE-003 — Account transitions preserve local state

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-001 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL, CMP-STARTUP

Account signout, revoke or switch shall invalidate account requests and credentials while preserving local gameplay/history.

**Failure behavior:** Never allow an old account response to populate the new account. Account work runs on its own worker and is never awaited by intake, overlay or alert paths.

**AC-CORE-003:** Given a local match is active with an optional signed-in account; when the account is revoked or switched; then protected account operations stop and late account results are discarded; valid local core continues; account work runs on its own worker and intake, overlay and alert paths never await it.

**Verification:** synthetic, native, live; procedures TP-01, UAT-01; status NOT_RUN.

**Elaboration:** [PRD.md](../product/PRD.md), [SAD.md](../design/SAD.md)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### CORE-004 — Owned integration configuration

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-001 · **Primary design:** CMP-INGEST · **Design components:** CMP-INGEST, CMP-STARTUP

Game integration setup shall write only its owned config and verified chosen loopback port.

**Failure behavior:** Port exhaustion or write failure returns an actionable error without overwriting unrelated files.

**AC-CORE-004:** Given ports 3000 through 3010 contain occupied and free candidates; when setup selects an available configured port; then the app and owned game config agree; unrelated game configuration is preserved.

**Verification:** synthetic, native, live; procedures TP-01, UAT-01; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES, DEC-GSI-INTAKE.

**Elaboration:** [PRD.md](../product/PRD.md), [SAD.md](../design/SAD.md), [GSI-INTAKE-SPEC.md](../design/GSI-INTAKE-SPEC.md)

**Contracts:** [runtime-status.schema.json](../../contracts/runtime-status.schema.json), [user-settings.schema.json](../../contracts/user-settings.schema.json), [settings-patch.schema.json](../../contracts/settings-patch.schema.json)

**Diagram views:** [DGM-003](../diagrams/source/DGM-003-deployment.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### CORE-005 — Supported matrix is explicit

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-001 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-STARTUP

The product shall label mode, OS and capture configurations outside the evidence matrix unverified or unsupported.

**Failure behavior:** Do not reinterpret an unsupported mode as the default supported mode.

**AC-CORE-005:** Given an untested mode or vision geometry is selected; when a source-dependent conclusion would be shown; then the unsupported capability is identified and independent valid local capabilities remain available.

**Verification:** synthetic, native, live; procedures TP-01, UAT-01; status NOT_RUN.

**Open dependencies:** DEC-GSI-INTAKE.

**Elaboration:** [PRD.md](../product/PRD.md), [SAD.md](../design/SAD.md), [GSI-INTAKE-SPEC.md](../design/GSI-INTAKE-SPEC.md)

**Contracts:** [runtime-status.schema.json](../../contracts/runtime-status.schema.json)

**Diagram views:** [DGM-013](../diagrams/source/DGM-013-match-lifecycle.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### DATA

#### DATA-001 — Freshness boundary

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-002 · **Primary design:** CMP-INGEST · **Design components:** CMP-INGEST

Own-source data shall be fresh only while admitted source age is at most 2000ms.

**Failure behavior:** Withdraw live assertions when stale; do not fabricate a new observation.

**AC-DATA-001:** Given a valid own observation at age 2000ms followed by age 2001ms; when the UI consumes both snapshots without a new source observation; then the first may be fresh and the second is stale; invalid input and polling do not renew the age.

**Verification:** synthetic, native; procedures TP-02, UAT-02; status NOT_RUN.

**Open dependencies:** DEC-GSI-INTAKE.

**Elaboration:** [DDD.md](../design/DDD.md), [ICD.md](../design/ICD.md), [GSI-INTAKE-SPEC.md](../design/GSI-INTAKE-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [event.schema.json](../../contracts/event.schema.json), [runtime-status.schema.json](../../contracts/runtime-status.schema.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-019](../diagrams/source/DGM-019-source-freshness.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### DATA-002 — Unknown is not zero

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-002 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-INGEST

Missing, invalid and unverified values shall remain explicitly unavailable.

**Failure behavior:** Never use parser defaults to imply alive, safe, ready or known empty.

**AC-DATA-002:** Given HP is absent in one sample and genuinely zero in another; when read models are built; then missing HP becomes unknown/null and admitted zero remains zero.

**Verification:** synthetic, native; procedures TP-02, UAT-02; status NOT_RUN.

**Elaboration:** [DDD.md](../design/DDD.md), [ICD.md](../design/ICD.md), [GSI-INTAKE-SPEC.md](../design/GSI-INTAKE-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [event.schema.json](../../contracts/event.schema.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-019](../diagrams/source/DGM-019-source-freshness.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### DATA-003 — Own-source admission

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-002 · **Primary design:** CMP-INGEST · **Design components:** CMP-INGEST

Own-player conclusions shall require supported game scope, actual playing activity and verified active local ownership. From an admitted supported payload, expose map.radiant_score, map.dire_score and signed map.clock_time as separate bounded renderer fields; preserve negative clock values before the horn, keep each field independently unknown or invalid when absent or malformed, and never replace unknown with zero. The existing combined score may remain an internal sum but shall not substitute for either team score.

**Failure behavior:** Keep own-player source unavailable when ownership or supported scope is unverified; selected-unit data is not proof of ownership. Missing or invalid map scores and clock remain independently unknown or invalid and are never filled with zero or inferred from the combined score.

**AC-DATA-003:** Given an authenticated supported playing payload with verified or unverified local ownership, and map radiant_score, dire_score or clock_time independently missing, valid or malformed; when source admission and renderer projection are evaluated; then unverified ownership cannot supply own-player or peer vitals; verified map observations expose Radiant score, Dire score and signed clock separately, missing or malformed fields remain independently unknown or invalid, negative pre-horn time is preserved, and no unknown value becomes zero.

**Verification:** synthetic, native; procedures TP-02, UAT-02; status NOT_RUN.

**Open dependencies:** DEC-GSI-INTAKE.

**Elaboration:** [DDD.md](../design/DDD.md), [ICD.md](../design/ICD.md), [GSI-INTAKE-SPEC.md](../design/GSI-INTAKE-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [event.schema.json](../../contracts/event.schema.json), [runtime-status.schema.json](../../contracts/runtime-status.schema.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-013](../diagrams/source/DGM-013-match-lifecycle.mmd), [DGM-019](../diagrams/source/DGM-019-source-freshness.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### DATA-004 — Session and clock fences

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-002 · **Primary design:** CMP-INGEST · **Design components:** CMP-INGEST

Session/source changes shall fence late work without treating every regressed clock as a new match.

**Failure behavior:** Preserve authoritative watermark/identity through temporary invalidity; require actual session evidence.

**AC-DATA-004:** Given an old response and regressed input arrive after a new session epoch; when consumers evaluate pending work; then old work is rejected and event baselines cannot generate false new-match cues.

**Verification:** synthetic, native; procedures TP-02, UAT-02; status NOT_RUN.

**Open dependencies:** DEC-GSI-INTAKE.

**Elaboration:** [DDD.md](../design/DDD.md), [ICD.md](../design/ICD.md), [GSI-INTAKE-SPEC.md](../design/GSI-INTAKE-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [event.schema.json](../../contracts/event.schema.json), [runtime-status.schema.json](../../contracts/runtime-status.schema.json)

**Diagram views:** [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-006](../diagrams/source/DGM-006-logical-erd.mmd), [DGM-013](../diagrams/source/DGM-013-match-lifecycle.mmd), [DGM-019](../diagrams/source/DGM-019-source-freshness.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### DATA-005 — Enemy observation boundary

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-002 · **Primary design:** CMP-VISION · **Design components:** CMP-VISION, CMP-INGEST

Enemy visibility and positions shall derive only from admitted visible capture data.

**Failure behavior:** Unknown coverage cannot be labeled safe.

**AC-DATA-005:** Given game telemetry contains no enemy position and capture is unavailable; when the live field renders enemies; then no current position or hidden enemy stats are invented.

**Verification:** synthetic, native; procedures TP-02, UAT-02; status NOT_RUN.

**Elaboration:** [DDD.md](../design/DDD.md), [ICD.md](../design/ICD.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [event.schema.json](../../contracts/event.schema.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### DATA-006 — Static game data provenance

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-002 · **Primary design:** CMP-INGEST · **Design components:** CMP-INGEST

Static hero/item/ability data shall identify patch, source revision/hash and verified coverage.

**Failure behavior:** No guessed damage formula or patch compatibility.

**AC-DATA-006:** Given a catalog lacks a known patch or class validation; when advice or skill UI requests those facts; then the facts are marked partial/unsupported and not described as latest.

**Verification:** synthetic, native; procedures TP-02, UAT-02; status NOT_RUN.

**Open dependencies:** DEC-GAME-DATA.

**Elaboration:** [DDD.md](../design/DDD.md), [ICD.md](../design/ICD.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [event.schema.json](../../contracts/event.schema.json)

**Diagram views:** [DGM-006](../diagrams/source/DGM-006-logical-erd.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### VISION

#### VISION-001 — Calibrated capture admission

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-002 · **Primary design:** CMP-VISION · **Design components:** CMP-VISION

Vision shall require healthy frames, matching session and validated minimap geometry.

**Failure behavior:** Preserve valid telemetry-only operation.

**AC-VISION-001:** Given capture changes monitor, size or minimap layout; when a frame arrives under an old calibration; then the old frame/coordinates are rejected and calibration readiness is explicit.

**Verification:** synthetic, native, live; procedures TP-02, UAT-02; status NOT_RUN.

**Elaboration:** [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [NFR.md](../quality/NFR.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Diagram views:** [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-013](../diagrams/source/DGM-013-match-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### VISION-002 — Missing requires continuing coverage

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-002 · **Primary design:** CMP-VISION · **Design components:** CMP-VISION

A missing-enemy episode shall require prior observed presence and continued valid coverage.

**Failure behavior:** Do not interpret empty/no frames as observed absence.

**AC-VISION-002:** Given an enemy was observed and the camera then stops yielding valid frames; when time passes beyond 5 seconds; then coverage becomes unavailable and no missing or all-clear assertion is manufactured.

**Verification:** synthetic, native, live; procedures TP-02, UAT-02; status NOT_RUN.

**Elaboration:** [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [NFR.md](../quality/NFR.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Diagram views:** [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### VISION-003 — Certified class coverage

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-002 · **Primary design:** CMP-VISION · **Design components:** CMP-VISION

Vision shall expose the actual set of certified hero classes.

**Failure behavior:** Pooled model accuracy cannot certify an individual class.

**AC-VISION-003:** Given an unvalidated class is detected; when identity is presented; then the class is marked unknown/unverified rather than silently accepted.

**Verification:** synthetic, native, live; procedures TP-02, UAT-02; status NOT_RUN.

**Elaboration:** [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [NFR.md](../quality/NFR.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### VISION-004 — Capture source separation

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-011 · **Primary design:** CMP-VISION · **Design components:** CMP-VISION, CMP-RECORDER

OBS recording shall not replace the accepted critical DXGI source without a distinct measured source-admission decision.

**Failure behavior:** A recorder or candidate virtual-camera failure cannot erase valid GSI.

**AC-VISION-004:** Given OBS is enabled for recording; when live vision continues; then the selected critical capture route remains explicit and accidental double capture is avoided.

**Verification:** synthetic, native, live; procedures TP-02, TP-11, UAT-11; status NOT_RUN.

**Open dependencies:** DEC-GAME-DATA.

**Elaboration:** [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [NFR.md](../quality/NFR.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-003](../diagrams/source/DGM-003-deployment.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### ALERT

#### ALERT-001 — Native independent warning path

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-003 · **Primary design:** CMP-ALERT · **Design components:** CMP-ALERT, CMP-RISK

Critical warnings shall be decided and admitted in native deterministic code without waiting for cloud, model, renderer or relay. In the overlay a critical warning is never hidden or delayed: when an event banner is showing, the banner keeps playing but yields (moves up 8 % of its height and shrinks to 88 % over 180 ms) and a compact critical strip, 40 px at 1080p and scaled like the banner, shows 4 px below it; with no banner showing the full critical warning shows as before (owner decision 2026-11-23, option ข, subject to the owner's mock review).

**Failure behavior:** No synchronous network, disk unpacking or inference in critical admission. Account work runs on its own worker and is never awaited by intake, overlay or alert paths.

**AC-ALERT-001:** Given network is down and a renderer/model task is stalled; a critical warning arriving while an event banner plays, in either order of the banner's preempted end and the critical start, and a critical warning with no banner; when an admitted actionable critical event occurs; then the local alert path continues and meets its acoustic gate; the critical strip shows at once below the yielding banner and stays for the critical cue's normal lifetime even after the banner fades out, the strip bottom stays within 32 px (scaled) below the banner stage's normal bottom, and with no banner the full critical warning shows as before; account work runs on its own worker and intake, overlay and alert paths never await it.

**Verification:** synthetic, native, live; procedures TP-03, UAT-03; status NOT_RUN.

**Elaboration:** [SDD.md](../design/SDD.md), [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json), [risk-golden.json](../../fixtures/risk-golden.json), [audio-outcome.schema.json](../../contracts/audio-outcome.schema.json), [event-catalog.json](../../contracts/event-catalog.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-003](../diagrams/source/DGM-003-deployment.mmd), [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-014](../diagrams/source/DGM-014-alert-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ALERT-002 — Risk is a labeled heuristic

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-003 · **Primary design:** CMP-RISK · **Design components:** CMP-RISK, CMP-ALERT

The missing-time score shall follow the documented versioned formula and be labeled heuristic.

**Failure behavior:** Invalid/non-finite input produces unavailable rather than a risk probability claim.

**AC-ALERT-002:** Given neutral headings and durations 4,5,8.5,12,20,40 seconds; when the baseline risk function runs; then outputs are 0,0,0.35,0.70,0.46,0.10 within declared floating tolerance.

**Verification:** synthetic, native, live; procedures TP-03, UAT-03; status NOT_RUN.

**Open dependencies:** DEC-RISK-DETAIL.

**Elaboration:** [SDD.md](../design/SDD.md), [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json), [risk-golden.json](../../fixtures/risk-golden.json)

**Diagram views:** [DGM-014](../diagrams/source/DGM-014-alert-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ALERT-003 — Evidence-gated audible gank

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-003 · **Primary design:** CMP-ALERT · **Design components:** CMP-ALERT, CMP-RISK

A high score alone shall not trigger audible gank; admitted coordinated multi-enemy movement/disappearance or independently verified position evidence is required.

**Failure behavior:** Suppress during pregame, own death, pause/nonadvancing clock, invalid game frame or capture gap.

**AC-ALERT-003:** Given score crosses the configured danger threshold with no qualifying movement/position evidence; when voice admission is evaluated; then no audible gank is emitted; a heuristic readout does not claim confirmed threat.

**Verification:** synthetic, native, live; procedures TP-03, UAT-03; status NOT_RUN.

**Open dependencies:** DEC-RISK-DETAIL.

**Elaboration:** [SDD.md](../design/SDD.md), [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json), [risk-golden.json](../../fixtures/risk-golden.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-014](../diagrams/source/DGM-014-alert-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ALERT-004 — Absence episode deduplication

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-003 · **Primary design:** CMP-ALERT · **Design components:** CMP-ALERT, CMP-RISK

Audible gank shall be deduplicated by absence episode with global 30s and per-contributor 60s minimum intervals.

**Failure behavior:** Re-arm only on confirmed continuous reappearance; one-frame noise does not re-arm.

**AC-ALERT-004:** Given a notified hero remains absent after both cooldowns expire; when a new risk evaluation occurs; then cooldown expiry alone does not create a new notification episode.

**Verification:** synthetic, native, live; procedures TP-03, UAT-03; status NOT_RUN.

**Open dependencies:** DEC-RISK-DETAIL.

**Elaboration:** [SDD.md](../design/SDD.md), [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json), [risk-golden.json](../../fixtures/risk-golden.json)

**Diagram views:** [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-014](../diagrams/source/DGM-014-alert-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ALERT-005 — Evidence-based correction

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-003 · **Primary design:** CMP-ALERT · **Design components:** CMP-ALERT, CMP-RISK

An audible risk correction shall require confirmed reappearance of all previously alerted heroes and score below clear.

**Failure behavior:** Coverage loss silently suspends unsupported claims and retains cooldown fences.

**AC-ALERT-005:** Given an alert was issued and score later falls solely by time decay; when the clear threshold is crossed; then no audible all-clear/correction is emitted from decay alone.

**Verification:** synthetic, native, live; procedures TP-03, UAT-03; status NOT_RUN.

**Elaboration:** [SDD.md](../design/SDD.md), [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json), [risk-golden.json](../../fixtures/risk-golden.json)

**Diagram views:** [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-014](../diagrams/source/DGM-014-alert-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ALERT-006 — Missing notification policy

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-003 · **Primary design:** CMP-ALERT · **Design components:** CMP-ALERT, CMP-RISK

A visibility-missing notification shall use absence strictly longer than 5s, once per hero/episode and global audio cooldown at least 10s.

**Failure behavior:** Drop missing-event audio while critical audio is active; no substituted media.

**AC-ALERT-006:** Given a previously observed enemy is absent 5000ms then 5001ms under valid coverage; when missing notification admission runs; then no threshold event at 5000ms; an eligible event after 5000ms obeys dedup and audio priority.

**Verification:** synthetic, native, live; procedures TP-03, UAT-03; status NOT_RUN.

**Elaboration:** [SDD.md](../design/SDD.md), [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json), [risk-golden.json](../../fixtures/risk-golden.json), [audio-outcome.schema.json](../../contracts/audio-outcome.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ALERT-007 — Critical media readiness

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-003 · **Primary design:** CMP-ALERT · **Design components:** CMP-ALERT, CMP-RISK, CMP-MEDIA

Critical audio readiness shall require valid mapped critical media before the demo warning gate passes.

**Failure behavior:** Do not substitute TTS, an unrelated clip or silently declare visual-only readiness.

**AC-ALERT-007:** Given gank/danger/revision media is missing or invalid; when critical readiness is assessed; then the product exposes not-ready and the acoustic gate cannot pass.

**Verification:** synthetic, native, live; procedures TP-03, UAT-03; status NOT_RUN.

**Open dependencies:** DEC-CRITICAL-MEDIA.

**Elaboration:** [SDD.md](../design/SDD.md), [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json), [risk-golden.json](../../fixtures/risk-golden.json), [event-catalog.json](../../contracts/event-catalog.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-014](../diagrams/source/DGM-014-alert-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ALERT-008 — Map-objective timer alerts

**Scope:** extension · **Priority:** extension · **Target milestone:** M3 · **Basis:** owner_decision / DEC-EVENT-RUNE-SPAWN

**Product needs:** PN-005 · **Primary design:** CMP-ALERT · **Design components:** CMP-ALERT, CMP-MEDIA, CMP-READMODEL

The host shall raise non-critical map-objective alerts derived only from the GSI game clock (map.clock_time) while map.game_state is in progress and the game is not paused, using one patch-versioned timing table that names a source for every row. Each objective (water, power, bounty and wisdom rune, Tormentor first spawn) has a Settings switch, on by default, and zero to two pre-warning lead times of 5 to 60 s each; the defaults are 30 s and 15 s for water and power runes and none for the others; a spawn alert always follows while the objective is on. Roshan respawn and Tormentor respawn are reported unavailable. Each alert shows as a compact rounded card with an original objective glyph and a colour accent from the HUD's fixed dark palette (the HUD never follows the app theme), centred directly under the danger meter (RISK) with a 4 px gap by default, where it may enter the top of the protected announcement region by at most 24 px at 1080 (scaled with screen height) under the UI-015 allowance; the objective name stays in English and the sentence follows the saved app language, which the host sends with every alert (Thai examples: “Power rune จะเกิดใน 30 วินาที”, “Water rune เกิดแล้ว”; the English UI stays fully English). An event video banner may cover the card. The tone plays on the single-slot sink as soon as it is idle, within 5 s of the due time; a clip that is playing is never cut, ducked or delayed for it, and a tone not started within 5 s is dropped and logged as suppressed. Each objective plays its own original locally synthesized short soft tone (distinct pitch or pattern) at the announcer channel volume. The card's size and position follow the UI-015 layout. An opt-in local diagnostic may log only the GSI map key names and roshan_* values; the info log lines objective_alert_shown (kind, lead_s, clock, and whether the tone played, the card was covered or the tone was suppressed), objective_prefs_changed and layout_changed carry no user identity.

**Failure behavior:** When the clock is missing, stale, paused or not in progress, no alert fires and nothing is derived from wall or browser time. A backward or forward clock jump fires no backlog of passed alerts. When an announcer clip or critical warning is playing, the text alert still shows at its due time and the tone is not played (owner decision 2026-11-22, DEC-EVENT-RUNE-SPAWN revision 3); any game clip that starts during the tone preempts it, so the tone never stops, ducks or delays a clip or the critical cue. A timing without a sourced row in the table, or a respawn without a death signal, stays unavailable and is never shown as a time; rows not yet confirmed against official patch notes are marked so in the table. No memory reading, game input or hidden enemy information. Invalid lead times or a corrupt prefs file keep the last valid prefs (defaults if none) and are reported; they never change the honesty rules of the scheduler.

**AC-ALERT-008:** Given a GSI clock stream in progress with all objectives enabled, including pauses, stale gaps, a backward jump, a forward jump and an announcer clip or critical warning playing at a due time; per-objective lead times set to valid and invalid values; a kill banner shown over the card; the app language set to Thai while the HUD window starts after the main window; when the clock passes 1:30, 1:45 and 2:00, then later spawn times, pauses and jumps; then the 2:00 water rune gets exactly one 30 s and one 15 s pre-warning and one spawn alert; bounty and wisdom runes get spawn alerts only; no alert fires while paused, stale or not in progress, and no backlog fires after a jump; Roshan and Tormentor respawn show unavailable; during a playing clip the text shows on time, the tone is not played, and the clip is not stopped, ducked or delayed; a game clip that starts during the tone preempts it; a disabled objective raises nothing; each enabled lead time gives exactly one pre-warning at that offset and invalid lead times (outside 5 to 60 s, duplicates, more than two) are rejected without changing the stored prefs; the card shows the English objective name with the app-language sentence (Thai when the saved app language is Thai, whenever the HUD window started), sits centred under the danger meter by default on a dark background under a light Windows theme, and may be covered by a video banner; its tone plays once the sink is idle within 5 s of the due time, otherwise it is suppressed and logged, and an announcer or critical clip is never cut, ducked or delayed; each objective's tone differs from the others; each shown alert writes one objective_alert_shown info line.

**Verification:** synthetic, native, live; procedures TP-03, TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [PRD.md](../product/PRD.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose the scheduler home, tone synthesis and overlay transport inside existing crates; do not add crates or change Cargo.lock; do not alter the stated behavior or evidence thresholds.

### MEDIA

#### MEDIA-001 — Single event taxonomy

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

One versioned catalog shall own event IDs, labels, detector availability, event groups, bounded tunables and ordinary ranks. Missing event_group is interpreted as basic.

**Failure behavior:** Do not hand-edit generated downstream copies.

**AC-MEDIA-001:** Given duplicate IDs, duplicate nonzero ranks or invalid tunables exist; when catalog validation/generation runs; then the catalog is rejected before generated runtime tables are published.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json), [event-catalog.json](../../contracts/event-catalog.json), [event-catalog.schema.json](../../contracts/event-catalog.schema.json), [event.schema.json](../../contracts/event.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-002 — Ordinary event arbitration

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

Ordinary same-tick arbitration shall select the highest-ranked ordinary event with a ready sound in the equipped loadout, using an in-memory prewarm lookup before arbitration and keeping critical admission separate. An unmapped or invalid event is recorded and skipped so the next-ranked mapped event can play. While an ordinary clip plays, a newly admitted ordinary event of strictly higher rank preempts it: the new clip starts at once and the old clip fades out under it over 100 ms (owner decision 2026-11-23); an equal or lower rank is dropped channel_busy. A critical, correction or notice cue that preempts an ordinary clip uses the same fade and is never delayed by it.

**Failure behavior:** Do not read from disk during admission. Unmapped or invalid events are recorded and skipped; unrelated clips are never substituted. Baseline/reconnect must not invent first blood, kills, level or match-start events.

**AC-MEDIA-002:** Given one tick produces a higher-ranked ordinary event with no ready sound and a lower-ranked mapped event; a multi_kill admitted while a hero_kill clip plays, an equal-rank event during a clip, and a critical cue during an ordinary clip; when the announcer chooses media; then the unmapped event is recorded and skipped, and the next-ranked mapped event is admitted; the multi_kill starts at once while the hero_kill fades out over 100 ms and one clip_preempted {from_event, to_event, fade_ms} info line is written; the equal-rank event is dropped channel_busy; the critical cue starts without delay over the fading ordinary clip; a faded clip reports preempted, never finished.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json), [event-catalog.json](../../contracts/event-catalog.json), [audio-outcome.schema.json](../../contracts/audio-outcome.schema.json), [event-catalog.schema.json](../../contracts/event-catalog.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-003 — Respawn unvoiced

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

Respawn shall remain a detectable event without an automatic audio mapping.

**Failure behavior:** Do not treat every event ID as requiring a sound.

**AC-MEDIA-003:** Given an admitted own death transitions to respawn; when the detector publishes the transition; then history/state may reflect respawn but the announcer does not voice it.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json), [event-catalog.json](../../contracts/event-catalog.json), [audio-outcome.schema.json](../../contracts/audio-outcome.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-004 — Verified paired media

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

Event audio and its paired visual shall originate from the exact verified variant in one of at most four ordered equipped package revisions. The selected source pack remains attached to playback and preview.

**Failure behavior:** No loose-file/default-pack/generated-card/event-TTS fallback.

**AC-MEDIA-004:** Given mixed audio is corrupt while an original identity audio file is valid; when playback is requested; then the event pair is rejected and a local reason is recorded.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json), [loadout.schema.json](../../contracts/loadout.schema.json)

**Diagram views:** [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-010](../diagrams/source/DGM-010-media-install-activation.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-005 — One audio owner

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA, CMP-ALERT, CMP-VOICE

Native audio shall own paired visual lifecycle and cancel both on stop/preemption/session fence. One exception (owner decision 2026-11-23): when a critical cue preempts an ordinary clip whose banner is showing, the audio is preempted but the banner's visual keeps playing, yielding to the critical strip, until it ends or its fail-safe time; it is still fenced by session changes and stop.

**Failure behavior:** The renderer cannot independently select or replay event media.

**AC-MEDIA-005:** Given a clip has a declared visual tail; when a critical interruption or session change occurs; then obsolete audio and visual tails terminate under the same generation; a banner whose audio was preempted by a critical cue keeps its visual until its own end while the critical strip shows.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Open dependencies:** DEC-EVENT-AUDIO.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json), [audio-outcome.schema.json](../../contracts/audio-outcome.schema.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-006 — Bounded hostile package handling

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

Package parsing shall enforce strict archive limits and reject traversal, duplicates and digest mismatch.

**Failure behavior:** Cap container/expanded total 512MiB, payload 32MiB, descriptor/registry 2MiB and payload count 2048.

**AC-MEDIA-006:** Given an archive has unsafe paths, duplicate records or expansion beyond limits; when installation validates the archive; then it fails in bounded staging with no active-registry mutation.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Open dependencies:** DEC-PACK-CRYPTO.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json), [media-install-request.schema.json](../../contracts/media-install-request.schema.json), [media-install-result.schema.json](../../contracts/media-install-result.schema.json)

**Diagram views:** [DGM-010](../diagrams/source/DGM-010-media-install-activation.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-007 — Atomic install versus activation

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

Installation remains separate from activation. An ordered loadout of up to four distinct installed package revisions, event locks and per-event match consistency shall commit atomically at the existing match and audio-lease barrier; active.json mirrors slot 1 for older builds.

**Failure behavior:** A failed or deferred loadout change cannot misreport a successful install as active. loadout.json is the commit point, its pending form is last-write-wins, and a corrupt loadout is reported rather than reset.

**AC-MEDIA-007:** Given a legacy store has active.json but no loadout.json, then a valid two-pack loadout is requested while a match or equipped audio lease blocks changes; when the store opens, migrates in memory, receives the loadout request and later resumes after the barrier; then the legacy pack is slot 1 without an eager write; the request is deferred; after the barrier loadout.json commits, active.json mirrors slot 1, reopen preserves both packs, and crashes or a mismatched mirror recover to the old or new complete loadout.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json), [media-install-request.schema.json](../../contracts/media-install-request.schema.json), [media-install-result.schema.json](../../contracts/media-install-result.schema.json), [loadout.schema.json](../../contracts/loadout.schema.json)

**Diagram views:** [DGM-010](../diagrams/source/DGM-010-media-install-activation.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-008 — Trust domains are separate

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

Package integrity, publisher trust and purchase entitlement shall be verified by their own authorities.

**Failure behavior:** Use application-owned trusted keys and newly selected signature namespace.

**AC-MEDIA-008:** Given a locally hash-valid package supplies its own public key or claims Store authority; when the installer evaluates trust; then local integrity alone grants neither trusted publisher nor purchase access.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Open dependencies:** DEC-PACK-CRYPTO.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json), [media-install-result.schema.json](../../contracts/media-install-result.schema.json)

**Diagram views:** [DGM-010](../diagrams/source/DGM-010-media-install-activation.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-009 — Per-application session volume

**Scope:** extension · **Priority:** extension · **Target milestone:** M3 · **Basis:** owner_decision / DEC-EDGE-ORB

**Product needs:** PN-013 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-ALERT

Kohbor shall change only Windows Core Audio shared-mode session volume and mute on the default render device for the Dota 2 process and the applications the user placed in a mixer slot (UI-011). It shall not inject input, access other processes' memory or files, or send session data over a network. Before the first change of an application, store its original level and mute locally, one entry for the application when its sessions agree and one per session when they differ; restore on application exit, orb/mixer disable, user Reset, and next start after abnormal exit. A per-session original whose session ended while the application still runs is dropped and logged, not reported as restored. Re-read sessions after a default-device change or new session and never set a session that has not been read. Logs contain channel, executable base name and level only.

**Failure behavior:** Do not change an unreadable, missing or unconfigured process, or an application with differing sessions whose originals cannot be recorded per session. On restore failure report it honestly and retain recovery data for the next start.

**AC-MEDIA-009:** Given a fake session manager with present, missing and unreadable sessions, applications with several sessions at equal and differing levels, and persisted original levels; when the host sets, restores, crashes and restarts, and the default device or session changes; then only eligible Dota and slot-configured application sessions change, all sessions of an application move together, differing session values are recorded and restored per session, originals restore on exit/disable/reset and after crash restart, changed devices/sessions are re-read before use, logs contain only the permitted fields, and no process input, memory or files are accessed; on the owner PC lower Dota, Chrome and an added application, kill Kohbor, restart and confirm original levels restore.

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES, DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [NFR.md](../quality/NFR.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Implement the built-in fail-closed foreground activation guard. Choose window internals within the specified input and critical alert boundaries.

### UI

#### UI-001 — Independent control and overlay

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

The product shall separate control-window chrome from the transparent always-on-top click-through gameplay overlay.

**Failure behavior:** Native safe-area/focus behavior needs measured evidence.

**AC-UI-001:** Given the player changes control-window route; when gameplay remains active; then overlay visibility/focus behavior stays independent and does not cover protected game areas.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-VISUALS.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md), [VISUAL-DESIGN-PROPOSAL.md](../design/VISUAL-DESIGN-PROPOSAL.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [design-tokens.json](../../contracts/design-tokens.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-002 — Session monitor field topology

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** owner_decision / DEC-SESSION-MONITOR; DEC-POSTGAME-BREAKDOWN; DEC-REPLAY-AUTOFETCH

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

The centred Live field shall use the replay viewer's shared full hero-card structure in every phase (none/menu, loading, draft, pregame, live, post-game, unknown and host-lost), except when UI-016 pinned mode applies during a match. It has ten stable hero slots: Radiant's five cards in a left column, the minimap pane in the centre, and Dire's five cards in a right column. Each team column shall have exactly the minimap pane's height, split into five equal card rows ordered by slot ID. The map-pane header is a compact replay-style scoreboard: show the separately admitted Radiant score at the left, the signed GSI game clock in the centre (mm:ss, negative before the horn), and the separately admitted Dire score at the right; each unknown value is an em dash, never zero. Below them, show a bar labelled 'Kill momentum (last 3 min)' / 'โมเมนตัมคิล (3 นาทีล่าสุด)' computed only from observed Radiant and Dire score samples kept in memory for the current match; this is kill momentum, not win probability, and the neutral bar stays empty until a full fresh three-minute baseline exists. Its tooltip states that it shows kills, not win probability. The map header has no title or status text. The shell header status line is the single place for GSI, phase, data-age and capture status; keep the status chips and put their details in tooltips, with no long status sentence. Remove the strip above the map. The map centre shows one short state line only when no map image is available. There is no separate own-hero card: the own hero occupies its team slot only when its team and slot are known, with a subtle highlight and a 'You' chip, and is filled from GSI; the other nine use the same replay structure and remain unknown until an admitted source exists. Values without an admitted source render as em dashes or dashed slots, never as zero. The own hero's stat rows shall show labelled derived values wherever the module returns a value, using its GSI hero ID, level and observed item IDs plus the installed-game stats pack. Each derived value is marked and its tooltip lists its sources and 'ไม่รวมผลชั่วคราว / without temporary effects'; null results remain em dashes and allies and enemies remain unknown. If the game build is unavailable, show it as unknown with the stats-pack build and label the newest supported ruleset as a fallback. No path traces or unobserved enemy values are shown. Before a match, the centre shows a local-history dashboard with matches today and yesterday, W/L and win rate with the Overview all-modes label where applicable, the last match's hero, result, K/D/A and duration, and the best hero this week when known; empty history says 'ยังไม่มีแมตช์ที่บันทึก'. Keep the Start Dota 2 action when Dota is not running. When the match ends or phase becomes post_game, the centre changes to a post-game summary with an approximately 600 ms flip or door transition; reduced motion swaps instantly. The summary remains until the next match starts or the user closes it. The summary has a result banner and a hero header above Summary, Timeline and Replay tabs and does not scroll at the sizes above. Stage 1 shows the result (win, loss or em dash), duration, game mode and patch when admitted, final score and the match ID in the banner; the own hero, level, K/D/A, LH/DN, GPM, XPM and net worth in the hero header, each compared with the hero benchmark only while the user's OpenDota benchmark opt-in is on (otherwise no comparison is shown); the final inventory, backpack and neutral item from observed item IDs; derived end stats when available; and in Timeline this match's full alert and announcer recap with the count of skipped events. Stage 1 uses only fresh or host-recorded GSI values and correctly linked local history. When the match ends, the ten hero cards keep their last observed state with a 'ค่าสุดท้าย / Final' mark until the next match starts or the summary is closed: the own card keeps its host-recorded end values (hero, level, K/D/A, LH/DN, GPM/XPM, net worth and items), and the other nine keep their admitted draft-roster heroes with unknown values. Stage 2 appears only when the replay library has a ready bundle linked by the install-keyed match hash; it fills all ten cards from the digest and embeds the UI-023 summary blocks (ten-hero scoreboard with items, advantage chart with kill markers, buildings, lanes against expectation and key moments) under UI-023's source and unknown rules, and a button and each moment open Analytics on that match. Without a ready replay bundle, the Replay tab says how to use the replay-folder watch or add a .dem file. The match ID is shown with a copy action. Its source is the linked replay bundle, otherwise the GSI match ID held in memory for the current session only; it is never written to logs, GSI or session history or settings; apart from display, it is used only for the opt-in replay fetch of MEM-008, so after a restart it appears only from a linked replay bundle. Gold sources and hero damage are not shown from GSI. At widths 1280–1920 the shell header controls never overlap: status chips may wrap to another line, the second-monitor side selector is one compact segmented control, and Start Dota 2 appears only when Dota is not running. In every phase, Live has no event feed, sound/pack panel, GSI hint box or other side panels; the global shell header and footer remain. During draft, the existing UI-010/UI-012 draft strip and statistics column occupy the bounded centre region at the same height as the team columns, without scrolling. The map pane has no title or status row; the frame setup ('ตั้งค่ากรอบ') and detail ('ละเอียด') controls are under the minimap or capture section in Settings and are absent from the Live map pane; the scoreboard and toolbar stay within the map pane and cannot change card geometry. No Live region scrolls. All ten cards and the minimap remain inside the box without clipping at supported sizes including 1280×720; portrait, icon and card font sizes may scale while card text remains at least 12 px. Below the smallest size that can show all ten cards legibly, the existing window-too-small state appears. A host-owned last-finished-match record shall persist the match epoch, install-keyed match hash, observed end values and ended time across phase returning to none and application restart; the epoch remains available in none/menu. The post-game panel stays associated with that record, opens or remains reachable after a hidden-window match end, and can be reopened from Live until a new draft starts. The panel has three steps: show the observed recap, explain that the player downloads the replay in Dota and show live watcher/parse states with an explicit unknown state when the watcher status interface is unavailable, then offer analysis only for a validated matching replay bundle. The player may manually choose a .dem file. No result is inferred from kills: unknown result is "— ไม่ทราบ"; the score is formatted "Radiant 33 : 23 Dire" with admitted values, and recap events are localized. The post-game replay box describes only the just-finished match and never falls back to the newest replay. When that match has no replay and fetch is off, show the Dota 2 download hint and the Settings → Replay & AI link. Show the opponents summary only when this match has a parsed, hash-linked replay.

**Failure behavior:** If the measured box cannot show every full replay-structure card legibly with text at least 12 px, show the existing window-too-small state; never scroll or clip a card or map. Unknown is an em dash or dashed slot, never zero. Derived values require a non-null module result and the admitted own GSI hero ID, level and observed item IDs; allies and enemies stay unknown. Missing, stale, invalid or unlinked scoreboard, history, recap or replay data stays unknown; the kill-momentum bar is never a win-probability estimate. Do not show fabricated game-clock values, path traces or unobserved enemy values. Do not repeat GSI, phase, data-age or capture status outside the shell header; if header controls do not fit, wrap them without overlap. Empty history uses the explicit Thai empty state; unavailable history remains explicitly unavailable. Without an admitted explicit match result, render "— ไม่ทราบ"; never infer win/loss from kills or score. Missing watcher status and scan time remain unknown. A missing or mismatched host record, stale snapshot, unlinked replay, or unvalidated bundle stays unavailable/unknown; never attach the summary to a later match or open replay analysis before bundle validation. A missing match ID shows an em dash and is never derived from the match hash. Without the benchmark opt-in no comparison is shown, never a zero or placeholder comparison.

**AC-UI-002:** Given the Live page in none/menu, loading, draft, pregame, live, post-game, unknown or host-lost phase at 1920×1080, 1920×1000, 1600×900, 1366×768 or the 1280×720 minimum, with only values admitted from GSI and same-session draft memory, plus shell-header measurements at 1280, 1440 and 1920 px widths, with current-match score samples, local history and replay-library state; and a newest ready replay for a different match, no replay for the just-finished match, and replay fetching off.; when the player resizes Live, the phase changes, the host stops answering, or admitted session and roster values change; then unless UI-016 pinned mode is active during a match, Radiant's five cards, the centre minimap pane and Dire's five cards stay inside the Live box; both card columns equal the map pane height and split evenly in slot order; all ten full replay-structure cards are visible with text at least 12 px; no page or region scrolls and no card or map is clipped; the own hero is highlighted only when its team and slot are known and is filled from GSI, and its stat rows show labelled derived values wherever the module returns them with source tooltips and the caveat 'ไม่รวมผลชั่วคราว / without temporary effects'; null results and unknown values stay em dashes or dashed slots, allies and enemies remain unknown, the scoreboard shows only separately admitted Radiant and Dire scores and the signed GSI clock, while the shell header is the only location for GSI, phase, data-age and capture status; status chips provide detail in tooltips and no long status sentence or strip is repeated above the map, unknown game build and stats-pack build are labelled with the newest supported ruleset marked as fallback, scores and net worth never imply a win probability, the Live page has no event feed, sound/pack panel, GSI hint box or other side panels in any phase, and a too-small state appears below the legible minimum; the map header has no title or status text and its centre shows one short state line only when no map image is available; frame setup and detail controls appear under the minimap or capture section in Settings; the pre-game dashboard shows local matches today and yesterday, W/L and win rate with the Overview all-modes label, the last match and the best hero this week when known, or 'ยังไม่มีแมตช์ที่บันทึก' for empty history; Start Dota 2 appears only while Dota is not running; the post-game summary follows the approximately 600 ms transition or swaps instantly for reduced motion, remains until a new match starts or it is closed, shows only linked GSI/local-history/replay values, and opens Insights → Replay only for a ready bundle linked by the install-keyed hash; after match end the ten cards keep their last observed state with the Final mark (own card from host-recorded end values, others unknown until Stage 2 fills them from the digest); the match ID appears with a copy action from the linked bundle or the current session's in-memory GSI value, and no log, GSI or session history or settings file contains it; shell-header controls do not overlap at 1280, 1440 or 1920 px;  For a finished match, the host persists its epoch, install-keyed hash, observed end values and ended time through phase none and app restart; reopening Live displays that same match until a new draft. A hidden window can surface or reach the panel after match end and its Live entry reopens it. The panel presents all three steps, tells the player to download in Dota, shows watcher and parse state (unknown when no status source exists), supports manual .dem selection, and exposes analysis only for a ready validated bundle linked to the same match. Missing result says "— ไม่ทราบ" and is never derived from kills; score uses "Radiant 33 : 23 Dire" formatting and recap event labels are localized. [DEC-UX-REVIEW-20261125]. The replay box describes only the just-finished match and never labels another match as latest. With no replay and fetch off it shows the Dota 2 download hint and Settings → Replay & AI link. The opponents summary appears only for a parsed, hash-linked replay of this match..

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md), [VISUAL-DESIGN-PROPOSAL.md](../design/VISUAL-DESIGN-PROPOSAL.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [design-tokens.json](../../contracts/design-tokens.json)

**Diagram views:** [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-003 — Neutral production start

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

Production shall begin without fixture roster, fake inventory or preselected hero.

**Failure behavior:** Fixture play/reset/seed controls cannot ship as production authority.

**AC-UI-003:** Given no admitted game state is available; when the live view opens; then waiting/unknown state is visible and inspector remains neutral.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md), [VISUAL-DESIGN-PROPOSAL.md](../design/VISUAL-DESIGN-PROPOSAL.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [design-tokens.json](../../contracts/design-tokens.json)

**Diagram views:** [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-004 — Compact unknown cards

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

Enemy and unknown ally cards shall omit unknown-only skill/item rows.

**Failure behavior:** Do not create four question-mark skill tiles or infer values from account/premium.

**AC-UI-004:** Given no admitted skills/items exist for a visible card; when the card renders; then it shows admitted portrait/visibility/age with unknown age as an em dash.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md), [VISUAL-DESIGN-PROPOSAL.md](../design/VISUAL-DESIGN-PROPOSAL.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [design-tokens.json](../../contracts/design-tokens.json)

**Diagram views:** [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-005 — Selected inspector and ownership

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-ENTITLEMENT

Detailed inspector shall open only after explicit selection and show only admitted data.

**Failure behavior:** Cooldown zero is not proof of castability; distinguish passive/unlearned/unknown/stale.

**AC-UI-005:** Given an ability comes from an unverified selected unit; when the user opens the corresponding hero view; then unowned ability data is not exposed; selection is fenced by session/slot/identity.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Diagram views:** [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-006 — Keyboard and focus

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

Keyboard interaction shall use visible focus, meaningful names and ordered Escape behavior.

**Failure behavior:** Do not steal focus from the game or announce every timer tick.

**AC-UI-006:** Given inline inspector details are expanded; when Escape is pressed twice; then first close inline detail then inspector, restoring control focus safely.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md), [VISUAL-DESIGN-PROPOSAL.md](../design/VISUAL-DESIGN-PROPOSAL.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [design-tokens.json](../../contracts/design-tokens.json)

**Diagram views:** [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-007 — Native estimated timers

**Scope:** deferred · **Priority:** later · **Target milestone:** M7 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-012 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-INGEST, CMP-ENTITLEMENT

Manual enemy timers shall use a reviewed native game-clock store and label expiry estimated-ready.

**Failure behavior:** Reviewed enemy ULT/BKB/TP timer scope, at most 15 keys; arbitrary fixture timers remain tests.

**AC-UI-007:** Given game clock pauses or becomes stale; when wall time passes; then timers freeze and cannot become observed-ready from browser time.

**Verification:** synthetic, native, live; procedures TP-05, TP-12, UAT-12; status NOT_RUN.

**Open dependencies:** DEC-PREMIUM, DEC-INTERFACES.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-008 — Peripheral threat salience

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** owner_decision / DEC-SESSION-MONITOR

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

The session monitor shall make admitted threats noticeable in peripheral vision: an admitted critical warning switches the map frame and a labelled banner to the critical state for the warning lifetime, newly admitted enemy sightings are marked once, and stale or missing capture dims the map with a labelled stale state.

**Failure behavior:** Routine updates never flash or pulse; no motion is derived from predicted paths, decay or unobserved data; reduced motion replaces animation with static emphasis.

**AC-UI-008:** Given the session monitor shows a live field with valid capture; when a critical warning is admitted, then revised, and capture later becomes stale; then the map frame and banner enter the critical state within the same admission, clear on revision or expiry, and the stale state dims and labels the map instead of keeping live markers.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-VISUALS, DEC-RISK-DETAIL.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md), [VISUAL-DESIGN-PROPOSAL.md](../design/VISUAL-DESIGN-PROPOSAL.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json), [design-tokens.json](../../contracts/design-tokens.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-009 — Edge quick panel

**Scope:** extension · **Priority:** extension · **Target milestone:** M3 · **Basis:** owner_decision / DEC-EDGE-ORB

**Product needs:** PN-013 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-ALERT

The optional edge orb shall reveal only after the pointer rests in its hot zone for the reveal dwell (default 1 s, adjustable), showing a continuous 80% to 100% scale cue during the dwell, and shall accept clicks only once revealed. Only the revealed circle and open panel may accept mouse clicks. The outermost 8 px shall remain click-through in every state. The orb shall never take keyboard focus, cover PROTECTED_REGIONS or appear outside a match, without Dota foreground, or in exclusive fullscreen. Its three-channel mixer and Focus mode are specified by UI-011. Each orb or panel click shall be checked for foreground activation; on activation the panel closes and the orb fails closed for the rest of the match. Keep honest status, a Settings off switch and a UI-006 keyboard-reachable twin for each control.

**Failure behavior:** A qualified critical alert closes the panel and locks the orb in the same admission without delaying the critical cue. Unknown status is never shown as ready. Log activation, close the panel and restore click-through immediately. Hide the orb if click-through cannot be guaranteed.

**AC-UI-009:** Given a supported Windows borderless live or bot match with Dota foreground and the orb enabled; when across normal live or bot matches the tester uses the orb or panel and exercises hover, protected regions, focus loss and Settings, and admits a qualified critical alert while the panel is open; then at least 30 orb or panel interactions are logged with zero activation_detected; only guarded hit regions receive orb input and the outer 8 px pass clicks through. No orb or panel pixel intersects PROTECTED_REGIONS. The panel closes and the orb locks in the alert admission while the cue meets its acoustic deadline. Unknown stays unknown, the orb hides outside a match or in exclusive fullscreen, and an induced activation closes the panel, restores click-through and hides the orb until the match ends. Settings can disable the orb and each panel control has a keyboard-reachable deck twin.

**Verification:** synthetic, native, acoustic; procedures TP-05, TP-13, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES, DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [NFR.md](../quality/NFR.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [runtime-status.schema.json](../../contracts/runtime-status.schema.json), [user-settings.schema.json](../../contracts/user-settings.schema.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Implement the built-in fail-closed foreground activation guard. Choose window internals within the specified input and critical alert boundaries.

#### UI-011 — Orb sound mixer

**Scope:** extension · **Priority:** extension · **Target milestone:** M3 · **Basis:** owner_decision / DEC-EDGE-ORB

**Product needs:** PN-013 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-ALERT

The orb panel shall present vertical channels in a fixed order: Game (Dota 2) first, then up to three slots the user configures in Settings, so the panel never shows more than four channels. A slot is the Kohbor announcer, the Music channel (a user-selected application, or the first running default), or another application that has a Windows audio session, chosen by executable name (for example Discord). Slot order is editable; the defaults are Kohbor and Music. Each channel shall provide a 0 to 100% level and mute, applied within 100 ms. An application with several sessions shall show their common value, or a mixed state with the range when they differ, and a change moves all of its sessions together. A configured application with no live session shall show the missing state. Focus mode shall lower every non-game application channel (Music and added applications) to an adjustable default of 20%, never raising a lower level, and restore each prior value on a second activation; the Kohbor announcer is never lowered, so Kohbor alerts stay audible (owner decision 2026-11-22). An optional user-assigned Focus hotkey is off by default. Show honest missing/unreadable session states and explain failed sets while restoring the last read level. Every control, including the slot editor, shall have a keyboard-reachable UI-006 deck twin. A fader wheel step is 5% and is active only while the panel is open. The panel shall fit one to four channels at 720p and 1080p without overflow, never cover a protected region and keep the 8 px click-through edge.

**Failure behavior:** Never show a missing or unreadable session as controllable, and never show one invented value for an application whose sessions differ. Refuse more than three slots, a duplicate application, Dota as a slot, or a second Kohbor or Music slot; an invalid stored setting falls back to the defaults. On set failure restore the last read value and state the reason. Preserve Dota focus and critical-alert priority.

**AC-UI-011:** Given a supported match, present, mixed, missing and unreadable sessions, and mixer settings with zero to three slots (Kohbor, Music and an added multi-session application), Focus level and optional hotkey; when the player configures and reorders slots, adjusts, mutes, focuses, restores, uses a wheel and encounters missing or failed sessions; then the panel shows Game first and at most four channels in the configured order and fits at 720p and 1080p without covering a protected region, each present channel exposes its actual 0 to 100% value and mute, a mixed application shows its range and moves all sessions together, a configured application without a session shows the missing state, Focus lowers every non-game application channel to the selected level, never lowers the Kohbor announcer, and restores each prior value, the hotkey is inactive unless enabled, wheel steps by 5% only with panel open, failures are honest, invalid slot settings are refused, and every control has a deck twin.

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES, DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [NFR.md](../quality/NFR.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [runtime-status.schema.json](../../contracts/runtime-status.schema.json), [user-settings.schema.json](../../contracts/user-settings.schema.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Implement the built-in fail-closed foreground activation guard. Choose window internals within the specified input and critical alert boundaries.

#### UI-010 — Display-only hero pick card

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** owner_decision / DEC-PICK-STATS

**Product needs:** PN-014 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-INGEST, CMP-VISION

One right-edge column at x=1728–1907 at 1920×1080, scaled with height, shall grow down from y=132 to fit its sections (owner choice of Option A, 2026-11-22), end no lower than y=860, use text no smaller than 12 px at 1080p, and shall show a read-only suggestions list before the own pick and the pick card after it. V1 list entries show hero, historical win rate in the selected bracket and game count; under 1,000 games show a low-data state. Show five estimated position groups labelled “ค่าประมาณ” (estimate). The list may include banned or picked heroes and shall say so. Never show win-chance %. Position and bracket are set in the deck; the in-game bar is read-only and has a separate on/off switch. The pick card remains phase-gated; v2 strike-out depends on Draft Bar 5b only. Within one draft, a Draft Bar 5b slot keeps its last identified hero until another hero is identified for that slot or the draft phase ends; it is then cleared as before (owner decision F3, 2026-11-23). The list is meta first: most picks in the selected bracket, then win rate. Counter-pick suggestions (DEC-PICK-STATS revision 3) come only from a reviewed static counter table shipped with the app, with a reason, patch and date per row; they show in the deck and, at most three rows on top under “แก้ทาง {enemy}”, in the in-game column, only while at least one enemy hero is identified by Draft Bar 5b and both the slot side and the own side are known. Counters carry no statistic and no win-chance %, and stay behind a build switch until the owner has reviewed the table and an in-game mock. A statistical matchup edge awaits an adequate data source and remains an open dependency.

**Failure behavior:** Hide without an admitted own pick, a supported placement or an active draft phase. Suppress v2 comparisons when 5b is unavailable and keep eligible v1 content. Remove previous hero statistics immediately on a repick or swap.

**AC-UI-010:** Given an admitted draft, deck-selected position and bracket, and supported 1920×1080 layout; when the player picks or changes hero, advances through selection, strategy and showcase, and uses builds with and without admitted 5b output; then the single column occupies x=1728–1907 from y=132 to no lower than y=860, scaled with height, with text no smaller than 12 px, and shows the read-only list before own pick and existing pick card after it; the list shows historical bracket win rate and game count, labels under-1,000 samples low-data and five position groups as estimates, discloses that listed heroes may be banned or already picked, never shows win-chance %, accepts no in-game input, and has its own on/off switch; v2 only strikes picked heroes when 5b is available; counter suggestions appear only from the shipped reviewed table, only with at least one identified enemy and known slot and own sides, at most three in game, never as a statistic, and none while either side is unknown; a statistical matchup edge remains unavailable pending a qualified source.

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES, DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DRAFT-BAR-SPEC.md](../design/DRAFT-BAR-SPEC.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces within the stated behavior, permissions and evidence thresholds.

#### UI-015 — Overlay layout editor

**Scope:** extension · **Priority:** extension · **Target milestone:** M3 · **Basis:** owner_decision / DEC-EVENT-RUNE-SPAWN

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

Settings shall offer a layout edit mode that shows a 16:9 preview of the screen with each registered overlay widget as a box the user can drag and resize (scale presets or a slider). Boxes snap and clamp to the safe area: they never cover the protected regions (minimap, bottom HUD, top bar, announcement) nor the 8 px screen edges. The one exception (owner decision 2026-11-22, DEC-EVENT-RUNE-SPAWN revision 5) lets the map-objective card enter the top of the announcement region by at most 24 px at 1080, scaled with screen height, so that it can sit under the danger meter; every other widget stays fully blocked. A saved copy of an earlier default is read as the current default; any other saved layout still wins. Each widget has reset to default. Layouts persist per widget id in a local prefs file with a schema_version; the editor is reusable for other widgets, and only the map-objective card (ALERT-008) is registered now. The edit mode is a Settings surface; the in-game overlay stays click-through.

**Failure behavior:** An unreadable or invalid layout file or entry falls back to that widget's default and is reported; an out-of-bounds value is clamped into the safe area, never applied raw. Changing a layout never steals Dota focus or makes an overlay window interactive.

**AC-UI-015:** Given the layout editor with the objective card at its default, 1920x1080 and 2560x1440 previews, and a corrupt layout file; when the user drags and resizes the card into protected regions and edges, resets it, and restarts; then the box always stays inside the safe area and off the protected regions except that the objective card may enter at most the top 24 px (at 1080, scaled with height) of the announcement region and is clamped there when dragged deeper, a widget without that allowance stays fully blocked, the default card sits centred under the danger meter with a 4 px gap, reset restores the default, the saved layout persists with its schema_version and is applied to the card in the overlay, a corrupt file falls back to the default, and each change writes one layout_changed info line.

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Implementation freedom:** Choose the editor's internals, preset values and the prefs file layout within the safe-area, persistence and click-through boundaries. Reference products may inform the interaction; copy no assets.

#### UI-016 — Second-monitor Live layout

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M3 · **Basis:** owner_decision / DEC-LIVE-SECOND-MONITOR

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

The Live page shall offer a per-device second-monitor setting: off by default, monitor 2 left of the main monitor, or monitor 2 right of it. The pinned layout applies only while a match runs (draft, pregame or live with the host answering); before or after a match, during an unknown phase, or after host loss, the centred layout shows and the stored side remains available on the Live page. In pinned mode, the captured minimap panel is pinned to the inner edge that touches the main monitor. Five enemy cards sit in one row directly above the minimap and our five cards sit in one row directly below it; each row has the same width as the map panel, with all cards ordered by slot ID. The map panel and two rows fit the measured Live box without clipping or scrolling. All ten cards use the replay viewer's shared full structure; there is no separate own-hero card, and the own hero occupies its team slot with a subtle highlight and 'You' chip. No hero identities are added by UI-016; the panel may show only the existing observed total 'เห็นศัตรู n/5'. The map panel's outer area is a priority grid shown only in live phase: objective timers, own-hero HP log, agent orb, own farm graph, and team kill timeline, dropping lowest-priority panels first when they cannot fit. Objective timers for the next power rune, bounty, wisdom, lotus, Tormentor, day/night and Roshan respawn reuse the existing ALERT-008 timing table without changing alert behaviour; every table-derived timer shows the patch written on its row (for example 'กติกา 7.42') and does not claim that the running game uses that patch. Timers advance only from fresh admitted GSI map.clock_time. Day/night state comes from fresh admitted map.daytime or nightstalker_night; time to the next switch is '— ไม่ทราบ' unless a cycle rule is recorded. Lotus is '— ไม่ทราบ' while no schedule rule exists. Roshan respawn is '— ไม่ทราบ' unless a fresh admitted map.roshan_respawn_seconds value or a verified respawn-window rule supplies it; the 'Roshan ตาย' control appears only with a verified respawn-window rule. The HP log lists newest-first own-hero GSI HP drops, merges drops no more than 0.5 s apart, includes game time, HP lost and HP after, labels the source as unknown, retains a 60 s HP sparkline, omits heal rows, and remains in memory for the current match only. The orb uses one round waveform SVG and the exact label 'เอเจนต์ (เสียง) — เร็ว ๆ นี้'; it has a slow idle breath and animates on Kohbor's own output playback, follows a real mixer level only if available, otherwise shows playback activity without a fake level, honors reduced motion and hidden-window state, and has no microphone or input. Speech-to-speech, STT and TTS wait for local voice-assistant RAM measurement (Full MVP step 3, VOICE-001..007). The farm graph uses own GSI GPM, XPM and last-hit samples in memory and marks LH@10 only after the game clock passes 10:00; the kill timeline uses the same score samples as the scoreboard and does not replace the existing three-minute momentum bar. Panels never scroll or overlap, keep text at least 12 px, and disappear from lowest to highest priority as the outer box shrinks; all five fit at 1920×1080. The map overlay draws only on Kohbor's captured map image in pinned mode while the map is shown in pregame or live, and is absent while the draft board is shown: power-rune spots, bounty runes, wisdom runes and lotus pools show game-clock seconds during the last 60 s and briefly show 'พร้อม'; timing reuses ALERT-008 and shows its recorded rule patch. Known coordinates use local replay rune-event world positions transformed with the replay viewer's manifest calibration and carry their replay patch/build provenance; unknown positions are omitted and no installed-game patch is inferred. Roshan's active pit and side-switch pulse/label appear only when a verified rule or admitted GSI field identifies the pit and switch; roshan_state or roshan_respawn_seconds alone does not identify a pit, so its marker stays '— ไม่ทราบ' without such a source. The navigation rail moves to the outer edge and the friends rail collapses only while pinned mode is active during a match. Card portrait, icon and font sizes may scale, but card text remains at least 12 px; below the smallest size that can show all ten cards legibly, the existing window-too-small state appears. The map geometry is determined from the Live box, never by card or panel content. The map uses the existing live capture path, scales smoothly, never identifies heroes or draws motion arrows, and remains visibly stale when its source is stale. The pinned map uses the UI-002 scoreboard header and kill-momentum rules; the global status chips may wrap, and the side selector remains one compact segmented control without header overlap.

**Failure behavior:** Without a stored side, outside a match, during an unknown phase or after host loss, show the centred layout and keep the side selector available. If the measured box cannot show the two five-card rows and map panel legibly with card text at least 12 px, show the existing window-too-small state; never scroll, clip cards or move/resize the map because of card or panel content. If the outer area is too small for every panel, omit the lowest-priority panels first. Missing, stale, malformed or patch-unverified objective rules show '— ไม่ทราบ'; unknown rune, lotus or Roshan positions are not drawn, and an unverified pit never gets a marker. The Roshan death button is absent without a verified respawn window. Unknown values remain dashed slots or em dashes, never zero or guessed values. A stale or unavailable capture keeps its stale or waiting presentation; an old frame is never shown as live. Damage attribution remains unknown; missing hero timers are not inferred; no listening state, microphone, input field or fake audio level is shown.

**AC-UI-016:** Given left and right second-monitor settings at 1920×1080, 1600×900 and 1440×900, plus an undersized window, before/during/after a match and with a stale capture; own GSI samples with HP drops, healing and missing fields; a verified and an unverified objective schedule; and playback with and without an output-level meter; when the player selects a side, resizes the window or enters/leaves draft, pregame and live phases while the GSI game clock and team scores advance; then while the setting applies the enemy five-card row is directly above the map panel and our five-card row directly below it, all three regions share the map panel width and remain inside the box with no overlap, scroll or clipping, every card remains complete at 12 px text or larger, and map geometry does not depend on card or panel content; on both edges all five outer panels fit at 1920×1080, and smaller 1600×900 and 1440×900 boxes omit panels from lowest priority upward while preserving text at 12 px or larger; the damage rows are newest-first and merge drops within 0.5 s, do not attribute damage, and omit muted heal rows while the HP sparkline includes observed gains; the orb has no input controls and never shows listening or a fabricated level, is static under reduced motion or when hidden, and tracks only admitted output playback; the farm graph uses own GSI samples and marks LH@10 only after 10:00, while its kill timeline uses the scoreboard's same score samples and leaves the three-minute momentum bar intact; objective and Roshan values are '— ไม่ทราบ' unless current-patch rules or admitted GSI fields verify them, and unknown map positions are omitted; below the legible minimum the window-too-small state appears; outside a match, at an unknown phase or after host loss the centred layout returns and the stored side remains reachable on the Live page; the shell header/footer stay visible and only the pinned-mode navigation rails change.

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [design-tokens.json](../../contracts/design-tokens.json)

**Implementation freedom:** Choose the measurement, storage and CSS mechanism. Keep the map geometry independent of content (UI-002) and the setting local to the device.

#### UI-017 — Second-monitor content from admitted sources

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M3 · **Basis:** owner_decision / DEC-LIVE-SECOND-MONITOR

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

In either in-match Live layout, all ten hero slots shall use the replay viewer's shared full card structure, ordered by slot ID. The own hero occupies one of the five slots on its team, is subtly highlighted with a 'You' chip, and takes admitted values from GSI. Other ally identities come only from same-session Draft Bar memory; HP, mana and skills remain em dashes or dashed slots until a consented peer source is admitted (PEER-003, PEER-010), and an ultimate-dot state appears only from an admitted topbar.ally_ult_state source. Enemy identity comes only from same-session draft memory; HP, mana, skills and stats remain unknown unless an admitted source supplies them. Every value retains its source meaning; ally HP is never read from the in-game top bar and abnormal states (stuns, targeting) are not shown.

**Failure behavior:** Unknown is an em dash, never zero, never a guessed side and never a guessed cooldown. If the own team or own hero cannot be matched, the ally cards stay unknown and no ultimate-dot state is bound to a guessed ally. An unrecognised dot value is unknown.

**AC-UI-017:** Given a session with and without admitted draft identities, a known and unknown own team, risk counts present and absent, no objective source, own GSI abilities, ally ultimate-dot inputs present and absent, and peer data present and absent; when the second-monitor Live page renders; then both layouts show exactly five own-team slots and five enemy slots in slot order using the shared replay-card structure; the own hero appears once in its team row and its admitted values come from GSI; ally vitals/cooldowns appear only with admitted peer sharing; enemy values remain unknown without an admitted source; unknowns render as em dashes or dashed slots, no enemy value is inferred, and abnormal states are absent.

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Implementation freedom:** Choose presentation within the Command Deck tokens. Add a new source only through its own requirement and decision.

#### UI-018 — Second-monitor rune countdowns on the minimap

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M3 · **Basis:** owner_decision / DEC-LIVE-SECOND-MONITOR

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

Only while pinned mode shows the minimap in pregame or live (not while the draft board is shown), the minimap overlay shall draw original countdown labels at known normalised positions for the next power-rune spots, bounty runes, wisdom runes and lotus pools. A label appears only during the last 60 game-clock seconds before an occurrence and says 'พร้อม' briefly at and just after the spawn; it never uses wall time to advance the game clock. Timing reuses the ALERT-008 schedule rules or admitted GSI fields; each table-derived countdown shows the patch recorded on that rule and does not claim a mapping to the installed game patch. It uses the admitted GSI game clock and duplicates no spawn table in the renderer. Rune coordinates come only from local replay rune events and the replay viewer's recorded map calibration; each coordinate carries source patch and build provenance, and a missing build or unknown coordinate is not inferred. Roshan's active pit and side-switch pulse/label appear only when a verified rule or admitted GSI field identifies the pit and switch; roshan_state or roshan_respawn_seconds alone does not identify a pit, so the map shows '— ไม่ทราบ' without a guessed marker. Roshan respawn is '— ไม่ทราบ' unless a fresh admitted roshan_respawn_seconds or verified respawn-window rule supplies it; the manual 'Roshan ตาย' control appears only when the respawn window is verified. Overlay labels never take pointer input and are drawn only over Kohbor's captured map image. Rune presentation follows D.4: the minimap uses a distinct rune-type mark and time-only chip; the rune names live in a legend below the map. Show the full text chip only inside that rune alert lead time (or in the final ten seconds when no user lead applies). Use distinct shapes for rune kinds and a dashed ring where the coordinate/source patch differs from the schedule patch; this provenance does not claim the installed game patch.

**Failure behavior:** A missing, malformed, stale, patch-unverified or unavailable schedule shows '— ไม่ทราบ' in the objective panel and no countdown labels on unknown spots. Positions absent from admitted local map data remain omitted; approximate pixels are never promoted to measurements. An unverified Roshan pit remains '— ไม่ทราบ' with no pit marker or side-change pulse, and no Roshan death control appears without a verified respawn rule. Overlay labels never draw without Kohbor's own map image, never appear outside pinned live mode, and never accept pointer input. Unknown spots, schedule, source patch or scan state remain omitted or explicitly unknown; do not invent map coordinates or imply installed-patch compatibility. The legend must identify only admitted rune types and patch-difference rings must be dashed and described as provenance.

**AC-UI-018:** Given verified and unverified current-patch schedules for power runes, bounty, wisdom, lotus and Tormentor; known and unknown normalised spot positions; a stale or malformed schedule; admitted and absent GSI roshan fields; and an own captured map frame that is fresh, stale or absent; when the pinned minimap renders and the GSI game clock advances; then known rune and lotus positions show seconds only in the final 60 game-clock seconds and briefly show 'พร้อม'; no wall-clock extrapolation or guessed pixels appear; unknown schedules and positions remain unknown or omitted; Roshan pit and side-change information appears only when verified, otherwise the overlay says '— ไม่ทราบ' without a pit marker; Roshan respawn and its manual death control remain unavailable unless both rule and signal gates are met; the overlay is pointer-inert and appears only over the captured map image in pinned live mode;  On the map, each known rune shows only its type mark and time until the configured alert lead threshold; the full chip appears only within that lead (or final ten seconds without a configured lead). Rune kinds have distinct shapes, names appear in a legend below the map, and a source/schedule patch difference uses a dashed ring. Unknown positions remain absent. [DEC-UX-REVIEW-20261125]..

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Implementation freedom:** Choose marker shapes, colours and the label style within the Command Deck tokens. Do not duplicate the ALERT-008 timing table.

#### UI-019 — Second-monitor broadcast-style draft board

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M3 · **Basis:** owner_decision / DEC-LIVE-SECOND-MONITOR

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

In second-monitor mode, while the draft runs, the centre map panel shall show a broadcast-style draft board instead of the minimap, in the same bounded map region: one row per team (Radiant, then Dire), five slots each, with our side marked when the own team is known. Each slot shows the hero portrait and name once identified by Draft Bar 5b in the same session, and the player name from Draft 5c only while the visible show-player-names setting is on; names stay in memory for the current match only and are never written to disk, logs, events or the network (DEC-DRAFT-BAR). Our side shows the picked hero's historical win rate in its estimated position group, labelled as an estimate; the enemy side shows the win rate and an estimated lane from the position groups, labelled as a guess. Statistics come only from the hero statistics projection under DEC-PICK-STATS and its consent. No win-chance percentage is shown. When the draft ends the square returns to the minimap.

**Failure behavior:** An unidentified hero, a missing name, a name while the setting is off, or a missing or out-of-range statistic shows an em dash. Statistics are shown only for identified heroes. Low-data statistics carry a low-data label.

**AC-UI-019:** Given a draft with some heroes identified, the own team known and unknown, names present with the setting on and off, and statistics present, absent and out of range; when the second-monitor Live page renders during the draft and after it; then the map square shows two team rows of five slots; identities come only from same-session draft memory; names appear only with the setting on; our side shows an estimated win rate and position, the enemy side a win rate and a guessed lane; no win chance appears; unknown values are em dashes; and the minimap returns after the draft.

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Implementation freedom:** Choose the board styling within the Command Deck tokens. Add a statistics or names source only through its own requirement and decision.

#### UI-012 — In-memory deck draft strip

**Scope:** extension · **Priority:** extension · **Target milestone:** M3 · **Basis:** owner_decision / DEC-DRAFT-BAR

**Product needs:** PN-015 · **Primary design:** CMP-READMODEL · **Design components:** CMP-VISION, CMP-INGEST, CMP-READMODEL

During HERO_SELECTION, STRATEGY_TIME and TEAM_SHOWCASE, the Live page in the control window shall show the ten top-bar hero slots with an identified hero name or an unknown state with its reason. This extension is supported only at 1920×1080 and 100% scale. Slot imagery, identification results and reasons shall remain in memory for the current match only; clear them at match end, session change and application exit. Draft Bar data shall never be shown in the overlay, written to logs or disk, or sent over the network.

**Failure behavior:** At unsupported resolution or scale, failed capture, or insufficient identification confidence, show unavailable/unknown with the actual reason; never guess a hero or retain values from another match/session.

**AC-UI-012:** Given an owner-played Dota draft at 1920×1080 and 100% scale with an admitted session; when the Live page is shown during HERO_SELECTION, STRATEGY_TIME or TEAM_SHOWCASE, including unsupported geometry, capture/identification failure and match/session/app clear boundaries; then all ten top-bar slots show the correctly identified hero name or unknown with a reason; unsupported conditions expose their actual reason; the strip remains in-memory and control-window only with no overlay/log/disk/network sink, and data clears at match end, session change and app exit. Accuracy evidence remains NOT_RUN until an owner-played draft is observed.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES, DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DRAFT-BAR-SPEC.md](../design/DRAFT-BAR-SPEC.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Keep draft pixels, hero identities and unknown reasons in the in-memory control-window path; do not add a persistence, network or overlay sink.

#### UI-013 — Show-app hotkey

**Scope:** extension · **Priority:** extension · **Target milestone:** M3 · **Basis:** owner_decision / DEC-SHOW-APP-HOTKEY

**Product needs:** PN-016 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

A global show-app hotkey, default Ctrl+Alt+X, shall toggle the control window. When the control window is hidden, minimized or not focused, a press shows, restores and focuses it; when it is visible and focused, a press hides it (to the tray when the tray icon exists) and returns the foreground to the window that was foreground before the show, if that window still exists. The hotkey shall be registered with the operating system hotkey registration only: no keyboard hook, no synthesized input to any process and no read of another process. It is on by default, has a Settings switch, and is remappable with registration-conflict detection and transactional replacement. Support is promised over borderless fullscreen and windowed Dota; Settings discloses that in exclusive fullscreen the game may minimize when the window comes up.

**Failure behavior:** If registration fails, keep the prior working binding (or none), show the error with its reason, and never silently choose another key. Reject baseline and Windows-reserved keys. Warn when an Alt-only key may take a Dota binding. Do not show the overlay, HUD or orb as a side effect; they keep their own visibility and focus rules.

**AC-UI-013:** Given the hotkey enabled, Dota in borderless fullscreen or windowed mode, and the control window hidden to the tray, minimized or behind Dota; when the player presses the hotkey, presses it again, remaps it to a free key, remaps it to a key another program holds or to a reserved key, and disables it; then the first press brings the control window to the foreground and the second hides it with Dota foreground again; no synthesized input is sent and overlay or orb activation_detected stays zero; a free key replaces the old one, a held or reserved key leaves the prior binding working with a visible error, and disabling releases the key; Settings discloses the exclusive-fullscreen limit.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Implementation freedom:** Choose the registration thread, preference file and capture UI. Keep RegisterHotKey-only input, the toggle and focus-return behaviour, and transactional replacement.

#### UI-020 — Overview player dashboard and news

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M3 · **Basis:** owner_decision / DEC-OVERVIEW

**Product needs:** PN-008 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-MEMORY

The Overview page shall show only the player's own data and Kohbor news, in two columns, and shall always lay out every section, whatever data exists. The main column shows statistics from the local match history (MEM-006) merged with OpenDota own-match history only after the user's explicit opt-in (MEM-006): the overall win rate with its base count and, while counted matches include unknown types, the label "All modes (ranked/unranked not yet separated)" (Thai "รวมทุกโหมด (ยังแยก ranked/unranked ไม่ได้)"); a wins/losses/no-result donut with the match count in its centre; the latest counted results as 20 win/loss slots titled with the true count, empty slots drawn as outlines; a 10-match rolling win-rate trend whose frame, axes and gridlines are always drawn and whose line appears only from 10 counted matches; two best-hero panels like Dota Plus, "best of all time" and "this month" (the local calendar month, whose start the renderer sends to the host), each with three cards showing the bundled hero icon, name, games, wins–losses, win rate and average K/D/A, ranked by win rate among heroes with at least 3 counted games (then more counted games) and filled by most played when fewer qualify; and a recent-matches table with hero, W/L or an em dash, K/D/A, duration, date and source badge. The side column shows the news feed (UPDATE-005): the pinned post first, a type filter (all, announcements, app updates, patches) and an "all news" control that expands the list in place. The page shows no readiness or GSI content, and the shell's GSI and game status chips are hidden on the Overview page only. The OpenDota card replaces the coming-soon card: its Thai and English disclosure names the own 32-bit Steam account ID sent to api.opendota.com, the provider's visibility of IP address and request time, and the request-stop/cache-delete effect of revocation; consent defaults off. The same consent switch appears in Settings. Off, loading, merged, empty, private-profile, not-found, rate-limited, budget-exhausted, offline and unavailable states stay explicit; a private profile shows 'Expose Public Match Data' and Retry, and rate/offline failures show cached data and its fetch time.

**Failure behavior:** With no counted matches the rates read 0.0% over a base of 0 matches with the all-modes label, the donut is an empty grey ring with 0 in its centre, all 20 result slots are empty outlines, the trend frame shows a faint "needs at least 10 matches" note, and the table keeps its header with faint empty rows up to five; loading, unavailable and empty histories add a one-line note instead of collapsing a section. Empty best-hero slots hold a fixed set of three placeholder heroes (Pudge, Juggernaut, Crystal Maiden) with every stat at 0, 0–0, 0.0% and 0.0 K/D/A, shown desaturated with a "Placeholder" chip and the tooltip "No data yet — showing a placeholder slot"; a placeholder never repeats a real hero and never counts toward any number. A real value that is unknown shows an em dash, and a match with no result is never counted or shown as a loss. The news column shows loading, offline with the cached fetch time, unavailable, empty or switched-off states; a news failure never blanks the statistics column. OpenDota is not contacted while consent is off. Private-profile, empty, 404, rate-limited, budget-exhausted, offline and unavailable outcomes remain distinct; rate/offline outcomes preserve cached rows and their fetch time, and revocation prevents late cache writes.

**AC-UI-020:** Given an empty history and a history of about 25 matches with counted, unknown-type and no-result matches, heroes with fewer and more than 3 counted games, matches on both sides of the local month start, and news views that are loading, current, offline with cache, unavailable, empty and switched off and OpenDota host replies for loading, merged, empty, private profile, 404, rate limit and offline cache; when the Overview page renders in Thai and English and the user changes the news filter and expands the news list; then every section is laid out in both cases; with no data the tiles read 0.0% and 0 matches with the all-modes label, the donut is an empty ring with 0, the 20 slots and the trend frame are empty, the table shows five faint empty rows and both best-hero panels show three labelled, desaturated placeholders with zero stats; with data the rates use counted matches with their base count, the donut splits wins, losses and no result, the trend line appears only from 10 counted matches, each panel ranks by win rate from 3 counted games and falls back to most played, the month panel uses only matches from the local month start, and remaining slots are placeholders; the news column shows the pinned post first and the filtered, expandable list or the matching state; the OpenDota card and Settings disclose the account ID, recipient, IP/time exposure and revoke effects before opt-in, default off; the private-profile state shows 'Expose Public Match Data' and retry; empty, 404, rate-limited, budget-limited, offline and unavailable states are explicit, with cached data and fetch time on rate/offline; and each real match row shows Kohbor or OpenDota as its source; and no GSI or readiness chip appears on the Overview while other pages keep them.

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Implementation freedom:** Choose the layout details within the Command Deck tokens and the variant D design. Add a statistics source only through its own requirement and decision.

#### UI-021 — Analytics three-column replay analysis

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M5 · **Basis:** owner_decision / DEC-ANALYTICS-V2; DEC-ANALYTICS-LAYOUT; DEC-LANE-EXPECTATION

**Product needs:** PN-008 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-MEMORY

The Analytics destination shall retain the `insights` route and provide a measured three-region replay workspace: Radiant cards, a centre replay stage and Dire cards alongside a resizable right pane with summary above the assistant. The shell header contains Analytics controls in one row; complete picker fields collapse in the approved order or move into a menu, and labels never truncate. The stage uses a square map with the playback timeline directly below it; the lane strip is removed and kill scores move into team headers. Size classes use measured page width, per-class stored pane preferences, clamped dimensions, four keyboard-accessible resize handles, reversible collapse rails and reset. Target map sides are 374 px at 1366×768, 448 px at 1440×900, 640 px at 1920×1080, 1,068 px at 2560×1440 and 440 px at 1920×1080 at 125 percent scaling. At compact sizes, non-selected cards use compact density and the selected hero retains every setting-visible row; hero cards do not scale below 85 percent before the roster-strip fallback. Analytics stat labels use the available stat icons at the approved larger size with accessible titles. The status tray shows only bundle-provided modifiers, prioritized icons, duration rings and seconds, a “+N” overflow list, and never removes crowd-control entries; for bundles older than schema 3.2 or where fields are unknown, missing permanent buffs, auras and stack counts show “บัฟถาวร ออร่า และจำนวนสแต็ก ยังไม่มีในข้อมูลรีเพลย์”. The renderer never derives Duel-win or tower buffs. One shared 13-checkbox hero-card display preference defaults every field on, removes hidden cells and reflows rows without gaps, and is exposed in Settings and Analytics quick access; Live wiring follows the OV merge. Selecting text offers Ask, Go to time when its timestamp is known, and Note. Ask puts a source/time quote chip in the composer without a tab change; one selected passage creates one note. Keyboard controls include Ctrl+\, Ctrl+Shift+\, Ctrl+J, Ctrl+Alt+0, F6/Shift+F6, I and Ctrl+Shift+A/N. Lane analysis shall preserve the approved raw lane result and add a separate score, expected score and verdict. Expectations use the role baseline and reviewed coach rows only; an unavailable expectation displays "ยังไม่มีค่าคาดหมาย" with the raw result, never a guessed value. Every added reading is labelled "ทดลอง · เกณฑ์ชั่วคราว" until R1, R2 and R3 pass. Show the user's own lane average separately as "ค่าเฉลี่ยของคุณ" and never blend it into the expectation. Lane cards show score, available components, expectation source and verdict; the matchup strip repeats "เกินคาด / ตามคาด / ต่ำกว่าคาด" and optional "มาก" when space permits, and falls back to the lane card when crowded. Turbo boundaries and unsupported or unknown lane shapes remain unknown. Match result sources resolve in order: engine, local GSI history keyed by match hash, own OpenDota history only when enabled, then unknown; conflicting sources produce unknown and a warning. Calibration refuses fewer than 30 lanes and writes a report plus proposed constants v1 and its hash without changing the shipped constants table. Coach review enforces blind, half-blind and revealed stages and exports constants and bundle hashes without personal data. R1 requires calibration on at least 30 lanes; R2 requires at least 15 blind coach reviews, at least 70 percent agreement and no opposite-direction pair; R3 requires no unexplained contradiction between score sign and the coach reading of raw values. The Analytics host stores local derived lane rows for all three lanes at <app data dir>/analytics/lane-history.json, atomically and bounded to the newest 1,500 rows. Rows contain no player names or IDs, match IDs or match hashes, or file paths. Clear-history deletes the lane store and calibration report. Coach Review belongs in Lab or Developer mode. "ค่าเฉลี่ยของคุณ" may appear on screen but is never part of an AI digest/request. Provide per-match deletion of chat, notes and reports even when replay data has been evicted or removed. Lane dedup uses a separate irreversible HMAC(install key, bundle hash) token file, deleted by clear-history; clear the existing lane store once. The Analytics match list replaces the match dropdown and the Replay/JSON report sub-tab. It is bounded to the page and independently scrolls with up to 200 rows, groups rows under newest-first Thai calendar-day headings, offers date chips for all/today/7 days/30 days and a picker for one day, result filters for win/loss/all, and a filtered count. Rows show time, own hero portrait/name, result, duration, replay state, and open/parse/re-parse/remove actions; one “เพิ่มไฟล์ .dem” button sits by the list. Unlinked rows derive own hero/result only from a replay bundle carrying player account IDs and a locally known own account ID; otherwise both remain “— ไม่ทราบ”.

**Failure behavior:** Invalid or evicted bundles show an explicit unavailable or replay-removed state. Missing result, own hero, lineup, duration, score, size, card or chart values remain unknown and render “—”. If saved chat history cannot be read, show an unavailable state rather than inventing or replacing the transcript. Raw HTML and model-controlled SVG attributes are inert; unsafe schemes and invalid document IDs cannot navigate; only approved #t= and #ev= links seek. An invalid or over-limit kohbor-chart block is shown as a code block with a validation reason. A missing reviewed expectation stays null and shows "ยังไม่มีค่าคาดหมาย" with the raw result. Unknown boundaries, unsupported shapes and missing required score components stay null with a reason. Calibration does not write the constants table, and no reading loses its trial label before R1, R2 and R3 pass. Conflicting match result sources show unknown and emit one warning. Corrupt lane history is unavailable and logged; a failed atomic write preserves the previous file. The lane store never exceeds 1,500 rows and contains no person or match identifiers. A failed clear reports the deletion failure. Personal-average values never enter AI request projection, digest, transcript or provider payload. Per-match chat, notes and reports remain independently deletable when replay files or bundles are absent. Clear-history removes the dedup token file and the current lane store according to the one-time migration. Pane values remain clamped by the measured window class, and reset changes only that class. When status data is absent, render the approved information note rather than estimating permanent, aura, Duel-win or tower effects. A hidden card field is removed from layout rather than replaced by an empty placeholder. A selected quote without a known timestamp remains quotable and can be saved as one note, but offers no seek action.

**AC-UI-021:** Given the 1366×768, 1440×900, 1920×1080, 2560×1440 and 1920×1080-at-125-percent window models, known and unknown replay bundle fields, known and unknown replay bundle fields, complete same-match chat.json turns, hostile and safe Markdown, valid and invalid kohbor-chart fences, and the 1440×900 and 1920×1080 viewport models, lane score and expectation fixtures with missing values, empty and reviewed expert tables, calibration sets below and above the 30-lane threshold, blind-review stages, and conflicting result sources, an empty or corrupt local lane store, more than 1,500 lane rows, and clear-history fixtures; and a replay library with up to 200 dated and undated rows, linked local-history rows, bundle identities that match or do not match the locally known account, and unknown result/hero values.; when the user resizes and collapses panes, resets a size class, changes hero-card checkboxes, selects text, asks about a quote, seeks when a timestamp exists, saves a note and opens status overflow; the user opens Analytics, selects a replay and hero, reads the transcript, receives streamed Markdown, follows time/evidence/document links, filters rows, collapses a panel and opens detailed Replay tools, compares the raw result with the trial reading, runs calibration and completes a coach review, captures lane rows, restarts, and clears Analytics history; then both viewports use measured page-width pane geometry without overlap or page-level scrolling; the map sides match the five target models; the top bar keeps labels whole or moves controls into More; pane dimensions clamp and persist by size class with reset; compact non-selected cards contract while the selected hero retains every setting-visible row; cards stay at or above 85 percent before the roster fallback; hidden card fields reflow without gaps from a default-all-on shared setting; status overflow retains every crowd-control entry and reports hidden effects; missing permanent, aura and stack fields display the approved note without inferred Duel or tower buffs; selected text creates a source/time quote chip in the composer without changing tabs and saves one note per selection; every chat turn renders through the safe streaming renderer; document links open in the left panel; only time and evidence links seek; charts render only after validation; unknown values remain em dashes; and no context chips are shown; raw lane results remain unchanged; unknown expectations render "ยังไม่มีค่าคาดหมาย"; personal averages stay separate; all added readings keep the trial label; crowded strips fall back to lane cards; Turbo and unsupported shapes remain unknown; source conflicts show unknown and warn; calibration refuses fewer than 30 lanes and never changes shipped constants; and blind review cannot skip a stage; the local store keeps the newest 1,500 three-lane rows without person or match identifiers; writes are atomic; corrupt data remains unavailable; and clear-history deletes the lane store and calibration report; Coach Review is located in Lab or Developer mode. Personal average is visible only in the UI and absent from every AI request. Per-match chat, notes and reports can be deleted without a replay. Lane bundle dedup uses the separate keyed-token file and clear-history removes it; the legacy lane store is cleared once. [DEC-UX-REVIEW-20261125]. The Analytics match list has newest-first Thai day headings, date chips for all/today/7 days/30 days and a one-day picker, win/loss/all filters, and a filtered match count. Its bounded list scrolls independently with 200 rows, and rows expose time, own hero portrait/name, result, duration, replay state and open/parse/re-parse/remove actions beside one “เพิ่มไฟล์ .dem” button. The Replay/JSON report sub-tab and dropdown are absent. Unlinked identity is derived only from bundle player IDs plus the locally known own account ID; otherwise hero/result show “— ไม่ทราบ”..

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [SDD.md](../design/SDD.md), [DDD.md](../design/DDD.md), [LANE-EXPECTATION.md](../design/LANE-EXPECTATION.md), [DESIGN-IMPLEMENTATION.md](../design/DESIGN-IMPLEMENTATION.md)

**Contracts:** [replay-bundle.schema.json](../../contracts/replay-bundle.schema.json)

**Diagram views:** [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Reuse the current replay library and match viewer; keep policy and view-model logic testable without launching the app, and preserve unknown values. Use one hashed constants table and preserve source provenance, unknown reasons and the raw result.

#### UI-022 — Consented replay assistant with user-selected provider

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** owner_decision / DEC-ANALYTICS-V2

**Product needs:** PN-008 · **Primary design:** CMP-ADVICE · **Design components:** CMP-ADVICE, CMP-MEMORY, CMP-READMODEL

The optional Analytics assistant shall support Anthropic, OpenAI Platform API key, OpenRouter, Google Gemini, xAI Grok, DeepSeek, Qwen in DashScope compatible mode, Ollama, LiteLLM and user-entered OpenAI-compatible endpoints, using API keys only and no ChatGPT subscription sign-in. A provider remains hidden until an owner-side verified flag is set after a recorded live test; the off-by-default “แสดงผู้ให้บริการที่ยังไม่ทดสอบ” toggle reveals untested providers without verifying them. AI is off by default. A request requires explicit submit, a selected provider and model, consent for that provider and current origin, and its key where required. No question is sent during draft, pregame or live phases. Ollama and loopback LiteLLM are user-run programs that Kohbor never starts; the loopback label says Kohbor cannot know whether the local program forwards data. Fixed providers use compiled HTTPS hosts; Qwen uses a closed host list; custom endpoints allow HTTPS or HTTP only to 127.0.0.1, [::1] or localhost, without credentials, query or fragment, and redirects are never followed. Each key is DPAPI-protected per provider and bound to its origin, never returned to the renderer, logged or stored in the ledger. Every request always sends the assistant rules and tool definitions, the locally computed whole-match digest (all ten heroes, phases, lanes, deaths, fights and objectives), viewer facts, the saved-note index and stored AI report when present, replay clock, selected hero, explicit question and earlier user and assistant turns from the same match chat, including prior-provider replies. The digest cache lasts one hour. Other replay details are available through bounded read-only local tools; an open document is never inlined into the request. Consent names the recipient and host, describes this always-sent content and local read tools, and is shown again once for this consent-text update. The AI report is generated only after an explicit user click, shows a cost note, uses the Settings model (default `claude-opus-5-5`) and may be regenerated on request. The user pays providers directly; Kohbor adds no account, server, telemetry or billing. Models use a reviewed list, an explicit user-triggered fetch or validated free text except Anthropic, which keeps its closed list of three. The host streams where supported, validates note tools and offers a user-pressed “บันทึกเป็นโน้ต” action when tool calling is unavailable or fails. With AI off, missing consent or required key, invalid endpoint or an active match phase, no request occurs and local template questions remain available. Consent also names the newly sent lane fields: raw lane result; lane score and its available components; expected score, uncertainty, verdict and source; the separate personal average when available; and the source and state of the match result. The assistant must cite the supplied expectation source, never invent an expectation, and never state a match result when it is unknown. This digest change increments the consent text version so the user reviews consent again. The untested-provider toggle is visible only in Developer mode. In ordinary mode, AI is presented as unavailable in this release and the chat is collapsed. The screen-only "ค่าเฉลี่ยของคุณ" is excluded from AI content and transport.

**Failure behavior:** Missing provider, model, key, current-origin consent or AI enablement fails closed without transport. An active draft, pregame or live phase blocks sending with a clear message. Invalid or changed endpoints fail closed; changing a user endpoint deletes that provider key and consent. API key, permission, billing or quota, bad request, not-found, rate limit, overload, refusal, truncation, local-program-unavailable and network/TLS errors stay distinct and never include a key. A redirect is an error and is never followed. Unknown usage remains unknown; tool failures fall back to the user-pressed save-as-note action without writing automatically. Invalid tool input writes nothing. Consent revocation invalidates active work. An open document is not inlined, and no forbidden identity, raw match ID or file path is sent. Do not expose the untested-provider toggle outside Developer mode. In ordinary mode do not present AI as open for use; keep its chat collapsed. Never include personal average in a request, digest or provider payload.

**AC-UI-022:** Given fake transport and stream fixtures, provider and endpoint validation cases, AI/key/consent combinations, same-match transcript history, saved-note/report fixtures, a consent-version change, tool fallback and explicit report-generation fixtures; tests make no network calls, lane score, expected-versus-actual readings and source provenance in the selected-match digest; when the user selects a provider and model, reviews and grants consent, enables AI, submits or cancels a question, changes provider after the transfer warning, opens a document, requests an AI report and explicitly saves an answer as a note; then only explicit submit with current-provider/current-origin consent, the required key and AI enabled reaches the provider; the exact always-sent content is disclosed and matches the request; prior same-match turns are resent; details are read with local tools and open-document text is not inlined; no forbidden identity field is sent; no redirect is followed; a consent-text update is shown once again; report generation requires a user click, uses the chat model and discloses cost; invalid tools write nothing; and closed gates make zero transport calls; the exact new lane fields and their source are disclosed before consent, match result unknown stays unknown in assistant rules, and the changed consent text is versioned;  In Developer mode only, the user can reveal untested providers without marking them verified. In ordinary mode, AI is labelled unavailable in this release and the chat remains collapsed. AI request assertions prove the personal average is absent from digest, projection and transport. [DEC-UX-REVIEW-20261125]..

**Verification:** synthetic, native; procedures TP-08, UAT-07; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [SDD.md](../design/SDD.md), [DDD.md](../design/DDD.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [LANE-EXPECTATION.md](../design/LANE-EXPECTATION.md)

**Contracts:** [user-settings.schema.json](../../contracts/user-settings.schema.json)

**Diagram views:** [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd)

**Implementation freedom:** Reuse the current replay library and match viewer; keep policy and view-model logic testable without launching the app, and preserve unknown values. Keep the always-sent lane digest fields and consent disclosure synchronized.

#### UI-023 — Analytics match summary tab

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M5 · **Basis:** owner_decision / DEC-MATCH-SUMMARY; DEC-ANALYTICS-LAYOUT; DEC-LANE-EXPECTATION

**Product needs:** PN-008 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-MEMORY

The Analytics summary tab shall show, for the selected local match and from its local replay digest only: a header with winner, team kill totals, duration, game mode and patch; ten hero cards (five per team) with hero, K/D/A, net worth or, when net worth is absent and labelled as such, gold earned, lane and lane result; a per-minute Radiant-minus-Dire advantage chart with kill markers; buildings destroyed, on the minimap when a replay map calibration and building positions exist and otherwise as a list; lane results against the lane expectation; one scoreboard per team with K, D, A, net worth, LH/DN, GPM/XPM, hero damage and final items with purchase minute; the draft in pick and ban order; and key teamfights and objectives. Selecting a chart minute, kill marker, building, teamfight or objective seeks the replay to that time. The tab makes no network request and shows no data that is not in the digest. The summary includes a “คู่ปรับ” section for the selected own hero: five headline stats and one row per enemy hero with kills, deaths to that hero, kill gold and gold lost. Stats 1 and 3 use recorded deaths, killer identity and victim team. Non-enemy hero deaths are grouped as “อื่น ๆ”, ties show all leaders, and a zero event count says “ไม่มี”. Stats 2, 4 and 5 stay — with “ต้องแยกรีเพลย์ด้วยเอนจินรุ่นใหม่” until complete typed engine values are present; estimates and unknown-as-zero are forbidden.

**Failure behavior:** Every value absent from the digest renders “—” or “— ไม่ทราบ” with the digest's reason where one exists, never zero or an estimate. Team totals render only when all five heroes have the value. Unknown items differ from an empty item list. A building whose state is unknown is counted as unknown, not standing. Without map calibration or positions the building map is replaced by a list with the stated reason. A missing draft, series or outcome shows an unknown state; the tab never infers a winner. The “คู่ปรับ” section never attributes non-enemy deaths to a hero. Tied leaders all remain visible and zero event counts say “ไม่มี”. Stats 2, 4 and 5 without complete engine values render — with “ต้องแยกรีเพลย์ด้วยเอนจินรุ่นใหม่”; missing, incomplete or malformed values never become an estimate or zero. The post-game summary shows this section only when the just-finished match has a parsed, hash-linked replay.

**AC-UI-023:** Given digest fixtures with complete data, each field missing in turn (outcome, duration, mode, patch, net worth with and without gold earned, items, draft, advantage series, building positions, lane outcome), a roster with fewer than five heroes per team, and map calibration present and absent; and one real replay of a match the owner played; and own-hero death samples with enemy, allied, tower, creep, neutral, deny, suicide and unknown killers; tied and zero event counts; absent, partial, malformed and complete per-victim kill-gold, per-killer gold-loss and neutral-camp aggregates; plus post-game replay states that are missing, unparsed, hash-unlinked or ready and hash-linked.; when the user opens the summary tab and selects chart minutes, kill markers, buildings, teamfights and objectives; then present values match the digest exactly; every missing value shows the unknown mark with its reason and nothing is inferred; team totals appear only for complete rosters; the building view falls back to a list with its reason; each selection seeks the replay to the stated time; and no network request is made. The “คู่ปรับ” section shows all five headline stats and per-enemy kill/death/gold rows for the own hero; stats 1 and 3 use deaths, killer and victim team, other deaths remain under “อื่น ๆ”, ties retain every leader, and zero events say “ไม่มี”. Stats 2, 4 and 5 show — with “ต้องแยกรีเพลย์ด้วยเอนจินรุ่นใหม่” until complete engine data is present; unknown values are never estimated or shown as zero..

**Verification:** synthetic, native; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [DESIGN-IMPLEMENTATION.md](../design/DESIGN-IMPLEMENTATION.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [replay-bundle.schema.json](../../contracts/replay-bundle.schema.json)

**Implementation freedom:** Reuse the current replay library and match viewer; keep policy and view-model logic testable without launching the app, and preserve unknown values. Use one hashed constants table and preserve source provenance, unknown reasons and the raw result.

### STATS

#### STATS-001 — Separate consent for public hero statistics

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** owner_decision / DEC-PICK-STATS

**Product needs:** PN-014 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

The native host shall admit public hero statistics requests only under their own explicit Settings consent, default off. No sign-in, other consent or action shall grant it. Revocation shall stop new sends, cancel pending work, discard late responses and delete the statistics cache. Local core shall remain usable without the extension.

**Failure behavior:** Keep sends and cache access disabled after revocation, including on retry or restart. A late response shall not restore display or cache. Report a cache deletion error accurately and keep the revoked cache inaccessible.

**AC-STATS-001:** Given a fresh profile, separate account and consent states, and a controllable provider with a seeded stats cache after opt-in; when the player signs in, opens a draft and changes unrelated consents before explicitly opting in, then revokes while a request is pending; then only the dedicated switch enables the route. Revocation stops all new sends, discards the late response, clears displayed statistics and deletes the local stats cache while local core remains available.

**Verification:** synthetic, native; procedures TP-08, UAT-07; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [user-settings.schema.json](../../contracts/user-settings.schema.json), [settings-patch.schema.json](../../contracts/settings-patch.schema.json)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces within the stated behavior, permissions and evidence thresholds.

#### STATS-002 — Closed outgoing public statistics projection

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** owner_decision / DEC-PICK-STATS

**Product needs:** PN-014 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL, CMP-INGEST

For the v1 suggestions list the native host shall send no user- or match-specific data. It shall fetch only fixed public aggregate table requests, at most four per rolling 24 h, and filter locally using the deck-selected position and bracket. The existing pick-card own-hero-ID flow remains separately permitted under consent. Player names, account and match identifiers, hero IDs, voice, memory and all other match context shall not be sent for the v1 list. Counter suggestions make no request: they use only the reviewed table shipped with the app, and identified enemy or ally heroes are never sent. A table request counts toward the four once the host attempts to connect or send, whether it then succeeds or fails (connect failure, timeout, HTTP error, invalid body); a request that fails during local setup before any connect or send, or is cancelled before send, does not count (owner decision 2026-11-22). The cap stays four per rolling 24 h.

**Failure behavior:** Reject requests with extra or invalid fields before transport. Do not serialize a wider game, account or draft object or substitute a missing own hero ID.

**AC-STATS-002:** Given public hero statistics consent is on and test data contains the own hero, an optional manual bracket and forbidden fields; when the host constructs and sends a valid request, then receives negative cases with an extra field including each forbidden data class; then v1 list requests are fixed public table requests, no more than four per rolling 24 h counting every request that attempted to connect or send and none that failed or was cancelled before any network I/O, with no user- or match-specific fields; position and bracket filtering occurs locally, and counter suggestions cause no request and send no identified hero. The separate pick-card flow continues to carry only the admitted own hero ID and optional manual bracket, and forbidden context is rejected before send.

**Verification:** synthetic, native; procedures TP-08, UAT-07; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [DRAFT-BAR-SPEC.md](../design/DRAFT-BAR-SPEC.md)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces within the stated behavior, permissions and evidence thresholds.

#### STATS-003 — Honest public statistics values and states

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** owner_decision / DEC-PICK-STATS

**Product needs:** PN-014 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

Each displayed hero statistics value shall show its provider, provider-stated data window, fetch time and sample size. Label a missing data window as unspecified. Distinguish loading, live, stale, offline, rate-limited, provider error, unknown hero, not enough matches and consent off. Fix metric-specific minimum samples before evaluating data. Never show a match win-chance percentage. The v1 suggestions list shows historical bracket win rates and games; fewer than 1,000 games is a low-data state. Five position groups are estimates labelled “ค่าประมาณ” (estimate); listed heroes may already be picked or banned and this limitation is disclosed. No win-chance percentage is shown. Provider facts and unknowns are stated in the design: OpenDota heroStats and laneRoles have no stated data window, Immortal values were zero at review, lane data does not separate all five positions, and quota, licence and data window remain unverified.

**Failure behavior:** Withhold unsupported or undersampled values. Use eligible cached values only with explicit age and stale labelling when applicable. Never silently fall back, invent a number, reuse another hero's results or interpret missing data as zero.

**AC-STATS-003:** Given minimum samples have been fixed and fixtures cover valid and missing metadata, stale data, every unavailable state and sample sizes around the minima; when the host evaluates provider responses, cache expiry, a new hero and consent transitions; then each shown value carries source, window or unspecified label, fetch time and its sample size, every state is distinct, and insufficient or expired data produces no unsupported value. No invented value, silent fallback or win-chance percentage appears; the suggestions list shows source and sample, low-data below 1,000 games, labels estimated five-position groups, discloses possible banned/picked heroes, shows no win-chance percentage, and never fills unknown provider metadata.

**Verification:** synthetic, native; procedures TP-08, UAT-07; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces within the stated behavior, permissions and evidence thresholds.

### VOICE

#### VOICE-001 — Explicit microphone consent

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-006 · **Primary design:** CMP-VOICE · **Design components:** CMP-VOICE

The microphone shall be off by default and opened only for a user-held PTT turn after local opt-in.

**Failure behavior:** Other consent and account sign-in never enable mic.

**AC-VOICE-001:** Given mic consent is false; when PTT or account/cloud/peer action occurs; then no microphone stream opens.

**Verification:** synthetic, native, live; procedures TP-06, UAT-06; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [NFR.md](../quality/NFR.md), [SDD.md](../design/SDD.md)

**Contracts:** [error-catalog.json](../../contracts/error-catalog.json)

**Diagram views:** [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-009](../diagrams/source/DGM-009-voice-lifecycle.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### VOICE-002 — Remappable conflict-safe PTT

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-006 · **Primary design:** CMP-VOICE · **Design components:** CMP-VOICE

Default PTT Ctrl+Alt+V shall be remappable with registration conflict detection and transactional replacement.

**Failure behavior:** Do not silently remap or steal reserved keys; Alt+M remains mute.

**AC-VOICE-002:** Given a new binding cannot be registered; when the user changes the shortcut; then the prior working binding/settings remain and error is visible.

**Verification:** synthetic, native, live; procedures TP-06, UAT-06; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [NFR.md](../quality/NFR.md), [SDD.md](../design/SDD.md)

**Contracts:** [error-catalog.json](../../contracts/error-catalog.json)

**Diagram views:** [DGM-009](../diagrams/source/DGM-009-voice-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### VOICE-003 — One bounded voice turn

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-006 · **Primary design:** CMP-VOICE · **Design components:** CMP-VOICE

Voice shall have one fenced active turn, minimum hold 250ms and maximum 15s.

**Failure behavior:** Late callbacks cannot resume the superseded request.

**AC-VOICE-003:** Given a turn is decoding and a new valid press arrives; when the new turn starts; then the prior turn is cancelled; unmatched release and repeat held press do not create work.

**Verification:** synthetic, native, live; procedures TP-06, UAT-06; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [NFR.md](../quality/NFR.md), [SDD.md](../design/SDD.md)

**Contracts:** [error-catalog.json](../../contracts/error-catalog.json)

**Diagram views:** [DGM-009](../diagrams/source/DGM-009-voice-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### VOICE-004 — Bounded transient audio

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-006 · **Primary design:** CMP-VOICE · **Design components:** CMP-VOICE

Mic capture shall bound PCM to 480000 bytes of mono 16kHz i16 data with 2s open timeout.

**Failure behavior:** No persistence or indefinite decoder retention; STT ownership/lifetime must be closed before integration.

**AC-VOICE-004:** Given device opening fails or a turn is cancelled; when capture exits; then stream/resources are released and transient data is zeroed/dropped within the frozen handoff contract.

**Verification:** synthetic, native, live; procedures TP-06, UAT-06; status NOT_RUN.

**Open dependencies:** DEC-VOICE-BUFFER.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [NFR.md](../quality/NFR.md), [SDD.md](../design/SDD.md)

**Contracts:** [error-catalog.json](../../contracts/error-catalog.json)

**Diagram views:** [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-009](../diagrams/source/DGM-009-voice-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### VOICE-005 — All-local conversation

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-006 · **Primary design:** CMP-VOICE · **Design components:** CMP-VOICE

STT, transcript, free-form question, interpretation and answer shall remain local.

**Failure behavior:** Never fall back to cloud or log raw speech/text.

**AC-VOICE-005:** Given local STT or answer provider is unavailable; when the player finishes PTT; then the product reports unavailable and preserves critical local alerts.

**Verification:** synthetic, native, live; procedures TP-06, UAT-06; status NOT_RUN.

**Open dependencies:** DEC-MODELS, DEC-VOICE-BUFFER.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [NFR.md](../quality/NFR.md), [SDD.md](../design/SDD.md)

**Contracts:** [error-catalog.json](../../contracts/error-catalog.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-009](../diagrams/source/DGM-009-voice-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### VOICE-006 — Priority preemption

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-006 · **Primary design:** CMP-VOICE · **Design components:** CMP-VOICE, CMP-MEDIA, CMP-ALERT

Voice answers shall use Normal priority, preemptible by announcer and critical audio; critical preemption shall not wait on Voice locks.

**Failure behavior:** No stale answer or visual tail may resume later. Ordinary announcer preemption also fences the superseded voice answer.

**AC-VOICE-006:** Given a Voice turn is listening/decoding/answering/speaking; when a qualified critical alert is admitted; then the turn is cancelled and the urgent cue meets its acoustic deadline.

**Verification:** synthetic, native, live; procedures TP-06, UAT-06; status NOT_RUN.

**Open dependencies:** DEC-EVENT-AUDIO.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [NFR.md](../quality/NFR.md), [SDD.md](../design/SDD.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [error-catalog.json](../../contracts/error-catalog.json), [audio-outcome.schema.json](../../contracts/audio-outcome.schema.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-009](../diagrams/source/DGM-009-voice-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### VOICE-007 — Honest preparation and failure

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-006 · **Primary design:** CMP-VOICE · **Design components:** CMP-VOICE

Voice shall distinguish preparing, missing model, device/permission error and timeout.

**Failure behavior:** Do not hide failed/rejected turns from completion/latency statistics.

**AC-VOICE-007:** Given a selected local model is cold or absent; when a turn requests inference; then the UI shows preparing or unavailable with bounded completion behavior.

**Verification:** synthetic, native, live; procedures TP-06, UAT-06; status NOT_RUN.

**Open dependencies:** DEC-MODELS.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [NFR.md](../quality/NFR.md), [SDD.md](../design/SDD.md)

**Contracts:** [error-catalog.json](../../contracts/error-catalog.json)

**Diagram views:** [DGM-009](../diagrams/source/DGM-009-voice-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### MEM

#### MEM-001 — Local evidence-supported aggregates

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-008 · **Primary design:** CMP-MEMORY · **Design components:** CMP-MEMORY

Player memory shall store only supported local aggregates with evidence provenance.

**Failure behavior:** Peer data and model guesses cannot become observed memory.

**AC-MEM-001:** Given an archive lacks explicit outcome or death-cause facts; when the memory reader builds a snapshot; then those facts remain unknown and no fabricated winrate/death narrative appears.

**Verification:** synthetic, native; procedures TP-07, UAT-08; status NOT_RUN.

**Elaboration:** [DDD.md](../design/DDD.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-006](../diagrams/source/DGM-006-logical-erd.mmd), [DGM-015](../diagrams/source/DGM-015-memory-forget.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEM-002 — Atomic versioned snapshots

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-008 · **Primary design:** CMP-MEMORY · **Design components:** CMP-MEMORY

Memory snapshots shall use schema/source fingerprint and atomic replacement.

**Failure behavior:** Changing storage engines needs migration and budget evidence.

**AC-MEM-002:** Given a snapshot rebuild fails halfway; when the next reader opens memory; then the prior valid snapshot or explicit unavailable state is used; no partial snapshot is admitted.

**Verification:** synthetic, native; procedures TP-07, UAT-08; status NOT_RUN.

**Elaboration:** [DDD.md](../design/DDD.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Diagram views:** [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-006](../diagrams/source/DGM-006-logical-erd.mmd), [DGM-015](../diagrams/source/DGM-015-memory-forget.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEM-003 — Durable forget cutoff

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-008 · **Primary design:** CMP-MEMORY · **Design components:** CMP-MEMORY

Forget shall durably advance a nondecreasing logical/epoch cutoff comparable with archive timestamps before deleting derived snapshots.

**Failure behavior:** Corrupt cutoff fails closed. Do not persist a process-monotonic timestamp across restarts; conservatively exclude same-granularity source timestamps.

**AC-MEM-003:** Given older local logs remain after a memory clear; when the application restarts and rebuilds memory; then sources at or before cutoff cannot be re-learned; cutoff cannot move backward.

**Verification:** synthetic, native; procedures TP-07, UAT-08; status NOT_RUN.

**Elaboration:** [DDD.md](../design/DDD.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Diagram views:** [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-006](../diagrams/source/DGM-006-logical-erd.mmd), [DGM-015](../diagrams/source/DGM-015-memory-forget.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEM-004 — Bounded source-linked summaries

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-008 · **Primary design:** CMP-MEMORY · **Design components:** CMP-MEMORY

A completed-session summary shall contain at most 3 evidence-linked points with timestamps.

**Failure behavior:** Do not call a model narrative a proven event.

**AC-MEM-004:** Given a session is incomplete or an outcome is unobserved; when summary generation runs; then partial/unknown is shown and win/loss/death cause are not inferred.

**Verification:** synthetic, native; procedures TP-07, UAT-08; status NOT_RUN.

**Open dependencies:** DEC-SESSION.

**Elaboration:** [DDD.md](../design/DDD.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Diagram views:** [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-006](../diagrams/source/DGM-006-logical-erd.mmd), [DGM-013](../diagrams/source/DGM-013-match-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEM-005 — Separate history deletion

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-008 · **Primary design:** CMP-MEMORY · **Design components:** CMP-MEMORY

Clearing memory shall preserve separately governed raw history unless the user explicitly deletes it.

**Failure behavior:** Automatic archival retention/deletion needs an explicit rule.

**AC-MEM-005:** Given the player chooses forget memory; when clear executes; then derived memory is forgotten and unrelated archives remain.

**Verification:** synthetic, native; procedures TP-07, UAT-08; status NOT_RUN.

**Open dependencies:** DEC-RETENTION.

**Elaboration:** [DDD.md](../design/DDD.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Diagram views:** [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-006](../diagrams/source/DGM-006-logical-erd.mmd), [DGM-015](../diagrams/source/DGM-015-memory-forget.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEM-006 — Local own-match history

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M3 · **Basis:** owner_decision / DEC-OVERVIEW

**Product needs:** PN-008 · **Primary design:** CMP-MEMORY · **Design components:** CMP-MEMORY, CMP-INGEST

From admitted game data with verified ownership, the host shall record one entry per own match: the session's keyed match hash (never the raw match ID or SteamID64), own hero, result (win when map.win_team equals the own team, loss when it is the other team, otherwise null), kills, deaths, assists, duration from the game clock, end time, match type and source "kohbor", and no other player's name or identifier. A match left before a winner is reported (menu, game window or process loss, or a new match) is recorded with a null result, never as a loss. Match type is taken only from an explicit, documented GSI field; GSI provides none (GSI-INTAKE-SPEC 4.2, A-9), so it is recorded as unknown. Win rates count every recorded match with a known result whose type is ranked, unranked or unknown; until match types are known this means all modes, and the rate is labelled as covering all modes. Bot, custom and lobby matches, once their type is known, are recorded but not counted; custom games are never recorded. Matches with a null result are never counted. The history is an atomic JSON file in the app data directory, bounded to the newest 500 entries, and the user can clear it from the Overview. When the user explicitly opts in (off by default), the native host shall persist enabled consent even if no current verified GSI account ID is available; when current verified GSI ownership becomes available after opt-in, it shall send only the user's own positive 32-bit Steam account ID, derived from that verified ownership, to the fixed /api/players/{account_id}/matches endpoint on api.opendota.com and request at most 150 entries with only match_id, hero_id, player_slot, radiant_win, kills, deaths, assists, duration, start_time, lobby_type and game_mode. Fetch on demand at most once automatically per app start, and enforce the mirrored four network-reaching requests per rolling 24 h budget. OpenDota sees the account ID, client IP address and request time; no SteamID64 or raw match ID is sent or stored. Persist the 32-bit account ID only in opendota-settings.json while enabled; persist only install-keyed match hashes and projected fields in opendota-history-cache.json, bounded to 500 entries with fetch time. Persist only request timestamps in opendota-request-ledger.json to enforce this budget. Merge local history first and fill only older matches, deduplicating by the same installation-keyed match hash; local unknown match types continue to count as all modes, while imported bot, custom and lobby matches are excluded. Revocation stops requests, discards late results, deletes the account ID and OpenDota cache. The Overview clear action deletes both local and OpenDota history while preserving consent.

**Failure behavior:** Unverified or mismatched ownership, custom games and spectating record nothing. A corrupt or oversized file starts empty with one warning code, and a failed write keeps the previous file. An abandoned entry never replaces a completed entry for the same match. A hero win rate is computed only from 5 counted games. OpenDota is not contacted while consent is off, without a current verified own account ID, or without the installation HMAC key. Private profile, empty result, 404, provider rate limit, local request-budget exhaustion, offline transport and invalid response are explicit states; failed requests preserve any previous cache. Revocation cancels transport and prevents late cache writes before deleting the cache. A cache-write or revoke-deletion failure is reported.

**AC-MEM-006:** Given admitted payload sequences for a completed match won and lost, a match left before the winner is known, a draft-only lobby, a custom game, an unverified owner, a captured bot match and a corrupt history file, consent-off/on states and synthetic OpenDota fixtures covering valid, duplicate, private, empty, 404, rate-limit, offline and malformed responses; when the host processes the payloads, restarts, clears the history and computes the summary, the user enables own-history, requests the Overview, retries, clears both stores and revokes consent; then exactly one entry per played match is stored with the keyed hash and no raw match ID or SteamID64; the result follows win_team against the own team; a left match has a null result; the lobby, custom game and unverified owner store nothing; every match type is unknown and counted when it has a result; the summary counts ranked, unranked and unknown-type matches with a result and excludes bot, custom, lobby and null-result matches; the file never exceeds 500 entries; a corrupt file starts empty; and clearing empties the store; no request occurs before consent; requests use only the verified 32-bit own account ID, fixed endpoint, projected fields and a limit no greater than 150; at most one automatic request occurs per app start and no more than four network-reaching requests occur per rolling 24 h; matches merge by the same install-keyed hash with local rows preferred and only older provider rows added; bot, custom and lobby provider rows are excluded while local unknown types count until known; each failure and empty result has its explicit state; only the account ID while enabled, keyed match hashes and projected cached fields are stored; revoke stops sends, removes the account ID and cache, and clearing empties both histories without turning consent off.

**Verification:** synthetic, native; procedures TP-07, UAT-08; status NOT_RUN.

**Open dependencies:** DEC-RETENTION.

**Elaboration:** [GSI-INTAKE-SPEC.md](../design/GSI-INTAKE-SPEC.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Implementation freedom:** Choose the storage layout and summary computation within the stated fields, bounds and counting rule. A match-type source needs an explicit GSI field or its own decision.

#### MEM-007 — Replay analysis and bounded bundle retention

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M5 · **Basis:** owner_decision / DEC-REPLAY-AUTO-PARSE; DEC-REPLAY-AUTOFETCH

**Product needs:** PN-008 · **Primary design:** CMP-MEMORY · **Design components:** CMP-MEMORY, CMP-INGEST, CMP-READMODEL

Replay acquisition keeps the read-only Dota replay-folder watcher and manual .dem picker. The existing persisted `auto_analyse` Settings switch (default on) is labelled “แยกรีเพลย์อัตโนมัติ” / “Parse replays automatically”; it remains the single source of truth and the owner may reverse its default. When on, newly detected watched .dem entries that become Available are automatically queued newest first, subject to the retention-bounded automatic queue; manual file picks remain explicit parse requests, and nothing auto-queues while the switch is off. Automatic and manual requests share the existing queue and match barrier, run one parse at a time at low priority only after the player leaves the match, and show ready only after bundle validation. Removing a watched entry suppresses rediscovery while its .dem remains in Dota’s folder; failed and needs_pack entries are never automatically retried. Turning the switch off cancels queued automatic jobs while a running parse finishes. Turning it on queues existing Available watched entries once, newest first, capped by configured retention so automatic backlog queueing never exceeds that count. Keep the newest 20 validated derived bundles plus every pinned bundle (“เก็บไว้”), with a configurable retained count of 5 to 100 and a configurable size cap defaulting to 2 GiB. Pins do not count toward newest N but count toward bytes. On either limit, remove oldest unpinned Kohbor-derived bundles only. Never delete pins, source .dem files or Dota-folder contents. Keep notes and chat after bundle eviction, label the replay removed, and delete notes/chat only on separate user action. Match bundles to MEM-006 with the existing install-keyed match hash and show matched own hero/result/K/D/A/date; otherwise values remain unknown until the user sets their hero. Make no network request (except the opt-in replay fetch of MEM-008) or Dota write, and do not add match IDs or replay paths to logs. An automatic job interrupted by process exit or stopped by a match start returns to Available/not parsed and is auto-queued again when the match barrier clears; genuine parse failures remain manual and are not silently retried. Queue order and retention eviction use match ended time. Evicted replays remain in the library as lightweight rows. Retention fields apply only after explicit Save, which previews how many matches will be removed. Use the approved parsing labels: "ยังไม่แยก", "รอคิว", "กำลังแยก", "พร้อมดู" and "แยกไม่สำเร็จ"; reserve "วิเคราะห์" for analysis and AI.

**Failure behavior:** The existing match barrier must clear before parsing; an unknown phase or unavailable source never authorizes a parse. Turning auto off leaves no queued automatic job and does not cancel an already running parse. Removed watched entries remain suppressed while their source .dem remains; failed and needs_pack entries are not auto-retried. A bundle remains unavailable until validation succeeds. Unknown values show “— ไม่ทราบ”. If pinned bundles alone exceed the cap, preserve them and show the over-cap state. Eviction failure preserves the prior index and files. Bundle removal never deletes .dem, notes or chat. Logs contain no match IDs or replay paths. Interrupted or match-start-stopped automatic work must not be marked as a real parse failure; return it to Available for autoqueue. Real failed jobs remain manual. Unknown ended time does not become an invented ordering timestamp. Evicted entries retain their light row and match end time. Retention changes do not apply until Save and the removal count is shown before applying.

**AC-MEM-007:** Given library fixtures with watched and manual sources, newly detected Available entries, newest-first timestamps, removed-entry tombstones, failed and needs_pack entries, persisted default-on/off/corrupt settings, an active and cleared match barrier, queued and running jobs, and retention count and byte limits; when the watcher discovers files, auto is toggled off and on, an explicit manual parse is requested, a watched entry is removed, a replay reaches failed or needs_pack, backlog queueing runs, and a bundle is validated or evicted; then new watched Available entries queue automatically only when enabled, newest first, through the shared one-worker match-barrier queue; manual picks remain explicit; parsing waits until the barrier clears; off cancels queued automatic work but not a running parse; on queues existing Available watched entries once within retention; removed, failed and needs_pack entries are never auto-queued; ready follows validation; retention never deletes .dem, pins, notes or chat; no network (outside MEM-008), Dota writes, match IDs or paths enter logs;  Interrupt a queued/running automatic parse by process restart and by a match starting: it returns to Available and is queued again once safe. A real failure remains failed and manual. Queue and eviction use match end time; eviction removes the bundle but preserves a lightweight match row. Changing retention shows a removal-count preview and takes effect only after Save. UI uses the approved five parse-state labels and uses "วิเคราะห์" only for analysis/AI. [DEC-UX-REVIEW-20261125]..

**Verification:** synthetic, native; procedures TP-07, UAT-08; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [DDD.md](../design/DDD.md), [SDD.md](../design/SDD.md), [REPLAY-PIPELINE.md](../design/REPLAY-PIPELINE.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [replay-bundle.schema.json](../../contracts/replay-bundle.schema.json), [user-settings.schema.json](../../contracts/user-settings.schema.json)

**Diagram views:** [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-013](../diagrams/source/DGM-013-match-lifecycle.mmd)

**Implementation freedom:** Reuse the current replay library and match viewer; keep policy and view-model logic testable without launching the app, and preserve unknown values.

#### MEM-008 — Opt-in replay fetch via OpenDota

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M5 · **Basis:** owner_decision / DEC-REPLAY-AUTOFETCH

**Product needs:** PN-008 · **Primary design:** CMP-MEMORY · **Design components:** CMP-MEMORY, CMP-INGEST

Settings → Replay & AI has a switch “ดึงรีเพลย์อัตโนมัติผ่าน OpenDota” / “Fetch replays automatically via OpenDota”, off by default and separate from the OpenDota history (MEM-006) and benchmark consents. Before it can be enabled, its consent text states that the match ID and the user's IP address go to api.opendota.com, that the replay is downloaded from Valve's replay host over plain HTTP, that neither service is operated by Kohbor, and that turning it off cancels pending work and deletes downloaded files. While on, when the player's own match ends and the existing match barrier clears, the host takes the match ID held in memory for the current session only (never from logs, history or settings) and, unless that match's .dem is already in the watched Dota replay folder or in the library, asks OpenDota `GET /api/matches/{match_id}` for the replay location; when no replay URL is present it may send at most one `POST /api/request/{match_id}` and poll with backoff within a bounded per-match budget. It then downloads the replay file from the Valve replay host into a staging folder inside the replay library, detects the container by magic bytes (Zstandard only; any other container is rejected as failed), decompresses it under a size cap and a disk-free floor, and hands the .dem to the existing parse queue as an automatic request; ready still follows bundle validation. The downloaded and decompressed files are deleted after a successful parse; a failed parse keeps no download. The fetch sends no account ID, player name or other match data, never writes to the Dota folder, and logs only state transitions without match IDs, URLs or paths. The post-game replay box shows the fetch states: off, waiting for match end, looking up, requested, downloading, parsing, ready, not available yet, expired, rate-limited, offline and failed.

**Failure behavior:** Off, unknown consent or a corrupt setting means no network request. No match ID in memory (for example after an app restart) means no fetch and the manual guidance stays. Rate-limited, offline, not-found, not-yet-available and expired responses become explicit states with bounded backoff; there are no hidden retries after the app exits and no retry loop beyond the per-match budget. A download over the size cap, a failed decompression, an unknown container or a disk floor breach deletes the staged file and shows failed. A file the engine rejects is a genuine parse failure and is not retried automatically. Turning the switch off cancels lookups and downloads, deletes staged files and leaves an already running parse to finish. A .dem that appears in the watched folder first wins and cancels the fetch.

**AC-MEM-008:** Given a release build with the switch off, then on, a finished own match with its ID held in memory, and synthetic OpenDota and replay-host transports returning a replay URL, no replay URL then a URL after a request, 404, 429, offline, an oversized body, a bzip2 body, a Zstandard body and a corrupt body; when the match ends and the barrier clears, the switch is toggled, the same .dem also appears in the watched folder, and the app restarts mid-download; then nothing is requested while off; while on, one lookup per match and at most one parse request are sent with only the match ID; the replay is downloaded, accepted only as Zstandard by magic bytes (a bzip2 or unknown body fails and is deleted), decompressed within the cap and queued as an automatic parse; the download is deleted after a successful parse and on every failure; each failure maps to its explicit state without unbounded retries; a watched-folder .dem cancels the fetch; turning off cancels and deletes staged files; logs contain no match ID, URL or path; native evidence is one owner match fetched end to end in a release build..

**Verification:** synthetic, native; procedures TP-07, UAT-08; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [REPLAY-PIPELINE.md](../design/REPLAY-PIPELINE.md), [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [user-settings.schema.json](../../contracts/user-settings.schema.json)

**Implementation freedom:** Reuse the existing OpenDota WinHTTP transport, request ledger and the replay library's queue and barrier; decode with the owner-approved ruzstd 0.9.1 (default features off); keep the fetch state machine testable with fake transports.

### ADVICE

#### ADVICE-001 — Required local/manual advice

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-007 · **Primary design:** CMP-ADVICE · **Design components:** CMP-ADVICE

Manual advice shall be noncritical, locally available when an admitted backend is ready, and preemptible.

**Failure behavior:** Unavailable local provider produces unavailable, not a silent remote request.

**AC-ADVICE-001:** Given cloud consent is off and local inference is ready; when manual advice is requested; then only permitted local context is used and critical warnings retain priority.

**Verification:** synthetic, native; procedures TP-08, UAT-07; status NOT_RUN.

**Open dependencies:** DEC-MODELS.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [cloud-context.schema.json](../../contracts/cloud-context.schema.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ADVICE-002 — Independent cloud consent

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-007 · **Primary design:** CMP-ADVICE · **Design components:** CMP-ADVICE

Cloud gameplay context shall default off and require a separate field/purpose opt-in.

**Failure behavior:** Block the send and offer local/unavailable.

**AC-ADVICE-002:** Given the user signs in, selects a provider, enters a key or presses Ask; when cloud advice dispatch is evaluated; then none of those actions alone grants context consent.

**Verification:** synthetic, native; procedures TP-08, UAT-07; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [cloud-context.schema.json](../../contracts/cloud-context.schema.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ADVICE-003 — Closed cloud projection

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-007 · **Primary design:** CMP-ADVICE · **Design components:** CMP-ADVICE

Cloud context shall be built only from coarse phase, allowed own stats, team score and fixed non-player instructions.

**Failure behavior:** Never serialize the entire game/local context and redact afterward.

**AC-ADVICE-003:** Given a local state also contains enemy data, items, exact clock, memory, voice or account identifiers; when cloud projection is built; then only the closed allowed fields pass and extra fields are rejected.

**Verification:** synthetic, native; procedures TP-08, UAT-07; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [cloud-context.schema.json](../../contracts/cloud-context.schema.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ADVICE-004 — Consent and session cancellation

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-007 · **Primary design:** CMP-ADVICE · **Design components:** CMP-ADVICE, CMP-VOICE

Cloud consent/backend/session changes shall invalidate sends, cache and late responses.

**Failure behavior:** Retries, CLI/API and error paths cannot bypass the latest consent generation.

**AC-ADVICE-004:** Given a consented request is in flight; when consent is revoked before response delivery; then future sends stop and the old response is discarded.

**Verification:** synthetic, native; procedures TP-08, UAT-07; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [cloud-context.schema.json](../../contracts/cloud-context.schema.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### PEER

#### PEER-001 — Fresh opt-in join

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-009 · **Primary design:** CMP-PEER · **Design components:** CMP-PEER

Every create/join shall require checked sharing consent and explicit activation in one action.

**Failure behavior:** No autojoin, saved room credentials or view-only mode.

**AC-PEER-001:** Given peer consent is false or an invitation is accepted without new checked consent; when native join is requested; then the request is rejected before networking.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [peer-sample.schema.json](../../contracts/peer-sample.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-011](../diagrams/source/DGM-011-peer-sequence.mmd), [DGM-012](../diagrams/source/DGM-012-peer-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PEER-002 — Strict source admission

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-009 · **Primary design:** CMP-PEER · **Design components:** CMP-PEER

Peer publication shall require app 570, playing activity, verified local owner, positive match ID, exact team and pregame/in-progress.

**Failure behavior:** Do not replay an older valid source until a new admitted observation arrives.

**AC-PEER-002:** Given one required source condition is missing or invalid JSON arrives; when publication is evaluated; then prior publication is withdrawn and no new sample is sent.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [peer-sample.schema.json](../../contracts/peer-sample.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-011](../diagrams/source/DGM-011-peer-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PEER-003 — Narrow complete vitals

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-009 · **Primary design:** CMP-PEER · **Design components:** CMP-PEER

Peer stats shall contain only own hero/level/HP/maxHP/mana percent/alive/respawn with exact bounds.

**Failure behavior:** Ability/item sharing requires a new protocol and consent decision.

**AC-PEER-003:** Given a sample omits alive, has HP above max or includes ability/item/position; when schema and semantic admission run; then the sample is rejected; unknown is not defaulted.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [peer-sample.schema.json](../../contracts/peer-sample.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-011](../diagrams/source/DGM-011-peer-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PEER-004 — Bounded membership

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-009 · **Primary design:** CMP-PEER · **Design components:** CMP-PEER

Rooms shall contain at most 5 members including owner, last at most 4 hours and expire unsynced membership after 30s.

**Failure behavior:** Invite rotation invalidates future old-code joins but does not itself kick current members.

**AC-PEER-004:** Given owner leaves or membership exceeds its timeout; when relay state advances; then the appropriate membership/room closes and clients require explicit new join.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [peer-sample.schema.json](../../contracts/peer-sample.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-012](../diagrams/source/DGM-012-peer-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PEER-005 — Conservative peer age

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-009 · **Primary design:** CMP-PEER · **Design components:** CMP-PEER

Peer freshness shall use accumulated monotonic durations and become stale after 2000ms.

**Failure behavior:** Do not compare wall clocks across devices or retain live vitals when IPC stops.

**AC-PEER-005:** Given an observation is repeatedly polled without a new game sample; when relay/native/UI age calculation advances; then polling never rejuvenates it; 2000ms may be live and 2001ms is stale/null.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [peer-sample.schema.json](../../contracts/peer-sample.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-011](../diagrams/source/DGM-011-peer-sequence.mmd), [DGM-012](../diagrams/source/DGM-012-peer-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PEER-006 — Bounded network worker

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-009 · **Primary design:** CMP-PEER · **Design components:** CMP-PEER

Peer transport shall use one sequential in-flight sync worker outside the alert path.

**Failure behavior:** Use 1500ms request timeout, 32KiB reply cap and normal 500ms cadence; no task per game tick.

**AC-PEER-006:** Given a transport timeout occurs; when retry logic runs; then bounded 0.5–5s retry and successful null probe precede resuming samples.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [peer-sample.schema.json](../../contracts/peer-sample.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-011](../diagrams/source/DGM-011-peer-sequence.mmd), [DGM-012](../diagrams/source/DGM-012-peer-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PEER-007 — Native credentials and trusted origins

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-009 · **Primary design:** CMP-PEER · **Design components:** CMP-PEER

Peer networking shall be native-owned HTTPS with literal loopback HTTP only for local testing. Account and refresh tokens shall be native-owned, sent only to the pinned account-service origin, and never be present in the renderer.

**Failure behavior:** No URL credentials/query/fragment, redirects or token logging. An account or refresh token exposed to the renderer or sent to an origin other than the pinned account-service origin fails.

**AC-PEER-007:** Given a renderer asks for an arbitrary origin/path/redirect or account, refresh or member credentials; when transport authorization runs; then untrusted request is rejected and tokens stay native; account and refresh tokens are sent only to the pinned account-service origin and never enter the renderer.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [peer-sample.schema.json](../../contracts/peer-sample.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Diagram views:** [DGM-003](../diagrams/source/DGM-003-deployment.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-011](../diagrams/source/DGM-011-peer-sequence.mmd), [DGM-012](../diagrams/source/DGM-012-peer-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PEER-008 — Leave fences old work

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-009 · **Primary design:** CMP-PEER · **Design components:** CMP-PEER

Leave shall clear local credentials and peer samples immediately and fence pending responses.

**Failure behavior:** Remote delete is best effort; report expiry behavior without promising instantaneous remote erasure.

**AC-PEER-008:** Given a join/sync response is delayed; when the player leaves before it arrives; then late data cannot reconnect or repopulate the room.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [peer-sample.schema.json](../../contracts/peer-sample.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-011](../diagrams/source/DGM-011-peer-sequence.mmd), [DGM-012](../diagrams/source/DGM-012-peer-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PEER-009 — No peer persistence or AI

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-009 · **Primary design:** CMP-PEER · **Design components:** CMP-PEER

Peer stats shall remain transient within authorized same-room/match/team viewing. The account service persists only: login-identity hash, account id, friend code, display name, settings, device public keys, session records, friendships, pending requests (30-day expiry), blocks, Share list entries (scopes, pause state, last-confirmed date), pairing ids, rate-limit counters, and single-use sign-in, refresh and delete challenges (nonce hash, device-key thumbprint, purpose, times), kept until consumed or expired and then purged (expiry at most 10 minutes). It never persists: email, raw Google subject, Google tokens, Steam ID, IP address history, presence or last-seen history, match ids, match bindings, pairing activity times, or any sample (ACCOUNT-IDENTITY-REVIEW rule 26, as amended by ACCOUNT-DATABASE-DESIGN section 14.3).

**Failure behavior:** Friendship/premium/cloud consent cannot expand recipients. Peer samples remain transient; the account service fails this requirement if it persists any value outside the rule-26 allow-list or any deny-list value, including a challenge after consumption or expiry or beyond the 10-minute maximum.

**AC-PEER-009:** Given memory/history/advice attempts to consume a peer sample or account records are persisted; when peer-data routing and account persistence are checked; then the route is denied and samples are not written to logs or AI; only rule-26 allow-list fields persist, single-use challenges are purged after consumption or expiry within at most 10 minutes, and no deny-list value is stored.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [peer-sample.schema.json](../../contracts/peer-sample.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-011](../diagrams/source/DGM-011-peer-sequence.mmd), [DGM-012](../diagrams/source/DGM-012-peer-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PEER-010 — Separate-scope ability and cooldown sharing

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** owner_decision / DEC-PEER-ABILITIES

**Product needs:** PN-009 · **Primary design:** CMP-PEER · **Design components:** CMP-PEER

Peer ability and cooldown sharing shall require a separate opt-in abilities scope and use a separate closed DTO containing only the sender's admitted own-hero ability slots, learned levels, ultimate flag and cooldown seconds.

**Failure behavior:** Without the abilities scope nothing is sent or shown; unknown cooldown stays unknown; cooldown zero is not shown as ready; no item, position, enemy or selected-unit data.

**AC-PEER-010:** Given a teammate shares vitals but has not granted the abilities scope, and another teammate has granted both; when the receiver renders both allied cards; then the first shows vitals with a not-shared skill row and no ability payload was transmitted; the second shows only admitted ability levels and cooldowns with their age, and withdrawing the scope clears them.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES, DEC-GAME-DATA.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [peer-abilities.schema.json](../../contracts/peer-abilities.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### ACCOUNT

#### ACCOUNT-001 — External-provider sign-in

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

Sign-in uses one external identity provider through the system browser with loopback redirect and PKCE; no embedded webview; no credentials are entered in Kohbor

**Failure behavior:** A flow using multiple providers, an embedded webview, or Kohbor-entered credentials fails.

**AC-ACCOUNT-001:** Given a user starts sign-in with the configured external identity provider; when the provider hand-off returns through the loopback redirect; then the flow uses the system browser and PKCE, uses one provider, opens no embedded webview, and collects no credentials in Kohbor.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-ACCOUNT-ORIGIN.

**Elaboration:** [ACCOUNT-IDENTITY-REVIEW.md](../design/ACCOUNT-IDENTITY-REVIEW.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ACCOUNT-002 — Minimal provider identity storage

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

The service stores a keyed hash of the provider subject and no email, profile or provider token

**Failure behavior:** The requirement fails if service storage contains the raw provider subject, email, profile data or a provider token.

**AC-ACCOUNT-002:** Given an external identity provider has authenticated a user; when the account service stores the login identity; then it stores a keyed hash of the provider subject and stores no email, profile or provider token.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-ALPHA-AUTH-DEVIATION, DEC-ACCOUNT-ORIGIN, DEC-PRIVACY-LEGAL.

**Elaboration:** [ACCOUNT-IDENTITY-REVIEW.md](../design/ACCOUNT-IDENTITY-REVIEW.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ACCOUNT-003 — Separate account identity layers

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

Account id, login identity and friend code are separate; only friend code and display name are ever disclosed, and only to the owner's chosen counterparts

**Failure behavior:** The requirement fails if the account id, login identity or any other identity value is disclosed, or disclosure reaches anyone beyond the owner's chosen counterparts.

**AC-ACCOUNT-003:** Given an account has a login identity, account id, friend code and display name; when identity is disclosed during an account or social operation; then the three identity layers remain separate and only the friend code and display name reach the owner's chosen counterparts.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-ACCOUNT-ORIGIN, DEC-PRIVACY-LEGAL, DEC-SOCIAL.

**Elaboration:** [ACCOUNT-IDENTITY-REVIEW.md](../design/ACCOUNT-IDENTITY-REVIEW.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ACCOUNT-004 — Private rotatable friend codes

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

Friend codes are random, non-sequential, checksummed, rotatable, and use a Kohbor-only format

**Failure behavior:** A sequential, unchecked, non-rotatable or non-Kohbor friend code fails validation.

**AC-ACCOUNT-004:** Given an account is issued a friend code; when the code is inspected and then rotated; then the code is random, non-sequential, checksummed, rotatable and uses a Kohbor-only format.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-ACCOUNT-ORIGIN, DEC-NAME, DEC-SOCIAL.

**Elaboration:** [ACCOUNT-IDENTITY-REVIEW.md](../design/ACCOUNT-IDENTITY-REVIEW.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ACCOUNT-005 — Protected Kohbor sessions and cache clearing

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-001, PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

Sessions are Kohbor-issued, short-lived, rotating, device-bound and stored under OS user-scoped protection; sign-out and switch erase all cached account data

**Failure behavior:** A session that is not Kohbor-issued, short-lived, rotating, device-bound and OS user-protected fails; cached account data remaining after sign-out or switch also fails.

**AC-ACCOUNT-005:** Given a user has a Kohbor account session and cached account data; when the session is renewed, then the user signs out or switches accounts; then the session is short-lived, rotating, device-bound and stored under OS user-scoped protection, and all cached account data is erased.

**Verification:** synthetic, native; procedures TP-01, TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-ACCOUNT-IDENTITY, DEC-ALPHA-AUTH-DEVIATION, DEC-ACCOUNT-ORIGIN, DEC-PRIVACY-LEGAL.

**Elaboration:** [ACCOUNT-IDENTITY-REVIEW.md](../design/ACCOUNT-IDENTITY-REVIEW.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ACCOUNT-006 — Offline account independence

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-001, PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

With no network or no account service, startup and every local function are unaffected; the account surface reports offline or unavailable without prompting

**Failure behavior:** Network or account-service unavailability must not interrupt startup or any local function, or prompt the user.

**AC-ACCOUNT-006:** Given the device has no network or the account service is unavailable; when the application starts and the user uses local functions; then startup and every local function are unaffected and the account surface reports offline or unavailable without prompting.

**Verification:** synthetic, native; procedures TP-01, UAT-01; status NOT_RUN.

**Elaboration:** [ACCOUNT-IDENTITY-REVIEW.md](../design/ACCOUNT-IDENTITY-REVIEW.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ACCOUNT-007 — Product-scoped token authority

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

Tokens and tickets carry a Kohbor issuer and a single audience; a token from any other product or audience is rejected

**Failure behavior:** A token or ticket with an issuer other than Kohbor, or with an audience other than its single authorized audience, is rejected.

**AC-ACCOUNT-007:** Given a token or ticket is presented for account or relay authority; when its issuer and audience are checked; then only a Kohbor issuer with one audience is accepted and tokens from another product or audience are rejected.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-ALPHA-AUTH-DEVIATION, DEC-ACCOUNT-ORIGIN, DEC-NAME.

**Elaboration:** [ACCOUNT-IDENTITY-REVIEW.md](../design/ACCOUNT-IDENTITY-REVIEW.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ACCOUNT-008 — In-app account export and deletion

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

The user can export and delete the account from the app; deletion removes every server record and local cache. The only record kept is one deletion-ledger entry outside the database, holding the keyed login-identity hash and the deletion time, used only to repeat the deletion after a backup restore and purged 30 days after the deletion, never before every backup taken before the deletion has been rotated out

**Failure behavior:** Export or deletion unavailable in the app, any server record or local cache remaining after deletion, a ledger entry holding anything beyond the keyed login-identity hash and deletion time, a ledger stored inside the restored database, a replay that deletes an account created after the recorded deletion, or an entry kept past 30 days once no earlier backup remains, or purged while one does, fails.

**AC-ACCOUNT-008:** Given a user has an account and can access its controls in the app; when the user exports and then deletes the account; then the account can be exported; deletion removes every server record and local cache and writes one ledger entry with only the keyed login-identity hash and deletion time; after a backup restore, replay removes the restored account but not a newer account with the same login hash; the entry is purged after 30 days once no earlier backup remains.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-DELETION-LEDGER, DEC-ACCOUNT-ORIGIN, DEC-PRIVACY-LEGAL, DEC-RETENTION.

**Elaboration:** [ACCOUNT-DATABASE-DESIGN.md](../design/ACCOUNT-DATABASE-DESIGN.md), [ACCOUNT-IDENTITY-REVIEW.md](../design/ACCOUNT-IDENTITY-REVIEW.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ACCOUNT-009 — No unapproved platform identity binding

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

No platform identity is bound to an account unless DEC-STEAM-BINDING is decided; when bound it is proven by provider verification plus a local match and is never a login, recovery credential, search key or displayed value

**Failure behavior:** A platform identity bound before DEC-STEAM-BINDING is decided, without both proofs, or used as a login, recovery credential, search key or displayed value fails.

**AC-ACCOUNT-009:** Given DEC-STEAM-BINDING is undecided or a binding is enabled under a decided record; when a platform identity is considered for an account; then none is bound while undecided; any bound identity has provider verification and a local match and is not a login, recovery credential, search key or displayed value.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-ACCOUNT-ORIGIN, DEC-PRIVACY-LEGAL, DEC-STEAM-BINDING.

**Elaboration:** [ACCOUNT-IDENTITY-REVIEW.md](../design/ACCOUNT-IDENTITY-REVIEW.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ACCOUNT-010 — No cross-product account sharing

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

No account, identifier, key, secret or endpoint is shared with or imported from another product; a cross-product link requires its own decision and consent

**Failure behavior:** Any account, identifier, key, secret or endpoint shared with or imported from another product, or any cross-product link without its own decision and consent, fails.

**AC-ACCOUNT-010:** Given a Kohbor account and another product account exist or a link is proposed; when account data and trust material are handled or the cross-product link is attempted; then nothing is shared with or imported from the other product and a link proceeds only after its own decision and consent.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-ACCOUNT-LINK, DEC-PRIVACY-LEGAL.

**Elaboration:** [ACCOUNT-IDENTITY-REVIEW.md](../design/ACCOUNT-IDENTITY-REVIEW.md), [DECISION-LOG.md](../governance/DECISION-LOG.md), [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### SOCIAL

#### SOCIAL-001 — Optional account boundary

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

Friends/presence/targeted invitations shall require private server-authorized account actions without gating guest rooms. The account is located only by an exact server-minted friend code; denial reasons are indistinguishable.

**Failure behavior:** No public social directory or implicit platform-friend import. A lookup must use an exact server-minted friend code; denial reasons for unknown code, blocked pair and requests-off outcomes are indistinguishable.

**AC-SOCIAL-001:** Given a signed-out user opens core or joins a guest room, or a signed-in user submits a friend lookup; when account checks and exact friend-code lookups run; then core and permitted guest flow remain available; account-only social actions are unavailable to signed-out users; lookup locates an account only by an exact server-minted friend code, and unknown-code, blocked-pair and requests-off denials are indistinguishable.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-SOCIAL.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-003](../diagrams/source/DGM-003-deployment.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### SOCIAL-002 — Friendship is not sharing consent

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

Mutual friendship and optional presence shall not automatically share gameplay stats.

**Failure behavior:** Presence means app online only; avoid public last-seen/history.

**AC-SOCIAL-002:** Given two users become friends and presence is enabled; when a targeted invitation arrives; then a new checked peer-sharing action is still required before joining.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-SOCIAL.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### SOCIAL-003 — Transactional invitation authority

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

Social joins shall use reviewed registered-relay proof and one-use expiring authority with transaction ordering.

**Failure behavior:** Tokens/account IDs do not leak to a relay; production awaits full private design.

**AC-SOCIAL-003:** Given a block/revoke races an invitation or admission; when authorization resolves; then the frozen server transaction order rejects stale authority; no auto guest fallback occurs.

**Verification:** synthetic, native; procedures TP-10, UAT-10; status NOT_RUN.

**Open dependencies:** DEC-SOCIAL.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Diagram views:** [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### OBS

#### OBS-001 — Explicit local media session

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M4b · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-011 · **Primary design:** CMP-RECORDER · **Design components:** CMP-RECORDER

OBS control shall be authenticated loopback WebSocket v5 and user-started with no implicit mic.

**Failure behavior:** Credentials remain local; no cloud/stream upload.

**AC-OBS-001:** Given OBS is installed but recording has not been requested; when the desktop app starts or enters a match; then no recording/replay/microphone starts automatically.

**Verification:** synthetic, native, live; procedures TP-11, UAT-11; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [NFR.md](../quality/NFR.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-003](../diagrams/source/DGM-003-deployment.mmd), [DGM-016](../diagrams/source/DGM-016-recording-sequence.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### OBS-002 — Observe before audio suppression

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M4b · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-011 · **Primary design:** CMP-RECORDER · **Design components:** CMP-RECORDER, CMP-MEDIA

Highlight planning shall consume admitted session-bound events before ordinary announcer priority filtering.

**Failure behavior:** Wrong-owner/stale/out-of-session events cannot qualify.

**AC-OBS-002:** Given two admitted events share a tick and only one is voiced; when highlight eligibility is evaluated; then the quiet/suppressed eligible event can still schedule a clip.

**Verification:** synthetic, native, live; procedures TP-11, UAT-11; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [NFR.md](../quality/NFR.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json)

**Diagram views:** [DGM-006](../diagrams/source/DGM-006-logical-erd.mmd), [DGM-016](../diagrams/source/DGM-016-recording-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### OBS-003 — Measured clip coverage

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M4b · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-011 · **Primary design:** CMP-RECORDER · **Design components:** CMP-RECORDER

A requested event clip shall target 30s pre-roll and 15s post-roll using actual buffer/save/trim timestamps.

**Failure behavior:** Saving the entire OBS buffer is not proof of exact trimming.

**AC-OBS-003:** Given the buffer cannot cover the full requested interval; when a clip is saved and indexed; then actual coverage and partial reason are reported; full coverage is not fabricated.

**Verification:** synthetic, native, live; procedures TP-11, UAT-11; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [NFR.md](../quality/NFR.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json)

**Diagram views:** [DGM-006](../diagrams/source/DGM-006-logical-erd.mmd), [DGM-016](../diagrams/source/DGM-016-recording-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### OBS-004 — Recorder failure isolation

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M4b · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-011 · **Primary design:** CMP-RECORDER · **Design components:** CMP-RECORDER

OBS failure, auth loss, disk pressure or restart shall degrade recording independently.

**Failure behavior:** Bound pending saves; no blocking trim on the critical path.

**AC-OBS-004:** Given OBS stops while valid telemetry/alerts run; when recording operations fail; then status shows recorder unavailable while local alerts keep functioning.

**Verification:** synthetic, native, live; procedures TP-11, UAT-11; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [NFR.md](../quality/NFR.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json)

**Diagram views:** [DGM-003](../diagrams/source/DGM-003-deployment.mmd), [DGM-016](../diagrams/source/DGM-016-recording-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### OBS-005 — Recording-mode evidence

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M4b · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-011 · **Primary design:** CMP-RECORDER · **Design components:** CMP-RECORDER

Recording acceptance shall measure OBS separately and combined while retaining app budgets, latency and FPS gates.

**Failure behavior:** No assertion that the external recorder belongs to the app Job Object.

**AC-OBS-005:** Given recording is active during a validation profile; when CPU/RAM/GPU/FPS and alert timings are measured; then receipts separate app processes from OBS and report combined totals without inventing a combined cap.

**Verification:** synthetic, native, live; procedures TP-11, UAT-11; status NOT_RUN.

**Open dependencies:** DEC-OBS-BUDGET.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [NFR.md](../quality/NFR.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json)

**Diagram views:** [DGM-003](../diagrams/source/DGM-003-deployment.mmd), [DGM-016](../diagrams/source/DGM-016-recording-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### UPDATE

#### UPDATE-001 — Trusted updater authority

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M8 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-012 · **Primary design:** CMP-UPDATER · **Design components:** CMP-UPDATER

The native updater shall validate signed manifests/artifacts from trusted endpoints independent of account.

**Failure behavior:** Network failure never permits unsigned installation.

**AC-UPDATE-001:** Given renderer input attempts an arbitrary update URL or signature is invalid; when update verification runs; then installation is refused and valid local core remains usable.

**Verification:** synthetic, native; procedures TP-12, UAT-12; status NOT_RUN.

**Elaboration:** [RUNBOOK.md](../operations/RUNBOOK.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-018](../diagrams/source/DGM-018-release-update.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UPDATE-002 — Safe installation timing

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M8 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-012 · **Primary design:** CMP-UPDATER · **Design components:** CMP-UPDATER

Update installation shall wait for a safe gameplay/media state.

**Failure behavior:** An update prompt cannot force focus or local-core lock.

**AC-UPDATE-002:** Given a verified candidate is available during a match; when installation is requested; then it is deferred/staged without interrupting active gameplay.

**Verification:** synthetic, native; procedures TP-12, UAT-12; status NOT_RUN.

**Elaboration:** [RUNBOOK.md](../operations/RUNBOOK.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Diagram views:** [DGM-018](../diagrams/source/DGM-018-release-update.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UPDATE-003 — Promote identical signed bytes

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M8 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-012 · **Primary design:** CMP-UPDATER · **Design components:** CMP-UPDATER

Stable promotion shall reuse the exact approved signed candidate artifact.

**Failure behavior:** Tagging/normal commit alone does not publish; failed candidate cannot mutate stable.

**AC-UPDATE-003:** Given a candidate is ready for stable promotion; when release preparation runs; then hash and signature match the approved candidate and no rebuild/resign occurs.

**Verification:** synthetic, native; procedures TP-12, UAT-12; status NOT_RUN.

**Elaboration:** [RUNBOOK.md](../operations/RUNBOOK.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Diagram views:** [DGM-018](../diagrams/source/DGM-018-release-update.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UPDATE-004 — Verified recovery route

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M8 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-012 · **Primary design:** CMP-UPDATER · **Design components:** CMP-UPDATER

Release rollback/repair shall respect version and data-migration compatibility.

**Failure behavior:** Do not assume a manifest downgrade will be accepted.

**AC-UPDATE-004:** Given a failed update needs recovery; when the documented rollback route is tested; then a known approved compatible artifact or new higher emergency version restores operation.

**Verification:** synthetic, native; procedures TP-12, UAT-12; status NOT_RUN.

**Elaboration:** [RUNBOOK.md](../operations/RUNBOOK.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Diagram views:** [DGM-018](../diagrams/source/DGM-018-release-update.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UPDATE-005 — Kohbor news from one fixed public file

**Scope:** mvp · **Priority:** P1 · **Target milestone:** M3 · **Basis:** owner_decision / DEC-OVERVIEW

**Product needs:** PN-012 · **Primary design:** CMP-UPDATER · **Design components:** CMP-UPDATER, CMP-READMODEL

The native host shall fetch only https://raw.githubusercontent.com/Freshair129/kohbor-updates/main/news.json, once at app start with one retry and otherwise only when the user presses retry, over HTTPS with no redirects, no cookies, a 10 s deadline and a 256 KiB cap, sending no account, Steam, match or other user data. News is on by default; the first view of the news column and the Settings page disclose that GitHub sees the IP address and the time, and Settings provides a persisted off switch that stops every news request. The file is validated against contracts/news-feed.schema.json and accepted or rejected whole, skipping posts of an unknown type, and the last valid file is cached with its fetch time. Images come only from images/ paths of the same repository, at most 512 KiB, SHA-256 checked, and reach the renderer as data: URLs. A link opens in the system browser only from the validated feed by post and link index, and not during a match. Posts are Kohbor-written; patch notes link to Valve and copy no Valve text. Logs carry outcome codes only.

**Failure behavior:** A network error, HTTP error, oversized or invalid file keeps the cached feed, shown as offline with its fetch time, or shows unavailable when nothing is cached. A missing or tampered image is left out. With news off nothing is fetched and no post is shown.

**AC-UPDATE-005:** Given a scripted transport returning a valid feed, a network failure then success, HTTP errors, an invalid and an oversized body, an image with a wrong hash, and news switched off; when the host starts, retries and the renderer reads the news view; then only the fixed feed path and same-repository image paths are requested, at most two attempts per start; a valid feed is cached and shown; failures keep the cache as offline with its fetch time or show unavailable; an invalid file never replaces the cache; the tampered image is not served; with news off nothing is requested or shown; links resolve only by post id and index; and no test touches the network.

**Verification:** synthetic, native; procedures TP-12, UAT-12; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [news-feed.schema.json](../../contracts/news-feed.schema.json)

**Implementation freedom:** Choose the cache layout and polling within the fixed origin, limits and disclosure. Any other origin needs its own decision.

### PREMIUM

#### PREMIUM-001 — Native trusted capability lease

**Scope:** deferred · **Priority:** later · **Target milestone:** M7 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-012 · **Primary design:** CMP-ENTITLEMENT · **Design components:** CMP-ENTITLEMENT

Premium presentation shall require native-verified server authority; planned lease lifetime 7 days, refresh 24h and max 2 installations.

**Failure behavior:** Production issuing/key/RLS/device design is deferred.

**AC-PREMIUM-001:** Given a renderer/local fixture claims premium without a valid grant; when capability admission runs; then base remains active and protected details are not enabled.

**Verification:** synthetic, native; procedures TP-12, UAT-12; status NOT_RUN.

**Open dependencies:** DEC-PREMIUM.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PREMIUM-002 — Quiet loss and regain

**Scope:** deferred · **Priority:** later · **Target milestone:** M7 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-012 · **Primary design:** CMP-ENTITLEMENT · **Design components:** CMP-ENTITLEMENT, CMP-READMODEL

Expired/revoked/unverifiable premium authority shall reduce presentation to base without disturbing the live map.

**Failure behavior:** Offline with a still-valid signed lease retains capabilities.

**AC-PREMIUM-002:** Given a selected premium detail is open; when authority is lost then later regained; then detail/late responses clear quietly; map/feed remain; regain does not reopen selection.

**Verification:** synthetic, native; procedures TP-12, UAT-12; status NOT_RUN.

**Open dependencies:** DEC-PREMIUM.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### PREMIUM-003 — No premium data or safety expansion

**Scope:** deferred · **Priority:** later · **Target milestone:** M7 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-012 · **Primary design:** CMP-ENTITLEMENT · **Design components:** CMP-ENTITLEMENT

Premium shall never gate safety/core or enlarge source/consent permissions.

**Failure behavior:** A paid tier is not evidence of game data availability.

**AC-PREMIUM-003:** Given base user is in a valid match and a premium user lacks peer consent; when features evaluate access; then base alerts/GSI/overlay work and premium cannot access unconsented or unavailable fields.

**Verification:** synthetic, native; procedures TP-12, UAT-12; status NOT_RUN.

**Open dependencies:** DEC-PREMIUM.

**Elaboration:** [UX-SPEC.md](../design/UX-SPEC.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Contracts:** [decisions.json](../../contracts/decisions.json)

**Diagram views:** [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### QUALITY

#### QUALITY-001 — Acoustic warning latency

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-003 · **Primary design:** CMP-PERFORMANCE · **Design components:** CMP-PERFORMANCE, CMP-VISION, CMP-RISK, CMP-ALERT

Alert latency shall have p50 ≤250ms and every measured sample ≤300ms over at least 300 samples per scenario/profile.

**Failure behavior:** Do not move T0 after frame-ready or replace hard maximum with p99.

**AC-QUALITY-001:** Given an instrumented scenario records acquisition-due or GSI-arrival T0 and loopback audible T3; when latency acceptance is evaluated; then T3−T0 includes scheduling/acquisition/inference waits and meets both limits.

**Verification:** native, live, acoustic; procedures TP-13, TP-03, UAT-03; status NOT_RUN.

**Elaboration:** [NFR.md](../quality/NFR.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Diagram views:** [DGM-003](../diagrams/source/DGM-003-deployment.mmd), [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### QUALITY-002 — Whole-app CPU and memory

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE; DEC-RESOURCE-BUDGET

**Product needs:** PN-005 · **Primary design:** CMP-PERFORMANCE · **Design components:** CMP-PERFORMANCE

Every 1s normalized app-group CPU sample shall be ≤10%. Aggregate peak full Working Set and, when the app group uses the GPU, peak dedicated GPU memory (VRAM) shall be measured and reported with raw receipts; they have no pass/fail limit until the product owner sets one from the first measured baseline. GPU use is allowed. A value that was not measured is unknown, never zero or within budget.

**Failure behavior:** No excluding model/shared pages or substituting private/PSS/forced trim. A RAM or VRAM value that was not measured is unknown, never zero or within budget. No RAM or VRAM limit is applied or claimed until the owner sets one.

**AC-QUALITY-002:** Given host, WebView, sidecars and active external model processes are measured; when warmup and idle/live/voice-alert profiles run; then all included process samples satisfy the fixed CPU limit with raw receipts; app-group full Working Set and, when the GPU is used, VRAM are measured per process and reported with raw receipts for the idle, live and voice-alert profiles, as the baseline from which the owner sets a limit.

**Verification:** native, live, benchmark; procedures TP-13, UAT-05; status NOT_RUN.

**Elaboration:** [NFR.md](../quality/NFR.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Diagram views:** [DGM-003](../diagrams/source/DGM-003-deployment.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### QUALITY-003 — FPS preservation

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-PERFORMANCE · **Design components:** CMP-PERFORMANCE

Game FPS loss shall be ≤3% in each of at least 3 overlay-off/on pairs of 60s under matched device/segment/settings.

**Failure behavior:** A pooled average cannot hide a failing pair.

**AC-QUALITY-003:** Given paired runs use identical validated conditions; when FPS impact is calculated per pair; then every pair passes and evidence includes active recording where applicable.

**Verification:** live, benchmark; procedures TP-13, UAT-05; status NOT_RUN.

**Elaboration:** [NFR.md](../quality/NFR.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Diagram views:** [DGM-003](../diagrams/source/DGM-003-deployment.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### QUALITY-004 — Vision quality per class

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-002 · **Primary design:** CMP-PERFORMANCE · **Design components:** CMP-PERFORMANCE, CMP-VISION

Presence precision/recall shall be ≥95%/90% and identity precision/recall ≥95%/85% for each certified class.

**Failure behavior:** No train/holdout leakage or pooled substitution.

**AC-QUALITY-004:** Given at least 50 episodes/class in at least 5 held-out matches and 20 whole held-out matches exist; when independent quality evaluation runs; then per-class results, 95% confidence intervals and disagreement are reported.

**Verification:** benchmark, live; procedures TP-13, TP-02, UAT-02; status NOT_RUN.

**Elaboration:** [NFR.md](../quality/NFR.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### QUALITY-005 — Warning quality

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-003 · **Primary design:** CMP-PERFORMANCE · **Design components:** CMP-PERFORMANCE, CMP-RISK, CMP-ALERT

High-danger audible false alerts shall be ≤1 per 30 minutes, actionable recall ≥90% and false-safe calls 0.

**Failure behavior:** Death alone is not ground truth and no coverage cannot be safe.

**AC-QUALITY-005:** Given independently labeled threat episodes are fixed before tuning; when each sensitivity profile is evaluated after dedup; then results meet each threshold with attribution/evidence.

**Verification:** benchmark, live; procedures TP-13, TP-03, UAT-03; status NOT_RUN.

**Elaboration:** [NFR.md](../quality/NFR.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### QUALITY-006 — Voice quality and speed

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-006 · **Primary design:** CMP-PERFORMANCE · **Design components:** CMP-PERFORMANCE, CMP-VOICE

Warm accepted voice turns shall begin audible answer within 2s each and complete ≥95% per language, with cold preparation ≤10s.

**Failure behavior:** Count failures/timeouts/rejects; synthetic scorer tests do not certify a model.

**AC-QUALITY-006:** Given at least 100 held-out utterances per language cover quiet/game-audio and separate speakers/sessions; when local STT/answer is evaluated; then English WER ≤15%, Thai CER ≤20%, intent/critical-slot accuracy ≥90% and latency/completion pass.

**Verification:** benchmark, native, acoustic; procedures TP-13, TP-06, UAT-06; status NOT_RUN.

**Open dependencies:** DEC-MODELS.

**Elaboration:** [NFR.md](../quality/NFR.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Diagram views:** [DGM-009](../diagrams/source/DGM-009-voice-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### QUALITY-007 — Preemption under overlap

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M5 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-006 · **Primary design:** CMP-PERFORMANCE · **Design components:** CMP-PERFORMANCE, CMP-VOICE, CMP-MEDIA, CMP-ALERT

All 100 overlap trials shall cancel stale voice/advice while retaining the critical acoustic deadline.

**Failure behavior:** Report failed/cancelled trials without pruning the denominator.

**AC-QUALITY-007:** Given each voice lifecycle stage overlaps critical admission; when priority/cancellation trials execute; then every trial preempts obsolete output and meets the alert deadline.

**Verification:** native, acoustic; procedures TP-13, TP-03, TP-04, TP-06, UAT-06; status NOT_RUN.

**Open dependencies:** DEC-EVENT-AUDIO.

**Elaboration:** [NFR.md](../quality/NFR.md), [EVENT-AUDIO-SPEC.md](../design/EVENT-AUDIO-SPEC.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json), [audio-outcome.schema.json](../../contracts/audio-outcome.schema.json)

**Diagram views:** [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-009](../diagrams/source/DGM-009-voice-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### QUALITY-008 — Evidence-gated certification

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M8 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-012 · **Primary design:** CMP-PERFORMANCE · **Design components:** CMP-PERFORMANCE, CMP-STARTUP, CMP-UPDATER

Every required OS/mode/geometry cell shall have at least 10 live sessions before certification.

**Failure behavior:** A unit test, browser screenshot or commit does not substitute for the missing evidence.

**AC-QUALITY-008:** Given a required cell lacks device or acoustic evidence; when release readiness is assessed; then the cell is SKIP/INDETERMINATE and the mandatory gate remains unsatisfied.

**Verification:** native, live; procedures TP-13, UAT-12; status NOT_RUN.

**Open dependencies:** DEC-NATIVE-EVIDENCE.

**Elaboration:** [NFR.md](../quality/NFR.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### LAB

#### LAB-001 — Opt-in local minimap test recording

**Scope:** extension · **Priority:** P1 · **Target milestone:** M3 · **Basis:** owner_instruction / Owner instruction recorded 2026-11-24 in DECISION-LOG.md

**Product needs:** PN-002 · **Primary design:** CMP-RECORDER · **Design components:** CMP-RECORDER, CMP-VISION

With the Lab-only consent switch “บันทึกภาพมินิแมปเพื่อทดสอบ (Lab)” off by default, Kohbor shall record only fresh minimap crops from the existing capture path while the match is live and crop is confirmed, at 1–5 fps (default 2 fps) as lossless PNGs on one bounded background worker. Each JSONL frame row shall contain the frame file name, game clock, session epoch, phase, capture age, crop rectangle, capture size and stale flag; each match manifest shall contain the install-keyed match hash using the existing match-ID HMAC when known or an explicitly marked session-epoch fallback (never raw match ID), game build, resolution, DPI, capture profile, crop confirmation, read-only Dota minimap hero-icon/size settings, team side and ally/enemy hero IDs only, with unknown values null. Store only minimap crops under app data `lab/minimap-recordings/<match hash>/`; cap total storage at a configurable 5 GB default and frames per match at 150 MB, stop recording for the match and notify the player at a cap, count non-blocking full-queue drops in the manifest, exclude player names, account IDs, chat, audio and full-screen frames, never automatically delete recordings, and delete only on the player's explicit per-match action.

**Failure behavior:** Frames outside a fresh live confirmed-crop admission are ignored; a full queue drops without waiting and increments the match drop counter; a capacity hit stops recording for that match and reports the reason; deleting one match never removes another; unknown data stays null.

**AC-LAB-001:** Given Lab consent off and on, synthetic live/fresh/confirmed and ineligible frames, a full bounded queue, and total/per-match cap boundaries; when the player enables recording and eligible minimap frames arrive through the existing capture path; then only eligible minimap crops are written as PNG on the background worker with the specified JSONL and privacy-safe manifest, ineligible frames are ignored, queue overflow is non-blocking and counted, capacity stops the match with a notice, and only the chosen recording is deleted after user confirmation.

**Verification:** synthetic; procedures TP-02, TP-05; status PASS.

**Elaboration:** [KOHBOR-LAB.md](../design/KOHBOR-LAB.md), [DECISION-LOG.md](../governance/DECISION-LOG.md)

**Implementation freedom:** Use only the already admitted minimap crop and existing dependencies; do not add game automation, cloud transfer, labels, new capture sources or automatic retention cleanup.

<!-- END GENERATED REQUIREMENTS -->
