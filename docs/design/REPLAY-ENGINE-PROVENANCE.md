# Replay engine provenance

## Pinned build

| Field | Value |
| --- | --- |
| Source repository | `F:\gsi-log\replay-engine-rs` |
| Source commit | `a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9` |
| Engine version | `0.2.0` |
| Target | `x86_64-pc-windows-msvc` |
| Build command | `cargo build --release --locked --offline` |
| Rust toolchain | `rustc 1.96.0 (ac68faa20 2026-05-25)`, host `x86_64-pc-windows-msvc` |
| Built artifact | `release/replay-engine-rs.exe` |
| Size | 3,229,184 bytes |
| SHA-256 | `a05f91761a8bacd84ceec24522568e526e6a35a27525c3dfbfaf5e7f3192dcb6` |
| Kohbor source path | `apps/desktop/src-tauri/binaries/replay-engine-rs-x86_64-pc-windows-msvc.exe` |

The lead built the source commit in `F:\cargo-lanes\replay-engine-a609ecc`. This task copied the artifact into the Kohbor source tree and recomputed its size and SHA-256. `replay_library::engine` accepts this hash and engine version before each spawn. The earlier RP1 development binary hash `5955ddc1b2c8bc5881876ca29e6fb73d1be7c449a6715fe76ede849f5828b46a` is removed from the pin set. It predates source commit `a609ecc`, so a development environment still pointing at it receives `engine_hash_mismatch` until it points to the pinned binary.

Tauri's base configuration declares `bundle.externalBin` as `binaries/replay-engine-rs`, with the target-suffixed source file above. This declaration is valid while `bundle.active` remains `false`; the Tauri build configuration processes the external binary, while installer generation remains disabled. No release bundle was built in this task.

## Dependency license inventory

The engine package declares `MIT OR Apache-2.0`. The `Cargo.lock` at the source commit contains 125 registry packages. Their local crate manifests list permissive license expressions: MIT, Apache-2.0, BSD-3-Clause, ISC, Zlib, Unlicense, 0BSD, CDLA-Permissive-2.0, Unicode-3.0, and combinations of these; three dependencies are BSD-3-Clause (`snap`, `prettytable`, and `subtle`). Apache-2.0 with LLVM-exception also appears. No GPL, AGPL, SSPL, or other non-permissive/copyleft license expression was found in the locked registry manifest inventory. This is a metadata inventory, not a legal opinion or independent rights review.

## Rebuild

1. Check out `a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9` from `F:\gsi-log\replay-engine-rs`.
2. Use Rust `1.96.0-x86_64-pc-windows-msvc` and an owned Cargo target directory on F:.
3. Run `cargo build --release --locked --offline` from the repository root.
4. Hash `target/release/replay-engine-rs.exe` with SHA-256. Accept it for Kohbor only if it is 3,229,184 bytes and matches the pinned hash above; otherwise investigate the difference and update the pin only through a reviewed provenance change.

The engine reads the `.dem` bytes passed to it and writes a schema 3.0 Match Bundle. Its dependency disables `source2-demo`'s default download features and enables the Dota parser feature only. See [REPLAY-PIPELINE.md](REPLAY-PIPELINE.md) for the host contract, limits and runtime path order.
