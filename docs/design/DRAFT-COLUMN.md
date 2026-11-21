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

`s = H / 1080`. Column: `x = W − 192·s`, `y = 132·s`, `w = 180·s`, `h = 488·s`. At 1920×1080: x 1728–1907, y 132–619.

- The overlay draws only at 1920×1080, 100 % scale (same rule as Draft Bar 5b: `draft_bar::OUTPUT_WIDTH/HEIGHT/SCALE_PERCENT`). Otherwise the state is `unsupported_output`: the overlay draws nothing and the deck shows the list with the reason.
- Protected rectangles at 1920×1080 (MEASURED, one frame 1919×1072, up to 8 px uncertainty): top bar 215–1708 × 0–117, hero grid 133–1292 × 181–698, hero detail panel 1344–1719 × 181–786, LOCK IN 1345–1614 × 794–851, RANDOM 1627–1719 × 794–851, chat 838–1323 × 869–1062. A test asserts the column touches none of them and keeps ≥ 8 px to the detail panel and ≥ 12 px to the right screen edge.
- The after-pick card uses the same column in hero selection. Its strategy-time position is one named constant `PICK_CARD_STRATEGY_TIME_RECT` (initially the same column) marked **UNVERIFIED for strategy time** in code and in the deck: the strategy-time screen has not been measured.

## 3. Phase gate (DECIDED)

Exact draft screen from GSI `map.game_state`: `HeroSelection` = `DOTA_GAMERULES_STATE_HERO_SELECTION`, `StrategyTime` = `DOTA_GAMERULES_STATE_STRATEGY_TIME`; anything else (including TEAM_SHOWCASE, loading, pre-game) is no draft screen.

| Condition | Column |
| --- | --- |
| no admitted session, ownership not verified, spectating, replay, GSI stale > 10 s, not a draft screen | hidden |
| draft screen, own hero not yet known | suggestions list, or the consent chip (8 s once per app run) when consent is off |
| draft screen, own hero known | pick card (hard swap in the same update; the list is removed) |
| in-game bar switch off | overlay hidden; the deck still shows the view |

A repick or swap removes the previous hero's values in the same update (UI-010 failure behaviour).

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
- Transport: HTTPS to the single host `api.opendota.com:443`, redirects disabled, cookies disabled, generic `User-Agent: Kohbor`, 4 s connect / 10 s total, body cap 4 MiB. Tests use a mock transport; no test touches the network.

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

## 8. Ranking (DECIDED before data)

- Rate = wins / picks in the bracket column (`pub_*` for ทุกแรงก์). Ranked only with picks ≥ 1,000; sort rate desc, then picks desc, then hero ID asc. Shown as tenths of a percent (half up), computed from integers.
- Rows: walk the sorted group; emit until 8 untaken ranked rows; v2 taken heroes met on the way are emitted struck out, without a rank. If fewer than 8 heroes are ranked, append low-data heroes (picks desc) until 8 rows; their rate is withheld.
- Bracket column summing to 0 → `bracket_empty` (MEASURED: Immortal was 0 for every hero on 2026-11-21).
- Pick card: rate with ≥ 1,000 picks else `not_enough_matches`; form = rate − 50.0; duration cells < 30 min, 30–45, > 45 (bins merged by `duration_bin` seconds), each shown only with ≥ 300 games; the length sentence only when no cell is withheld and max − min ≥ 3.0.

## 9. v2 strike-out (DRAFT-BAR-SPEC 1.2.0)

Only heroes that Draft Bar 5b admits as identified in the ten slots are struck out. The view carries `draft_bar.available` and `identified/10`. 5b unavailable → v1 rows and the note "Kohbor ยังไม่เห็นว่าตัวไหนถูกแบนหรือถูกเลือกแล้ว". Bans are never known. 5b identities go only into the in-memory view model; never to disk, logs or the network. Player names never enter this view. No matchup edge.

## 10. View contract `draft-column-view` 1 (host → windows)

Event `draft://view` to the `draft` window; command `draft_column_view` for the control window. A field typed `int` is a required non-negative integer; a field typed `null | int` is nullable, and `null` means unknown, never zero. `suggestions.rows` holds at most 16 rows; a payload that violates any type, enum or bound is invalid as a whole. The overlay draws at most 8 rows (the deck draws all rows). Lead decision 2026-11-21: the 8 overlay rows go first to the top still-pickable (untaken) rows in host order; a struck-out (taken) row is drawn only if it ranks above a drawn untaken row and there is room left (that is, fewer than 8 untaken rows exist); otherwise struck-out rows are left out of the overlay. Host order is kept for the rows that are drawn. Text floor in the overlay: no text below 8.5 px at scale 1 (hero names 11 px, rates 11 px, games 8.5 px), from the design's measured minimum; readability in play is still unmeasured.

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
              win_rate_tenths: null | int, low_data: bool, taken: bool } ],
    ranked_count: int,
    draft_bar: { available: bool, identified: int, slots: 10 }
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

Win-chance %, predicted outcome, ban suggestions, matchup edge, item/lane/skill advice, player names, ranks, hero or item art (names are text from the provider's hero table).

## 12. Evidence and open points

Synthetic only from this build. UNVERIFIED: overlay behaviour over Dota 2 (click-through, never activated, placement, readability of 8.5–12.5 px text), the strategy-time card position, the durations response shape, the provider's data window, per-IP quota and licence, Turbo/Captains Mode screens.
