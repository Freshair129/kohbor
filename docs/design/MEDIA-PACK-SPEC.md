---
title: "Media Package Specification"
doc_id: "MEDIA-PACK-SPEC"
version: "0.4.0"
status: "draft"
updated: "2026-11-26"
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

The ordinary announcer chooses at most one highest-ranked event with ready sound from a tick. Events without ready sound are recorded as unmapped or invalid and removed before arbitration, so the next-ranked mapped event can play. Multi-kill outranks streak, which outranks first blood and ordinary kill; special-event ranks are in the canonical catalog. Multi-kill counts kills within the configured 18-second default window; streak counts kills since death. The catalog MUST preserve this distinction. Default milestone levels are 6, 12, 18, and 25. Low-health and low-mana defaults are 20% and 15% respectively. Session changes, counter resets, reconnects, and regressed or late samples MUST NOT create duplicate achievements.

Critical safety cues use a separate priority policy and MUST preempt lower-priority playback where required. A missing-enemy cue is emitted at most once per hero per missing episode after more than five seconds of valid coverage; its global voice cooldown is at least ten seconds. It is dropped, not queued, while critical audio is active. Respawn remains a valid state transition but is unvoiced in the initial mapping policy.

Preemption MUST cancel both the old audio and its paired visual lifecycle. Outcomes MUST record a local reason, such as admitted, unmapped, invalid-media, stale-session, superseded-revision, critical-active, cooldown, or muted. Event history MUST distinguish detection from actual audio admission. The proposed class/preemption matrix, mute semantics and outcome codes are in [EVENT-AUDIO-SPEC §4–§5](EVENT-AUDIO-SPEC.md).

## MP-03 — Package contents and integrity

A package MUST contain a descriptor, event registry, payload size/digest inventory, and referenced media. Descriptor 1.3.0 keeps one media pair on each binding. In 1.4.0 a binding keeps `event_id` and optional `tier`, and its media fields live only inside `variants[]`; binding-level media fields are not accepted. Proposed descriptor 1.4.1 adds an optional per-variant `caption`: trimmed plain text of 1 to 200 Unicode scalar values with no control characters or line breaks. Authors should supply NFC text. Captions are display text only; they do not prove that speech matches the text, establish identity or authority, or provide audio input. This 1.4.1 contract proposal requires owner approval before integration. Descriptor 1.4.3 adds optional binding-level `hero`, a lower-case `npc_dota_hero_*` ID no longer than 64 characters; 1.4.3 is the only descriptor version that accepts this field, and older descriptors remain valid. Each variant has a pack-unique `variant_id`, one mixed audio asset and, optionally, its paired silent video and identity audio. `clip_id` is optional provenance for local logs only and MUST NOT be trusted as identity or authority. A variant without video is audio-only: it plays its audio, shows no banner, and declares no visual tail. A variant with video is paired by event key inside the same package; the author guarantees the timing. Pairing a voice from one package with a banner from another is not part of the first release (see DEC-PACK-BANNER-ADDON). Original audio may be retained for identity/provenance; it MUST NOT replace invalid mixed audio.

One variant has no `selection`; two or more variants require exactly one of `random`, `shuffle` or `sequence`. Optional `weight` (1-100, default 1) is valid only with `random`. `when` is reserved for phase 2 and a 1.4.x reader MUST reject it with a clear reason. The reader continues to admit 1.3.0, mapping each binding to one deterministic variant. These descriptor choices are recorded in DEC-VOICE-VARIANTS.

**Presentation (`media-manifest` 1.4.2, owner decision 2026-11-20):** `presentation` is optional. It may contain `description` (at most 500 Unicode scalar values), `author` (at most 80), `languages` (at most eight unique BCP-47 tags), and `cover` (a referenced `files[]` path with role `cover`). Text has no control characters or paragraph separators, so descriptions remain a single paragraph. A cover is a still PNG or WebP, at most 1 MiB and 1920×1080; APNG and animated WebP are rejected. Older descriptor versions and 1.4.2 descriptors without `presentation` remain valid. The pack author mapping accepts the same `presentation` object; CLI flags `--description`, `--author`, repeatable `--language`, and `--cover` override mapping values. The author packages the cover under a content-derived path and never returns the source path in the descriptor's host summary.

