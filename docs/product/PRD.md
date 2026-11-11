---
title: "Product Requirements Document"
doc_id: "PRD"
status: draft
version: "0.3.4"
updated: "2026-11-10"
owner_role: "Product owner"
baseline: "DEC-BASELINE"
approval_status: "Not approved"
---

# Product Requirements Document (PRD)

## 1. Purpose and product problem

Create **Kohbor / เกาะเบาะ**, a Windows companion for Dota 2: permitted observations, timely audio cues, local questions, evidence review and optional coordination with consenting teammates.

Match attention is limited. Players need useful information without constant panel switching or intrusive narration. Stale information, false reassurance, gameplay interruption and unconsented context sharing undermine that value.

The intended value is better awareness, less interface effort and control over data, audio and recording. This hypothesis has no demonstrated win-rate effect or validated market-size, adoption, revenue or willingness-to-pay targets.

This PRD defines **why, for whom, what and priority**. The [SRS](../requirements/SRS.md) and registry define verifiable behavior; architecture, contracts and test plans define implementation and verification. No formal documentation-standard certification is claimed.

## 2. Stakeholders and responsibilities

| Stakeholder | Need or responsibility |
| --- | --- |
| Local player | Useful cues, honest states, local startup and control over interruption |
| Consenting teammate | Deliberate, limited sharing within the correct room/match/team |
| Returning player | Evidence-supported history and a reliable way to forget derived memory |
| Optional account user | Social/account capabilities independent of local gameplay |
| Media user/content owner | Verified event playback, local clips and authorized assets |
| Product owner | Decide scope, commercial policy, privacy changes and release acceptance |
| Engineering and validation owners | Implement contracts, report evidence and expose unresolved dependencies |

The owner selected the product name Kohbor / เกาะเบาะ. The agent develops its original visual identity and freezes the delegated technical identifiers while preserving behavior, data permissions, thresholds and scope.

## 3. Goals and user needs

Stable need IDs map to existing SRS requirements without broadening them. **contracts/requirements.json** retains the detailed requirement text.

| Need ID | User need and intended outcome | Existing requirement mapping |
| --- | --- | --- |
| PN-001 | Start and keep using available local functions while signed out or offline; configure game integration without damaging unrelated settings. | CORE-001, CORE-003, CORE-004, CORE-005 |
| PN-002 | Know which own-player and visible-enemy facts are current, supported and attributable; distinguish unknown from a real zero. | DATA-001, DATA-002, DATA-003, DATA-004, DATA-005, DATA-006, VISION-001, VISION-002, VISION-003, QUALITY-004 |
| PN-003 | Receive justified urgent warnings and corrections without cloud or renderer delays; avoid repeated or unsupported audio. | ALERT-001, ALERT-002, ALERT-003, ALERT-004, ALERT-005, ALERT-006, ALERT-007, QUALITY-001, QUALITY-005 |
| PN-004 | Hear and see consistent event media from the selected verified pack, with clear installation and activation status. | MEDIA-001, MEDIA-002, MEDIA-003, MEDIA-004, MEDIA-005, MEDIA-006, MEDIA-007, MEDIA-008 |
| PN-005 | Read the match field and passive overlay at a glance, inspect by choice and navigate accessibly without disrupting gameplay or performance. | UI-001, UI-002, UI-003, UI-004, UI-005, UI-006, CORE-002, QUALITY-002, QUALITY-003 |
| PN-006 | Hold a remappable key to ask a private local voice question; understand preparation/failure and yield to urgent audio. | VOICE-001, VOICE-002, VOICE-003, VOICE-004, VOICE-005, VOICE-006, VOICE-007, QUALITY-006, QUALITY-007 |
| PN-007 | Request useful manual advice while separately controlling whether a narrowly defined context may reach a cloud provider. | ADVICE-001, ADVICE-002, ADVICE-003, ADVICE-004 |
| PN-008 | Review supported local evidence and concise summaries, then clear derived memory without silently deleting separate history. | MEM-001, MEM-002, MEM-003, MEM-004, MEM-005 |
| PN-009 | Deliberately share narrow own-player vitals with consenting peers, see freshness and leave without stale information reappearing. | PEER-001, PEER-002, PEER-003, PEER-004, PEER-005, PEER-006, PEER-007, PEER-008, PEER-009 |
| PN-010 | Use optional friends and invitations without turning friendship, presence or sign-in into gameplay-sharing consent. | SOCIAL-001, SOCIAL-002, SOCIAL-003 |
| PN-011 | Explicitly record local highlights with truthful coverage and save status while recorder failure leaves local gameplay services available. | OBS-001, OBS-002, OBS-003, OBS-004, OBS-005, VISION-004 |
| PN-012 | Receive verified updates and predictable optional capability transitions without losing local core, map context or control. | UPDATE-001, UPDATE-002, UPDATE-003, UPDATE-004, PREMIUM-001, PREMIUM-002, PREMIUM-003, UI-007, QUALITY-008 |

