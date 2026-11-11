---
title: "M0 Plan and Status"
doc_id: "M0-PLAN"
status: draft
version: "0.3.6"
updated: "2026-11-11"
authority: "DEC-BASELINE; SDP milestone M0; PROMPT.md first deliverable"
owner_role: "Engineering lead (builder)"
approval_status: "not_approved"
---

# M0 Plan and Status

M0 delivers six items: identity, scope, the interface decisions, a locked toolchain, a feasibility plan and a minimal skeleton ([SDP](SDP.md), [PROMPT.md](../../PROMPT.md)). This page records what exists, what was actually run and what is still open. Evidence classes are kept separate (AGENTS.md): **source** and **synthetic** tests ran, and **native, live, acoustic and resource evidence is NOT_RUN**.

## 1. Deliverable status

| # | Deliverable (PROMPT.md) | Status | Where |
| --- | --- | --- | --- |
| 1 | Technical identity and DEC-NAME closure | **Done**. Frozen 2026-11-11; the update origin stays null until the updater service is specified | [product-identity.json](../../contracts/product-identity.json), §2 |
| 2 | Scope matrix: first demo, full MVP, extensions, deferred | **Done** | §3 |
| 3 | Architecture and toolchain lock proposal | **Proposed**. Rust is pinned; Tauri/pnpm are pinned at the M1 scaffold | §4, [rust-toolchain.toml](../../rust-toolchain.toml) |
| 4 | Traceability: needs → requirements → components → modules → tests | **Started**. RTM `implementation_refs` now point at real code for CORE-004 and DATA-001..004 | §5, [RTM](../requirements/RTM.csv) |
| 5 | Decision log: delegated questions resolved, genuine blockers named | **Done** for M0. DEC-NAME is decided; DEC-GSI-INTAKE waits on the spike | [Decision Log](../governance/DECISION-LOG.md) |
| 6 | Runnable local-core slice with honest unknown states; synthetic fixtures kept out of production | **Done** (synthetic) | `crates/kohbor-gsi`, §6 |
| — | Feasibility: GSI capture spike | **Ran 2026-11-11** with the owner's approval: 10,686 payloads from one bot match. The spike config was then uninstalled. Results are in GSI-INTAKE-SPEC §10.1 | `crates/kohbor-gsi-capture`, §7 |

## 2. Technical identity (DEC-NAME)

| Field | Frozen value | Rationale |
| --- | --- | --- |
| `app_id` | `io.github.freshair129.kohbor` | Reverse-DNS form under the repository owner's GitHub namespace, so it implies no unowned domain (AGENTS.md) |
| `package_id` | `kohbor` | Crate/npm/package base name |
| `data_directory` | `%APPDATA%\io.github.freshair129.kohbor` | Tauri app-data convention for `app_id` |
| `gsi_config_slug` | `kohbor` | Gives `gamestate_integration_kohbor.cfg` (GSI-INTAKE-SPEC §2.2). The spike uses `kohbor_spike` |
| `uri_scheme` | `kohbor` | Reserved; registered only when a feature needs it |
| `media_signature_domain` | `kohbor-media-signature-v1` | Domain-separation label for DEC-PACK-CRYPTO |
| `peer_hash_domain` | `kohbor-peer-hash-v1` | Domain-separation label for the peer protocol |
| `update_manifest_origin` | null | Not inferred from the name; set when UPDATE-001's service is specified (M8) |

Changing `app_id` after the first installed build moves the user's data directory, so treat it as stable from now on.

## 3. Scope matrix (registry `target_milestone` × `scope`)

| Milestone | Scope | Count | Requirements |
| --- | --- | ---: | --- |
| M1 | MVP | 9 | CORE-001, CORE-002, CORE-004, CORE-005, DATA-001..004, QUALITY-002 |
| M2 | MVP | 11 | ALERT-001, ALERT-007, MEDIA-001..008, QUALITY-001 |
| M3 | MVP | 21 | DATA-005, DATA-006, VISION-001..004, ALERT-002..006, UI-001..006, UI-008, QUALITY-003..005 |
| M4b | MVP | 5 | OBS-001..005 |
| M5 | MVP | 15 | VOICE-001..007, MEM-001..005, ADVICE-001, QUALITY-006, QUALITY-007 |
| M6 | MVP | 4 | CORE-003, ADVICE-002..004 |
| M6 | Extension | 13 | PEER-001..010, SOCIAL-001..003 |
| M7 | Deferred | 4 | UI-007, PREMIUM-001..003 |
| M8 | MVP | 5 | UPDATE-001..004, QUALITY-008 |

The four groupings:

- **First demo (M4a):** M1–M3 requirements plus authorized critical media and one recorded live evidence match (SDP). M4a adds no requirements of its own.
- **Full MVP:** every `mvp` row, 70 requirements.
- **Extensions:** 13 requirements.
- **Deferred:** 4 requirements.

## 4. Architecture and toolchain lock (proposal)

| Item | Value | Basis |
| --- | --- | --- |
| Rust | **1.96.0** (stable, MSVC); `rust-toolchain.toml` | Measured on the build machine; the workspace builds, tests, and passes `clippy -D warnings` and `rustfmt` |
| Rust crates | serde 1, serde_json 1, sha2 0.10, hmac 0.12, getrandom 0.2, base64 0.22, tiny_http 0.12, winreg 0.56 (spike), jsonschema 0.61.1 (tests only) | Locked by `Cargo.lock` |
| Tauri | **v2 line**. The exact crate version is pinned at the M1 scaffold, to match `@tauri-apps/cli` 2.12.4 (current npm stable) | SAD §4 baseline. Tauri 3.0 is alpha on crates.io and is **not** adopted without an ADR |
| Node.js | 24.20.0 | Measured |
| pnpm | Not installed. Pin via `packageManager` (current 12.18.0) and corepack at the M1 scaffold | SAD §4 baseline |
| WebView2 runtime | 157.0.4341.42 present | Measured |
| Dev OS | Windows 11 build 26300 | **Not** a certified matrix cell (19045/26100); dev evidence only |

