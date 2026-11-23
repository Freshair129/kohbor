# Replay Pipeline

Status: RP1 host and contract implementation. The renderer view is a later slice (RP2); release packaging and a source-matched engine pin are RP3.

## Flow

```text
Dota `game/dota/replays` --read-only poll every 30 s--┐
                                                     ├─> validate path/size -> available
Renderer file picker (RP2) -> typed client -> .dem --┘
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

The Dota replay folder is located from the existing game-install discovery and read without modifying it. Watching discovers numeric `<match_id>.dem` files; it never moves, copies, writes or deletes a Dota file. Manual intake accepts a user-selected `.dem` path through `replay_library_parse`. The RP1 renderer client is an API only; the picker UI and viewer integration belong to RP2.

## States and commands

Watched matches become `available`. A user parse request changes the item/job to `queued`, then `parsing` only after the existing Draft/Pregame/Live match barrier clears. One worker serializes engine runs. A validated bundle becomes `ready`; an unknown patch becomes `needs_pack`; other failures become `failed` with a stable reason code. State changes emit `replay-library://state-changed` with `{ "changed": true }`.

The host exposes `replay_library_list`, `replay_library_open`, `replay_library_parse`, `replay_library_remove` and `replay_library_clear` only through the Lab/demo command handler. `open` returns the validated manifest, match and world JSON documents. Remove and clear operate inside Kohbor's replay library. Clear cancels queued jobs and refuses while a parse is running.

## Storage, limits and process controls

The library is `<app data dir>/replays/index.json` plus one folder per canonical raw match ID. Each match folder contains the three engine bundle files and `meta.json` (source path, `.dem` SHA-256, engine version and SHA-256, parse time and status). JSON/index and metadata writes use the host atomic-write helper. Engine output is written into a fresh `.staging` directory, structurally checked, and renamed into place only after the whole bundle passes.

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

The executable lookup prefers `app_resource_dir/binaries/replay-engine-rs.exe`; only Lab/dev builds may fall back to `KOHBOR_REPLAY_ENGINE`. The host hashes the selected executable against the contract allowlist before spawning it, passes arguments directly as an array, and never invokes a shell. The current 0.2.0 hash is `5955ddc1b2c8bc5881876ca29e6fb73d1be7c449a6715fe76ede849f5828b46a`. It is a development binary built before engine source HEAD `a609ecc`; this pin does not qualify a release. RP3 must replace it with a source-matched, reviewed release pin.

The host validates required JSON keys and value types without a new schema crate, checks schema version 3.0, compares match IDs and engine versions across manifest/match, checks the `.dem` hash, and verifies each bundle file's recorded byte length and SHA-256. Any missing, extra, oversized or malformed output rejects the full bundle.

## Privacy and failure handling

The raw match ID is retained in the local replay library's index, per-match directory and bundle. It is not written to application logs. The renderer receives library entries through the replay commands; the ID is not added to GSI/session history. Engine stdout may include paths and match metadata, so the host consumes only the success flag or a bounded error code, then discards stdout. Stderr contents are never logged; the log receives only `ENGINE_STDERR_CAPTURED`.

Replay processing has no network path. The host makes no replay network request; the inspected engine source uses `source2-demo` without default download features and contains no network call. This is source inspection, not a network-isolation acceptance test. Kohbor does not write Dota configuration, access game memory, inject input or automate gameplay. Reading Dota's replay folder is not game automation.

An unknown patch maps to `needs_pack`; RP1 does not fetch a pack or send replay data anywhere. Invalid input, unavailable or incorrectly pinned engine, spawn failure, timeout, output over-limit, invalid CLI response and malformed bundle map to stable failure codes. If a match becomes active after launch, the host kills the child and rejects that job. No failed or partial output is installed.

## Evidence boundary

The schemas describe engine version 0.2.0 and Match Bundle schema 3.0 based on inspected source and local example bundles. Rust host unit tests and the typed client test are synthetic/source checks. One saved `.dem` parsed with the pinned dev executable and its output passed structural/hash validation in 8.481 seconds. This is native integration evidence for that exact dev binary and replay only; it is not release, live-match, viewer or product acceptance.
