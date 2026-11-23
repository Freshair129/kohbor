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

## Death review v3 (DEC-COACH-REVIEW)

The owner approved coach analytics for the Lab preview on 2026-11-23 (DEC-COACH-REVIEW). Slice 1 adds a `kohbor.post-match-replay.v3` report and a Death review panel above the map. v2 and v1 files still open without the panel.

**Report.** v3 keeps every v2 field and adds a top-level `rules` block and a `coach` block per match. `rules` names the version and values of every threshold and must say `provisional: true` and `enemyPositions: "full-map"`. `coach` holds the following, all derived by the exporter:

- one analysis per own death, with the same ticks as `deaths`:
  - type: `solo`, `gank`, `teamfight` or `other`
  - enemies involved and allies nearby
  - position sample age
  - seconds the enemy team could see the hero in the last 10 seconds
  - allied and enemy observer-ward cover
  - seconds since an enemy Smoke
- the lane outcome at 10:00 per own lane: summed net-worth difference against the lane opponents, deaths before 10:00 and a provisional ahead/even/behind label, with roaming heroes listed apart
- at most 3 template notes, each citing the death ticks it rests on

`report.ts` validates the closed v3 shape, including that note evidence points at real deaths and that lane heroes are own roster slots.

**Panel.** `CoachPanel.tsx` labels every value `DERIVED` or `OBSERVED`. It marks death types as built from full-map enemy positions, which the player could not see during play. Below 10 matches it shows k/n and no rates. Teammates appear only as hero and roster slot.

**Exporter.** `tools/replay_export/` is a Lab/dev tool:

- `pin.py` refuses to run unless the installed gem-dota matches `PIN.json`: version 0.14.0, the RECORD SHA-256 and every file hash. The copy is the one already on the owner's machine; nothing is downloaded.
- `extract.py` builds an allow-listed neutral model from a parsed DEM and never reads player names, Steam IDs, account IDs or chat.
- `analyze.py` applies the provisional rules and is standard-library only.
- `privacy.py` scans every output against the parsed DEM's names and IDs. `export.py` refuses to write a file that fails the scan.
- The user supplies the DEM files and the map image through a local layout file. The exporter has no network path.

```powershell
<gem-dota venv>\Scripts\python.exe tools\replay_export\export.py --hero earthshaker --patch 7.41f --layout layout-7.41f.json --dem C:\path\match.dem --out report.json
```

**Tests.** `tools/tests/test_replay_export.py` covers the rules, the privacy scan and the exporter on a DEM-derived neutral fixture. The fixture is trimmed and has its real match ID replaced. `apps/desktop/tests/post-match-replay-v3.test.ts` covers the v3 reader and the summary. The committed viewer fixture is rebuilt and compared by the Python test. These are synthetic and recorded-offline evidence. They are not native WebView2 or live evidence. The owner confirms the provisional thresholds after 3–5 matches.

## Match view R1 (DEC-REPLAY-INTEGRATION)

Slice R1 hosts the owner's replay-viewer prototype in the same Lab tab. A switch at the top of the tab selects "DEM report" (the viewer above) or "Match view (prototype)". Switching unmounts the other view and discards its data. The Lab gate, the post-match admission and the memory-only rule above apply unchanged. There is no native command, network path or persistence.

**Input.** The user multi-selects `manifest.json`, `match.json` and `world.json` of one Match Bundle (replay engine schema 3.0) produced on their machine. `matchBundle.ts` admits it:

- all three files are required, with one copy of each
- size limits: manifest 1 MiB, match 48 MiB and world 24 MiB, checked before the files are read
- `schema` must be `3.0` in both the manifest and `match.meta`
- byte counts and SHA-256 must match the manifest
- the fields the viewer reads must have the expected shape: per-second series of `t1 - t0 + 1` samples, hero ids and `#rrggbb` colours, and deaths that point at known heroes

The panel names the file and the failing field. Patch, game mode and length come from `match.meta`; when absent the panel shows them as unknown.

**Icons.** No game image is committed. An optional second pick of a folder, such as the patch pack folder with `map.png` and `icons/{heroes,abilities,items}`, supplies the minimap and icons. Images picked with the bundle are accepted as well. Only the names the bundle needs are read, as data URLs, because the app CSP allows `data:` but not `blob:` images. Each file is limited to 2 MiB (8 MiB for the map) and the total to 64 MiB. The viewer resolves every image through one `icon(kind, name)` function. It looks up the exact name and otherwise returns a neutral tile with that name, so a hero never gets another hero's icon. Without a map image the map draws on a dark ground.

**Viewer.** `matchViewer.mjs` is generated from `F:\gsi-log\replay-viewer-proto\viewer.js` with mechanical changes only:

- data comes from the admitted bundle
- images go through `icon()`
- lookups resolve inside the shadow root
- the window and document listeners, animation frames, timers and resize observers stop on unmount
- inline `onerror` handlers are removed (the CSP blocks them)
- a hero missing from `statdb.heroes` shows no derived stats

`matchViewer.css` maps the prototype variables to Command Deck tokens on `:host` and loads no web fonts. The layout is the prototype's: five cards left, score, clock, momentum graph and map in the centre, and five cards right. The static markup has English text in `matchViewerCopy.mjs`. Labels that the prototype script writes at run time remain Thai in R1.

**Prototype tables.** `matchTables.mjs` holds the single-match tables: the map projection, the ultimate, CC and mobility patterns, the "Mid pair" button and the opening death. They are labelled "R1: prototype tables, single match 9093797968; replaced in R4". The view always shows a prototype note. It adds a warning when the bundle's own map calibration differs from the table.

**Evidence.** `apps/desktop/tests/match-bundle.test.ts` covers admission, icon resolution and a source check for network calls. It uses a synthetic bundle (`node --test`). A local browser harness opened the real bundles under the app CSP. This is not native WebView2 evidence and not an owner check.
