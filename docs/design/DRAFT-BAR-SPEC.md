---
title: "Draft Bar: hero and player-name reading during hero selection (design and third-party data contract)"
doc_id: "DRAFT-BAR-SPEC"
status: draft
version: "0.1.0"
updated: "2026-11-14"
authority: "Lead design for MVP-2 slice 5 under the owner's decision DEC-DRAFT-BAR (2026-11-14). Elaborates DATA-005 (enemy observation boundary) and the SECURITY-PRIVACY rule that other players' data needs a separately versioned contract and permission change. Changes no requirement statement."
approval_status: "agent_decision_not_owner_approved"
---

# Draft Bar

A local feature: during HERO_SELECTION, STRATEGY_TIME and TEAM_SHOWCASE the app reads the top hero bar of the Dota 2 window from the screen and shows the ten players' heroes and names in the deck. Evidence class of everything below: design only; nothing has been built or run.

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

## 4. Third-party data contract: "draft-bar projection 1.0.0" (PROPOSED; required before 5c ships)
Third-party personal data = other players' nicknames (and ranks if ever read). Contract:
1. Source and purpose: read locally from the screen only to show the owner who is in the match. No other purpose.
2. Lifetime: held in memory for the current match only; cleared at match end, on session change, and on app exit. A crash must leave nothing (no dump of these structures).
3. Forbidden sinks (hard rules, each with a test): disk (settings, caches, logs, crash reports, receipts), network, events and the event catalog, announcer text and clips, audio history, share/export files, peer projections, the overlay window, runtime-status, any contract schema other than this one.
4. Display: the control window deck only. Never in the overlay.
5. Control: a visible setting "แสดงชื่อผู้เล่น" (show player names) controls the name path; default ON per the owner's decision, but the choice is visible and one click; turning it off clears the memory immediately and stops the OCR.
6. Identification of heroes (5b) is not personal data; it follows the same lifetime and sink rules for simplicity.
7. Reference portraits: built at runtime from the user's installed Dota 2 files read-only into memory or a local cache that is not committed and not shipped; if the user's files cannot be read the hero identification is unavailable (unknown), never bundled art.
8. Any change to this list (a new sink, longer lifetime, ranks) needs a new contract version and an owner decision.

## 5. Open points for the feasibility spike (5a/5b)
- Exact rectangle of the bar and the ten portrait boxes at 1920x1080 (needs a real frame; measured from an owner screenshot, UNVERIFIED on other patches).
- Whether the installed game files can be read for portraits without a VPK parser dependency (a new dependency needs the owner's OK); if not, options are reported.
- OCR engine choice (Windows.Media.Ocr is built into Windows 10/11 and needs no dependency: UNVERIFIED for Thai/other-script nicknames); accuracy on stylised Dota fonts is unknown.
