# Documentation Changelog

## 0.3.6 — 2026-11-11

M1 has started. The first host slice is in `apps/desktop` (Tauri v2); details in [M1 Status](../delivery/M1-STATUS.md).

- **Pins:** Tauri 2.12.5 (CLI/API 2.12.4), pnpm 12.18.0 through corepack, React 19.3.1, Vite 8.3.5, @vitejs/plugin-react 6.1.2, TypeScript 7.0.2, windows 0.61.3 and jsonschema =0.59.0 (tests only).
- **Supply-chain policy:** pnpm `minimumReleaseAge` is 14 days (`apps/desktop/pnpm-workspace.yaml`). Versions published within the window were downgraded.
- **Containment:** a Job Object with a 10% hard cap, set up before the WebView starts, plus a per-second sampler and soft throttle.
- **Secrets:** DPAPI protects the intake token and the installation key.
- **Owned GSI config:** setup skips ports claimed by other configs and never overwrites a changed file.
- **Intake server:** loopback only, body limit, strict JSON, token check and rate cap.
- **Read model:** `runtime_get_status` and `own_snapshot` commands, and the Command Deck Overview UI.
- **Ran on the development machine:**
  - 33 tests pass and clippy is clean;
  - the release build succeeds;
  - the host and all 6 WebView2 processes were confirmed inside the job, with about 381 MB Working Set.
- **Not run:** live-game, certified-matrix and acceptance runs.
- **RTM:** `implementation_refs` added for CORE-001/002/004/005, DATA-001..004 and QUALITY-002.
- **Kill tiers (owner decision 2026-11-11, same version):** `multi_kill` carries tier 2–5 and `streak` tier 3–10, taken from the event count. A package may bind one clip per tier with `bindings[].tier`; lookup takes the highest bound tier not above the occurrence, and no qualifying binding means silence. Event catalog taxonomy 1.1 (schema 1.1.0) and media-manifest 1.2.0, both additive. Five fixture cases added. No detector or player code exists yet (M2). The owner also directed that counting follows Dota 2 itself: multi-kill is a chain with at most 18 s between consecutive own kills, streak is the game's kill streak, and events of other games are not catalog events. New detector assumptions E-3 and E-4 are unconfirmed.
- **Hardening and UX pass (2026-11-11, same version):** details in [M1 Status §3.2](../delivery/M1-STATUS.md).
  - Fixed: game lookup counted one library twice; a second instance could rewrite the owned configuration; failed admission renewed freshness; the config-conflict state had no replace action; a silent poll failure left old values looking live.
  - Added: single-instance guard, in-process game detection, refusal of web-originated requests (new assumption A-10 in GSI-INTAKE-SPEC), token rotation on replace, flushed atomic writes, epoch recovery for a corrupt runtime state, stricter CSP, and a next-step panel and honest-state chips in the Overview.
  - Ran: 45 tests, clippy, fmt, the frontend build and a synthetic UI check. Not run for this pass: release build, native launch and live game.

## 0.3.5 — 2026-11-11

M0 has started. This release adds the first source code. Application evidence beyond source and synthetic tests remains NOT_RUN.

- **DEC-NAME decided:** the technical identity is frozen in `contracts/product-identity.json`. `app_id` is `io.github.freshair129.kohbor`. Only the update origin remains open.
- **[M0 Plan and Status](../delivery/M0-PLAN.md):**
  - deliverable status;
  - scope matrix;
  - toolchain lock (Rust 1.96.0 pinned; Tauri v2 and pnpm to be pinned at the M1 scaffold; Tauri 3 alpha not adopted);
  - component-to-module map;
  - what ran;
  - the capture-spike procedure.
- **Rust workspace:**
  - `crates/kohbor-gsi`: strict JSON, owned config rendering, admission and ownership, own-snapshot 2.0.0, session continuity. 19 tests pass, including validation of produced snapshots against the contract schema. Clippy is clean.
  - `crates/kohbor-gsi-capture`: an M0 spike tool that installs a spike-only GSI config, captures redacted payloads and summarizes A-1..A-9 and E-1/E-2. It is not run yet.
