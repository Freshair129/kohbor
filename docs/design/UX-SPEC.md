---
title: "User Experience Specification"
doc_id: "UX-SPEC"
status: draft
version: "0.3.14"
updated: "2026-11-25"
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
| Control: Overview | Player data dashboard and Kohbor news (UI-020, DEC-OVERVIEW, variant D): own statistics from local history and, only after explicit opt-in, older own matches from OpenDota (MEM-006); full layout with zeros, empty frames and labelled placeholder heroes when no local data exists (DEC-OVERVIEW rev 3); best heroes for all time, this month and the local calendar day (win-rate ranking from at least three counted games, otherwise most played); a locally scrollable recent-match list with an in-page full-history dialog; and nemesis heroes aggregated only from exact-bundle local replay lane digests, ranked after three faced lanes per hero. Nemesis rows contain hero identities and lane counts only, never player names or account IDs; optional style tags use only attributes in the installed local hero stats pack. Source badge on every match row; news column with a type filter and in-page "all news" list. The OpenDota card and Settings disclose the 32-bit account ID sent to `api.opendota.com`, IP/time visibility and revoke/cache deletion before enabling; the switch defaults off. | No readiness or GSI content and no GSI/game status chips on this page; uncounted values are em dashes with the reason; no matches today shows an em dash and reason; missing or evicted replay digests do not count. OpenDota waiting-for-account, private, empty, not-found, rate-limited, offline and loading states remain explicit. If consent is enabled before a fresh verified Dota 2 account is available, the checkbox stays checked and the card tells the player to open Dota 2 until the account is verified, with `Expose Public Match Data` and retry for private profiles; failures show cached data and fetch time; local history stays available when OpenDota is off. |
| Control: Live | Default destination with the existing GSI setup, Dota launch status and Deck | Never fill live areas with demo statistics |
| Control: Voice Packs | Audio bar and three views: Installed packs, Sounds by event, and Last match | Listen uses the verified active pack and its overlay playback outside a match |
| Control: Store | In Developer mode, provides the Store front-end skeleton preview (catalog, pack detail, wallet/credits, real installed library, honest empty history); with Developer mode off, displays the honest "ยังไม่เปิด" state | No backend, no payments, no prices, no checkout and no network requests; all buy and top-up controls are disabled and labelled "ยังไม่เปิดขาย"; unknown values stay "—"; library displays only real installed packs |
| Control: Analytics | Three-column post-match workspace with a day-grouped match list and replay detail; left analysis and Markdown documents, centre minimap and ten hero cards, right optional multi-provider chat | The route remains `insights`; the JSON report UI and separate Replay tab are removed; unknown bundle/history values stay unknown; the assistant sends nothing without current-provider consent, required key, AI enabled and explicit submit |
| Control: Status | Current readiness, own-hero, audio and build/version status panels | Unknown values remain unknown; GSI setup action opens this destination |
| Control: Logs | Local logs, raw audio decisions and technical pack information; requires Developer mode | When disabled, explain the setting needed to open it |
| Control: Account | Avatar flyout with the existing account page, settings, version and only supported sign-in/out actions | Presence stays unknown unless a real presence source exists; identity does not authorize game-data sharing |
| Control: Settings | A left section navigation and one scrollable content area for General, Alerts & objectives, Overlay layout, Minimap/capture, Replay & AI, Hotkeys, Privacy/data, Developer, and Audio & controls; every existing Settings control appears in exactly one section | The content area is the only scroll container; section choice is remembered per renderer; unavailable capabilities stay explicit |
| Gameplay overlay and HUD | Passive alerts, event media, selected stats, companion status and the draft pick card | Transparent, always-on-top, click-through and free of control-window chrome |
| Edge orb (extension) | Vertical three-channel mixer, Focus mode and honest status | Separate mouse-only window. Dota keeps keyboard focus |

### Settings page (UI-015; UI-013; UI-020–022; UPDATE-005)

