---
title: "User Experience Specification"
doc_id: "UX-SPEC"
status: draft
version: "0.3.7"
updated: "2026-11-21"
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
| Control: Overview | Reserved destination; currently shows “ยังไม่มีหน้านี้” | Overview content is pending design |
| Control: Live | Default destination with the existing GSI setup, Dota launch status and Deck | Never fill live areas with demo statistics |
| Control: Voice Packs | Audio bar and three views: Installed packs, Sounds by event, and Last match | Listen uses the verified active pack and its overlay playback outside a match |
| Control: Store | Reserved destination; currently shows “ยังไม่มีหน้านี้” | No catalog, wallet or transaction service is available |
| Control: Insights | Summary placeholder and Replay subview when that existing Lab/preview page is available | Replay remains inside Insights; no summary data is fabricated |
| Control: Status | Current readiness, own-hero, audio and build/version status panels | Unknown values remain unknown; GSI setup action opens this destination |
| Control: Logs | Local logs, raw audio decisions and technical pack information; requires Developer mode | When disabled, explain the setting needed to open it |
| Control: Account | Avatar flyout with the existing account page, settings, version and only supported sign-in/out actions | Presence stays unknown unless a real presence source exists; identity does not authorize game-data sharing |
| Control: Settings | Existing language, Developer mode, Steam-name display and connection guide controls | Bounded categories; explain unavailable capabilities |
| Gameplay overlay and HUD | Passive alerts, event media, selected stats, companion status and the draft pick card | Transparent, always-on-top, click-through and free of control-window chrome |
| Edge orb (extension) | Vertical three-channel mixer, Focus mode and honest status | Separate mouse-only window. Dota keeps keyboard focus |

The owner-selected desktop shell uses a 72u left rail with icon-plus-Thai-label navigation: Overview, Live, Voice Packs, Insights and Store in the primary group; Status, Logs and Settings in the bottom group. The right rail contains only the profile avatar and its flyout; notifications, friends, party and quick-add controls are not rendered. Replay is an Insights subview, never a main destination. The page header keeps the global mute control; a thin status line below it carries GSI intake, game phase, data age, capture and current Deck status, with a GSI setup action that opens Status. Content sits on a raised Command Deck surface. The Pulse Grid rail ribbon is decorative, has no pointer interaction and never represents status; its default `--power` is 55%. The ribbon-off setting is a TODO until Settings has a lighting preference.

Use the normal keyboard tab order/roving rail focus and expose the active navigation entry with `aria-current="page"`. At 1920 × 1080, 1366 × 768 and 1280 × 720 the shell must not create horizontal scroll or cut Thai labels.

### Voice Packs (P1)

- The audio bar shares the header mute state and shows the selected pack, announcer volume and existing volume shortcuts. Installed packs opens by default. Arrow keys, Home and End move between the three tabs.
- Each card shows the pack display name, revision, status, distinct mapped events, voice variants and variants with banners. The cover rests on an early frame of a silent WebM from that pack. The cover preference is Kill, Multi-kill (lowest tier), Streak (lowest tier), First blood, Match started, then descriptor order. The first visual variant of the selected binding supplies the frame. A neutral initial appears when a cover is unavailable. The current descriptor has no pack description.
- Cards use 4, 3 and 2 columns at 1920 × 1080, 1366 × 768 and 1280 × 720 respectively. The tab panel and the details drawer bound scrolling. Details group the pack's bindings by game event, show Thai labels, tier, voice count, rotation and banner availability. Escape closes the drawer and focus returns to its Details button.
- Sounds by event uses the host's active event and tier map. An unreadable row alone displays unknown. Listen calls the host audition and reports the overlay banner outcome in words. Only the active pack is eligible, and audition is blocked during a match. A switch request uses the existing deferred activation policy when playback or the match holds the current pack.
- Last match shows recent sound outcomes for the available in-memory session. “Played” means the player received the sound for playback. The recap provides no match totals, game clock or persistence in this slice. Developer Logs holds the raw decisions and pack identifiers.

Build and History remain subviews, not extra main pages. Local clip controls require a clearly documented placement within this structure; a new main destination is a deliberate design decision. Do not inherit a prototype navigation bar as production routing.

This inventory describes the target information architecture, not a requirement to complete every service in the desktop MVP. A retained destination for deferred Store, wallet or account-security capabilities presents an honest unavailable or not-in-this-build state, with no simulated transactions or apparently working controls. Mark each destination's delivered, unavailable or deferred state in the build scope. Its navigation entry does not establish backend readiness, operational authorization or completed feature acceptance.

## 3. Fixed canvas baseline

The inherited desktop baseline uses an authored stage of **1420 × 760** and a **1280 × 720 panel-local canvas**. These are different coordinate spaces. Fit the authored stage to the available window while preserving its geometry; do not replace its dimensions with a reference screenshot's viewport.