- **RTM:** `implementation_refs` now point at real code for CORE-004 and DATA-001..004. Status stays NOT_RUN.
- `.gitignore` ignores `spike-out/`.
- **Capture spike, run 2026-11-11 with the owner's approval.** 10,686 payloads from one bot match; the spike config was uninstalled afterwards. Results are in GSI-INTAKE-SPEC §10.1:
  - Confirmed: A-1, A-2, A-3 (max gap 1,250 ms), A-5, A-7 for one account and E-2.
  - Consistent but not isolated: A-4 and A-8.
  - Partly: A-9 and E-1.
  - Not tested: A-6.
- **Fixes from the spike:**
  - Two extra session states, TEAM_SHOWCASE and WAIT_FOR_MAP_TO_LOAD.
  - An empty hero name during selection now counts as missing.
  - Port choice now skips ports claimed by other applications' GSI configs (another app claimed 3000).
  - Five captured-shape replay tests. Tests total 25, all passing.

## 0.3.4 — 2026-11-11

Added the first visual design proposal under DEC-VISUALS. The product owner approved its **direction** on 2026-11-11: mark, palette, type, state encodings, layout direction and the working name "Command Deck". Measured values, remaining screens and native/visual acceptance are not approved, so DEC-VISUALS stays open.

- [VISUAL-DESIGN-PROPOSAL](../design/VISUAL-DESIGN-PROPOSAL.md) records:
  - an original companion mark;
  - a dark-first palette with one teal accent, blue/orange team colours, critical/warning/stale tokens and no "safe" green;
  - IBM Plex Sans Thai and IBM Plex Mono type;
  - honest-state encodings that use shape, line and words, not colour alone.
- Four annotated artboards, sourced in `docs/design/visuals/`:
  - design tokens;
  - Command Deck Overview (waiting state with independent readiness rows);
  - Command Deck Live (sample data: field anatomy, neutral inspector, detection-versus-audio event feed);
  - the 1920×1080 overlay with no-cover HUD zones and five modules.
- `contracts/design-tokens.json` 0.1.0. Values marked `needs_measurement` must be confirmed natively.
- UI-001/002/003/004/006 link the proposal and the tokens.
- **Owner decisions (2026-11-11):**
  - **DEC-SESSION-MONITOR** turns the Command Deck Live destination into a second-screen session monitor. The enlarged minimap dominates for peripheral threat awareness, flanked by own-team and enemy hero cards.
  - **UI-002** changes from the inherited top/bottom/right topology to this layout. Its ID is kept and its basis is now `owner_decision`.
  - **New UI-008** covers peripheral threat salience: the critical map frame, single sighting marks, the stale dimming and no routine flashing.
  - **DEC-PEER-ABILITIES** adds **PEER-010**: teammate ability levels and cooldowns, behind a separate opt-in `abilities` scope. It has its own closed DTO, `contracts/peer-abilities.schema.json`, with three fixtures.
  - The registry grows from 85 to 87 requirements. PN-005 and PN-009 are updated.
  - The canvas Live board was redrawn as the Session Monitor.

Not changed:
- Requirement statements, acceptance predicates, scopes and NOT_RUN evidence.
- UX-SPEC rules.
- Runtime schemas.
- Diagrams.

## 0.3.3 — 2026-11-11

Closed the documentation gaps blocking M2 (events, audio and media). Every addition is a **proposal** under DEC-EVENT-AUDIO. The single new product behavior, health danger, was approved as proposed by the product owner on 2026-11-11 (DEC-HEALTH-DANGER). Nothing else has been approved, and nothing has been executed.

- Added [EVENT-AUDIO-SPEC](../design/EVENT-AUDIO-SPEC.md):
  - **Canonical event catalog:** 17 production event IDs plus a test-only `latency_probe`, with ranks, tunables, detectors, milestones and th/en labels. Baselines and resumes never replay achievements. The product owner confirmed the ordinary ranks not fixed by MP-02 on 2026-11-11.
  - **Health danger:** approved as proposed on 2026-11-11 and pinned by six golden traces (`fixtures/health-danger-golden.json`, replayed by the validator). It is audible only once verified critical media exists.
  - **Audio priority:** five classes (critical, correction, notice, ordinary, voice) with an incoming-versus-active preemption matrix.
  - **Prewarm and critical readiness rules.**
  - **Mute semantics:** acknowledged by the product owner on 2026-11-11. Global mute silences critical audio too, and safety audio then reports not ready.
  - **14 audio outcome codes.**
  - **Media profiles:** WAV (mono 48 kHz PCM16) and WebM (VP9/VP8, one video track, no audio track), plus a renderer decode probe.
  - **Packaging:** the descriptor/registry layout, installer-bound authority (only `local_import` until DEC-PACK-CRYPTO closes) and install contracts.
