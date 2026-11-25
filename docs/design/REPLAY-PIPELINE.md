# Replay Pipeline

Status: RP1 host/contract, RP2 Lab viewer, RP4 library metadata/native picker and installed ability data, and RP3 bundled engine pin are implemented. Tauri bundle generation remains disabled by the base configuration.

## Flow

```text
Dota `game/dota/replays` --read-only poll every 30 s--┐
                                                     ├─> validate path/size -> available
Native IFileOpenDialog (*.dem) -> typed client -> library intake --┘
                                                        |
                                                        v
                                         queued -> wait for match_active=false
                                                        |
                                                        v
                                     verify pinned engine SHA-256 -> spawn
                                                        |
                                                        v
                                 write only to library staging directory
                                                        |
                                                        v
                          validate all bundle files, sizes and cross-file hashes
                                  | valid                         | invalid
                                  v                               v
                       atomically install folder/index       reject whole bundle
                                  |
                                  v
                                ready
```

The Dota replay folder is located from the existing game-install discovery and read without modifying it. Watching discovers numeric `<match_id>.dem` files; it never moves, copies, writes or deletes a Dota file. Manual intake opens the Windows `IFileOpenDialog` with a `*.dem` filter and passes the selected path through `replay_library_parse`; extension, file type, header and size are checked by the host. The Lab list response includes match date, explicit own-hero metadata when the bundle identifies one, duration, patch and the watched folder path. Missing game installs and unknown own heroes stay explicit unknown values.

## States and commands

Watched matches become `available`. With the existing persisted `auto_analyse` setting (default on) labelled **“แยกรีเพลย์อัตโนมัติ” / “Parse replays automatically”** enabled, newly detected watched `.dem` entries are queued automatically, newest first. Enabling the setting queues the existing `available` watched entries once, newest first, capped by the configured retention count. Automatic requests and explicit parse requests use the same queue; one worker runs them serially at low priority only after the existing match barrier clears. Disabling the setting cancels queued automatic work and leaves a running parse to finish. Manual file picks still require an explicit parse request. User-removed watched entries stay suppressed while their source `.dem` remains in Dota's folder, and `failed` and `needs_pack` entries are never auto-retried. A bundle becomes `ready` only after validation; an unknown patch becomes `needs_pack`; other failures become `failed` with a stable reason code. State changes emit `replay-library://state-changed` with `{ "changed": true }`.

The host exposes `replay_library_list`, `replay_library_pick_file`, `replay_library_open`, `replay_library_parse`, `replay_library_remove` and `replay_library_clear` only through the Lab/demo command handler. `open` returns the validated manifest, match and world JSON documents. Remove and clear operate inside Kohbor's replay library. Clear cancels queued jobs and refuses while a parse is running. The native dialog runs on a dedicated COM STA thread and returns a path only; replay bytes stay on disk and the library copies from the selected path after validation.

## Storage, limits and process controls

The library is `<app data dir>/replays/index.json` plus one folder per canonical raw match ID. Each match folder contains the three engine bundle files and `meta.json` (source path, `.dem` SHA-256, engine version and SHA-256, parse time, status, match date, explicit own hero, duration and patch when available). The list backfills missing display metadata from an already stored bundle, supporting entries created before RP4. JSON/index and metadata writes use the host atomic-write helper. Engine output is written into a fresh `.staging` directory, structurally checked, and renamed into place only after the whole bundle passes.

| Boundary | Limit or behavior |
| --- | --- |
| Folder watch | Poll every 30 seconds; read-only |
| Input replay | Regular `.dem`, `PBDEMS2\0` header, at most 400 MiB |
| `manifest.json` | At most 1 MiB |
| `match.json` | At most 48 MiB |
| `world.json` | At most 24 MiB |
| Library retention | Default 20 matches or 2 GiB total; evict oldest first |
| Engine process | One parse at a time, 120-second timeout, below-normal Windows priority |
| Captured stdout and stderr | At most 1 MiB per pipe; output files are monitored against their individual caps |

The bundled executable is registered in `bundle.externalBin` as `binaries/replay-engine-rs`; the source filename carries the required `x86_64-pc-windows-msvc` target suffix. The base Tauri config can retain this setting while `bundle.active` is false: the host build config accepts and copies the sidecar, while installer generation stays disabled. At runtime the host checks the fixed application resource and executable-adjacent bundle paths first. Lab/dev builds may fall back to `KOHBOR_REPLAY_ENGINE` only if no bundled path exists. Before every spawn the selected binary must match engine 0.2.0, source commit `a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9`, SHA-256 `a05f91761a8bacd84ceec24522568e526e6a35a27525c3dfbfaf5e7f3192dcb6`. The previous RP1 dev hash `5955ddc1b2c8bc5881876ca29e6fb73d1be7c449a6715fe76ede849f5828b46a` is no longer allowed; it predates the pinned source and fails closed. See [REPLAY-ENGINE-PROVENANCE.md](REPLAY-ENGINE-PROVENANCE.md) for the build and license inventory.

