---
title: "Software Verification Test Plan"
doc_id: "TEST-PLAN"
status: "draft"
version: "0.3.4"
updated: "2026-11-11"
owner_role: "Verification lead — unassigned"
approval_status: "not_approved"
---

# Software Verification Test Plan

## 1. Scope and present evidence

Verify the [SRS](../requirements/SRS.md), its 85 registered requirements, and [PRD needs](../product/PRD.md). The [NFR](NFR.md) owns measurement definitions and unchanged thresholds; [UAT](UAT-PLAN.md) records player acceptance. Link procedures and results through the [RTM](../requirements/RTM.csv).

Highest risks are false reassurance, incorrect event audio, unconsented disclosure, stale asynchronous work, game interference, and corrupted installation/persistence. Cover full MVP separately from peer/social extensions and deferred premium capabilities. A limited demonstration does not certify full MVP.

Included [fixtures](../../fixtures/index.json) test selected structural/semantic contracts and arithmetic. The supplied [Gherkin scenarios](../../acceptance/scenarios.feature) are test designs requiring application bindings. Neither proves application integration, native behavior or live performance. All new application results start **NOT_RUN**.

## 2. Responsibilities and environments

| Unassigned role | Responsibility |
| --- | --- |
| Verification lead | Coverage, environment manifest, evidence and defect triage |
| Engineering owner | Reproducible candidate, instrumentation, automated bindings and fixes |
| Privacy/security reviewer | Outbound projections, authority and failure-path review |
| Native/UAT operator | Controlled Windows/device/live tasks and raw observations |
| Product/release owner | Scope decisions and explicit acceptance; no approval recorded here |

Use unit/state-machine tests; schema/golden tests; real socket/provider integration; browser interaction tests; installed Windows tests; and live game/acoustic/resource tests. Record exact source/artifact/dependency/model hashes, OS/build, hardware, audio device, patch and configuration. Initial certification targets Windows x64 builds 19045/26100, live All Pick Ranked/Unranked, 1920×1080 primary display, 100% DPI, borderless fullscreen/default minimap. Native injected-input evidence is distinct from live-match evidence. Use isolated test profiles, authorized synthetic media and test accounts; preserve user files.

## 3. Entry and exit

Before a procedure: identify requirements, candidate, expected trace, test-data reset, instrumentation and evidence destination. Close its blocking [decisions](../governance/DECISION-LOG.md), including interfaces, motion admission, media trust, models, session eligibility or retention where applicable. Missing prerequisites block that procedure; independent work continues.

A slice exits when its required methods pass, contradictory results are resolved, and limitations are recorded. Release additionally requires all mandatory selected-scope NFR/native/live/privacy/update evidence and explicit acceptance. An unavailable state can pass a failure-behavior test without satisfying the missing capability's success gate.

## 4. Verification procedures

**TP-01 — Startup and platform.** CORE-001–005. Launch installed guest/offline profiles; exercise account switch, resource-containment setup failure, occupied ports and unsupported configurations. Confirm failure occurs before UI when containment fails, valid local functions remain available, and setup preserves unrelated configuration. Retain startup/process receipts and file comparisons.

**TP-02 — Data and vision admission.** DATA-001–006, VISION-001–003. Replay admitted, malformed, wrong-owner/team and unsupported inputs; test age 2,000/2,001 ms, regressed clock, genuine new session and late responses. Interrupt vision independently. Assert invalid input never refreshes freshness, unknown differs from zero, stale generations are discarded, and valid telemetry survives vision loss. Retain source/read-model traces.

**TP-03 — Audible-alert lifecycle.** ALERT-001–007. Compare golden arithmetic, then drive complete admitted motion traces. Test high score without coordination, continuing absence beyond cooldown, confirmed reappearance, time-decay-only score reduction and coverage loss. Verify 30-second global/60-second contributor fences and separate >5-second missing notification with ≥10-second audio cooldown. Require mapped critical media for the positive path. Missing media must report not-ready. Confirm no substituted audio, all-clear from decay, or replay after cancellation.

**TP-04 — Media and installation.** MEDIA-001–008. Validate catalog duplicates/ranks; install valid, oversized, traversal, duplicate-record, bad-hash and corrupt-media packages. Test highest-event mapping absent, detected respawn, hostile publisher claim, repeated install request and activation during a match/lease. Assert silence without lower-rank fallback, unchanged healthy selection on rejection, independent installed/deferred status, and synchronized audio/visual cancellation. Compare registry and payload digests before/after failures. Catalog, priority-matrix, outcome-code and decode-probe cases follow [EVENT-AUDIO-SPEC](../design/EVENT-AUDIO-SPEC.md); `latency_probe` measurements are engineering evidence only and never satisfy QUALITY-001 acceptance.

