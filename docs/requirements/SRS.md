---
title: "Software Requirements Specification"
doc_id: "SRS"
status: "draft"
version: "0.3.2"
updated: "2026-11-10"
owner: "Product Owner"
approval_status: "not_approved"
---

# Software Requirements Specification (SRS)

## 1. Introduction

### 1.1 Purpose

This SRS defines observable behavior, data boundaries, quality attributes and constraints for **Kohbor / เกาะเบาะ**, a new Windows desktop companion for Dota 2. It guides engineering, verification and acceptance.

The [PRD](../product/PRD.md) defines needs, outcomes, scope and priorities; this SRS translates them into verifiable obligations. The owner selected the product name. The builder defines its visual identity and unresolved stable technical identifiers without changing functionality, permissions or acceptance.

### 1.2 Scope and document status

The full MVP covers local telemetry, admitted visible-screen observations, deterministic warnings, event media, control window and passive overlay, manual advice, local voice conversation, local history/memory, bounded summaries and explicit local highlights recording. Guest team-status sharing and private social coordination are extensions. Premium authority and advanced services remain deferred unless explicitly scheduled.

The first demonstration is a limited delivery milestone, not completion of the full MVP. Action-damage analysis remains subject to its recorded scope/data decision. Game injection, hidden enemy data, automatic gameplay actions, cloud voice/memory processing and inherited product compatibility are outside this baseline.

Version 0.3.2 is a reviewable draft. The owner-selected name is recorded; the requirements baseline, implementation and native/live acceptance have not been approved. Its organization follows conventional requirements-engineering practice; no formal standards-conformance certification is asserted.

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

The native host owns admission, critical decisions, audio, cancellation, protected operations and resource accounting. The control window presents configuration and inspection; the transparent overlay presents passive gameplay information.

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

The register preserves **85 stable IDs**, with scope, priority, obligation, failure behavior, acceptance scenario, evidence class, contracts and dependencies. Moving documents does not change IDs.

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

The [diagram catalog](../diagrams/README.md) supplies controlled architectural, data and behavioral views. Its mappings appear in the RTM and detailed rows below. The figures supplement the same baseline; their arrows and short guards cannot broaden a permission or replace a complete textual requirement.

## 8. Atomic software requirements

<!-- BEGIN GENERATED REQUIREMENTS -->

This controlled section and the structured registry represent the same requirements baseline. Edit an atomic row through the registry, regenerate, and review the complete SRS context for consistency. The registry is an authoring mechanism, not a higher authority. Acceptance scenarios are test designs; new-application evidence is NOT_RUN until actual execution is recorded.

### CORE

#### CORE-001 — Account-independent local startup

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-001 · **Primary design:** CMP-STARTUP · **Design components:** CMP-STARTUP

The local core shall start without account, network or premium authority.

**Failure behavior:** Do not lock gameplay functionality behind sign-in or an entitlement check.

**AC-CORE-001:** Given a cold installed application with no account and no network; when the player starts the application; then local setup, permitted intake, overlay and available local functions are usable; account services show unavailable.

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

**Failure behavior:** Never allow an old account response to populate the new account.

**AC-CORE-003:** Given a local match is active with an optional signed-in account; when the account is revoked or switched; then protected account operations stop and late account results are discarded; valid local core continues.

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

Own-player conclusions shall require supported game scope, actual playing activity and verified active local ownership.

**Failure behavior:** Keep source unavailable; selected-unit data is not proof of ownership.

**AC-DATA-003:** Given a valid-looking payload belongs to another selected unit or unverified player; when source admission is evaluated; then the payload cannot supply own-player or peer vitals.

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

Critical warnings shall be decided and admitted in native deterministic code without waiting for cloud, model, renderer or relay.

**Failure behavior:** No synchronous network, disk unpacking or inference in critical admission.

**AC-ALERT-001:** Given network is down and a renderer/model task is stalled; when an admitted actionable critical event occurs; then the local alert path continues and meets its acoustic gate.

**Verification:** synthetic, native, live; procedures TP-03, UAT-03; status NOT_RUN.

**Elaboration:** [SDD.md](../design/SDD.md), [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json), [risk-golden.json](../../fixtures/risk-golden.json)

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