**Authoring note (`kohbor-pack-build` mapping 1.0.0):** Existing bindings with `event_id`, optional `tier`, `audio`, `visual`, `identity_audio` and `visual_tail_ms` remain valid and produce the existing 1.3.0 descriptor shape when no binding uses `variants`. A binding may instead contain `variants: [{ audio, visual?, visual_tail_ms?, identity_audio?, clip_id?, variant_id?, weight?, caption? }]` and a binding-level `selection` when there are two or more variants. The builder assigns missing `variant_id` values deterministically in pack order, checks each variant's media and caption text rules, and reports each written or skipped variant by ID. If any binding uses this form without captions, every output binding uses descriptor 1.4.0; a captioned mapping uses descriptor 1.4.1; presentation on its own uses descriptor 1.4.2; a hero condition uses descriptor 1.4.3. Identical payload media shared by variants is stored once.

For a hero-scoped binding, the mapping file also accepts optional `hero: "npc_dota_hero_*"`; output uses descriptor 1.4.3. A descriptor still has at most one binding per event and tier. Across an equipped loadout, a matching scoped binding outranks general bindings for that same event and tier; tier precedence remains unchanged. If the own hero is unknown or does not match, the scoped binding is skipped. The media event map schema 1.3.0 includes the binding hero on its variants so Pack Library and the event × pack coverage table can disclose conditions such as “เฉพาะ Sniper”.

The initial container profile uses indexed ZIP with deterministic ordering: descriptor first, then ASCII-sorted payload paths, fixed timestamp 1980-01-01, and regular-file permissions 0644. JSON, WAV and optional inert source/font assets use Deflate level 6; already compressed video uses Stored. Readers admit methods 0 and 8. SHA-256 covers decompressed payload bytes. Copy/export preserves existing bytes; rebuilding changed content creates a new revision.

| Limit | Maximum |
| --- | ---: |
| Container size | 512 MiB |
| Expanded aggregate, including descriptor | 512 MiB |
| Individual payload | 32 MiB |
| Descriptor or registry, each | 2 MiB |
| Payload count | 2,048 |
| Event mapping keys | 128 |
| Variants per binding | 16 |
| Variants per pack | 512 |

Admission MUST reject traversal/unsafe paths, duplicate ZIP records or JSON keys, encryption, unexpected extras/comments, hidden prefixes/suffixes, header disagreement, dangling references, unsupported media, and size/digest mismatches. Extraction MUST use bounded streaming into staging. An invalid package MUST leave the previously installed and active revision intact.

## MP-04 — Audio and video contract

Every admitted variant MUST be checked independently: its mixed PCM WAV and optional silent WebM with a supported video track and no audio track. If identity audio is present, its sample count MUST match that same variant's mixed audio. The authoring output profile uses mono 48 kHz PCM16 audio. The native player MUST successfully decode media before advertising readiness; a plausible file extension or container header is insufficient. [EVENT-AUDIO-SPEC §6](EVENT-AUDIO-SPEC.md) proposes the exact WAV profile, the VP9/VP8 WebM track rules and a renderer decode probe for the video that the renderer plays.

Each variant's audio duration is at most 15,000 ms. Its optional visual tail is an integer from zero to 3,200 ms, default zero, and MUST be zero for an audio-only variant. Video may continue only for that declared tail after natural audio completion. Stop, mute, preemption, or session invalidation cancels it immediately. A catalog event with `critical_media_required: true` MUST have exactly one variant, so critical readiness cannot select an unchecked alternative. Selection execution and history belong to the runtime; this descriptor change does not replace the native readiness gate.

Native audio is the single audio owner. Video is always muted. The renderer consumes the admitted pair’s lifecycle and MUST NOT independently choose media or restart an event. Event preview and production playback MUST use the same package reader and admission rules. A drawer preview of an exact variant from an installed, non-active pack uses that pack's verified revision and sends audio only to the app preview path; it MUST NOT send the preview visual to the in-game overlay. The drawer obtains its silent visual URL from the host's verified media protocol and plays that URL in its own panel. The active-pack audition retains overlay playback only when the caller opts in; the default is panel-only. A match blocks both non-active audition and its visual URL command.

