---
domain: ui-overlay
tier: B
card_version: 1
verified_at_commit: da88446
verified_on: 2026-11-13
verified_by: "claude-code subagent draft (read-only), integrated by the governance lead; agent work, not human review"
---

# UI and overlay: renderer, Command Deck, overlay window, visual design

A map, not evidence. Authority: decisions, PRD/SRS, contracts, code and tests, then this card.

## 1. Purpose and boundary

- Owns: `apps/desktop/src/**`, docs/design/UX-SPEC.md, docs/design/VISUAL-DESIGN-PROPOSAL.md, `contracts/design-tokens.json`, `docs/design/visuals/**` (two DESIGN-NOTES.md files, HTML artboards, canvas files).
- Must not touch: host-core trust-boundary files (intake, secrets, containment, instance, atomic .rs, tauri.conf.json, capability files; docs/delivery/AGENT-WORKFLOW.md section 6). An overlay window needs edits to the last two, so it needs a `host-core` claim and an R2 read.
- Interfaces: `host-core` (runtime-status, own-snapshot, settings-patch); `events-audio` and `packs` (AudioPanel, AudioHistory, EventBanner, PackPanel and their parsers sit in this write area, behaviour is theirs); `vision-risk` (map, markers); `account` (Share list screens are drawings only).

## 2. Owner decisions

- Decided: DEC-SESSION-MONITOR; adjacent: DEC-BANNER-LAB-STYLE (banner look, does not approve tokens), DEC-SHARE-LIST (design only).
- Open: DEC-VISUALS (direction approved 2026-11-11 per the proposal, record still open), DEC-INTERFACES, DEC-NATIVE-EVIDENCE, DEC-SOCIAL.

## 3. Now / Next / Blocked

- NOW: the renderer shows the Overview, own-hero card, setup guide, audio, pack and history panels; no Live, Session Monitor or other destination exists in source (apps/desktop/src/App.tsx; UI-002).
- NOW: an overlay window exists in source (built in code, transparent, on top, click-through, listen-only capability, shown only while a banner or chip is up) with a 720x220 banner stage, a status chip and a feature-gated DEMO trigger; synthetic evidence only, nothing is verified on Windows or in the game (docs/design/S6-OVERLAY-SLICE.md section 9; apps/desktop/src-tauri/src/overlay.rs; owner checks in the game are listed in section 9 of the plan (TP-05, UAT-05; UI-001).
- NOW: styles.css names IBM Plex but no font file or `@font-face` is in the source, so real Thai rendering in WebView2 is UNVERIFIED (apps/desktop/src/styles.css:26).
- NEXT (no owner needed): Session Monitor on mocks, before native capture exists (docs/design/REMAINING-SYSTEM-DESIGN.md section 5, design task 4.8, not yet in the roadmap backlog; UI-002, UI-004).
- NEXT: screens still undesigned: Voice Packs, Insights, Account, Settings, Store "not in this build", live-field variants (docs/design/VISUAL-DESIGN-PROPOSAL.md section 7; DEC-VISUALS).
- BLOCKED: safe areas, readable sizes, motion timing and FPS need Windows measurements (docs/design/VISUAL-DESIGN-PROPOSAL.md section 7; DEC-NATIVE-EVIDENCE; QUALITY-003).
- BLOCKED: overlay content needs owner answers on the own-stat row and banner size (Q-UI-1, Q-UI-2 below; UI-001, UI-003).

## 4. Open questions

- Q-UI-1 (owner: product owner, raised 2026-11-13): does the overlay carry an HP/mana own-stat row? UX-SPEC and the proposal say yes, the roadmap says no (docs/design/UX-SPEC.md line 134; docs/delivery/SPRINT-ROADMAP.md line 137).
- Q-UI-2 (owner: product owner, raised 2026-11-13): which banner size is binding: 360x203 clip, 720x220 stage, or 1280x480 WebM? (docs/design/visuals/s3-audio-ui/DESIGN-NOTES.md section 6 item 3; docs/delivery/SPRINT-ROADMAP.md line 137; DEC-BANNER-LAB-STYLE)
- Q-UI-3 (owner: product owner, raised 2026-11-12): scaled deck for the Live destination versus the 1420x760 stage (docs/design/REMAINING-SYSTEM-DESIGN.md section 6, O-12). The other owner questions in the two DESIGN-NOTES files (command-deck section 11, s3-audio-ui section 6) have no DEC id either.

## 5. Artefacts

Machine list: [state.json](state.json). Governing: UX-SPEC, VISUAL-DESIGN-PROPOSAL, `contracts/design-tokens.json`. Reference: the two DESIGN-NOTES.md files and their artboards.

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
