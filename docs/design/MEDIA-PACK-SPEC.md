---
title: "Media Package Specification"
doc_id: "MEDIA-PACK-SPEC"
version: "0.3.5"
status: "draft"
updated: "2026-11-11"
owner: "Product owner"
approval_status: "not_approved"
---

# Media Package Specification

This annex specifies a new desktop companion’s event-media behavior. It is a draft target contract, not a claim that an implementation or runtime qualification exists. “MUST” identifies required behavior once this baseline is adopted. The media authoring application is an external producer; rebuilding its editor is a separate scope decision.

## MP-01 — Separate detection, catalog, and media

The event catalog MUST have one canonical, versioned source. It defines stable semantic IDs, localized labels, source ownership, detector references, bounded tunables, ordinary voice ranks, introduction versions, and deprecation. Generated consumer tables MUST be checked against that source. Detector references identify reviewed native logic; they are not executable expressions or a runtime rule engine.

Detection answers whether an event occurred. Catalog availability answers whether this runtime implements its detector. Package mapping answers whether verified media exists for that event. A registered event MUST NOT be treated as implemented, and an implemented event MUST NOT imply that audio is available. Unknown, unsupported, unobserved, and unmapped are separate states.

| Event semantic group | Permitted source | Availability rule |
| --- | --- | --- |
| Kill, first blood, multi-kill, streak, death, respawn, level milestone | Admitted local-player game-state transitions | Session-bound; initialize baselines without replaying historical events |
| Low health or mana | Admitted local-player values | Threshold edge, not continuous repetition |
| Enemy missing | Valid local visual coverage and prior observation | No coverage means unavailable; never infer safety |
| Gank risk, health danger, warning retraction | Native safety detectors | Independent critical admission path |
| Dynamic advice | Separate advice capability | Does not become an event-pack fallback |

The builder MAY choose neutral machine identifiers, but MUST freeze them in the catalog before fixtures or package schemas depend on them. The proposed catalog is [event-catalog.json](../../contracts/event-catalog.json), elaborated in [EVENT-AUDIO-SPEC](EVENT-AUDIO-SPEC.md) under DEC-EVENT-AUDIO. IDs MUST never be reused for a different meaning. Adding an ID increments the minor taxonomy version; changing a meaning or removing a supported identity requires a major version and migration decision. Retired IDs remain recorded.

## MP-02 — Detection and audio arbitration

The ordinary announcer chooses at most one highest-ranked event from a tick. Multi-kill outranks streak, which outranks first blood and ordinary kill. Multi-kill counts kills within the configured 18-second default window; streak counts kills since death. The catalog MUST preserve this distinction. Default milestone levels are 6, 12, 18, and 25. Low-health and low-mana defaults are 20% and 15% respectively. Session changes, counter resets, reconnects, and regressed or late samples MUST NOT create duplicate achievements.

Critical safety cues use a separate priority policy and MUST preempt lower-priority playback where required. A missing-enemy cue is emitted at most once per hero per missing episode after more than five seconds of valid coverage; its global voice cooldown is at least ten seconds. It is dropped, not queued, while critical audio is active. Respawn remains a valid state transition but is unvoiced in the initial mapping policy.

Preemption MUST cancel both the old audio and its paired visual lifecycle. Outcomes MUST record a local reason, such as admitted, unmapped, invalid-media, stale-session, superseded-revision, critical-active, cooldown, or muted. Event history MUST distinguish detection from actual audio admission. The proposed class/preemption matrix, mute semantics and outcome codes are in [EVENT-AUDIO-SPEC §4–§5](EVENT-AUDIO-SPEC.md).

## MP-03 — Package contents and integrity

A package MUST contain a descriptor, event registry, payload size/digest inventory, and referenced media. Every playable mapping identifies one mixed audio asset and its paired silent video. Original audio may be retained for identity/provenance; it MUST NOT replace invalid mixed audio.

The initial container profile uses indexed ZIP with deterministic ordering: descriptor first, then ASCII-sorted payload paths, fixed timestamp 1980-01-01, and regular-file permissions 0644. JSON, WAV and optional inert source/font assets use Deflate level 6; already compressed video uses Stored. Readers admit methods 0 and 8. SHA-256 covers decompressed payload bytes. Copy/export preserves existing bytes; rebuilding changed content creates a new revision.

| Limit | Maximum |
| --- | ---: |
| Container size | 512 MiB |
| Expanded aggregate, including descriptor | 512 MiB |
| Individual payload | 32 MiB |
| Descriptor or registry, each | 2 MiB |
| Payload count | 2,048 |
| Event mapping keys | 128 |

Admission MUST reject traversal/unsafe paths, duplicate ZIP records or JSON keys, encryption, unexpected extras/comments, hidden prefixes/suffixes, header disagreement, dangling references, unsupported media, and size/digest mismatches. Extraction MUST use bounded streaming into staging. An invalid package MUST leave the previously installed and active revision intact.

## MP-04 — Audio and video contract

Event playback MUST use verified mixed PCM WAV and paired silent WebM with a supported video track and no audio track. The authoring output profile uses mono 48 kHz PCM16 audio. The native player MUST successfully decode media before advertising readiness; a plausible file extension or container header is insufficient. [EVENT-AUDIO-SPEC §6](EVENT-AUDIO-SPEC.md) proposes the exact WAV profile, the VP9/VP8 WebM track rules and a renderer decode probe for the video that the renderer plays.

Audio duration is at most 15,000 ms. Original and mixed audio durations MUST match. The optional visual tail is an integer from zero to 3,200 ms, default zero. Video may continue only for that declared tail after natural audio completion. Stop, mute, preemption, or session invalidation cancels it immediately.

