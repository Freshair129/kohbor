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

Tauri commands: `runtime_get_status` (payload validated against `runtime-status.schema.json` in tests), `own_snapshot` (own-snapshot 2.0.0) and `gsi_setup` (a user action). The capability file grants `core:default` to the control window only.

## 3. What ran (development machine, 2026-11-11)

| Check | Result | Evidence class |
| --- | --- | --- |
| `cargo test --workspace` | 33 passed (8 host: throttle, DPAPI round trip, intake token/menu/rate limit, two runtime-status schema checks) | source / synthetic |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean | source |
| `pnpm build` (tsc + vite) | passes | source |
| `pnpm tauri build` (release, no bundle) | builds `kohbor-desktop.exe` | source |
| Launch the release app and enumerate its process tree | Host plus 6 `msedgewebview2.exe`, **all `IsProcessInJob = true`**; total Working Set ≈ 381 MB; all exited with the host | native observation (dev OS 26300, not a certified cell) |
| UI screenshot | Overview showed: not configured; owner unknown; waiting for match; mode unverified; critical audio not ready; vision unavailable; CPU 0.3%, RAM 394 MB; account unused. Own-hero fields "—" | native observation |

The game-connection setup was **not clicked** during these checks, because it writes into the Dota folder. The owner can run it from the app.

## 4. Next in M1

1. Owner runs `gsi_setup` from the app, restarts Dota 2, and checks a live "สด" state with own vitals (native/live evidence for CORE-004 and DATA-001..003).
2. Cold offline startup test, plus a failure-injection test for containment (CORE-002 exit code 1).
3. Settings UI (locale) through the `settings-patch` contract.
4. Resource runs per the NFR protocol: idle and live profiles, ten minutes × 3, with receipts.
