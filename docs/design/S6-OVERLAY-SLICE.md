---
title: "S6 Overlay Slice: banner window over the game (plan)"
doc_id: "S6-OVERLAY-SLICE"
status: draft
version: "0.2.0"
updated: "2026-11-13"
authority: "Lead plan under the owner's roadmap reorder (overlay pulled ahead of Sprint 4); elaborates UI-001 and SPRINT-ROADMAP Sprint 6 first row. Changes no requirement statement. DEC-VISUALS stays open: this is topology and honest empty states, not an approved look."
approval_status: "agent_decision_not_owner_approved"
---

# S6 Overlay Slice: banner window over the game

Labels: **SPEC** = stated in a governing document (cited). **PROPOSED** = this plan's choice, reviewable. **UNVERIFIED** = needs Windows, hardware or the owner. Nothing here is native or live evidence.

## 1. Goal and non-goals

When a Dota 2 event is detected and the audio owner admits a clip, a separate window over the game plays the pack's silent WebM while the host plays the audio, with an honest status chip. A demo trigger shows this without a running game. **Not in this slice:** own-stat row (Q-UI-1: the roadmap row says no HP or mana row), risk indicator, companion art, Session Monitor, scale/opacity profiles, FPS measurement (QUALITY-003), pixel approval (DEC-VISUALS).

Hard rules (SPEC: AGENTS.md, UI-001): the overlay is a separate window; no injection, hooking or game-memory reads; no input sent to the game; no new network use; the renderer never selects or queues media (native owns playback, `playback.ts`).

## 2. Window model