### Event cabinet audition

The Packs page consumes `media_event_map` schema 1.2.0 and retains P2's 1.0.0 and P1's 1.1.0 readers. The legacy `media_cabinet` command remains available until P1 removes it. The map has a host-owned row for each active or proposed-unavailable catalog event and tier. Rows add `event_group`, lock state, contributing sources, and per-variant install/package provenance; the top level adds ordered `loadout[]` and prewarm sample usage, cap, and skipped-variant count. Existing state and caption rules remain. A malformed, missing, duplicate, or unexpected row makes only the affected row unknown in the renderer; it does not erase the remaining catalog.

For example, an inherited tier can be reported as `{"event_id":"multi_kill","tier":3,"state":"inherited","reason":"TIER_FALLBACK","resolved_from_tier":2,"selection":"random","variants":[{"variant_id":"v1","clip_id":"clip_01","duration_ms":2100,"has_visual":true}]}`. An unavailable first-blood row remains visible with `state: "detector_unavailable"` and `reason: "EVENT_UNAVAILABLE"`; an unvoiced `hero_respawn` row uses `state: "not_voiced"` and `reason: "NOT_VOICED"`.

The drawer requests `media_installed_event_map(install_id)`. The reply uses the same 1.2.0 shape. Its `active` field identifies the requested revision, even when a different revision is active. The host reads and verifies installed bindings without changing activation or publishing media routes. An inactive revision lists exact authored tiers because its audition command takes an exact tier. An unknown install returns `UNKNOWN_INSTALL`. A match can block audition while the read-only map remains available.

The audio engine considers one audition after game requests and admits it only when the channel and visual are idle. `media_audition_variant(event_id, tier, variant_id)` resolves that exact active-pack variant without applying random, shuffle, or sequence selection. `media_audition_installed_variant(install_id, event_id, tier, variant_id)` resolves the exact verified variant from an installed but non-active revision. A preview request is rejected with a reason code during a match, and audition requests are rate-limited. A game request cancels a pending or playing audition. Drawer previews send only audio to the audio owner; `media_variant_visual_url(install_id, event_id, tier, variant_id)` returns either `null` or an exact `kohbor-media` URL for the verified silent visual, never a filesystem path. Active-pack audition uses the overlay only when `overlay: true`; the option defaults to false. Started and ended playback lifecycle events carry the playback ID so the panel can synchronize its video with the audio and stop it at the same lifecycle boundary. Audition results continue to distinguish audio start/end from whether a visual was shown; they do not claim that audio was physically heard. The host checks the media match flag and checks the GSI phase again at admission. Draft (including strategy), pregame, live and postgame block auditions until GSI reports an unknown or idle phase and the media match flag is clear.

Overlay diagnostics use the permitted `overlay://video-diagnostic` event. The overlay reports video `started` on `playing`, or `failed` with a bounded reason/error code on `video.onError`, URL rejection, or a five-second start timeout. The audition waits up to six seconds for that visual outcome after audio ends. Host logs also record overlay window shown/hidden/skipped outcomes and non-2xx media-protocol status/reason codes. These info lines contain no paths or user data.

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

The proposed request and result contracts are [media-install-request.schema.json](../../contracts/media-install-request.schema.json) and [media-install-result.schema.json](../../contracts/media-install-result.schema.json); the descriptor carries no authority field (EVENT-AUDIO-SPEC §7). The host-owned `media_pack_summaries` response is version 1.2.0 and retains older readers. Each 1.2.0 record adds `equipped_slot` (nullable, zero-based) and derived `pack_kind` (`basic`, `special`, `mixed`, or `unknown`) alongside authority, legacy metadata, and presentation. Authority is assigned from the installer source, never from a package name. Cover URLs are allowlisted by the existing media protocol; summaries expose no payload paths.

