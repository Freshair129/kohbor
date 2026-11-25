# Replay engine provenance

## Pinned build

| Field | Value |
| --- | --- |
| Source repository | `F:\gsi-log\replay-engine-rs` |
| Source commit | `5db800267e3406f6322a5fb0f52285b9b0c7c4dc` |
| Schema implementation commit | `9c314e03b0f6222f14a8ad478bb55af7e5bda4d6` |
| Engine version | `0.3.0` |
| Output bundle schema | `3.1`; Kohbor also admits existing `3.0` bundles |
| Target | `x86_64-pc-windows-msvc` |
| Build command | `cargo build --release --locked --offline` |
| Rust toolchain | `rustc 1.96.0 (ac68faa20 2026-05-25)`, host `x86_64-pc-windows-msvc` |
| Lead build lane | `F:\cargo-lanes\replay-engine-9c314e0` |
| Built artifact | `release/replay-engine-rs.exe` |
| Size | 3,258,368 bytes |
| SHA-256 | `9d7aa88bbd8d1b31cd35489dcbecd88058c6ba68a118f88042e2cc262ff5d05d` |
| Kohbor source path | `apps/desktop/src-tauri/binaries/replay-engine-rs-x86_64-pc-windows-msvc.exe` |

The Rust source at `5db8002` is identical to the schema implementation at `9c314e0`; `5db8002` only rebuilt the WASM packages. The schema implementation adds winner and draft results, total gold earned and attacker-side damage to enemy heroes. The lead built the executable from `5db8002` with Rust 1.96.0 in the lane above. This task recomputed the copied artifact's byte length and SHA-256. `replay_library::engine` checks its size, hash and engine version before each spawn. Any size or hash mismatch fails as `engine_hash_mismatch`.

The executable in the engine repository's own `target/release` folder has SHA-256 `33f0775a97b91701bcbed0eca1001ca17b2c21701f7ba12f59b73b7c587863a1`. Its timestamp predates source commit `5db8002`, so it differs from the lead-built artifact. Only the lead-built executable listed above is pinned; do not pin the repository's pre-commit build.

Tauri's base configuration declares `bundle.externalBin` as `binaries/replay-engine-rs`, with the target-suffixed source file above. This declaration is valid while `bundle.active` remains `false`; the Tauri build configuration processes the external binary, while installer generation remains disabled. No release bundle was built in this task.

## Dependency license inventory

The engine package declares `MIT OR Apache-2.0`. Comparing the 0.2.0 lock at `a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9` with the 0.3.0 lock at `5db800267e3406f6322a5fb0f52285b9b0c7c4dc` shows one change: the `replay-engine-rs` package version changes from `0.2.0` to `0.3.0`. All 125 registry packages, versions and sources are unchanged, so the prior license inventory applies unchanged: local crate manifests list MIT, Apache-2.0, BSD-3-Clause, ISC, Zlib, Unlicense, 0BSD, CDLA-Permissive-2.0, Unicode-3.0 and combinations of these; three dependencies are BSD-3-Clause (`snap`, `prettytable` and `subtle`), and Apache-2.0 with LLVM-exception also appears. No GPL, AGPL, SSPL or other non-permissive/copyleft license expression was found in the locked registry manifest inventory. This is a metadata inventory, not a legal opinion or independent rights review.

## Rebuild

1. Check out `5db800267e3406f6322a5fb0f52285b9b0c7c4dc` from `F:\gsi-log\replay-engine-rs`.
2. Use Rust `1.96.0-x86_64-pc-windows-msvc` and an owned Cargo target directory on F:.
3. Run `cargo build --release --locked --offline` from the repository root.
4. Hash `target/release/replay-engine-rs.exe` with SHA-256. Accept it for Kohbor only if it is 3,258,368 bytes and matches the pinned hash above; otherwise investigate the difference and update the pin only through a reviewed provenance change.

The engine reads the `.dem` bytes passed to it and writes schema 3.1 Match Bundles. Schema 3.1 adds keys only; schema 3.0 data remains supported by both host and renderer admission. See [REPLAY-PIPELINE.md](REPLAY-PIPELINE.md) for the host contract, limits, validation and runtime path order.
