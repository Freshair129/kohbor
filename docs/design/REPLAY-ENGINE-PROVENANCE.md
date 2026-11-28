# Replay engine provenance

## Pinned build

| Field | Value |
| --- | --- |
| Source repository | `F:\gsi-log\replay-engine-rs` (read-only source) |
| Source commit | `240f694495a3e93a82bf77f60d6295096a89eeef` |
| Engine version | `0.5.0` |
| Output bundle schema | `3.3`; Kohbor admits `3.0`, `3.1`, `3.2` and `3.3` |
| Target | `x86_64-pc-windows-msvc` |
| Provided artifact | `F:\gsi-log\replay-engine-rs\target\release\replay-engine-rs.exe` |
| Kohbor artifact | `apps/desktop/src-tauri/binaries/replay-engine-rs-x86_64-pc-windows-msvc.exe` |
| Size | 5,334,528 bytes |
| SHA-256 | `c3aa986727cd53b83f560336ef60edfa47b89a6a25e0d191d706cec5e8953f4b` |

The source release artifact was checked before copying and the installed copy was checked again. Both checks reported the size and SHA-256 above. `replay_library::engine` checks its size and hash before every spawn; a mismatch fails closed as `engine_hash_mismatch`. The 0.5.0 bundle adds per-event `deaths[].gold_lost` / `gold_lost_reason` and `deaths[].bounty` / `bounty_reason`; `meta.camps` / `camps_reason`, `gold_unattached` and `neut_gold_unattached`; and `heroes[].neut_gold` / `neut_gold_reason` and `neut_gold_unattached`. The host computes opponent aggregates from those event fields; the engine bundle does not add a top-level `match.opponent_stats` object.

## 7.42 map overview asset

Owner decision, 2026-11-28: use the sharpest installed 7.42 detailed overview now; a true 3D top-down render is later work. The exporter defaults to `--map-style detailed`, accepts `simple` as an explicit alternative, and permits `--map-entry` to override the texture while retaining `resource/overviews/dota.txt` calibration in its provenance. The official-calibration reference came from the `material` entry, `materials/overviews/dota.vmat` → `materials/overviews/dota_tga_d8178876.vtex_c` (512×512, 1,050,628 bytes, SHA-256 `6bd12841e16c747578faaee8aafdacea805a6cc8bfc5367eb08ad8fd58336196`), upscaled by the exporter with smooth bilinear filtering to 1024×1024 (1,048,072 bytes, SHA-256 `29376a9f3597da13c2e375200f18c01f649270c2eae38ca5ec5ac3c6931a36d2`). The chosen override is `materials/overviews/dota_psd_8b4a9409.vtex_c`, 1024×1024, 4,196,356 source bytes, SHA-256 `a10430c9dab968ed2fdca0c0f18d1a3005f8771d2dc8ebe6eb9f69883adccff5`; its native-size 1024×1024 PNG is 913,042 bytes, SHA-256 `447028ade5f1075f65e7dbc9fd1a4979fe948973c01f8c5cf2b403bbf3a38769`.

The owner/lead had checked the official calibration against a 7.42 replay. This update registered the override to that official-material reference with Sobel edge cross-correlation. Candidate-to-reference fit: `reference_x=1.0156*candidate_x-8`, `reference_y=1.0156*candidate_y-14`; edge correlation improved from `0.4957714481` at identity to `0.9706959282`. The official-mapping displacement at top-left, top-right, bottom-right, bottom-left and centre is respectively `16.1245`, `16.1041`, `8.1963`, `8.2363` and `6.0206` px, so the pack uses the composed transform and its method records “registered to the official overview”. Runtime coefficients are `px=[0.05322376334585866,512.0126033871603]` and `py=[-0.05322376334585866,517.9204411185506]`. The reference fountain's measured cyan-feature centroid projects 0.3484 px from the candidate cyan centroid and remains inside its measured bounds. The Tormentor at `(7744,-6208)` projects to `(924.1774,848.3336)` inside the measured bright feature; its rounded projected pixel matches the feature mask. The ancient check transforms back to official-reference pixel `(192.0,801.3)`. World radii use the same composed scale (`r_px=r_world*sqrt(abs(a*c))`); a 185-unit radius is about 9.8464 px. Provenance records both logical material paths, the selected texture override, source and output hashes, registration metrics and the Valve-art rights-review note. Hero paths and ward-vision overlays are not implemented here; the later true 3D render is outside this round.

### Previous pin: 0.4.2

Engine 0.4.2 (source `6626b20`, 2026-11-25) embedded the 7.42 patch pack next to 7.41f and selected a pack by replay end date (7.41f until 2026-11-17, 7.42 from 2026-11-18); its schema was 3.2. It was pinned after engine 0.4.1 returned `unknown_patch` ("no patch pack covers end date 2026-11-25") for the owner's 2026-11-25 replay; the lead ran 0.4.2 read-only on that replay into a scratch folder: exit 0, patch 7.42, analyze 4.0 s, output deleted afterwards. The only `Cargo.lock` change from 0.4.1 to 0.4.2 was the `replay-engine-rs` package version.

The 0.3.0 source lock at `5db800267e3406f6322a5fb0f52285b9b0c7c4dc` was compared with the 0.4.1 lock at `c9951596d46d64467a906907005b2da3073b62dc`. The only `Cargo.lock` diff is the `replay-engine-rs` package version changing from `0.3.0` to `0.4.1`; dependency versions and sources are unchanged, including `source2-demo 0.5.11`. The previously reviewed license inventory therefore has the same dependency set. This is dependency metadata, not a legal opinion or independent rights review.

Tauri declares the target-suffixed sidecar while `bundle.active` remains false; installer generation was not run. This update did not build the 0.5.0 source artifact or independently compare dependency locks.

## Native parse evidence

The opt-in host integration test ran the previous pinned executable against one saved `.dem` under `F:\gsi-log\demos` read-only. On 2026-11-25 the test reported `elapsed_ms=10151 result=ready`; its temporary bundle was under the assigned F: lane and the test removed it. The lead's separate 0.4.2 read-only parse is recorded above. Engine 0.5.0 was not run against a replay in this update; Rust tests validate source behavior but are not parser or product acceptance evidence.

## Rebuild

1. Check out `240f694495a3e93a82bf77f60d6295096a89eeef` from the read-only source repository.
2. Use the source repository's pinned Rust toolchain and an owned Cargo target directory on F:.
3. Follow that revision's locked offline release build instructions.
4. Hash `target/release/replay-engine-rs.exe`. Admit it only when it is 5,334,528 bytes and matches the SHA-256 above; investigate any difference before changing this pin.

Engine 0.5.0 writes schema 3.3. The host and renderer retain admission for schemas 3.0, 3.1 and 3.2. See [REPLAY-PIPELINE.md](REPLAY-PIPELINE.md) for the host contract, validation and runtime path order.