**TP-05 — UI and focus.** UI-001–006. Exercise neutral startup, unknown cards, explicit selection, changing hero/session, keyboard/Escape, overlay positioning and multiple control viewports. Verify map topology, visible focus, truthful fields and discarded late inspector responses. Confirm actual Windows click-through, DPI and focus independently of browser screenshots.

**TP-06 — Voice.** VOICE-001–007, QUALITY-006–007. Test microphone default-off, hotkey collision/remap, 249/250 ms hold, 15-second cap, device/open timeout, missing model and new-turn supersession. Interrupt every active voice stage. Verify bounded transient buffers, release and no stale resumption. Run NFR language/latency measurements; fixture scoring is not recognition evidence.

**TP-07 — Memory.** MEM-001–005. Hand-calculate complete/partial-session aggregates; interrupt snapshot replacement; forget, restart and corrupt the cutoff. Assert no recovered pre-cutoff memory, at most three supported summary points, and byte-identical retained journals. Run NFR query/storage measurements after eligibility and byte-unit decisions close.

**TP-08 — Advice and privacy.** ADVICE-001–004; cross-check VOICE-005, MEM-001 and PEER-009. Capture requests at controlled API/CLI/relay boundaries with separate consents off/on. Inject forbidden voice, memory, enemy and identifier fields; withdraw consent during pending responses/retries. Assert closed projections, no forbidden egress and no late display/cache use. Retain sanitized field evidence including errors/cancellation; exclude secrets and microphone content.

**TP-09 — Peer extension.** PEER-001–009. On two controlled devices, test unchecked consent, mismatched scope, sequence replay, frozen UI, timeout/null-probe recovery, leave-during-join, owner expiry and invitation rotation versus kick. Inspect persistence. Assert immediate withdrawal, bounded workers, conservative freshness, no token exposure and no saved peer stats.

**TP-10 — Social extension.** SOCIAL-001–003. With reviewed service contracts/test accounts, exercise mutual friendship, presence, invitation redemption/replay/expiry and account revocation. Inspect transaction receipts. Require separate fresh sharing consent; reject unauthorized joins without guest fallback or unintended peer disclosure.

**TP-11 — Recording.** OBS-001–005, VISION-004. Explicitly start authenticated loopback OBS; request clips with sufficient and insufficient history; interrupt auth, process and disk availability. Check actual save/trim timestamps against 30-second pre-roll/15-second post-roll, partial labels and real files. Report app, OBS and combined resources separately; recording retains acoustic/FPS gates. A combined-profile pass awaits the recorded budget decision.

**TP-12 — Update and capability authority.** UPDATE-001–004; PREMIUM-001–003 and UI-007 only when scheduled. Reject invalid signatures, defer unsafe installation, simulate interruption and verify approved recovery with migration compatibility. Compare candidate/stable bytes and signatures. For premium, test valid-offline lease, expiry/revocation and late detail responses; base safety/map persist without fabricated authority. Use test keys/services; this plan does not authorize publication.

**TP-13 — Quality budgets and certification.** CORE-002, QUALITY-001–008. Verify actual child-process containment. Measure T3−T0 from acquisition due/input arrival to loopback first audio: ≥300 samples/scenario/profile, p50≤250 ms, every sample≤300 ms; all 100 overlap trials must pass. Warm up two minutes; measure each NFR phase ten minutes × three runs at one-second sampling. Enforce CPU≤10%, full Working Set≤900,000,000 bytes, and ≤3% FPS loss per three matched ≥60-second pairs. Apply per-class vision, warning, language and ≥10-live-session-per-cell protocols without changing denominators.

## 5. Results, defects and reruns

Record NOT_RUN, PASS, FAIL, SKIP or INDETERMINATE with expected/actual results and local raw-evidence references. SKIP, missing hardware and empty samples cannot satisfy mandatory gates. Retain timeouts and failures; never average away a failed maximum or pair.

Defects record reproduction, requirement/procedure IDs, exact build, impact, evidence and owner role. Privacy/authority violations, false-safe behavior, destructive corruption and required-gate failures block the affected acceptance claim. Reproduce the failure, fix it, rerun the impacted procedure and justified regressions; preserve prior receipts. Only a recorded scope/requirement decision may change acceptance. No signatures or approvals are supplied by this draft.