Installation and activation are separate transactions. Results distinguish `installed`, `active`, `deferred`, and `activation_error`; copying bytes MUST NOT be reported as successful activation.

### Local uninstall

The Packs library may uninstall an installed revision by its opaque `install_id`. Uninstall MUST be refused with a typed reason while the revision is the active pointer, appears in the committed loadout, appears in a pending loadout or pending activation, or a match is active. Uninstall never changes playback selection as a side effect.

The store first atomically commits a `registry.json` without the record, then removes `revisions/<install_id>`. A failure before the registry rename leaves the record and revision intact. A crash after the rename may leave an unlisted revision directory; opening the store MUST still succeed and recovery removes that orphan. Keep request-result entries and the monotonic install-ID counter so removed IDs are not reused. Uninstall removes only the managed local revision and its announcer clips; it does not touch the original package file wherever the user stored it.

## MP-07 — Identity, revisions, and cancellation

Opaque installation ID is distinct from publisher/media identity and revision. Registry records MUST bind authority, media identity, revision, and digest. Local imports MUST NOT impersonate system or store authority.

Playback holds an immutable revision lease until audio and declared visual tail finish. Loadout changes wait until the match barrier permits activation and no equipped install lease remains. Late asynchronous results MUST match profile, session, revision, and loadout generation before taking effect.

Retain prior revisions for repair and rollback; never garbage-collect a leased revision. Cache adoption succeeds only after verified publication. Failed adoption MUST be retryable after repair even when the installation ID is unchanged.

### Multi-pack loadout (DEC-MULTI-PACK, decided 2026-11-25)

The ordered loadout contains zero to four verified installed revisions plus `event_locks` and `match_consistency`. It lives in `loadout.json` beside `active.json`; `active.json` always mirrors slot 0 for older builds. A valid loadout is authoritative on open and repairs a disagreeing mirror. When only `active.json` exists, open migrates it in memory without writing until the next change. Corrupt loadout data is an error, never an empty reset. Changes apply immediately while idle and defer as one last-request-wins document while a match or any equipped install lease blocks them. `media_activate` continues to mean replacement with one pack.

Across the loadout, the most specific eligible tier wins. For equal event/tier keys, an eligible hero-scoped binding for the observed own hero outranks general bindings; an unknown or non-matching own hero skips the scoped binding. Multiple matching scoped variants follow the existing event-lock and mixed-pool selection rules. A descriptor remains unique by event/tier. A satisfied event lock limits that event to one package; an unsatisfied lock keeps the mix and is reported. Critical events still select exactly one source. Other same-key variants form a pool; a single-pack pool preserves its authored selection, while a mixed pool uses uniform no-repeat shuffle over the union. History is keyed by `(install_id, variant_id)`. When enabled, match consistency remembers one pack per event only after audio admission; it defaults off and resets on session or loadout-generation changes.

The shared PCM cap remains 40,000,000 samples. Prewarm fills it in R0 critical, R1 first source per key, R2 other sources per key, then R3 remaining variants, deduplicating identical audio payloads by SHA-256. Skipped variants remain visible as `prewarm_cap`. Reads, hashing, decoding, merging, and renderer probes happen during prewarm, never event admission. The catalog's `event_group` is `basic` or `special`; older catalogs without the field mean `basic`. Pack kind is derived from mapped, non-inert bindings and does not change the signed pack descriptor. The first three new special detectors remain unavailable until live captures confirm true positives. Replay-only events stay outside announcer packs. Store facets remain design-only.

## MP-08 — Fail-silent behavior and safety readiness

Both playback resolution APIs first reject an active revision tree containing a link or reparse point. Within a revision that passes that gate, `Store::resolve_variants` re-verifies each variant independently and reports each payload failure alongside any verified siblings in descriptor order, failing the binding if none verify. Playback uses the remaining verified variants of that binding or produces silence with a reason-coded result if none remain. The compatibility API `Store::resolve` still checks only the first variant's pair for playback. Do not substitute unrelated clips, original audio, loose files, embedded defaults, generated event visuals, or event TTS.

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
