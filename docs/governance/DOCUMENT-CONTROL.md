---
title: "Document Control and Baseline Governance"
doc_id: "DOCUMENT-CONTROL"
status: draft
version: "0.3.4"
updated: "2026-11-11"
owner_role: "Product owner and engineering lead; people not assigned"
approval_status: "not_approved"
---

# Document Control and Baseline Governance

## 1. Status and intended use

This package specifies **Kohbor / เกาะเบาะ**, the desktop companion name selected by the owner. It is a reviewable extracted baseline, not approval of all requirements, an exhaustive reverse-engineering certification or an implementation receipt. No application has been built or run from this package. All new-application verification begins **NOT_RUN**.

DEC-BASELINE identifies the normalized target extracted from supplied reference material reviewed on 2026-11-10, including reference snapshot `0f7ccd768bf18faa7dca36256470008c2a5f9c81`. A local-recording owner issue was also reviewed that day; mutable issue text is outside that snapshot. The package removes source product/module branding, links and assets. It supplies no existing binary, private signing key or inherited compatibility promise.

Version 0.2.0 makes the product, requirements, design and verification responsibilities explicit. It preserves the 85 existing requirement IDs and their behavior, scopes, acceptance predicates and evidence status. It adds product-need traceability and document context. It does not close an open model, protocol, visual-design or operational decision by renaming a document.

## 2. Documents and semantic authority

1. Explicit decisions authorized by the current owner govern changes to this new product. Record the decision, affected need/requirement IDs and approval that actually occurred.
2. The [PRD](../product/PRD.md) governs product purpose, user needs, scope, priority and outcomes. The [SRS](../requirements/SRS.md) governs the corresponding verifiable software obligations. These are parts of one versioned baseline, not competing specifications.
3. The SAD, SDD, ICD, DDD, UX, security/privacy and NFR documents elaborate those obligations within their stated responsibility. A frozen design cannot add a new product permission or relax a requirement.
4. Test plans, fixtures, prototypes, source code and agent instructions implement or verify the baseline; they cannot independently change it.

Accepted decisions must be incorporated into every affected artifact. A contradiction between a product need, SRS row, normative design elaboration or contract is a baseline defect: stop the affected integration and reconcile it. Unrelated work can continue. Do not use this ordering to silently discard the inconvenient side of a contradiction.

### Structured representations

[requirements.json](../../contracts/requirements.json) is the structured authoring representation of the **same** atomic requirements embedded in the SRS. It is not a higher authority than the human document. The rendering tool updates only the marked atomic section and generates [RTM.csv](../requirements/RTM.csv) plus [Gherkin designs](../../acceptance/scenarios.feature). The handwritten SRS context remains in SRS.md. A mismatch fails package validation.

[product-needs.json](../../contracts/product-needs.json) mirrors the stable PRD need IDs and requirement mappings. [decisions.json](../../contracts/decisions.json) records the open-decision data used with the decision log. These records must agree with their human documents.

The requirements registry format is 1.3.0: 1.2.0 added diagram references and 1.3.0 adds the planning field `target_milestone`. Version 0.3.2 changes one runtime schema incompatibly (own-snapshot 1.0.0 → 2.0.0, per-field availability) and adds runtime-status, user-settings and settings-patch 1.0.0; no application has shipped, so no reader of the retired version exists. Version 0.3.3 changes event.schema incompatibly (1.0.0 → 2.0.0: `instance_id`, `session_id`, catalog-bound `event_type`, closed payload additions), advances media-manifest to 1.1.0 (optional `identity_audio_path`) and adds event-catalog, audio-outcome and media-install request/result 1.0.0. Version 0.3.4 adds the DEC-VISUALS proposal and contracts/design-tokens.json 0.1.0 without changing runtime schemas. The evidence-support schema pins this package's spec version, 0.3.4; this is a baseline binding, not a new application wire protocol.

## 3. Controlled vocabulary