Every page stays within the canvas. Overflow uses bounded regions, tabs or pagination. There is no page-level scrollbar on supported desktop sizes. Do not independently shrink text to squeeze additional content into a region.

Validate desktop references at 1280 × 720, 1440 × 900 and 1672 × 941, recording actual content bounds and DPI. These are reference windows, not a certified native support matrix. Small mobile/tablet prototypes do not establish a mobile product.

The native inner safe area, minimum readable scaled typography, minimum interactive target and final compact-card dimensions require measurement and a recorded decision. Do not invent measured values. If a proposed design cannot meet readability within the baseline, resolve that conflict before changing geometry.

## 4. Live field anatomy

The product owner redesigned this destination on 2026-11-11 as a **second-screen session monitor** (DEC-SESSION-MONITOR). It replaces the inherited top/left/bottom/right topology:

| Region | Required content | Unavailable behavior |
|---|---|---|
| Center | Dominant region: enlarged admitted minimap with eligible markers, sighting trails and threat state | Explicit unavailable/Lite/stale state; stale capture dims and labels the map |
| Own-team column | Own hero first, then four stable allied slots; vitals, and abilities/cooldowns only under the separate abilities scope (PEER-010) | Not-shared or unknown allies stay compact; no fabricated vitals or skills |
| Enemy column | Five stable enemy slots with admitted visibility and age | Preserve slots; known identity and admitted visibility/age only |
| Event/inspector region | Recent events; the inspector opens as a drawer over this region | Neutral initially; opening it never moves the map |

**Peripheral salience (UI-008).** The monitor is read from the corner of the eye:

- An admitted critical warning switches the map frame and a labelled banner to the critical state.
- A new sighting is marked once.
- Sighting trails show only previously admitted positions.
- Routine updates never flash.
- No motion comes from prediction or decay.
- Reduced motion uses static emphasis.

The map is the first visual priority; missing-enemy information is second. Own hero is not repeated among the four allied slots. Cooldown zero on a shared ability is shown as a value, never as "ready".

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

The `overlay` and `hud` windows remain passive and click-through under UI-001. Every in-game surface preserves Dota's keyboard input and clears essential minimap, skill, shop and status regions. Use the shared typography and colors with lighter layout than the control window.

Retain the baseline arrangement: own-stat row near the top-left, risk indicator near top-center, event banner below the game's announcement area and companion presence near top-right. Exact native placement and media dimensions require measurement on the supported matrix. Screenshot-box geometry alone does not prove native banner clearance.

Provide per-module enabled state, position, scale and opacity, with saved profiles and a clearly labelled editor preview. Customized values survive ordinary settings migrations. Treat missing observations independently from user-disabled modules.

Warning, revision, event and advice states follow the shared priority/audio contract. A healthy observation with low estimated risk is distinct from no coverage. The overlay must never turn unknown into a green reassurance. Verified event media has no invented image or speech substitute.

**Edge orb (UI-009, UI-011).** Use one separate window at the left edge. The tucked disc is 80% scale; after pointer dwell it grows continuously to 100% over 1 s, then slides out and accepts clicks. Dwell is adjustable; reduced motion preserves the same timing. The outermost 8 px always pass clicks through. Keep Dota foreground and never capture its keyboard or Escape. Hide when disabled, outside a match, when Dota loses foreground, in exclusive fullscreen or when safe placement cannot be guaranteed. Keep the orb and panel outside `PROTECTED_REGIONS`. A qualified critical alert closes the panel and locks the orb in the same admission. The critical cue keeps its deadline. Each orb/panel click checks foreground before and after; activation closes the panel, restores click-through and hides the orb for the rest of the match.

The vertical mixer has Game, Kohbor and selected Music faders, each 0–100% with mute, plus Focus mode (20% default, adjustable, second activation restores prior music level). Optional Focus hotkey is off by default. Wheel over a fader steps 5% only while panel is open. Show missing/unreadable states honestly; each control has a keyboard-reachable deck twin. Settings has an off switch for the whole orb.

**Hero pick card (UI-010).** Display the card passively after the own pick during hero selection and strategy time. Hide before the pick and from team showcase onward. Use the owner's 1920×1080 selection and strategy screenshots to measure clearance from Dota's draft UI before placing the card. Show the own hero in v1. V2 waits for Draft Bar 5b and uses the local hero-only projection in [Draft Bar §4](DRAFT-BAR-SPEC.md). Other players' names stay in the deck. A repick or swap clears obsolete values immediately.

The control window owns the separate default-off public statistics switch and optional bracket choice. The card shows source, data window, fetch time and sample size alongside the state. Disclose unavailability or staleness per STATS-003. If an estimated draft lean is supported, label it as a statistic derived from heroes. Omit a win-chance percentage and the starting-items card.

Animation should support state change, respect reduced motion, and stay within resource limits. Measure actual FPS and audio behavior; visual smoothness in a browser fixture is insufficient.

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
