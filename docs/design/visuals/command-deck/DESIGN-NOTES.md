# Command Deck — design notes

Status: **proposal, not approved**. DEC-VISUALS stays open. Sample data only. Nothing here is loaded by the application or replaces existing code. Account, friends, presence and Share list services do not exist; their screens are "design only, service pending" (DEC-SOCIAL, SOCIAL-003). Revised for the owner's Share list model; see section 13.

Files: `index.html` (gallery), `deck-live.html` (A), `minimap.html` (B), `cards.html` (C), `social.html` (D + G), `inspector.html` (E), `glance-and-idle.html` (F), `tokens.css` (tokens + shared component styles). Plain HTML/CSS, no build step. `deck-live.html?fill=1` is the deck at the viewport size; the other pages frame it in scaled iframes. Parameters: `state`, `share=on|notice|paused|none`, `mode=glance|detail`, `econ=1`, `solo=map`, `theme=dark|light|auto`, `lang=th|en`.

## 1. Decisions and why

1. **The map takes the full height.** No header bar. On 1920 × 1080 the map box is 1024 × 1024 (95% of the height); two 372-px columns flank it. The status that used to sit in a header moved to the top of each column.
2. **The map box depends on the viewport only.** Its size is one `min()` of viewport units. State, mode, language, tier and the drawer cannot move it. Measured in the browser: 0 px change across 11 states × 4 Share list states × 2 economy settings × 2 modes × 2 languages at four sizes (re-measured after the Share list revision).
3. **Markers are outlines around what the picture already shows.** The captured frame carries the smooth motion. A marker that arrives a frame late does not jump, and the picture is never hidden behind an icon.
4. **Same shape on the card and on the map.** Ally = ring, enemy = diamond, with the same slot label (A1, E1). The eye matches card to marker without reading.
5. **Missing enemies are the second thing to read.** The enemy column starts with a count line; a missing slot gets the amber surface and the largest number in the column. A seen slot is calm.
6. **Peripheral cues are shape and area, not small text.** Critical = striped hazard frame + banner with a triangle and 48-px words. Stale = dimmed, hatched, dashed frame. Small Thai text is for fixation, not for the corner of the eye.
7. **Glance and detail share one geometry.** Detail only reveals things in space glance already reserved.
8. **The Share list indicator and a one-press pause are always on the deck**, in both modes, at the top of the own-team column. The indicator names the people receiving and their scopes, or says that nobody is.
9. **No per-match prompt; three conditions instead.** Sharing flows only when you deliberately added the friend (first-time sheet confirmed), the friend added you, and both are on the same team in the same match. See section 13 and the flow in `social.html`.
10. **No green, no "ready", no guessed numbers.** Unknown is "—". Cooldown 0 is printed as `0 วิ` in a quieter weight. HP is a neutral bar.
11. **Scaled unit.** `1u = 1 px` at 1920 × 1080; all deck sizes are multiples of `u`, with pixel floors on type so small screens drop rows instead of shrinking text below the floor.

## 2. Regions

| Region | 16:9 (1920 × 1080) | 4:3 (1024 × 768) | Portrait (1080 × 1920) | Content |
| --- | --- | --- | --- | --- |
| Rail | 72u, left | same | same | Mark + seven destinations (UX-SPEC §2). Constant width; recedes in glance |
| Map | 1024u square, centre | 860u square, left | 970u square, top | Captured frame, markers, trails, frame state, banner, rate badge |
| Own team | 372u column, left | top of right column | left column below the map | Phase, clock, pause control, Share list indicator; own hero; four ally slots |
| Enemy team | 372u, top right | bottom of right column | right column below the map | Count line; five stable slots |
| Events / inspector | 372u, bottom right | strip under the map | under the enemy column | Recent events; inspector drawer over this region; audio dock slot |

At 4:3 the ability rows collapse to the ultimate cooldown only, enemy slots become one line, and events show one line each. That is a deliberate loss, stated here, not a shrink.

## 3. State tables

### 3.1 Minimap panel

