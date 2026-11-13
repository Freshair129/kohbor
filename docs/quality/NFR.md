---
title: "Nonfunctional Requirements and Measurement Specification"
doc_id: "NFR"
status: "draft"
version: "0.3.6"
updated: "2026-11-11"
owner: "Product Owner"
approval_status: "not_approved"
---

# Nonfunctional Requirements and Measurement Specification

## Meaning of acceptance

These are rebuild targets; no native, live-game, acoustic, language-quality or resource pass is claimed. Specifications, source, fixtures, screenshots and production receipts establish different things.

Record PASS, FAIL, SKIP or INDETERMINATE. Missing prerequisites, untested configurations and empty samples cannot pass. Retain failures/timeouts. Threshold changes require a versioned decision and affected reruns.

## Certified starting matrix

| Dimension | Initial target |
|---|---|
| Operating system | Windows x64: Windows 10 build 19045; Windows 11 build 26100 |
| Game modes | Live 5v5 All Pick, Ranked and Unranked |
| Vision geometry | 1920×1080, 16:9, primary display, 100% DPI |
| Game presentation | Borderless fullscreen, default HUD, minimap bottom-left |
| Reference hardware tier | Six physical cores / twelve logical processors, 16 GB RAM, compatible desktop-capture GPU, display at least 60 Hz |
| Voice languages | Thai and English, including separately reported code-switch cases |
| Account/connectivity | Guest, signed in, cold offline, network loss, sign-out/revocation/account switch |

Record CPU/GPU SKU, memory, driver, OS, audio device/backend, display settings, game patch, model/labels hashes and build identity.

At least ten live sessions are required per required OS/mode/geometry cell. Enumerate the cells before testing. Unsupported custom/Turbo/spectator/replay modes, other resolutions/DPI/layouts and untested multimonitor capture remain explicit unsupported or unverified. A second-screen control UI does not imply certified capture on arbitrary displays. GSI-only operation remains useful while clearly showing unavailable vision coverage.

## Performance budgets

| Metric | Required limit | Measurement boundary |
|---|---|---|
| Critical alert latency | p50 ≤250 ms and every measured sample ≤300 ms | Acquisition/game-input arrival through first audible output |
| CPU | Every one-second sample ≤10% | Application process group and attributable active model/local app-relay processes, normalized by logical CPU capacity |
| RAM | Peak aggregate full Working Set measured and reported; no limit until the owner sets one from the first baseline | Application host, renderer, helpers, active inference/model processes and local app relay; external OBS reported separately |
| GPU memory (VRAM) | When the application uses the GPU (allowed), peak dedicated GPU memory of the same process set measured and reported; no limit until the owner sets one from the first baseline | Measured per process with a stated method and raw receipts; native and live evidence; unmeasured means unknown, never within budget (DEC-RESOURCE-BUDGET) |
| Game FPS impact | ≤3% loss in each paired comparison | Same device, game segment and settings |
| Valid game-data freshness | Age ≤2,000 ms | Valid source observation; invalid input never refreshes age |
| Warm voice response | Every accepted ready/warm turn ≤2,000 ms | Talk-key release through first audible answer |
| Cold model preparation | ≤10 seconds | Visible preparing state; separate from warm-turn result |
| Loaded memory query | ≤5 ms | Query after a valid derived snapshot is loaded |
| Memory storage | ≤5 MB per 1,000 contributing matches | Bounded derived memory; define exact byte unit in storage ADR before measurement |

Report the RAM measurement in decimal bytes. No RAM or VRAM limit applies yet: the owner sets it from the first measured baseline of the idle, live and voice-alert profiles (DEC-RESOURCE-BUDGET); a value that was not measured is unknown, never zero or within budget. Do not substitute private bytes, proportional set size or host-only usage. Sum full Working Set for the declared process set, including shared resident pages according to that method. Include external STT/model processes actually serving the application and a relay if it runs on the gaming computer. Do not trim Working Set, trigger special garbage collection or omit child processes merely to pass.

