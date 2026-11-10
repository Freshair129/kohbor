---
title: "Software Development Plan (SDP)"
doc_id: "SDP"
status: draft
version: "0.3.2"
updated: "2026-11-11"
authority: "DEC-BASELINE; see DOCUMENT-CONTROL"
owner_role: "Engineering lead; people not assigned"
approval_status: "not_approved"
---

# Software Development Plan (SDP)


## Delivery strategy

Use small vertical slices with source, contract and evidence ownership. Do not dispatch a separate agent or worktree per requirement. A reviewer may inspect shared code read-only. The integrator serializes shared contract, event-registry and generated-document writes.

| Milestone | Deliverable | Entry dependencies | Exit gate |
| --- | --- | --- | --- |
| M0 | Identity, scope, interface decisions, locked toolchain, feasibility plan | Read full baseline | Decision register assigned; no unresolved blocking choice disguised as code |
| M1 | Local startup, resource cap, GSI admission, honest read model | M0 core contracts | Cold offline/native startup; invalid/source/session tests |
| M2 | Verified pack cache, native audio, deterministic events and alert path | M1, authorized media | Preemption, corrupt-package and latency engineering fixtures |
| M3 | DXGI observation, risk, overlay and shared base live view | M1/M2, calibration/model/catalog contracts | Geometry/freshness/unknown tests; initial native vision and acoustic evidence |
| M4a | First demonstration | M2/M3, authorized media and an evidence-recording method | One recorded live evidence match meeting demo acceptance; does not certify automated highlights or full MVP |
| M4b | OBS local highlights | M2/M3, OBS prerequisites and recording contracts | Verified saved/trimmed local clips with actual coverage, failure/recovery and native OBS evidence |
| M5 | Required local/manual strategic advice, local PTT/STT/answer, memory/history and bounded summary | Model feasibility, corpus consent, completion/retention rules | Advice/context tests, per-language benchmarks, privacy and resource evidence |
| M6 | Optional cloud advice route and separately scoped peer/friends extensions | Frozen projections, separate consent, service designs | Negative/race/privacy tests; two-device/native evidence and operations |
| M7 | Premium authority and advanced details if scheduled | Dedicated server/key/RLS/rollback design | Trusted lease/revocation tests; never a blocker for base M3 |
| M8 | Release candidate and full supported matrix | All selected MVP targets | Full acceptance evidence, signed same-artifact candidate/promotion review |

The registry field `target_milestone` (shown in SRS and RTM) names the earliest milestone that must deliver each requirement; it is planning metadata and does not change scope, priority or acceptance. Final release acceptance of every MVP requirement remains M8. CORE-003 is mapped to M6 because the optional account surface it constrains first exists there; its MVP scope is unchanged; the Decision Log records how the guard applies to builds without an account surface.

M4a and M4b may proceed before M5 when their dependencies are ready. Passing M4a certifies only the limited demonstration: it does not imply M4b or full-MVP completion. M4b remains the local-highlights MVP target. Required local/manual advice does not depend on optional cloud or social services. Extension sequencing can vary when dependencies and scope stay explicit.

## Definition of ready

An implementation task has a bounded purpose, requirement IDs, exact inputs/output and failure behavior, relevant schema/state transition, owner for mutable files, acceptance scenario, evidence class and named open dependencies. A prototype is allowed only when clearly separated from production state.

## Definition of done

All linked requirements have actual implementation evidence; required tests ran on the reported revision; failures are resolved or an explicit scope decision exists; security/data boundaries have negative coverage; UI states are truthful; docs/contracts/generated artifacts agree; remaining native/live gates are identified without being called complete.

For release readiness, source/test completion alone is insufficient. docs/quality/NFR.md owns the measurement protocol and docs/operations/RUNBOOK.md owns release mechanics.

## Backlog record template

Each task records task_id, milestone, requirement_ids, owner, reserved_paths, dependencies, deliverables, test_cases, evidence_level, status, blockers and disposition. A branch belongs to a task; ownership does not transfer merely because another agent can access its directory.

## Estimation

Do not derive calendar dates from this document count. Estimate after M0 spikes establish local-model feasibility, vision coverage, real hardware access and reusable media availability. Track work remaining separately for source implementation, data/assets, native validation, live matches and release operations.

## Review handoff

Hand off a concrete diff or build, requirement coverage, reproduction commands, evidence receipts and unresolved decisions. Retain unique work and runtime evidence until recovery/ownership are verified. A merge is not permission to delete an active checkout.
