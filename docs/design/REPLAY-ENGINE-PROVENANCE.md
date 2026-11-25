# Replay engine provenance

## Pinned build

| Field | Value |
| --- | --- |
| Source repository | `F:\gsi-log\replay-engine-rs` (read-only source) |
| Source commit | `c9951596d46d64467a906907005b2da3073b62dc` |
| Engine version | `0.4.1` |
| Output bundle schema | `3.2`; Kohbor admits `3.0`, `3.1` and `3.2` |
| Target | `x86_64-pc-windows-msvc` |
| Build command | `cargo build --release --locked --offline` |
| Rust toolchain | `1.96.0-x86_64-pc-windows-msvc` |
| Build lane | `F:\cargo-lanes\replay-engine-c995159` |
| Build record | `F:\kohbor-coord\runs\engine-c995159\result.json` |
| Built artifact | `release\replay-engine-rs.exe` |
| Size | 3,368,448 bytes |
| SHA-256 | `9ecf53d7555a3288d386fa93e4dd9bafa2fe2e060660aef9dcb52f972165cfd8` |
| Kohbor source path | `apps/desktop/src-tauri/binaries/replay-engine-rs-x86_64-pc-windows-msvc.exe` |

The pinned artifact was copied from the lead-built lane and its size and SHA-256 were recomputed before use. `replay_library::engine` checks its size and hash before every spawn; a mismatch fails closed as `engine_hash_mismatch`. The source README and `tests/bundle-3.2.schema.json` define the admitted 3.2 additions: `match.meta.daynight` / `daynight_reason` and `match.heroes[].mods_long` / `mod_stacks`. Modifier sources retain the upstream union of hero ID, `{unit, team}`, or `null`; the renderer consumes these values through the replay bundle without inventing defaults.

The 0.3.0 source lock at `5db800267e3406f6322a5fb0f52285b9b0c7c4dc` was compared with the 0.4.1 lock at `c9951596d46d64467a906907005b2da3073b62dc`. The only `Cargo.lock` diff is the `replay-engine-rs` package version changing from `0.3.0` to `0.4.1`; dependency versions and sources are unchanged, including `source2-demo 0.5.11`. The previously reviewed license inventory therefore has the same dependency set. This is dependency metadata, not a legal opinion or independent rights review.

The local source tree's `target/release` output is not the pinned artifact. Only the lead-built executable above is admitted. Tauri declares the target-suffixed sidecar while `bundle.active` remains false; installer generation was not run.

## Native parse evidence

The opt-in host integration test ran the pinned executable against one saved `.dem` under `F:\gsi-log\demos` read-only. On 2026-11-25 the test reported `elapsed_ms=10151 result=ready`; its temporary bundle was under the assigned F: lane and the test removed it. A parser result qualifies only that executable, replay and source snapshot; it is not installed-app, live-match, viewer or product acceptance evidence.

## Rebuild

1. Check out `c9951596d46d64467a906907005b2da3073b62dc` from the read-only source repository.
2. Use Rust `1.96.0-x86_64-pc-windows-msvc` and an owned Cargo target directory on F:.
3. Run `cargo build --release --locked --offline` from the source root.
4. Hash `target/release/replay-engine-rs.exe`. Admit it only when it is 3,368,448 bytes and matches the SHA-256 above; investigate any difference before changing this pin.

Engine 0.4.1 writes schema 3.2. The host and renderer retain admission for schemas 3.0 and 3.1. See [REPLAY-PIPELINE.md](REPLAY-PIPELINE.md) for the host contract, validation and runtime path order.
