---
title: "Game-State Intake Specification (M1)"
doc_id: "GSI-INTAKE-SPEC"
status: draft
version: "0.3.5"
updated: "2026-11-11"
authority: "DEC-BASELINE; elaborates CORE-004, CORE-005, DATA-001..004 under DEC-GSI-INTAKE"
owner_role: "Integration owner; people not assigned"
approval_status: "not_approved"
---

# Game-State Intake Specification (M1)

## 1. Status and authority

This document closes the M1-blocking gaps in game-state intake. It covers the owned game configuration, the loopback intake server, source admission, ownership verification, session boundaries, mode visibility and the freshness clock. It elaborates CORE-004, CORE-005 and DATA-001 to DATA-004. It does not change their statements, failure behavior or acceptance predicates.

The content is a **proposed freeze** under [DEC-GSI-INTAKE](../governance/DECISION-LOG.md). It falls within delegated builder authority: neutral wire names, routine implementation choices and local configuration. No reviewer has approved it. Two kinds of statement appear below:

- **Normative (MUST/SHALL):** this product's behavior. Valid whether or not an assumption holds.
- **Assumption (A-n):** an expected property of the game's integration feature. It MUST be confirmed by the M0 capture spike (§10) before M1 exit. If an assumption is false, the affected capability stays unavailable. Fixing the spec is a change-controlled correction, not an improvisation in code.

Technical identifiers that depend on DEC-NAME (for example the configuration file slug) are referenced symbolically. They remain null in [product-identity.json](../../contracts/product-identity.json) until DEC-NAME closes.

## 2. Owned game configuration (CORE-004)

### 2.1 Locating the game

1. Read the Steam installation path from the current-user registry value `HKCU\Software\Valve\Steam\SteamPath` (A-1).
2. Read `<SteamPath>/steamapps/libraryfolders.vdf`. For each library, look for `steamapps/appmanifest_570.acf`. The game directory is `<library>/steamapps/common/<installdir from the manifest>`.
3. If no library or more than one library holds the game, ask the user to choose the game folder. Validate that it contains `game/dota/cfg`.
4. MUST NOT scan whole drives, follow symlinks out of the chosen folder, or write anywhere except the owned file in §2.2.

Failure: `GSI_GAME_NOT_FOUND` with the action "choose the Dota 2 folder". Local core keeps running.

### 2.2 File identity and ownership

- Path: `<game>/game/dota/cfg/gamestate_integration/gamestate_integration_<gsi_config_slug>.cfg`. `gsi_config_slug` is a new technical-identity field: lowercase `[a-z0-9_]{3,32}`, frozen with DEC-NAME.
- Create the `gamestate_integration` directory if it is absent. Never modify, rename or delete any other file in that directory.
- Write to a temporary file in the same directory, flush, then atomically replace.
- After each write, store the SHA-256 of the written bytes in settings (`gsi.config_sha256`).
- On a later write: if the file exists and its hash differs from the stored hash, it was edited outside the app. Do not overwrite it. Report `GSI_CONFIG_CONFLICT` and offer an explicit "replace owned configuration" action.

### 2.3 File content

The file uses the game's key-value text format (A-2). Values are proposals; the spike confirms them.

```
"<display name> intake"
{
  "uri"        "http://127.0.0.1:<port>/"
  "timeout"    "5.0"
  "buffer"     "0.1"
  "throttle"   "0.1"
  "heartbeat"  "1.0"
  "data"
  {
    "provider" "1"
    "map"      "1"
    "player"   "1"
    "hero"     "1"
  }
  "auth"
  {
    "token" "<install token>"
  }
}
```

- **Data minimization:** M1 requests only `provider`, `map`, `player` and `hero`. The `abilities` block is added when the own-ability read model and PEER-010 contracts freeze (DEC-GAME-DATA, DEC-PEER-ABILITIES). Abilities, items, buildings and draft data are added only when their read-model contracts freeze under DEC-INTERFACES / DEC-GAME-DATA. Adding them is a configuration rewrite, which follows §2.4.
- **Heartbeat:** `1.0` s is chosen so that a quiescent game (pause, idle, dead) still renews freshness well inside the 2,000 ms boundary of DATA-001. The game is expected to send at least one POST per heartbeat interval even without state change (A-3).
- **Install token:** 32 bytes from the OS CSPRNG, base64url without padding (43 characters). Generated once per installation. The game requires the token in plain text in the file. The app copy is stored protected with Windows DPAPI in current-user scope (§7). Rotation: an explicit user action or a detected conflict replacement. Rotation increments the source generation (§5).