- Contracts:
  - **New:** `event-catalog.json` with its schema, and `audio-outcome`, `media-install-request` and `media-install-result` (all 1.0.0).
  - **`event.schema` 2.0.0:** renamed `instance_id`, added `session_id` and `taxonomy_version`, bound `event_type` to the catalog, and added `count`/`level`/`value_percent` to the payload.
  - **`media-manifest` 1.1.0:** added an optional `identity_audio_path`.
  - **Error catalog 1.2.0:** added `AUDIO_MUTED` and `MEDIA_PROBE_FAILED`.
- Fixtures: 17 new cases (57 total). The validator now:
  - checks catalog semantics: unique IDs and ranks, tunable bounds, re-arm side;
  - checks that the `event.schema` enum equals the catalog;
  - checks binding file roles.
- Registry: MEDIA-001..008, ALERT-001/006/007, VOICE-006 and QUALITY-001/007 now link the new spec and contracts. MEDIA-001/002/004/005, VOICE-006 and QUALITY-007 depend on DEC-EVENT-AUDIO. ALERT-007 keeps its DEC-CRITICAL-MEDIA dependency.
- The DGM-010 guard note now states that the installer, not the package, binds authority. The figure itself is unchanged.

Not changed:
- Requirement statements, failure behavior, acceptance predicates, scopes, priorities and NOT_RUN evidence.
- The 19 diagram sources and SVGs.

## 0.3.2 — 2026-11-11

Closed the documentation gaps that blocked starting M1. A readiness review found that game-state intake, ownership, session start, mode visibility, the per-field read model, runtime status/settings contracts and requirement-to-milestone mapping were missing or contradictory. Every addition is a **proposal** under the new DEC-GSI-INTAKE record. Nothing has been approved or executed.

- Added [GSI-INTAKE-SPEC](../design/GSI-INTAKE-SPEC.md). It defines:
  - the owned game configuration: location, ownership hash, contents, 1.0 s heartbeat and port persistence/restart behavior;
  - the loopback intake bounds: token authentication, a 131,072-byte body limit and a 50 requests/s cap;
  - field-level admission;
  - ownership verification, combining token authenticity with a cross-check against the platform identity;
  - session-start evidence and restart continuity, using keyed hashes and a strictly increasing epoch;
  - mode visibility ("unverified" unless the source exposes the mode);
  - the monotonic freshness clock.
  Nine assumptions about the game integration feature must be confirmed by an M0 capture spike.
- Changed own-snapshot.schema.json from 1.0.0 to 2.0.0. Each vital now carries its own `{value, availability}`, so AC-DATA-002 ("missing HP is unknown, real zero stays zero") can be expressed. The schema also encodes the fresh ≤2000 / stale ≥2001 ms boundary and keeps fields from being fresher than their snapshot. All six existing fixtures were migrated and six new cases added.
- Added runtime-status.schema.json, the `runtime.get_status` response. Its health domains are independent: intake, ownership, session, mode, resource, media, capture and account.
- Added user-settings.schema.json and settings-patch.schema.json, both 1.0.0, covering M1 fields only. Their 10 fixture cases are included.
- Error catalog 1.1.0:
  - nine GSI error codes;
  - a `message_key` localization key on every code.
- SDD §14: proposed Job Object flags, membership verification, per-second sampling, soft-throttle entry/exit (enter at ≥9.0%, exit at three samples ≤8.0%) and intra-cap thread priority. The 10% / 900,000,000-byte limits and the 8% hysteresis are unchanged.
- Requirement registry format 1.3.0: every requirement has a `target_milestone`, rendered into the SRS and a new RTM column. There are nine M1 requirements. CORE-004, CORE-005 and DATA-001/003/004 now depend on DEC-GSI-INTAKE and link to the new spec and contracts.
- Added the `gsi_config_slug` technical-identity field. It is null until DEC-NAME closes.
- Bumped the document version bindings, the evidence schema and the integrity manifest to 0.3.2. The 19 diagram sources, SVGs and atlas are unchanged and keep their 0.3.1 labels. No diagram contradicts the new text at its level of abstraction.

