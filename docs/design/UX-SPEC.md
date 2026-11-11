---
title: "User Experience Specification"
doc_id: "UX-SPEC"
status: draft
version: "0.3.4"
updated: "2026-11-11"
baseline: "DEC-BASELINE"
owner_role: "UX owner; people not assigned"
approval_status: "not_approved"
---

# User Experience Specification

## 1. Authority and design freedom

This contract carries **DEC-BASELINE** behavior under [Document control](../governance/DOCUMENT-CONTROL.md), without approving a new visual design. The product name is **Kohbor / เกาะเบาะ**, selected by the owner. The agent designs the original mascot, palette and module labels while preserving these behaviors.

A first visual proposal (tokens, mark, Command Deck Overview/Live and overlay artboards) is in [VISUAL-DESIGN-PROPOSAL](VISUAL-DESIGN-PROPOSAL.md) under DEC-VISUALS; it is not approved. **Final neutral mockups, design tokens, artwork and motion/audio assets are not supplied.** Create these and record design decisions before visual acceptance. Fixtures do not establish live data, native geometry or resource compliance.

Logical destinations below do not prescribe URL paths, file names or IPC identifiers. Those belong in the implementation contracts. User-facing copy supports Thai; English technical identifiers must not leak into ordinary states without a useful explanation.

## 2. Window and route inventory

| Surface / destination | Content and actions | Boundaries |
|---|---|---|
| Control: Overview | Readiness, observed match summary, audio controls, companion activity | Never fill live areas with demo statistics |
| Control: Live | Tabs for Live field, Build, CV heroes, Cooperative sharing | Shared destination for every tier |
| Control: Voice Packs | Installed inventory, verification, activation, explicit preview | Preview does not simulate a live event |
| Control: Store | Catalog, wallet, inventory, transaction history | Optional account services; no gate on local gameplay |
| Control: Insights | Summary and local history tabs | Separate simple evidence summary from deferred deep analysis |
| Control: Account | Optional sign-in, linked identity and supported account settings | Identity does not authorize game-data sharing |
| Control: Settings | General, Overlay, Audio and alerts, AI, Modules and CV, Privacy, System | Bounded categories; explain unavailable capabilities |
| Gameplay overlay | Passive alerts, event media, selected stat modules, companion status | Transparent, always-on-top, click-through; no control-window chrome |

The control window retains seven main destinations. Build and History remain subviews, not extra main pages. Local clip controls require a clearly documented placement within this structure; a new main destination is a deliberate design decision. Do not inherit a prototype navigation bar as production routing.

This inventory describes the target information architecture, not a requirement to complete every service in the desktop MVP. A retained destination for deferred Store, wallet or account-security capabilities presents an honest unavailable or not-in-this-build state, with no simulated transactions or apparently working controls. Mark each destination's delivered, unavailable or deferred state in the build scope. Its navigation entry does not establish backend readiness, operational authorization or completed feature acceptance.

## 3. Fixed canvas baseline

The inherited desktop baseline uses an authored stage of **1420 × 760** and a **1280 × 720 panel-local canvas**. These are different coordinate spaces. Fit the authored stage to the available window while preserving its geometry; do not replace its dimensions with a reference screenshot's viewport.

Every page stays within the canvas. Overflow uses bounded regions, tabs or pagination. There is no page-level scrollbar on supported desktop sizes. Do not independently shrink text to squeeze additional content into a region.

Validate desktop references at 1280 × 720, 1440 × 900 and 1672 × 941, recording actual content bounds and DPI. These are reference windows, not a certified native support matrix. Small mobile/tablet prototypes do not establish a mobile product.

The native inner safe area, minimum readable scaled typography, minimum interactive target and final compact-card dimensions require measurement and a recorded decision. Do not invent measured values. If a proposed design cannot meet readability within the baseline, resolve that conflict before changing geometry.

## 4. Live field anatomy

