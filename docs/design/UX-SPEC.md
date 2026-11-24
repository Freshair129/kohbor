---
title: "User Experience Specification"
doc_id: "UX-SPEC"
status: draft
version: "0.3.10"
updated: "2026-11-23"
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
| Control: Overview | Player data dashboard and Kohbor news (UI-020, DEC-OVERVIEW, variant D): own statistics from local history and, only after explicit opt-in, older own matches from OpenDota (MEM-006); full layout with zeros, empty frames and labelled placeholder heroes when no local data exists (DEC-OVERVIEW rev 3); source badge on every match row; news column with a type filter and in-page "all news" list. The OpenDota card and Settings disclose the 32-bit account ID sent to `api.opendota.com`, IP/time visibility and revoke/cache deletion before enabling; the switch defaults off. | No readiness or GSI content and no GSI/game status chips on this page; uncounted values are em dashes with the reason; OpenDota waiting-for-account, private, empty, not-found, rate-limited, offline and loading states remain explicit. If consent is enabled before a fresh verified Dota 2 account is available, the checkbox stays checked and the card tells the player to open Dota 2 until the account is verified, with `Expose Public Match Data` and retry for private profiles; failures show cached data and fetch time; local history stays available when OpenDota is off. |
| Control: Live | Default destination with the existing GSI setup, Dota launch status and Deck | Never fill live areas with demo statistics |
| Control: Voice Packs | Audio bar and three views: Installed packs, Sounds by event, and Last match | Listen uses the verified active pack and its overlay playback outside a match |
| Control: Store | Reserved destination; currently shows “ยังไม่มีหน้านี้” | No catalog, wallet or transaction service is available |
| Control: Analytics | Three-column post-match replay workspace; left analysis and Markdown documents, centre minimap and ten hero cards, right optional Claude chat; full Replay tools stay a subview | The route remains `insights`; unknown bundle/history values stay unknown; the assistant sends nothing without key, consent, AI enabled and explicit submit |
| Control: Status | Current readiness, own-hero, audio and build/version status panels | Unknown values remain unknown; GSI setup action opens this destination |
| Control: Logs | Local logs, raw audio decisions and technical pack information; requires Developer mode | When disabled, explain the setting needed to open it |
| Control: Account | Avatar flyout with the existing account page, settings, version and only supported sign-in/out actions | Presence stays unknown unless a real presence source exists; identity does not authorize game-data sharing |
| Control: Settings | Existing language, Developer mode, Steam-name display and connection guide controls, plus the news switch with its GitHub disclosure (UPDATE-005) | Bounded categories; explain unavailable capabilities |
| Gameplay overlay and HUD | Passive alerts, event media, selected stats, companion status and the draft pick card | Transparent, always-on-top, click-through and free of control-window chrome |
| Edge orb (extension) | Vertical three-channel mixer, Focus mode and honest status | Separate mouse-only window. Dota keeps keyboard focus |


### Analytics (UI-021, UI-022, MEM-007; DEC-ANALYTICS)

Analytics keeps the stable `insights` destination and renames the rail label to Analytics. The page uses three columns at the approved reference sizes. The left column is a document workspace with the read-only system summary first and saved AI Markdown notes in tabs. The centre column reuses the replay match viewer: map, Radiant and Dire five-card lineups sized to the map pane, hero-only kill score, playback bar, death ticks, cited-time markers, a nearby-deaths strip and a `View details` action for the existing Replay tools. Clicking a card selects a hero for the assistant; double-clicking lets the user identify their own hero. The right column is an IDE-style assistant with streaming text, removable context chips, cancellation, distinct errors and per-answer input/cache/output usage.

A replay dropdown has previous/next controls, filters for all, pinned and not analysed, a library-usage chip and a manual `.dem` picker. Each row displays status, local Thai date, lineups, result, own hero, duration, derived size and hero kills. Fields without evidence show `— ไม่ทราบ`, never zero. Empty, parsing, ready, note-open, no-AI, replay-list and settings views use actual library state. Parsing uses indeterminate progress unless the engine supplies a measured percentage. The map may not shrink below its tested minimum; collapsing either side panel gives the map more room. The full page has no page-level scroll at 1440x900 and 1920x1080.