Settings uses a fixed left section navigation beside one content area. The navigation remains visible while the selected content scrolls; neither cards nor embedded tables scroll independently, and the Settings page does not introduce horizontal scrolling. The section order is General, Alerts & objectives, Overlay layout, Minimap/capture, Replay & AI, Hotkeys, Privacy/data, Developer, and Audio & controls. General contains language, Steam-name display and app glass appearance; Developer contains Developer mode; Alerts & objectives contains the five objective switches and lead times plus the optional local map-key diagnostic. Minimap/capture contains the existing minimap frame/detail controls and GSI connection guide. Replay & AI contains the existing Analytics preferences; its existing `auto_analyse` switch (default on) is labelled **“แยกรีเพลย์อัตโนมัติ” / “Parse replays automatically”**. Its help says parsing starts after the player leaves the match, one replay at a time, at low priority. Below it, a separate switch **“ดึงรีเพลย์อัตโนมัติผ่าน OpenDota” / “Fetch replays automatically via OpenDota”** (MEM-008, DEC-REPLAY-AUTOFETCH) defaults off; its consent text names the match ID and IP sent to api.opendota.com, the plain-HTTP download from Valve's replay host, that neither is operated by Kohbor, and that turning it off cancels pending work and deletes downloaded files. The post-game replay box reports only the just-finished match, never another match's newest replay; with no matching replay and fetch off, it shows the Dota 2 download hint and a link to Replay & AI. It keeps the MEM-008 fetch states. Hotkeys contains the show-app hotkey. Privacy/data contains news, own-history OpenDota consent and public hero-stat consent/preferences. Audio & controls contains the existing edge-orb and mixer controls.

The renderer remembers the selected section in local storage under `kohbor.settings-section`, with access wrapped so a blocked preference store cannot prevent Settings from opening. The initial section is Overlay layout. A layout-related entry point selects Overlay layout even when another section was last open; ordinary entry restores the last section.

Overlay layout leads with the objective-card editor. Its game preview fills the content width while retaining 16:9, and the objective card remains draggable and resizable with the existing scale controls and protected-area clamps. Full-screen and reset actions sit in a toolbar above the preview. These changes do not alter layout storage or commands.

Alerts & objectives uses one table row per objective with an enable switch, lead-time inputs 1 and 2, and the existing 0–2 values / 5–60 second hint. Empty inputs correspond to no saved pre-warning and are labelled as such; the spawn alert remains governed by the existing objective switch. Enable switches keep their immediate patch behavior. Lead-time edits share one section-level Save and are validated with the existing integer, range and duplicate rules before one settings patch. Storage keys and host commands remain unchanged.

All Settings text and controls use at least 12 px, with larger section headings. The layout targets 1440×900 and 1920×1080 without horizontal scrolling by constraining tables and allowing long labels to wrap. This is a CSS/source target, not a measured result; rendered content heights and horizontal overflow remain NOT_RUN until the lead's visual review because this worker is not permitted to launch the app or a browser.


### Analytics (UI-021, UI-022; DEC-ANALYTICS and DEC-ANALYTICS-V2)

Analytics retains the `insights` destination. Its shell header hosts one row of controls; the workspace places Radiant cards, a square centre replay map, and Dire cards beside a right pane with summary above the assistant. The four summary, compare, AI-report and notes tabs stay in the summary pane. Safe `#doc=` links select the referenced report or note there, and AI reports appear only after the user presses the generate button below the cost note. The timeline sits directly under the map; the replay stage has no lane strip, and kill scores stay in the team headers. Nearby deaths and cited-time markers remain available. The match list is a bounded, independently scrolling panel grouped by newest-first Thai calendar day, with all/today/7 days/30 days and single-day date filters, win/loss/all filters, and a filtered count. Each row shows time, own hero portrait and name, result, duration, replay state and open/parse/re-parse/remove actions; one `เพิ่มไฟล์ .dem` button sits beside the list. Unlinked rows derive own hero and result only when local account identity matches player IDs in the bundle; otherwise they show `— ไม่ทราบ`. The removed JSON report import is not part of this workflow. Chat history over its token budget offers an explicit local recap note and new conversation action. Pane sizes use the measured page and the target map sides documented in “Analytics layout redesign” below; the older fixed 423 px and 289 px chat widths no longer govern this layout.

Replay and Analytics share the six-row hero-card model: top, bars, abilities, items, stats and derived stats. Analytics uses the local stat-icon catalog with accessible names and tooltips, and its card scale stays at or above 85 percent. The shared 13-field setting removes disabled cells and reflows the remaining cells without empty gaps; non-selected cards may use compact density while the selected hero retains every enabled row. The 12 px minimum applies outside hero cards; smaller type inside the card follows the approved reference. GPM/XPM remain Live-only until replay data provides them. Missing permanent buffs, auras and stack counts remain unknown with the approved note for bundles older than schema 3.2 or where fields are unknown; the renderer does not derive Duel-win or tower buffs. The card has no hero-damage cell until the replay engine provides that value. Missing or unavailable values render “—”, never zero or a fabricated measurement. Live will consume the shared preference after the OV-owned card layout merges.