The host validates required JSON keys and value types without a new schema crate, checks schema version 3.0, compares match IDs and engine versions across manifest/match, checks the `.dem` hash, and verifies each bundle file's recorded byte length and SHA-256. Any missing, extra, oversized or malformed output rejects the full bundle.

## Privacy and failure handling

The raw match ID is retained in the local replay library's index, per-match directory and bundle. It is not written to application logs. The renderer receives library entries through the replay commands; the ID is not added to GSI/session history. Engine stdout may include paths and match metadata, so the host consumes only the success flag or a bounded error code, then discards stdout. Stderr contents are never logged; the log receives only `ENGINE_STDERR_CAPTURED`.

Replay processing has no network path. The host makes no replay network request; the inspected engine source uses `source2-demo` without default download features and contains no network call. This is source inspection, not a network-isolation acceptance test. Kohbor does not write Dota configuration, access game memory, inject input or automate gameplay. Reading Dota's replay folder is not game automation.

An unknown patch maps to `needs_pack`; RP1 does not fetch a pack or send replay data anywhere. Invalid input, unavailable or incorrectly pinned engine, spawn failure, timeout, output over-limit, invalid CLI response and malformed bundle map to stable failure codes. If a match becomes active after launch, the host kills the child and rejects that job. No failed or partial output is installed.

## Evidence boundary

The contracts describe engine version 0.2.0 and Match Bundle schema 3.0 based on inspected source and local example bundles. Rust host unit tests, node tests and the typed client check are source/synthetic checks. In this RP4/RP3 run, the copied pinned executable parsed one saved `.dem` through the opt-in host integration test, and the resulting bundle passed structural and hash validation in 8.852 seconds. This is native integration evidence for that exact binary and replay only; it does not qualify an installed app, release, live match, viewer or product acceptance.

## Viewer integration (RP2/RP4)

The Lab Match view calls `replay_library_list`, listens for state changes, opens ready entries with `replay_library_open`, and sends parse/remove/clear actions through `replayLibraryClient`. The Match view no longer accepts bundle files directly. `clear` asks for confirmation. The renderer does not display source paths or match IDs. List rows display the host's match date, own hero if explicitly identified, duration if known and current status chip. The watched-folder line shows the host path or an explicit unknown state when Dota is not found. The native chooser button asks the host to open the Windows file dialog; drag and drop remains available.

The manual `.dem` control validates its extension in the renderer; the host validates the real file and size before queueing. A Tauri drop event also supplies filesystem paths. The file dialog is a Windows host command and no Tauri plugin or new crate is used.

Map calibration comes from `manifest.map.calibration` when valid; the matching patch pack is the fallback. Unsupported patches show `needs_pack`, and the viewer does not use another patch's values. Bundled data under `apps/desktop/src/assets/replay-packs/` covers 7.41f and 7.42 and totals 273,416 bytes including provenance, below the 2 MiB ceiling. It was generated from the read-only engine pack at source commit `a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9`; `provenance.json` records dotaconstants source versions and per-input/per-output SHA-256 values. The upstream MIT note is recorded for review, not an independent rights determination.

Hero names, attack-speed tables, formulas, curated buffs and map calibration remain patch-scoped. Ability slot order and explicit ultimate markers are read from the locally exported build 6989 `stats.json` (128 hero records, 127 with named ability slots, 1,654 ability-slot rows and 134 explicit ultimate rows in this export). CC ability names are the dotaconstants-derived CC candidates intersected with those installed hero slots, so stale candidates absent from build 6989 are dropped. `AbilityBehavior` flags are preserved as source data but do not independently prove that an ability applies control; the available hero scripts exported no ability modifier rows. The UI labels this source as installed build 6989 and says patch compatibility is unverified; `stats.json` records its patch as `unverified`. The 7.42 pack's map calibration is marked by its source as unvalidated against a 7.42 replay. Thai and English viewer copy is supplied through `matchViewerCopy` for static markup, runtime DOM text and canvas labels.

The existing RP1 validation remains the authority for on-disk bundle hashes and structural checks. The renderer performs a structural admission before mounting the view. Node tests cover library state/actions, metadata view-model behavior, patch lookup, calibration choice, installed-build ultimate data, CC candidates against dotaconstants fixtures, and pack provenance. These source/synthetic checks do not establish a native dialog interaction, native WebView2 behavior or owner acceptance.

Contract patch 1.0.1 now matches the pinned engine output: world-track `hp` and `n` are optional, `p` is exactly three numeric arrays, and rune `by` is a required nullable string. The wire bundle remains schema 3.0, and the host's handwritten structural validator enforces the same shape.
