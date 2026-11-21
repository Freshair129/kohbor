---
title: "Security and Privacy Specification"
doc_id: "SECURITY-PRIVACY"
status: "draft"
version: "0.3.7"
updated: "2026-11-21"
owner: "Product Owner"
approval_status: "not_approved"
---

# Security and Privacy Specification

## Purpose and authority

This document specifies target behavior, not a verified implementation. Naming and visual identity are implementation choices. Subscription tier, provider choice, account session and friendship cannot expand a data permission.

“Local” means processing and storage on the player's computer. A remotely reachable inference server is not local merely because the same software also runs locally. A controlled loopback provider is permitted only after its endpoint, process ownership, resource accounting and permitted data have been specified. Voice processing must remain on-device; no remote endpoint or automatic cloud fallback is allowed.

## Data route matrix

| Data | Local consumers | Optional cloud advisor | Team-status relay | Optional account/social service | Public hero statistics provider |
|---|---|---|---|---| --- |
| Coarse phase; own hero, level, kills/deaths/assists, gold, net worth, HP%, mana%; team score | Approved local modules | Explicit purpose-specific opt-in only | Only the smaller relay projection below | Not social data | Forbidden |
| Exact clock, own items, burst estimate | Approved local modules | Forbidden in initial contract | Forbidden in initial protocol | Forbidden | Forbidden |
| Raw game payload, raw match identifier, local platform identity | Admission and local processing | Forbidden | Forbidden; use scoped match binding | Never add to social gameplay storage | Forbidden |
| Own hero, level, HP, maxHP, mana%, alive, respawn seconds | Local display | Only fields separately allowed above | Explicit sharing consent and valid membership/scope | Forbidden | Forbidden |
| Screen images, crops, detections, enemy identity/position and text derived from them | Approved local observation/advice | Forbidden | Forbidden | Forbidden | Forbidden |
| Local journal, replay, history, player memory and all derived summaries | Approved local modules | Forbidden, including aggregates | Forbidden | Forbidden | Forbidden |
| Microphone audio, transcript, free-form voice utterance | Transient local voice processing | Forbidden | Forbidden | Forbidden | Forbidden |
| Account identifiers, contacts, credentials and security data | Minimal identity operations | Forbidden in prompts | Account tokens/identifiers forbidden | Purpose-limited identity/social authorization | Forbidden |
| Private friendships, blocks, presence and invitations | Account UI | Forbidden | Only separately specified proof/ticket exchange | Private bounded records | Forbidden |
| Diagnostic paths, logs and performance samples | Local diagnostics | Forbidden | Forbidden | No automatic upload | Forbidden |
| Static persona and fixed instructions with no player data | Local configuration | Permitted | Unnecessary | Unnecessary | Forbidden |
| Own hero ID from admitted GSI and optional manually chosen bracket for public hero statistics | Native public statistics adapter | Separate advice projection above | Separate relay projection below | Forbidden | Permitted only with separate explicit consent |

Public-profile enrichment is a separate optional integration. Its request fields, caching, visibility and authorization must be specified before implementation. It cannot become a back door for uploading gameplay, memory, voice or private social data.

## Separate consent states

Maintain independent settings and cancellation generations for cloud context, public hero statistics, microphone, peer sharing, social presence and profile linking.

Cloud context defaults off. Sign-in, entering a provider key, selecting a provider and pressing Ask do not grant consent. Present the purpose and exact permitted field groups beside the opt-in. Persist this preference locally. Construct a purpose-built projection; do not serialize a complete game object and then attempt to remove sensitive fields. An explicit cloud selection with consent off returns a local alternative or unavailable. Automatic provider selection must skip cloud.

Revocation increments the relevant generation before new work can be admitted. Cancel future sends, invalidate matching caches and discard late results. Request identity must include the necessary session, consent, backend and account generations. A response from an older context cannot restore state, start playback or repopulate cache. Apply the same gate to API, subprocess, retry, error, cancellation and cache routes.

Microphone access defaults off and applies only during deliberate push-to-talk. It does not permit disk recording, corpus collection, account linking or cloud processing.

Every deliberate team create/join requires newly checked sharing consent in the same action. Reject absent consent before any network request. No automatic join, persisted room credential or passive view-only mode is part of the initial protocol. Friendship and accepted invitations do not grant gameplay sharing; the joining user must still consent.