External OBS is not included in the application's CPU/RAM ceiling and is not presumed contained by its process job. During recording, report application, OBS and combined CPU/RAM as distinct measurements. This draft defines no combined ceiling; a recorded budget decision is required before an overall recording-profile performance pass can be claimed. An application-only pass must retain that scope. FPS impact and critical-alert latency keep their existing limits with recording active; recording cannot be omitted from those runs when that capability is being accepted.

The CPU hard cap and acceptance ceiling are **10%**. Configure process containment and cap before starting the UI or helpers. If job creation, configuration or assignment fails, log a sanitized error and exit with code 1 before those processes start. An accounting-query failure after successful setup shows unavailable CPU, retains the established OS cap and keeps capture throttled. The throttle entry/exit rules, Job Object flags and membership verification are proposed in [SDD §14](../design/SDD.md).

The recovery threshold of **8% for three consecutive one-second samples** is soft throttle hysteresis, not a different hard-cap value. Verify actual renderer/helper membership and crash cleanup; intended inheritance is not evidence of containment. Unknown external model ownership or unaccounted resource use makes that provider unavailable for acceptance.

## Critical alert timing and preemption

Define landmarks before instrumentation:

| Landmark | Meaning |
|---|---|
| T0 | Fresh game POST arrival, or time a critical acquisition cycle becomes due before scheduler/acquisition/inference wait |
| T1 | Deterministic native warning decision |
| T2 | Audio admission |
| T3 | First audible sample measured by audio loopback |

Report T3−T0. Never move T0 after frame acquisition or polling to hide waiting time. Report upstream observation age separately. Record at least 300 samples per scenario/profile and include a live audible probe. p95/p99 supplement the hard maximum; they cannot replace it. Event-driven evaluation is required where needed to meet the budget; a 500 ms watchdog does not demonstrate 300 ms acquisition-to-audio latency.

Run 100 overlap trials with ordinary voice/advice active. Every critical warning must cancel obsolete output and meet latency. Cover capture, decoding, answering and speaking; late completion cannot resume cancelled work. Audio unavailable must report failure without crashing.

## Resource and FPS protocol

Warm up for two minutes. Measure idle, live vision and voice-plus-critical-alert phases for ten minutes each, repeated three times, sampling every second. Do not run builds or unrelated model reviews during acceptance. Preserve all samples and process identities.

When accepting recording, repeat the affected phases with the actual OBS encoder, quality, audio and replay-buffer configuration active; retain application, OBS and combined measurements. The paired FPS comparison must represent the claimed recording configuration, and the acoustic probe must include simultaneous recording work.

For FPS, perform at least three paired overlay-off/overlay-on runs of at least 60 seconds per phase using the same device, game scenario and settings. Compute:

`fps_loss_percent = max(0, (baseline_fps - overlay_fps) / baseline_fps × 100)`

Each pair must meet ≤3%; averaging passing and failing pairs is insufficient. Record the frame-time source, excluded frames with reasons and acquisition validity. Missing game process, permissions, baseline or operator confirmation yields SKIP/INDETERMINATE. A synthetic performance-receipt test proves the validator, not real FPS behavior.

## Vision and warning quality

Hero-presence precision must be at least 95% and recall at least 90%. Hero-identity precision must be at least 95% and recall at least 85% for **each certified class**. Pooled scores cannot conceal a weak class.

Collect at least 50 independent appearance episodes per class across at least five held-out matches, with a holdout of at least 20 whole matches. Report 95% confidence intervals and annotator disagreement. Split entire matches between training, tuning and holdout; adjacent frames from the same match are not independent evidence.

High-danger false audible warnings must be at most one per 30 minutes after deduplication, reported separately by sensitivity. Actionable danger recall must be at least 90%, and false-safe assertions must be zero. Label actionable threats from observed evidence rather than treating every death as proof of a missed warning. Use the predefined ±8-second attribution window around labeled risk samples and adjudicate disagreement before tuning.

