---
title: "Draft column: pre-pick suggestions list, after-pick card and v2 strike-out (build design)"
doc_id: "DRAFT-COLUMN"
status: draft
version: "0.1.0"
updated: "2026-11-21"
authority: "Lead build design under DEC-PICK-STATS revision 2 for UI-010 and STATS-001..003. Requirements, decisions and DRAFT-BAR-SPEC 1.2.0 section 4 win on conflict. Agent design, not owner approval."
owner_role: "Engineering lead"
approval_status: "not_approved"
---

# Draft column

Build design for the in-game draft column approved under DEC-PICK-STATS revision 2 (owner, 2026-11-21, "Approve all 11"). Sources: UI-010, STATS-001..003 (`contracts/requirements.json`), [Draft Bar §4](DRAFT-BAR-SPEC.md) projection 1.2.0, [SECURITY-PRIVACY](../security/SECURITY-PRIVACY.md) "Public hero statistics request and cache", [UX-SPEC](UX-SPEC.md). Measured geometry and the provider facts read on 2026-11-21 are in the coordination folder `design/draft-suggest-20261121/DESIGN.md` (sections 2, 3, 4, 5) and `design/pick-card-20261120/DESIGN.md` (sections 2, 3, 4).

Labels: **MEASURED** (from that design's owner frame or live read), **DECIDED** (lead build choice in this file), **UNVERIFIED**.

## 1. Pieces

| Piece | Where | What |
| --- | --- | --- |
| Draft window | native host, label `draft`, `index.html?view=draft` | Transparent, always on top, click-through, never focusable or activated, skip taskbar. Placed exactly on the column rectangle. Capability `draft.json`: `core:event:default` only. Shown only while it has something to draw. |
| Hero stats host | `src-tauri/src/hero_stats/` | Prefs, closed outgoing requests, transport trait (WinHTTP implementation, mock in tests), strict parsers, cache, request budget, position groups, ranking, view model, phase gate. |
| Renderer | `src/draft/` | Overlay stage (`?view=draft`), deck panel with the same view, Settings section. Draws the view model; computes nothing that the host decides. |

The renderer never makes a provider request. Only the native host does.

## 2. Placement (DECIDED from MEASURED)

`s = H / 1080`. Column: `x = W − 192·s`, `y = 132·s`, `w = 180·s`, `h = content·s`. At 1920×1080: x 1728–1907, from y 132 down to no lower than y 860.

**Owner choice 2026-11-22: Option A, sectioned cards** (owner's reply "Option A", relayed by the lead after the owner reviewed both mocks in `F:\kohbor-coord\design\draft-column-v2-20261122\`). The owner had said the 8.5 px column was too small to read and asked for it to grow downward in sections. The design:
- Separate cards, from the top: brand bar (position, bracket, status pill) → `แก้ทาง {enemy}` (when ready) → `ฮีโร่เมต้า` or `การ์ดฮีโร่` + `ตามความยาวเกม` → source footer.
- Text is at least 12 px: names and rates 15 px, headings 15 px.
- Colors are Command Deck tokens.
- Every block has a fixed height: brand 30, head 50 (counters 70), meta card 70, counter card 70, message 88, pick body 178, durations 140, footer 101, chip 56, gap 6, card border 2. Text is clamped or ellipsized inside its block.
- The host computes the window height from the view (`hero_stats::layout::overlay_column_height`, mirrored by `overlayColumnHeight` in `draftColumnModel.ts`, with a test that keeps the two in step) and sizes the native window to it. `column_window_placed` logs the result.
- State heights: meta 685, counters + meta 693, pick card 571, fetch failed 283 (571 with counters), consent chip 56. The cap is 728, which ends at y 860. Until card v3 (2026-11-23) they were 578, 692, 554, 266 and 530.
- The deck keeps its own layout.

**Card v3, owner choice 2026-11-23: Variant B** (owner's reply "Variant B", relayed by the lead after the owner reviewed three mocks in `F:\kohbor-coord\design\draft-card-v3-20261123\`; the real component is shot in `final\` there). Meta and counter rows are now cards of 70 px:
- Portrait: the upright selection image from the installed game, through the existing `hero_portrait` command (`portraits.rs`, read-only, memory only, no bundled or remote art). It is 70×70 and fades into the card. A missing image shows a neutral "?".
- Card backgrounds are opaque (`#141414`), so the game's text behind cannot show through.
- Win-rate tone, Command Deck tokens: the right edge (4 px) and the rate are `--ally` at ≥ 50.0 + 1.0, `--critical` at ≤ 50.0 − 1.0, otherwise neutral. A chip shows the difference from 50.0, computed from the rate already shown. No rate → no tone.
- Lane line, for example `Off 71% · ชนะ 51.9%`: the hero's share of its games in the lane of the selected position (over the 3 lanes), then its win rate in that lane, shown only with ≥ 100 lane games (`LANE_MIN_GAMES`). Both come from the cached laneRoles table; no new request. The lane table has 3 lanes and no bracket, so Pos 1 and 5 share Safe, Pos 3 and 4 share Off, and the footer says so (`Pos 3+4 = เลน Off ทุกแรงก์`). With no position set there is no lane line.
- Rows drawn in the overlay: 7 meta cards with no counters; with k counter cards (k ≤ 3), 6 − k meta cards. Before card v3: 8 and 8 − k.
- The footer has five lines; the lane caveat is the new one.

- The overlay draws only at 1920×1080, 100 % scale (same rule as Draft Bar 5b: `draft_bar::OUTPUT_WIDTH/HEIGHT/SCALE_PERCENT`). Otherwise the state is `unsupported_output`: the overlay draws nothing and the deck shows the list with the reason.
- Protected rectangles at 1920×1080 (MEASURED, one frame 1919×1072, up to 8 px uncertainty): top bar 215–1708 × 0–117, hero grid 133–1292 × 181–698, hero detail panel 1344–1719 × 181–786, LOCK IN 1345–1614 × 794–851, RANDOM 1627–1719 × 794–851, chat 838–1323 × 869–1062. A test asserts the column touches none of them and keeps ≥ 8 px to the detail panel and ≥ 12 px to the right screen edge.
- The after-pick card uses the same column in hero selection and strategy time. `PICK_CARD_STRATEGY_TIME_RECT` is now the largest column rectangle (180×728 at 1728,132). The strategy-time check covers only two owner frames (Pudge on a050ae8, Lion on 6195c3c), where the right edge is clear from about y 130 down to the bottom. Hero selection below y 698 has not been checked against an owner frame; the protected rectangles from the measured frame all end at x ≤ 1719.

## 3. Phase gate (DECIDED)

Exact draft screen from GSI `map.game_state`: `HeroSelection` = `DOTA_GAMERULES_STATE_HERO_SELECTION`, `StrategyTime` = `DOTA_GAMERULES_STATE_STRATEGY_TIME`; anything else (including TEAM_SHOWCASE, loading, pre-game) is no draft screen.

| Condition | Column |
| --- | --- |
| no admitted session, ownership not verified, spectating, replay, GSI stale > 10 s, not a draft screen | hidden |
| draft screen, own hero not yet known | suggestions list, or the consent chip (8 s once per app run) when consent is off |
| draft screen, own hero known | pick card (hard swap in the same update; the list is removed) |
| in-game bar switch off | overlay hidden; the deck still shows the view |

A repick or swap removes the previous hero's values in the same update (UI-010 failure behaviour).

With consent off, the overlay shows only the 8 s chip. After that the deck says `ยังไม่ได้เปิดการยินยอมสถิติ` and names the switch (`ดึงสถิติฮีโร่จากอินเทอร์เน็ต`).

**Draft Bar capture freshness (fix 2026-11-22, live test on a050ae8).** The own snapshot has `source.age_ms = null` outside Pregame/Live. The capture host's draft gate read that age, so 5b never opened a grabber during a live draft: `grab_fps` was 0 and the phase stayed `idle` until pregame. The gate now reads `draft_age_ms`, which is the intake age of the latest admitted payload, set only while `draft_bar_allowed`. The own-snapshot contract is unchanged. Regression test: `intake::tests::captured_hero_selection_payload_opens_the_draft_bar_gate`. It replays the redacted M0 hero-selection capture, so it is synthetic-replay evidence, not a live check.

**Overlay column scale (fix 2026-11-22, owner frame on a050ae8: "มันเล็กมากมองไม่เห็น").** The host sizes the native window to the column rect itself, 180×488 at 1728,132. The overlay stage then applied the full-screen formula `draftColumnPlacement(innerWidth, innerHeight)` to that 180×488 viewport. That gave scale 488/1080 ≈ 0.452 at x 93, y 60, so the column drew about 81 px wide at screen x 1821, y 192. The stage now uses `draftStagePlacement`: a viewport narrower than it is tall is the column window, so the column fills it (since Option A: a viewport under 600 px wide is the column window, at scale width/180, 1 at 1920×1080). A screen-shaped viewport, such as the browser mock, keeps the screen formula. That mock is why the earlier 1920×1080 measurement looked right. Measured after the fix in a 180×488 in-app browser viewport: rect 0,0 180×488, scale 1. Native rendering is not yet checked.

**Info logs for the next live test.** All are emitted on change only and carry codes and counts, never hero names or pixels.
- `capture_host/draft_capture_started`
- `capture_host/draft_capture_skipped` with `reason`. The reason is either a gate code (`not_draft_phase`, `draft_bar_not_allowed`, `gsi_not_fresh`, `no_session_epoch`) or the capture withdraw reason (for example `no_game_window`, `draft_window_visibility_unconfirmed` or a DPI message).
- `capture_host/draft_capture_ended`
- `capture_host/draft_bar_identified` with `identified` and `slots`
- `draft/column_shown` and `draft/column_hidden`, each with `reason`, `mode`, `data_state`, `consent`, `draft_bar_available` and `identified`
- `draft/column_window_placed`, logged when the shown window changes, with the actual native `x`, `y`, outer and inner size, and `scale_factor`

## 4. Settings and consent (DECIDED)

Stored in the data directory as `draft-stats-prefs.json` (schema 1, deny unknown fields; a missing or corrupt file reads as the defaults and never as consent):

| Field | Default | Meaning |
| --- | --- | --- |
| `hero_stats_consent` | `false` | STATS-001 switch "ดึงสถิติฮีโร่จากอินเทอร์เน็ต". Only this field enables provider requests. |
| `draft_bar_enabled` | `true` | Separate in-game bar on/off. Never enables requests. |
| `position` | `null` | `null` = ทุกตำแหน่ง; 1–5 = estimated group. |
| `bracket` | `null` | `null` = ทุกแรงก์ (`pub_*`); 1–8 = Herald … Immortal (`N_pick/N_win`). |

Only the control window may read or change these (command window-label check, as for orb prefs). Contract follow-up: these fields are not yet in `contracts/user-settings.schema.json`; the lead session owns that change (DEC-INTERFACES).

Revocation (consent true → false): increment the consent generation, cancel in-flight work, drop any late response whose generation is old, clear displayed statistics, delete the cache file. A deletion failure is reported in the view (`cache_delete_failed`) and the cache stays inaccessible.

## 5. Outgoing requests (DECIDED, STATS-002)

Closed enum, no other URL can be built:

| Request | URL | Fields from the user or match |
| --- | --- | --- |
| `HeroStats` | `https://api.opendota.com/api/heroStats` | none |
| `LaneRoles(1|2|3)` | `https://api.opendota.com/api/scenarios/laneRoles?lane_role=N` | none (fixed constants) |
| `OwnHeroDurations(hero_id)` | `https://api.opendota.com/api/heroes/{hero_id}/durations` | the admitted own hero ID only (pick-card flow) |

- Public tables: fetched only when the app starts with consent on, when consent is turned on, or on the deck's retry; never at draft start. At most 4 table requests per rolling 24 h, counted in a persisted ledger that survives restarts. A complete set fetched < 24 h ago is not refetched.
- Own-hero durations: when the admitted own hero becomes known in a draft screen and the cache has no fresh entry; at most 30 per hour and 200 per rolling 24 h; never with the bracket (the endpoint takes none).
- One request in flight. 429 → `rate_limited` with `Retry-After` if given (else 1 h); no automatic retry.
- **Counting rule (DEC-PICK-STATS revision 4, owner "Fix the counting rule", 2026-11-22).** A table request counts toward the four per rolling 24 h once the host attempts to connect or send, whether it succeeds or fails (connect failure, timeout, HTTP error, oversize or invalid body). A request that fails during local WinHTTP setup (`TransportError::NotSent`) or is cancelled before send (`CancelledBeforeSend`) does not count. The ledger entry is still written at dispatch, so a crash mid-request stays counted. `finish` removes that one entry again for a pre-I/O failure and logs `fetch_not_counted`; `fetch_failed` carries `counted`. The cap is unchanged. Own-hero duration budgets are not changed by this rule.
- Transport: HTTPS to the single host `api.opendota.com:443`, redirects disabled, cookies disabled, generic `User-Agent: Kohbor`, 4 s connect / 10 s total, body cap 4 MiB. Tests use a mock transport; no test touches the network.
- **Fix 2026-11-22 (owner build 6195c3c).** Every live table request failed as `fetch_failed` before any I/O. The cause was the session handle: `WINHTTP_OPTION_DISABLE_FEATURE` was set on it and returned `ERROR_WINHTTP_INCORRECT_HANDLE_TYPE` (12018). That option is valid only on request handles, and the request handle already disables cookies. The ledger still counted both attempts, on 2026-11-22 at 08:10Z and 09:16Z. Regression test: `transport::setup_tests::local_winhttp_setup_succeeds_without_any_network_io`, which opens the handles only and sends nothing.
- One manual check on 2026-11-22 from the owner's machine: `GET /api/heroStats` with `User-Agent: Kohbor` and no proxy returned HTTP 200 with 167,550 bytes. The current parser reads it as 127 heroes with brackets 1–8 and `pub`. The raw body was not saved to the repo. The lane-role tables were not fetched, so their live shape is still unverified.
- Info logs carry the kind only (`hero_stats`, `lane_roles_N`, `own_durations`) and never a URL, hero ID or body:
  - `draft/fetch_started`
  - `fetch_skipped` (`budget` or `ledger_write_failed`)
  - `fetch_ok`
  - `fetch_failed`, with class `network`, `timeout`, `oversize`, `cancelled`, `http_status`, `rate_limited`, `parse` or `cache_write`
  - `fetch_transport_error`, with the WinHTTP `stage`, the `win32` code, and a class (`dns`, `connect`, `timeout`, `tls`, `setup`, `cancelled` or `network`)

## 6. Validation and cache (DECIDED, STATS-003)

- Parsers are strict: required fields with the right types; laneRoles `games`/`wins` are decimal strings and parse to integers or the table is invalid; `wins ≤ games` / `win ≤ pick`; hero IDs positive; duplicate rows summed only where the provider splits by `time`. An invalid body is `provider_error` (`invalid_body`); nothing partial is used.
- Durations response shape (`duration_bin`, `games_played`, `wins`) is **UNVERIFIED** against a live read; a mismatch shows `provider_error`, never a guess.
- Cache file `hero-stats-cache.json`: the three lane tables, the hero table and up to 20 own-hero duration entries (least recently used dropped), each with `fetched_at_ms` (UTC). Fresh < 24 h, stale 24 h–< 7 d, discarded ≥ 7 d. No user, match or Draft Bar data. Never logged.
- Data window: the provider states none → shown as `ไม่ระบุช่วง`.

## 7. Position groups (rule B, DECIDED before data, not to be loosened)

Per hero, sum `games` over the `time` rows of each lane table (1 safe, 2 mid, 3 off). `total = safe + mid + off`. A hero with `total < 200` is in no group. The hero is in lane L if `games(L) ≥ 100` and `games(L) ≥ 25 %` of `total`. `support` = `"Support"` in the hero's `roles` from heroStats.

| Group | Rule |
| --- | --- |
| 1 | safe and not support |
| 2 | mid |
| 3 | off and not support |
| 4 | off and support |
| 5 | safe and support |

A hero may be in several groups. Every group is labelled `ค่าประมาณ`. The win rate shown is the hero's rate in the bracket across all positions, and the card says so. The lists produced by this rule are written for the owner in `design/draft-suggest-20261121/POSITION-GROUPS.th.md` before the list is enabled in a build (`SUGGESTIONS_LIST_BUILD_ENABLED`).

**Override list (owner option B, 2026-11-22).** After rule B, the host applies the reviewed Kohbor-owned list in `apps/desktop/src-tauri/src/hero_stats/position-overrides.json`. The list adds to rule B and never replaces it.

- Each row holds the hero id and the internal name, the positions to remove and to add, a reason, the patch (`7.42`) and the date.
- A row applies only when both the id and the internal name match the provider's heroStats row. Otherwise the row is skipped and rule B stays for that hero.
- If the list fails validation it is not applied at all. Validation fails on: a schema other than 1, a position outside 1–5, the same position in remove and add, a row that changes nothing, an empty text field, or a duplicate hero.
- The owner reviews the list each patch. The Thai table is in `POSITION-GROUPS.th.md`.

With the list in place, `SUGGESTIONS_LIST_BUILD_ENABLED` is `true`.

## 8. Ranking (DECIDED before data)

- Rate = wins / picks in the bracket column (`pub_*` for ทุกแรงก์). Ranked only with picks ≥ 1,000. The sort is meta first (owner, 2026-11-22): picks in the bracket desc, then rate desc, then hero ID asc. The list title is `ฮีโร่เมต้า`; until 2026-11-22 the sort was rate first and the title was `ฮีโร่ชนะบ่อย`. Picks are counted across all positions, because the lane table is not split by bracket. No matchup or counter logic. Shown as tenths of a percent (half up), computed from integers.
- Rows: walk the sorted group; emit until 8 untaken ranked rows; v2 taken heroes met on the way are emitted struck out, without a rank. If fewer than 8 heroes are ranked, append low-data heroes (picks desc) until 8 rows; their rate is withheld.
- Bracket column summing to 0 → `bracket_empty` (MEASURED: Immortal was 0 for every hero on 2026-11-21).
- Pick card: rate with ≥ 1,000 picks else `not_enough_matches`; form = rate − 50.0; duration cells < 30 min, 30–45, > 45 (bins merged by `duration_bin` seconds), each shown only with ≥ 300 games; the length sentence only when no cell is withheld and max − min ≥ 3.0.

## 9. v2 strike-out (DRAFT-BAR-SPEC 1.2.0)

Only heroes that Draft Bar 5b admits as identified in the ten slots are struck out. The view carries `draft_bar.available` and `identified/10`. 5b unavailable → v1 rows and the note "Kohbor ยังไม่เห็นว่าตัวไหนถูกแบนหรือถูกเลือกแล้ว". Bans are never known. 5b identities go only into the in-memory view model; never to disk, logs or the network. Player names never enter this view. No statistical matchup edge.

## 9a. Counter picks (DEC-PICK-STATS revision 3, option c; `COUNTERS_BUILD_ENABLED` = true since the owner approved the 7.42 table and the mock on 2026-11-22)

- **Source.** Only the reviewed static table `apps/desktop/src-tauri/src/hero_stats/counter-table.json` ships with the app.
  - Each enemy entry has a patch and a date. Each counter row has a reason.
  - The table is validated whole: any defect means no counters at all.
  - A row whose id disagrees with the cached provider heroStats row (same id, different internal name) is dropped.
  - No request is made for counters. Identified heroes are never sent, logged or stored.
- **Inputs.**
  - Enemies come only from identified 5b slots, via `draft_bar_snapshot`. Each slot carries its `team` (`radiant` for slots 0–4, `dire` for slots 5–9, from 5b).
  - The own side comes from admitted GSI `player.team_name`, and only with verified ownership during a draft phase.
  - With the own side unknown, or any identified slot's side unknown, the state is `sides_unknown` and no counter is shown.
- **States.** `build_disabled`, `draft_bar_unavailable`, `sides_unknown`, `no_enemy`, `ready`. Rows exist only in `ready`, and `ready` may still have zero rows.
- **Rows.**
  - Excluded: any hero identified in any slot, and, when a position is set and the groups are known, any hero outside that group (`position_filtered`).
  - Sort: the number of identified enemies answered (desc), then first appearance (enemy slot order, then table order), then hero ID.
  - The host keeps at most 6 rows. The deck draws all of them, with the reason. The overlay draws at most 3, on top, under `แก้ทาง {enemies}`; the meta list then shows 6 − k cards (card v3).
  - The counter relation has no number, and the source line says `ไม่ใช่สถิติ`. Since card v3 a counter card also shows the hero's own bracket win rate (≥ 1,000 games) and its lane line, as the meta cards do; neither is a matchup number.
- **Layout.** Since Option A (2026-11-22) the counters are their own card at the top: a two-line heading of 70 px and up to 3 rows. Since card v3 (2026-11-23) each row is a 70 px card with portrait, name, rate and the reason at 12 px, and the meta list shows 6 − k cards. Until 2026-11-22 the block was 96 px at 8.5 px inside a 488 px column.
- **Consent.** Counters appear inside the suggestions mode, which still requires the stats consent. The table itself needs no network.

## 10. View contract `draft-column-view` 1 (host → windows)

Event `draft://view` to the `draft` window; command `draft_column_view` for the control window. A field typed `int` is a required non-negative integer; a field typed `null | int` is nullable, and `null` means unknown, never zero. `suggestions.rows` holds at most 16 rows; a payload that violates any type, enum or bound is invalid as a whole. The overlay draws at most 7 rows since card v3 (8 before; the deck draws all rows). Lead decision 2026-11-21: the overlay rows go first to the top still-pickable (untaken) rows in host order; a struck-out (taken) row is drawn only if it ranks above a drawn untaken row and there is room left (that is, fewer untaken rows exist than the overlay can draw); otherwise struck-out rows are left out of the overlay. Host order is kept for the rows that are drawn. Text floor in the overlay: no text below 8.5 px at scale 1 (hero names 11 px, rates 11 px, games 8.5 px), from the design's measured minimum; readability in play is still unmeasured.

```text
{
  schema: 1,
  mode: "hidden" | "consent_chip" | "suggestions" | "pick_card",
  overlay: { visible: bool, hidden_reason: null | "unsupported_output" | "bar_off" | "no_draft" | "list_build_disabled" },
  phase: null | "hero_selection" | "strategy_time",
  placement_verified: bool,                 // false for the strategy-time card position
  data_state: "consent_off" | "loading" | "live" | "stale" | "fetch_failed" | "rate_limited" | "provider_error" | "bracket_empty",
  context: { position: null | 1..5, bracket: null | 1..8 },
  source: {
    provider: "OpenDota", window: null,
    fetched_at_ms: null | int, failed_at_ms: null | int,
    failure: null | "network" | "timeout" | "http_status" | "invalid_body" | "too_large" | "rate_limited" | "cache_delete_failed",
    http_status: null | int, retry_after_ms: null | int,
    table_requests_24h: int, table_request_budget: 4
  },
  suggestions: null | {
    rows: [ { hero_id: int, name: string, rank: null | int, games: int,
              win_rate_tenths: null | int, low_data: bool, taken: bool,
              hero_key?: null | string,   // card v3: npc_dota_hero_*, for hero_portrait; invalid -> null
              lane?: null | Lane } ],      // card v3; invalid -> null, the row stays
    ranked_count: int,
    draft_bar: { available: bool, identified: int, slots: 10 },
    counters: {            // added 2026-11-22; a renderer treats a missing or invalid block as null (no counters)
      state: "build_disabled" | "draft_bar_unavailable" | "sides_unknown" | "no_enemy" | "ready",
      source: "kohbor_reviewed_table",
      enemies: [ string ] (<= 5), enemies_identified: int (0-5), position_filtered: bool,
      rows: [ { hero_id: int, name: string, against: [ string ] (1-5), reason: string,
                hero_key?: null | string, games?: null | int,
                win_rate_tenths?: null | int,   // the hero's own bracket rate, >= 1,000 games
                lane?: null | Lane } ] (<= 6; empty unless "ready")
    }
    // Lane = { position: 1..5, lane: "safe" | "mid" | "off", shared_with: null | 1..5, games: int,
    //          share_tenths: null | int, win_rate_tenths: null | int (>= 100 lane games) }
  },
  pick_card: null | {
    hero_id: null | int, name: null | string,
    state: "live" | "stale" | "loading" | "unknown_hero" | "not_enough_matches" | "fetch_failed" | "rate_limited" | "provider_error" | "bracket_empty",
    games: null | int, win_rate_tenths: null | int, form_tenths: null | int,
    durations: { state: same set as above, fetched_at_ms: null | int,
                 cells: [ { bin: "short" | "mid" | "long", games: null | int, win_rate_tenths: null | int } x3 ] }
  }
}
```

## 11. Never shown

Win-chance %, predicted outcome, ban suggestions, a statistical matchup edge (counters are table reasons, never numbers), item/lane/skill advice, player names, ranks, item art. Since card v3 (owner, 2026-11-23) hero portraits from the installed game and the lane share and lane win rate are shown; they are statistics, not advice.

## 12. Evidence and open points

Synthetic only from this build. UNVERIFIED: card v3 portraits in the native overlay window (the `hero_portrait` call from the `draft` window is untested there; the browser mock shows "?"), lane numbers from a real laneRoles cache, overlay behaviour over Dota 2 (click-through, never activated, placement, readability of 8.5–12.5 px text), the strategy-time card position, the durations response shape, the provider's data window, per-IP quota and licence, Turbo/Captains Mode screens.
