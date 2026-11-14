# Software Engineering Documentation Index

**Baseline 0.3.6 · Draft · index revised 2026-11-14 · application evidence NOT_RUN**

Start with PRD and SRS. Read the design, interface and verification documents for each implementation slice. Agent workflow is defined in root AGENTS.md and PROMPT.md and in [Agent Workflow](delivery/AGENT-WORKFLOW.md). To find who owns an area, what is decided and what is open there, read the [domain registry](domains/README.md) and the card of the domain you will touch (see "Domains" below).

This is the only index of documents. Every Markdown file in the tree, is linked below or is an artefact of a domain card; `tools/validate_domains.py` fails the gate when one is missing.

| Document | Engineering responsibility | Main reader |
| --- | --- | --- |
| [PRD](product/PRD.md) | Problem, users, 12 product needs, outcomes, priorities and scope | Product, engineering, QA |
| [Use Cases](product/USE-CASES.md) | End-to-end player actions, alternate/failure paths | Product, UX, QA |
| [Feature Catalog](product/FEATURE-CATALOG.md) | Capability inventory and existing algorithm elaborations | Product, engineering |
| [SRS](requirements/SRS.md) | Context, constraints and all 87 atomic software requirements | Engineering, QA |
| [RTM](requirements/RTM.csv) | Need → requirement → design → test → evidence linkage | Engineering, QA, reviewers |
| [SAD](design/SAD.md) | System context, architectural views, quality allocation | Architecture, engineering |
| [SDD](design/SDD.md) | Component allocation, algorithms, state, concurrency and failure | Engineering |
| [DDD](design/DDD.md) | Data definitions, provenance, ownership, persistence and erasure | Engineering, privacy |
| [ICD](design/ICD.md) | Interface boundaries, draft/frozen status and contract requirements | Integrating teams |
| [UX Specification](design/UX-SPEC.md) | Screens, state, interactions, layout and accessibility | UX, engineering, QA |
| [Game-State Intake Specification](design/GSI-INTAKE-SPEC.md) | M1 owned game configuration, loopback intake, admission, ownership, session and freshness clock | Engineering, integrating teams |
| [Event and Audio Specification](design/EVENT-AUDIO-SPEC.md) | M2 canonical event catalog, health-danger proposal, audio priority/preemption, outcome reasons, media decode checks and install contracts | Engineering, media, QA |
| [Visual Design Proposal](design/VISUAL-DESIGN-PROPOSAL.md) | DEC-VISUALS proposal: identity, tokens, Command Deck and overlay artboards | UX, engineering, product owner |
| [Media Package Specification](design/MEDIA-PACK-SPEC.md) | Media formats, trust, installation and playback lifecycle | Media, engineering, QA |
| [S3-5 Host Critical Warning Path](design/S3-5-HOST-CRITICAL-PATH.md) | Design and task breakdown of the host critical warning path (agent decision, not owner approved) | Engineering, media |
| [S6 Overlay Slice](design/S6-OVERLAY-SLICE.md) | Plan of the in-game banner overlay window, placement, honest chip states and the no-game demo trigger (agent plan, not owner approved) | Engineering, UX, product owner |
| [Package Format Review](design/PACKAGE-FORMAT-REVIEW.md) | Agent review: whether a package needs its own file type, signature and encryption options | Engineering, product owner |
| [S4 Observation Boundary](design/S4-OBSERVATION-BOUNDARY.md) | Sprint 4 no-hardware slice: capture admission, freshness, coverage and the enemy observation boundary (agent decision, not owner approved) | Engineering, QA |
| [Draft Bar](design/DRAFT-BAR-SPEC.md) | MVP-2 slice 5: reading the top hero bar during hero selection (mirror, hero identification, player names) with the third-party data contract (agent design, not owner approved) | Engineering, QA, Privacy |
| [Remaining System Design](design/REMAINING-SYSTEM-DESIGN.md) | Agent design for Sprint 4 onward: capture, risk, account, store and entitlement, peer relay (proposed) | Architecture, engineering |
| [Backend Stack](design/BACKEND-STACK.md) | Alpha/beta hosting stack (broker and relay on the owner's PC through a tunnel, Postgres-only Supabase, Pages front end), secrets, backups, home-PC risks and who creates what; base text by the codex-backend session (proposed, provider facts unverified) | Owner, engineering, operations |
| [Store and Pack Delivery](design/STORE-PACK-DELIVERY.md) | Agent design: catalog, entitlement, signed download, offline receipts and revocation, failure modes, alpha versus production, owner questions (proposed; nothing built, hosted or priced) | Architecture, engineering, product owner |
| [Account and Identity Review](design/ACCOUNT-IDENTITY-REVIEW.md) | Agent review behind DEC-ACCOUNT-IDENTITY and DEC-GID-REUSE: sign-in, friend code, privacy (proposed, not a human or legal review) | Engineering, product owner, privacy |
| [Sharing Model Review](design/SHARING-MODEL-REVIEW.md) | Agent review behind DEC-SHARE-LIST and DEC-SIGNIN-OPTIONAL: Share list, guests, same-match limits (proposed) | Engineering, product owner, privacy |
| [Command Deck design notes](design/visuals/command-deck/DESIGN-NOTES.md) | Reasons behind the Command Deck HTML artboards (proposal, DEC-VISUALS) | UX, engineering |
| [Audio and event-media UI notes](design/visuals/s3-audio-ui/DESIGN-NOTES.md) | Reasons behind the Sprint 3 overlay banner and audio panel artboards (proposal) | UX, engineering |
| [Security and Privacy](security/SECURITY-PRIVACY.md) | Admission, consent, data flow, authority and protection | Engineering, security/privacy |
| [NFR and Measurement](quality/NFR.md) | Quality budgets, reference environments and measurement definitions | Engineering, QA |
| [Test Plan](quality/TEST-PLAN.md) | Verification strategy, test procedures, evidence and gates | QA, engineering |
| [UAT Plan](quality/UAT-PLAN.md) | Player tasks and acceptance decisions | Product owner, testers |
| [Software Development Plan](delivery/SDP.md) | Delivery dependencies, milestone readiness and completion | Engineering lead |
| [Agent Workflow](delivery/AGENT-WORKFLOW.md) | Lead/worker roles, briefs, risk tiers, the acceptance gate, claims and handover | Engineering lead, every agent |
| [Sprint Roadmap](delivery/SPRINT-ROADMAP.md) | Sprint order, tasks, models and the backlog status table (the only task list) | Engineering lead, owner |
| [S3-1 Windows Native Audio Spike](delivery/S3-1-AUDIO-SPIKE.md) | Windows audio output library choice and what ran (silence only) | Engineering, media |
| [M1 Status](delivery/M1-STATUS.md) | Tauri host slice: containment, intake, runtime status, Overview UI; what ran | Engineering lead, owner |
| [M0 Plan and Status](delivery/M0-PLAN.md) | Frozen identity, scope matrix, toolchain lock, module map, local-core slice results and capture-spike procedure | Engineering lead, owner |
| [Operations Runbook](operations/RUNBOOK.md) | Installation, configuration, diagnostics, release and recovery | Maintainers, operators |
| [Document Control](governance/DOCUMENT-CONTROL.md) | Authority, baseline versions, review and change control | All contributors |
| [Decision Log](governance/DECISION-LOG.md) | Delegated choices and unresolved dependencies | Product, engineering |
| [Risk Register](governance/RISK-REGISTER.md) | Delivery/technical uncertainty, mitigation and closure evidence | Product, engineering, QA |
| [Implementation Readiness](governance/IMPLEMENTATION-READINESS.md) | Concrete remaining design work, safe starting points and evidence limits | Implementing agents, engineering, reviewers |
| [Agent Registry, Router and Enforcement](governance/AGENT-REGISTRY.md) | Profiles, roles, task routes with the owner-evidence level of each, domain policies, who is working now, and what is enforced and what is only reported (agent proposal; source: `AGENT-REGISTRY.json`) | Engineering lead, owner, every agent |
| [Agent Registry Design](governance/AGENT-REGISTRY-DESIGN.md) | Review of the first registry draft, the final design (registry schema, router, taxonomy, enforcement ladder) and the implementation plan P1 to P7 (P1 to P6 implemented) | Engineering lead, owner |
| [Progress Map](governance/PROGRESS-MAP.md) | Generator `tools/progress_map.py`: one offline HTML mind map of every area and its derived status, the status rules, sources and limits (agent tool, not human reviewed) | Owner, engineering lead, every agent |
| [Glossary](governance/GLOSSARY.md) | Requirement, module, component, domain, crate, write area and claim, the primary-domain rule and the generated map | All contributors |
| [Changelog](governance/CHANGELOG.md) | What changed in the documentation baseline | Reviewers |

## Domains

[Domain registry](domains/README.md): who owns which write areas, which decisions and requirements belong to them, and where the domain card is. A card is a map, not evidence: the authority order is unchanged (decisions, PRD/SRS, contracts, code and tests, then the card). Work in progress is tracked outside the repository in the coordination ledger described in the registry.

## Root files and templates

| File | Role |
| --- | --- |
| [README](../README.md) | Thai overview and reading order of the package |
| [GUIDE-TH](../GUIDE-TH.md) | Thai guide to the documents |
| [AGENTS](../AGENTS.md) | Instructions every agent follows; routing to this index and the domain cards |
| [PROMPT](../PROMPT.md) | Prompt that started the implementation agent |
| [ADR template](../templates/adr.md) | Decision record template |
| [Feature specification template](../templates/feature-spec.md) | Feature specification template |

## Diagram views

[Diagram index](diagrams/README.md) and [offline visual atlas](diagrams/INDEX.html) contain 19 controlled views: context, containers, deployment, data flow, ERD, sequence, state and user/release flows. Each has an editable Mermaid source, generated SVG, governing document, requirement IDs and limitations.

## Supporting artifacts

The [structured requirement registry](../contracts/requirements.json) and [product-needs registry](../contracts/product-needs.json) represent the same baseline as PRD/SRS. They support generation and checks. [Schemas](../contracts/), [fixtures](../fixtures/), [acceptance designs](../acceptance/scenarios.feature) and the [evidence template](../acceptance/evidence-template.json) support implementation and verification; none records a successful new-app execution.

## Traceability and current gaps

RTM has one row per software requirement, including upstream need IDs and downstream component IDs, design/contract references, acceptance scenario and test/UAT procedure IDs. The implementation column is filled slice by slice from `implementation_refs` in the requirement registry (not every requirement has code yet). The evidence column stays empty until a receipt bound to an exact revision and artifact hash exists. Filter by either need, component or test to trace backward as well as forward.

The [decision log](governance/DECISION-LOG.md) and [maturity table](governance/DOCUMENT-CONTROL.md) state where protocol, UX, model or operational detail is not yet ready. Document structure makes those gaps visible; it does not resolve them by itself.