| State | Source of truth | What the user sees | Thai | English |
| --- | --- | --- | --- | --- |
| Live | capture available, calibration valid, frame matches session + source generation | Picture, markers, solid frame, filled-dot badge | ภาพสด 30 fps · เฟรม 33 ms | Live 30 fps · frame 33 ms |
| Critical | `risk_alert` / `health_danger` admitted (UI-008) | Striped frame, banner with ▲ | ระวังโดนแกงค์ | Gank risk |
| Lite | throttle or user setting (proposed) | Dashed frame, half-dot dashed badge, markers kept | Lite 5 fps · เฟรม 200 ms | Lite 5 fps · frame 200 ms |
| Stale | no new admitted frame; after 5 s coverage is unavailable (VISION-002) | Last frame dimmed + hatched, markers removed | ภาพแผนที่เก่า · 3.2 วิ | Map image is stale · 3.2 s |
| Calibrating | geometry not validated (VISION-001) | Dimmed picture, corner brackets, no markers | กำลังปรับเทียบตำแหน่งมินิแมป | Calibrating the minimap position |
| Wrong monitor / layout | calibration invalidated; old frames rejected | No picture, plate, recalibrate action in detail | ตำแหน่งมินิแมปเปลี่ยน — ต้องปรับเทียบใหม่ | Minimap moved — recalibration needed |
| Capture lost | `capture.state = unavailable` | No picture, plate | จับภาพมินิแมปไม่ได้ | Minimap capture lost |
| Vision unavailable | `capture.state = unsupported` / `CAPABILITY_UNAVAILABLE` | Labelled decorative diagram, no markers | การมองเห็นแผนที่ใช้ไม่ได้ในบิลด์นี้ | Map vision is not available in this build |
| No game | `session.phase = none`, intake listening | Mark, neutral plate, empty columns | รอเข้าเกม | Waiting for game |
| In menu | intake receiving, phase none | Same, with fresh game-data chip | อยู่ในเมนูเกม | In the game menu |

In every degraded state the enemy column says **ไม่มีข้อมูลการมองเห็น — ไม่ใช่ "ปลอดภัย" / No visibility data — not "safe"**.

### 3.2 Hero cards

| State | Source of truth | What the user sees | Thai | English |
| --- | --- | --- | --- | --- |
| Own, live | local GSI, owner verified | Teal border, double-ring token, HP number + bar, mana %, abilities | — | — |
| Own, stale | `intake.state = stale` | Dashed border, grey bars, old values dimmed, age | ข้อมูลเก่า · 3.2 วิ | Stale · 3.2 s |
| Own, unverified | `ownership.state = unverified` | No values | ยืนยันผู้เล่นไม่ได้ — ไม่แสดงค่า | Player could not be verified — values hidden |
| Ally, vitals only | friend granted you vitals, not abilities | Vitals; one worded line instead of tiles | สกิล: เขาไม่ได้อนุญาต | Abilities: not granted to you |
| Ally, vitals + abilities | friend granted you both (PEER-003 + PEER-010) | Vitals + tiles (slot, level, cooldown s) | อัลติ · ยังไม่อัป · 0 วิ | Ult · unlearned · 0 s |
| Ally, + economy | **PROPOSED** scope, granted to you by that friend | Dashed chip next to the name; absent when not granted | ทอง · ข้อเสนอ | Gold · proposed |
| Ally, not sharing with you | not mutually listed, one-sided, paused by them, or no client | Compact dashed card; the reason is only on the Share list screen | ไม่ได้แชร์ให้คุณ | Not sharing with you |
| Ally, stale | peer age > 2,000 ms (PEER-005) | Values become "—", dashed, hollow dot, age | ซ่อนค่าเดิมจนกว่าจะได้ข้อมูลใหม่ | Old values hidden until fresh data arrives |
| Ally, dead | shared `alive = false`, respawn | Hatched token, ✕, respawn seconds | ตาย · เกิดใหม่ใน 23 วิ | Dead · respawn in 23 s |
| Ally, unknown | nothing admitted | Em-dash token | ยังไม่มีข้อมูล | No data yet |
| Listed friend not on your team | other team, other match, or no match | **No card.** Nothing sent, received or drawn on the deck; an enemy-team friend is an ordinary enemy slot | — | — |