Workspace layout:

- `crates/kohbor-gsi`: pure intake logic (CMP-INGEST, part of CMP-READMODEL).
- `crates/kohbor-gsi-capture`: the M0 spike tool. It is not shipped in the product.
- **Next (M1):** a Tauri host crate (CMP-STARTUP: Job Object per SDD §14; loopback intake server; settings and runtime status) and a React/TS UI package.

## 5. Component → module map

| Component (SDD) | Module now | Planned | Milestone |
| --- | --- | --- | --- |
| CMP-INGEST | `kohbor-gsi::{strict_json, config, admission, session}` | Host intake server (bind/auth/rate), config writer, DPAPI secrets | M1 |
| CMP-READMODEL | `kohbor-gsi::snapshot` (own-snapshot 2.0.0) | runtime-status, settings, UI adapters | M1 → M3 |
| CMP-STARTUP | — | Tauri host, Job Object, sampler (SDD §14) | M1 |
| CMP-MEDIA, CMP-ALERT | — | Catalog, arbitration, audio owner (EVENT-AUDIO-SPEC) | M2 |
| Others | — | Per SDP | M3+ |

## 6. Local-core slice: what ran

On the build machine, 2026-11-11:

| Command | Result |
| --- | --- |
| `cargo test --workspace` | 25 passed, 0 failed (after the spike: 5 captured-shape replays and a claimed-port test added) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --all` | applied |

What the 19 tests cover:

- **Strict JSON:** duplicate keys, depth and trailing data.
- **Configuration rendering:** loopback URI, heartbeat 1.0, M1 data blocks only, port/slug/token validation.
- **Admission:**
  - the token check;
  - provider 570;
  - menu, spectator and custom game not admitted;
  - unknown game state is unsupported, not mapped;
  - ownership from platform identity: unverified and mismatch both block vitals.
- **Snapshot:** the 2000/2001 ms boundary; missing HP is `unknown`; a genuine 0 stays 0; HP above max is `invalid`.
- **Contract check:** every produced snapshot is validated against `contracts/own-snapshot.schema.json`.
- **Session:** start, continue, owner change, restart resume, epochs never regress, and no raw match ID persisted.

Evidence class: **source and synthetic only**. All payloads are fabricated test inputs in `crates/kohbor-gsi/tests/`. None ship in production code. Requirement statuses stay NOT_RUN because their required native/live evidence has not run.

`kohbor-gsi-capture locate` (read-only) on this machine:

- **A-1 confirmed:** the Steam path was read from the registry, and the Dota folder was found through `libraryfolders.vdf` and `appmanifest_570.acf`.
- **A-7 partly confirmed:** the platform identity value is present. Whether it matches the payload needs the capture.
- The GSI folder already holds three configurations from other applications. They are left untouched.

## 7. Capture spike (runs on the owner's machine)

**What it does.**

- **Install:** writes one new file, `gamestate_integration_kohbor_spike.cfg`. It refuses to overwrite an existing file and never touches other files.
- **Capture:** records only to the git-ignored `spike-out/` folder.
- **Redaction:**
  - the token is dropped;
  - `player.steamid`/`accountid`/`name` are replaced;
  - match IDs become per-run salted hash prefixes.
- **Uninstall:** removes the spike file only if its hash is unchanged.

**Commands** (repository root):

```bash
cargo run -p kohbor-gsi-capture -- install
```

Then restart Dota 2 (A-4), play or spectate the sessions listed below, and run:

```bash
cargo run -p kohbor-gsi-capture -- capture --minutes 30
```

```bash
cargo run -p kohbor-gsi-capture -- uninstall
```

**Sessions to cover:**

- menu
- a bot or practice match through post-game
- a pause
- a death and respawn
- selecting an allied unit and a courier (A-8)
- spectating a match or replay (A-6)
- an All Pick game and a Turbo game (A-9)
- first blood by you and by someone else (E-1)

**Output.** `summary.json` reports:

- the maximum payload gap and the maximum gap between identical payloads (A-3);
- the field inventory (A-5);
- the game states seen (E-2);
- whether the payload identity matched the platform identity (A-7);
- hero names per match (A-8).

The redacted corpus feeds the fixtures that close DEC-GSI-INTAKE.

## 8. M0 exit gate

**Met:**

- the technical identity is frozen;
- the scope matrix is in place;
- the Rust toolchain is pinned;
- the decision register has owners;
- the local-core logic slice runs.

**Open:**

| Item | Blocks |
| --- | --- |
| Remaining capture cases: spectate/replay (A-6), a Turbo match (A-9), first blood by self (E-1), explicit unit/courier selection (A-8), a second Steam account (A-7) | DEC-GSI-INTAKE closure and the M1 exit |
| ~~Pin Tauri and pnpm by scaffolding the host app~~: done 2026-11-11 ([M1 Status](M1-STATUS.md)) | — |
| Review of this plan | — |

No unresolved choice has been disguised as code. Where an assumption is unconfirmed, the code yields `unsupported`, `unknown` or `unverified`, never a value.
