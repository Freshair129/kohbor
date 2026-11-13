---
title: "Visual Design Proposal — Command Deck and Overlay"
doc_id: "VISUAL-DESIGN-PROPOSAL"
status: draft
version: "0.3.6"
updated: "2026-11-11"
authority: "DEC-BASELINE; proposal under DEC-VISUALS; elaborates UX-SPEC for UI-001..UI-006"
owner_role: "Product designer (builder-delegated proposal); product owner reviews"
approval_status: "direction_approved_by_owner_2026-11-11"
---

# Visual Design Proposal — Command Deck and Overlay

## 1. Status

This is the first proposal under **DEC-VISUALS**. The [UX Specification](UX-SPEC.md) defines behavior and layout; this proposal adds the visual design on top. It does not change any requirement or UX-SPEC rule.

The artifacts are:

- the design tokens in [design-tokens.json](../../contracts/design-tokens.json);
- four annotated artboards in [visuals/](visuals/).

Values that need native measurement are marked as such.

**Owner decision (2026-11-11): the visual direction is approved.** The approval covers the companion mark, the dark-first palette and its no-"safe"-green rule, IBM Plex Sans Thai/Mono, the honest-state encodings, the Command Deck/overlay layout direction and the working name "Command Deck". It does **not** approve the measured values (`needs_measurement`), the screens still to be designed, or native/visual acceptance. Nothing has been measured on Windows or in a live game.

"Command Deck" is the working name for the control window (UX-SPEC §2). The rename is presentation only; destinations and behavior are unchanged.

| Artboard | Source | Shows |
| --- | --- | --- |
| Design tokens and data states | [Main.dc.html](visuals/Main.dc.html) | Mark, palette, type scale, spacing/radii, the six honest data states, critical/warning/no-coverage banners |
| Command Deck — Overview, waiting | [Overview.dc.html](visuals/Overview.dc.html) | Seven-destination rail; header; independent readiness rows; empty match card; audio with the mute note; companion |
| Command Deck — Session Monitor (second screen), sample data | [Live.dc.html](visuals/Live.dc.html) | Enlarged minimap with sighting trails and critical frame; own-team cards with shared skills/cooldowns; enemy cards; recent events |
| Overlay in game, sample data | [Overlay.dc.html](visuals/Overlay.dc.html) | 1920×1080 reference with no-cover HUD zones and five overlay modules |

The `.dc.html` files are design-canvas sources. They render in the canvas editor, not in the application. [canvas.json](visuals/canvas.json) records the board layout and its annotation notes.

## 2. Identity

- **Mark and companion.** An original "cushion" companion (เกาะเบาะ): a rounded seat shape with a back arc, eyes and two small clinging arms, drawn as one-colour stroke SVG. It uses no source-brand or game assets (AGENTS.md identity rule).
- **Hero identity.** Placeholders are initials (E1–E5, A1–A4) in team colour. No game portraits are used.
- **Look.** Dark-first, low-chroma surfaces with one teal accent. It is chosen to sit quietly next to the game and to keep the overlay legible over varied scenes.

## 3. Tokens (summary of design-tokens.json)

| Group | Values |
| --- | --- |
| Surfaces | ground `#0E1116`, surface `#161B22`, raised `#1D2430`, line `#2A3341` |
| Text | `#E6EAF0`, secondary `#C3CBD6`, muted `#9AA6B5` |
| Accent | `#3CC8B4` (product), surface `#12302C` |
| Teams | ally `#6CA8FF`, enemy `#FF8A4C` |
| Severity | critical `#FF5A4A` with `#1A0B08` text; warning `#F2B544` on `#2B2414`; stale `#6B7686` |
| Type | IBM Plex Sans Thai for UI (Thai and Latin); IBM Plex Mono for numbers and ages; 28/20/16/14/12 px |
| Space and radii | 4-px steps (4–32); chip 6, control 8, card 10, banner 12 |

**Team colours.** Ally blue and enemy orange differ in lightness as well as hue, so they still read for red/green colour-vision deficiency.

**Critical.** Critical always combines a solid fill, a triangle glyph and text.

**No safety green.** No green is used for "safe" anywhere. Absent coverage is a grey dashed state with the text "ไม่มีข้อมูลการมองเห็น — ไม่ใช่ ‘ปลอดภัย’" (UX-SPEC §8).

**Contrast.** Text and muted text on surface exceed 4.5:1. Final contrast, minimum caption size (12 px proposed) and minimum target size (44 px proposed) still need measurement under supported scaling (UX-SPEC §3, §10).

## 4. Honest data states (UX-SPEC §6)

Each state is told apart by **shape, line style and words**, never by colour alone:

| State | Encoding | Thai label |
| --- | --- | --- |
| Fresh | Solid accent border, filled dot, age | สด · 0.3 วิ |
| Stale | Dashed grey border, hollow dot, age | ข้อมูลเก่า · 3.2 วิ |
| Unknown | Em dash, never 0 | ยังไม่มีข้อมูล |
| Unavailable | Cross glyph and reason | ใช้ไม่ได้ในบิลด์นี้ |
| Estimated | ≈ glyph, blue border | คาดว่าพร้อม |
| Catalog-only | "REF" tag | ข้อมูลอ้างอิง ไม่ใช่ค่าจริง |

## 5. Command Deck

**Layout.** It fits the 1420×760 authored stage. A 216-px navigation rail lists the seven destinations from UX-SPEC §2:

- ภาพรวม (Overview)
- สด (Live)
- แพ็กเสียง (Voice Packs)
- ร้านค้า (Store), tagged ยังไม่เปิด
- สรุปผล (Insights)
- บัญชี (Account), tagged ไม่บังคับ
- ตั้งค่า (Settings)

The header carries the page title, match state, the command palette (Ctrl+K) and the overlay toggle (Ctrl+Alt+S).

**Overview (waiting state).** The readiness list has eight rows, one per health domain of runtime-status 1.0.0 in [runtime-status](../../contracts/runtime-status.schema.json). The `events` domain added in 1.1.0 has no row yet:

1. game intake
2. ownership
3. match
4. mode (shown as "ยืนยันไม่ได้", per GSI-INTAKE-SPEC §4.2)
5. critical audio (`not ready` with a route to Voice Packs)
6. map vision (unavailable in this build)
7. resources (shown against the 10% and 2,000 MB limits; 900 MB before DEC-RESOURCE-BUDGET)
8. account (optional)

A single healthy row never implies the others. The rest of the page:

- The match card states there is no data and shows no sample statistics.
- The audio card carries the acknowledged mute rule (EVENT-AUDIO-SPEC §4): muting also silences critical alerts.
- The companion card shows the microphone consent state.

**Live → Session Monitor (sample data, clearly badged; DEC-SESSION-MONITOR).** The owner redesigned this destination on 2026-11-11 for a second screen read from peripheral vision. The grid is now:

- **Collapsed rail:** a 56-px rail leaves room for the map.
- **Own-team column (300 px):** own hero first, then four allies. HP/mana bars, plus 1/2/3/ultimate skill tiles with cooldown seconds when the teammate shares abilities (PEER-010). "CD 0" is a value, not "ready". Not-shared, stale and not-using-Kohbor allies stay compact.
- **Enlarged minimap (680×680):** the dominant region.
  - Enemy diamonds are 28 px, with trails of up to three admitted sightings.
  - A ring marks a new sighting.
  - A missing enemy is a dashed diamond at its last-seen point, with its age.
  - A critical warning turns the map frame red (6 px) and shows a labelled banner.
- **Enemy column:** five slots with seen/missing/stale/never-seen and age, followed by a short event list. The inspector opens as a drawer over this column.

The event feed shows **detection apart from audio outcome**, for example "ตรวจพบ · ไม่เล่นเสียง (เสียงเตือนสำคัญกำลังเล่น)" (EVENT-AUDIO-SPEC §5).

## 6. Gameplay overlay

**Reference frame and keep-out zones.** On a 1920×1080 reference frame, these game zones must not be covered:

- the top bar
- the announcement area
- the minimap
- abilities and status
- shop and items

The zone boxes are **placeholders until measured natively** (UX-SPEC §8, §10).

**Modules.** All modules are click-through and sit on 82%-opaque panels:

1. **Own-stat row**, top-left: HP/mana bars, level and a freshness dot.
2. **Risk indicator**, top-centre under the game bar: grey and dashed when coverage is absent.
3. **Event banner**, below the game's announcement area: critical red with glyph and text, plus a silent-video slot fed only by a verified pack.
4. **Companion**, top-right: listening and microphone state.
5. **Missing-enemies chip** (needs M3 vision).

Every module supports enable, position, scale, opacity and saved profiles (UX-SPEC §8).

## 7. Still open before visual acceptance

| Item | Needed |
| --- | --- |
| Native safe areas | Measure HUD clearance and banner placement on Windows 19045/26100 at 1920×1080, 100% DPI, borderless fullscreen |
| Readability and targets | Confirm the 12-px minimum caption and 44-px targets under supported scaling; verify contrast with real renders |
| Remaining destinations | Voice Packs, Insights, Account, Settings and the Store "not in this build" state; local clip controls placement |
| Live-field states | GSI-only, stale, error, base-vs-Pro and inspector-open variants; the 1-px map-stability check |
| Motion and reduced motion | Banner entry/exit timing; reduced-motion variant; FPS impact measurement |

DEC-VISUALS stays **open**. The direction is approved; closure still needs the remaining screens and the native measurements above.
