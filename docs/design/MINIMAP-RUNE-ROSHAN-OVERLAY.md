# Minimap rune and Roshan overlay note

Date: 2026-11-25  
Scope: UI-018 implementation note for the owner brief RN.

## Rune marks

The pinned minimap uses the installed game's timer icons for bounty, wisdom, lotus, and generic power runes. Before the scheduled game-clock occurrence, the icon is dimmed and grayscale with a compact counter. The counter uses rounded-up whole minutes from 60 seconds (`2m`, at most two characters) and two-digit seconds below 60 (`59` through `01`). At the scheduled spawn and during the existing three-second observed spawn window, the icon is full colour with no counter. Rune names are available only in the tooltip and accessible label. Timers remain sourced from the ALERT-008 read model; this renderer adds no timing table.

An icon that is absent or fails to load uses the existing neutral dot mark. All four requested images were found in the installed VPK. Their per-file source paths, sizes, hashes, dimensions, and decoder record are in `apps/desktop/src/assets/dota/6993/runes/provenance.json`. The destination directory label is 6993, but the read-only installed game reports Steam build ID 26908863 and ClientVersion 6996; the provenance preserves that mismatch rather than attributing the files to build 6993. These are Valve game assets. Rights review remains required before public release.

Rune positions continue to come from three local replay bundles tagged 7.41f, projected through the replay manifest calibration. The bundles do not record a Dota build. When the timing rule patch differs from the position patch, the existing dashed position marker remains and the tooltip identifies both sources.

The existing replay-derived coordinate set contains bounty and power-rune positions only. The three bundles have no wisdom-rune or lotus position events, so those two icons are exported but omitted from the map until their positions have a source.

## Roshan pit switch

No current patch rule mapping day/night to Roshan's pit was found in the ALERT-008 rule source or the inspected statdb patch pack. GSI `map.daytime` and `clock_time` alone do not identify the current pit or next switch. The objectives panel therefore reports the switch as unknown unless a verified, explicit patch rule is supplied to the pure model. A synthetic fixture tests the boundary calculation only; it is not a game rule and is not wired into production. Replay tracks show two inter-pit endpoint clusters, but there is no verified current-patch mapping from those endpoints to the active pit, so no Roshan marker is placed on the map.

Replay correlation was checked across all three 7.41f bundles. `world.json` records `v`, `qh`, `qu`, `t0`, `heroes`, `tracks`, and `runes`; `match.json` metadata and the manifests contain no daytime/nighttime field or state. Decoding Roshan's quantized tracks and classifying samples within 100 world units of the two endpoint clusters yields eight complete alternating transitions: A→B at about 900.6, 900.7, 900.8, 1500.7, and 2081.9 seconds; B→A at about 1200.4, 1800.4, and 1800.5 seconds. Most transitions align with an approximately 300-second cadence, with one late/outlying start. This establishes alternating movement between the endpoints, but not which endpoint is day versus night: both assignments fit the recorded observations. Per the rule gate, no day/night pit constant or live switch countdown/marker is added; the production state remains unknown.

The replay engine README and `world.json` bundles do track Roshan (`k: "rosh"`) with half-second samples, quantized world positions, and RDP simplification. For eight complete inter-pit tracks across the three local 7.41f bundles, the measured durations were 29.00, 29.27, 29.00, 29.27, 45.87, 28.55, 29.18, and 29.08 seconds (median 29.13; range 28.55–45.87). The method cumulatively decodes `p` from `t0` (half-seconds for time, eight world units per position delta), identifies the two endpoint clusters near (-3192, 2392) and (2864, -2768), then linearly interpolates the first and last crossings of a 100-world-unit radius around those endpoints. One incomplete track was excluded. This is a historical replay measurement, not a current-patch movement rule or live remaining-time observation, so the UI does not present it as a countdown.

## Verification boundary

Pure Node tests cover the compact rune labels, icon appearance/fallback, and the Roshan day/night boundary and unknown-rule case. These tests are synthetic source-level checks. No Windows native minimap capture or live-game observation is claimed by this note.
