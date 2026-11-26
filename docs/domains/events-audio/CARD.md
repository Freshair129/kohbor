---
domain: events-audio
tier: A
card_version: 3
verified_at_commit: a8b636b
verified_on: 2026-11-25
verified_by: "Codex worker; reviewed listed source changes, not human or owner approval"
---

# events-audio: event detection, arbitration, native audio, critical warning path

A map, not evidence. Decisions, requirements, contracts, code and tests outrank this card. If a line here disagrees with them, report the gap.

## 1. Purpose and boundary

- **Owns (write areas):** `crates/kohbor-events` (detector, arbiter, `kohbor-replay` tool), `crates/kohbor-audio` (pure audio owner, readiness, WAV decode), `crates/kohbor-audio-native` (WASAPI sink, Windows only), `crates/kohbor-lab` (dev-only event profile and replay sandbox), and the host critical path in `apps/desktop/src-tauri/src/audio/`, `media/`, `host.rs`, `commands.rs`, `clock.rs`.
- **Must not touch:** `crates/kohbor-pack*` internals (`packs`); `intake.rs` admission, `status.rs`, `settings.rs` (`host-core`); `apps/desktop/src/**` (`ui-overlay`); shared contracts and generated files such as `contracts/event-catalog.json`, `docs/requirements/RTM.csv`, `package-manifest.json` (integration owner, AGENTS.md "Work policy").
- **Interfaces:**
  - `packs`: the host prewarm table is filled from `Store::resolve` (exact tier); the tier fallback rule lives in the host table.
  - `host-core`: detection runs inside the intake accept path; the engine hook takes a message and never waits.
  - `ui-overlay`: playback event, mute command, audio history, probe commands, runtime-status `media` and `events` domains.
  - `banner-lab`: consumes the event catalog; no banner file is shipped or committed.

## 2. Owner decisions (ids only; text in contracts/decisions.json)

- **Decided:** DEC-HEALTH-DANGER, DEC-CRITICAL-MEDIA, DEC-PACK-AUDIO-OPTIONAL-VISUAL, DEC-BANNER-LAB-STYLE (consumer side only), DEC-EVENT-RUNE-SPAWN, DEC-MULTI-PACK, DEC-HERO-PACK-SNIPER-ULT.
- **Proposed, not decided:** DEC-EVENT-AUDIO (owner acknowledgements are recorded inside it; the decision as a whole is not approved).
- **Open:** DEC-INTERFACES, DEC-PACK-CRYPTO, DEC-NATIVE-EVIDENCE.
- **Deferred:** DEC-PACK-BANNER-ADDON.
- Agent decisions in the S3-5 design (P1..P8) have no DEC id: see section 4.

## 3. Now / Next / Blocked

- Event intake, arbitration, host wiring and the `kohbor-lab` profile/replay sandbox map to `apps/desktop/src-tauri/src/intake.rs`, `apps/desktop/src-tauri/src/host.rs`, `crates/kohbor-events`, `crates/kohbor-lab` and [EVENT-AUDIO-SPEC](../../design/EVENT-AUDIO-SPEC.md).
- Objective timing implementation and its patch-versioned table map to `crates/kohbor-events/src/objectives.rs`, `crates/kohbor-events/data/objective-timings.json` and `apps/desktop/src-tauri/src/objectives/` (ALERT-008).
- Critical-media readiness and its authorized-media gate map to DEC-HEALTH-DANGER, DEC-CRITICAL-MEDIA and ALERT-007.
- `first_blood` remains unavailable while E-1 is open (DEC-EVENT-AUDIO; `contracts/event-catalog.json`).
- Special voice-pack events remain `proposed_unavailable` until live captures confirm their detectors (DEC-MULTI-PACK; `contracts/event-catalog.json`).
- Device, acoustic and current-build owner/native acceptance remain pending in [SPRINT-ROADMAP](../../delivery/SPRINT-ROADMAP.md) (DEC-NATIVE-EVIDENCE; QUALITY-001).
- Command names and contract ownership remain open under DEC-INTERFACES.

## 4. Open questions (no DEC id)

- Do the S3-5 section 12 items P1..P8 (mute not persisted, probe failure blocks readiness but not audio, all clips prewarmed, same-tick ordinary event plays when the critical clip is unmapped, probe tolerance, extra `media-src` CSP entry and others) need an owner acknowledgement or a DEC id? Owner: product owner. Raised: 2026-11-12 (S3-5 design date).

## 5. Artefacts (machine list: state.json)