| Region | Required content | Unavailable behavior |
|---|---|---|
| Top | Five stable enemy slots | Preserve slots; show known identity and admitted visibility/age only |
| Left | Own hero, relevant missing/status information, separate companion visual | Known own observations only; no fabricated vitals |
| Center | Largest region; actual admitted minimap preview and eligible markers | Explicit unavailable/Lite/stale state |
| Bottom | Four stable ally slots, excluding self | Compact unknown allies; fresh peer detail only where authorized |
| Right | Permanently reserved summary/inspector/event-feed region | Neutral initially; never collapse its width |

The map is the first visual priority; missing-enemy information is second. Own hero is not repeated among the four allied slots.

Enemy and unknown ally cards collapse to portrait or identity placeholder, admitted seen/missing and last-seen age. Hide unknown-only skill/item rows; do not fill the screen with question-mark tiles. Unknown age appears as “—”, not a fabricated timestamp.

Live markers require matching frame, session, source generation, healthy capture and calibrated geometry. Convert source pixels using the admitted region dimensions. Decorative artwork may be a labelled non-live fallback, but it cannot host live markers under unmatched geometry.

With input frozen, map x/y/width/height change by **no more than 1 CSS px** when opening, changing or closing detail, switching event-feed tabs or changing tier.

## 5. Selection and inspector behavior

Begin with no hero selected. Live refresh cannot open an inspector or move focus. Clicking a hero card or a known marker selects the same stable entity key. An unknown marker cannot guess an identity.

The selected entity key contains session epoch, team and player slot, with hero identity/revision guards. Switching hero resets inline detail, expanded backpack and local scroll. Session/source/identity changes invalidate stale selection and reject late responses.

Base permits the eligible base summary. Valid Pro permits the full inspector, using only admitted values. A skill preview may show up to four admitted entries with “+N”; the complete list supports variable length. Do not hard-code every hero to four abilities.

Catalog description is distinct from observed learned rank and readiness. Catalog-only information must be labelled. Cooldown zero alone does not establish castability. Skill cooldown, charge recharge, passive state, unlearned state, insufficient evidence and manual estimates are distinct.

For inventory, distinguish known empty, unknown, stack count, charges, cooldown and inventory group. Clicking any skill or item opens detail only; it never controls the game.

Escape collapses inline detail first, then closes the inspector. An editing control may consume Escape for its own operation. Close returns focus to the selected card when available. The inspector is not a modal focus trap.

Closing or changing selection does not delete a timer. The native timer module owns time and lifecycle. Manual estimates use labels such as “estimated ready,” including after expiry. Stale or paused authoritative game time must not continue from browser wall time.

## 6. Field provenance and honest states

Every dynamic view field retains value, availability, source, observed age, source generation and revision. Show provenance where it changes the user's interpretation; do not bury critical uncertainty in a tooltip.

| Condition | Required presentation |
|---|---|
| Known value, including zero | Render the actual value with appropriate units |
| Unknown | “—” or a specific no-data label; never substitute zero |
| Unavailable producer | Explain the unavailable capability and available recovery action |
| Stale observation | Mark old data and age; no affirmative ready/safe claim |
| Withdrawn peer sample | Clear peer vitals and reduce the card to its admitted base state |
| Catalog-only information | Identify it as reference information, not a live measurement |
| Manual estimate | Label the estimate separately from observation |
| Missing asset | Stable placeholder and readable identity; preserve dimensions |
| Waiting/no match | Neutral readiness state, no seeded roster |
| GSI-only mode | Local functions remain available; CV-derived fields are unavailable |

Peer freshness must follow the single producer contract, including its 2,000/2,001 ms boundary tests. Do not create a second UI timeout with different semantics. A percentage does not justify reconstructing an unknown numerator or maximum.

Parsed selected-unit skills remain unavailable until ownership admission succeeds. Paid access does not broaden the own-player, peer or CV data contract.

## 7. Entitlement presentation

| State or transition | Required UI |
|---|---|
| Signed-out / absent authority | Base field, neutral inspector, available event feed |
| Valid Pro | Enable eligible inspector, timers, peer detail and reviewed review integration |
| Offline with valid lease | Retain Pro; evaluate source freshness separately |
| Lapsed / revoked / unverifiable | Immediately return to base presentation |
| Pro regained | Restore available actions; keep selection neutral |