**Elaboration:** [SDD.md](../design/SDD.md), [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json), [risk-golden.json](../../fixtures/risk-golden.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### ALERT-007 — Critical media readiness

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-003 · **Primary design:** CMP-ALERT · **Design components:** CMP-ALERT, CMP-RISK, CMP-MEDIA

Critical audio readiness shall require valid mapped critical media before the demo warning gate passes.

**Failure behavior:** Do not substitute TTS, an unrelated clip or silently declare visual-only readiness.

**AC-ALERT-007:** Given gank/danger/revision media is missing or invalid; when critical readiness is assessed; then the product exposes not-ready and the acoustic gate cannot pass.

**Verification:** synthetic, native, live; procedures TP-03, UAT-03; status NOT_RUN.

**Open dependencies:** DEC-CRITICAL-MEDIA.

**Elaboration:** [SDD.md](../design/SDD.md), [FEATURE-CATALOG.md](../product/FEATURE-CATALOG.md), [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md)

**Contracts:** [event.schema.json](../../contracts/event.schema.json), [risk-golden.json](../../fixtures/risk-golden.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-014](../diagrams/source/DGM-014-alert-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### MEDIA

#### MEDIA-001 — Single event taxonomy

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

One versioned catalog shall own event IDs, labels, detectors, bounded tunables and ordinary ranks.

**Failure behavior:** Do not hand-edit generated downstream copies.

**AC-MEDIA-001:** Given duplicate IDs, duplicate nonzero ranks or invalid tunables exist; when catalog validation/generation runs; then the catalog is rejected before generated runtime tables are published.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-002 — Ordinary event arbitration

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

Ordinary same-tick arbitration shall select the highest emitted ordinary rank before media lookup, separately from critical admission.

**Failure behavior:** No mapping-driven fallback. Baseline/reconnect must not invent first blood, kills, level or match-start events.

**AC-MEDIA-002:** Given one tick produces several admitted ordinary events; when the announcer chooses media; then the highest emitted ordinary rank is chosen; if its mapping is missing/invalid the slot stays silent without a lower-rank media fallback.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Open dependencies:** DEC-INTERFACES.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-003 — Respawn unvoiced

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

Respawn shall remain a detectable event without an automatic audio mapping.

**Failure behavior:** Do not treat every event ID as requiring a sound.

**AC-MEDIA-003:** Given an admitted own death transitions to respawn; when the detector publishes the transition; then history/state may reflect respawn but the announcer does not voice it.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-004 — Verified paired media

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

Event audio/visual playback shall originate from the active verified package revision.

**Failure behavior:** No loose-file/default-pack/generated-card/event-TTS fallback.

**AC-MEDIA-004:** Given mixed audio is corrupt while an original identity audio file is valid; when playback is requested; then the event pair is rejected and a local reason is recorded.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json)

**Diagram views:** [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd), [DGM-010](../diagrams/source/DGM-010-media-install-activation.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-005 — One audio owner

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA, CMP-ALERT, CMP-VOICE

Native audio shall own paired visual lifecycle and cancel both on stop/preemption/session fence.

**Failure behavior:** The renderer cannot independently select or replay event media.

**AC-MEDIA-005:** Given a clip has a declared visual tail; when a critical interruption or session change occurs; then obsolete audio and visual tails terminate under the same generation.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json)

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

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json)

**Diagram views:** [DGM-010](../diagrams/source/DGM-010-media-install-activation.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### MEDIA-007 — Atomic install versus activation

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M2 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-004 · **Primary design:** CMP-MEDIA · **Design components:** CMP-MEDIA

Installation and activation shall be separate idempotent transactions.

**Failure behavior:** A later activation error cannot misreport the successful install as active.

**AC-MEDIA-007:** Given a valid installation commits while a match/media lease prevents selection; when activation is requested; then the receipt distinguishes installed from activation deferred; the old active revision remains usable.

**Verification:** synthetic, native; procedures TP-04, UAT-04; status NOT_RUN.

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json)

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

**Elaboration:** [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md), [ICD.md](../design/ICD.md)

**Contracts:** [media-manifest.schema.json](../../contracts/media-manifest.schema.json)

