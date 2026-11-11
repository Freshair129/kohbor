---
title: "M1 Status"
doc_id: "M1-STATUS"
status: draft
version: "0.3.6"
updated: "2026-11-11"
authority: "DEC-BASELINE; SDP milestone M1"
owner_role: "Engineering lead (builder)"
approval_status: "not_approved"
---

# M1 Status

M1 delivers local startup, the resource cap, GSI admission and an honest read model (SDP). This page records what the first M1 slice contains and what actually ran. Evidence classes stay separate. The **native** item below is a development-machine observation; it is not certified-matrix evidence. Live and acceptance runs are **NOT_RUN**.

## 1. Toolchain pinned at the M1 scaffold

| Item | Pinned |
| --- | --- |
| Rust | 1.96.0 (`rust-toolchain.toml`) |
| Tauri | `tauri =2.12.5`, `tauri-build =2.7.4`, `@tauri-apps/cli 2.12.4`, `@tauri-apps/api 2.12.4` |
| Windows API | `windows =0.61.3` (JobObjects, ProcessStatus, Threading, Cryptography) |
| Package manager | pnpm 12.18.0 through corepack (`packageManager` in `apps/desktop/package.json`) |
| Frontend | React 19.3.1, Vite 8.3.5, @vitejs/plugin-react 6.1.2, TypeScript 7.0.2 |
| Supply-chain policy | `apps/desktop/pnpm-workspace.yaml` sets `minimumReleaseAge: 20160`, so every npm package, including transitive ones, must be at least 14 days old. Vite 8.3.8 (published the same day) and plugin-react 6.3.0 (2 days old) were downgraded. The test-only `jsonschema` crate is pinned `=0.59.0` (0.61.1 was 1 day old). Cargo has no equivalent policy yet: auditing locked crate ages is a follow-up |

## 2. What the slice contains (`apps/desktop`)

| Requirement | Implementation | Notes |
| --- | --- | --- |
| CORE-001 | The app starts with no account or network; the account row reads "ไม่ได้ใช้" | Local core only |
| CORE-002 | `src-tauri/src/containment.rs`, `main.rs` | Job Object with a 10% hard CPU cap and `KILL_ON_JOB_CLOSE`, no breakaway. The host joins the job **before** Tauri/WebView2 start. On failure: `CPU_CONTAINMENT_FAILED`, exit 1 |
| QUALITY-002 | `containment.rs` Sampler and Throttle | Per-second normalized job CPU and full Working Set of job processes; the soft throttle is implemented per SDD §14 |
| CORE-004 | `src-tauri/src/intake.rs` (`Runtime::setup`, `start`) | Locates the game; skips ports claimed by other configs; writes only `gamestate_integration_kohbor.cfg` and never overwrites a changed file; persists settings; startup binds only the persisted port, with the GSI-INTAKE-SPEC §2.4 rules |
| DATA-001..004 | `intake.rs` with `crates/kohbor-gsi` | Loopback-only server with the 131,072-byte limit, strict JSON, token check, 50 requests/s cap, admission/ownership, session tracker with persisted epoch; freshness comes from monotonic arrival time |
| CORE-005 | `status.rs`, UI | Mode always reported "unverified"; independent health rows |
| Secrets | `src-tauri/src/secrets.rs` | DPAPI current-user blobs for the intake token and the HMAC installation key |
| UI | `apps/desktop/src` | Command Deck Overview: eight readiness rows from `runtime_get_status` and an own-hero card from `own_snapshot`; Thai copy for error codes; nothing fabricated |

Tauri commands: `runtime_get_status` (payload validated against `runtime-status.schema.json` in tests), `own_snapshot` (own-snapshot 2.0.0) and `gsi_setup` (a user action; `replace: true` is the separate explicit confirmation to replace an owned configuration that was edited outside the app). The capability file grants `core:default` to the control window only.

## 3. What ran (development machine, 2026-11-11)