Not changed:
- Requirement statements, failure behavior, acceptance predicates, scopes, priorities and NOT_RUN evidence.

Review follow-ups also fixed in 0.3.2:
- Restored the spaces lost around numbers and comparison operators in 37 registry fields (for example "code1" → "code 1", "be≤10%" → "be ≤10%") and in SDD/FEATURE-CATALOG prose. The SRS section, RTM and Gherkin were regenerated. Numbers and units are unchanged.
- FEATURE-CATALOG now lists the shared base live canvas as MVP, matching PRD and UI-002. Premium details stay deferred.
- The Decision Log reconciles CORE-003 (MVP) with extension-scope accounts: it is a guard on builds that contain an account surface and adds no account feature. Its scope is unchanged.

Still open for M2: a canonical event catalog, a "health danger" detector definition and an audio priority matrix.

## 0.3.1 — 2026-11-10

Prepared the first repository baseline for **Kohbor / เกาะเบาะ** after the owner selected the name and supplied the destination repository.

- Recorded the selected display name in the identity manifest and agent handoff. Technical identifiers remain delegated and explicitly unresolved; the name selection does not close all of DEC-NAME.
- Added an implementation-readiness summary with concrete interface, data, media, risk, voice, visual, model and validation gaps.
- Clarified two use-case summaries against the existing governing design: audible correction requires admitted reappearance and the revision threshold; room joining and non-null own-data publishing have distinct prerequisites.
- Preserved all 85 requirement IDs, behavior and acceptance predicates, the 19 diagram sources/SVGs, and NOT_RUN application evidence. Updated document version bindings and regenerated derived views and the integrity manifest.
- Added repository ignore and line-ending rules for reproducible document bytes and local dependency hygiene.

This publication does not claim a completed implementation, frozen production interfaces, approved visual design, native/live acceptance or application release approval.

## 0.3.0 — 2026-11-10

Added a controlled diagram catalog and offline atlas with 19 editable Mermaid figures and generated SVGs. Replaced five embedded diagram sources with references to the controlled figures. Added context/deployment/data-flow views, critical-work sequences, source/voice states and user/release flows; linked diagrams to their governing documents, SRS IDs, open decisions and RTM rows. Added pinned rendering tools and source/output consistency checks. Existing requirement behavior, acceptance thresholds, scopes and NOT_RUN evidence remain unchanged. Diagram rendering does not certify application execution or close unresolved design decisions.

## 0.2.0 — 2026-11-10

Reorganized the package around conventional software engineering document responsibilities after review of its agent-oriented presentation.

- Added a substantive PRD with product problem, stakeholders, goals, scope, outcomes and 12 stable product needs.
- Added full SRS context and retained every existing atomic requirement in one readable SRS. The 85 IDs, obligations, failure behavior, scope, acceptance predicates and application evidence statuses remain unchanged.
- Made PRD/SRS the human requirements baseline. The JSON registry is its structured representation; generation is an editing mechanism, not a competing semantic authority.
- Established SAD, SDD, DDD, ICD and UX document roles; added architectural views, design component allocation and explicit design maturity.
- Added Test Plan, UAT Plan and a risk register. Preserved the existing NFR targets, scenarios and structural/semantic fixtures.
- Moved traceability to `docs/requirements/RTM.csv` and added product needs, design components, procedures and empty implementation/evidence references.
- Retained agent workflow instructions separately from product requirements. Updated read order, relative links and generation/validation tools.
- Advanced the structured requirement format to 1.1.0 for additive traceability fields; bound evidence templates/schema to spec version 0.2.0. Runtime schema definitions remain unchanged except document paths/descriptions.

This is a documentation revision. It adds no product approval, completed protocol decision, application implementation, native/live test receipt, certification or distribution authorization.

## 0.1.0 — 2026-11-10

Initial brand-neutral extracted draft with product scope, 85 requirement records, behavior/design notes, contracts, test designs, fixtures, delivery plan and agent handoff. Document roles and upstream product traceability were insufficiently explicit; version 0.2.0 addresses those gaps.