**Diagram views:** [DGM-010](../diagrams/source/DGM-010-media-install-activation.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### UI

#### UI-001 — Independent control and overlay

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

The product shall separate control-window chrome from the transparent always-on-top click-through gameplay overlay.

**Failure behavior:** Native safe-area/focus behavior needs measured evidence.

**AC-UI-001:** Given the player changes control-window route; when gameplay remains active; then overlay visibility/focus behavior stays independent and does not cover protected game areas.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Open dependencies:** DEC-VISUALS.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Diagram views:** [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-002 — Stable live-field topology

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

The base live field shall preserve five enemy slots, own hero, four allied slots, central map and permanent right region.

**Failure behavior:** Use bounded subregion scrolling; do not shrink text to hide overflow.

**AC-UI-002:** Given frozen data and viewport while inspector/feed/tier changes; when layout transitions run; then map x/y/width/height remain within 1 CSS px and no page-level scroll is introduced.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Diagram views:** [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-003 — Neutral production start

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

Production shall begin without fixture roster, fake inventory or preselected hero.

**Failure behavior:** Fixture play/reset/seed controls cannot ship as production authority.

**AC-UI-003:** Given no admitted game state is available; when the live view opens; then waiting/unknown state is visible and inspector remains neutral.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

**Diagram views:** [DGM-017](../diagrams/source/DGM-017-user-flow.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### UI-004 — Compact unknown cards

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M3 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-READMODEL · **Design components:** CMP-READMODEL

Enemy and unknown ally cards shall omit unknown-only skill/item rows.

**Failure behavior:** Do not create four question-mark skill tiles or infer values from account/premium.

**AC-UI-004:** Given no admitted skills/items exist for a visible card; when the card renders; then it shows admitted portrait/visibility/age with unknown age as an em dash.

**Verification:** synthetic, native, live; procedures TP-05, UAT-05; status NOT_RUN.

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

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

**Elaboration:** [USE-CASES.md](../product/USE-CASES.md), [UX-SPEC.md](../design/UX-SPEC.md)

**Contracts:** [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json)

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

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [NFR.md](../quality/NFR.md), [SDD.md](../design/SDD.md)

**Contracts:** [error-catalog.json](../../contracts/error-catalog.json)

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

Peer networking shall be native-owned HTTPS with literal loopback HTTP only for local testing.

**Failure behavior:** No URL credentials/query/fragment, redirects or token logging.

**AC-PEER-007:** Given a renderer asks for arbitrary origin/path/redirect or member credentials; when transport authorization runs; then untrusted request is rejected and tokens stay native.

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

Peer stats shall remain transient within authorized same-room/match/team viewing.

**Failure behavior:** Friendship/premium/cloud consent cannot expand recipients.

**AC-PEER-009:** Given memory/history/advice attempts to consume a peer sample; when data routing is checked; then the route is denied and samples are not written to logs or AI.

**Verification:** synthetic, native; procedures TP-09, UAT-09; status NOT_RUN.

**Elaboration:** [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md), [ICD.md](../design/ICD.md)

**Contracts:** [peer-sample.schema.json](../../contracts/peer-sample.schema.json), [peer-sync.schema.json](../../contracts/peer-sync.schema.json)

**Diagram views:** [DGM-001](../diagrams/source/DGM-001-system-context.mmd), [DGM-002](../diagrams/source/DGM-002-containers.mmd), [DGM-004](../diagrams/source/DGM-004-local-data-flow.mmd), [DGM-005](../diagrams/source/DGM-005-external-data-flow.mmd), [DGM-011](../diagrams/source/DGM-011-peer-sequence.mmd), [DGM-012](../diagrams/source/DGM-012-peer-lifecycle.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

### SOCIAL

#### SOCIAL-001 — Optional account boundary

**Scope:** extension · **Priority:** extension · **Target milestone:** M6 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-010 · **Primary design:** CMP-SOCIAL · **Design components:** CMP-SOCIAL

Friends/presence/targeted invitations shall require private server-authorized account actions without gating guest rooms.

**Failure behavior:** No public social directory or implicit platform-friend import.

**AC-SOCIAL-001:** Given a signed-out user opens core or joins a guest room; when account checks run; then core and permitted guest flow remain available; account-only social actions are unavailable.

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

**Elaboration:** [NFR.md](../quality/NFR.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

**Diagram views:** [DGM-003](../diagrams/source/DGM-003-deployment.mmd), [DGM-008](../diagrams/source/DGM-008-alert-sequence.mmd)

**Implementation freedom:** Choose internal implementation after freezing interfaces; do not alter the stated behavior, permissions or evidence thresholds.

#### QUALITY-002 — Whole-app CPU and memory

**Scope:** mvp · **Priority:** P0 · **Target milestone:** M1 · **Basis:** inherited_target / DEC-BASELINE

**Product needs:** PN-005 · **Primary design:** CMP-PERFORMANCE · **Design components:** CMP-PERFORMANCE

Every 1s normalized app-group CPU sample shall be ≤10% and aggregate peak full Working Set ≤900000000 bytes.

**Failure behavior:** No excluding model/shared pages or substituting private/PSS/forced trim.

**AC-QUALITY-002:** Given host, WebView, sidecars and active external model processes are measured; when warmup and idle/live/voice-alert profiles run; then all included process samples satisfy the fixed limits with raw receipts.

**Verification:** native, benchmark; procedures TP-13, UAT-05; status NOT_RUN.

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

**Elaboration:** [NFR.md](../quality/NFR.md)

**Contracts:** [evidence.schema.json](../../contracts/evidence.schema.json)

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

<!-- END GENERATED REQUIREMENTS -->
