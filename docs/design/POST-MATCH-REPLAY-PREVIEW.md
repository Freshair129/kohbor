# Post-match replay preview

This Lab preview adds a separate desktop Replay destination. It imports a local, normalized report derived from completed Dota 2 DEM files. It supports heatmaps, arbitrary match subsets within a patch, timeline playback at 1×, 2×, 5× and 10×, individual and team track filters, and a 15-second lead-in to the selected player's deaths. Matches use their own game clocks and align at 0:00. A shorter match disappears after its final clock.

The patch selector loads a layout and scopes the match picker, heatmap, deaths, and paths to that patch. The viewer initially opens the newest patch with matches. Switching patches stops active playback and refreshes the map projection, roster, death list, and timeline together. A map-only patch can be previewed before its first replay is imported.

Selecting a death shows the credited attacker, ability or attack, and selected Combat Log events from the same 15-second lead-in. The active log row follows the replay clock. Commentary and coaching suggestions appear with the evidence when supplied by the local report. Switching clips clears the previous selection and its advice.

## Runtime boundary

The existing development and Lab build condition controls the preview tab. A known closed Dota process or corroborating post-game runtime and Steam status allows the import. Dota reports the post-game phase for only a few seconds before it returns to the menu with an unknown phase. The app therefore remembers a corroborated post-game observation and keeps the import available in the Dota menu until it observes the next draft, pregame or live phase or a pending launch. This memory is not persisted. Draft, pregame, live play, launch pending and lost host status block the import. Leaving the tab or losing admission unmounts the viewer, cancels playback and discards the in-memory report. The import has no native command, network path, live projection, account dependency or persistence.

DATA-005 and AC-DATA-005 continue to govern the live display. This preview neither modifies that observation path nor supplies its acceptance evidence. A reviewed post-match data contract and native WebView2 admission and behavior verification are prerequisites for promotion beyond Lab. Browser results and synthetic source tests do not record a live or native acceptance pass.

## Local file format

The preview accepts a UTF-8 JSON file up to 16 MiB. The top-level keys are `format`, `source`, `matches`, and `maps`. Set `format` to `kohbor.post-match-replay.v2` and `source` to `DEM`. Supply 1–12 unique matches and 1–12 patch layouts. The renderer validates the closed structure in `report.ts`. Provenance is established by the offline exporter.

- A match contains string `id`, gameplay `patch` such as `7.41f` or `7.42`, display `hero`, `team`, the observed start and duration in game seconds, the selected player's `points` and `deaths`, and a roster with one `self`, four `ally-N`, and five `enemy-N` entries. Team names are `Radiant` and `Dire`. The `start` field can be negative when pre-horn positions are available. Death clips use that signed game clock.
- Each point is `[gameSecond, x, y]`. Points use one ordered sample per living second. Coordinates share the normalized map coordinate system. A death contains `t`, `x`, `y`, `tick`, and a nullable `killer` display label. A life end contains only `t`, `x`, and `y`. Roster entries contain `key`, `hero`, `team`, and `lifeEnds`. Entries other than `self` also contain `points`.
- A death may contain a `review` with a `finish` action, up to eight time-ordered `moments` in the clip, and nullable `summary` and `advice` strings localized as `{ th, en }`. Each action contains `actor`, nullable `ability`, and `illusion`. Moments add game time `t` and `kind` (`cast`, `control`, or `damage`). The exporter joins the death record to its Combat Log entry at the exact tick and distinguishes player suggestions from logged evidence. Missing reviews retain coordinate playback.
- Each layout contains `patch`, canvas `width` and `height`, `calibration`, and a PNG, JPEG, or WebP `map` data URL. Calibration uses `u = ox + x * sx` and `v = oy + y * sy` in the layout's canvas space. Each match must resolve to exactly one layout by patch.
- Existing v1 files open on their original 1000 × 959 layout and display an unknown patch. To assign a patch, re-export with verified metadata in v2.

The desktop preview consumes the normalized report. DEM decoding stays in the offline export process. For a future patch, corroborate the replay's server build and match date against the gameplay release, capture the matching map and projection, then append a layout in the versioned report. A matching replay server build is stronger evidence than a third-party calendar that groups or has yet to index letter patches. Preserve the old patch layout for historical matches. Map art and replay data stay in the local import file.
