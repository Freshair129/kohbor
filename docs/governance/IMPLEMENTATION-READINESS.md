---
title: "Kohbor Implementation Readiness"
doc_id: "IMPLEMENTATION-READINESS"
status: draft
version: "0.3.1"
updated: "2026-11-11"
owner_role: "Product owner and engineering lead; reviewers not assigned"
approval_status: "not_approved"
---

# Kohbor Implementation Readiness

## Current position

The owner selected **Kohbor / เกาะเบาะ** as the product name. This repository contains a substantive software-engineering specification and an implementation handoff. It does not contain a completed application. The name decision does not approve all requirements, resolve technical identities, or certify any runtime behavior.

The baseline contains 12 product needs, 85 software requirements, component/data/interface designs, verification and UAT procedures, schemas, synthetic examples and 19 controlled diagram views. This is a reviewable draft with explicit unfinished design work. The registered requirements are an inventory, not proof that every possible behavior has been fully specified.

## What can start

Read [PRD](../product/PRD.md), [SRS](../requirements/SRS.md), [Document Control](DOCUMENT-CONTROL.md), [Decision Log](DECISION-LOG.md), and [SDP](../delivery/SDP.md). Follow root [AGENTS.md](../../AGENTS.md) and [PROMPT.md](../../PROMPT.md) for execution. Use the selected name in [product-identity.json](../../contracts/product-identity.json); freeze its unresolved technical identifiers during M0 before packaging depends on them.

Start M0 feasibility work, requirement review, technical-identity definition and independent vertical slices. An unresolved decision blocks only its named dependencies. Keep optional or unqualified capabilities explicitly unavailable until their prerequisites exist. Do not silently invent a protocol, data source, model qualification, paid service or consent grant to make a slice appear complete.

## Remaining design work

| Area | Detail still required | Decision / governing document | Interim behavior |
| --- | --- | --- | --- |
| Technical identity | Stable app/package IDs, data directory, URI scheme, signing/hash domains and update origin policy | DEC-NAME; [Decision Log](DECISION-LOG.md) | Product name fixed; technical fields remain explicitly unresolved |
| Interfaces | Complete command/event payload catalog, transport framing, authentication, versioning, idempotency, size/rate bounds and error mapping for every supported operation | DEC-INTERFACES; [ICD](../design/ICD.md) | Draft schemas support bounded spikes; no claim of complete interoperability |
| Player/game read models | Complete admitted inventory/ability/cooldown shapes, ownership, freshness, unsupported-data behavior and patch-qualified catalogs | DEC-INTERFACES, DEC-GAME-DATA; [DDD](../design/DDD.md), [ICD](../design/ICD.md) | Unknown values stay unknown; no unobserved enemy state or invented readiness |
| Persistence and lifecycle | Physical persisted schemas, migrations, completion evidence, aggregate eligibility, quotas and retention/deletion behavior | DEC-SESSION, DEC-RETENTION; [DDD](../design/DDD.md) | Partial sessions remain partial; no destructive automatic cleanup or unsupported outcome claims |
| Media trust and playback readiness | Complete install/catalog contract, canonical signing bytes, algorithm/key distribution, verification vectors and authorized critical media | DEC-PACK-CRYPTO, DEC-CRITICAL-MEDIA; [Media Package Specification](../design/MEDIA-PACK-SPEC.md) | Untrusted packages inactive; missing critical assets reported not ready |
| Risk alerts | Complete coordinated-motion/disappearance admission, reappearance guards, timing/heading/ETA policy and positive/negative golden traces | DEC-RISK-DETAIL; [SDD](../design/SDD.md) | Reference scores informational; affected audible alerts unavailable until admission is frozen; no correction from coverage loss or time decay |
| Voice buffers | Exact PCM ownership, capture termination, bounded decoder lifetime, cancellation races and zeroization deadlines | DEC-VOICE-BUFFER; [SDD](../design/SDD.md), [Security and Privacy](../security/SECURITY-PRIVACY.md) | A capture-only spike may discard bytes and report STT unavailable |
| Visual design and native behavior | Review supplied design-system 1.0.0,248 tokens, 24 components and Live-field specimens; complete affected-route screens, rendered browser checks and native viewport/DPI/focus/safe-area evidence | DEC-VISUALS, DEC-NATIVE-EVIDENCE; [Design kit](../../design/README.md), [UX Specification](../design/UX-SPEC.md) | Authored values and static/DOM checks are available; pixel approval and rendered/native acceptance remain open |
| Model and resource qualification | Selected model artifacts/hashes, calibration and held-out evidence; application and external recording resource results | DEC-MODELS, DEC-OBS-BUDGET; [NFR](../quality/NFR.md) | Unqualified model capability unavailable; report app and recorder separately and combined |
| Optional services | Hosted social protocols, ownership and operations; commercial entitlement authority and revocation | DEC-SOCIAL, DEC-PREMIUM; [ICD](../design/ICD.md), [Runbook](../operations/RUNBOOK.md) | Local core independent; no production premium grant or implicit cloud permission |

The governing documents contain the exact obligations and thresholds. This table summarizes gaps; it does not override them or introduce extra scope.

## Evidence that exists and evidence still needed

[spec-validation.json](../../acceptance/spec-validation.json) records document/traceability consistency and synthetic contract checks. [diagram-validation.json](../../acceptance/diagram-validation.json) records diagram generation, source/output hashes and the documented visual inspection. These results do not establish application correctness.

Application test bindings, integration runs, Windows native execution, live matches, acoustic timing, resource/FPS measurements, installer/update behavior and owner UAT remain **NOT_RUN** until an actual implementation and receipts exist. Empty implementation/evidence references are intentional. Do not replace them with planned test names and label those tests passed.

Use [Test Plan](../quality/TEST-PLAN.md), [UAT Plan](../quality/UAT-PLAN.md), and [RTM](../requirements/RTM.csv) to bind each result to the exact code and artifact tested. Publishing this specification is not a release approval for the application.

## Supplied design kit

The [design kit](../../design/README.md) adds the original Nightglass visual direction, semantic/component tokens, an offline interactive board, static poster and [implementation handoff](../design/IMPLEMENTATION-HANDOFF.md). Token and bundle checks are static; [DOM logic checks](../../design/board/dom-validation.json) are synthetic. Actual browser rendering could not be completed in this environment, and Windows/native/live/acoustic/resource evidence is still NOT_RUN. Use the board scope matrix before assuming every planned acceptance journey exists in the fixture.
