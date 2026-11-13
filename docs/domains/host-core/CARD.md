---
domain: host-core
tier: B
card_version: 1
verified_at_commit: 7ad4c7f
verified_on: 2026-11-13
verified_by: "claude-code subagent draft (read-only), integrated by the governance lead; agent work, not human review"
---

# Host core: game-state intake and the Tauri host shell

A map, not evidence. Authority: decisions, PRD/SRS, contracts, code and tests, then this card.

## 1. Purpose and boundary

- Owns: `crates/kohbor-gsi`, `crates/kohbor-gsi-capture` (dev spike tool), and in `apps/desktop/src-tauri/src` the files intake, containment, settings, status, instance, secrets, atomic, clock, lib, main.
- R2 (lead reads the full diff, read-only review first): the list in docs/delivery/AGENT-WORKFLOW.md section 6, which includes intake, secrets, containment, instance, atomic, tauri.conf.json, capability files and `kohbor-gsi` admission and ownership code.
- Must not touch: `audio/`, `media/`, `host.rs` (`events-audio`); renderer `apps/desktop/src/**` (`ui-overlay`).
- Interfaces: `events-audio` (intake feeds the detector, apps/desktop/src-tauri/src/intake.rs:8); `ui-overlay` (runtime-status, own-snapshot, settings-patch contracts).
- Unassigned or shared: `commands.rs`, and `lib.rs` (which also wires audio, media and the `kohbor-media` scheme) are in no registry row (apps/desktop/src-tauri/src/lib.rs).

## 2. Owner decisions

- Decided: DEC-NAME (technical identity frozen; update origin stays null until UPDATE-001, contracts/product-identity.json).
- Open: DEC-GSI-INTAKE (status proposed), DEC-INTERFACES, DEC-NATIVE-EVIDENCE.

## 3. Now / Next / Blocked

- NOW: the M1 slice exists in source; the only live evidence is one owner match on the pre-hardening build, none on the hardened build (docs/delivery/M1-STATUS.md sections 3.1, 3.2).
- NOW: `capture` is reported unavailable (CAPABILITY_UNAVAILABLE) from a constant, not a probe (apps/desktop/src-tauri/src/status.rs:82).
- NEXT (owner machine): native proof of the containment failure exit code and of cold start with no network (docs/delivery/M1-STATUS.md section 4 item 2; DEC-NATIVE-EVIDENCE).
- NEXT (owner machine): release build of the hardened revision, then a live match incl. A-10, a second launch and the replace action (docs/delivery/M1-STATUS.md section 4 item 1).
- NEXT: capture the remaining assumptions A-6, A-7 (second account), A-8, A-9, A-10 (docs/design/GSI-INTAKE-SPEC.md section 10.1; DEC-GSI-INTAKE).
- BLOCKED: closing DEC-GSI-INTAKE needs those captures plus owner acknowledgement of the mode "unverified" rule (docs/design/GSI-INTAKE-SPEC.md section 10; DEC-GSI-INTAKE).
- BLOCKED: resource budget runs (10 min x 3, idle and live) are not started (docs/delivery/M1-STATUS.md section 3.3; QUALITY-002).
- BLOCKED: updater origin until its service is specified (contracts/product-identity.json; UPDATE-001).

## 4. Open questions

- Q-HC-1 (owner: engineering lead, raised 2026-11-12): how to close the slow-drip residual risk on the intake listener; add a revision-conflict code to the error catalog? (docs/delivery/M1-STATUS.md section 4 item 4)
- Q-HC-2 (owner: governance lead, raised 2026-11-12): should a hero mismatch renew freshness and the sequence? Spec sections 4 and 5 are ambiguous (docs/delivery/M1-STATUS.md section 3.3).
- Q-HC-3 (owner: governance lead, raised 2026-11-13): who owns `commands.rs` and the shared parts of `lib.rs`? (docs/domains/README.md section 1)

## 5. Artefacts

Machine list: [state.json](state.json). Governing: GSI-INTAKE-SPEC, M0-PLAN, M1-STATUS (docs/design, docs/delivery). Reference: AGENT-WORKFLOW section 6, product-identity.json.

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
