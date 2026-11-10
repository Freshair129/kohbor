---
title: "Kohbor Design Implementation Handoff"
doc_id: "DESIGN-HANDOFF"
version: "1.0.0"
spec_baseline: "0.3.1"
authored: "2026-11-11"
timezone: "Asia/Bangkok"
status: "reviewable_candidate"
approval_status: "not_approved"
---

# Design implementation handoff

## Objective and read order

Use the existing **Kohbor / เกาะเบาะ** identity and this design kit to build the control-window presentation within the existing SWE baseline. The current repository is still a specification/design repository, not a completed Tauri application.

Read root [AGENTS.md](../../AGENTS.md), [PRD](../product/PRD.md), [SRS](../requirements/SRS.md), [Document Control](../governance/DOCUMENT-CONTROL.md) and [Decision Log](../governance/DECISION-LOG.md). Then read [UX-SPEC](UX-SPEC.md), [DESIGN-SYSTEM](DESIGN-SYSTEM.md), [COMPONENT-CATALOG](COMPONENT-CATALOG.md), [DESIGN-BOARD](DESIGN-BOARD.md) and [DESIGN-ACCEPTANCE](DESIGN-ACCEPTANCE.md). Open the standalone HTML and inspect its states; treat the static poster as a composition reference.

Do not redesign the system from the poster alone. The written behavior and token roles carry meaning that an image cannot encode.

## What is selected and what remains open

| Selected delegated design | Still open or outside this deliverable |
| --- | --- |
| Nightglass palette, original mark/companion studies, Thai/Latin typography roles, space/radius/focus/motion scales | Owner pixel approval, final outlined wordmark, animation/audio media qualification |
| Fixed stage/panel and live-region authored coordinates; 5 enemy/1 own/4 ally topology | Measured native inner safe area, min window, DPI/readability/target sizing and game HUD clearance |
| Seven-route IA; clips within Insights → History filter | All production routes and optional/deferred service availability |
| Component intent/state/copy/focus contracts and token layers | Full frozen IPC, actual read models, source admission, capability authority, native timers |
| Offline HTML and deterministic state examples | Browser rendering verification, Windows/WebView implementation and all runtime acceptance |

Technical identity remains DEC-NAME work. Do not infer an app ID, URI scheme, signature domain or update origin from this palette or logo. Baseline architecture remains Tauri/Rust plus React/TypeScript/WebView2; this design task creates no skeleton and pins no application dependency.

## Token integration

The canonical editable values are under `design/tokens/source/`. Import the generated `design/tokens/tokens.css` once at the application's root after choosing the actual application directory. Use semantic or component properties in UI code; use primitive names only when authoring new semantic roles. Do not copy hex values from the poster into components.

```css
.hero-card {
  background: var(--kb-color-surface-raised);
  color: var(--kb-color-text-primary);
  border: 1px solid var(--kb-color-border-strong);
  border-radius: var(--kb-radius-md);
  min-height: var(--kb-target-default);
  padding: var(--kb-space-3);
  font: var(--kb-typography-supporting);
}

.hero-card:focus-visible {
  outline: var(--kb-focus-width) solid var(--kb-color-focus);
  outline-offset: var(--kb-focus-offset);
}
```

Set accessibility adapter attributes on `<html>`: `data-kb-contrast="more"` for the project increased-contrast treatment, and `data-kb-motion="reduced"` to remove nonessential transitions. System `forced-colors` overrides project palettes. Never infer user privacy, paid access, source freshness or microphone permission from a theme attribute.

Preserve token aliases when using the merged DTCG-form document. The resolved document is useful for platforms requiring concrete values. Read the bounded supported-type profile in the [token README](../../design/tokens/README.md); importers may require their own adapter. No Figma document or importer-specific mapping is supplied.

## Implementation order

1. Resolve technical identity and any named interface prerequisites for the slice; preserve one owned checkout and file-area ownership.
2. Build app shell, seven destinations, neutral startup, bounded content regions and theme/focus primitives from tokens. Make unavailable/deferred screens honest before integrating a service.
3. Build state primitives and compact hero cards around the authoritative read model. Preserve real zero, unknown, stale, unavailable, estimated and withdrawn as distinct cases. Do not derive a new UI timeout or castability heuristic.
4. Build the fixed Live field and empty right region. Measure map rectangles with frozen data before adding inspector contents; recheck after each relevant state transition.
5. Add explicit selection and summary/event tabs, then permitted details after native capability/source admission. Preserve Escape ordering, native timer ownership and quiet downgrade behavior.
6. Integrate PTT/consent/audio, media, recorder/Clips and history as dependency-ordered slices from SDP. The component catalog describes their presentation; it does not supply missing backend authority.
7. Add actual browser/WebView/native/live receipts to RTM via the registry only after executing the corresponding gates. Keep source checks and DOM fixtures separately labelled.

## Adapter boundaries

- Native producers own availability, identity, generation, observed age, capability verification and source clocks. Components render supplied values and emit explicit user intent.
- An enemy card does not gain HP, ability or item facts because a catalog, sign-in, premium flag or mockup can display them.
- Peer scope is the existing narrow vitals contract. Ability/item sharing is not included. Received peer samples never become AI, memory, history or durable logs.
- Mic, cloud advice, peer sharing, presence and recording consent are independent. Fixture controls do not ship as production controls.
- Optional Store/account/premium service failure leaves available local core intact. No simulated transaction, entitlement, account status or readiness may fill a production page.
- No page-level scroll is introduced within the supported fixed control canvas. Use bounded scroll, tabs or pagination; never shrink an individual label to make it fit.

## Verification commands for this kit

```bash
python tools/build_design_tokens.py --check
python tools/validate_design.py --check
python tools/build_design_board.py --check
node tools/check_design_board.mjs --check
python tools/render_requirements.py --check
python tools/validate_spec.py
```

The DOM command uses the repository's locked documentation dependencies; run `npm ci --prefix tools --ignore-scripts --no-audit --no-fund` if needed. The poster renderer is a separate static image tool. Neither command set measures application CPU, RAM, FPS, acoustic latency, native focus or game safe areas.

Before calling application visuals accepted, execute the currently unrun browser scenarios and native gates in Design Acceptance. The current kit has static and synthetic-DOM evidence only. Keep its browser limitation and incomplete journeys visible; do not copy its design fixture receipt into an application PASS field.
