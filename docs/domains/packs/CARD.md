---
domain: packs
tier: A
card_version: 1
verified_at_commit: 7ad4c7f
verified_on: 2026-11-13
verified_by: "claude-code subagent draft (read-only), integrated by the governance lead; agent work, not human review"
---

# Domain card: packs (verified package store and authoring tool)

A map, not evidence. Decisions: `contracts/decisions.json`. Requirements: `contracts/requirements.json`. Statuses, hashes and backlog rows are not copied here.

## 1. Purpose and boundary

- **Owns.** `crates/kohbor-pack` (descriptor, container reader, media profile checks, trust assessment, `Store`: install, activation, crash recovery, resolve). `crates/kohbor-pack-author` (tool `kohbor-pack-build`). `contracts/media-manifest.schema.json`, `contracts/media-install-request.schema.json`, `contracts/media-install-result.schema.json` and the `fixtures/media-*.json` files with their rows in `fixtures/index.json`.
- **Governing documents.** `docs/design/MEDIA-PACK-SPEC.md` (MP-03, MP-06, MP-07, MP-09), `docs/design/PACKAGE-FORMAT-REVIEW.md`, `docs/design/EVENT-AUDIO-SPEC.md` sections 6, 7 and 7.1, Sprint 2 and S3-8 in `docs/delivery/SPRINT-ROADMAP.md`. Requirements: MEDIA-004, MEDIA-006, MEDIA-007, MEDIA-008.
- **Must not touch.** The event catalog and detectors, arbitration, native audio, and the host critical path (`events-audio`). Renderer pack panels (`ui-overlay`). Pack audio and banner sources: they live outside the repository under `packs-src:` and `lab:` and are never described by path here.
- **Interfaces.**
  - `events-audio`: consumes `Store::resolve`, `active`, `records` from `apps/desktop/src-tauri/src/media/` and `host.rs`; it owns the tier rule applied on top of the exact lookup.
  - `banner-lab`: hands a clip folder and a mapping file to `kohbor-pack-build`; output stays outside the repository.
  - `commerce`: later trust domains and entitlement. Integrity is not purchase entitlement (MEDIA-008). Nothing is designed.
  - `ui-overlay`: the pack manager panel in `apps/desktop/src/packs.ts` calls the host commands `media_inspect`, `media_install`, `media_activate`.
  - `governance`: integration owner of shared contracts, `contracts/product-identity.json` and the decision records.

## 2. Owner decisions (ids only; text in contracts/decisions.json)

- **Decided:** DEC-PACK-AUDIO-OPTIONAL-VISUAL, DEC-COMMERCE-INTENT (intent only, authorises no design).
- **Open or proposed:** DEC-PACK-CRYPTO, DEC-EVENT-AUDIO (install contracts are in its spec), DEC-CRITICAL-MEDIA, DEC-INTERFACES (transport framing of the install request), DEC-NATIVE-EVIDENCE.
- **Deferred:** DEC-PACK-BANNER-ADDON, DEC-PREMIUM.
- **Neighbour, decided:** DEC-BANNER-LAB-STYLE (banner format that a pack must be able to carry).
- No record exists for the package-format decision of `docs/design/PACKAGE-FORMAT-REVIEW.md` (see section 4).

## 3. Now / Next / Blocked

- Now: descriptor, container, media, trust, store and resolve modules exist with 12 test files; tests were NOT run for this card, so pass state at this commit is UNVERIFIED (`crates/kohbor-pack/tests`).
- Now: the parser requires `schema_version` "1.3.0" and accepts audio-only bindings (`crates/kohbor-pack/src/descriptor.rs`, DEC-PACK-AUDIO-OPTIONAL-VISUAL).
- Now: signature status is always `Unavailable`; the only authority granted is `local_import`; a publisher claim is recorded as unverified (`crates/kohbor-pack/src/trust.rs`, DEC-PACK-CRYPTO).
- Now: the host uses `Store` in `media/prewarm.rs` and `host.rs`; `kohbor-pack-author` is a dev-dependency of the host only (`apps/desktop/src-tauri/Cargo.toml`).
- Next: the tool still writes `<package_id>-r<revision>.zip`, while the review's chosen option wants a product-owned extension that is not yet in the identity manifest (`crates/kohbor-pack-author/src/lib.rs`).
- Next: replace the "API STUB, every body is todo!()" crate header and review the blanket `allow(unused_variables, dead_code)` (`crates/kohbor-pack/src/lib.rs`).
- Next: the "repacked file" rejection wording and the Open-dialog filter of the review were not found in `apps/desktop/src`; UNVERIFIED whether planned (`docs/design/PACKAGE-FORMAT-REVIEW.md` section 11).
- Next: check that a banner-lab VP9 clip with alpha at 1280x480 passes native inspection and the renderer probe; UNVERIFIED (DEC-BANNER-LAB-STYLE).
- Blocked: signed publisher or store authority and trust roots wait for the signature decision (DEC-PACK-CRYPTO).
- Blocked: a voice package plus a banner from another package, and any `extends` field, are not built (DEC-PACK-BANNER-ADDON).
- Blocked: audible critical alerts need authorised, verified critical clips; rights are an owner statement, unverified, handled by the owner's rights team (DEC-CRITICAL-MEDIA).
- Blocked: store, wallet, paid packs and encryption options have no design (DEC-COMMERCE-INTENT, DEC-PREMIUM).

