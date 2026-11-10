# Software Engineering Documentation Index

**Baseline 0.3.2 · Draft · 2026-11-11 · application evidence NOT_RUN**

Start with PRD and SRS. Read the design, interface and verification documents for each implementation slice. Agent workflow is separately defined in root AGENTS.md and PROMPT.md.

| Document | Engineering responsibility | Main reader |
| --- | --- | --- |
| [PRD](product/PRD.md) | Problem, users, 12 product needs, outcomes, priorities and scope | Product, engineering, QA |
| [Use Cases](product/USE-CASES.md) | End-to-end player actions, alternate/failure paths | Product, UX, QA |
| [Feature Catalog](product/FEATURE-CATALOG.md) | Capability inventory and existing algorithm elaborations | Product, engineering |
| [SRS](requirements/SRS.md) | Context, constraints and all 85 atomic software requirements | Engineering, QA |
| [RTM](requirements/RTM.csv) | Need → requirement → design → test → evidence linkage | Engineering, QA, reviewers |
| [SAD](design/SAD.md) | System context, architectural views, quality allocation | Architecture, engineering |
| [SDD](design/SDD.md) | Component allocation, algorithms, state, concurrency and failure | Engineering |
| [DDD](design/DDD.md) | Data definitions, provenance, ownership, persistence and erasure | Engineering, privacy |
| [ICD](design/ICD.md) | Interface boundaries, draft/frozen status and contract requirements | Integrating teams |
| [UX Specification](design/UX-SPEC.md) | Screens, state, interactions, layout and accessibility | UX, engineering, QA |
| [Game-State Intake Specification](design/GSI-INTAKE-SPEC.md) | M1 owned game configuration, loopback intake, admission, ownership, session and freshness clock | Engineering, integrating teams |
| [Media Package Specification](design/MEDIA-PACK-SPEC.md) | Media formats, trust, installation and playback lifecycle | Media, engineering, QA |
| [Security and Privacy](security/SECURITY-PRIVACY.md) | Admission, consent, data flow, authority and protection | Engineering, security/privacy |
| [NFR and Measurement](quality/NFR.md) | Quality budgets, reference environments and measurement definitions | Engineering, QA |
| [Test Plan](quality/TEST-PLAN.md) | Verification strategy, test procedures, evidence and gates | QA, engineering |
| [UAT Plan](quality/UAT-PLAN.md) | Player tasks and acceptance decisions | Product owner, testers |
| [Software Development Plan](delivery/SDP.md) | Delivery dependencies, milestone readiness and completion | Engineering lead |
| [Operations Runbook](operations/RUNBOOK.md) | Installation, configuration, diagnostics, release and recovery | Maintainers, operators |
| [Document Control](governance/DOCUMENT-CONTROL.md) | Authority, baseline versions, review and change control | All contributors |
| [Decision Log](governance/DECISION-LOG.md) | Delegated choices and unresolved dependencies | Product, engineering |
| [Risk Register](governance/RISK-REGISTER.md) | Delivery/technical uncertainty, mitigation and closure evidence | Product, engineering, QA |
| [Implementation Readiness](governance/IMPLEMENTATION-READINESS.md) | Concrete remaining design work, safe starting points and evidence limits | Implementing agents, engineering, reviewers |
| [Changelog](governance/CHANGELOG.md) | What changed in the documentation baseline | Reviewers |

## Diagram views

[Diagram index](diagrams/README.md) and [offline visual atlas](diagrams/INDEX.html) contain 19 controlled views: context, containers, deployment, data flow, ERD, sequence, state and user/release flows. Each has an editable Mermaid source, generated SVG, governing document, requirement IDs and limitations.

## Supporting artifacts

The [structured requirement registry](../contracts/requirements.json) and [product-needs registry](../contracts/product-needs.json) represent the same baseline as PRD/SRS. They support generation and checks. [Schemas](../contracts/), [fixtures](../fixtures/), [acceptance designs](../acceptance/scenarios.feature) and the [evidence template](../acceptance/evidence-template.json) support implementation and verification; none records a successful new-app execution.

## Traceability and current gaps

RTM has one row per software requirement, including upstream need IDs and downstream component IDs, design/contract references, acceptance scenario and test/UAT procedure IDs. The implementation and evidence columns are intentionally empty until the builder has actual code and receipts. Filter by either need, component or test to trace backward as well as forward.

The [decision log](governance/DECISION-LOG.md) and [maturity table](governance/DOCUMENT-CONTROL.md) state where protocol, UX, model or operational detail is not yet ready. Document structure makes those gaps visible; it does not resolve them by itself.
