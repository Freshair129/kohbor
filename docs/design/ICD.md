---
title: "Interface Control Document (ICD)"
doc_id: "ICD"
status: draft
version: "0.3.5"
updated: "2026-11-11"
authority: "DEC-BASELINE; see DOCUMENT-CONTROL"
owner_role: "Integration owner; people not assigned"
approval_status: "not_approved"
---

# Interface Control Document (ICD)


## Contract maturity

The bundle includes JSON Schema examples for an admitted own-player read model, cloud advice projection, peer vitals, a local event envelope, media manifest and evidence receipt. These schemas use neutral field names and draft 2020-12 syntax. They are proposed normalization for a new product, not a binary-compatible implementation of a previous protocol.

Before independent components integrate, freeze the exact command/envelope/version contracts under DEC-INTERFACES. Authentication, hashing domains, JSON canonicalization, duplicate-key rejection, transport URLs, body framing and rate limiting require explicit implementation decisions even when a JSON schema exists. Generated types should come from the frozen contracts.

## Data surfaces

All cataloged interfaces are draft in this handoff. The own-player, cloud, peer, event, media and evidence schemas cover selected payload shapes; they do not freeze a complete endpoint or IPC contract. UI commands, OBS orchestration, updater manifests, account services and entitlement still require their interface-specific records under DEC-INTERFACES and the dependent decisions.

For each interface, freeze: producer/consumer and authority; direction/transport; exact request/response/event payloads; units/null/unknown rules; authentication and consent; body/queue/rate bounds; timeout/cancellation; idempotency and retries; errors; version compatibility; and positive, negative and race fixtures. Reference the relevant SRS IDs. A missing field in this checklist is unresolved design work, not permission to choose behavior that broadens the baseline.

| Surface | Authority | Contract |
| --- | --- | --- |
| Game → native | Local permitted game input | [GSI-INTAKE-SPEC](GSI-INTAKE-SPEC.md): owned configuration, token authentication, body/rate bounds, admission and ownership; reject unsupported owner/session |
| Native → control UI | Native admitted own data | contracts/own-snapshot.schema.json 2.0.0 (per-field availability) plus semantic checks |
| Native → control UI | Independent health | contracts/runtime-status.schema.json (response of runtime.get_status) |
| Control UI → native | Settings | contracts/user-settings.schema.json and contracts/settings-patch.schema.json |
| Native events → local consumers | Session/source-owned event | contracts/event.schema.json 2.0.0 (event_type = non-test IDs of contracts/event-catalog.json); topic-specific payload must be closed |
| Native audio → history | Audio admission outcome | contracts/audio-outcome.schema.json |
| Authoring tool → installer | Install request / result | contracts/media-install-request.schema.json and contracts/media-install-result.schema.json; transport per DEC-INTERFACES |
| Native → optional cloud | Separate advice consent | contracts/cloud-context.schema.json; closed projection only |
| Native ↔ optional peer relay | Separate peer consent | contracts/peer-sample.schema.json within reviewed membership protocol |
| Native ↔ optional peer relay (abilities) | Separate abilities consent scope (PEER-010) | contracts/peer-abilities.schema.json; never sent under vitals consent alone |
| Package → installer | Untrusted bytes | contracts/media-manifest.schema.json plus strict integrity/media validation for all imports; privileged issuer claims additionally require trusted authenticity checks |
| Validation → evidence register | Actual executed test | contracts/evidence.schema.json |

The own snapshot is deliberately smaller than the eventual full UI model. It proves unknown/fresh semantics for own vitals, not all inventory/skill/cooldown interfaces. Do not infer missing full contracts from illustrative fields.

Version 2.0.0 of the own snapshot replaces the all-or-nothing `own` object with per-field `{value, availability}` pairs so that one missing value (for example HP) is `unknown` while other admitted values stay observed (AC-DATA-002). A non-null value requires availability `fresh` or `stale`; a null value requires `unknown`, `invalid` or `unsupported`. A field can never be fresher than its snapshot. `estimated` belongs to the shared availability vocabulary (DDD) but is not valid for own game-state fields, which have no estimation method; derived contracts that admit estimates must name their method. Version 1.0.0 is retired; no reader needs to accept it because no build has shipped.

## Proposed native command catalog