## 4. Open questions (no DEC id)

- Record the package-format decision (review option A2) in `contracts/decisions.json` and the Decision Log, and confirm the extension spelling (proposal `.kohborpack`; it must not be `.gann`). Owner, raised 2026-11-12 in `docs/design/PACKAGE-FORMAT-REVIEW.md` sections 9 and 11. DEC-PACK-CRYPTO covers signing, not the extension. Task 1 of that section is not done: no decision record for the package-format choice exists yet.
- Is an in-app import of a clip folder (the review's option D) in scope, and for which milestone? Owner, raised 2026-11-12 (`docs/design/PACKAGE-FORMAT-REVIEW.md` section 11).

## 5. Artefacts

Machine list with `reviewed_sha256`: `state.json`. Key paths:

- Code: `crates/kohbor-pack/src` (`container.rs` strict ZIP reader, `store.rs` transactions, `descriptor.rs`, `media.rs`, `trust.rs`, `limits.rs`, `catalog.rs`), `crates/kohbor-pack-author/src` (`lib.rs`, `archive.rs`, `audio.rs`).
- Tests: `crates/kohbor-pack/tests` (hostile, crash, install, activation, resolve, trust, audio_only, windows_fs), `crates/kohbor-pack-author/tests`.
- Contracts: the three media schemas and `fixtures/media-*.json`.
- Design for the host side of install: `docs/design/S3-5-HOST-CRITICAL-PATH.md` (owned by `events-audio`).

## 6. Traps and lessons

- Install and activation are separate calls: `Store::install` returns only `installed` or `rejected`; a retry never re-runs activation (`docs/design/EVENT-AUDIO-SPEC.md` section 7.1, MEDIA-007).
- Ordinary ZIP tools cannot produce an acceptable package: the reader is deliberately narrow, so a re-zipped pack fails with a reason that means nothing to a player (`docs/design/PACKAGE-FORMAT-REVIEW.md` section 4).
- `Store::resolve` is an exact tier lookup. The "highest bound tier not above the occurrence" rule lives in the host prewarm table, not here (`docs/design/EVENT-AUDIO-SPEC.md` section 7.1).
- The store assumes one writer (the host's single-instance guard); an authoring tool must never write into a live store (`docs/design/EVENT-AUDIO-SPEC.md` section 7.1).
- Crash tests inject only at step boundaries; power loss and storage-cache failure are not demonstrated and need native evidence (`crates/kohbor-pack/src/store.rs` header, DEC-NATIVE-EVIDENCE).
- Identity: `container_sha256` is the transport and idempotency check; `descriptor_sha256` is the content identity. Request results are kept for the last 512 requests, then treated as new (`docs/design/PACKAGE-FORMAT-REVIEW.md` section 9, `docs/design/EVENT-AUDIO-SPEC.md` section 7.1).
- Local integrity never grants publisher or purchase authority, whatever the descriptor says, including package ids such as `store` or `system` (`crates/kohbor-pack/src/trust.rs`, MEDIA-008).
- Native WebM parsing checks declared size, tracks and duration only; real decodability comes from the renderer probe, which is a separate evidence class (`docs/design/EVENT-AUDIO-SPEC.md` sections 6 and 7.1).
- The crate does no file I/O for the event catalog: the caller loads `contracts/event-catalog.json`, so a stale catalog copy changes which IDs are inert (`crates/kohbor-pack/src/catalog.rs`, MEDIA-001).
- A document can lag the code: the review still says every binding needs a video and cites manifest 1.2.0, while 1.3.0 made the video optional; check the decision record first (`docs/design/PACKAGE-FORMAT-REVIEW.md`, DEC-PACK-AUDIO-OPTIONAL-VISUAL).

## 7. Who may write, how to claim, how to ask

- **Write.** One claim holder per write area in section 1; shared contracts, `product-identity.json`, generated files and the decision records go through the governance integration owner. Workers report what this card needs; they never edit it (`docs/domains/README.md` section 4).
- **Claim.** One file per claim in the coordination ledger outside the repository, written before the first write and closed by the lead (`docs/domains/README.md` section 4).
- **Ask.** Read this card and `state.json`; then a read-only consultant primed with the governing documents; message the claim holder only for in-flight facts; ask the owner for decisions, rights and taste (`docs/domains/README.md` section 3). Rights to pack audio are never decided by an agent.
- **Evidence classes** stay separate: source, synthetic, native, live, acoustic. A synthetic pack test is not native or acoustic evidence (`AGENTS.md`, MEDIA-PACK-SPEC MP-10).