## 4. Scope, features and priority

**P0** means required for full MVP in the current registry. Demo inclusion, extension scope and delivery completion are separate dimensions.

| Product scope | Included target | Boundary |
| --- | --- | --- |
| First demonstration | Local admission, observed visibility/risk, verified event media, missing-enemy notice, truthful interface and one recorded evidence match | Does not certify full conversation, all hero coverage, automated highlights or public release |
| Full MVP: awareness | Local core, admitted vision, deterministic alerts/correction, event announcer, overlay and shared base live field | No hidden enemy information, fabricated readiness or premium dependency |
| Full MVP: companion | Required local/manual advice, Thai/English local PTT, local memory/history and at most three evidence-linked summary points per completed session | Cloud advice is optional; its consent/projection rules remain mandatory if enabled |
| Full MVP: media and delivery | User-started OBS highlights, trusted update/recovery and supported-matrix evidence | Demo recording and automated-highlight acceptance have separate gates |
| Explicit extensions | Guest peer rooms; optional friends/presence/invitations | Separate contracts, consent and operational acceptance |
| Deferred capabilities | Premium authority and advanced inspector/timers, deep review, recovery coaching, multi-provider routing, tone presets, broadcast co-host, soundtrack, commerce/security-recovery services | No apparently working production controls backed by invented behavior |
| External dependencies | Media-authoring tool and optional account/catalog/payment services | Rebuilding separate products is not automatically desktop MVP scope |

Local action-damage analysis is excluded from the first demo. Full-MVP inclusion and exact patch/formula coverage remain pending **DEC-GAME-DATA**; neither mandatory nor optional status is silently assumed.

A logical navigation destination may remain visible with an honest deferred/unavailable state. Navigation presence does not expand delivery scope.

### Non-goals

Game-memory access, injection, automated gameplay, unseen enemy feeds, cloud microphone processing, cloud personal memory, default analytics, automatic recording/joining, implicit subscriptions, inherited-format compatibility, and unapproved additional platforms or public services are outside scope.

## 5. Product and business rules

1. Available local core works without an account, network or premium authority. Optional-service failure does not unlock forbidden data or disable unrelated valid functions.
2. Every displayed conclusion respects source admission, freshness and identity. Unknown, stale, invalid and estimated are meaningful states.
3. Risk is a labelled heuristic. A high score alone is insufficient evidence for an audible gank warning; corrections require the specified confirming evidence.
4. Cloud advice, peer sharing, microphone use and social presence have separate consent. Sign-in or friendship does not substitute.
5. Voice audio, transcript and interpretation stay local. Received peer information remains transient and cannot become AI or memory input.
6. Event media comes from the active verified package. Integrity, publisher trust, purchase entitlement and activation are separate decisions.
7. OBS recording is explicit and local. It neither enables a microphone nor replaces the critical vision source automatically.
8. Premium changes presentation only. Valid offline leases retain access; invalid authority returns to base quietly. Core alerts and data permissions never depend on tier.
9. Deferred commercial behavior has no price, subscription, payment-provider or deployment authorization supplied by this PRD.
10. Naming and styling may change independently of stable identifiers and approved behavior.

## 6. [UX goals](../design/UX-SPEC.md)

