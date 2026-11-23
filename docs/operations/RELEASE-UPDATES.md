# Windows update release runbook

This runbook prepares a signed NSIS installer and its Tauri updater manifest. The release helpers are Python 3.11+ standard-library scripts under `tools/release/`. They do not publish releases. The owner reviews the draft commands and publishes by hand or after an explicit authorization to the lead.

## Version and signing rules

- Every release must have a strictly higher SemVer than the latest version already offered to users. The updater rejects equal or older versions. The release version is read from the workspace `Cargo.toml`.
- The NSIS installer is per-user (`currentUser`) so passive updates do not need elevation. It uses the small WebView2 download bootstrapper; if the Evergreen runtime is missing, initial installation needs internet access.
- The owner keeps the updater private key and password on the owner's machine. In the owner's PowerShell session, set either `TAURI_SIGNING_PRIVATE_KEY` or `TAURI_SIGNING_PRIVATE_KEY_PATH`, plus `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, from the owner's protected secret source. Do not put values in command arguments, scripts, files, logs, screenshots or chat. The build helper checks only that the required names are set and passes the environment to Tauri.
- Check variable names without displaying values:

  ```powershell
  Get-ChildItem Env: | Where-Object Name -in @(
    'TAURI_SIGNING_PRIVATE_KEY',
    'TAURI_SIGNING_PRIVATE_KEY_PATH',
    'TAURI_SIGNING_PRIVATE_KEY_PASSWORD'
  ) | Select-Object -ExpandProperty Name
  ```

- The installer has no Authenticode signature. Windows may show Microsoft Defender SmartScreen. The download page should say: **“Windows may show a SmartScreen warning because this installer is not Authenticode-signed. Download only from the official `Freshair129/kohbor-releases` page. If you trust the source and have verified the release, select More info, then Run anyway.”**

## Candidate, draft, test, promote, publish

1. Bump the workspace version to a SemVer higher than the currently offered release. Review the exact source revision and keep the worktree clean. Do not change Cargo dependencies as part of a version bump unless separately approved.
2. Before the owner runs a real Cargo build, use the owned F: Cargo target lane with process-scoped `CARGO_TARGET_DIR`, complete the applicable disk preflight and acquire that lane's exclusive lock. The script inherits `CARGO_TARGET_DIR`; it does not choose, clean or share a target directory.
3. In the owner's PowerShell session, prepare a candidate. Pass the installed Tauri CLI path as an argument; do not embed a machine-specific CLI path in scripts:

   ```powershell
   python tools/release/build_candidate.py `
     --tauri-cli 'F:\path\to\tauri.cmd' `
     --out 'F:\kohbor-coord\releases'
   ```

   The helper refuses a dirty worktree or missing signing variables. It runs the NSIS updater build with Cargo/Corepack/pnpm offline settings and Cargo `--locked`, then copies the installer and `.sig` to `<out>/<version>/candidate/`. `candidate.json` records the source commit, version, lockfile hashes, artifact names, sizes, hashes and build time. The worker dry-run is not a build receipt.
4. Write concise update notes to a UTF-8 text file, then create `latest.json` with the exact candidate tag:

   ```powershell
   python tools/release/make_latest_json.py `
     'F:\kohbor-coord\releases\<version>\candidate' `
     --notes-file 'F:\path\to\release-notes.txt' `
     --tag 'v<version>'
   ```

   The tag must be `v<version>`. The manifest uses the public updates repository URL; its `signature` is the `.sig` file content verbatim.
5. Print the two draft-release commands:

   ```powershell
   python tools/release/publish_plan.py 'F:\kohbor-coord\releases\<version>\candidate'
   ```

   The public `Freshair129/kohbor-updates` draft contains the installer, `.sig` and `latest.json`. The separate `Freshair129/kohbor-releases` draft contains only the installer for the invite-gated first download. Review the commands and artifacts before the owner runs them.
6. Test the draft installer on Windows. For the in-app old-to-new test, account for the fixed public endpoint `https://github.com/Freshair129/kohbor-updates/releases/latest/download/latest.json`: a GitHub draft is not served as the anonymous public `latest` release. The owner must authorize a published test release/endpoint or a separately scoped test build before the app can fetch the draft manifest. Until then, record the in-app update test as NOT_RUN; do not treat installing the private draft installer as proof of updater acceptance. When an authorized test source is available, install the prior signed version, check for the new strictly higher version, then explicitly download and install it. Confirm the update signature is accepted, the app restarts, and update actions remain deferred during draft, pregame and live match states. Record native results separately from synthetic script tests.
7. Prepare the proposed stable directory by copying the approved candidate's installer, `.sig` and `latest.json` byte-for-byte. Do not rebuild or re-sign. Run:

   ```powershell
   python tools/release/promote_check.py `
     'F:\kohbor-coord\releases\<version>\candidate' `
     'F:\kohbor-coord\releases\<version>\stable'
   ```

   Promotion passes only when candidate and stable installer/signature SHA-256 values match, `latest.json` is byte-identical, and both manifests' signature strings match their `.sig` files.
8. After review and owner release approval, publish the approved public update release and the private first-download release. The public update release must expose `latest.json` at `https://github.com/Freshair129/kohbor-updates/releases/latest/download/latest.json`. Publishing is an owner action; the helper only prints draft commands.

## Recovery (UPDATE-004)

Keep the last known-good installer and its `.sig` in the protected release archive. If an update needs repair, prepare and sign a new emergency release with a strictly higher SemVer, test it, and publish its manifest. Never lower `latest.json` to an older version: the app offers only strictly newer versions and a manifest downgrade is not a recovery route. Preserve the known-good pair while the emergency candidate is reviewed.

## Per-release checklist

- [ ] Workspace SemVer is strictly higher than the version currently offered.
- [ ] Exact source commit is reviewed; worktree is clean; lockfiles are unchanged unless approved.
- [ ] Owner signing variable names are present; no secret value was printed, stored or passed as an argument.
- [ ] F: Cargo lane preflight and exclusive lock are satisfied before the real build.
- [ ] NSIS installer and `.sig` were built offline with locked dependencies and captured in `candidate.json`.
- [ ] `latest.json` has the matching `v<version>` tag, notes, RFC 3339 publication date and exact signature text.
- [ ] Public update and private first-download drafts contain only their specified assets.
- [ ] Old-to-new native install/update test and match deferral test have recorded results.
- [ ] Stable copies pass `promote_check.py` with unchanged installer and signature bytes.
- [ ] Owner approved and published both releases; the download page includes the SmartScreen note.
- [ ] Last known-good installer and `.sig` remain available for a higher-version recovery.
