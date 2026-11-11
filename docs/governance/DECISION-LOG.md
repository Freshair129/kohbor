---
title: "Decision Log and Open Items"
doc_id: "DECISION-LOG"
status: draft
version: "0.3.3"
updated: "2026-11-11"
authority: "DEC-BASELINE; see DOCUMENT-CONTROL"
owner_role: "Product and engineering owners; people not assigned"
approval_status: "not_approved"
---

# Decision Log and Open Items

## Recorded product-name decision

On 2026-11-10 the product owner selected **Kohbor / เกาะเบาะ**. The English display name is `Kohbor`; the Thai display name is `เกาะเบาะ`. [product-identity.json](../../contracts/product-identity.json) records this choice with unresolved technical fields left null.

This closes the product-name choice only. DEC-NAME remains delegated for app/package identifiers, local data directory, URI scheme, signature/hash namespaces and an update origin when its service is specified. No domain or service origin follows from the name. Original visual design remains open under DEC-VISUALS. The naming decision does not approve the specification as a whole, implementation evidence or a release.

## What the builder can decide

| Area | Delegated freedom | Boundary |
| --- | --- | --- |
| Presentation and technical identity | Original character/palette and other visuals; freeze remaining stable technical identifiers | Preserve the owner-selected Kohbor / เกาะเบาะ name, function, consent and data truthfulness |
| Internal structure | Module directories, idiomatic symbols, bounded implementation details | Keep contract ownership and hot-path constraints |
| Dependencies | Pin measured compatible versions within baseline stack | No unsupported performance/privacy claims |
| Neutral wire names | Choose/freeze a consistent new protocol | No implied old interoperability |
| Testing tools | Appropriate harnesses and fixtures | Evidence classes and thresholds remain fixed |
| Presentation details | Typography, icon set, component styling | Preserve inherited layout/interaction and accessibility until a redesign decision |

## What needs a product or contract decision

privacy expansion; replacing local inference with cloud; resource-budget relaxation; removing required MVP voice/memory; changing base/premium behavior; automatic recording or data deletion; new account/payment services; compatibility with an older format; changing supported/certified scope.

## Open decision register

The machine-readable source is contracts/decisions.json. Each record names an owner role, affected work, safe interim behavior and closure evidence. OPEN does not stop unrelated development.