| Item | Choice | Label |
| --- | --- | --- |
| Window | Second Tauri window label `overlay`, URL `index.html?view=overlay` (the renderer already renders only `EventBanner` for it, `main.tsx`). Created by the host at startup, hidden until there is something to show. | PROPOSED |
| Flags | `transparent`, `decorations:false`, `alwaysOnTop`, `skipTaskbar`, `focus:false` with `focusable:false`, `shadow:false`, not resizable, `visible:false` initially; `set_ignore_cursor_events(true)` after build (click-through) | SPEC UI-001 (transparent, always on top, click-through), PROPOSED (exact flags) |
| No focus steal | `focusable:false`, show with no activation; never call `set_focus`. UI-006: do not steal focus from the game | SPEC UI-006; UNVERIFIED on Windows |
| Capability | Separate capability file for `overlay`: `core:event:default` (listen) only. No `invoke` of app commands, no fs, no shell. The control capability keeps `core:default` | PROPOSED |
| Events | `app.emit("playback://event")` is a broadcast, so the overlay webview receives it with `listen` (`runtime.ts` `subscribePlayback`); no new event, no new emitter (S3-5 F10) | SPEC S3-5 I1 |
| Media | `<video src=http://kohbor-media.localhost/v1/...>` through the existing protocol; `media-src` in the CSP already allows it; both windows share the one registered scheme | SPEC S3-5 section 5; origin handling of the scheme stays the S3-7c open question |
| Boundary change | Edits to `tauri.conf.json`, `capabilities/`, and `lib.rs` are host-core trust files: this slice needs an R2 read (terra read-only review, then the lead's diff read) | SPEC ui-overlay card section 1 |

Visibility: the window is shown when the first banner starts and hidden when the visual ends (`playback.visual === null`), so an idle overlay is not a transparent always-on-top layer over the game. The Ctrl+Alt+S toggle is not built here (needs a global shortcut; PROPOSED later).

## 3. Placement and safe area

SPEC: never cover essential minimap, skills, shop, status regions; the banner sits **below the game's announcement area**, top-center region (UX-SPEC section 8); the stage is **720 x 220**, the clip 1280 x 480 scaled to it (SPRINT-ROADMAP Sprint 6; Q-UI-2 on size is open, the roadmap value is used and marked PROPOSED). The artboard puts the clip top edge at 220 px of 1080 (20.4 vh) (`s3-audio-ui/DESIGN-NOTES.md` section 1), unmeasured.

PROPOSED, pure function `placeBanner(monitor)` (tested without a window):
- Unit = monitor physical pixels. Reference height 1080: `scale = monitor.height / 1080`; stage = `720*scale` by `220*scale`, rounded; top = `220*scale`; horizontally centered on the monitor.
- Protected regions are fractions of the monitor, all UNVERIFIED defaults taken from Dota 2's default 16:9 layout (minimap bottom-left, shop/inventory bottom-right and bottom-center, top bar, announcement area top-center): minimap `x 0..0.20, y 0.70..1`, bottom HUD `y 0.78..1`, top bar `y 0..0.05`, announcement area `y 0.08..0.19` (the stage top at 0.204 sits just below it). The test asserts the stage rectangle intersects none of them, and a clamp keeps it inside the monitor. If a monitor is narrower than 4:3 the stage is scaled down by width.
- Which monitor: the monitor of the foreground game window is not read (reading another process's window is outside this slice). PROPOSED: the **primary monitor** by default; one persisted setting `overlay.monitor` (index) is out of scope, noted as a follow-up. Multi-monitor and DPI: the host uses physical pixel size and the monitor's scale factor; the renderer sizes in CSS pixels from `devicePixelRatio` so the stage stays 720 x 220 logical at 1080p. UNVERIFIED at 125/150% and on mixed-DPI setups.

## 4. Renderer: banner stage, chip, scheduling

- **Banner stage** (SPEC UI-001/Sprint 6): a fixed 720 x 220 box; `<video muted playsInline autoPlay>` keyed by `playback_id`, `object-fit: contain`, no text drawn on the frame (DESIGN-NOTES s3-audio-ui section 1 items 2, 7). No verified visual means no element (`EventBanner` rule today).
- **Sync with the audio owner** (PROPOSED): the host emits `started` at admission with `at_ms` in the event clock; audio is already in the sink hand-off. The renderer starts the video on that event and cannot do better than one webview event hop; clip audio is never played by the webview (the video is muted). Fixed audio-to-video offset is an acoustic and visual measurement: UNVERIFIED, owner step. The existing fail-safe (`at_ms + audio + tail + 500 ms`) hides a stuck video.
- **Preemption and cooldown** are the arbiter's, not the renderer's (SPEC EVENT-AUDIO-SPEC; `reducePlayback`): a new `started` replaces the banner in the same frame (no crossfade, no queue); `ended(reason)` for a non-current id is ignored; a duplicate `started` is ignored. The tests pin this using the real arbiter output shapes.
- **Chip** (PROPOSED, state names from `mediaStatus.ts`/`runtime-status`): a small fixed chip at the stage's top-right corner inside the window. It shows honest state only, never a green "safe". Window is shown for a chip state only on change to a *degraded* state or while a banner plays, so a healthy idle game shows nothing:

| Chip state | When | Copy source |
| --- | --- | --- |
| `ready` | media status ready | brief "พร้อม · ผ่านการตรวจ" while the banner plays; bounded text, no safety claim (DESIGN-NOTES) |
| `not_ready` / reason | pack has no verified critical audio, invalid, probe failed | existing copy table, shortened |
| `muted` | status `unavailable/AUDIO_MUTED` | "ปิดเสียงอยู่ · เสียงเตือนสำคัญจะไม่ดัง" |
| `no_device` | status `unavailable/CAPABILITY_UNAVAILABLE` | "ไม่มีอุปกรณ์เสียง" |
| `no_pack` | no pack active (media list empty or no active install) | "ยังไม่มีแพ็ก" |
| `unknown` | no status or host lost | "ยังไม่ทราบสถานะ" |
| `demo` | event came from the demo trigger | "DEMO · ไม่ใช่เหตุการณ์จริง" always shown |

  The overlay window cannot call commands, so it reads status by `listen`ing to a new `overlay://status` event the host emits on change (PROPOSED; a read-only mirror of `runtime-status.media`) rather than `invoke`. If emitting it is too invasive for the host files, the fallback is a small `invoke` allowed for the overlay capability for `runtime_get_status` only.
- **Exclusive fullscreen** (SPEC: no injection; platform fact): a normal window cannot draw above a game in exclusive fullscreen. Dota 2 must run in **borderless windowed** ("Fullscreen windowed"). The control window's setup guide and the owner how-to say this; the overlay itself cannot detect it (UNVERIFIED), so the guide states it as a precondition, not a detected state. A visible chip for it is not possible without reading the game process, which is excluded.

## 5. Demo trigger without a game

Finding (FROM-REPO): replaying GSI payloads into the loopback intake **cannot** produce an event for an arbitrary fixture. `admit()` returns `Ownership::Unverified` unless the signed-in Steam account id is read and equals the payload's `player.steamid` (`kohbor-gsi/admission.rs`; intake reads it with `platform_steamid64()`), and the detector only runs on `Verified` ticks (`intake.rs`). `health-danger-golden.json` carries HP traces, not full GSI bodies. So a loopback poster would need the owner's real Steam id: not suitable as a no-game demo.

PROPOSED: a **demo mode inside the host**, enabled only when the environment variable `KOHBOR_DEMO=1` is set at launch (works in a release build; absent means the command does not exist in the invoke handler's accepted set and the Demo panel is hidden). It offers one command `demo_trigger(event_id, tier)` that builds a `TickMsg` with the catalog event and offers it to the engine exactly as intake does (`offer_tick`), so the **real arbiter, cooldown, native audio, `playback://event`, kohbor-media and overlay** run; only the GSI detection is replaced. Every demo playback is flagged `demo` (field on the overlay status event and in the history source label) so it cannot be mistaken for a detection, and the chip says so. The control window shows a "Demo" panel (only with `KOHBOR_DEMO=1`) with a button per event. It sends no network traffic and no input to the game. Demo mode does not alter any detection threshold, readiness rule or the history of real events (they are labelled).

Real Dota 2: the app already has `gsi_setup` (writes the GSI cfg into the Dota install). The how-to covers: run setup in the Command Deck, set Dota 2 to borderless windowed, start a match, see the banner on a detected event. Which event fires first is the detector's: `hero_kill`/`first_blood` need confirmed ownership (E-1 open, `first_blood` is held back).

## 6. Pack content for the demo

SPEC: the only entry from the lab is `kohbor-pack-build`; output stays outside the repo; the tool refuses a path inside a git worktree. `lab:fable/mapping.json` binds six banners to catalog events (`multi_kill` tiers 2 and 3, `first_blood`, `match_started`, `hero_kill`, `health_danger`); `lab:fable/demo-package/` already holds a built zip. A dev pack for this demo is built from those WebM and WAV files into a folder under the lab or the coordination temp area. Facts to check, not assume: the pack tool binary's date predates audio-only bindings (lab card), and the clips' VP9 alpha decode in WebView2 is **UNVERIFIED** (lab card Next). Authorization of any clip as critical media (DEC-CRITICAL-MEDIA) is the owner's; the dev pack does not make readiness `ready` for the critical path and the chip says what the host reports.

## 7. Evidence plan

Source and synthetic only: pure tests (window flags table, placement math, chip reducer, scheduling), Rust unit tests for the demo gate and window config, `python tools/gate.py`, a launch smoke with a temporary data dir, screenshots of `?view=overlay&mock=1` on the dev server (synthetic UI on mocks). **Not claimed:** transparency, click-through, no-focus-steal, always-on-top over Dota 2, VP9 alpha decode, audio-video offset, FPS, exclusive fullscreen behaviour. Those are the owner's native and live steps (TP-05, UAT-05).

## 8. Blocking owner decisions and open items

None block building the slice. Owner input needed to *close* it: (1) Q-UI-2 banner size binding (720 x 220 used); (2) Q-UI-1 own-stat row (excluded here); (3) DEC-VISUALS for the look; (4) whether `KOHBOR_DEMO=1` demo mode is acceptable in release builds (PROPOSED: yes, env-gated, labelled); (5) the safe-area fractions need a measurement on the owner's screen; (6) rights and authorization of the lab clips and any use as critical media stay with the owner's rights team.

## 9. As built (commit ae8cef6 and later; synthetic evidence only)

Where the build differs from sections 2 to 6 (the build wins):

- The overlay window is built in code (`lib.rs` `build_overlay_window`, flags from `overlay::overlay_window_spec()`), not declared in `tauri.conf.json`; the config file is unchanged. Capability `capabilities/overlay.json` grants `core:event:default` only. The window is 720 x 256 logical (stage 720 x 220 plus a 36 px chip strip), sized in physical pixels from the primary monitor (`overlay::place_overlay`, tested for 1024x768 to 3840x2160, ultrawide and secondary monitors).
- Visibility: the overlay renderer decides (`overlayShouldShow`) and emits `overlay://visible`; the host calls `show()` or `hide()` and re-applies click-through. The renderer emits `overlay://ready`; the host replays the last `overlay://status`. UNVERIFIED on Windows: whether a hidden WebView2 window runs the renderer promptly enough (a late first banner would show here), and whether `show()` on a non-focusable window takes no focus. If a hidden webview proves too slow, the fallback is to keep the window shown and fully transparent, at an unmeasured FPS cost.
- Chip states: `no_pack` is folded into `not_ready` with reason `MEDIA_UNMAPPED` (the host reports the same fact). A `playback_ended` input ends the demo chip.
- Demo: compiled only with cargo feature `demo` or in debug builds, and enabled only with `KOHBOR_DEMO=1`. `Host::demo_offer` offers a `TickMsg` with session epoch 1 and the reserved generation `u64::MAX` to the real engine; history rows whose instance id carries that generation are labelled DEMO. Intake, admission and `Ownership::Verified` are untouched (source tests assert it). A demo cannot fence a real match (epoch 1); during a real session at a higher epoch a demo tick is dropped as stale. Only event ids the detector can emit (`kohbor_events::EventId`) can be triggered.
- Risk strip mount: an empty `#overlay-risk-strip` (720 x 28 logical, bottom edge of the stage, hidden while empty) is reserved for the MVP-1 integration; the window height is unchanged.
- Not run: a launch smoke of the real app. The application data directory has no override, so a launch would use the owner's real data directory; the single-instance guard also applies. Synthetic screenshots of `?view=overlay&mock=1` on the dev server stand in; the mock URL has no real media, so the stage is empty there.
- Demo data directory: in demo builds, `KOHBOR_DATA_DIR=<absolute path>` (honoured only together with `KOHBOR_DEMO=1`; release builds ignore it) moves the whole application data directory, so the owner can try the demo without touching the real packs, settings or GSI token. Not verified in a launch.