Production authority must be real and verified. Local storage, fixture input and the renderer cannot grant access. Without authority, production stays base; fixture mode may simulate tier behavior explicitly.

On downgrade, remove Pro detail, selection, expanded state and late responses in one coherent transition. Preserve the route, map bounds, own/base observations and local event feed. Do not show a blank frame, redirect, modal, toast or sound. Recover focus to an available card or region heading without stealing game focus.

Presentation changes do not clear native timers, stop peer producers or change consent. Core telemetry, alerts, gameplay overlay and voice alerts remain independent of tier.

## 8. Gameplay overlay

The overlay must not intercept game mouse/keyboard input or cover essential minimap, skill, shop and status regions. Its typography and colors share the product design language; its layout stays lighter than the control window.

Retain the baseline arrangement: own-stat row near the top-left, risk indicator near top-center, event banner below the game's announcement area and companion presence near top-right. Exact native placement and media dimensions require measurement on the supported matrix. Screenshot-box geometry alone does not prove native banner clearance.

Provide per-module enabled state, position, scale and opacity, with saved profiles and a clearly labelled editor preview. Customized values survive ordinary settings migrations. Treat missing observations independently from user-disabled modules.

Warning, revision, event and advice states follow the shared priority/audio contract. A healthy observation with low estimated risk is distinct from no coverage. The overlay must never turn unknown into a green reassurance. Verified event media has no invented image or speech substitute.

Normal rendering is passive. Animation should support state change, respect reduced motion, and stay within resource limits. Measure actual FPS and audio behavior; visual smoothness in a browser fixture is insufficient.

## 9. Keyboard, accessibility and copy

| Baseline shortcut | Action |
|---|---|
| Ctrl+Alt+S | Toggle gameplay overlay visibility |
| Ctrl+Alt+V | Hold for local push-to-talk; remappable |
| Alt+Up / Alt+Down | Adjust volume by ten percentage points |
| Alt+M | Toggle mute |
| Ctrl+K | Open control command palette |
| Ctrl+/ or ? | Open control shortcut help |
| Enter / Space | Activate the focused card, marker or action |
| Escape | Close current inline detail before its parent surface |

Scope local shortcuts to the control interface; register global shortcuts only where required. Detect conflicts and provide remapping without silently taking a game binding.

Use visible focus, accessible names, adequate contrast and status cues beyond color. Preserve Thai glyphs and line height. Test long labels, empty strings, localized numbers and zoom/DPI behavior. Screen readers announce meaningful selection/action results, not every countdown tick.

Suggested Thai state copy includes “รอเข้าเกม”, “ยังไม่มีข้อมูล”, “ข้อมูลเก่า”, “คาดว่าพร้อม” and “ดูเหตุการณ์”. Final wording may change while preserving the distinct meanings.

## 10. Explicit design work and evidence

| Open item | Required resolution |
|---|---|
| Product identity and visual assets | New name, mascot, logo, palette, typography and asset provenance |
| Final neutral screens | Annotated empty/live/stale/error/base/Pro mockups for affected destinations |
| Native safe areas | Measure actual window chrome, scaled content and game HUD clearance |
| Readability and targets | Set measured minimum typography/target sizes under supported scaling |
| Compact field dimensions | Measure revised compact cards and bounded inspector; keep map invariant |
| Local clips placement | Choose an existing destination and document entry/exit without silent navigation expansion |

Deliver deterministic interaction fixtures, keyboard checks, frozen-input geometry measurements and viewport captures. Keep native overlay/focus/audio/game/resource evidence separate. Every visual acceptance claim identifies the tested build and environment.

These open items do not authorize guessed live values or hidden scope changes. Record the decision, update the controlled baseline when required, and then implement the agreed interface.


## Related engineering diagrams

- [DGM-017 — Startup, navigation and explicit user tasks](../diagrams/INDEX.html#DGM-017)

Read each figure with its catalog notes and linked SRS requirements. Short labels do not replace failure, consent or cancellation guards. The [diagram index](../diagrams/README.md) provides editable sources and maintenance rules.