The right IDE-style assistant displays the complete user/assistant transcript from `chat.json`; each turn, including partial streaming output, goes through the safe Markdown renderer. The “ผู้ช่วยเห็น:” context-chip row is removed. Only `#t=<seconds>` and `#ev=<table>/<index>` render as seek actions. Safe `#doc=<id>` links open the referenced document in the left panel. HTTPS links render a copy-URL action and cannot navigate or contact a remote host. Raw HTML is escaped; nested lists, task boxes, horizontal rules, strikethrough, aligned tables, hard breaks and labelled/copyable code are supported.

A fenced `kohbor-chart` block is parsed and validated before Kohbor draws a built-in SVG bar, line, area or timeline chart. Limits are 8 KiB per block, six series and 120 points per series; numbers must be finite, `null` is a gap, references resolve only from the selected match, labels are escaped and truncated, and time-axis events seek the replay. Invalid charts appear as code with a one-line validation reason. No Mermaid renderer or dependency is added.

The optional assistant remains off by default and requires explicit submit, current provider/model, current-origin consent and a key when required. Every request sends assistant rules/tools, the complete locally computed match digest (all ten heroes, phases, lanes, deaths, fights and objectives), viewer facts, saved-note index and any stored AI report, replay clock, selected hero, question and previous same-match user/assistant turns. The digest cache lasts one hour. Other match details are read on demand through bounded local read-only tools. An open document is never inlined. The consent screen describes the recipient, host, always-sent content and local tools and is shown once again for the updated consent text. The one-time AI report is created only after a user click, includes a cost note and uses the Settings model (default `claude-opus-5-5`). Player names, Steam/account IDs, raw match IDs and file paths are excluded. Provider, endpoint, key, routing, match-phase, failure and explicit note-save rules remain those in DEC-ANALYTICS and DEC-AI-PROVIDERS; usage and unknown context remain explicitly labelled unknown.

The Analytics match list replaces the old replay picker and JSON report sub-tab. It groups rows by Thai match day, applies date and result filters, shows the filtered count and scrolls inside its bounded panel. A single manual `.dem` picker and per-row open, parse, re-parse and remove actions stay beside those matches. Match identity from a bundle is used only when its player account ID matches the locally known account; absent identity remains `— ไม่ทราบ`. Empty, parsing, ready, note-open, no-AI and settings states use actual library state. Parsing is indeterminate without an engine percentage.
### Voice Packs (P1)

- The audio bar shares the header mute state and shows the selected pack, announcer volume and existing volume shortcuts. Installed packs opens by default. Arrow keys, Home and End move between the three tabs.
- Each card shows the pack display name, revision, status, distinct mapped events, voice variants and variants with banners. The cover is the pack's optional still image (PNG or WebP) from `presentation.cover` ([MEDIA-PACK-SPEC](MEDIA-PACK-SPEC.md), media-manifest 1.4.2), shown as an image; banners never supply it. A pack without a presentation cover shows its neutral initial and no problem badge. Only a declared cover that the host rejects (`COVER_INVALID`) or that fails to load is reported as a cover problem. The pack description comes from `presentation.description` and is shown in the details drawer, not on the card.
- Cards use 4, 3 and 2 columns at 1920 × 1080, 1366 × 768 and 1280 × 720 respectively. The tab panel and the details drawer bound scrolling. Details group the pack's bindings by game event, show Thai labels, tier, voice count, rotation and banner availability. Escape closes the drawer and focus returns to its Details button.
- Sounds by event is an inset grouped list (iOS-settings style): group label above each card, one name and one muted line per row, a disclosure chevron to show the voices and one round play button as the only call to action. Rows without voices show the reason and no play control; a row whose pack has voices keeps its play control even when the event detector is unavailable. Voice rows show "เสียงที่ N", duration and a play button; clip and variant ids stay out of visible text. Sounds by event uses the host's active event and tier map. An unreadable row alone displays unknown. Listen calls the host audition and reports the overlay banner outcome in words. Only the active pack is eligible, and audition is blocked during a match. A switch request uses the existing deferred activation policy when playback or the match holds the current pack.
- Last match shows recent sound outcomes for the available in-memory session. “Played” means the player received the sound for playback. The recap provides no match totals, game clock or persistence in this slice. Developer Logs holds the raw decisions and pack identifiers.

