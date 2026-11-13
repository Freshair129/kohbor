---
title: "Glossary: how work is divided"
doc_id: "GLOSSARY"
status: draft
version: "0.2.0"
updated: "2026-11-13"
authority: "Process only; explains existing terms and defines no requirement"
owner_role: "Governance lead (integration owner)"
approval_status: "not_approved"
---

# Glossary: how work is divided

Several words divide the work in different ways. They do not nest cleanly. The mapping is generated, never stored twice: `python tools/validate_domains.py --map` (`--json` for tools). The design is in [AGENT-REGISTRY-DESIGN](AGENT-REGISTRY-DESIGN.md) section 2.5.

| Term | What it is | Source of truth | Example |
| --- | --- | --- | --- |
| **Requirement** | One atomic software obligation; its id is the spine of the mapping | `contracts/requirements.json` | `MEDIA-004` |
| **Module** (requirement module) | The group a requirement belongs to; the prefix of its id | `module` in the requirement registry | `media` |
| **Component** | A design component that requirements are allocated to | `### CMP-...` in [SDD](../design/SDD.md); `primary_design_component_id` | `CMP-MEDIA` |
| **Domain** | A set of write areas with its own decisions and documents; the unit a session claims and a card describes | [Domain Registry](../domains/README.md), `docs/domains/<id>/state.json` | `packs` |
| **Crate** | A Rust package under `crates/` | The folder; `crates` and `planned_crates` in one card's `state.json` | `kohbor-pack` |
| **Write area** | Repository paths (or a logical root such as `lab:`) that one domain owns | `write_areas` in `state.json` | `crates/kohbor-pack/**` |
| **Claim** | A dated statement that one session fills a role in given write areas for one task on one branch from a base commit | One file in the coordination ledger | `packs__S9.json` |

Also used: a **role** is a job filled for one task; a **profile** is the tool, model and effort that fills it; a **route** is a task type and domain mapped to a role and profile; a **steward** is the lead that holds a domain's claim. Five files are called a registry: the requirement registry, the decision registry (`contracts/decisions.json`), the domain registry, the [agent registry](AGENT-REGISTRY.md) and the coordination registry (`registry/branches.json` in the ledger).

## The mapping rule

1. **Primary domain of a requirement.** The one card that lists it in `requirement_ids`; else the one row of the domain registry whose `Default modules` names its module or its id; else UNASSIGNED. Two candidates are a CONFLICT. Both are reported, neither fails the gate.
2. **Interface requirements.** A card that depends on a requirement owned by another domain lists it in `interface_requirement_ids`. A dangling id there fails; a requirement that is primary in the same card is reported.
3. **Module and component per domain are derived** from the requirements the domain is primary for. A module or component that serves several domains is normal and is printed, not flagged. Only `Default modules`, `interface_requirement_ids` and `crates` are stored.
4. **Crate to domain: one to one.** Each crate is named by exactly one card (`crates`, or `planned_crates` while it does not exist). A crate that no card names, one that two cards name, and a planned crate that now exists are reported.
5. **Tracked file to domain.** A file belongs to the domain whose write areas match it. The map prints how many tracked files have one owner, none and several, and the folders with no owner. Filling gaps is card work, done by the lead that touches the domain.
6. **Claim to write area.** A claim names a subset of the write areas of the domains it touches, on one branch, from a base commit. `python tools/check_claim_scope.py` reports changed paths outside it. Shared integration files (manifest, RTM, SRS atomic section, validation report, INDEX, changelog) are written by every lead on its own branch and are exempt (Agent Workflow section 9.1).

## State on 2026-11-13 (measured, will change)

87 requirements: 47 primary by card, 40 by default module, 0 conflict, 0 unassigned. 441 tracked files: 235 owned by one write area, 204 by none (diagrams, `apps/desktop` renderer files outside the host areas, `spikes/audio`, design documents, contract schemas and root files), 0 by several.
