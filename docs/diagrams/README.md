# Diagram Index and Maintenance

**Version 0.3.1 · Draft · Application evidence NOT_RUN**

Open [INDEX.html](INDEX.html) for the offline searchable atlas, or open the SVG links below. Each editable Mermaid source has one generated SVG. The catalog identifies its purpose, governing document, requirement IDs and open decisions. Read the notes in the atlas/catalog with the image: short labels cannot express every guard or invariant.

These are tailored engineering views: system context, containers, deployment, data flow, logical ERD, sequences, states and user/release flows. They are not a claim to implement every UML diagram type or every C4 level. A missing detailed protocol is still an open decision even when its interaction is pictured.

## Diagram inventory

| ID | Diagram | View | Governing document | Editable source |
| --- | --- | --- | --- | --- |
| DGM-001 | [System context](rendered/DGM-001-system-context.svg) | flowchart | [SAD.md](../design/SAD.md) | [Mermaid](source/DGM-001-system-context.mmd) |
| DGM-002 | [Application containers and local storage](rendered/DGM-002-containers.svg) | flowchart | [SAD.md](../design/SAD.md) | [Mermaid](source/DGM-002-containers.mmd) |
| DGM-003 | [Deployment and resource accounting](rendered/DGM-003-deployment.svg) | flowchart | [SAD.md](../design/SAD.md) | [Mermaid](source/DGM-003-deployment.mmd) |
| DGM-004 | [Local data flow and lifetime separation](rendered/DGM-004-local-data-flow.svg) | flowchart | [DDD.md](../design/DDD.md) | [Mermaid](source/DGM-004-local-data-flow.mmd) |
| DGM-005 | [Independent external data routes](rendered/DGM-005-external-data-flow.svg) | flowchart | [SECURITY-PRIVACY.md](../security/SECURITY-PRIVACY.md) | [Mermaid](source/DGM-005-external-data-flow.mmd) |
| DGM-006 | [Local logical entity relationships](rendered/DGM-006-logical-erd.svg) | erDiagram | [DDD.md](../design/DDD.md) | [Mermaid](source/DGM-006-logical-erd.mmd) |
| DGM-007 | [Contained offline startup](rendered/DGM-007-startup-sequence.svg) | sequence | [SAD.md](../design/SAD.md) | [Mermaid](source/DGM-007-startup-sequence.mmd) |
| DGM-008 | [Evidence-gated native warning](rendered/DGM-008-alert-sequence.svg) | sequence | [SDD.md](../design/SDD.md) | [Mermaid](source/DGM-008-alert-sequence.mmd) |
| DGM-009 | [Local voice turn lifecycle](rendered/DGM-009-voice-lifecycle.svg) | state | [SDD.md](../design/SDD.md) | [Mermaid](source/DGM-009-voice-lifecycle.mmd) |
| DGM-010 | [Media installation and deferred activation](rendered/DGM-010-media-install-activation.svg) | flowchart | [MEDIA-PACK-SPEC.md](../design/MEDIA-PACK-SPEC.md) | [Mermaid](source/DGM-010-media-install-activation.mmd) |
| DGM-011 | [Consented peer sharing and leave sequence](rendered/DGM-011-peer-sequence.svg) | sequence | [ICD.md](../design/ICD.md) | [Mermaid](source/DGM-011-peer-sequence.mmd) |
| DGM-012 | [Peer membership lifecycle](rendered/DGM-012-peer-lifecycle.svg) | state | [SDD.md](../design/SDD.md) | [Mermaid](source/DGM-012-peer-lifecycle.mmd) |
| DGM-013 | [Match identity and completion lifecycle](rendered/DGM-013-match-lifecycle.svg) | state | [SDD.md](../design/SDD.md) | [Mermaid](source/DGM-013-match-lifecycle.mmd) |
| DGM-014 | [Evidence-gated alert latch and correction](rendered/DGM-014-alert-lifecycle.svg) | state | [SDD.md](../design/SDD.md) | [Mermaid](source/DGM-014-alert-lifecycle.mmd) |
| DGM-015 | [Durable forgetting without history deletion](rendered/DGM-015-memory-forget.svg) | sequence | [DDD.md](../design/DDD.md) | [Mermaid](source/DGM-015-memory-forget.mmd) |
| DGM-016 | [Explicit local recording and measured clips](rendered/DGM-016-recording-sequence.svg) | sequence | [SDD.md](../design/SDD.md) | [Mermaid](source/DGM-016-recording-sequence.mmd) |
| DGM-017 | [Startup, navigation and explicit user tasks](rendered/DGM-017-user-flow.svg) | flowchart | [UX-SPEC.md](../design/UX-SPEC.md) | [Mermaid](source/DGM-017-user-flow.mmd) |
| DGM-018 | [Exact-candidate release and safe update](rendered/DGM-018-release-update.svg) | flowchart | [RUNBOOK.md](../operations/RUNBOOK.md) | [Mermaid](source/DGM-018-release-update.mmd) |
| DGM-019 | [Own-source admission and freshness](rendered/DGM-019-source-freshness.svg) | state | [DDD.md](../design/DDD.md) | [Mermaid](source/DGM-019-source-freshness.mmd) |