Maintain a separate control window and passive, click-through gameplay overlay. Present the map first, missing-enemy information second. The live field has five enemy slots, own hero, four allied slots and a permanently reserved right region.

Start neutrally: no fabricated roster, fake inventory or selected hero. Unknown-only skill/item rows remain hidden. The inspector opens through explicit selection and only presents admitted detail permitted by current capability authority. The local event timeline remains accessible at base.

Retain the fixed desktop-canvas baseline and bounded internal scrolling until a documented redesign decision. Keyboard focus must remain visible and stable; Escape closes detail before its parent. Thai typography, understandable status copy and non-color cues are required. Final original visuals, safe margins and measured scaled readability remain design work, not supplied approvals.

## 7. Product success and acceptance evidence

All new-build evidence currently begins **NOT_RUN**. The product succeeds when the intended tasks and their alternate paths work under the declared environment, not merely when screens exist.

| Outcome | Existing acceptance basis |
| --- | --- |
| Timely justified warnings | p50 ≤250 ms; every measured sample ≤300 ms; required quality/coverage and acoustic evidence |
| Low gameplay interference | Every one-second app-group CPU sample ≤10%; aggregate peak full Working Set ≤900,000,000 bytes; matched-pair FPS loss ≤3% |
| Trustworthy observation and conversation | Per-class vision and per-language voice gates in QUALITY-004 and QUALITY-006 |
| Reliable interruption | All required overlap trials preserve the critical deadline and cancel obsolete voice/advice |
| Understandable control | Journey evidence for startup, unknown/stale states, stop/mute/leave, consent withdrawal and forgetting |
| Honest certification | Exact supported OS/mode/geometry cells, native/live evidence, tested revision and artifact identity |
| Safe distribution | Authorized assets, verified installer/update/recovery and explicit release decision |

No commercial success metric is invented. Any later adoption, retention, satisfaction or revenue target needs its own definition, measurement plan and owner decision.

## 8. Constraints, assumptions and dependencies

Initial acceptance targets are Windows x64 builds 19045 and 26100; live 5v5 All Pick Ranked/Unranked; and calibrated 1920×1080, 16:9, 100% DPI, default bottom-left minimap in borderless fullscreen. These are test targets, not existing compatibility certification.

Local model feasibility within fixed budgets is unproven. Required dependencies include authorized media, verified game catalogs/classes, configured local devices, Windows/Dota test access and separately installed OBS. Optional network services need their own operational ownership. Full control-window viewport coverage does not imply vision support for every display geometry.

## 9. Risks and [open decisions](../governance/DECISION-LOG.md)

| Risk or uncertainty | Required decision/evidence |
| --- | --- |
| Local models exceed resource/latency budgets | DEC-MODELS and measured feasibility; no silent cloud fallback |
| Incomplete game/class/ownership evidence | DEC-GAME-DATA; keep unsupported conclusions unavailable |
| Unfrozen risk-admission details | DEC-RISK-DETAIL; informational score cannot substitute for audible-warning evidence |
| Missing media or package trust definition | DEC-CRITICAL-MEDIA and DEC-PACK-CRYPTO |
| Unmeasured original interface | DEC-VISUALS and native geometry/accessibility evidence |
| Ambiguous completion, retention or audio lifecycle | DEC-SESSION, DEC-RETENTION and DEC-VOICE-BUFFER |
| Optional service/commercial complexity | DEC-SOCIAL and DEC-PREMIUM; preserve local-only progress |
| Recorder's combined system cost | DEC-OBS-BUDGET and separate/combined measurements |

DEC-NAME and DEC-INTERFACES are early engineering decisions. DEC-NATIVE-EVIDENCE assigns real validation access. An open decision blocks its dependent scope, not unrelated work.

## 10. Review and approval

**Status: draft; no new product approval recorded.** Product scope and decision ownership require product review; requirement coverage requires engineering/QA review; changed data/security boundaries require their respective review. Record accepted changes against affected PN IDs and canonical requirement IDs. Approval of this PRD is distinct from completed implementation, runtime acceptance and release authorization.