Markdown is rendered without a new dependency: escape HTML, allow headings, paragraphs, lists, bold/italic, inline/fenced code, tables and blockquotes, and do not execute raw HTML. Only `#t=<seconds>` and `#ev=<table>/<index>` render as seek buttons; all other links render as plain text. System summary is read-only. AI notes are stored separately from evictable replay bundles; deleting a note requires confirmation.

The assistant is off by default and the Settings key field never echoes the key. Before the first request, show the exact outbound request content (derived selected-match context and hero names; scrubber time and selected hero when enabled; the question; all earlier messages in this same match's chat, resent with every question; and open-document text when included). The user's API key is sent directly to Anthropic over HTTPS only in the `x-api-key` authentication header; it is not prompt content or stored in logs or the local request ledger. Player names, Steam/account IDs, raw match IDs and file paths are excluded from request content. Key, consent and AI-on are all gates, in addition to explicit user submit; no analysis or replay-selection action sends a request. Consent revocation cancels outstanding work. Without AI, show the fixed local questions and answers from `coach.ts`, and state that nothing is sent. Settings expose the allowed Claude models, consent/revocation, storage limits, auto-analysis switch, key set/delete and metadata-only request ledger including input, cache-read, cache-creation and output token usage. Analytics preferences currently use a separate versioned `analytics/settings.json` store; migrating them into the shared settings store is a known follow-up.

Automatic analysis runs only when the runtime phase is none or menu and one job is active. The library keeps the newest 20 analysed bundles plus all `เก็บไว้` pins by default, within 2 GiB; Settings permit 5 to 100 unpinned matches and a size cap. Eviction removes derived bundle files only. Dota `.dem` files stay read-only, and an evicted match's notes/chat remain and show `replay removed`.

The owner-selected desktop shell uses a 72u left rail with icon-plus-Thai-label navigation: Overview, Live, Voice Packs, Insights and Store in the primary group; Status, Logs and Settings in the bottom group. The right rail contains only the profile avatar and its flyout; notifications, friends, party and quick-add controls are not rendered. Replay is an Insights subview, never a main destination. The page header keeps the global mute control; a thin status line below it carries GSI intake, game phase, data age, capture and current Deck status, with a GSI setup action that opens Status. The status line is hidden on the Overview page only (DEC-OVERVIEW item 5); every other page keeps it. Content sits on a raised Command Deck surface. The Pulse Grid rail ribbon is decorative, has no pointer interaction and never represents status; its default `--power` is 55%. The ribbon-off setting is a TODO until Settings has a lighting preference.

Shell details as built (recorded 2026-11-21 from the merged code; presentation only, no new requirement):
- **Left rail state:** collapsed (icon only) by default, expanded with Thai labels and section headers "เกม", "เมนูหลัก", "ระบบ". The toggle button and **Ctrl+B** switch it (ignored while a modal dialog is open); the state is remembered per renderer under `kohbor.rail-left`.
- **Game selector:** a card at the top of the rail shows Dota 2 and its connection state; its menu lists other games as visibly unsupported and not selectable. It never implies support for another game.
- **Glass:** Settings → "รูปลักษณ์ / กระจก" offers Off, Low, Medium (default) and High, remembered per renderer under `kohbor.glass-level`. Reduced transparency, forced colors or Off use solid surfaces. The in-game HUD keeps its own dark glass and ignores this setting.
- **Status footer:** a 28 px footer carries resource readouts; it never replaces the status line's honest states.
- **First-run connection prompt:** when the first runtime status says the Dota 2 GSI file is not installed and the user has not answered before, the control window asks once ("เชื่อมเกาะเบาะกับ Dota 2") and runs the existing GSI setup on one click. "Later" or Escape closes it; the setup card and Settings action stay. A configuration conflict defers to the setup card. The answer is remembered per renderer.

Use the normal keyboard tab order/roving rail focus and expose the active navigation entry with `aria-current="page"`. At 1920 × 1080, 1366 × 768 and 1280 × 720 the shell must not create horizontal scroll or cut Thai labels.

### Voice Packs (P1)

- The audio bar shares the header mute state and shows the selected pack, announcer volume and existing volume shortcuts. Installed packs opens by default. Arrow keys, Home and End move between the three tabs.
- Each card shows the pack display name, revision, status, distinct mapped events, voice variants and variants with banners. The cover is the pack's optional still image (PNG or WebP) from `presentation.cover` ([MEDIA-PACK-SPEC](MEDIA-PACK-SPEC.md), media-manifest 1.4.2), shown as an image; banners never supply it. A pack without a presentation cover shows its neutral initial and no problem badge. Only a declared cover that the host rejects (`COVER_INVALID`) or that fails to load is reported as a cover problem. The pack description comes from `presentation.description` and is shown in the details drawer, not on the card.
- Cards use 4, 3 and 2 columns at 1920 × 1080, 1366 × 768 and 1280 × 720 respectively. The tab panel and the details drawer bound scrolling. Details group the pack's bindings by game event, show Thai labels, tier, voice count, rotation and banner availability. Escape closes the drawer and focus returns to its Details button.
- Sounds by event is an inset grouped list (iOS-settings style): group label above each card, one name and one muted line per row, a disclosure chevron to show the voices and one round play button as the only call to action. Rows without voices show the reason and no play control; a row whose pack has voices keeps its play control even when the event detector is unavailable. Voice rows show "เสียงที่ N", duration and a play button; clip and variant ids stay out of visible text. Sounds by event uses the host's active event and tier map. An unreadable row alone displays unknown. Listen calls the host audition and reports the overlay banner outcome in words. Only the active pack is eligible, and audition is blocked during a match. A switch request uses the existing deferred activation policy when playback or the match holds the current pack.
- Last match shows recent sound outcomes for the available in-memory session. “Played” means the player received the sound for playback. The recap provides no match totals, game clock or persistence in this slice. Developer Logs holds the raw decisions and pack identifiers.

Build and History remain subviews, not extra main pages. Local clip controls require a clearly documented placement within this structure; a new main destination is a deliberate design decision. Do not inherit a prototype navigation bar as production routing.

This inventory describes the target information architecture, not a requirement to complete every service in the desktop MVP. A retained destination for deferred Store, wallet or account-security capabilities presents an honest unavailable or not-in-this-build state, with no simulated transactions or apparently working controls. Mark each destination's delivered, unavailable or deferred state in the build scope. Its navigation entry does not establish backend readiness, operational authorization or completed feature acceptance.

## 3. Fixed canvas baseline

The inherited desktop baseline uses an authored stage of **1420 × 760** and a **1280 × 720 panel-local canvas**. These are different coordinate spaces. Fit the authored stage to the available window while preserving its geometry; do not replace its dimensions with a reference screenshot's viewport.

Every page stays within the canvas. Overflow uses bounded regions, tabs or pagination. There is no page-level scrollbar on supported desktop sizes. Do not independently shrink text to squeeze additional content into a region.

Validate desktop references at 1280 × 720, 1440 × 900 and 1672 × 941, recording actual content bounds and DPI. These are reference windows, not a certified native support matrix. Small mobile/tablet prototypes do not establish a mobile product.

The native inner safe area, minimum readable scaled typography, minimum interactive target and final compact-card dimensions require measurement and a recorded decision. Do not invent measured values. If a proposed design cannot meet readability within the baseline, resolve that conflict before changing geometry.

## 4. Live field anatomy

The product owner redesigned Live on 2026-11-24 after reviewing the full-screen app (UI-002, UI-016; DEC-REPLAY-INTEGRATION item 10 and DEC-LIVE-SECOND-MONITOR). Use the replay-style hero field in every phase; the only exception is the pinned second-monitor arrangement while a match runs. Keep the global shell header and footer. Remove the event feed, sound/pack panel, GSI setup hint and other side panels from Live in every phase. GSI setup remains on Status, and sound/pack status remains on its own page. The Live page does not return to the old separate own-hero/list layout outside a match.

| Mode | Required layout | Unavailable behavior |
|---|---|---|
| Centred (second monitor off, all phases) | Radiant's five slot-ordered cards left; minimap pane centred; Dire's five slot-ordered cards right. Both team columns exactly match the minimap pane's height and split it into five equal rows. The centre strip shows the admitted GSI clock and phase, plus existing waiting/capture status when available. | Unknown values stay em dashes or dashed slots. Score and net worth are omitted because the player client does not observe them. If GSI does not expose a game clock, omit it until its source is admitted; never use a host clock or fabricate a value. Below the smallest legible size, show the existing window-too-small state. |
| Pinned (second monitor left/right) | The map panel stays on the inner edge. Five enemy cards form a row directly above it; our five cards form a row directly below it. Both rows share the map panel's width. The panel and rows are laid out from the Live box. | Keep all ten full cards visible or show the existing window-too-small state below the legible minimum. Never scroll or clip. |

### 4.0 Shared replay-structure hero cards (UI-002)

There is no separate own-hero card in any field phase. The own hero occupies its slot among the five cards on its team, with a subtle outline and a "You" chip only when its team and slot are known. All ten cards share the replay viewer's full card structure, in replay order: portrait and level; name; K/D/A, LH/DN and gold (GSI gold is not net worth); HP and mana bars; abilities with levels and cooldowns plus the buff-chip area; six items, backpack, TP and neutral; and STR/AGI/INT, DMG, REG, ARM, MR, AMP, AS, MS, EV, SR, LS, GPM and XPM cells. The other nine may use the same structure in a denser arrangement. Portraits and icons may scale; card text never drops below 12 px. The own hero's stat rows show labelled derived values wherever the module returns them from the own GSI hero ID, level and observed item IDs plus the installed-game stats pack. Each non-null estimate has a dotted underline and a tooltip listing its sources and the exact caveat "ไม่รวมผลชั่วคราว / without temporary effects"; null results remain "—", and allies/enemies remain unknown. If the game build is unavailable, show it as unknown with the stats-pack build and label the newest supported ruleset as a fallback.

The own card is filled only from GSI for directly observed values: level, alive/respawn, K/D/A, LH/DN, gold, HP, mana percentage, abilities, items, backpack, TP, neutral and admitted GPM/XPM. Derived attributes and stats use only the own GSI hero ID, level and observed item IDs plus the installed-game stats pack and supported rules; they are labelled estimates, not direct GSI readings. The current renderer has no admitted game-build field. When build is unknown, it labels the game build unknown, shows the data-pack build, and marks the newest supported rules (7.42) as a fallback. Unknown values remain em dashes or dashed slots, never zero. Ally identities come only from same-session Draft Bar memory; ally vitals/cooldowns and all ally stats remain unknown until a consented source is admitted. Enemy identity comes only from same-session draft memory; no unobserved enemy vitals, skills or stats are shown. Do not draw path traces. Missing icons use the runtime portrait or neutral initials tile; never substitute another hero's or item's icon.

The centred field uses equal-height team columns, each matching the full minimap pane height. Use it in none/menu, loading, draft, pregame, live, post-game, unknown and host-lost states, unless UI-016 pinned mode is active during a match. At supported sizes down to 1280×720, all five cards per side remain inside the box. There is no page scroll or internal scroll on Live in any phase. If the measured box cannot show every card legibly at 12 px text, use the existing window-too-small state. The map-pane header, toolbar and bottom status controls stay inside the pane and do not change card geometry.

The centred map pane's compact one-line header shows the title, capture status, frame age, frame-settings action and details action above the largest square map stage that fits the centre column. CAP/RX/PAINT rates remain in the global footer. With input frozen, changing card content cannot move or resize the minimap pane or stage.

**Peripheral salience (UI-008).** An admitted critical warning may switch the map frame and a labelled banner to critical state; a new sighting is marked once; any trail uses only previously admitted positions; routine updates never flash; reduced motion uses static emphasis. No motion comes from prediction or decay. Cooldown zero from an admitted peer source remains a value, not "ready".

### 4.1 Draft strip and column (UI-010, UI-012)

Preserve the existing draft strip and draft-column behavior in the control window. During HERO_SELECTION, STRATEGY_TIME and TEAM_SHOWCASE, the Live page may show ten top-bar slots (Radiant five, gap, Dire five) with identified names or an unknown reason. UI-012 supports only 1920×1080 at 100% scale. In centred mode, the existing UI-010 strip and UI-012 statistics column occupy the bounded centre region at the same height as the team columns; they do not take a separate page row or scroll. Draft imagery and identity results stay in memory for the current match only, never appear in the game overlay, logs, disk or network, and clear at match end, session change and app exit. Keep the draft strip/column bounded and non-scrolling; if the complete card field cannot fit legibly, use the window-too-small state.

### 4.2 Second-monitor mode (UI-016, UI-017)

A per-device setting is off by default or records whether monitor 2 is left or right of the main monitor. It applies only during draft, pregame or live with the host answering. Before/after a match, during an unknown phase or after host loss, show the centred layout and retain the setting. Keep the side selector keyboard-reachable on the Live page.

In pinned mode, Radiant/Dire identity follows observed team and slot data. The enemy five-card row is directly above the map pane, our five-card row directly below it, and both rows use the pane's full width. The minimap is the largest square that fits the pane, pinned to the inner edge and smoothly scaled. The map title, status and controls occupy the free area beside that square. Constraining five cards to the square width would leave roughly 100 px per card at 1920×1080 and less at 1280×720, below the 216 px measured card floor, so the full-width rows remain. A draft board may replace the map stage during draft (UI-019). Rune countdown overlays use only an admitted scheduler source (UI-018), never identify heroes and do not take clicks. The Live page has no event feed, quick-strip side panel, sound/pack panel, GSI hint box, free slot or internal/page scroll in any phase. The shell header/footer remain. At every supported size down to 1280×720, all ten cards and the map pane fit without clipping at 12 px minimum card text; below the smallest legible size show the existing window-too-small state. The map's geometry comes only from the measured Live box, not card content.

Only while this pinned mode is active does the navigation rail move to the outer edge and the friends rail collapse. The second-monitor preference and selector remain local to the device.

**Card provenance (UI-017).** The own hero's values come from GSI. Other ally and enemy identities come only from same-session Draft Bar memory. Ally HP/mana/cooldowns are unknown until consented peer sharing (PEER-003, PEER-010) is admitted; an ultimate-dot state remains unknown until its top-bar input is admitted. Enemy vitals, skills and stats remain unknown without their own admitted source. Ally HP is never read from the game's top bar. Do not show stuns or targeting.

**Rune countdowns (UI-018).** Rune markers and labels use the ALERT-008 scheduler only, show minutes while far and seconds within the configured lead time, have no Roshan/Tormentor respawn labels, and never take clicks.

**Draft board (UI-019).** While draft runs, the centre map panel may show the existing broadcast-style board instead of the minimap: Radiant then Dire, five slots each. Hero identity comes from same-session Draft Bar 5b; player names from Draft 5c appear only while the visible show-player-names setting is on and remain in memory only. Statistics come from the consented hero-statistics projection; mark estimated position and guessed enemy lane as such, and never show win chance. After draft, the panel returns to the minimap.

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
| Ctrl+Alt+X | Show or hide the control window (UI-013); on by default, remappable, borderless or windowed only |
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