## Authority and traceability

PRD/SRS and approved decisions govern the same baseline represented by these figures. A diagram cannot broaden data access, consent, scope or acceptance. A conflict with text is a defect to reconcile, not a choice between convenient interpretations. The [catalog](catalog.json) and [RTM](../requirements/RTM.csv) connect diagrams with requirements. Not every requirement needs a diagram; diagram coverage is not implementation or verification coverage.

The ERD is a logical model, not a frozen physical database schema. C4-style context/container views use generic Mermaid flowchart notation; a container denotes an executable application or data store, not necessarily Docker. Component responsibilities remain in SDD; context and deployment figures must not silently allocate new services.

## Update workflow

1. Resolve the relevant requirement/decision and edit the `.mmd` source plus catalog notes/mappings.
2. Regenerate SVGs with the pinned renderer; inspect readability and every failure/cancellation path affected.
3. Rebuild the atlas, regenerate SRS/RTM views if mappings change, and validate the package.
4. Commit source, catalog, rendered figures and updated traceability together. Do not hand-edit the SVG while leaving its source unchanged.

Renderer setup and commands, from the package root:

```bash
npm ci --prefix tools
node tools/render_diagrams.mjs
python tools/build_diagram_atlas.py
python tools/render_requirements.py
python tools/validate_spec.py
```

The rendering dependencies are documentation tools, not application runtime dependencies. The pinned Mermaid, jsdom and svgdom renderer runs locally under Node; it uses the unmodified DejaVu fonts bundled in `tools/fonts` with their license. Initial dependency installation requires npm access, but subsequent rendering requires no browser or render service. `--module-root` accepts an existing directory containing the pinned packages. Generated SVG and HTML require no JavaScript diagram engine or network to display the figures. The atlas's local navigation/search uses a small embedded script.

Text metrics come from the bundled fonts through svgdom rather than a browser layout engine. Keep the tested classic appearance and HTML labels disabled. After regeneration, rasterize the affected figures with an independent SVG viewer and inspect label spacing, line wrapping, box bounds and arrows; finite coordinates alone are not visual acceptance.

The render report checks syntax/output and hashes; it does not establish semantic correctness. Review the requirements and notes whenever a diagram changes. Native, live, acoustic and resource evidence remains NOT_RUN.

## Notation references

- [C4 diagram views](https://c4model.com/diagrams): choose the abstraction levels and supporting views that add value; this package does not claim formal C4 conformance.
- [Mermaid introduction](https://mermaid.js.org/intro/): text sources for editable diagrams. The package pins its renderer version in `tools/package.json` and keeps a generated SVG with every source.
