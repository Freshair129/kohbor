---
title: "Kohbor Design Board"
doc_id: "DESIGN-BOARD"
version: "1.0.0"
spec_baseline: "0.3.1"
authored: "2026-11-11"
timezone: "Asia/Bangkok"
status: "reviewable_candidate"
approval_status: "not_approved"
---

# Design Board — what the visual reference establishes

Open the [standalone HTML board](../../design/board/kohbor-design-board.html) after downloading it, or inspect the [static poster](../../design/board/kohbor-design-board.png). Both describe the **Nightglass** design direction. The HTML is an offline design document containing a live-field interaction specimen; the PNG is a deterministic visual composition, not a browser screenshot or application capture.

## 1. What is shown

| Board section | Purpose | Governing source |
| --- | --- | --- |
| Identity | Original K/seat mark and static seat-shaped companion; quiet character, restrained mint | DESIGN-SYSTEM §3; assets README |
| Live field | Fixed canvas, five enemy slots, own hero, four allies, central map, permanent right region | UX-SPEC §§3–6; UI-002–UI-006 |
| Foundations | Exact palette, Thai/Latin typography, spacing, corners and solid material | Token sources and DESIGN-SYSTEM §3 |
| Components | Action hierarchy, input, consent specimen and local voice status | COMPONENT-CATALOG; native consent remains outside this fixture |
| Data states | Fresh, unknown, stale, estimated, unavailable and withdrawn examples | UX-SPEC §6; DATA/PEER requirements |
| Passive overlay | Proposed relative placement and areas deliberately left clear | UX-SPEC §8; native measurement still required |
| Handoff | Embedded real JSON/CSS downloads and artifact/evidence version | Token README; DESIGN-ACCEPTANCE |

No screenshot, game portrait, inherited brand asset, real-person voice or live game event is imported into the board. The map background is decorative and has **no live markers**. The original companion is a design study, not a verified audio/model readiness animation. The editable wordmark uses real text and is not a frozen outlined production asset.

## 2. Implemented interaction subset

The **design controls outside the application stage** let a reviewer switch Waiting, Synthetic data, Stale and Source unavailable/GSI-only. These labels never grant a production data source or paid capability. Reloading returns to neutral startup.

| Interaction | Implemented behavior | Limit |
| --- | --- | --- |
| Scenario select | Neutral state has nine compact unknown slots and no own values. Live/stale examples supply explicitly synthetic observations. GSI-only preserves own fixture values but clears unknown enemy/ally identities and ages. | No actual source admission, session transport or clocks. |
| Base / Pro fixture | Changing either clears selection and inline detail. Base keeps eligible summary and local events. Pro shows an additional provenance-detail example only. | No production entitlement/lease; not a full ability/item inspector. |
| Known hero card | Opens only its allowed summary. Unknown cards cannot be selected. | No marker-selection fixture, hero catalog, enemy vitals or peer ability sharing. |
| Summary / Events | Semantic tabs with selected state, roving focus, Left/Right/Home/End keys. The right region stays allocated. | DOM logic tests do not measure browser layout. |
| Escape | Closes the inline provenance detail first, then inspector; returns focus to the selected card when it remains available. | Native/game focus safety is untested. |
| High contrast / reduced motion | Uses the generated token adapter attributes and additional specimen border treatment. | Actual OS forced-colors and rendered contrast still need browser/native inspection. |
| Input / opt-in / voice examples | Edits local specimen text, toggles a non-device switch, or changes visible voice state copy. | No mic/device/recording/AI/network operation. |
| Download JSON/CSS | Generated HTML embeds the canonical merged tokens and CSS; source index fetches sibling files when served and reports an offline fallback if unavailable. | These are project-profile DTCG forms; no universal Figma/importer compatibility claim. |

The seven navigation destinations and Live subview labels are a **noninteractive anatomy specimen**. This board does not implement seven production screens. Complete route, modal, recorder, media-install, ability/item, peer room, late-response or session-epoch journeys are specified in the component catalog/acceptance plan and require subsequent fixtures and application work.

## 3. Authored geometry

| Coordinate space | Region | x | y | Width | Height |
| --- | --- | ---: | ---: | ---: | ---: |
| Stage | Stage | 0 | 0 | 1420 | 760 |
| Stage | Navigation | 20 | 20 | 88 | 720 |
| Stage | Local panel | 120 | 20 | 1280 | 720 |
| Panel | Header | 0 | 0 | 1280 | 72 |
| Panel | Enemy row | 228 | 84 | 616 | 72 |
| Panel | Own hero / companion | 16 | 168 | 196 | 430 |
| Panel | Map | 228 | 168 | 616 | 430 |
| Panel | Ally row | 228 | 610 | 616 | 94 |
| Panel | Reserved right region | 860 | 84 | 404 | 620 |

The map has the greatest individual live-region area. The right region is permanently allocated; its content changes without reflowing map columns. These are authored design choices within inherited topology, not measured Windows pixels. Browser border-box and actual transformed rectangle measurement remain an explicit next verification step.

The long board document may scroll and reflow as a document. Its embedded application specimen remains a fixed stage. Fitting that illustration into a small document viewport does not establish mobile application support or an acceptable production minimum size. At a 1280 × 720 content viewport the inherited uniform fit gives a calculated scale of approximately 0.9014; 14 px text becomes approximately 12.62 CSS px. Native readability and minimum-window resolution remain open.

## 4. Build and evidence

Editable sources are [index.html](../../design/board/index.html), [board.css](../../design/board/board.css) and [board.js](../../design/board/board.js). `tools/build_design_board.py` embeds the generated tokens, source styles/script, original SVGs, four Noto font files and their full copyright/OFL notices into one HTML. Its `--check` verifies exact bytes against inputs.

[bundle-validation.json](../../design/board/bundle-validation.json) records the static build hashes. [dom-validation.json](../../design/board/dom-validation.json) records sixteen synthetic DOM checks, including neutral startup, narrow data presentation, ordered Escape, tier-loss cleanup, keyboard tabs, GSI-only uncertainty, adapter controls, embedded downloads and absence of device/network side effects in the exercised subset. jsdom has no layout engine; the fixture width is a harness input and its rectangles are not accepted measurements.

Rendered browser inspection was attempted but not completed: the cloud browser could not reach the workspace HTTP server; its error page could not be observed under the browser URL policy. No alternate surface, raw browser commands or policy workaround was used. **Rendered viewport/font/focus-ring checks remain NOT_RUN**. The poster remains a static composition, regardless of whether its bitmap looks polished.

All Windows/native/live/acoustic/resource gates remain governed by [DESIGN-ACCEPTANCE.md](DESIGN-ACCEPTANCE.md) and the existing Test/UAT plans. DEC-VISUALS stays open for those measurements and owner visual review.
