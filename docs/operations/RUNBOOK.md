---
title: "Operations and Release Runbook"
doc_id: "RUNBOOK"
status: draft
version: "0.3.4"
updated: "2026-11-10"
authority: "DEC-BASELINE; see DOCUMENT-CONTROL"
owner_role: "Operations owner; people not assigned"
approval_status: "not_approved"
---

# Operations and Release Runbook


## Configuration

Separate user settings, trusted app configuration, runtime credentials and release secrets. Settings use a schema version and atomic migration. Secrets never appear in renderer state, logs, command-line examples, repository files or evidence exports. Ship public verification keys only.

Configuration inventory must include loopback GSI address/token, own integration-config path, capture/calibration profile, hotkeys, audio device/volume/mute, verified pack selection, local model paths/revisions, separate consent flags, optional relay origin, optional account endpoint, OBS loopback endpoint and trusted update channel. Defaults must keep cloud context, peer sharing, microphone and recording off.

## Diagnostics and observability

Produce local reason-coded events for source invalidity, coverage loss, package rejection, dropped stale work, recording failure and resource unavailability. Bound log volume. Diagnostic export is a deliberate local action and excludes tokens, raw microphone/transcript, peer vitals, account/security data and full raw game payloads.

Keep operational health distinct from user facts. An unavailable GPU temperature is an em dash; it is not a reason to guess system safety. Do not add telemetry/crash upload to make diagnosis convenient.

## Installation and recovery

The installer establishes new stable identity paths and optional game integration configuration with reversible ownership. Preserve unrelated files. Package installation uses staged verification; update installation verifies signatures and artifact hashes before replacing bytes. Write migration/backup/rollback procedures for settings and local snapshots.

Application exit releases owned recording control and bounded tasks; clean only exact owned resources. Process containment and kill-on-close behavior need native proof including child membership. Do not kill arbitrary model or OBS processes belonging to another user/session.

## Release model

1. Freeze an exact source revision and version.
2. Run source and contract gates plus the required native/live evidence.
3. Build and sign one candidate artifact with secured release keys.
4. Verify artifact digest, signature, installation, supported matrix and known issues.
5. Promote the exact same approved bytes to stable; do not rebuild/resign during promotion.

Tags alone do not publish. Candidate and stable are separate channels. Production publication/promotion is an explicit release action, not a side effect of a normal commit. The app's signed stable updater remains independent of account sign-in.

## Update behavior

Native code owns trusted manifest endpoints and public keys. Renderer inputs cannot choose arbitrary update URLs. Manifest platform keys describe actual platforms, not channels. Signature failure blocks installation. During a match or active unsafe media state, stage or defer installation without interrupting gameplay. A missing network does not block local startup.

## Rollback

Retain last approved artifact identity and migration compatibility. If version downgrade is unsupported, release a new higher emergency-patch version rather than assuming pointing a manifest backward will work. Test repair/corrupt-download/interrupted-install paths and preserve useful local data.

## Evidence before public distribution

Include source/artifact hashes, locked dependencies, license/media provenance, selected feature scope, measured operating matrix, actual test outcomes, privacy traffic evidence, limitations and release owner decision. Prototype screenshots, synthetic metrics and source comments cannot substitute for live acoustic/resource evidence.

No production backend, Store, signing key, registered domain or media-rights clearance is supplied by this specification. Missing external prerequisites are visible unavailable services, not placeholder endpoints shipped as working.

## Related engineering diagrams

- [DGM-018 — Exact-candidate release and safe update](../diagrams/INDEX.html#DGM-018)

Read each figure with its catalog notes and linked SRS requirements. Short labels do not replace failure, consent or cancellation guards. The [diagram index](../diagrams/README.md) provides editable sources and maintenance rules.
