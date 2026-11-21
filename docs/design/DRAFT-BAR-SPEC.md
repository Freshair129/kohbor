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

The 1.1.0 change added a display sink for the admitted hero projection. The 1.2.0 change adds strike-out of already-picked heroes to the read-only suggestions list, using 5b identities only. It requires no migration of Draft Bar data, which is held only in memory. The existing deck display continues to use the local Draft Bar path.

## 5. Open points for the feasibility spike (5a/5b)
- **Geometry measured from one owner frame, patch of 2026-11-21:** `DB.owner-frame-heropick.webp` is saved at 1919x1072. Measurements are in that saved image's coordinates; the duplicated 1920x1080 output origin is one pixel left and eight pixels up, so the output coordinates add `(1, 8)`. The complete mirror strip is `(x=209, y=8, width=1494, height=119)` and includes the central clock gap plus player labels. Portrait-only matching boxes are 119x65 at `y=8`; Radiant x origins are `209, 333, 457, 581, 705`, and Dire x origins are `1088, 1212, 1336, 1460, 1584`. This single-frame measurement does not establish geometry for another resolution, scale or game patch; those remain unsupported until measured.
- **Portrait reference candidate:** The existing VPK reader uses the local roster and the root `panorama/images/heroes/` landscape textures. The upright `selection/` textures still serve the own-hero card. Actual top-bar alignment and matching accuracy require native draft observations.
- **Source traceability:** `apps/desktop/src-tauri/src/capture_host.rs` routes the crop and exposes the control-window response, `apps/desktop/src-tauri/src/draft_bar.rs` holds the in-memory matcher and lifecycle tests, and `apps/desktop/tests/draft-bar.test.ts` covers the deck projection. The SRS has no separate requirement for the deck mirror. UI-010 names the 5b dependency for its pick card and remains a separate, unverified requirement.
- OCR engine choice (Windows.Media.Ocr is built into Windows 10/11 and needs no dependency: UNVERIFIED for Thai/other-script nicknames); accuracy on stylised Dota fonts is unknown.
