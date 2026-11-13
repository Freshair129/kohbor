# Sprint 3 audio and event-media UI — design notes

Status: **proposal, not approved.** DEC-VISUALS stays open. Nothing here replaces code in `apps/`.
Files: `index.html` (gallery), `overlay.html`, `audio-panel.html`, `tokens.css`, this note.
Sources read: UX-SPEC 0.3.6 (§3, 4, 6, 8, 9), VISUAL-DESIGN-PROPOSAL 0.3.6 and its artboards, `apps/desktop/src/styles.css`, `copy.ts`, `EventBanner.tsx`, `AudioPanel.tsx`, EVENT-AUDIO-SPEC §2–5, `event-catalog.json`, `runtime-status.schema.json`.

## 1. Decisions and why

### Overlay event banner

1. **A dark mat under every line.** The clip sits on a plate (`--ground` at 90%). Accent lines, brackets and the tier rail are drawn on the plate, never on the game, so their contrast does not depend on the scene. A light hairline (`--text` at 55%) outside the plate separates it from a dark scene. An opaque, alpha-less clip then looks framed on purpose.
2. **The frame carries no text.** The clip and the native audio say what happened. The frame says only how strong (tier) and what kind (ordinary or critical). This meets "no always-on text" and avoids Thai/English layout in the game.
3. **Tier = weight around the clip, plus a rail under it.** Four strength levels, all outside the clip box:

   | Level | multi_kill tier | streak tier | Frame |
   | --- | --- | --- | --- |
   | 0 | (untiered events) | (untiered events) | 2 px accent line, no rail |
   | 1 | 2 | 3–4 | 2 px line, rail |
   | 2 | 3 | 5–6 | 3 px line, short corner brackets |
   | 3 | 4 | 7–8 | 4 px line, long corner brackets |
   | 4 | 5+ | 9–10+ | 4 px line, closed inner ring |

   The rail shows the exact step: 4 diamonds for `multi_kill`, 8 bars for `streak`; filled versus hollow. The top level uses near-white, not amber and not red, because amber means "not ready" and red means "critical" elsewhere in the product.