Own stale keeps dimmed old values (as the shipped panel does); a stale peer shows none, because the peer contract makes stale values null.

### 3.3 Enemy slots

| State | Source of truth | What the user sees | Thai | English |
| --- | --- | --- | --- | --- |
| Seen | admitted visible capture (DATA-005) | Solid diamond, small age | ◆ เห็นอยู่ | ◆ Seen now |
| Missing | prior sighting + continuing coverage (VISION-002) | Dashed diamond, amber surface, large age | ◇ หายจากแผนที่ | ◇ Missing |
| Never seen | no admitted sighting | Em dash | ยังไม่เคยเห็น | Never seen |
| Seen, class not certified | VISION-003 | Sighting shown, identity not asserted | ◆ เห็น · ระบุตัวไม่ได้ | ◆ Seen · unverified |
| No coverage | capture stale / lost / uncalibrated | Dashed, em dash | ไม่มีข้อมูลการมองเห็น | No visibility data |

### 3.4 Event rows and social states

Event rows: line one = detected, line two = audio outcome. Filled dot = heard (`admitted`), half = cut (`preempted`), slashed = not heard (the other twelve codes), each with its reason in words. All fourteen strings, Thai and English, are in `inspector.html`.

| Share list state | Source of truth | What the user sees | Thai | English |
| --- | --- | --- | --- | --- |
| Signed out | `account.state = signed_out` | Core works; account is optional | บัญชี (ไม่บังคับ) | Account (optional) |
| Presence | opt-in, app online only | Dot + words | เปิดแอปอยู่ / ไม่ได้เปิดแอป | App online / App offline |
| Friend, not listed | friendship only | "Add to Share list" button | เพิ่มเข้ารายชื่อแชร์ | Add to Share list |
| You added them | your list only | ◌ one-sided, nothing sent | คุณเพิ่มแล้ว · รอ Fah เพิ่มคุณ | You added them · waiting for Fah |
| They added you | their list only | ◌ one-sided, nothing sent | Ploy เพิ่มคุณแล้ว · รอคุณเพิ่ม | Ploy added you · waiting for you |
| Mutual | both lists | ✓ active | เพิ่มกันทั้งสองฝ่าย · ใช้งานอยู่ | Mutual · active |
| Sharing now | mutual + same match + same team | Deck indicator with names and scopes | กำลังแชร์ให้ 3 คน: … (เลือด/มานา) · … (สกิล) | Sharing with 3: … (vitals) · … (abilities) |
| Nobody receiving | no listed friend on your team | Deck indicator | ไม่มีเพื่อนในรายชื่อแชร์ในแมตช์นี้ | No Share list friend in this match |
| Friend starts receiving | condition becomes true | One quiet, dismissible row in the event region | Bank อยู่ทีมเดียวกับคุณในแมตช์นี้ — เริ่มได้รับ เลือด/มานา | Bank is on your team in this match — now receiving vitals |
| Not receiving: other team | friend's own match id + team | × and reason | ไม่ได้รับ — อยู่ทีมตรงข้ามในแมตช์นี้ | Not receiving — on the other team in this match |
| Not receiving: other match / offline | match id differs / presence | × and reason | ไม่ได้รับ — อยู่แมตช์อื่น / ไม่ได้เปิดแอป | Not receiving — in another match / app offline |
| Paused (all or one friend) | your pause switch | ! amber, explicit resume button | หยุดแชร์ชั่วคราว · แชร์ต่อ | Sharing paused · Resume sharing |
| Removed / scope withdrawn | your action | Plain sentence: stops at once, received data cleared | นำ Tonkla ออกจากรายชื่อแชร์แล้ว | Tonkla removed from your Share list |
| Removed by them / blocked | `PEER_UNAUTHORIZED` | × chip and consequence | Fah นำคุณออกจากรายชื่อแชร์ของเขา | Fah removed you from their Share list |
| Service down | `account.state = unavailable` | ! chip; local features keep working | บริการเพื่อนและรายชื่อแชร์ใช้ไม่ได้ตอนนี้ | Friends and Share list service unavailable |


