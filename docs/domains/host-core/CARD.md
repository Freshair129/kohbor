---
domain: host-core
tier: B
card_version: 3
verified_at_commit: a8b636b
verified_on: 2026-11-25
verified_by: "Codex worker; reviewed listed source changes, not human or owner approval"
---

# Host core: game-state intake and the Tauri host shell

A map, not evidence. Authority: decisions, PRD/SRS, contracts, code and tests, then this card.

## 1. Purpose and boundary

- Owns: `crates/kohbor-gsi`, `crates/kohbor-gsi-capture` (dev spike tool), and in `apps/desktop/src-tauri/src` the files intake, containment, settings, status, instance, secrets, atomic, clock, lib, main, plus `analytics/` and `replay_library/`.
- R2 (lead reads the full diff, read-only review first): the list in docs/delivery/AGENT-WORKFLOW.md section 6, which includes intake, secrets, containment, instance, atomic, tauri.conf.json, capability files and `kohbor-gsi` admission and ownership code.
- Must not touch: `audio/`, `media/`, `host.rs` (`events-audio`); renderer `apps/desktop/src/**` (`ui-overlay`).
- Interfaces: `events-audio` (intake feeds the detector, apps/desktop/src-tauri/src/intake.rs:8); `ui-overlay` (runtime-status, own-snapshot, settings-patch contracts).
- Unassigned or shared: `commands.rs`, and `lib.rs` (which also wires audio, media and the `kohbor-media` scheme) are in no registry row (apps/desktop/src-tauri/src/lib.rs).

## 2. Owner decisions

- Decided: DEC-NAME, DEC-OWN-SNAPSHOT-2-1, DEC-UPDATE-ENDPOINT.
- Open: DEC-GSI-INTAKE, DEC-INTERFACES, DEC-NATIVE-EVIDENCE.
- Replay and Analytics interfaces: DEC-REPLAY-INTEGRATION, DEC-REPLAY-AUTO-PARSE, DEC-ANALYTICS-V2, DEC-LANE-EXPECTATION, DEC-UX-REVIEW-20261125.

## 3. Now / Next / Blocked

- GSI intake and host trust-boundary ownership map to [GSI-INTAKE-SPEC](../../design/GSI-INTAKE-SPEC.md), `apps/desktop/src-tauri/src/intake.rs` and `apps/desktop/src-tauri/src/host.rs` (DEC-GSI-INTAKE).
- Replay discovery, parse queue, manual re-parse, retention and host commands map to [REPLAY-PIPELINE](../../design/REPLAY-PIPELINE.md), `apps/desktop/src-tauri/src/replay_library/` and `apps/desktop/src-tauri/src/analytics/` (DEC-REPLAY-AUTO-PARSE; DEC-REPLAY-INTEGRATION).
- Current-build owner/native acceptance and remaining host gates are pending in [SPRINT-ROADMAP](../../delivery/SPRINT-ROADMAP.md) (DEC-NATIVE-EVIDENCE).
- GSI assumptions and resource-budget measurements remain open at their source (GSI-INTAKE-SPEC section 10.1; M1-STATUS; QUALITY-002).
- The update origin remains unresolved; track Q-HC-5 in `contracts/product-identity.json` and [UPDATER](../../design/UPDATER.md).

## 4. Open questions

- Q-HC-1 (owner: engineering lead, raised 2026-11-12): how to close the slow-drip residual risk on the intake listener; add a revision-conflict code to the error catalog? (docs/delivery/M1-STATUS.md section 4 item 4)
- Q-HC-2 (owner: governance lead, raised 2026-11-12): should a hero mismatch renew freshness and the sequence? Spec sections 4 and 5 are ambiguous (docs/delivery/M1-STATUS.md section 3.3).
- Q-HC-3 (owner: governance lead, raised 2026-11-13): who owns `commands.rs` and the shared parts of `lib.rs`? (docs/domains/README.md section 1)
- Q-HC-4 (owner: governance lead, raised 2026-11-24): local AGENTS.md says only the display name is decided and technical identity remains unfrozen, while DEC-NAME and the identity contract describe a frozen technical identity (AGENTS.md; contracts/product-identity.json).
- Q-HC-5 (owner: product owner, raised 2026-11-24): product-identity.json leaves the update origin null, while UPDATER.md documents a public endpoint; reconcile the governing update-origin decision before treating either as binding (contracts/product-identity.json; docs/design/UPDATER.md).

## 5. Artefacts

Machine list: [state.json](state.json). Governing: GSI-INTAKE-SPEC, M0-PLAN, M1-STATUS (docs/design, docs/delivery). Reference: DERIVED-STATS, REPLAY-PIPELINE, REPLAY-ENGINE-PROVENANCE, ANALYTICS-HOST-API, LANE-EXPECTATION, DESIGN-IMPLEMENTATION, ALPHA-LAUNCH, UPDATER, RELEASE-UPDATES, AGENT-WORKFLOW section 6, product-identity.json, DECISION-LOG.

## 6. Traps and lessons

- `gsi_setup` writes into the Dota folder; no check or test may click it, and the owner's build may hold the same data directory and port, so start no second app instance (docs/delivery/M1-STATUS.md section 3, 3.2).
- Another application's config claims port 3000; the port is chosen at first setup, persisted, and must stay in 3000-3010 (docs/design/GSI-INTAKE-SPEC.md section 10.1; M1-STATUS 3.2).
- A failed admission must not renew freshness or consume a sequence number; freshness comes from monotonic arrival time taken after the body is complete (docs/delivery/M1-STATUS.md section 3.2).
- Compare Steam library locations by resolved path, not text (docs/delivery/M1-STATUS.md section 3.2).
- Synthetic payloads stay in `crates/kohbor-gsi/tests`; unknown stays unknown, never a default (docs/delivery/M0-PLAN.md section 6; AGENTS.md).
- Set the owned `CARGO_TARGET_DIR` in the same command as every cargo call and read the `Cargo.lock` diff after any dependency change (docs/delivery/AGENT-WORKFLOW.md sections 7.2, 7.4).

## 7. Who may write, how to claim, how to ask

- Open a claim in the coordination ledger (`coord:claims/`, see AGENTS.md) before the first write; the holder updates this card at task end. Workers report what the card needs; they never edit it.
- Ask: this card, then a read-only consultant over the governing documents, then the owner for decisions and native or live runs (docs/domains/README.md section 3).