## Public hero statistics request and cache

DEC-PICK-STATS and STATS-001–003 govern this optional flow. A separate Settings switch defaults off. Explain the purpose, exact request fields, provider and cache behavior before opt-in. Sign-in, a draft, provider selection and other consent states cannot enable it. The provider sees the client's IP address, own hero ID and request time, and sees the bracket if the player chose to include it. The separate v1 suggestions list uses at most four fixed public-table requests per day and sends nothing user- or match-specific; the provider still sees the client IP address and request time. Position/bracket filtering is local.

The native host owns the HTTPS request to the selected, fixed provider origin, with OpenDota first. Validate the closed outgoing projection before transport. The only request data are the admitted own hero ID and optional manually chosen bracket. Provider and dataset selection are routing choices. Exclude player names, account and match IDs, ally and enemy heroes, images, voice, memory and game clocks. The renderer has no provider request authority. Keep provider I/O outside intake and critical warning paths. Apply the current consent and own-pick generation when sending, retrying, receiving and caching.

The cache stores only public aggregate tables for the requested own hero, keyed by provider, dataset, own hero and optional bracket with the provider's data window and fetch time. Entries are fresh for less than 24 h, visibly stale from 24 h to less than 7 d, and discarded at 7 d. The local keys can reveal which heroes were queried. Exclude Draft Bar observations, session links and player data from this cache, and keep cache data out of logs, exports and crash reports. On revoke, stop sends, cancel work, clear displayed values, discard late results and delete the local statistics cache. A deletion failure keeps the cache inaccessible and reports the failure. Local gameplay continues independently.

## Team sharing admission and transport

Admit an outgoing sample only when all of the following are known and valid: game application identifier 570, explicit playing activity, verified active local player ownership, canonical positive match identifier, exact Radiant/Dire team, pregame or in-progress state, and every required stat. Spectator, selected-unit ambiguity, missing identity, malformed JSON/schema or incomplete stats invalidate the previous publication immediately.

The initial stat projection contains only:

| Field | Valid domain |
|---|---|
| Hero | Canonical game hero key for the verified local player |
| Level | Integer 1–30 |
| HP / maximum HP | Integers 0–100,000 / 1–100,000; HP cannot exceed maximum |
| Mana percentage | Integer 0–100; do not fabricate absolute mana |
| Alive | Explicit boolean; never infer from HP or respawn countdown |
| Respawn seconds | Explicit integer 0–600 |

Abilities, items, positions, economy and other players' data require a separately versioned contract and permission change. They must not silently enter this projection.

Recipients must be authorized room members with the same room-scoped match binding and team. Raw match and platform identifiers remain local. A scoped hash is an equality fence, not anonymity or official game attestation. The relay operator can read shared stats; do not claim end-to-end encryption.

Remote relay transport requires HTTPS. Literal loopback HTTP is allowed for local testing. Native transport rejects redirects, URL credentials, query strings, fragments and arbitrary paths; accepts a validated root origin only. The renderer cannot submit arbitrary requests or receive the member token. A new product must select and version its wire identifiers and hash domain explicitly; changing protocol strings does not preserve interoperability automatically.

Bound the initial service: five members including owner; four-hour room lifetime; membership expires after 30 seconds without sync; at most 100 rooms; requests at most 4,096 bytes; responses at most 32 KiB. Default rate limits are create 10/minute/IP, join 20/minute/IP, sync at least 200 ms apart/member, and at most 1,024 limiter entries. Reject unknown schema keys. Do not trust forwarded client-IP headers without an explicit deployment design. No public room inventory, cross-origin renderer access, or cacheable room response.

Use one asynchronous request in flight, outside telemetry and critical-warning paths. Nominal sync cadence is 500 ms, request timeout 1,500 ms and bounded transient retry 0.5–5 seconds. After a transport failure, successfully publish a null scope/sample probe before resuming stats. Duplicate/regressed sequence numbers and obsolete room generations are rejected.

Age accumulates conservatively from source age, publisher upload bound, relay elapsed time, complete receiver round-trip time and local UI elapsed time. Use monotonic durations, not wall-clock comparison between computers. Live age is at most 2,000 ms; older data becomes null/stale. Polling cannot refresh an old observation. The UI expires data even if its event stream stops.

