# Third-party data files

Data files copied into the repository from outside sources. Each entry records origin, hash, use, and what is NOT established. Rights are never decided by engineering.

## apps/desktop/src-tauri/data/item-prices.json

- Content: 294 entries, internal item name (no `item_` prefix) to shop cost in gold, from 1 to 7650.
- Use: derived net worth in the match HUD (`hud.rs`: gold plus the shop cost of every named item in the own items block). Compiled in with `include_str!`; no runtime fetch.
- Origin: copied byte for byte on 2026-11-15 from the owner's earlier project (G-Maiden) file `src-tauri/data/item-prices.json`. That project's own provenance record lists the upstream as an OpenDota "dotaconstants" snapshot; upstream revision and retrieval date were not recorded there either.
- SHA-256: `1d9fdd96788b91ed4cb1b8db140f8575540916aa654d15dbc859acd1b7259fd3`.
- Authorisation: the product owner authorised the copy in chat on 2026-11-15 ("ลอกมาเลยก็ได้ เพราะเกมเดียวกัน"). This is an owner statement, not a licence review.
- NOT established: the licence of the upstream data, the patch the prices belong to (prices change between patches; an item missing from the table makes the HUD net worth approximate and the chip shows a leading `~`), and any right to redistribute the file in a shipped product. Rights stay with the owner's rights process.
- Update rule: replace the file only with a new recorded snapshot (origin, date, hash) and keep the parse test (`price_table_parses_and_resolves_bare_and_prefixed_names`) passing.

## Overlay HUD visual style (apps/desktop/src/hud.css, HudStage.tsx, hudCopy.ts)

- Content: the look of the owner's earlier project (G-Maiden) overlay `src/src/overlay/FullOverlay.tsx`: colour tokens, the dark glass container (background, 1 px cyan edge, radius 16, shadows), the stat chip layout and sizes (16 px bold value over a 9 px uppercase muted label, padding 7x13), the four-segment risk meter (16x8 px, radius 2, gap 3, label to the right, a lit segment count of level + 1), the font stack and the reference scale (1920 px wide screen). Re-typed as CSS for our own data model (`hud://state`, availability, `~` approximate, stale dimming); no code, assets or brand text (no product name on screen) were copied verbatim beyond these values.
- Origin and date: read on 2026-11-15 from the owner's local copy of that project.
- Authorisation: the product owner ordered it in chat on 2026-11-15 (take the G-Maiden overlay component over directly; earlier: "เอาโค้ดเข้ามาใช้ก็ได้"). This is an owner statement about his own earlier project, not a licence review.
- NOT established: any licence or third-party rights in that project's design, fonts or assets beyond the owner's authorisation; rights stay with the owner's rights process. The HUD uses the system font stack ("Segoe UI", system-ui); no font file is bundled.

## apps/desktop/src/assets/editor/editor-reference-1080p.jpg

