# Minimap rune and Roshan overlay note

Date: 2026-11-25  
Scope: UI-018 implementation note for the owner brief RN.

## Rune marks

The pinned minimap uses the installed game's timer icons for bounty, wisdom, lotus, and generic power runes. Before the scheduled game-clock occurrence, the icon is dimmed and grayscale with a compact counter. The counter uses rounded-up whole minutes from 60 seconds (`2m`, at most two characters) and two-digit seconds below 60 (`59` through `01`). At the scheduled spawn and during the existing three-second observed spawn window, the icon is full colour with no counter. Rune names are available only in the tooltip and accessible label. Timers remain sourced from the ALERT-008 read model; this renderer adds no timing table.

An icon that is absent or fails to load uses the existing neutral dot mark. All four requested images were found in the installed VPK. Their per-file source paths, sizes, hashes, dimensions, and decoder record are in `apps/desktop/src/assets/dota/6993/runes/provenance.json`. The destination directory label is 6993, but the read-only installed game reports Steam build ID 26908863 and ClientVersion 6996; the provenance preserves that mismatch rather than attributing the files to build 6993. These are Valve game assets. Rights review remains required before public release.

Rune positions continue to come from three local replay bundles tagged 7.41f, projected through the replay manifest calibration. The bundles do not record a Dota build. When the timing rule patch differs from the position patch, the existing dashed position marker remains and the tooltip identifies both sources.

The existing replay-derived coordinate set contains bounty and power-rune positions only. The three bundles have no wisdom-rune or lotus position events, so those two icons are exported but omitted from the map until their positions have a source.

## Roshan pit switch

The Roshan rule is now derived from three local replay bundles parsed with engine 0.4.1 / schema 3.2. All three bundle patches are `7.41f`; no Dota build is recorded. The day/night series in `match.meta.daynight` follows the five-minute cycle (day starts at 0, then day/night alternate every 300 seconds), which lets each clustered `world.json` Roshan (`k: "rosh"`) transition be joined to its phase.

**Measured rule (patch 7.41f, n=8 transitions from 3 replays):** pit X, near `(-3192, 2392)`, is used by day; pit Y, near `(2864, -2768)`, is used by night. Pit X remains active through the early day/night changes. The first observed move is at 900.6–900.8 seconds, so the rule activates at 15:00; after activation, Roshan moves when the day/night phase changes. First-exit timing relative to the nearest phase edge has a median offset of +0.41 seconds and range -18.34 to +0.74 seconds. Eight first-exit-to-last-arrival durations have a median of 29.13 seconds and range 28.55 to 45.79 seconds. One 45.79-second path is an excursion outlier; it is retained in the evidence and median rather than discarded. The direction-to-phase mapping is consistent across all observed transitions: X→Y at night and Y→X by day.

**Method:** use `world.qu` (8 world units per position delta); cumulatively sum `p[0]`, `p[1]`, and `p[2]` from track `t0` (time deltas are half-seconds), then decode positions. For each complete track, find the first exit from the 100-world-unit circle around the source endpoint and the last arrival into the same-radius destination circle, linearly interpolating both crossings. Join the switch start to the nearest schema 3.2 `meta.daynight` edge, allowing up to 20 seconds for the observed excursion. This identifies the outlier start 18.34 seconds before the 2100.2-second night edge; the path reaches Y at 2127.65 seconds. No raw replay, match ID or player name is retained in this derivation. The constants and source description are in `apps/desktop/src/live/objectiveCountdownModel.ts`.

The live minimap uses fresh GSI `map.daytime` and `clock_time`, with a five-second freshness limit and a phase-consistency check. The countdown uses the same compact `2m` / two-digit-seconds style as the rune marks. After movement begins, the model shows `กำลังย้าย` and the remaining seconds from the measured 29.13-second median. It places the marker at the phase's destination endpoint while that movement countdown is active. Stale or contradictory GSI timing stays unknown. The tooltip says `วัดจากรีเพลย์ 7.41f · n=8`; an ALERT-008 schedule label mismatch is identified as such because GSI provides no live game-patch field. Replay patch/build compatibility remains limited to the recorded 7.41f replay evidence.

## Verification boundary

Pure Node tests cover the replay fixture derivation, switch boundaries, the measured walk countdown, patch-label mismatch and stale-daytime unknown state. These tests are synthetic source-level checks. No Windows native minimap capture or live-game observation is claimed by this note.