## 4. Type scale and viewing distance

Stack: `"IBM Plex Sans Thai", "Leelawadee UI", system-ui, sans-serif`; numbers in `"IBM Plex Mono", Consolas, monospace` (the shipped stacks). The Google Fonts link is optional; offline the system Thai face is used.

| Role | Size | Floor | Used for |
| --- | --- | --- | --- |
| Banner | 48u | 28 px | Critical banner title |
| Hero number | 40u | 26 px | Own HP, missing-enemy age |
| Number | 32u | 20 px | Ally HP, respawn, counts, clock |
| Label | 24u | 15 px | Names, states, cooldown values, event names |
| Secondary | 20u | 13 px | **Minimum for state-bearing text in glance mode** |
| Detail | 16u | 12 px | **Absolute minimum**: unit words, max HP, learned level, slot keys |

Rationale, computed and not measured: a 24-inch 1920 × 1080 panel has a 0.277-mm pixel; at 1 m one arc-minute is 0.291 mm. A 32-px Plex Mono digit is about 22 px tall = 6.2 mm = 21 arc-minutes, inside the 20–22 arc-minute range usually recommended for comfortable reading. A 40-px digit is about 27 arc-minutes. A 20-px Thai label has a consonant body of roughly 11 px (my estimate of the font's proportions) = about 11 arc-minutes: legible when looked at, not from the periphery. That is why peripheral meaning is carried by frames, surfaces, shapes and the large numbers. On 1366 × 768 the floors apply (13 px glance, 12 px detail) and that size should be treated as "readable at desk distance", not at 1 m.

Interactive targets are never scaled below 44 px. Focus ring: 3 px accent, 2 px offset.

## 5. Motion spec

Only `transform` and `opacity` animate. `prefers-reduced-motion: reduce` removes every animation and transition.

| Element | Motion | Duration | Reduced motion |
| --- | --- | --- | --- |
| Critical frame + banner | opacity 0 → 1, once | 80 ms linear | appears instantly |
| New sighting ring | scale 0.35 → 1, opacity 1 → 0.55, once | 600 ms ease-out | static ring |
| Inspector drawer | opacity + 8-px translate | 140 ms ease-out | appears instantly |
| Placeholder specks (mock only) | slow drift, stands for live video | 7–14 s | static |
| Routine value updates | none | — | — |

No flashing, no pulsing, no looping emphasis. Nothing moves because of prediction, decay or elapsed time: trails are stepped by sample index, last-seen markers stay put, ages are numbers.

## 6. Contrast (WCAG 2.x ratio, computed from the hex values)

| Token | Dark: on ground / surface / raised | Light: on ground / surface / raised | Non-colour cue |
| --- | --- | --- | --- |
| `--text` | 15.7 / 14.3 / 12.9 | 16.0 / 17.5 / 15.0 | — |
| `--text-2` | 11.6 / 10.6 / 9.5 | 9.5 / 10.4 / 8.9 | — |
| `--muted` | 7.7 / 7.0 / 6.3 | 5.8 / 6.3 / 5.4 | — |
| `--accent` (own, live, sharing) | 9.1 / 8.3 / 7.5 | 5.3 / 5.8 / 5.0 | filled dot, double ring, ⇄, words |
| `--ally` | 7.8 / 7.2 / 6.4 | 5.6 / 6.1 / 5.2 | ring shape, A-label |
| `--enemy` | 8.1 / 7.4 / 6.7 | 5.5 / 6.0 / 5.2 | diamond shape, E-label, ◆ |
| `--warning` (missing) | 10.3 / 9.4 / 8.5 | 5.4 / 5.9 / 5.1 | dashed diamond, ◇, words |
| `--critical` | 6.1 / 5.6 / 5.1 | 5.3 / 5.8 / 4.9 | striped frame, ▲, banner words |
| `--stale` (strokes only, never text) | 4.1 / 3.8 / 3.4 | 4.2 / 4.6 / 4.0 | dashed line, hollow dot, hatching |
| `--line-strong` (control outlines) | 5.5 / 5.0 / 4.5 | 4.2 / 4.6 / 4.0 | — |

Pairs: critical ink on critical 6.2 (dark) / 5.8 (light); text on accent-surface 11.7 / 15.1; text on warning-surface 12.7 / 15.8; warning on warning-surface 8.4 / 5.4; text on critical-surface 14.3 / 14.8.

Map overlay (fixed in both themes, on `--map-ground` #0b0f14): own 9.3, ally 7.9, enemy 8.2, missing 10.5, critical 6.2, text 15.9, muted 7.8. Labels sit on an 88% dark plate, so these hold over any picture; marker strokes have a dark halo. Contrast over a real captured frame is not verified.

## 7. Tokens

Reused unchanged from `apps/desktop/src/styles.css`: `--ground --surface --raised --line --line-subtle --text --text-2 --muted --accent --accent-surface --ally --warning --warning-surface --stale --critical`, the font stacks, radii 6/8/10/12, 44-px target.

Shared with the parallel `s3-audio-ui/tokens.css` (same names and values): `--line-strong --critical-ink --critical-surface`, every light-theme value, `--ease-out --dur-enter --dur-critical --dur-exit`.

New here: `--enemy` (in design-tokens.json, not yet in styles.css) and its light value, `--ally-surface --enemy-surface --hatch --scrim`, `--u` and the scaled roles `--fz-banner/hero/num/label/sec/det`, `--g1…g6`, `--bw`, `--lh-tight`, `--dur-once`, and the fixed map set `--map-ground/own/ally/enemy/missing/critical/critical-ink/text/muted/halo/plate`. The focus ring is 3 px (shipped: 2 px).

## 8. Second-monitor ergonomics

- **Always-on-dark.** Tokens follow `prefers-color-scheme` with a `data-theme` override. The Live deck pins `data-theme="dark"` by default; the user may choose "follow system" or light. The captured picture and its markers keep fixed colours in both themes.
- **Burn-in.** No pixel shifting in a match (it would break the 1-px rule). Instead: permanent chrome uses only line/surface tokens; bright fills are reserved for transient states; the largest area is video. With no match the deck goes to the idle state, where the mark relocates in discrete steps.
- **Glance / detail.** Glance is the default in a live match. Pointer entry or a key press shows detail; it leaves after a quiet period. The timeout value is not chosen here.
- **No hover-only information, no required interaction in a match.** Every interactive control has a keyboard path (table in `inspector.html`).

## 9. Where this differs from the owner's brief

| Brief | Design | Why |
| --- | --- | --- |
| Share with listed friends "at all times" | Standing permission, but data flows only when both added each other and both are on the same team in the same match | Proposed safeguards 1 and 2 (section 13) |
| Nobody sets anything up per match | True: no prompt. The deck indicator and one-press pause replace it; the first add needs an explained, confirmed sheet | Proposed safeguards 5 and 6 |
| Share gold | Shown only as a **PROPOSED** "economy" scope: dashed, labelled, absent unless granted, disabled in every sheet | PEER-003 excludes it; it needs a new protocol and consent decision |
| "Share GSI state" | Three per-friend scopes, each direction separate; vitals is the minimum to be listed | No view-only mode in the baseline; PEER-010 makes abilities a separate scope |
| Login and friends | Sign-in optional for the core; friendship and presence never put anyone on the Share list | SOCIAL-001, SOCIAL-002 |
| Minimap at 30 fps | Drawn as the target, with a badge that states real rate and frame age, plus a Lite state | The budget has not been measured |
| Allies on the map | Overlay rings only for sharing teammates detected in the frame; positions never come from peers | PEER-003 carries no position |
| Teammates' mana | Percentage only | PEER-003 carries mana as a percentage |

## 10. Where this differs from the first proposal (Live.dc.html)

- 48-px header removed; the map grows from 680 px on a 1420 × 760 stage to 1024u on 1920 × 1080. The 1420 × 760 authored stage of UX-SPEC §3 is **not** used for this screen: a second monitor is full-screen, so the deck scales with the viewport. This needs an owner decision (below).
- Rail 56 px with icons → 72u with words (no icon-only navigation).
- Enemy markers were filled diamonds; now outlines around the icon in the picture.
- "E1 เข้าใกล้เลนกลาง" and "กำลังเคลื่อน" removed: they read as inferred movement. The banner states admitted facts only.
- Critical frame was a 6-px red line (colour only); now a striped hazard frame.
- The inspector covers the event region, not the enemy column, so missing-enemy information stays visible.
- Level sits under the token; mana is a percentage; the "sample data" tag is outlined so it also passes contrast in light.
- Type is larger throughout (the first proposal went down to 10 px).

## 11. Open decisions for the owner

1. **Standing consent versus per-match consent.** The Share list replaces the baseline's fresh opt-in per join (PEER-001, SOCIAL-002). Section 13 lists the conflicts. This is the main decision.
2. **Keep or drop "Join with a code".** Drawn only as a small secondary path for signed-out users (social.html D1, D11) that still asks every time.
3. **Economy scope.** Add it or not; which fields; new DTO and consent text.
4. **Periodic re-confirmation.** Should a standing entry expire or ask again, for example every 30 days? Not drawn.
5. **Notifying friends of changes.** Should a listed friend who is offline be told later that you paused, removed them or changed a scope? Not drawn. Today's design tells nobody; the other side just sees "not sharing with you".
6. **Does pause also stop receiving?** Drawn: pause stops only what you send. Fairness may argue for both directions.
7. **Who decides "same match, same team".** Each client reports its own match id and team. The service must compare them without trusting one side; a wrong answer would leak to an enemy. Needs a protocol and threat review.
8. **30 fps budget.** CPU, GPU and the ≤ 3% game-FPS loss are unmeasured. What rate does Lite use, and what triggers it?
9. **Calibration flow.** Automatic, guided, or manual; who starts recalibration; whether a wrong crop may ever be shown.
10. **Friend-presence notification.** None is drawn.
11. **Private social server.** DEC-SOCIAL: sign-in method, where the Share list is stored, retention, limits.
12. **Capture sub-states in the contract.** Calibrating, Lite, stale and recalibration-needed need states or reason codes.
13. **Viewport-scaled deck versus the 1420 × 760 stage** for this destination.
14. **Overlay rings for teammates who are not sharing.** Currently none.
15. **Hero names and portraits.** Only placeholders are used.
16. **History row ownership.** Reconcile with the audio panel in `s3-audio-ui`.

## 12. What I did not verify

- Not viewed on a real second monitor, at a real 1 m distance, or in peripheral vision. The type rationale is arithmetic.
- No FPS, CPU, GPU or memory measurement. No real capture; the picture is a CSS placeholder and the "30 fps" text is sample copy.
- Marker legibility over a real captured frame, and the choice of badge corners, are untested.
- Thai rendering was checked only in headless Edge and the built-in browser pane on this Windows machine, with the IBM Plex web font loaded. The offline fallback (Leelawadee UI) was not visually checked.
- Not tested in the Tauri WebView2 window, at DPI scaling other than 100%, or on an OLED panel for burn-in.
- No screen-reader pass; no colour-vision simulation; contrast is computed from hex values, not sampled from renders.
- Reduced motion was implemented but not exercised with the OS setting on.
- The 1-px map check was a script in the browser on the mockup, not a frozen-input measurement of the application.
- The Share list screens were looked at only in Thai, dark theme, at 1366 px wide; English and light were not viewed for `social.html` after the revision. The Share list wording has not been tested with any user, and nobody has checked that people understand "standing" sharing from the first-time sheet.
- The indicator was looked at with three recipients only. Five recipients, or long names, were not tried and will need more lines or truncation.
- Nothing about the Share list is implemented or protocol-checked: mutual matching, the same-match same-team test, clearing on removal and pause are drawings.
- Checked after the Share list revision: all seven pages load with no console errors at 1920 x 1080 and 375 px width, and none has horizontal page scroll at either width (measured on all seven). The deck's map box moved 0 px across 11 states x 4 Share list states x 2 economy settings x 2 modes x 2 languages at 1920 x 1080, 1366 x 768, 1024 x 768 and 1080 x 1920; the same sweep found no card overflowing its slot by more than 2 px at those four sizes. Pages other than the deck were not re-checked at 1366 x 768 after the revision. 2560 x 1440 was viewed only as a scaled frame. (Corrected by the lead: the first wording claimed more than was measured.)

## 13. Share list model (owner change)

The owner replaced "party/room + per-match consent" with a standing **Share list** (รายชื่อแชร์). Kohbor has no party, room or lobby concept and those words are not in the UI. A player lists the friends they share with; when listed friends are teammates in a match, sharing flows with no per-match step.

Decisions in this design:

- Friends and the Share list are two things. Becoming friends adds nobody; "เพิ่มเข้ารายชื่อแชร์ / Add to Share list" is a second deliberate step.
- The deck shows who receives what in every match, with a one-press pause beside it. A listed friend who is not on your team gets no card and no mention on the deck; the reason is on the Share list screen only.
- Ally cards word everything from the receiver's side: "เขาไม่ได้อนุญาต" (not granted to you), "ไม่ได้แชร์ให้คุณ" (not sharing with you).

Safeguards drawn. **Each one is proposed and needs an owner decision**, because together they replace the baseline rule of consent at every join:

1. **Mutual.** Data flows only when both people added each other. Three worded states: you added them, they added you, mutual. A one-sided add sends nothing, and what the other side offered is not revealed.
2. **Same match and same team only.** Other team, another match, no match or offline means nothing is sent. No screen implies an enemy-team friend receives anything.
3. **Per-friend scopes, each direction separate.** Vitals, abilities, and the still-PROPOSED economy scope. You see what they granted you; they see what you granted them; both shown side by side.
4. **Pause and revoke.** Global pause on the deck and in the list, per-friend pause, remove. Removing or withdrawing a scope stops sending at once and clears what was received. Resume is always an explicit press.
5. **Always-visible indicator** instead of a prompt, plus one quiet, dismissible notice when a listed friend starts receiving. Never a modal during play.
6. **First-time sheet.** The first add shows what is shared, with whom, the same-team rule, pause and remove, and requires ticking "Vitals" and "I understand". Later adds use a compact version.
7. **Vitals are the minimum.** Withdrawing vitals from someone removes them from the list.

Conflicts with the baseline, stated plainly:

| Baseline | What it says | What the Share list does |
| --- | --- | --- |
| PEER-001 | Every create/join needs checked consent and explicit activation in one action; no autojoin, no saved credentials | Consent is given once per friend and then stands; sharing starts automatically when the conditions are true |
| SOCIAL-002 | Friendship and presence never share automatically; an invitation still needs a new checked action | Friendship still shares nothing, but a mutual Share list entry shares without a new action each match |
| PEER-008 | Leave clears local credentials and peer samples at once and fences late responses; remote delete is best effort | Pause, remove, scope withdrawal and match end must each behave like "leave". The copy says the other side's screen clears; that relies on their client and the 2-second staleness rule, and cannot be promised if their client is unreachable |
| PEER-009 | No peer persistence | The Share list itself (who, which scopes, paused or not) is persisted, on the account service side only. Gameplay samples are still never persisted, logged or sent to AI, on either side |
| PEER-004 | Rooms of at most 5, 4 hours, 30 s sync expiry | There is no room. The team size bounds recipients to 4; an equivalent sync-expiry rule is still needed |

Not designed here: the protocol, how the service verifies match and team, list size limits, and abuse cases (for example a listed friend who is a spectator or coach).
