# OpenDota hero benchmarks in Analytics compare

**Scope:** BM, DEC-POSITIONING-20261125 item 2 and the DEC-COACH-REVIEW item 3 amendment. This feature is a display aid for the selected replay hero. It does not change coaching thresholds or submit benchmark values to an advisor.

## Consent and requests

The separate `benchmarks_on` preference is off by default. Enabling it is explicit consent for the host to send the selected hero ID to OpenDota for `GET /api/benchmarks?hero_id=<id>`. The hero ID is the only variable part of that path. The renderer passes a local canonical hero key to the host; it is never sent to OpenDota.

The host first looks for the selected canonical key (`npc_dota_hero_*`) in the existing hero-stats cache, and uses that table only while its separate hero-stats consent is active. If the cache is absent or lacks the hero, the enabled benchmarks host may fetch `GET /api/heroes`, with no query or other parameters, and cache its public key-to-ID map with the fetch time. That request uses the same benchmarks budget. If neither source yields an ID, the view remains unknown and no `/api/benchmarks` request is made.

No account ID, match ID, player name, rank, bracket, local hero measurements, or other match data are sent. Both requests use the existing OpenDota WinHTTP transport. The consent text names both possible data items: the selected hero ID and the parameterless public hero list.

## Budget, cache, and withdrawal

The separate `hero-benchmarks-request-ledger.json` permits at most four requests in a rolling 24-hour window, shared by `/api/heroes` and `/api/benchmarks`. It follows DEC-PICK-STATS revision 4: an attempt counts after connect or send is attempted, whether it succeeds or fails; local setup failures and cancellation before send do not count. The ledger survives withdrawal so turning the switch off and on cannot reset the quota.

`hero-benchmarks-cache.json` stores the public hero map and benchmark curves by numeric hero ID, each with a fetched-at time. Turning the switch off persists `benchmarks_on: false`, cancels in-flight work, rejects late results, and deletes the complete benchmarks cache. A deletion error is returned to the UI. No match identifier or replay-derived value is stored in this cache.

## Compare view

The compare tab uses local bundle values for GPM, XPM, last hits per minute, hero damage per minute, and kills per minute. It interpolates each available OpenDota curve on the device and labels the result as an estimate. Missing replay values, benchmark curves, malformed fields, or an unknown hero ID remain unknown; none becomes zero.

OpenDota benchmarks have no bracket parameter. The view identifies the data as OpenDota benchmarks covering all ranks, displays the fetched date, and never labels a bracket. The compare panel retains the current layout and a 12 px text floor.

## Fixture and evidence boundary

Rust parser fixtures are synthetic and follow only the documented shape: `{ hero_id, result: { gold_per_min, xp_per_min, kills_per_min, last_hits_per_min, hero_damage_per_min, hero_healing_per_min, tower_damage } }`, with each metric represented as `{ percentile, value }` points. Partial fields remain unknown. Node tests use synthetic curves to verify local interpolation and disabled, loading, ready, offline, budget-exhausted, and unknown view states.

No live OpenDota request was made for this implementation. A live request and consent-flow check remain an owner check after the build; synthetic tests do not establish provider availability or response compatibility.
