# In-app updater design (CMP-UPDATER)

Status: draft for owner review, 2026-11-23. Requirements: UPDATE-001..004 (SRS), PN-012. Decision: DEC-UPDATE-ENDPOINT and its 2026-11-14 and 2026-11-23 entries in DECISION-LOG.md.

## Intent

A tester who installed Kohbor from the invite-gated download can move to a newer signed build from inside the app, without visiting a website. The app never installs anything the owner did not sign, never interrupts a match, and never acts without the user's click.

## Decided (owner)

| Topic | Decision | Source |
| --- | --- | --- |
| Channel | GitHub Releases. Installer and updater assets in a separate public release repository; source stays private | 2026-11-14 |
| Packaging | NSIS installer with in-app update; no portable exe | 2026-11-14 |
| Signing | Updater key (minisign/Ed25519 via the Tauri signer). The owner generates and keeps the private key and gives the team only the public key. No Authenticode for now | 2026-11-14 |
| Library | `tauri-plugin-updater` =2.14.1 behind the existing `updater` Cargo feature | 2026-11-23 |
| Update host | A new public updates repository. `Freshair129/kohbor-releases` stays private for the invite-gated first download. The app holds no token | 2026-11-23 |
| Consent | Ask each time. Automatic checking is off by default. The user turns it on or presses Check. Each download and each install needs its own click. Never offered or run while a match is in draft, pregame or live | 2026-11-23 |

## Open (owner)

- The public updates repository exists: `Freshair129/kohbor-updates` (created 2026-11-23). Endpoint: `https://github.com/Freshair129/kohbor-updates/releases/latest/download/latest.json`. `UPDATE_ENDPOINT` is set when the public key is set too, so the updater never runs half-configured.
- The updater public key was supplied by the owner on 2026-11-23 (minisign key ID 46F0CD83B29D4677). It is embedded in `update.rs` (`UPDATE_PUBKEY`) and in `tauri.conf.json` (`plugins.updater.pubkey`, with `requireSignedVersion: true`). The private key and its password stay on the owner's machine.
- Publishing any release, tagging, and promotion.

While the endpoint or the public key is `None`, the updater reports `NO_ENDPOINT` or `NO_PUBKEY` and does nothing else. That is the fail-closed default.

## Behaviour (app)

1. **Trusted origin only (UPDATE-001).** The endpoint and public key are compile-time constants in `update.rs`. No renderer argument, setting or file can supply a URL, a key or a target path. The plugin verifies the signature of the downloaded artifact against the embedded public key before installing. Any network, parse or signature failure ends in a reason code and leaves the running app untouched.
2. **States.** `idle → checking → up_to_date | available(version, notes) → downloading(progress) → ready → installing`, plus `failed(reason)`. Reasons use the existing constants: `BUILD_WITHOUT_UPDATER`, `NO_ENDPOINT`, `NO_PUBKEY`, `MATCH_ACTIVE`, `NETWORK`, `SIGNATURE`, `BAD_RESPONSE`, `BACKEND_UNAVAILABLE`.
3. **Consent.** Check, Download and Install are three user actions. Automatic checking is a setting that defaults to off. When it is on, the app checks at most once per app start and once per 24 hours, and only shows the result; it never downloads by itself.
4. **Safe timing (UPDATE-002).** While a match is in draft, pregame or live, check, download and install are refused with `MATCH_ACTIVE` and the UI says why. A download that is in progress when a match starts finishes but is not installed; install waits for the user after the match. The prompt never takes focus and never blocks local core.
5. **Install.** The NSIS updater runs in passive mode, which closes Kohbor, installs and restarts. Before install the app stops its host threads the same way it does on Quit, so audio levels are restored (MEDIA-009).
6. **Versions.** Only a strictly newer SemVer than the running version is offered (`update::is_newer`). The app never accepts a downgrade from the manifest (UPDATE-004 failure behaviour).
7. **Privacy.** The check sends no account, Steam or device identifier, only the standard request the plugin makes. Logs record state changes and reason codes, never URLs with query strings or signatures.

## Behaviour (release, later slice)

- Build the NSIS installer with `bundle.createUpdaterArtifacts`, sign it with the owner's key on the owner's machine, and produce `latest.json`.
- UPDATE-003: stable promotion copies the exact approved candidate assets and `latest.json` entry. A promotion script compares SHA-256 and signature bytes and refuses to rebuild or re-sign.
- UPDATE-004: the runbook keeps the last known-good installer and documents the emergency route: a new higher version, never a manifest downgrade.

## Slices

1. **App slice (this branch).** The real plugin backend behind `UpdateBackend`, wiring in `lib.rs`, commands, the consent setting, match deferral, UI states in the existing `UpdateControl`, and synthetic tests with a throwaway key and a local test server. With `None` constants the shipped behaviour stays fail-closed.
2. **Release slice.** NSIS bundle settings, signing and `latest.json` scripts, the promotion check, and the runbook. It needs the owner's key and the public repository.
3. **Native evidence.** Install an old signed test build, update to a newer signed test build, and confirm deferral during a match (UAT-12).

Release steps and tooling: [RELEASE-UPDATES.md](../operations/RELEASE-UPDATES.md) and `tools/release/`.

## Non-goals

No silent install, no delta updates, no rollback by downgrade, no updates through the account service, no telemetry, no Authenticode.