- DEC-NAME: the owner selected Kohbor / เกาะเบาะ; the remaining technical identity (now including the GSI configuration slug) is delegated to the builder for M0. Complete the existing identity manifest and record the relevant interface/trust decisions before closure. The selected name alone does not close this record.
- DEC-INTERFACES: freeze neutral schemas, full command payloads, framing, request limits, duplicate-key behavior and IPC authority. Example schemas alone are not a complete API.
- DEC-GSI-INTAKE (proposed 2026-11-11, not approved): the M1 slice of DEC-INTERFACES for game-state intake. [GSI-INTAKE-SPEC](../design/GSI-INTAKE-SPEC.md) proposes the owned configuration file and its contents (including a 1.0 s heartbeat so a quiescent game stays inside the 2,000 ms freshness boundary), port persistence and restart behavior, token authentication and body bounds, field-level admission, ownership verification (token plus platform-identity cross-check), session-start evidence and restart continuity, and the monotonic freshness clock. Assumptions A-1..A-9 about the game integration feature close only with M0 capture-spike evidence. Visible consequence for the product owner to acknowledge: game mode is reported "unverified" whenever the source does not expose it; the app never guesses it.
- DEC-EVENT-AUDIO (proposed 2026-11-11, not approved): the M2 slice of DEC-INTERFACES for events and audio. [EVENT-AUDIO-SPEC](../design/EVENT-AUDIO-SPEC.md) proposes the canonical catalog ([event-catalog.json](../../contracts/event-catalog.json)) with ranks beyond the fixed multi-kill > streak > first blood > kill order, a five-class audio priority/preemption matrix, outcome reason codes, the WAV/WebM profiles with a renderer decode probe, and install request/result contracts with installer-bound authority. The product owner acknowledged on 2026-11-11 that global mute also silences critical audio and reports safety audio not ready; this acknowledgement covers mute semantics only and does not approve the rest of DEC-EVENT-AUDIO.
- DEC-HEALTH-DANGER (open, product owner): MP-01 names a "health danger" safety detector without defining it. EVENT-AUDIO-SPEC §3 proposes HP ≤25% after a ≥15-point drop within 2,000 ms, re-arm at ≥40%, 20-second cooldown. Until the owner decides, the detector is unavailable and M2 latency engineering uses a test-only probe.
- DEC-MODELS: select vision artifacts before M3 and local STT/answer before M5, each only after the fixed resource/quality feasibility evidence. No model is certified by this package.
- DEC-VISUALS: original annotated wireframes, design tokens, exact desktop fit/minimum window and native game safe areas are still to be produced. Inherited topology is supplied; there are no pixel-approved image assets in this bundle.
- DEC-PACK-CRYPTO: choose new signature domain, canonicalization, archive identity and trusted key distribution; verify against known-answer fixtures before interoperability.
- DEC-SESSION: freeze complete/partial session and aggregate eligibility semantics. Conservative default: partial sessions can appear in labeled history, but outcomes and complete-session summaries require explicit completion evidence.
- DEC-RETENTION: set explicit local-log/clip quotas, retention and destructive-cleanup consent before shipping. No silent automatic deletion is authorized by this draft.
- DEC-PREMIUM: deferred authority service, subject/device model, key management, RLS, revocation and rollback need their own accepted design; base UI continues.
- DEC-SOCIAL: optional social data model, registered-relay tickets, transaction order, retention/limits and operations need complete contracts before production deployment.
- DEC-GAME-DATA: freeze supported hero classes, patch catalog, provenance and ability-ownership evidence; action-damage feature scope and exact formulas need an approved neutral spec. Unknown classes/data remain unknown.
- DEC-RISK-DETAIL: base missing-time/heading formula is included, but complete coordinated-motion/reappearance, death-window, heading and ETA fixtures must be frozen. Until then, the score is informational and audible missing-gank remains unavailable; do not claim a speculative discount or a bare threshold as approved behavior.
- DEC-CRITICAL-MEDIA: provide authorized verified gank/danger/revision assets for the first-demo gate. Missing critical media means not ready; a visual-only or synthesized fallback would need a new product decision.
- DEC-VOICE-BUFFER: define single-owner PCM handoff, stream termination, bounded decoding lifetime, zeroization and cancellation before local STT integration. No persistent audio/transcript is allowed.
- DEC-OBS-BUDGET: define the combined recording-profile acceptance decision after reporting app and external OBS separately; existing app/latency/FPS limits remain intact.
- DEC-NATIVE-EVIDENCE: assign actual Windows/Dota/OBS/audio devices and testers. This is an execution dependency, not a reason to fabricate a pass.

## Reconciled ambiguities

| Topic | Normalized decision for this draft |
| --- | --- |
| Source docs described voice both planned and present | Target requires local conversation; source presence is separate from native and full-path evidence |
| Earlier resource ceilings differ | Current target is 10% normalized CPU and 900,000,000-byte aggregate full Working Set |
| Event media fallback differs across old sections | Verified active package only; ordinary missing mapping silent, critical readiness separately gated |
| Earlier UI used mock/unknown skill grids | Production neutral start; compact unknown cards; hide unknown-only rows |
| Entire live page previously treated as premium | Base canvas for everyone; specific details need trusted capability |
| Old input vectors imply enemy telemetry | Enemy position/visibility only from admitted vision; do not fabricate hidden values |
| Voice/recording/broadcast overlap | Local PTT, explicit OBS highlights and deferred broadcast co-host are distinct |
| CORE-003 is MVP while accounts are extension scope | CORE-003 is a guard on any build that contains an optional account surface: account loss/switch must never disturb local core. It adds no account feature to MVP. Its target milestone is M6, where the account surface first exists; builds without accounts satisfy it trivially and record that scope in evidence rather than a PASS for account transitions |
| Store-only import vs local package installation | Local verified import is supported; integrity is not purchase entitlement |
| Existing signed formats include identity strings | New protocols/trust roots; legacy compatibility explicitly out of scope |

This draft is useful for M0 planning and independent local-core implementation. It is not a claim that every interface or design choice required for a full rebuild has already been frozen. The remaining work is visible and bounded so the next agent can resolve it deliberately.
