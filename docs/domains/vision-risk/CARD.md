---
domain: vision-risk
tier: B
card_version: 1
verified_at_commit: 0997546
verified_on: 2026-11-13
verified_by: "claude-code subagent draft (read-only), integrated by the governance lead; agent work, not human review"
---

# Vision and risk: screen capture, observation boundary, risk and alert policy (M3, not started)

A map, not evidence. Authority: decisions, PRD/SRS, contracts, code and tests, then this card.

## 1. Purpose and boundary

- Owns (planned, PROPOSED): `crates/kohbor-capture`, `kohbor-capture-native`, `kohbor-vision`, `kohbor-risk`, `spikes/capture` (docs/design/REMAINING-SYSTEM-DESIGN.md section 4). None exists: `crates/` and `spikes/` hold no such directory (checked 2026-11-13).
- Hard rule: an enemy field is never filled without an admitted sighting; unknown stays unknown (DATA-005; AGENTS.md; docs/design/REMAINING-SYSTEM-DESIGN.md Sprint 5 exit gate).
- Must not touch: host-core R2 files (settings, status, containment) without a `host-core` claim; the critical warning path except the admission wiring named in design task 6.4 of REMAINING-SYSTEM-DESIGN (`events-audio`).
- Interfaces: `host-core` (frame slot, runtime-status capture phases, settings categories); `events-audio` (critical path); `ui-overlay` (map region, markers, enemy column).

## 2. Owner decisions

- Decided: DEC-SESSION-MONITOR (the map-dominant monitor that capture serves).
- Open: DEC-MODELS, DEC-GAME-DATA, DEC-RISK-DETAIL, DEC-NATIVE-EVIDENCE, DEC-CRITICAL-MEDIA.

## 3. Now / Next / Blocked

- NOW: nothing built; `capture` is reported unavailable from a constant (apps/desktop/src-tauri/src/status.rs:82; VISION-001).
- NOW: the design (D1-D9, spikes V1-V5) is PROPOSED by an agent, not approved, nothing in it was run (docs/design/REMAINING-SYSTEM-DESIGN.md sections 0, 9).
- NEXT (no owner needed): contracts (design task 4.0), state-machine tests on a fake frame source (4.4) and risk golden tests (6.1), none yet in the roadmap backlog (docs/design/REMAINING-SYSTEM-DESIGN.md section 5; VISION-004, ALERT-002).
- NEXT (owner hardware): spikes V1-V3 need owner FPS pairs and stills (docs/design/REMAINING-SYSTEM-DESIGN.md section 1.8; DEC-NATIVE-EVIDENCE).
- BLOCKED: markers and identity need a detector that passes the gate and a certified class list (docs/design/REMAINING-SYSTEM-DESIGN.md section 1.7; DEC-MODELS, DEC-GAME-DATA).
- BLOCKED: score stays informational and audible gank unavailable until evidence admission is frozen (contracts/decisions.json DEC-RISK-DETAIL; ALERT-003).
- BLOCKED: an audible vision warning also needs authorized warning media (docs/design/REMAINING-SYSTEM-DESIGN.md section 7; DEC-CRITICAL-MEDIA).
- NOT VERIFIED: any capture cost, FPS loss or detector quality figure (docs/design/REMAINING-SYSTEM-DESIGN.md section 9; QUALITY-003, QUALITY-004).

## 4. Open questions

- Q-VR-1 (owner: governance lead, raised 2026-11-13): which sprint numbering holds for S4-S7? (docs/delivery/SPRINT-ROADMAP.md lines 106-131; docs/design/REMAINING-SYSTEM-DESIGN.md section 5)
- Q-VR-2 (owner: product owner, raised 2026-11-12): may own matches be recorded locally for the detector dataset (O-14) and what is the default display rate if 30 fps fails (O-13)? (docs/design/REMAINING-SYSTEM-DESIGN.md section 6)

## 5. Artefacts

Machine list: [state.json](state.json). Primary sources: REMAINING-SYSTEM-DESIGN sections 1, 4-7 (its ADR-A part is `account`, so the file is marked governing there) and SPRINT-ROADMAP sprints 4-5. Reference: SDD component CMP-VISION (docs/design/SDD.md).

## 6. Traps and lessons

- Two sprint numberings: the design puts capture in S4, sightings S5, risk S6, overlay S7; the roadmap puts capture and observation in S4, risk S5, overlay S6. Quote the document with the task id (docs/design/REMAINING-SYSTEM-DESIGN.md section 5, first paragraph; docs/delivery/SPRINT-ROADMAP.md).
- Capture freshness is the age of the last successful acquire cycle, a healthy timeout included; a paused game must not read as stale capture (docs/design/REMAINING-SYSTEM-DESIGN.md section 1.3).
- Exhausting the 10% job cap stops audio and alert threads too, so the video path must never push the job to the cap (docs/design/REMAINING-SYSTEM-DESIGN.md section 1.6).
- No predicted paths and no correction from time decay (DEC-SESSION-MONITOR safe interim; contracts/decisions.json DEC-RISK-DETAIL).
- A template or reference image derived from game assets is DEC-GAME-DATA provenance and a rights question for the owner (docs/design/REMAINING-SYSTEM-DESIGN.md section 1.5).
- 30 fps is a design target, not a requirement; the badge shows the measured rate (docs/design/REMAINING-SYSTEM-DESIGN.md baseline check).

## 7. Who may write, how to claim, how to ask

- Nothing exists to claim yet. The first write needs a claim in the coordination ledger (`coord:claims/`, see AGENTS.md); new crates, settings categories and runtime-status changes are R2 and go through the lead.
- Ask: this card, then a read-only consultant over the governing documents, then the owner for models, certified classes, risk parameters and hardware runs (docs/domains/README.md section 3).
