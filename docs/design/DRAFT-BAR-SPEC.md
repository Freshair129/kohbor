---
title: "Draft Bar: hero and player-name reading during hero selection (design and third-party data contract)"
doc_id: "DRAFT-BAR-SPEC"
status: draft
version: "1.2.0"
updated: "2026-11-21"
authority: "Lead design for MVP-2 slice 5 under DEC-DRAFT-BAR. DEC-PICK-STATS (2026-11-21) approves additive 1.2.0 picked-hero strike-out display under UI-010, dependent on 5b."
approval_status: "owner_approved_projection_display_change_only"
---

# Draft Bar

A local feature: during HERO_SELECTION, STRATEGY_TIME and TEAM_SHOWCASE the app reads the top hero bar of the Dota 2 window from the screen and shows the ten players' heroes and names in the deck. UI-010 adds the pick-card display projection in §4.4.

## 1. Why capture (evidence)
GSI with the blocks Kohbor requests (provider, map, player, hero) carries only the owner's own team and own pick in these phases (captured payloads `crates/kohbor-gsi/tests/captured/hero-selection-no-pick.json`, `team-showcase.json`). The `draft` and `allplayers` blocks are not requested and are believed (UNVERIFIED) to be spectator-only. The owner decided not to rely on GSI for this (DEC-DRAFT-BAR).

## 2. Slices
| Slice | Content | ML |
| --- | --- | --- |
| 5a | Mirror strip: the top-bar rectangle of the same DXGI frame shown as a live picture in the deck during the draft phases | none |
| 5b | Hero identification: template matching of the ten portraits against reference portraits built at runtime from the user's installed game files (read-only); unknown when confidence is low | none (classical matching); a learned model only after DEC-MODELS |
| 5c | Player names: local OCR of the name boxes, shown in the deck only | OCR (local) |
| 5d | Player colour: the colour strip above each card, read as the nearest Dota 2 player colour (DEC-DRAFT-BAR revision 2) | none (colour distance) |
| 5e | Our team's lane role: the label under each own-team name, read by matching its icon against the five role icons built at runtime from the installed game files; Support and Hard Support are split by label width | none (template match) |

## 3. Capture rules
- Same duplicated frame and the same grabber as the minimap, a second sub-rectangle; only while GSI says HERO_SELECTION, STRATEGY_TIME or TEAM_SHOWCASE (the minimap stays off in those phases, and the draft bar stays off in PRE_GAME and later). No game memory read, no injection, no input.
- 1920x1080 at 100 percent only (like the minimap); any other output is unsupported with the real reason shown.
- Cost (CPU, GPU, per-cycle ms) is measured and reported in the resource readout; no limit is set (DEC-RESOURCE-BUDGET).
- Low confidence is shown as unknown; nothing is guessed.

## 4. Third-party data contract: "draft-bar projection 1.2.0" (display changes approved under DEC-PICK-STATS)
Third-party personal data = other players' nicknames (and ranks if ever read). Contract:
1. Source and purpose: read locally from the screen only to show the owner who is in the match. No other purpose.
2. Lifetime: held in memory for the current match only; cleared at match end, on session change, and on app exit. A crash must leave nothing (no dump of these structures).
3. Forbidden sinks (hard rules, each with a test): disk (settings, caches, logs, crash reports, receipts), network, events and the event catalog, announcer text and clips, audio history, share/export files, peer projections, runtime-status and any contract schema other than this one. The overlay is a forbidden sink for names, ranks and raw Draft Bar data. The §4.4 display projection is the sole exception for the overlay and its dedicated view contract.
4. Display: player names, ranks and the raw mirror strip remain in the control window deck. Only locally admitted hero identities and numbers derived locally from them may appear in the display-only pick card under UI-010, after the own pick in HERO_SELECTION or STRATEGY_TIME. V2 waits for 5b. V2 may strike out already-picked heroes in the pre-pick suggestions list using only admitted 5b hero identities; no matchup edge is included until an adequate data source is approved. Other players' names never appear in the overlay.
5. Control: a visible setting "แสดงชื่อผู้เล่น" (show player names) controls the name path; default ON per the owner's decision, but the choice is visible and one click; turning it off clears the memory immediately and stops the OCR.
6. Identification of heroes (5b) is not personal data; it follows the same lifetime and sink rules, with the display exception in §4.4. Draft Bar output, including ally and enemy hero IDs and numbers derived from those IDs, never goes to network or disk. The separate STATS-002 route sources the own hero ID from admitted GSI and permits only that ID and an optional user-chosen bracket.
7. Reference portraits: built at runtime from the user's installed Dota 2 files read-only into memory or a local cache that is not committed and not shipped; if the user's files cannot be read the hero identification is unavailable (unknown), never bundled art.
8. Any change to this list (a new sink, longer lifetime, ranks) needs a new contract version and an owner decision.