| Check | Result | Evidence class |
| --- | --- | --- |
| `cargo test --workspace` | 33 passed (8 host: throttle, DPAPI round trip, intake token/menu/rate limit, two runtime-status schema checks) | source / synthetic |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean | source |
| `pnpm build` (tsc + vite) | passes | source |
| `pnpm tauri build` (release, no bundle) | builds `kohbor-desktop.exe` | source |
| Launch the release app and enumerate its process tree | Host plus 6 `msedgewebview2.exe`, **all `IsProcessInJob = true`**; total Working Set ≈ 381 MB; all exited with the host | native observation (dev OS 26300, not a certified cell) |
| UI screenshot | Overview showed: not configured; owner unknown; waiting for match; mode unverified; critical audio not ready; vision unavailable; CPU 0.3%, RAM 394 MB; account unused. Own-hero fields "—" | native observation |

The game-connection setup was **not clicked** during the checks above, because it writes into the Dota folder.

### 3.1 Owner live test, 2026-11-11

**Build under test.**

- The app is `kohbor-desktop.exe`, sha256 `4f8abc7c2c0785162336a1bd7845813aa41bf70a4d7046edc5f733ccaa480751`.
- It was built from commit `624112e`, **before** the npm pin downgrade in `d68022c`. Its embedded UI bundle was therefore built with Vite 8.3.8; the Rust code is unchanged by that later commit.
- The test ran on the owner's machine (Windows build 26300, not a certified cell) against one bot match.

| Step | Observation | Evidence class |
| --- | --- | --- |
| Owner clicked "ตั้งค่าการเชื่อมต่อเกม" | `gamestate_integration_kohbor.cfg` was written on **port 3001**. Port 3000 was skipped because another application's config claims it. The other three configurations were untouched (timestamps unchanged). Settings store `port 3001`, `token_ref dpapi:gsi-token` and the config hash; no plaintext token | native |
| Owner restarted Dota 2 | The listener on 3001 received traffic | native |
| Hero selection | Intake "สด" (0.1 s); ownership "ยืนยันแล้ว"; session #1, phase "เลือกฮีโร่"; own vitals "—", as designed for the draft phase ([screenshot](evidence/m1-live-2026-11-11-draft.png)) | live (one match) |
| In game | Intake "สด" (0.0 s); phase "กำลังเล่น"; **session #1 continued** with no new epoch; own hero, level, HP x/y, mana %, alive status all shown from admitted data; CPU 0.1%, RAM 413 MB ([screenshot](evidence/m1-live-2026-11-11-in-game.png)) | live (one match) |

This is a single owner-machine observation. It does **not** satisfy the acceptance procedures: TP/UAT counts, the certified matrix and the stale/ownership negative cases on live data are still to be run.

### 3.2 Hardening and UX pass, 2026-11-11

A review of the M1 slice found the defects below. They are fixed on branch `m1/hardening-ux`. **The live observations in §3.1 were made on the earlier build and do not cover this one.**

