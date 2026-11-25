---
domain: ui-overlay
tier: B
card_version: 3
verified_at_commit: a8b636b
verified_on: 2026-11-25
verified_by: "Codex worker; reviewed listed source changes, not human or owner approval"
---

# UI and overlay: renderer, Command Deck, overlay window, visual design

A map, not evidence. Authority: decisions, PRD/SRS, contracts, code and tests, then this card.

## 1. Purpose and boundary

- Owns: `apps/desktop/src/**`, docs/design/UX-SPEC.md, docs/design/VISUAL-DESIGN-PROPOSAL.md, `contracts/design-tokens.json`, `docs/design/visuals/**` (two DESIGN-NOTES.md files, HTML artboards, canvas files).
- Must not touch: host-core trust-boundary files (intake, secrets, containment, instance, atomic .rs, tauri.conf.json, capability files; docs/delivery/AGENT-WORKFLOW.md section 6). An overlay window needs edits to the last two, so it needs a `host-core` claim and an R2 read.
- Interfaces: `host-core` (runtime-status, own-snapshot, settings-patch); `events-audio` and `packs` (AudioPanel, AudioHistory, EventBanner, PackPanel and their parsers sit in this write area, behaviour is theirs); `vision-risk` (map, markers); `account` (Share list screens are drawings only).

## 2. Owner decisions

- Decided: DEC-SESSION-MONITOR, DEC-LIVE-SECOND-MONITOR, DEC-OVERVIEW, DEC-ANALYTICS-V2, DEC-LANE-EXPECTATION, DEC-UX-REVIEW-20261125, DEC-ANALYTICS-LAYOUT, DEC-AI-PROVIDERS, DEC-REPLAY-AUTO-PARSE, DEC-REPLAY-INTEGRATION, DEC-POSITIONING-20261125; stat-icon presentation is recorded in [DECISION-LOG](../../governance/DECISION-LOG.md); adjacent: DEC-BANNER-LAB-STYLE (banner look, does not approve tokens), DEC-SHARE-LIST (design only).
- Open: DEC-VISUALS (direction approved 2026-11-11 per the proposal, record still open), DEC-INTERFACES, DEC-NATIVE-EVIDENCE, DEC-SOCIAL.

## 3. Now / Next / Blocked

- Overview and Live/second-monitor layout map to [UX-SPEC](../../design/UX-SPEC.md) (UI-002, UI-016, UI-020; DEC-OVERVIEW; DEC-ANALYTICS-LAYOUT).
- Analytics UI-021/UI-022 and replay entry points map to [ANALYTICS-HOST-API](../../design/ANALYTICS-HOST-API.md), [DESIGN-IMPLEMENTATION](../../design/DESIGN-IMPLEMENTATION.md) and [REPLAY-PIPELINE](../../design/REPLAY-PIPELINE.md).
- The expert lane-expectation table remains empty pending reviewed coach rows (DEC-LANE-EXPECTATION; [LANE-EXPECTATION](../../design/LANE-EXPECTATION.md)).
- AI provider controls stay hidden in normal mode and gated to Developer mode (DEC-AI-PROVIDERS; DEC-UX-REVIEW-20261125; DEC-POSITIONING-20261125).
- The Store has a Developer-mode preview only; commerce remains undecided (DEC-COMMERCE-INTENT; DEC-POSITIONING-20261125; [SPRINT-ROADMAP](../../delivery/SPRINT-ROADMAP.md)).
- Overlay, layout editor and objective-alert modules map to `apps/desktop/src-tauri/src/overlay.rs`, `apps/desktop/src/layoutEditor/`, `apps/desktop/src/HudStage.tsx` and [S6-OVERLAY-SLICE](../../design/S6-OVERLAY-SLICE.md) (UI-001, UI-015; ALERT-008).
- Current-build owner/native acceptance and Windows measurements remain pending in [SPRINT-ROADMAP](../../delivery/SPRINT-ROADMAP.md) (DEC-NATIVE-EVIDENCE; QUALITY-003).
- Open overlay content and banner-size questions remain Q-UI-1 and Q-UI-2 below (UI-001, UI-003).

## 4. Open questions

- Q-UI-1 (owner: product owner, raised 2026-11-13): does the overlay carry an HP/mana own-stat row? UX-SPEC and the proposal say yes, the roadmap says no (docs/design/UX-SPEC.md line 134; docs/delivery/SPRINT-ROADMAP.md line 137).
- Q-UI-2 (owner: product owner, raised 2026-11-13): which banner size is binding: 360x203 clip, 720x220 stage, or 1280x480 WebM? (docs/design/visuals/s3-audio-ui/DESIGN-NOTES.md section 6 item 3; docs/delivery/SPRINT-ROADMAP.md line 137; DEC-BANNER-LAB-STYLE)

## 5. Artefacts

Machine list: [state.json](state.json). Governing: UX-SPEC, VISUAL-DESIGN-PROPOSAL, `contracts/design-tokens.json`. Reference: DERIVED-STATS, REPLAY-PIPELINE, REPLAY-ENGINE-PROVENANCE, the replay-integration decision entry in DECISION-LOG, the two DESIGN-NOTES.md files and their artboards.

## 6. Traps and lessons

- The artboards are plain HTML or canvas sources with sample data; contrast is computed from hex, not measured, and Thai was checked only in browsers (docs/design/visuals/command-deck/DESIGN-NOTES.md section 12).
- No green "safe" and no invented value: unknown shows "—"; a failed status poll shows host-lost, not a stale "live" reading (docs/design/VISUAL-DESIGN-PROPOSAL.md section 1; docs/delivery/M1-STATUS.md section 3.2).
- Opening the inspector or detail must move the map by at most 1 CSS px; this was checked only on the mockup (docs/design/UX-SPEC.md section 4; DESIGN-NOTES section 12).
- GSI replay into the loopback intake cannot produce a demo event: admission needs the signed-in Steam id to equal the payload steamid (crates/kohbor-gsi/src/admission.rs; docs/design/S6-OVERLAY-SLICE.md section 5).
- Preview the renderer with the dev-only mock harness (`?mock=1`, mocked invoke), never by starting a second app instance next to the owner's build (apps/desktop/src/main.tsx; docs/delivery/M1-STATUS.md section 3.2).
- Overlay modules must never cover the minimap rectangle, because capture would see the overlay (docs/design/UX-SPEC.md section 8; docs/design/REMAINING-SYSTEM-DESIGN.md section 4).
- Visual-direction approval is not measured-value or native acceptance (docs/design/VISUAL-DESIGN-PROPOSAL.md section 1).
- The overlay lives in a hidden window until a banner or chip is due and cannot draw above exclusive fullscreen: Dota 2 must run borderless windowed. A demo build (cargo feature `demo` plus `KOHBOR_DEMO=1`) shares the real engine, so do not use it for a real match (docs/design/S6-OVERLAY-SLICE.md sections 4 and 9).

## 7. Who may write, how to claim, how to ask

- Open a claim in the coordination ledger (`coord:claims/`, see AGENTS.md) before the first write; the holder updates this card at task end. Renderer work is R1 unless it touches a contract or the files in section 1.
- Ask: this card, then a read-only consultant over UX-SPEC and the proposal, then the owner for taste, banner size and overlay content (docs/domains/README.md section 3).