- Content: one 1920x1080 JPEG screenshot of a Dota 2 match at clock 0:52 (Tidehunter), taken by the product owner while running his earlier project G-Maiden, whose stat panel (Stat / Current / Goal) is visible at the top left. It contains Valve game art and UI (hero portraits, map, HUD) as seen on the owner's screen.
- Use: the default background of the Settings overlay layout editor (UI-015) only, so users can place the objective card against a real game screen. A local Lab reference file, when present, replaces it. It is not used anywhere else and is not shown in game.
- Origin: the owner's local Lab reference `<data dir>/lab/assets/dota-overlay-reference.jpg` (unredacted original, SHA-256 `0622e7c4c04c475c5ef42b778c43e66d0e32100d35557ec7b4e26629770f8894`, 388,875 bytes, dated 2026-11-15; it stays on the owner's machine and is not in the repository).
- Redaction (2026-11-22): the one player-identifying item, a chat-feed line with an account id and a player name at about y 748, was pixelated over x 555-1080, y 732-766 by the lead session; this session re-checked the whole image at full size and found no other player name or account id (the top bar shows role labels only, the top left shows stats only, the bottom shows the hero name only). The file carries no EXIF or XMP segment.
- SHA-256: `e989e489bd4a4519c9b2b8b346ecee290e6c1495248b4bf3729d4345f1f223ce` (333,443 bytes), copied byte for byte from the reviewed redacted file.
- Authorisation: owner decision 2026-11-22 relayed by the lead session after it explained the risk (Valve art distributed with the app; the owner's own rights call): “ยืนยันฝังเลยแล้วตรวจสอบภาพดีๆนะว่ามีชื่อของเพลเยอร์ทั้งของผมหรือของคนอื่นไหมถ้ามีก็เซ็นเซอร์เบลอให้หน่อย”. Recorded in DECISION-LOG. This is an owner statement, not a licence review.
- NOT established: any right to redistribute Valve game art or the G-Maiden panel in a shipped product. A rights-team review is required before any public release; the rest of the "no Valve art" rule still applies to every other asset.
- Update rule: replace only with another reviewed and redacted owner screenshot, recording origin, date, redaction and hash here; the test `the bundled editor screenshot is the reviewed redacted 1920x1080 JPEG without metadata` checks this hash.

## apps/desktop/src/assets/dota/6989/stats.json

- Content: compact hero base attributes and growth, primary attributes, armor, attack, movement, base health/mana and regeneration, magic resistance, normalized item attributes and derived-stat effects. Extended on 2026-11-24 with hero ability slots in order, explicit Ultimate classification when `AbilityType` is present, and `AbilityBehavior` flags and modifier names when present. Build 6989; patch remains unverified because the inspected game files contain no explicit 7.41f or 7.42 version string. This export contains 128 hero records, 127 with named ability slots, 1,654 slot rows and 134 rows with explicit Ultimate markers.
- Use: local input for own-hero stat estimates; values are labeled DERIVED and without temporary effects. The replay viewer also uses the installed hero ability slots and Ultimate markers, and intersects its patch-pack CC candidates against those slots. The JSON does not contain match telemetry or live state.
- Origin: re-exported on 2026-11-24 from the owner's installed Dota 2 at `G:\steam\steamapps\common\dota 2 beta`, read only and without download, by `crates/kohbor-game-assets` `kohbor-asset-export`. Provenance lists script paths, physical VPK archive paths, script hashes, VPK hashes, build number and exporter source commit. The 6989 export is 646,977 bytes; SHA-256 `dbb6f5de709f96de04e60b5a186df18be81a49545235a2d9fa095cfe0ec2cc04`.
- Authority: DEC-REPLAY-INTEGRATION item 3 permits local game-data extraction for this application; this records the owner decision, not a licence review.
- NOT established: any right to redistribute Valve-derived game data in a shipped product. Rights review is required before public release. Patch compatibility is unverified and rules must be selected explicitly by the caller.
- Update rule: extract from the installed game into a new build-number directory with the scoped stats-only exporter mode; do not overwrite a prior build's evidence or provenance. Behavior flags and modifier arrays preserve only fields found in the game scripts; missing fields stay unknown or empty.

## apps/desktop/src/assets/replay-packs/{7.41f,7.42}/

- Content: patch-scoped viewer data from the replay engine's embedded packs and the corresponding `dotaconstants` snapshots: map calibration, hero names, base attack speed, formulas, curated buffs, and heuristic ability/item lookups. The runtime Ultimate list comes from the installed build 6989 stats file; the runtime CC candidate list is the pack classification filtered to ability names present in those installed hero slots.
- Use: Lab Match view only. The 7.41f and 7.42 folders contain 273,416 bytes total including provenance, below the 2 MiB data budget. Unknown replay patches take the `needs_pack` path; no other patch's values are substituted.
- Origin: generated from `F:\gsi-log\replay-engine-rs\pack\<patch>` at engine source commit `a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9`. The 7.41f snapshot identifies `odota/dotaconstants @7fa7d26dc9`; 7.42 identifies `odota/dotaconstants @54d1b83415`. Each `provenance.json` records per-input and per-output size and SHA-256.
- Licence note: the upstream provenance identifies `dotaconstants` as MIT licensed. This is the upstream notice recorded by the source pack, not an independent rights determination. Public-release rights review remains required.
- Update rule: regenerate from the local engine pack only, keep source commit/version/hash provenance, rerun the pack size and CC/Ultimate tests, and do not add network fetching.

## apps/desktop/src/assets/dota/6989/ (Match view card images and minimap)

- Content: 189 hero images (128x72), 1,454 ability icons (64x64), 620 item icons (88x64) and one 1024x1024 minimap, all PNG, plus `provenance.json`. 2,264 images, 24,596,249 bytes. Valve game art.
- Use: the Lab Match view (Insights, Replay, "Match view"; `src/replay/bundledIcons.ts`) loads them as app assets instead of a user-picked folder. A missing name shows a neutral name tile, never another image. The Replay panel is only compiled into development and Lab builds (`VITE_KOHBOR_LAB=1`), so other builds do not emit these files.
- Origin: exported on 2026-11-23 from the product owner's installed Dota 2 (game build 6989, `game/dota/steam.inf` ClientVersion) by the dev tool `kohbor-asset-export` (`crates/kohbor-game-assets`, commit 09e363c), reading `game/dota/pak01_dir.vpk` read only: `panorama/images/heroes`, `panorama/images/spellicons`, `panorama/images/items` and `materials/overviews/dota_737_psd_28d44696.vtex_c`. Nothing was downloaded. `provenance.json` lists every file with its size and SHA-256 (provenance.json itself: SHA-256 `82f07e6c4d6e4585af3e8c4c66aa45249ecea5451832d2284750d030e213da89`, 331,453 bytes).
- Minimap choice: the 7.37 overview matches the frame and art of the 1024 px minimap the prototype calibration (`matchTables.mjs` `MAP`) was fitted on, within a few pixels by visual comparison; it lacks some later map features. The newer `dota_psd_8b4a9409` overview uses a different scale and would need its own calibration. Not measured numerically.
- Authorisation: DEC-REPLAY-INTEGRATION item 3 (product owner, 2026-11-23): hero, ability and item images may be bundled as Kohbor assets, built from the owner's installed game with no download; the installed-game reader stays as fallback. The owner asked on 2026-11-23 for the Match view to ship them now. This is an owner decision, not a licence review.
- NOT established: any right to redistribute Valve game art in a shipped product. **A rights review is required before any public release.** Some ability names a match needs have no icon under that name in the game files (for example `dark_seer_aggrandize`, `lycan_apex_predator`, seasonal consumables); they show as name tiles.
- Update rule: regenerate into a new `<build>/` folder with `kohbor-asset-export --out apps/desktop/src/assets/dota` (the tool refuses an existing build folder), record the new build here, and remove the old folder in the same change; the viewer uses the newest build present.