9. Per-slot roster (1.3.0, DEC-DRAFT-BAR revision 2, owner 2026-11-23): player colour (5d), our team's lane role (5e) and the derived own slot join the hero (5b) in one profile per slot. Colours and lane roles are not personal data, but they follow the same lifetime and forbidden sinks as every Draft Bar output: memory only, cleared at match end and on session change, and shown in the control-window deck. The player-name field stays `not_built` until 5c ships under this contract. Nothing about names changes.

The 1.1.0 change added a display sink for the admitted hero projection. The 1.2.0 change adds strike-out of already-picked heroes to the read-only suggestions list, using 5b identities only. It requires no migration of Draft Bar data, which is held only in memory. The existing deck display continues to use the local Draft Bar path.

## 5. Open points for the feasibility spike (5a/5b)
- **Geometry measured from one owner frame, patch of 2026-11-21:** `DB.owner-frame-heropick.webp` is saved at 1919x1072. Measurements are in that saved image's coordinates; the duplicated 1920x1080 output origin is one pixel left and eight pixels up, so the output coordinates add `(1, 8)`. The complete mirror strip is `(x=209, y=8, width=1494, height=119)` and includes the central clock gap plus player labels. Portrait-only matching boxes are 119x65 at `y=8`; Radiant x origins are `209, 333, 457, 581, 705`, and Dire x origins are `1088, 1212, 1336, 1460, 1584`. This single-frame measurement does not establish geometry for another resolution, scale or game patch; those remain unsupported until measured.
- **Remeasured 2026-11-23 (live test on 8d9f8c3 found 0/10):** the box was searched against references built from the installed game, on two owner STRATEGY_TIME frames at 1920x1080 (a050ae8 and 6195c3c). The result is strip `(x=209, y=6, width=1504, height=119)`, portrait boxes 119x68 at `y=6`, Radiant x origins `209, 333, 457, 581, 705` and Dire x origins `1098, 1222, 1346, 1470, 1594`. The first measurement put Dire 10 px left. Hero selection itself has not been remeasured.
- **References and preprocessing (2026-11-23):** the landscape portraits `panorama/images/heroes/npc_dota_hero_*_png` are DXT5 with "Image YCoCg Conversion" (95 of 127 in the installed build). They are converted from scaled YCoCg to RGB before matching. Before this they were matched as raw YCoCg channels, and no slot could identify. Only the top 58 % of each card is compared, because the rank banner, medal and info icon cover the lower part of every live card and are not in the reference art. Alternate-art (persona, arcana) cards do not match a base portrait and stay unknown. The match thresholds are unchanged (confidence 0.82, margin 0.08).
- **Logs:** `draft_references_built` gives the roster size, references built, the number converted from YCoCg, missing, unreadable and decode-failed counts, and a failure code. It never contains a path. `draft_bar_scores` gives the per-slot best score in hundredths and a reason, with the threshold, on change and at most every 5 s. It never contains a hero name.
- **Portrait reference candidate:** The existing VPK reader uses the local roster and the root `panorama/images/heroes/` landscape textures. The upright `selection/` textures still serve the own-hero card. Actual top-bar alignment and matching accuracy require native draft observations.
- **Source traceability:** `apps/desktop/src-tauri/src/capture_host.rs` routes the crop and exposes the control-window response, `apps/desktop/src-tauri/src/draft_bar.rs` holds the in-memory matcher and lifecycle tests, and `apps/desktop/tests/draft-bar.test.ts` covers the deck projection. The SRS has no separate requirement for the deck mirror. UI-010 names the 5b dependency for its pick card and remains a separate, unverified requirement.
- OCR engine choice (Windows.Media.Ocr is built into Windows 10/11 and needs no dependency: UNVERIFIED for Thai/other-script nicknames); accuracy on stylised Dota fonts is unknown.

