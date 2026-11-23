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

## Match view RP2 (DEC-REPLAY-INTEGRATION)

RP2 supersedes the R1 single-bundle preview. The Lab tab now lists entries from the local replay library and uses `replayLibraryClient` for open, parse, remove, clear and state-change refresh. Opening a ready entry loads the bundle from `replay_library_open`; there is no three-file picker in the Match view. Clear asks for confirmation. The viewer keeps the existing shadow-root lifecycle and local-only behavior.

Each row shows its host state (`available`, `queued`, `parsing`, `ready`, `failed(reason)` or `needs_pack`), source and library-added date. Match date, hero names and duration appear as unknown until the bundle is opened because RP1's list command does not return those fields. The watched folder is shown as unknown because RP1 does not expose its path. The UI does not print raw match IDs or replay source paths.

The “Add .dem file” control validates the extension and reported size before calling `replay_library_parse`. Dropped paths are accepted through the Tauri webview event. The file input may not expose a filesystem path; when it does not, the view reports that limitation. A native file dialog/path-return command or byte intake requires a host scope update. The RP1 host remains responsible for validating the real file size and matching state.

**Patch data and map.** A valid `manifest.map.calibration` is preferred; otherwise calibration falls back to the matching patch pack. Unsupported patches show `needs_pack` and do not use another patch's values. Patch data for 7.41f and 7.42 totals 273,416 bytes with provenance. It was derived from `F:\gsi-log\replay-engine-rs\pack\<patch>` at engine source commit `a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9`; each provenance file records dotaconstants versions, source and generated SHA-256 values, and an upstream MIT license note. That note is not an independent rights review. Control and mobility lookups are keyword heuristics over ability text, not parser-verified status. The supplied source has no hero-to-ability slot mapping or ultimate marker, so no verified ultimate lists are available; the view disables ultimate classification and continues to recognize BKB item events. The 7.42 map calibration is not validated against a 7.42 replay.

Thai and English labels, runtime messages and help text are localized through `matchViewerCopy`, including canvas labels. Match view tests run with `node --test --test-isolation=none` when Node's worker process cannot spawn. These source and synthetic checks do not establish native WebView2 rendering or owner acceptance.
