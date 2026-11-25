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

The bundled executable is registered in `bundle.externalBin` as `binaries/replay-engine-rs`; the source filename carries the required `x86_64-pc-windows-msvc` target suffix. The base Tauri config can retain this setting while `bundle.active` is false: the host build config accepts and copies the sidecar, while installer generation stays disabled. At runtime the host checks the fixed application resource and executable-adjacent bundle paths first. Lab/dev builds may fall back to `KOHBOR_REPLAY_ENGINE` only if no bundled path exists. Before every spawn the selected binary must match engine 0.3.0, source commit `5db800267e3406f6322a5fb0f52285b9b0c7c4dc`, size 3,258,368 bytes and SHA-256 `9d7aa88bbd8d1b31cd35489dcbecd88058c6ba68a118f88042e2cc262ff5d05d`. The pre-pin RP1 development hash `5955ddc1b2c8bc5881876ca29e6fb73d1be7c449a6715fe76ede849f5828b46a` is not allowed and fails closed. See [REPLAY-ENGINE-PROVENANCE.md](REPLAY-ENGINE-PROVENANCE.md) for the build and license inventory.

The host validates required JSON keys and value types without a new schema crate, accepts bundle schema `3.0` and `3.1`, requires the manifest and match schema values to agree, compares match IDs and engine versions across manifest/match, checks the `.dem` hash, and verifies each bundle file's recorded byte length and SHA-256. New parses produce schema `3.1`; existing schema `3.0` library bundles remain readable. Schema 3.1 adds optional `meta.winner` / `winner_reason` and `meta.draft` / `draft_reason`, plus `heroes[].gold_earned`, `gold_earned_final` and `dmg_out` (`total`, `p`, `m`, `u`). When present, these keys are type-checked; nullable winner/draft and gold values are retained, with reasons required for unknown winner/draft. Any unsupported version, missing required legacy key, oversized or malformed output rejects the full bundle.

## Privacy and failure handling

The raw match ID is retained in the local replay library's index, per-match directory and bundle. It is not written to application logs. The renderer receives library entries through the replay commands; the ID is not added to GSI/session history. Engine stdout may include paths and match metadata, so the host consumes only the success flag or a bounded error code, then discards stdout. Stderr contents are never logged; the log receives only `ENGINE_STDERR_CAPTURED`.

Replay processing has no network path. The host makes no replay network request; the inspected engine source uses `source2-demo` without default download features and contains no network call. This is source inspection, not a network-isolation acceptance test. Kohbor does not write Dota configuration, access game memory, inject input or automate gameplay. Reading Dota's replay folder is not game automation.

An unknown patch maps to `needs_pack`; RP1 does not fetch a pack or send replay data anywhere. Invalid input, unavailable or incorrectly pinned engine, spawn failure, timeout, output over-limit, invalid CLI response and malformed bundle map to stable failure codes. If a match becomes active after launch, the host kills the child and rejects that job. No failed or partial output is installed.

## Evidence boundary

The contracts describe engine version 0.3.0 and Match Bundle schema 3.1 while preserving admission for schema 3.0. Rust host unit tests and Node tests are source/synthetic checks. The prior RP4/RP3 run's 8.852-second parse was performed with the 0.2.0 binary and is historical evidence for that exact binary/replay only. On 2026-11-25, the opt-in native integration test parsed one saved replay from the owner's local `F:\gsi-log\demos` folder read-only with the pinned 0.3.0 binary: 16,578 ms, `result=ready`; no replay was copied into the repository, and the emitted receipt contains no match ID or player name. This native parser result is bound to the executable hash in [REPLAY-ENGINE-PROVENANCE.md](REPLAY-ENGINE-PROVENANCE.md) and the tested source snapshot reported with this task; it does not qualify an installed app, release, live match, viewer or product acceptance.

## Viewer integration (RP2/RP4)

The Lab Match view calls `replay_library_list`, listens for state changes, opens ready entries with `replay_library_open`, and sends parse/remove/clear actions through `replayLibraryClient`. The Match view no longer accepts bundle files directly. `clear` asks for confirmation. The renderer does not display source paths or match IDs. List rows display the host's match date, own hero if explicitly identified, duration if known and current status chip. The watched-folder line shows the host path or an explicit unknown state when Dota is not found. The native chooser button asks the host to open the Windows file dialog; drag and drop remains available.

The manual `.dem` control validates its extension in the renderer; the host validates the real file and size before queueing. A Tauri drop event also supplies filesystem paths. The file dialog is a Windows host command and no Tauri plugin or new crate is used.

Map calibration comes from `manifest.map.calibration` when valid; the matching patch pack is the fallback. Unsupported patches show `needs_pack`, and the viewer does not use another patch's values. Bundled data under `apps/desktop/src/assets/replay-packs/` covers 7.41f and 7.42 and totals 273,416 bytes including provenance, below the 2 MiB ceiling. It was generated from the read-only engine pack at source commit `a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9`; `provenance.json` records dotaconstants source versions and per-input/per-output SHA-256 values. The upstream MIT note is recorded for review, not an independent rights determination.

Hero names, attack-speed tables, formulas, curated buffs and map calibration remain patch-scoped. Ability slot order and explicit ultimate markers are read from the locally exported build 6989 `stats.json` (128 hero records, 127 with named ability slots, 1,654 ability-slot rows and 134 explicit ultimate rows in this export). CC ability names are the dotaconstants-derived CC candidates intersected with those installed hero slots, so stale candidates absent from build 6989 are dropped. `AbilityBehavior` flags are preserved as source data but do not independently prove that an ability applies control; the available hero scripts exported no ability modifier rows. The UI labels this source as installed build 6989 and says patch compatibility is unverified; `stats.json` records its patch as `unverified`. The 7.42 pack's map calibration is marked by its source as unvalidated against a 7.42 replay. Thai and English viewer copy is supplied through `matchViewerCopy` for static markup, runtime DOM text and canvas labels.

The existing RP1 validation remains the authority for on-disk bundle hashes and structural checks. The renderer performs a structural admission before mounting the view. Node tests cover library state/actions, metadata view-model behavior, patch lookup, calibration choice, installed-build ultimate data, CC candidates against dotaconstants fixtures, and pack provenance. These source/synthetic checks do not establish a native dialog interaction, native WebView2 behavior or owner acceptance.

Contract 1.1.0 matches the additive engine output: world-track `hp` and `n` remain optional, `p` is exactly three numeric arrays, rune `by` remains a required nullable string, and schema 3.1 adds optional winner, draft, gold-earned and damage fields. The manifest and match schema values accept `3.0` or `3.1` and must agree; old schema 3.0 library bundles continue opening. The host's handwritten structural validator checks the new fields when present. Analytics knowledge-digest use of these values is phase 2, after the analytics owner merges that work.