Build and History remain subviews, not extra main pages. Local clip controls require a clearly documented placement within this structure; a new main destination is a deliberate design decision. Do not inherit a prototype navigation bar as production routing.

### Store (UI-023; DEC-COMMERCE-INTENT amendment 2026-11-24)

The Store destination (`store`) provides the front-end skeleton preview when Developer mode is enabled, and retains the honest "ยังไม่เปิด" (not open yet) state when Developer mode is off. In preview mode, a prominent banner across the top states "ตัวอย่างการออกแบบ — ยังไม่ใช่สินค้าจริง ยังขายไม่ได้" (design preview — not real products, not for sale yet). The destination contains five core views: (1) **Catalog:** a grid of mock cards, each marked with a "ตัวอย่าง" badge, generic names (e.g. "แพ็กเสียงตัวอย่าง A"), abstract SVG covers, sample audition buttons, and disabled buy buttons labelled "ยังไม่เปิดขาย" with price "— ยังไม่กำหนดราคา", filterable by All, Announcer packs, and AI credits; (2) **Pack detail:** a drawer/view with large abstract cover, description, sample list, compatibility status (Kohbor version and signature check), and a disabled "ยังไม่เปิดขาย" buy button; (3) **Wallet / Kohbor Credits:** wallet balance showing "—", top-up bundle shapes (150 / 300 / 500 THB) with disabled "ยังไม่เปิดขาย" controls, and a note explaining how unified credits will be used for packs and AI inference; (4) **Library (owned):** truthful list of announcer packs actually installed on the user's PC using real data from the local media registry, displayed as "ติดตั้งแล้ว", never fabricating or simulating ownership; (5) **Purchase history:** an honest empty state stating no orders or transactions exist yet. No backend, no payment code, no real prices, no checkout, and no network requests exist. Audition of pack samples is disabled during an active match to protect audio clarity. Layout fits 1440×900 and 1920×1080 without page-level horizontal scroll, uses minimum 12 px text, and bounds scrollbars to the content area only.

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
| Centred (second monitor off, all phases) | Radiant's five slot-ordered cards left; map pane centred; Dire's five slot-ordered cards right. Both team columns exactly match the pane's height and split it into five equal rows. The map-pane header is a scoreboard: Radiant score left, signed GSI clock centred, Dire score right, with kill momentum below. | Unknown values stay em dashes or dashed slots. Momentum uses only current-match score samples in memory, stays neutral and empty until a full fresh three-minute baseline exists, and is never labelled as win probability. The one shell status line carries GSI/phase/capture state; no strip or repeated status appears above the map or in its title row. The map centre shows one short state line only when no image exists. Below the smallest legible size, show the existing window-too-small state. |
| Pinned (second monitor left/right) | The map panel stays on the inner edge. Five enemy cards form a row directly above it; our five cards form a row directly below it. Both rows share the map panel's width. The panel and rows are laid out from the Live box, and the same scoreboard header fits inside the map panel. | Keep all ten full cards visible or show the existing window-too-small state below the legible minimum. Never scroll or clip. Missing scores, clock or momentum stay unknown. |

### 4.0 Shared replay-structure hero cards (UI-002)

There is no separate own-hero card in any field phase. The own hero occupies its slot among the five cards on its team, with a subtle outline and a "You" chip only when its team and slot are known. All ten cards share the replay viewer's full card structure, in replay order: portrait and level; name; K/D/A, LH/DN and gold (GSI gold is not net worth); HP and mana bars; abilities with levels and cooldowns plus the buff-chip area; six items, backpack, TP and neutral; and STR/AGI/INT, DMG, REG, ARM, MR, AMP, AS, MS, EV, SR, LS, GPM and XPM cells. The other nine may use the same structure in a denser arrangement. Stat abbreviations are represented by the corresponding Dota stat icons where available; original SVGs cover GPM, XPM and stats without a game icon. Every icon keeps its Thai/English stat name in its accessible label and tooltip. Numeric values and the unknown dash do not change, and stat icons stay at the existing 12 px text height. Portraits and icons may scale; card text never drops below 12 px. The own hero's stat rows show labelled derived values wherever the module returns them from the own GSI hero ID, level and observed item IDs plus the installed-game stats pack. Each non-null estimate has a dotted underline and a tooltip listing its sources and the exact caveat "ไม่รวมผลชั่วคราว / without temporary effects"; null results remain "—", and allies/enemies remain unknown. If the game build is unavailable, show it as unknown with the stats-pack build and label the newest supported ruleset as a fallback.