Native audio is the single audio owner. Video is always muted. The renderer consumes the admitted pair’s lifecycle and MUST NOT independently choose media or restart an event. Event preview and production playback MUST use the same package reader and admission rules.

## MP-05 — Taxonomy compatibility

New packages MUST explicitly declare a taxonomy version. The canonical grammar is `MAJOR.MINOR`: two unsigned 32-bit ASCII integers, without signs, whitespace, suffixes, patch components, or leading zeros except literal zero. Missing, null, wrong-type, or malformed versions are rejected for the new format.

| Package versus runtime | Result |
| --- | --- |
| Same major, package older or equal | Admit only known IDs; missing mappings remain silent |
| Same major, package newer | Verify all media/integrity; unknown IDs remain inert |
| Different major | Reject without changing active selection |

Inert IDs MUST be excluded from playback and preview and reported locally. Catalog registration MUST NOT enable a detector that remains unavailable. The event-key limit remains independent of the runtime catalog’s size.

## MP-06 — Installation ownership and transactions

The desktop companion owns its profile registry, installed revisions, and active selection. An external authoring application requests installation; it MUST NOT write directly into the companion’s cache or infer its data root from the working directory.

A logical install request contains request ID, profile ID, package source path, expected container digest, optional project binding, and activation policy: `never`, `now_if_idle`, or `after_match`. The installer copies and validates actual bytes inside its own staging area before an atomic registry commit. Retrying the same request/identity MUST be idempotent.

The Windows transport MUST be local, current-user/profile-bound, reject remote clients, verify the registered server identity, and perform protocol/profile/nonce handshake. Frames are bounded to 16 KiB. Exact pipe names and framing belong to the protocol ADR. This boundary separates operating-system users and profiles; it does not attest every process within the same user account.

The proposed request and result contracts are [media-install-request.schema.json](../../contracts/media-install-request.schema.json) and [media-install-result.schema.json](../../contracts/media-install-result.schema.json); the descriptor carries no authority field (EVENT-AUDIO-SPEC §7).

Installation and activation are separate transactions. Results distinguish `installed`, `active`, `deferred`, and `activation_error`; copying bytes MUST NOT be reported as successful activation.

## MP-07 — Identity, revisions, and cancellation

Opaque installation ID is distinct from publisher/media identity and revision. Registry records MUST bind authority, media identity, revision, and digest. Local imports MUST NOT impersonate system or store authority.

Playback holds an immutable revision lease until audio and declared visual tail finish. Selection changes wait until the match barrier permits activation and no relevant lease remains. Late asynchronous results MUST match profile, session, revision, and generation before taking effect.

Retain prior revisions for repair and rollback; never garbage-collect a leased revision. Cache adoption succeeds only after verified publication. Failed adoption MUST be retryable after repair even when the installation ID is unchanged.

## MP-08 — Fail-silent behavior and safety readiness

Missing, unmapped, or invalid event media MUST produce silence and a reason-coded local result. Do not substitute unrelated clips, original audio, loose files, embedded defaults, generated event visuals, or event TTS.

Critical safety readiness is a separate state. The application MUST verify and prewarm its required critical cues before claiming audible safety alerts are ready. No archive decompression, disk I/O, cloud request, language-model inference, or renderer dependency may enter the critical audio admission path.

If required critical media is unavailable, expose unavailable readiness and fail the corresponding acceptance scenario. A visual-only safety fallback is **not authorized by ordinary fail-silent behavior**; adding it requires an explicit product decision. Dynamic advice speech remains a separate feature with its own consent and interruption rules.

## MP-09 — Trust and new product identity

Integrity, publisher authenticity, purchase entitlement, and active selection MUST be evaluated independently. A valid hash does not prove a purchase; a locally imported package does not acquire store authority. Package-supplied public keys MUST NOT establish trust.

The new product MUST use new application identifiers, namespaces, trust roots, and signature domains. This specification makes no promise of binary compatibility with any previous product, archive, receipt, account, or signing system. A future importer requires its own scope, fixtures, and explicit trust conversion rules.

**Agent-owned ADR gate:** before interoperability or any signed-authority path is enabled, the builder MUST record exact schemas, byte-level signature algorithm, canonicalization, domain separation, versioning, encoding, trusted-key distribution, rotation/revocation, and verification test vectors. This annex does not invent that wire protocol. Authority-dependent operations remain unavailable until this decision and its tests are complete. Private signing keys MUST remain outside packages, source code, logs, and client bundles.

## MP-10 — Acceptance evidence

Required fixtures cover valid installation; malformed/oversized archives; corrupt mixed audio with valid original; video containing audio; unsupported versions; inert future IDs; duplicate requests; activation failure after installation; repair/retry; switching during a visual tail; session invalidation; critical preemption; missing coverage; and detected-but-unvoiced respawn.

Every result MUST identify scenario version, source revision, binary digest, profile, and observed outcome. Package/unit tests, native injected-input tests, installed UI checks, live-match correctness, acoustic timing, and CPU/RAM/FPS measurements are separate evidence classes. Passing one MUST NOT be reported as passing the others. No public media distribution is implied; demonstration assets require recorded permission/provenance appropriate to their intended use.

## Related engineering diagrams

- [DGM-010 — Media installation and deferred activation](../diagrams/INDEX.html#DGM-010)

Read each figure with its catalog notes and linked SRS requirements. Short labels do not replace failure, consent or cancellation guards. The [diagram index](../diagrams/README.md) provides editable sources and maintenance rules.