| Command family | Input and authorization | Completion semantics |
| --- | --- | --- |
| runtime.get_status | Trusted control context | Snapshot of independent source/resource/media health; payload frozen as contracts/runtime-status.schema.json |
| settings.patch | Allowlisted versioned patch | Atomic apply or typed rejection; never arbitrary filesystem/network; request frozen as contracts/settings-patch.schema.json with optimistic `base_revision` |
| overlay.set_visibility / mute | Trusted user action | Idempotent effective-state receipt |
| voice.configure / press / release | Local mic consent + valid binding | One fenced turn; UI status excludes transcript/audio |
| advice.request / cancel | Local by default; separate cloud projection consent | Request ID and generation-bound result |
| memory.read / forget | Local profile | Forget persists cutoff before acknowledgement |
| media.inspect / install / activate | Bounded local file selection, installer validates bytes | installed/active/deferred/failure are separate fields |
| peer.connect / leave / kick / rotate | Checked fresh peer consent; native credentials | Connected only after admitted membership; token never returned |
| social.request / respond / invite | Server identity + relation authorization | Idempotent transaction; no guest fallback on failure |
| recorder.start / stop / save | Explicit media consent; separately configured mic | OBS receipt and actual clip coverage; no fabricated success |
| updates.check / install | Native trusted manifest/public keys; safe gameplay state | Verified staged update or deferred/error |

These names are proposed API labels. Implementations may use different idiomatic symbols after one ADR establishes a mapping; the behavior and security checks remain mandatory.

## Request lifecycle

Each state-changing operation defines its idempotency scope, timeout, cancellation behavior, maximum in-flight count and safe retry policy. A timeout is an unknown remote outcome until reconciled, not proof the server did nothing. Never retry a payment-like or membership mutation blindly.

Carry request ID and relevant generations in async requests and cache keys. Match/session/consent/account/backend/package changes invalidate old results. A retry cannot bypass current consent. Results check generations again before UI display, file commit or audio admission.

## Error model

Use a closed error code plus a localized safe message and recovery action. Examples are defined in contracts/error-catalog.json. Do not expose raw exceptions, URLs with tokens, device paths, account credentials, raw GSI or microphone text.

| Error distinction | Required behavior |
| --- | --- |
| unknown vs stale vs invalid | Preserve truthful reason; do not replace with a common zero |
| transport unavailable vs unauthorized | Retry bounded transient failure; require explicit new join/auth where revoked |
| installed but activation deferred | Report successful installation and pending selection separately |
| model missing vs model warming | Explicit unavailable or preparing state; no cloud fallback |
| CPU setup failed vs telemetry query unavailable | Abort pre-UI setup failure; keep capped runtime with unavailable metric |
| invalid signature vs network unavailable | Never treat network failure as permission to accept unsigned data |

## Version and compatibility rules

Use explicit schema version on persisted/wire objects. Unknown keys are rejected on cloud/peer/security DTOs. Optional, absent and null must be distinguished. No implicit legacy version default is required for the new format.

Reserve retired event IDs. Major taxonomy mismatch rejects a pack without replacing the active selection. Same-major newer event mappings may be verified but kept inert only under a frozen compatibility policy. Future peer ability/item fields require a new consent scope and schema version, not a permissive generic payload.

## Semantic tests beyond schema

- hp must not exceed max_hp; all admitted peer vitals must be present.
- fresh requires valid source age ≤2000ms; polling or invalid data cannot renew it.
- non-null peer sample requires non-null admitted room/match/team scope.
- monotonic sequence must increase within membership; old generation is rejected.
- normalized coordinates require matching capture geometry and source epoch.
- exactly zero cooldown does not imply castability.
- publisher-provided keys cannot bootstrap their own trust.
- sensitive free text must never enter otherwise-valid cloud fields.

The fixtures distinguish structural rejection, semantic rejection and lifetime transitions. A green schema check covers only the structural cases it actually ran.

## Related engineering diagrams

- [DGM-011 — Consented peer sharing and leave sequence](../diagrams/INDEX.html#DGM-011)

Read each figure with its catalog notes and linked SRS requirements. Short labels do not replace failure, consent or cancellation guards. The [diagram index](../diagrams/README.md) provides editable sources and maintenance rules.