An empty detection set, unrecognized hero, failed capture, invalid geometry, stale observation or unsupported mode must never be described as safe. Publish the actual certified class list and unknown coverage with model/label hashes. Heuristic risk is not a validated path prediction or calibrated probability unless separately measured.

## Voice and memory validation

For each language, local STT must achieve English word error rate ≤15%, Thai character error rate ≤20%, and critical intent/slot accuracy ≥90%. Use at least 100 held-out utterances per language, split by speaker/session and quiet/game-audio conditions; report code-switch separately. Completion must be at least 95% per language. Keep rejects, errors and timeouts in denominators where the protocol requires them.

Before choosing an engine/model, benchmark at least two eligible configurations for licensing, reproducible hashes, offline behavior, recognition quality, warm/cold latency and total resources. A synthetic utterance fixture can validate scoring but cannot establish real-language performance. A single-speaker corpus must disclose that limitation.

Capture contracts also require: microphone permission off by default; hold minimum 250 ms and cap 15 seconds; mono i16 at 16 kHz with a maximum 480,000-byte transient buffer; open timeout two seconds; bounded callback transport; buffer zeroed on every exit. A fake-device test must establish release-to-stream-drop ≤100 ms, followed by separate real-device confirmation. Runtime talk permission never authorizes collecting a benchmark corpus.

Memory fixtures must hand-calculate hero counts and final known statistics, distinguish observed sessions from explicit completed outcomes, and keep unsupported dimensions unknown. Choose the exact eligibility/ordering policy before implementation. Verify clear across refresh/restart, same-second exclusion, post-clear inclusion, monotonic cutoff, corrupt-marker failure and byte-identical preserved journals. Test no-egress even while the separate cloud advice option is enabled.

## Evidence levels and required receipts

| Evidence | What it establishes | What it cannot establish alone |
|---|---|---|
| Schema/golden fixture | Contract, boundary and deterministic arithmetic | Real integration or performance |
| Unit/state-machine test | Local transition and error behavior | Device permissions, acoustics or game focus |
| Real socket/provider integration | Transport, ordering and failure semantics | Live-game correctness |
| Browser interaction/screenshot | Layout, keyboard and fixture presentation | Native focus, DPI, capture or audio |
| Native Windows run | Actual devices, IPC, permissions and process behavior | All game-mode/class quality |
| Live-game/acoustic/resource run | Measured behavior in the recorded matrix cell | Untested hardware/layouts or future game patches |

Receipts include target, build/dependency hashes, environment, scenario/count, method, local raw-evidence reference, expected/observed result, failures, limitations, reviewer and timestamp. Mark synthetic fixtures explicitly.

Required negative suites cover: stale boundary 2,000/2,001 ms; invalid source not renewing freshness; session/owner/team change and late response; consent withdrawal and cache invalidation; strict outgoing allowlists; unavailable local model; unsupported vision; malformed persisted data; interrupted capture; peer leave during join/sync; sequence replay; owner expiry; null recovery probe; frozen UI event stream; account switch; missing assets; and failed resource containment.

## Feasibility and release gates

First prove a small end-to-end local slice: valid input → typed unknown-aware state → deterministic alert → real audio, with local core offline and account-independent. Then validate model feasibility before integrating full voice. If the selected model exceeds CPU/RAM/latency limits, choose a smaller configuration or raise a documented change request. Do not waive model accounting or quietly weaken the requirement.

A release requires passing contract/integration, native/live quality, privacy and update/rollback gates plus an explicit unsupported-capability list. Partial features need an unavailable state or scope decision. Missing evidence blocks the affected capability/release claim; unrelated approved development can continue.

## Changelog

| Version | Date | Change |
|---|---|---|
| 0.1.0 | 2026-11-10 | Initial neutral quality baseline and evidence protocol; no runtime pass implied. |