## 6. Per-slot roster (DEC-DRAFT-BAR revision 2, owner direction 2026-11-23)

Each top-bar slot (Radiant 0–4, Dire 0–4) is an independent profile in `draft_roster::SlotRoster`. One slot's failure never blocks another, and a field that cannot be read stays unknown with a reason.

| Field | Source | Value | Unknown when |
| --- | --- | --- | --- |
| `hero` | 5b | `npc_dota_hero_*` | below the match thresholds, empty slot, references unavailable |
| `player_colour` | 5d | palette name and median RGB | no strip yet (`no_player_colour`), strip not uniform |
| `lane_role` | 5e, our team only | `safe_lane`, `mid_lane`, `off_lane`, `support`, `hard_support` | enemy team, own side unknown, no label, low confidence, icons unavailable |
| `player_name` | 5c | not built | always `not_built` for now |
| `own_slot` | GSI team and own hero plus 5b | slot 0–9 | own side or hero unknown, own hero not identified, more than one match |

- Every field carries `value`, `confidence`, `observed_at_ms` (unix ms), `source` and `reason`.
- A known value is kept when a later reading is unknown; the strip leaves the screen after the draft. The roster is replaced only by a newer known reading, and it is cleared with the session.
- `own_slot` is never guessed from screen position.
- The roster rides in the control-window packet (`draft_bar_read`) as `roster` while the draft bar is active; the deck parses it with `parseSlotRoster`. Second-monitor hero cards, minimap icons, the draft board, counters and in-game top-bar CV will consume it later.
- Info log `draft_roster_changed` gives counts only: heroes, colours, lanes and whether the own slot is known. It never contains a name, hero or colour.

**Measured boxes (2026-11-23).** These come from three owner 1920x1080 frames: a Steam original in the pick phase (2026-11-19, patch 7.42) and STRATEGY_TIME frames from a050ae8 and 6195c3c. Frames with player names stay outside the repository.
- The strip is now `(x=209, y=0, width=1504, height=125)`. It carries the colour bar and the lane labels. Portrait boxes are unchanged (119x68 at `y=6`).
- **Colour strip:** rows `y=0–5` across the slanted card top. The sample box is slot `x+25` to `x+84`, `y=1–4`. The median colours match the Dota 2 player palette: blue, teal, purple, yellow, orange (Radiant 0–4) and pink, olive, light blue, dark green, brown (Dire 0–4). Every sample in all three frames was within 6 RGB units of its palette entry; an empty pick slot has no strip.
- **Lane label:** under our team's names only, icon and text within `y=107–120`. The read box is slot `x` to `x+124` (to the strip edge for the last Dire slot), `y=104–123`. Label widths are about 58 (Support), 59 (Off Lane), 64 (Mid Lane), 66 (Safe Lane) and 90 px (Hard Support). The icon is the leading 13 columns.
- **Role icons** are built at runtime from `panorama/images/rank_tier_icons/handicap/{safelane,midlane,offlane,softsupport,hardsupport}icon_psd`, five 100x100 images, cropped to their alpha box and resampled to 16x16.
- **Offline result on the three frames:** 29 of 30 colours read correctly (the empty pick slot stays unknown) and 15 of 15 own-team lane roles read correctly. Icon scores were Off Lane 0.91, Safe Lane 0.90, Support 0.61–0.73, Mid Lane 0.58–0.61 and Hard Support 0.46. The acceptance floor is 0.45 with a 0.08 margin; Hard Support is the weakest and falls back to unknown if it drops. `own_slot` was found where 5b identified the own hero (Pudge frame: slot 5) and stayed unknown otherwise.