- Spec: `docs/design/EVENT-AUDIO-SPEC.md` (catalog, tiers, arbitration, priority matrix, outcome reasons, formats); `docs/design/S3-5-HOST-CRITICAL-PATH.md` (threads, lanes, prewarm, probe, readiness, tasks 5a..5l, 6a, 6b, S3-7).
- Dev-only event-logic Lab: `docs/design/KOHBOR-LAB.md`, `crates/kohbor-lab`.
- Spike: `docs/delivery/S3-1-AUDIO-SPIKE.md` (WASAPI choice; silence-only observations). Standalone workspace `spikes/audio/`.
- Local event recap and playback outcome contract: [EVENT-AUDIO-SPEC](../../design/EVENT-AUDIO-SPEC.md#local-match-recap); separate from own-match history (MEM-006).
- Backlog: S1-* and S3-* rows in `docs/delivery/SPRINT-ROADMAP.md`.
- Contracts and fixtures: `contracts/event-catalog.json`, `contracts/audio-outcome.schema.json`, `fixtures/health-danger-golden.json` (replayed by `tools/validate_spec.py`).
- Evidence: `docs/delivery/evidence/objective-timers-live/README.md` (owner-observed live run of the map-objective card on integ 9c96013; receipt PASS for its named scenario only).
- Code: `crates/kohbor-events`, `crates/kohbor-audio`, `crates/kohbor-audio-native`, `apps/desktop/src-tauri/src/{audio,media}`.

## 6. Traps and lessons

- Admission code (clock, lanes, resolve, outcome, engine, slot sink) must hold no file, network, lock, blocking call or panic source. Three lexical source guards enforce it (`host.rs`, `audio/lanes.rs`, `audio/tests_engine.rs`); they match text, so a primitive behind another name is not caught. S3-5 section 10.
- Never start the application or open the real data directory in a test: use a fake boxed sink and a temporary store root. The single-instance guard runs before `run()`. S3-5 section 1 "Instance guard".
- Evidence classes stay apart. T3' is the buffer accepted into the sink hand-off, not sound; `latency_probe` and harness numbers never count toward QUALITY-001; native-silence runs prove no acoustics. S3-5 section 10; EVENT-AUDIO-SPEC section 3; S3-1 spike "Not measured".
- `hero.alive` is not a death signal: it went false without a death in recorded sessions. Deaths come from `player.deaths`, streak from `player.kill_streak`. EVENT-AUDIO-SPEC section 2 (E-3, E-5 replay).
- An unconfirmed detector must not silence a confirmed event: unavailable events are removed before ordinary arbitration. A chosen clip that is invalid is `invalid_media`, never replaced by a weaker tier or another event's clip. S3-5 section 2; EVENT-AUDIO-SPEC sections 2 and 7.1.
- Tier lookup is split: the store resolves one exact tier; the host table applies "highest bound tier not above, else tier-less". Do not move the fallback into the store. EVENT-AUDIO-SPEC 7.1; S3-5 section 2.
- The WASAPI sink never reopens after device loss, keeps the old endpoint after a default-device change, blocks in `start()` and joins in `Drop`; open and drop it only on the device supervisor thread. `kohbor-audio-native` compiles on Windows only. S3-5 F5 and section 8.
- Review by reading found a blocker the step-boundary tests could not see (a device may outlive the deadline), which produced the single-slot Sink contract: keep a read-only review before the lead's diff read on every R2 slice. AGENT-WORKFLOW section 7.4; SPRINT-ROADMAP backlog S3-2..S3-3.
- After any dependency edit read `git diff Cargo.lock`: an offline edit once downgraded 11 unrelated packages. One writer per crate: two workers in one crate break each other's build. AGENT-WORKFLOW section 7.4.
- Mute silences C1 to C5 and reports AUDIO_MUTED; a separate keep-safety-audible option would be a new feature and is not proposed. Acoustic acceptance runs unmuted. EVENT-AUDIO-SPEC section 4.

## 7. Who may write, how to claim, how to ask

- **Write:** one session holds the claim for a write area; one writer per crate; workers never edit this card, they report what it needs (docs/domains/README.md section 4; AGENTS.md "Work policy"). No charter exists for this domain, so no delegated approval.
- **Claim:** open a claim in the coordination ledger before the first write and close it at task end (docs/domains/README.md section 4).
- **Ask:** card, then a read-only consultant, then the active claim holder, then the owner (docs/domains/README.md section 3).
- **Owner only:** authorization and rights of any critical clip (DEC-CRITICAL-MEDIA), hardware sessions for device loss and acoustic latency, mute semantics, ranks, any new event, and each objective timing row (`confirmed_by_owner` in the timing table) (DEC-EVENT-RUNE-SPAWN).