The compact second-monitor selector remains a three-button segmented control. Its choices use single- or dual-monitor glyphs; the active monitor is accented for the left/right choices. Each icon button keeps its Thai/English accessible name and tooltip, pressed state and keyboard operation.

The own card is filled only from GSI for directly observed values: level, alive/respawn, K/D/A, LH/DN, gold, HP, mana percentage, abilities, items, backpack, TP, neutral and admitted GPM/XPM. Derived attributes and stats use only the own GSI hero ID, level and observed item IDs plus the installed-game stats pack and supported rules; they are labelled estimates, not direct GSI readings. The current renderer has no admitted game-build field. When build is unknown, it labels the game build unknown, shows the data-pack build, and marks the newest supported rules (7.42) as a fallback. Unknown values remain em dashes or dashed slots, never zero. Ally identities come only from same-session Draft Bar memory; ally vitals/cooldowns and all ally stats remain unknown until a consented source is admitted. Enemy identity comes only from same-session draft memory; no unobserved enemy vitals, skills or stats are shown. Do not draw path traces. Missing icons use the runtime portrait or neutral initials tile; never substitute another hero's or item's icon.

In the own hero's compact field card, keep the eight displayed stat cells ARM, MR, AMP, AS, MS, EV, SR and LS in two rows of four when the measured mode is `one-row-28` or `two-row-24`. `one-row-28` starts at a 467 px card width and can use the 2×4 grid at its 102 px minimum height. `two-row-24` requires at least 254 px width and 124 px height. In the narrower `two-row-22` mode, keep the complete existing 5-column by 3-row stat grid so no values disappear. This compact own grid omits REG because the HP and mana bars carry the same derived regeneration values. Other cards keep their own layouts and unknown states. The own compact card top-aligns to its existing team slot and uses its intrinsic height; the five-slot rows, map geometry and other cards keep their viewport-derived sizes.

Show HP and mana regeneration in the own hero's HP and mana bars, right-aligned and formatted to one decimal with a leading `+`; reuse the existing formatter's omission of a trailing `.0`. A known fresh zero is `+0`. If the GSI snapshot or item inputs are stale, or the derived model returns no value, show no regeneration text. The bar tooltip gives the Thai or English per-second value, marks it as a derived estimate, lists its sources and ruleset basis, and keeps the “without temporary effects / ไม่รวมผลชั่วคราว” caveat. Allies and enemies never receive these values.

The centred field uses equal-height team columns, each matching the full map pane height. Use it in none/menu, loading, draft, pregame, live, post-game, unknown and host-lost states, unless UI-016 pinned mode is active during a match. At supported sizes down to 1280×720, all five cards per side remain inside the box. There is no page scroll or internal scroll on Live in any phase. If the measured box cannot show every card legibly at 12 px text, use the existing window-too-small state. The map-pane scoreboard stays inside the pane and does not change card geometry.

The map-pane header is a compact replay-style scoreboard: Radiant's separate observed score on the left, the signed game clock in the centre (mm:ss, negative before the horn), Dire's score on the right, and a bar beneath labelled "โมเมนตัมคิล (3 นาทีล่าสุด)" / "Kill momentum (last 3 min)". The bar shows only the change in observed team kills from score samples in memory for the current match; its tooltip says it is not win probability. Keep it neutral and empty until a full fresh three-minute baseline exists. Unknown score and clock fields render as em dashes. The scoreboard has no map title or capture/status text. GSI, phase, age and capture status stay in the single shell status line; CAP/RX/PAINT rates remain in the global footer. A no-image minimap shows one short state line in its centre, and shows no state message over a map image. "ตั้งค่ากรอบ" and "ละเอียด" live in Settings under the minimap/capture section, never in the Live map pane. With input frozen, changing card content cannot move or resize the map pane.

### 4.0.1 Before and after a match

When no match is running, replace the empty map stage with a local match-history dashboard. Show today's and yesterday's match counts, W/L and win rate; use the Overview all-modes label when the local history has unknown match types. Show the last local match's hero, result, K/D/A and duration, and the best hero this week when the source supports it. Empty history says "ยังไม่มีแมตช์ที่บันทึก" and unavailable history stays explicitly unavailable; no example rows or fabricated statistics appear. Keep a single short Dota state line and the Start Dota 2 action while Dota is closed.