### 2.4 Port selection and persistence

- Candidate ports: 3000 through 3010 inclusive, on `127.0.0.1` only.
- **First setup:** read the `uri` of every other `gamestate_integration_*.cfg` in the folder and treat their loopback ports as **claimed**. Try the remaining candidates in order, bind the first free port, write the configuration with that port and persist it in settings (`gsi.port`). (Capture 2026-11-11: another application's file claimed port 3000 while nothing listened there. Binding a port that is merely free at that moment would collide with that application later.)
- **Later start:** bind the persisted port only. Do not silently move to another port. The game reads its configuration only at launch (A-4), so a running game would keep sending to the old port.
- **Persisted port occupied:**
  - If the game process is **not** running: select the next free candidate, rewrite the owned configuration (§2.2 rules) and persist it. Report the change in status.
  - If the game process **is** running: do not rewrite. Report `GSI_PORT_UNAVAILABLE` with the action "close the program using port N, or restart Dota 2 after reconfiguring". Intake is unavailable; the rest of local core continues.
  - Game-process detection uses the process list (image name `dota2.exe`) only. It is never a memory read or an injection.
- **All 11 candidates occupied:** `GSI_PORT_EXHAUSTED`. No configuration write.
- **Any configuration write failure** (permission, disk full, path vanished): `GSI_CONFIG_WRITE_FAILED`. Leave the previous file intact. This satisfies the CORE-004 failure behavior.

## 3. Loopback intake server

| Property | Rule |
| --- | --- |
| Bind | `127.0.0.1:<gsi.port>`; never `0.0.0.0` or IPv6 any |
| Method / path | `POST /` only; everything else gets 404/405 without parsing |
| Body limit | 131,072 bytes. Reject larger bodies with 413 before parsing (`GSI_BODY_REJECTED`) |
| Parsing | UTF-8 JSON, duplicate keys rejected, nesting depth ≤ 16; parse failure is `GSI_BODY_REJECTED` |
| Authentication | `auth.token` MUST equal the install token (constant-time comparison). Missing or mismatched → `GSI_AUTH_REJECTED`, not admitted |
| Rate | Requests above 50 per second are dropped after authentication. The admission stage consumes a latest-value slot, not an unbounded queue |
| Response | 200 with empty body once the request is authenticated and handed to admission; 4xx otherwise. The response never reveals token or admission detail |
| Logging | Codes and counts only. Never log raw bodies, tokens, player identifiers or match identifiers |

A rejected request never renews freshness, never changes session identity and never advances the source sequence.

## 4. Source admission (DATA-003, CORE-005)

Admission runs on each authenticated body. Field names are assumed GSI paths (A-5); the spike records the actual payloads.

| Check | Expected field (A-5) | Pass condition | Fail result |
| --- | --- | --- | --- |
| Provider | `provider.appid` | equals `570` | `invalid` (`SOURCE_INVALID`) |
| Activity | `player.activity` | equals `"playing"` | `unknown` (not playing; no own conclusions) |
| Not spectating | shape of `player` / `hero` | a single own-player object, not per-team collections (A-6) | `unsupported` (`GSI_UNSUPPORTED_SCOPE`) |
| Not custom game | `map.customgamename` | absent or empty | `unsupported` |
| Game phase | `map.game_state` | one of the supported states below | pregame/postgame states keep the session but expose no live own vitals |
| Ownership | see §4.1 | `verified` | `invalid` for own data (`OWNER_UNVERIFIED`) |
| Team | `player.team_name` | `radiant` or `dire` | team is null; own vitals still allowed |

Supported live states: `DOTA_GAMERULES_STATE_PRE_GAME`, `DOTA_GAMERULES_STATE_GAME_IN_PROGRESS`. Session-only states: `DOTA_GAMERULES_STATE_HERO_SELECTION`, `DOTA_GAMERULES_STATE_STRATEGY_TIME`, `DOTA_GAMERULES_STATE_TEAM_SHOWCASE`, `DOTA_GAMERULES_STATE_WAIT_FOR_PLAYERS_TO_LOAD`, `DOTA_GAMERULES_STATE_WAIT_FOR_MAP_TO_LOAD`, `DOTA_GAMERULES_STATE_POST_GAME`. The capture added TEAM_SHOWCASE and WAIT_FOR_MAP_TO_LOAD. The payload also carries `previously` and `added` diff blocks, which admission ignores: only the current top-level blocks count. Any other value is `unsupported`. It is never mapped to a nearby state.

### 4.1 Ownership verification

A payload is only **verified** as local-player data when all of the following hold:

1. **Authenticity.** It passed token authentication (§3). This shows it came from a game client configured by this installation.
2. **Platform identity cross-check.**
   - `player.steamid` is a 17-digit SteamID64 (A-5).
   - It equals `76561197960265728 + ActiveUser`, where `ActiveUser` is the 32-bit account ID read from `HKCU\Software\Valve\Steam\ActiveProcess\ActiveUser` (A-7).
   - The registry value is re-read when a session starts and when the identity changes.
3. **Own hero, not the selected unit.**
   - The `hero` block describes the controlled hero, not the currently selected unit (A-8).
   - Within a session, `hero.name` may change only at a session boundary. A change in the middle of a session makes ownership `mismatch`. An **empty** name during hero selection means "not picked yet" and is treated as missing, not as a change (capture 2026-11-11).
4. **Identity stability.** `player.steamid` is unchanged since session start.

Ownership states:

| State | Meaning |
| --- | --- |
| `verified` | All four conditions hold |
| `unverified` | The registry value is unavailable, zero or unreadable. This is never treated as verified |
| `mismatch` | A condition is contradicted |
| `unknown` | No admitted payload yet |

Only `verified` permits own-player conclusions or peer publication. `unverified` produces `OWNER_UNVERIFIED` with the action "sign in to Steam and restart Dota 2". The registry value is used only for comparison. It is never persisted, logged, displayed or sent.

### 4.2 Mode visibility (CORE-005)

The integration feature is expected not to expose the game mode or lobby type, for example All Pick versus Turbo, or ranked versus unranked (A-9).

- The app MUST NOT infer mode from timing, gold, score or any heuristic.
- Unless an admitted field explicitly identifies the mode, the read model reports `mode.state = "unverified"`. The UI shows "Game mode not verifiable from game data".
- Certification claims stay scoped to the NFR evidence matrix with that label.
- If the spike finds an explicit mode field, map only the documented values. All other values become `unsupported`.

Labeling every match "mode unverified" is a visible product consequence. The product owner must acknowledge it when DEC-GSI-INTAKE closes.

## 5. Generation, sequence and freshness clock (DATA-001)

- **`source.generation`.** A positive integer, persisted in local runtime state.
  - It increments whenever the intake binding changes: the server starts or restarts, the port changes, the token rotates, or the configuration is replaced.
  - Observations from an older generation are discarded.
- **`source.sequence`.** Assigned by the native admission stage. It starts at 1 for each generation and increments by 1 for each admitted payload. Rejected or non-admitted payloads do not consume a number.
- **Arrival time.** Captured from the OS monotonic clock (`QueryPerformanceCounter`) when the complete request body has been received, before parsing.
- **`source.age_ms`.** `floor((now_monotonic − arrival_monotonic_of_latest_admitted_payload) / 1 ms)`, evaluated whenever a snapshot is built.
- **Heartbeats renew freshness.** An authenticated, admitted heartbeat POST is a new source observation and resets the age. Payload equality with the previous body does not matter.
- **What cannot renew freshness:**
  - UI polling
  - snapshot rebuilds
  - rejected requests
  - payloads that fail admission
  - payloads from an older generation
- **Payload timestamp.** `provider.timestamp` and other payload clocks are diagnostics only. They are never used for age, ordering or session decisions.
- **Thresholds.** `fresh` when `age_ms ≤ 2000`; `stale` when `age_ms ≥ 2001` and the last admitted payload was valid. This is the semantic boundary of AC-DATA-001, now also encoded structurally in [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json).

## 6. Session boundaries (DATA-004)

DEC-SESSION continues to own *completion* and aggregate eligibility. This section defines only *start* and *continuity*.

| Situation | Evidence | Result |
| --- | --- | --- |
| New session | Admitted payload with non-empty, non-`"0"` `map.matchid` (A-5) that differs from the current session's match key | New opaque `session_id` (128-bit random), `session_epoch` + 1, event baselines re-initialized without replaying history |
| Owner change | Verified ownership with a different SteamID64 than the session owner | `session_epoch` + 1, all own values withdrawn, new `session_id` |
| Temporary invalidity, regressed clock, missing matchid, `game_state` regression | — | **Not** a new session. Keep identity and watermark; withdraw live values per DATA-002/DATA-003 |
| App restart during a match | First admitted payload whose match key and owner equal the persisted current session | Same `session_id` and same `session_epoch`. Event baselines re-initialize from the first new payload without emitting achievement or new-match cues |
| App restart, different or no persisted session | — | Treated as the "new session" row |
| Post-game | `map.game_state = DOTA_GAMERULES_STATE_POST_GAME` | Session stays current. Completion evidence is recorded only as a candidate for DEC-SESSION |

Persistence rules:

- **Match key.** HMAC-SHA-256 of `map.matchid`, keyed with a per-installation 32-byte secret (DPAPI-protected). Neither the raw match ID nor the SteamID64 is persisted.
- **`session_epoch`.** A persisted, strictly increasing counter. It is never reset and never reused after restart, migration or corruption recovery.
  - If the runtime-state file is corrupt, start a new session with an epoch greater than any value that can be recovered from the event archive.
  - If no value can be recovered, use the current wall-clock time in seconds. This is a one-time recovery jump, logged with a code.
- **Runtime-state file.** `<data_directory>/runtime-state.json` is written atomically. `data_directory` comes from DEC-NAME.
  - Content: `schema_version`, `source_generation`, `last_session_epoch`, `current_session` (`session_id`, `match_key`, `owner_key`, `epoch`).
  - `owner_key` is an HMAC of the SteamID64 under the same per-installation secret.

## 7. Local secrets

| Secret | Storage | Purpose |
| --- | --- | --- |
| Intake token | DPAPI current-user blob in the data directory; plain text in the owned game configuration (required by the game) | Authenticate local POSTs |
| Installation HMAC key | DPAPI current-user blob | Match key and owner key |

DPAPI failure at startup sets intake to `unavailable` (`GSI_SECRET_UNAVAILABLE`). It does not fail startup. Other modules MUST make their own secret-storage decisions; OBS and cloud keys are not covered here.

## 8. Error codes added to the catalog

`GSI_GAME_NOT_FOUND`, `GSI_CONFIG_CONFLICT`, `GSI_CONFIG_WRITE_FAILED`, `GSI_PORT_UNAVAILABLE`, `GSI_PORT_EXHAUSTED`, `GSI_AUTH_REJECTED`, `GSI_BODY_REJECTED`, `GSI_UNSUPPORTED_SCOPE` and `GSI_SECRET_UNAVAILABLE`, plus the existing `SOURCE_*` and `OWNER_UNVERIFIED` codes. Each code has a localization key and a user recovery action in [error-catalog.json](../../contracts/error-catalog.json).

## 9. Read-model outputs

- Own vitals go into [own-snapshot.schema.json](../../contracts/own-snapshot.schema.json) 2.0.0. Each field carries its own availability, so a missing HP value can be unknown while level stays observed.
- Intake, ownership, session and mode health go into [runtime-status.schema.json](../../contracts/runtime-status.schema.json).
- Port and configuration preferences go into [user-settings.schema.json](../../contracts/user-settings.schema.json).

## 10. M0 capture spike (closes the assumptions)

Run on a declared Windows/Dota/Steam configuration and record each payload with the token redacted. The spike data is evidence for DEC-GSI-INTAKE, not application acceptance.

| ID | Assumption | Spike check |
| --- | --- | --- |
| A-1 | `SteamPath` registry value locates Steam | Read value on a clean profile and a non-default library |
| A-2 | Key-value configuration format and the listed keys are honored | Game delivers POSTs with the configured URI, token and data blocks |
| A-3 | `heartbeat 1.0` produces ≤ 1,000 ms (+ jitter) inter-arrival with no state change | Measure inter-arrival during pause, death and idle; record max gap. If the max gap exceeds 2,000 ms, the stale behavior is truthful and must be reported to the owner, not hidden |
| A-4 | Configuration is read only at game launch | Change the port while the game runs; observe the old port still used |
| A-5 | Field paths `provider.appid`, `player.activity`, `player.steamid`, `player.team_name`, `map.matchid`, `map.game_state`, `map.customgamename`, `hero.*` exist as named | Record the field inventory across menu, demo/bot, live and post-game |
| A-6 | Spectator and replay payloads have a distinguishable shape | Record a spectated match and a replay |
| A-7 | `ActiveProcess\ActiveUser` holds the signed-in account ID | Compare with `player.steamid` on two accounts |
| A-8 | `hero` stays the controlled hero when another unit is selected | Select an allied unit, a courier and an enemy; confirm the `hero` block is unchanged |
| A-9 | No game-mode or lobby-type field exists | Record All Pick and Turbo payloads and diff them |

DEC-GSI-INTAKE closes when every assumption is confirmed or the spec is corrected, the fixtures in §9 are updated from the redacted corpus, and the product owner has acknowledged §4.2.

### 10.1 Results: capture of 2026-11-11

**Setup.** One owner machine: Windows build 26300, Dota 2 provider version 48, app build 26551391. The run lasted 3,042 s and captured 10,686 admitted payloads with 0 rejected requests, covering one bot match from menu to post-game plus one abandoned lobby. The redacted corpus stays local in `spike-out/`. Minimized fixtures derived from it are in `crates/kohbor-gsi/tests/captured/` and are replayed by `tests/captured_replay.rs`.

**This is spike evidence, not application acceptance.**

| ID | Result |
| --- | --- |
| A-1 | **Confirmed.** `SteamPath`, `libraryfolders.vdf` and `appmanifest_570.acf` located the game. Only one library was available (the default/non-default case was not tested) |
| A-2 | **Confirmed.** POSTs arrived at the configured URI with the token and the requested blocks |
| A-3 | **Confirmed.** Maximum gap 1,250 ms overall (at the menu). Maximum gap between identical payloads 1,085 ms. Maximum gap while paused 1,059 ms. Every gap is below 2,000 ms, so heartbeats keep a quiescent game fresh |
| A-4 | **Consistent, not isolated.** A first run with the game already open received 0 payloads; after a launch, payloads arrived. A port change was not tested in a running game |
| A-5 | **Confirmed, with additions.** All assumed paths exist. Additional diff blocks (`previously`, `added`) and fields (for example `hero.xpos/ypos`, `player.kill_list`, `map.win_team`, `map.paused`) were observed. Two extra session states were seen (above) |
| A-6 | **Not tested.** No spectated match or replay was captured |
| A-7 | **Confirmed for one account.** `player.steamid` equalled `76561197960265728 + ActiveUser` in every payload. Two accounts were not tested |
| A-8 | **Consistent, not explicitly tested.** `hero.name` stayed the same controlled hero through pre-game, live and post-game. Unit/courier selection was not recorded as a step |
| A-9 | **Partly.** No mode or lobby field appeared in `map` for this match. Only one mode was captured, so a Turbo diff is still needed |

**New finding:** another application's configuration claims port 3000 (§2.4 updated).

**Status.** DEC-GSI-INTAKE stays **proposed**. It still needs A-6 (spectate/replay), A-9 (a second mode), A-7 on a second account and A-8 with explicit selection, plus the owner acknowledgement of §4.2.