Ability and cooldown sharing (PEER-010, DEC-PEER-ABILITIES) uses its own opt-in `abilities` scope, off by default, and its own closed DTO. Vitals consent never implies it. Withdrawing the scope stops publication and clears received ability state.

Leave immediately clears local credentials and peer state and invalidates pending work; remote deletion is best effort. Owner leave/expiry closes the room. Invitation rotation blocks future use of the old generation but does not evict current members; kick and rotation are distinct operations. Relay restart clears its memory. Never promise instantaneous remote erasure during a disconnection.

## Native, account and social authority

The native host owns admission, storage, transport, cancellation and operational permissions. IPC validates calling window/origin and typed arguments; frontend booleans are not authorization.

The server owns account permissions and social visibility. Guest local gameplay and guest room-code access do not require an account. Private social actions require an authenticated account. Sign-out, revocation and account switch invalidate account work without locking the local core.

Keep account credentials at the account broker. The relay receives neither account JWTs nor private account identifiers. Native code owns member credentials; social storage does not store relay member tokens or room secrets. Registered relay proofs and one-use admission tickets have distinct purposes, expiries and replay protection. Revalidate room owner, generation and capacity after an asynchronous broker exchange.

Presence defaults off and means online in the application, not inferred game activity. Expose only a safe display name and permitted public identity for authorized relationships. No public directory, private contact disclosure or last-seen history. Block/remove must invalidate unredeemed authorization according to transaction ordering; it does not promise immediate eviction from an existing room. Failed social admission must not silently fall back to guest admission.

## Persistence, erasure and secrets

Microphone buffers never enter files, journal events, analytics or crash reports. Capture-only operation zeroes/drops buffers on release, hold limit and every cancellation/error/exit. Full STT integration must first specify bounded buffer ownership, decoder handoff and zeroization timing; a release-time deletion rule cannot silently become indefinite retention. User-facing events contain state/timing only. Benchmark audio requires separate speaker consent and remains on-device outside source control.

Player memory is derived locally. Clearing it writes a durable nondecreasing logical/epoch cutoff comparable with archive timestamps (not a persisted process-monotonic clock value) before removing the derived snapshot. Sources at or before that cutoff stay excluded after restart; same-second timestamps are excluded conservatively. An unreadable/corrupt marker fails closed instead of rebuilding forgotten data. Clearing derived memory preserves separately managed journal archives; the interface must explain this distinction. Unknown outcomes, death locations, ratings or style inputs remain unknown.

Secret/private keys and authentication tokens never enter prompts, ordinary logs, URLs or source control. Public verification keys may be version controlled and shipped. Provider keys belong only in the authentication channel for the selected provider. Signing private keys belong to the release service, separate from verification keys shipped to clients. Production secret storage, rotation and account-session recovery need explicit platform decisions before those services ship.

No automatic gameplay analytics, crash uploads or diagnostic exports. Local audits contain sanitized reason codes/metadata, never forbidden payloads or credentials.

## Decisions that must be closed before the affected feature ships

Record concrete values and owners for journal retention/size/deletion, private social-record retention and cleanup, secure local credential storage, account-cache expiry, service audit retention, and key rotation/revocation. A feature with undefined persistence behavior cannot ship persistent records. These decisions must not weaken the route matrix implicitly.

Inspect actual outbound payloads and filesystem writes during success, failure, retry, cancellation, sign-out and revocation. Static review/unit tests alone cannot prove runtime no-egress.

## Changelog

| Version | Date | Change |
|---|---|---|
| 0.3.6 | 2026-11-20 | DEC-PICK-STATS adds a separate public hero statistics route, opt-in and cache erasure rules (STATS-001–003). |
| 0.1.0 | 2026-11-10 | Initial neutral privacy and security target, with separate permissions, authority and unresolved operational decisions. |


## Related engineering diagrams

- [DGM-005 — Independent external data routes](../diagrams/INDEX.html#DGM-005)

Read each figure with its catalog notes and linked SRS requirements. Short labels do not replace failure, consent or cancellation guards. The [diagram index](../diagrams/README.md) provides editable sources and maintenance rules.