| Area | Defect found | Fix |
| --- | --- | --- |
| Game lookup | The Steam path in the registry and the same library in `libraryfolders.vdf` are spelled differently. Compared as text, a default installation counts twice and setup fails with `GSI_GAME_NOT_FOUND`. It worked on the owner machine only because the two spellings differ in the drive letter alone | Candidates are compared by resolved location. The manifest `installdir` must be a single folder name |
| Second instance | A second launch could not bind the port and, with the game closed, rewrote the owned configuration to another port, orphaning the first instance | Single-instance guard (named mutex); the second launch focuses the running window and exits |
| Process check | `tasklist` was spawned (console flash, executable search path). A failed check counted as "game not running" and allowed a rewrite | The process list is read in-process (ToolHelp). An unreadable list counts as running: no rewrite |
| Admission | An authenticated payload that failed admission (foreign provider) still renewed freshness and consumed a sequence number, against GSI-INTAKE-SPEC §3 and §5 | It no longer does. Arrival time is now taken after the body is complete |
| Web content | Any web page could make the intake read and parse 128 KiB bodies (it could not authenticate) | Requests with an `Origin` header are refused with 403 before the body is read (assumption A-10, not yet confirmed live) |
| Config conflict | `GSI_CONFIG_CONFLICT` was a dead end: no "replace owned configuration" action existed (GSI-INTAKE-SPEC §2.2), for example after the data directory was lost | `gsi_setup` with `replace: true`, behind a two-step confirmation in the UI; it rotates the token (§2.3) |
| Setup state | A failed setup left the row at "not configured" with a transient message. A failed re-setup could leave "listening" with no listener. Re-setup raced the closing socket of its own listener | Failure is reported as `unavailable` with its code. The listener is released only after the conflict rule passes; its own port stays a candidate, with a bounded bind retry. Setup runs are serialized and run off the main thread |
| Port claims | A leftover `*.cfg.tmp` of our own, or a non-configuration file, could claim a port | Only other `gamestate_integration_*.cfg` files of at most 64 KiB are read |
| Persistence | Temporary files were not flushed and stayed behind after a failed write. A corrupt runtime-state file restarted the session epoch at 1 (§6 forbids reuse) | Atomic writes flush and clean up. A corrupt file makes the epoch jump once to wall-clock seconds |
| Robustness | A poisoned mutex made every later command panic. The persisted port was truncated to 16 bits without a range check | Locks recover; the persisted port must be 3000–3010 |
| WebView policy | No defect; tightening only | CSP additionally sets `object-src`, `base-uri`, `form-action` and `frame-ancestors` to `'none'` |
| UI truthfulness | A failed status poll was silent, so an old "สด" reading stayed on screen as if live. The header showed "กำลังเล่น" in the fresh style while the source was stale | After three failed polls a banner says the screen has lost the host, and the values are dimmed. Ownership and match chips switch to the stale encoding when the source is not live |
| UI guidance | The setup button did not say what it writes. "ไม่พบ Dota 2 — เลือกโฟลเดอร์เกม" offered a folder choice that does not exist. Empty hero states all read "ยังไม่มีข้อมูล" | One next-step panel per state, with the write disclosure and the recovery action; honest copy for the missing folder picker; distinct hero states (draft, post-game, unverified, unsupported, stale with age); HP and mana bars drawn only from known values; chips use the glyph and border encodings of `design-tokens.json`; polling pauses while the window is hidden |

**What ran for this pass (development machine).**

| Check | Result | Evidence class |
| --- | --- | --- |
| `cargo test --workspace` | 45 passed: 20 host (was 8), and 10 + 5 + 10 in `kohbor-gsi` | source / synthetic |
| `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check` | clean | source |
| `pnpm build` (tsc + vite) | passes | source |
| UI states in a browser at 1280 × 688 against a scripted stand-in for the three commands: host lost, not configured, config conflict with two-step replace (`{replace: true}` sent only on confirm), live, stale with an invalid field, host lost with values | Rendered as designed; no page scroll in any state | synthetic fixture (not WebView2, not live data) |

**Not run for this pass.** The release build, the native launch, the process-in-job check and any live-game check. The owner's earlier build was running with a live game on the same data directory, so the new build was not started. Not exercised natively: the single-instance hand-over, the CSP change in WebView2, the replace action against the real game folder, and A-10.

## 4. Next in M1

1. ~~Owner runs `gsi_setup` and checks a live "สด" state with own vitals~~: done 2026-11-11 (§3.1) on the earlier build. Next: rebuild from the hardening pass (§3.2) and repeat the live check, including A-10, a second launch while running and the replace action. Then repeat on a certified matrix cell, and on live data check the stale state (pause the game's network or close it) and ownership mismatch.
2. Cold offline startup test, plus a failure-injection test for containment (CORE-002 exit code 1).
3. Settings UI (locale) through the `settings-patch` contract.
4. Resource runs per the NFR protocol: idle and live profiles, ten minutes × 3, with receipts.