When GSI reports post_game or the match has just ended, replace the centre map with a post-game summary using an approximately 600 ms flip/door transition; `prefers-reduced-motion` swaps instantly. Keep it until a new match starts or the user closes it. Per DEC-POSTGAME-BREAKDOWN the summary has a result banner (result, duration, mode and patch when admitted, final score, match ID with a copy action) and a hero header (own hero, level, K/D/A, LH/DN, GPM, XPM, net worth, with hero-benchmark comparison only while the OpenDota benchmark opt-in is on) above Summary, Timeline and Replay tabs; Stage 1 adds the final inventory, derived end statistics and, in Timeline, the full match recap of alerts and announcer events. After match end the ten hero cards keep their last observed state with a 'ค่าสุดท้าย / Final' mark: the own card from host-recorded end values, the others with draft-roster heroes and unknown values. Every missing or stale value is an em dash. Stage 2 appears only for a ready replay bundle whose install-keyed match hash matches local history; it fills all ten cards from the digest, embeds the UI-023 summary blocks, and opens Analytics on that match. The Analytics “ดูละเอียด” action opens the heavy viewer for that same already-loaded bundle at the current scrub time, including its heatmap, death rhythm, item simulator and timeline. Its “‹ กลับไป Analytics” control restores the selected match, time and open document. The manual JSON report import remains in its explicit Replay report route and is not the landing state for Analytics details. When no ready bundle is linked, explain the replay-folder watch or adding a `.dem` file. The raw match ID is displayed only in the post-game summary (from the linked bundle or the current session's in-memory GSI value) and is never logged or persisted from GSI.

**Peripheral salience (UI-008).** An admitted critical warning may switch the map frame and a labelled banner to critical state; a new sighting is marked once; any trail uses only previously admitted positions; routine updates never flash; reduced motion uses static emphasis. No motion comes from prediction or decay. Cooldown zero from an admitted peer source remains a value, not "ready".

### 4.1 Draft strip and column (UI-010, UI-012)

Preserve the existing draft strip and draft-column behavior in the control window. During HERO_SELECTION, STRATEGY_TIME and TEAM_SHOWCASE, the Live page may show ten top-bar slots (Radiant five, gap, Dire five) with identified names or an unknown reason. UI-012 supports only 1920×1080 at 100% scale. In centred mode, the existing UI-010 strip and UI-012 statistics column occupy the bounded centre region at the same height as the team columns; they do not take a separate page row or scroll. Draft imagery and identity results stay in memory for the current match only, never appear in the game overlay, logs, disk or network, and clear at match end, session change and app exit. Keep the draft strip/column bounded and non-scrolling; if the complete card field cannot fit legibly, use the window-too-small state.

### 4.2 Second-monitor mode (UI-016, UI-017)

A per-device setting is off by default or records whether monitor 2 is left or right of the main monitor. It applies only during draft, pregame or live with the host answering. Before/after a match, during an unknown phase or after host loss, show the centred layout and retain the setting. Keep the side selector keyboard-reachable on the Live page.

In pinned mode, Radiant/Dire identity follows observed team and slot data. The enemy five-card row is directly above the map pane, our five-card row directly below it, and both rows use the pane's full width. The minimap is the largest square that fits the pane, pinned to the inner edge and smoothly scaled. The map-pane scoreboard occupies its header; the map has no repeated phase/status title, and frame setup/details remain in Settings. Constraining five cards to the square width would leave roughly 100 px per card at 1920×1080 and less at 1280×720, below the 216 px measured card floor, so the full-width rows remain. A draft board may replace the map stage during draft (UI-019). Rune countdown overlays use only an admitted scheduler source (UI-018), never identify heroes and do not take clicks. In live phase only, the map pane's outer area holds five priority panels: objective timers, own-hero HP loss log, agent orb, own farm graph and team kill timeline. Keep every panel in memory for the current match only; the damage rows omit healing while the HP sparkline keeps observed gains, and no attacker is inferred. The kill timeline shares the exact score samples used by the scoreboard and supplements the existing three-minute momentum bar. The orb has no input and never says it is listening; animate it to Kohbor's own playback, using an actual output level only if the mixer exposes one. The current mixer has no output-level meter, so its visible playback animation is activity only. Do not build STT/TTS or network voice; voice waits for the local assistant memory measurement. Timers reuse ALERT-008 rules with the patch label each rule records, and use only the admitted GSI game clock; they do not infer an installed-game patch mapping. Day/night state uses fresh admitted GSI values and its next-switch timer remains — ไม่ทราบ until a cycle rule exists. The map overlay shows last-minute seconds and a brief พร้อม only at known normalised coordinates for power rune, bounty, wisdom and lotus. Unknown positions are omitted. Roshan pit and side-switch information requires a source that identifies the pit; roshan_state and roshan_respawn_seconds alone do not, so the map shows — ไม่ทราบ without a guessed pit marker. Drop panels from lowest priority upward as the outer box shrinks, with no scrolling or overlap and text at least 12 px. At 1920×1080 all five fit; the pinned-panel geometry model retains three at 1600×900 and two at 1440×900 on either edge. These viewport results are synthetic geometry checks; the lead owns the native render. The Live page has no event feed, sound/pack panel or GSI hint box in any phase, and the pinned outer panels do not add a separate Live side rail. The shell header/footer remain. At every supported size down to 1280×720, all ten cards and the map pane fit without clipping at 12 px minimum card text; below the smallest legible size show the existing window-too-small state. The map's geometry comes only from the measured Live box, not card or panel content.

Only while this pinned mode is active does the navigation rail move to the outer edge and the friends rail collapse. The second-monitor preference and selector remain local to the device.

**Card provenance (UI-017).** The own hero's values come from GSI. Other ally and enemy identities come only from same-session Draft Bar memory. Ally HP/mana/cooldowns are unknown until consented peer sharing (PEER-003, PEER-010) is admitted; an ultimate-dot state remains unknown until its top-bar input is admitted. Enemy vitals, skills and stats remain unknown without their own admitted source. Ally HP is never read from the game's top bar. Do not show stuns or targeting.

**Objective countdowns and Roshan (UI-018).** The pinned map shows seconds only in the last 60 game-clock seconds, then “พร้อม” briefly, at replay-derived normalised coordinates for power rune, bounty, wisdom and lotus. Timing reuses the ALERT-008 schedule table without changing alerts; every table-derived timer shows its recorded patch label and makes no claim that the installed game uses that patch. All time advances only from admitted GSI `map.clock_time`. Replay positions use rune-event world coordinates and the replay viewer's manifest transform; each coordinate carries source patch/build provenance. The three local bundles provide power/bounty coordinates from 7.41f, with the Dota build unrecorded; wisdom/lotus positions remain omitted because the bundles have no corresponding events. Day/night state uses only fresh admitted GSI `map.daytime` or `nightstalker_night`; the next switch is “— ไม่ทราบ” because no cycle rule exists. Roshan respawn remains unknown unless a fresh admitted `roshan_respawn_seconds` or verified respawn window supplies it. A `roshan_state` or respawn-seconds value does not identify a pit, so pit/side-switch stay “— ไม่ทราบ” unless a source identifies them; the manual death control stays absent until a verified respawn window exists. No renderer-owned spawn table or wall-clock extrapolation is allowed. Map overlays remain pointer-inert and appear only over the captured minimap while pinned mode shows the map in pregame or live; they are absent during the draft-board stage.

**Pinned live panels (UI-016).** The outer map area prioritises objectives, own damage, agent orb, own farm and team kill timeline. The HP log is newest-first, merges drops within 500 ms, says the source is unknown, omits gain rows and keeps a 60-second HP sparkline. The farm graph uses GPM, XPM and last hits from own GSI samples and marks LH@10 only after the game clock passes ten minutes. The kill timeline reads the same memory-only Radiant/Dire score samples as the scoreboard. Objective timers reuse the ALERT-008 rules, show each rule's patch label, and keep missing rules or GSI fields at “— ไม่ทราบ”; the UI never guesses an installed-game patch mapping. Panels do not scroll, overlap or use text below 12 px; hide lower-priority panels first. The orb has a slow idle breath, moves on Kohbor playback only, has no microphone/input/listening state, honors reduced motion and stops animating when the window is hidden. Output amplitude is shown only if a real mixer meter exists; none is exposed today, so the playback animation is not a level measurement. Speech-to-speech remains deferred to the local voice-assistant memory measurement.

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


## UX review amendments (2026-11-25, DEC-UX-REVIEW-20261125)

### Post-game Live panel (UI-002)

The host retains the last finished match record (epoch, install-keyed match hash, observed end values and ended time) across the phase returning to none and application restart. Live associates the panel with that exact record, keeps its epoch visible in none/menu, and provides a reopen action until the next draft. A hidden window can surface or reach the panel after match end. The panel has three steps: recap; download the replay in Dota while showing watcher and parse state, plus manual `.dem` selection; and analysis only after a validated bundle matches the same recorded match. Kohbor explains the Dota step and never clicks in the game. If watcher status is unavailable, show unknown. Do not infer win/loss from kills; unknown says "— ไม่ทราบ". Format the score as "Radiant 33 : 23 Dire" using admitted scores and localize recap event rows.

### Rune chips (UI-018, D.4)

On the map, show only a rune mark and time until its alert lead threshold; use the full chip only within the configured lead time, or in the final ten seconds when no user lead is set. Distinguish rune types by shape. Put names in the legend below the map. Use a dashed ring when the coordinate source patch differs from the schedule patch, and do not claim that either patch is the installed game patch. Unknown coordinates stay omitted. The current W1 overlay can place the legend at the bottom of the map overlay; placing it physically below the captured map needs the map wrapper's out-of-scope layout files.

### Analytics review (UI-021, UI-022)

Use measured page width and four tabs: "สรุป", "ทียบ", "AI" and "โน้ต". Show score in one matchup row. Reading mode widens the left panel while chat remains 423 px at 1920×1080 and 289 px at 1440×900. Coach Review belongs in Lab or Developer mode. "ค่าเฉลี่ยของคุณ" is visible only on screen and is not sent in an AI digest or request. Chat, notes and reports remain deletable per match when its replay is absent. The untested-provider toggle is Developer-only; other users see AI unavailable in this release with its chat collapsed.

### Replay states and retention (MEM-007)

Interrupted automatic jobs and jobs stopped by a match start return to Available and queue again when the barrier clears; actual parse failures remain manual. Queue order and eviction use match end time, and evicted matches remain as lightweight rows. Retention settings apply only after Save and a preview of the number of rows to remove. Use "แยกรีเพลย์" for parsing states and reserve "วิเคราะห์" for analysis and AI.

### Analytics layout redesign (UI-021, DEC-ANALYTICS-LAYOUT)

The Analytics shell header hosts one 48 px control row. Picker segments disappear as whole fields in the approved order; remaining controls move into More, and accessible names retain the full label. The measured workspace has Radiant and Dire cards around a square replay map, with the timeline immediately below the map. Kill scores sit in team headers and the old lane strip is removed. The right pane contains summary above the assistant and can collapse to a rail.

Pane defaults and applied sizes follow the measured page-width size class. Keep user preferences separately per class, clamp applied values to leave a usable map, collapse to reversible rails when required, and reset only the active class. H1/H2 resize the two card columns together, H3 resizes the side pane and H4 resizes summary versus assistant; keyboard and pointer operate each handle. Summary and assistant cannot both be collapsed. The centre map is at least 300 px when the available window permits it.

At 1366×768, 1440×900, 1920×1080, 2560×1440 and 1920×1080 at 125 percent scaling, pure pane math targets map sides of 374, 448, 640, 1,068 and 440 px respectively. These are model checks, not rendered measurements. Compact density hides secondary rows on non-selected heroes; the selected hero retains each enabled row. Card scale stays at least 85 percent before the roster-strip fallback. That scale exception applies to card content only.

Stat labels use the local stat-icon catalog with a readable accessible name and tooltip. The status tray displays only replay-bundle modifiers, ordered with buffs left and urgent debuffs/control at the right. Control effects are never dropped from the visible set; additional effects appear under “+N”. Rings and seconds reflect bundle time values. For bundles older than schema 3.2, or when those fields are unknown, permanent buffs, auras and stack counts remain unknown; show “บัฟถาวร ออร่า และจำนวนสแต็ก ยังไม่มีในข้อมูลรีเพลย์” and do not infer Duel-win or tower buffs.

The shared hero-card setting has 13 native checkboxes, all enabled initially. A hidden cell is removed, empty rows disappear, and remaining cells reflow without placeholders. Settings is the canonical home; Analytics quick access uses the same immediate-write controls. Live must consume the shared store after the OV card layout merges. “GPM / XPM” remains Live-only and status effects remain Replay-only until a Live data source exists.

Selecting text offers “ถาม AI เรื่องนี้”, “ไปที่ mm:ss” only for a known timestamp, and “จดโน้ต”. Ask adds one source/time quote chip to the composer without changing tabs. Each selected passage saved as a note creates exactly one note. The shortcuts are `Ctrl+\`, `Ctrl+Shift+\`, `Ctrl+J`, `Ctrl+0`, `F6`/`Shift+F6`, `I`, and `Ctrl+Shift+A`/`Ctrl+Shift+N`; this implementation uses `Ctrl+Alt+0` while packaged Windows WebView shortcut handling remains unverified.