| Term | Meaning |
| --- | --- |
| MUST / SHALL | Required for the requirement's stated scope |
| SHOULD | Recommendation; an evidenced alternative may meet the same obligation |
| MAY | Delegated choice within constraints |
| inherited target | Requirement extracted from reference decisions; new implementation still needs evidence |
| proposed normalization | Neutral field, enum or protocol shape awaiting the named freeze decision |
| delegated decision | Engineering may choose within existing product constraints and record rationale |
| observed | Supplied by an admitted source at a recorded time |
| inferred / estimated | Derived by a named method; never presented as directly observed |
| unknown / stale | No admitted value / prior value no longer current; neither means zero or safe |
| complete session | Supported start, end and outcome under the finalized completion contract |
| base / premium | Presentation capabilities; neither grants additional data or consent |
| certified configuration | Exact configuration with recorded qualifying evidence |

Requirement status, implementation status, execution result and owner approval are separate. Evidence statuses are NOT_RUN, PASS, FAIL, SKIP and INDETERMINATE. SKIP and INDETERMINATE do not satisfy a mandatory gate. A requirement can be mandatory while its implementation is absent.

## 4. Change control

For a behavioral change record: decision ID, date, decision-maker, reason, alternatives, PN/SRS IDs, design/contracts/tests affected, migration/rollback impact and actual approval status. Update the PRD/SRS baseline and structured counterparts together; regenerate views and validate links, mappings and fixtures. Preserve deprecated IDs and keep a changelog.

For frozen API/event contracts, incompatible semantic changes require a major version; compatible additions require minor; explanatory corrections may be patch. An expanded data/consent boundary requires a new explicit decision even if wire-compatible. Never invent `approved_by`, an approval date or a result receipt.

Before approving a baseline, review product scope, requirement completeness/testability, architecture feasibility, interface maturity and verification coverage. Assign actual reviewers when available. A document review does not authorize public release, prove native behavior or approve unresolved scope.

## 5. Readiness and open work

| Area | Current maturity | What must happen before affected implementation/acceptance |
| --- | --- | --- |
| PRD/SRS and traceability | Draft, internally checkable | Product/engineering review and controlled correction of gaps |
| Game-state intake (M1) | Proposed specification and schemas | DEC-GSI-INTAKE capture-spike confirmation and owner acknowledgement |
| Events and audio (M2) | Proposed specification, catalog and schemas | DEC-EVENT-AUDIO review (mute semantics acknowledged and DEC-HEALTH-DANGER decided 2026-11-11) |
| Interfaces and media trust | Draft contracts, partial schemas | DEC-INTERFACES and DEC-PACK-CRYPTO protocol freeze |
| Original UX | Behavioral/layout specification | DEC-VISUALS annotated designs and measured Windows evidence |
| Local models, game catalogs and risk evidence | Requirements present; artifacts not qualified | Named model/data/risk decisions and held-out evidence |
| Voice buffers, completion and retention | Constraints specified; design choices open | DEC-VOICE-BUFFER, DEC-SESSION, DEC-RETENTION |
| Optional services/commercial authority | Extension/deferred boundaries | Explicit contract, service ownership and authorization |
| App validation and distribution | NOT_RUN, not approved | Required native/live/acoustic/resource receipts and release decision |

The 85 rows are the registered inventory, not a proof that every possible source behavior has been extracted. Design annexes elaborate these rows; review any unmatched normative clause as a coverage gap before claiming completeness. Do not expand scope to resolve a documentation ambiguity.

## 6. Reading conventions and standards

PRD, SRS, SAD/SDD, interface/data design, test/UAT plans and traceability are deliberately tailored document roles. The directory names and Markdown format are project choices. This package is informed by conventional requirements and architecture documentation practice; it does not assert a formal standards-conformance assessment.

Schemas provide neutral shapes and closed allowlists, not complete production protocols. Authentication, duplicate-key rejection, bounds, canonicalization, races, clocks and semantic comparisons also need design and executable tests. Read the [ICD](../design/ICD.md), [decision log](DECISION-LOG.md) and [risk register](RISK-REGISTER.md) before freezing an affected interface.

Where information is incomplete, resolve the named decision into a concrete neutral specification. No original repository or hidden historical document is required to understand an obligation already stated here.

## Diagram control in version 0.3.1

The diagram catalog owns view IDs, purposes, document/requirement mappings and guard notes; `.mmd` files own editable figures. SVG and atlas HTML are generated views and must match source hashes. Each diagram describes the same draft baseline, not a new approved requirement or permission. Keep source, image, notes and RTM consistent; review the governing text whenever an arrow, branch or transition changes. Render success is not semantic correctness or application evidence.
