---
title: "Technical and Delivery Risk Register"
doc_id: "RISK-REGISTER"
status: draft
version: "0.3.5"
updated: "2026-11-11"
owner_role: "Engineering lead; person not assigned"
approval_status: "not_approved"
---

# Technical and Delivery Risk Register

These are evidenced specification dependencies and unproven feasibility questions, not measured failure probabilities. Owner columns identify required roles, not people who have accepted an assignment. All entries are open until closure evidence is recorded. Product risks and scope appear in [PRD](../product/PRD.md); concrete decisions are in [DECISION-LOG](DECISION-LOG.md).

| ID | Risk and affected need | Trigger / consequence | Mitigation and closure evidence | Role / decision |
| --- | --- | --- | --- | --- |
| RISK-01 | Local model feasibility; PN-006/007 | Voice/advice exceeds latency or whole-app RAM/CPU | Run bounded artifact feasibility before model integration; record exact model/license/hash, native measurements and per-language accuracy; no silent cloud fallback | ML/engineering; DEC-MODELS |
| RISK-02 | Observation/class coverage; PN-002/003 | Unsupported class, geometry or patch creates false fact/warning | Freeze catalog/admission; held-out per-class and game-matrix evidence; unavailable states for uncovered cases | Vision/QA; DEC-GAME-DATA |
| RISK-03 | Audible warning evidence; PN-003 | Informational score is mistakenly treated as sufficient warning admission | Freeze coordinated-motion, coverage-gap and reappearance fixtures; prove false-warning/false-safe gates and acoustic timing | Native/QA; DEC-RISK-DETAIL |
| RISK-04 | Critical media and package trust; PN-003/004 | Missing/corrupt asset or self-asserted trust blocks safe readiness | Obtain authorized critical assets; freeze signing/verification domains; test hostile archives and no-fallback readiness | Media/native; DEC-CRITICAL-MEDIA, DEC-PACK-CRYPTO |
| RISK-05 | Parallel interface drift; all needs | Independent components interpret draft schema as a complete protocol | Freeze exact interfaces and semantic rules before integration; one contract owner; negative/race tests. The M1 intake slice is proposed in GSI-INTAKE-SPEC and closes with capture-spike evidence | Engineering; DEC-INTERFACES, DEC-GSI-INTAKE |
| RISK-06 | Original UX not measured; PN-005 | Layout/focus/readability violates fixed field or disrupts play | Produce annotated layouts, focus and scaling evidence on declared Windows matrix | UX/QA; DEC-VISUALS |
| RISK-07 | Durable memory erasure; PN-008 | Restart/rebuild restores forgotten derived information | Choose completion/retention and durable comparable cutoff; test crash/restart/corrupt-marker behavior | Data/native; DEC-SESSION, DEC-RETENTION |
| RISK-08 | Transient voice lifecycle; PN-006 | Unbounded or late audio/transcript survives cancellation | Freeze bounded handoff/zeroization; test release, cancellation, mute, overlap and failure traces | Voice/native; DEC-VOICE-BUFFER |
| RISK-09 | Recording qualification; PN-011 | App-only metrics hide recorder/system cost or clips misstate coverage | Separate app/OBS/combined measurements, actual clip inspection, overlap timing and full highlight gate | Media/QA; DEC-OBS-BUDGET |
| RISK-10 | Optional-service authority; PN-009/010/012 | Account/friendship/entitlement expands consent or stalls local core | Keep boundaries; freeze service contracts/ownership before affected extension; test loss/withdrawal and stale work | Backend/product; DEC-SOCIAL, DEC-PREMIUM |
| RISK-11 | Missing native validation access; all needs | Synthetic success is presented as runtime certification | Assign Windows/game/audio/hardware access, execute required test matrix, preserve NOT_RUN otherwise | QA/product; DEC-NATIVE-EVIDENCE |
| RISK-12 | Incomplete requirement extraction; all needs | An unmapped normative annex clause or behavior is overlooked | Review needs, journeys and annexes against SRS/RTM; resolve gaps through change control; never infer completeness from row count | Product/engineering/QA; baseline review |

Review risks at each SDP milestone. Add evidence links and an actual closure decision when a risk is resolved; do not mark it closed because a prototype or document exists. New uncertainty that changes behavior, privacy, scope or budgets requires the relevant owner decision. Routine engineering choices can be documented within the existing authority.