4. **Critical is a different silhouette.** Square corners, a 6 px red frame (same weight as the Session Monitor's critical map frame), hazard stripes above and below the clip, and a warning-triangle tab on each side. It has no tiers, no rail and no scale animation. Shape, pattern and glyph each identify it without colour.
5. **Cut = replace in one frame.** The old banner is removed in the same frame the new one starts a 90 ms opacity fade. There is never a cross-fade, an overlap or a queue (MEDIA-005, same playback generation).
6. **Visual tail.** When the audio ends and the clip continues, an ordinary frame drops its rail and brackets (120 ms opacity). The frame goes quiet while the picture finishes. A critical frame never downgrades while visible. Exit is 180 ms.
7. **Empty = nothing.** No verified clip means no element at all, for ordinary and for critical events alike. The dotted box in the mockup's Empty state is a reviewer annotation only. This keeps the shipped `EventBanner` rule (`visual_url` absent returns `null`).
8. **Fixed clip box, `object-fit: contain`.** Proposed 360 × 203 px at 1920 × 1080, top edge at 220 px (20.4 vh). The largest ordinary frame is 394 × 255 px and the critical frame is 508 × 255 px, so both stay under the shipped `max-height: 24vh` and `min(680px, 54vw)`.

**Where this differs from what exists.**

- The first proposal's overlay artboard put text ("glyph and text") in the critical banner and a small 128 × 72 clip beside it. This proposal makes the clip the content and removes the text, per the "no always-on text" and "no substitute" rules given for this task.
- The shipped CSS places the banner at `top: clamp(116px, 14vh, 172px)`, which is 151 px at 1080p. That falls inside the first proposal's placeholder for the game's announcement area (118–198 px). This proposal uses 220 px, the first proposal's value. Both numbers are unmeasured.
- The shipped banner is a bare `<video>` with a 140 ms fade. The fade duration and easing direction are kept for level 0–1.

### Audio and alerts panel

1. **A real switch, where on means muted.** One 44 px control with `role="switch"`, `aria-keyshortcuts="Alt+M"`, a visible `Alt+M` key hint, a state line with an icon, and a warning-coloured on state. The accent colour is never used for "muted".
2. **The mute consequence is stated twice.** Before muting, a permanent line says muting silences critical alerts too. While muted, a bordered callout says critical alerts will not sound, and readiness reads `unavailable / AUDIO_MUTED`.
3. **Unmuting promises nothing.** The muted state says the status will return to whatever the host reports. The live specimen shows unmuting into `not_ready`.
4. **Readiness is a block with four redundant cues:** an edge pattern (solid, diagonal stripes, dashes, dots), the border style, a chip glyph (●, !, ×, —) and words. Only `ready` uses the accent colour, a solid edge and a filled dot.
5. **Each state answers three questions in order:** what the host reported, what that means for critical alerts right now ("will not sound"), and what to do next. The raw reason code follows in small mono type for support.
6. **`ready` is bounded.** The copy says the media is verified and preloaded and that Kohbor does not check volume or speakers. It does not say the user is safe.
7. **Light theme is new.** The shipped app is dark only. Light values are in `tokens.css`; the accent is darkened to `#0a7265` so it can carry outlines and glyphs on white.
8. **History rows show two facts.** A diamond and time mean "detected". A separate chip gives the audio result in four shapes: heard, cut short, not heard, no sound by design. A plain sentence gives the reason. Critical-class rows have a tint, a double left rule and a triangle glyph.

Improvements over the shipped `AudioPanel`: `unavailable` gains a dashed border (shipped `.chip-off` is solid like `unknown`); chip line height is 1.5 instead of 1.4; the control is a switch with a visible state instead of a button whose label flips; the two always-on notes become state-specific copy.

## 2. State tables

### Media readiness (`runtime-status.media`)

| State | Reason | User sees | Thai | English |
| --- | --- | --- | --- | --- |
| (no status) | — | Dotted edge, "—" chip | ยังไม่ทราบ · ยังไม่ได้รับสถานะจากระบบหลักของ Kohbor · ยังบอกไม่ได้ว่าเสียงเตือนสำคัญจะดังหรือไม่ | Unknown · No status has been received from the Kohbor host yet. · Kohbor cannot say yet whether critical alerts will sound. |
| `not_ready` | `MEDIA_UNMAPPED` | Striped edge, "!" chip | ยังไม่พร้อม · แพ็กที่ใช้อยู่ยังไม่มีเสียงเตือนสำคัญที่ผ่านการตรวจ · เสียงเตือนสำคัญจะไม่ดัง เหตุการณ์ที่ตรวจพบยังบันทึกไว้ในประวัติ · เลือกแพ็กที่มีเสียงเตือนสำคัญในหน้า “แพ็กเสียง” | Not ready · The active pack has no verified critical alert audio. · Critical alerts will not sound. Detected events are still recorded in history. · Choose a pack with critical alert audio in Voice Packs. |
| `not_ready` | `MEDIA_INVALID` | Striped edge, "!" chip | ไฟล์เสียงเตือนสำคัญในแพ็กไม่ผ่านการตรวจ หรือเปิดอ่านไม่ได้ · เสียงเตือนสำคัญจะไม่ดัง Kohbor จะไม่เล่นไฟล์อื่นแทน · ติดตั้งแพ็กนี้ใหม่ หรือเลือกแพ็กอื่นในหน้า “แพ็กเสียง” | The pack's critical alert audio failed verification or could not be decoded. · Critical alerts will not sound. Kohbor will not play a different file instead. · Reinstall this pack or choose another in Voice Packs. |
| `not_ready` | `MEDIA_PROBE_FAILED` | Striped edge, "!" chip | Kohbor ยังตรวจยืนยันภาพในคลิปของแพ็กไม่สำเร็จ · จึงยังไม่ยืนยันว่าสื่อเตือนสำคัญพร้อม · เปิดใช้แพ็กนี้อีกครั้ง ถ้ายังเป็นอยู่ให้ติดตั้งแพ็กใหม่ | Kohbor could not confirm the video in the pack's clip. · So critical alert media is not confirmed ready. · Activate the pack again; if this stays, reinstall the pack. |
| `not_ready` | `null` | Striped edge, "!" chip | ระบบหลักยังไม่ได้แจ้งเหตุผล · เสียงเตือนสำคัญจะไม่ดัง | The host did not report a reason. · Critical alerts will not sound. |
| `ready` | — | Solid accent edge, ● chip | พร้อม · เสียงเตือนสำคัญผ่านการตรวจและโหลดเตรียมไว้แล้ว · หมายถึงไฟล์เสียงพร้อมเท่านั้น Kohbor ไม่ได้ตรวจระดับเสียงหรือลำโพงของเครื่อง | Ready · Critical alert audio is verified and preloaded. · This covers the media only. Kohbor does not check your volume or speakers. |
| `unavailable` | `AUDIO_MUTED` | Switch on, callout, dashed edge, "×" chip | ใช้ไม่ได้ · ปิดเสียงทั้งหมดอยู่ — เสียงเตือนสำคัญจะไม่ดัง · ใช้ไม่ได้เพราะคุณปิดเสียงทั้งหมดไว้ · เมื่อเปิดเสียง สถานะจะกลับไปตามที่ระบบหลักรายงาน ซึ่งอาจยังไม่พร้อมก็ได้ · กด Alt+M หรือสวิตช์ด้านบนเพื่อเปิดเสียง | Unavailable · All audio is muted. Critical alerts will not sound. · Unavailable because you muted all audio. · After you unmute, this returns to whatever the host reports, which may still be not ready. · Press Alt+M or use the switch above to unmute. |
| `unavailable` | `CAPABILITY_UNAVAILABLE` | Dashed edge, "×" chip | ตอนนี้ Kohbor เล่นเสียงบนเครื่องนี้ไม่ได้ · เสียงเตือนสำคัญจะไม่ดัง อาจไม่พบลำโพงหรือหูฟังที่ใช้ได้ หรือบิลด์นี้ยังไม่มีความสามารถนี้ · ตรวจอุปกรณ์เสียงใน Windows แล้วปิดและเปิด Kohbor ใหม่ | Kohbor cannot play audio on this computer right now. · Critical alerts will not sound. No usable speaker or headphone may have been found, or this build lacks the capability. · Check the sound output in Windows, then restart Kohbor. |

Mute switch: label ปิดเสียงทั้งหมด / Mute all audio; state ตอนนี้เปิดเสียงอยู่ / Sound is on, ตอนนี้ปิดเสียงอยู่ / Sound is muted; note ถ้าปิดเสียง จะเงียบทุกประเภท รวมถึงเสียงเตือนสำคัญ / Muting silences every sound, including critical alerts.

### History outcomes (EVENT-AUDIO-SPEC §5)

| Outcome | Chip | Thai | English |
| --- | --- | --- | --- |
| `admitted` | ได้ยิน / Heard | เล่นเสียงแล้ว | Audio played. |
| `preempted` | ถูกตัดกลางคัน / Cut short | เริ่มเล่นแล้ว แต่เสียงเตือนสำคัญแทรกเข้ามา | It started, then a critical alert took over. |
| `unmapped` | ไม่ได้ยิน / Not heard | แพ็กที่ใช้อยู่ไม่มีเสียงสำหรับเหตุการณ์นี้ | The active pack has no sound for this event. |
| `invalid_media` | ไม่ได้ยิน | ไฟล์เสียงของเหตุการณ์นี้ไม่ผ่านการตรวจ | This event's sound file failed verification. |
| `muted` | ไม่ได้ยิน | คุณปิดเสียงทั้งหมดไว้ (แถวเตือนสำคัญ: เสียงเตือนสำคัญจึงไม่ดัง) | You had muted all audio (critical row: so the critical alert stayed silent). |
| `critical_active` | ไม่ได้ยิน | เสียงเตือนสำคัญกำลังเล่นอยู่ จึงไม่เล่นเสียงนี้ | A critical alert was playing, so this was not played. |
| `channel_busy` | ไม่ได้ยิน | มีเสียงอื่นกำลังเล่นอยู่ จึงไม่เล่นเสียงนี้ | Another sound was playing, so this was not played. |
| `lower_rank_same_tick` | ไม่ได้ยิน | มีเหตุการณ์ที่สำคัญกว่าเกิดพร้อมกัน จึงเล่นเสียงของเหตุการณ์นั้นแทน | A higher-priority event happened at the same moment and took the slot. |
| `cooldown` | ไม่ได้ยิน | เพิ่งเตือนเรื่องนี้ไป จึงยังไม่เตือนซ้ำ | This alert sounded recently, so it was not repeated. |
| `stale_session` | ไม่ได้ยิน | แมตช์หรือแหล่งข้อมูลเปลี่ยนก่อนถึงคิวเล่น | The match or data source changed before it could play. |
| `superseded_revision` | ไม่ได้ยิน | แพ็กเสียงถูกเปลี่ยนก่อนถึงคิวเล่น | The voice pack changed before it could play. |
| `detector_unavailable` | ไม่ได้ยิน | บิลด์นี้ยังตรวจจับเหตุการณ์นี้ไม่ได้ | This build cannot detect this event yet. |
| `inert` | ไม่ได้ยิน | Kohbor รุ่นนี้ไม่รู้จักเหตุการณ์นี้ | This version of Kohbor does not know this event. |
| `not_voiced` | — ไม่มีเสียง / No sound | เหตุการณ์นี้ไม่มีเสียงตั้งแต่แรก | This event has no sound by design. |

The mockup shows eight of these rows; the other six use the "not heard" chip with the sentence above.

### Overlay states

| State | Trigger | User sees |
| --- | --- | --- |
| Empty | No playback, or no verified clip | Nothing |
| Ordinary, level 0 | `hero_kill`, `first_blood`, `level_milestone`, other untiered events with a clip | Rounded mat, accent line |
| Ordinary, level 1–4 | `multi_kill` tier 2–5, `streak` tier 3–10 | As above with rail, brackets or ring |
| Critical | `health_danger`, `risk_alert` with a verified clip | Square red frame, stripes, two triangle tabs |
| Cut | A newer playback replaces this one | Old removed, new fades in 90 ms |
| Tail | Audio ended, clip still playing | Ordinary: rail and brackets fade out. Critical: unchanged |
| Exit | Clip or tail ended | 180 ms fade, 4 px rise |

## 3. Contrast (WCAG 2.x ratio, computed from the hex values; not measured from pixels)

| Pair | Dark | Light |
| --- | --- | --- |
| `--text` on `--surface` / `--raised` | 14.3 / 12.9 | 17.5 / 15.0 |
| `--text-2` on `--surface` / `--raised` | 10.6 / 9.5 | 10.4 / 8.9 |
| `--muted` on `--surface` / `--raised` | 7.0 / 6.3 | 6.3 / 5.4 |
| Ready: `--accent` on `--accent-surface` (glyph, border) | 6.8 | 5.0 |
| Ready: `--text` on `--accent-surface` (chip text) | 11.7 | 15.1 |
| Not ready: `--warning` on `--warning-surface` | 8.4 | 5.4 |
| Not ready: `--text` on `--warning-surface` | 12.7 | 15.8 |
| Unavailable: `--line-strong` on `--surface` / `--raised` (dashed border) | 5.0 / 4.5 | 4.6 / 4.0 |
| Critical row: `--critical` on `--critical-surface` (glyph, rule) | 5.6 | 4.9 |
| Critical row: `--text-2` on `--critical-surface` | 10.6 | 8.7 |
| `--stale` on `--surface` (edge pattern only, never text) | 3.8 | 4.6 |

Overlay, theme-independent, with the mat composited over the scene:

| Pair | Over white | Over mid grey | Over black |
| --- | --- | --- | --- |
| Accent `#3cc8b4` on the mat | 7.0 | 8.2 | 9.2 |
| Near-white `#e6eaf0` on the mat (brackets, ring) | 12.1 | 14.1 | 15.9 |
| Mat edge against the scene | 14.6 | 4.3 | 1.1 (the halo line gives 5.3) |
| Critical `#ff5a4a` against its dark ink `#1a0b08` | 6.2 | 6.2 | 6.2 |
| Critical ink keyline against the scene | 19.2 | 4.8 | about 1 (the red gives 6.8) |
| Hollow rail step outline on the mat | 3.9 | 3.9 | 3.9 |

Red against a bright or mid scene is only 3.1 and 1.3, which is why the critical frame has a dark keyline outside it.

## 4. Motion

Only `opacity` and `transform` animate. There is no blur, filter, animated shadow or layout change.

| What | Property | Duration | Easing |
| --- | --- | --- | --- |
| Ordinary enter, level 0–1 | opacity 0→1, translateY 6 px→0, scale 0.98→1 | 140 ms | `cubic-bezier(.2,.8,.2,1)` |
| Ordinary enter, level 2–3 | same, scale 0.95→1 | 180 ms | same |
| Ordinary enter, level 4 | same, scale 0.90→1 | 220 ms | `cubic-bezier(.2,1.3,.4,1)` |
| Critical enter | opacity only | 80 ms | linear |
| Critical tabs | opacity 1→0.45→1, once | 240 ms after an 80 ms delay | linear |
| Cut (replacement) | old: removed, 0 ms. New: opacity only | 90 ms | linear |
| Tail | rail and brackets opacity 1→0 | 120 ms | linear |
| Exit | opacity 1→0, translateY 0→−4 px | 180 ms | `cubic-bezier(.4,0,1,1)` |
| Mute switch thumb | translateX | 120 ms | ease-out |

**Reduced motion.** Every change is a single frame: no scale, no slide, no pulse, no fade. Emphasis is static (line weight, brackets, ring, rail count, critical shape). The mockup's toggle and `prefers-reduced-motion` both apply it.

## 5. Accessibility

- Font stack (unchanged from the app): `"IBM Plex Sans Thai", "Leelawadee UI", system-ui, sans-serif`; mono `"IBM Plex Mono", Consolas, monospace`. The mockups carry one optional Google Fonts link; without it they fall back to Leelawadee UI. The product should bundle the font, not fetch it.
- Line height 1.6 for body and 1.5 for chips. Text wraps (`overflow-wrap`); nothing is truncated or shrunk.
- Keyboard: the switch is a native button (Tab, Space, Enter) and answers Alt+M. Focus ring is 2 px accent with a 2 px offset.
- Targets: every button and the switch are at least 44 px high; history rows are 44 px.
- The overlay banner is `aria-hidden` and click-through, as shipped.

## 6. Open questions for the owner

1. **Critical with no verified clip shows nothing in the banner slot.** That follows the hard rule. Is the separate risk indicator (top centre) the only on-screen cue then, and is that acceptable?
2. **Should the critical frame carry a short word** (for example อันตราย) while it is shown? It is glyph-only here. The first proposal had text.
3. **Banner position and size:** 220 px from the top and a 360 × 203 clip are proposals. The shipped 151 px overlaps the first proposal's announcement-area placeholder. Which is right needs the native measurement.
4. **`CAPABILITY_UNAVAILABLE` covers "no output device" and "not in this build".** The copy names both. Should native report a separate code for a missing device so the UI can be exact?
5. **`MEDIA_PROBE_FAILED`** is worded from the shipped copy (the clip's video stream could not be verified). Should a video-probe failure hold critical *audio* readiness at not ready, or belong to a separate visual-media status?
6. **Does `ready` include a working output device?** The copy says Kohbor does not check volume or speakers. If native does check the device, the sentence should change.
7. **History time:** game clock, as drawn, or wall-clock time?
8. **Raw codes in the UI:** the reason and outcome codes are shown small after the explanation. Keep them, or move them to a details view?
9. **Light theme:** is a light control window wanted? The approved direction is dark-first.
10. **Event names in Thai:** มัลติคิล, คิลสตรีค, เลือดลดฮวบ and the rest are proposals in gamer-natural Thai, not approved copy.

## 7. What I did not verify

- Nothing was measured on a real overlay window: no transparency, always-on-top, click-through or DPI check.
- No FPS, GPU or CPU measurement. "Cheap" is a design constraint here (opacity and transform only), not a result.
- No native placement check. The HUD keep-out boxes are the first proposal's placeholders.
- No real clip was used: letterboxing, alpha-less backgrounds and non-16:9 clips are untested.
- Contrast ratios are computed from hex values, not sampled from rendered pixels or a real game scene.
- Thai rendering was checked only in the one browser available here (Chromium via Playwright), with IBM Plex Sans Thai loaded from Google Fonts. The Leelawadee UI fallback and WebView2 were not checked.
- No screen-reader or real-keyboard pass; Alt+M and the switch were exercised by script only.
- The motion was not watched frame by frame; durations are the CSS values.
- The copy has not been reviewed by a native-speaking editor or by the owner.
- The pages were opened at 1280 × 800 and 375 px wide only.
