---
domain: vision-risk
tier: B
card_version: 3
verified_at_commit: a8b636b
verified_on: 2026-11-25
verified_by: "Codex worker; reviewed listed source changes, not human or owner approval"
---

# Vision and risk: screen capture, observation boundary, risk and alert policy

A map, not evidence. Authority: decisions, PRD/SRS, contracts, code and tests, then this card.

## 1. Purpose and boundary

- Owns: `crates/kohbor-capture`, `kohbor-capture-native`, `kohbor-vision`, `kohbor-risk`, `kohbor-minimap-cv`, `kohbor-game-assets`, and host paths `apps/desktop/src-tauri/src/capture_host.rs` and `minimap_recorder/`. Minimap spike map: `tools/spikes/minimap-hero-id/README.md` (LAB-001; DATA-005).
- Hard rule: an enemy field is never filled without an admitted sighting; unknown stays unknown (DATA-005; AGENTS.md; docs/design/REMAINING-SYSTEM-DESIGN.md Sprint 5 exit gate).
- Must not touch: host-core R2 files (settings, status, containment) without a `host-core` claim; the critical warning path except the admission wiring named in design task 6.4 of REMAINING-SYSTEM-DESIGN (`events-audio`).
- Interfaces: `host-core` (frame slot, runtime-status capture phases, settings categories); `events-audio` (critical path); `ui-overlay` (map region, markers, enemy column).

## 2. Owner decisions

- Decided: DEC-SESSION-MONITOR (the map-dominant monitor that capture serves).
- Provisional: DEC-RESOURCE-BUDGET (GPU allowed for the vision path; no RAM or VRAM limit until the first measured baseline; see docs/design/REMAINING-SYSTEM-DESIGN.md section 1.7).
- Open: DEC-MODELS, DEC-GAME-DATA, DEC-RISK-DETAIL, DEC-NATIVE-EVIDENCE.
- Decided: DEC-CRITICAL-MEDIA.

## 3. Now / Next / Blocked

- Capture and observation boundaries map to [REMAINING-SYSTEM-DESIGN](../../design/REMAINING-SYSTEM-DESIGN.md), `apps/desktop/src-tauri/src/capture_host.rs`, `minimap_recorder/` and [SPRINT-ROADMAP](../../delivery/SPRINT-ROADMAP.md) (VISION-004; ALERT-002).
- The opt-in minimap recorder, consent boundary and hero-ID spike map to LAB-001, [KOHBOR-LAB](../../design/KOHBOR-LAB.md) and `tools/spikes/minimap-hero-id/README.md`.
- Detector/model readiness, certified classes and risk-detail policy remain open at their decision and design sources (DEC-MODELS; DEC-GAME-DATA; DEC-RISK-DETAIL; ALERT-003).
- Audible vision warnings still require authorized warning media (DEC-CRITICAL-MEDIA).
- Native/live capture acceptance and detector accuracy remain pending in [SPRINT-ROADMAP](../../delivery/SPRINT-ROADMAP.md) (DEC-NATIVE-EVIDENCE; LAB-001).
- No capture-cost, FPS-loss or detector-quality result is established here; measurement gates map to [REMAINING-SYSTEM-DESIGN](../../design/REMAINING-SYSTEM-DESIGN.md) (QUALITY-003; QUALITY-004).
- Public distribution rights review for bundled game art and data remains pending (DEC-POSITIONING-20261125; DEC-REPLAY-INTEGRATION).

## 4. Open questions

- Q-VR-1 (owner: governance lead, raised 2026-11-13): which sprint numbering holds for S4-S7? (docs/delivery/SPRINT-ROADMAP.md lines 106-131; docs/design/REMAINING-SYSTEM-DESIGN.md section 5)
- Q-VR-2 (owner: product owner, raised 2026-11-12): may own matches be recorded locally for the detector dataset (O-14) and what is the default display rate if 30 fps fails (O-13)? (docs/design/REMAINING-SYSTEM-DESIGN.md section 6)

## 5. Artefacts

Machine list: [state.json](state.json). Primary sources: REMAINING-SYSTEM-DESIGN sections 1, 4-7 (its ADR-A part is `account`, so the file is marked governing there), SPRINT-ROADMAP sprints 4-5, [KOHBOR-LAB](../../design/KOHBOR-LAB.md), and the [minimap hero-ID spike](../../../tools/spikes/minimap-hero-id/README.md). Reference: SDD component CMP-VISION (docs/design/SDD.md).

## 6. Traps and lessons

- Two sprint numberings: the design puts capture in S4, sightings S5, risk S6, overlay S7; the roadmap puts capture and observation in S4, risk S5, overlay S6. Quote the document with the task id (docs/design/REMAINING-SYSTEM-DESIGN.md section 5, first paragraph; docs/delivery/SPRINT-ROADMAP.md).
- Capture freshness is the age of the last successful acquire cycle, a healthy timeout included; a paused game must not read as stale capture (docs/design/REMAINING-SYSTEM-DESIGN.md section 1.3).
- Exhausting the 10% job cap stops audio and alert threads too, so the video path must never push the job to the cap (docs/design/REMAINING-SYSTEM-DESIGN.md section 1.6).
- No predicted paths and no correction from time decay (DEC-SESSION-MONITOR safe interim; contracts/decisions.json DEC-RISK-DETAIL).
- A template or reference image derived from game assets is DEC-GAME-DATA provenance and a rights question for the owner (docs/design/REMAINING-SYSTEM-DESIGN.md section 1.5).
- The default crop (p1080-default-v2, inner map without the grey frame) is validated in `Inner` mode: texture only, so a textured non-minimap image can read as live; only the user's crop confirmation says it is the map (crates/kohbor-minimap-cv structural_check_mode; MVP-1 fix2).
- 30 fps is a design target, not a requirement; the badge shows the measured rate (docs/design/REMAINING-SYSTEM-DESIGN.md baseline check).

## 7. Who may write, how to claim, how to ask

- Nothing exists to claim yet. The first write needs a claim in the coordination ledger (`coord:claims/`, see AGENTS.md); new crates, settings categories and runtime-status changes are R2 and go through the lead.
- Ask: this card, then a read-only consultant over the governing documents, then the owner for models, certified classes, risk parameters and hardware runs (docs/domains/README.md section 3).
