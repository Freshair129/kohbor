---
title: "Progress Map"
doc_id: "PROGRESS-MAP"
version: "0.1.0"
status: "draft"
updated: "2026-11-14"
owner: "Governance lead (integration owner)"
approval_status: "not_approved"
---

# Progress Map

`python tools/progress_map.py` reads the real project sources and writes **one self-contained HTML mind map** of every area of Kohbor and its status. It is a generator: nothing on the page is typed by hand, and the page is a snapshot that can go stale. Agent tool, not reviewed by a human; it defines no product requirement.

```text
python tools/progress_map.py [--out FILE] [--open] [--github] [--check] [--json] [--now ISO] [--stale-hours N]
                             [--repo DIR] [--coord DIR] [--design-dir DIR]
```

Default output `F:\kohbor-local\progress\progress-map.html` (outside the repository, never committed). The page needs no network, font, script or image from outside; it follows `prefers-color-scheme`. `--github` also reads pull requests with the `gh` CLI and is optional. `--check` exits 1 when the existing file was built from another commit than HEAD. The tool never writes inside the repository.

## Sources (each may be missing; a missing source is listed on the page as not read)

| Source | Gives |
| --- | --- |
| `contracts/requirements.json` (fallback `docs/requirements/RTM.csv`) | requirement id, title, module, priority, milestone, implementation, evidence, open decisions, design refs |
| `contracts/decisions.json` | decisions that are open, proposed, provisional or deferred, their owner role and the question |
| `docs/domains/*/state.json` and `CARD.md` section 3 | domains, and the Now / Next / Blocked lines |
| Coordination ledger `claims/` and `registry/branches.json` (`--coord`, `KOHBOR_COORD_DIR`) | active work, who, branch |
| `docs/delivery/SPRINT-ROADMAP.md` | phases and the Backlog status table |
| `docs/design`, `docs/delivery`, `docs/operations`, and `--design-dir` folders (`KOHBOR_DESIGN_DIR`, default `F:\kohbor-design\*`) | design documents that exist |
| git | HEAD, branch, distance from `origin/main`, recent merged titles |
| newest cached `kohbor-gate-*.log` | last gate lines, labelled as possibly not for this commit |

## Status rules

Applied in this order to each requirement; the first that holds wins. The page shows the rule that fired as "why" in the side panel.

| Status | Rule |
| --- | --- |
| Verified | `evidence_refs` is not empty **and** `new_build_status` is `PASS` (the registry's own verdict that every required evidence class passed) |
| Built, not verified | at least one `implementation_refs` file exists on disk, or evidence exists but is not `PASS`. A listed file that is not on disk does not count |
| In progress | no implementation yet, and an active claim whose write area covers a planned implementation file or whose task or note names the requirement id |
| Needs the owner | no implementation, and an open owner-only decision named in `open_decisions` (status open, proposed or provisional and owner role containing "product owner") |
| Designed only | no implementation; a design, contract or spec file named by the requirement exists |
| Not started | the requirement exists but none of its design files exist |
| Unknown | no data at all. Never shown as done |

An active claim over files that already exist does not demote a built requirement: it stays built and gets a marker. Other leaves: a **decision** is Needs the owner when open, proposed or provisional with an owner-only role, Not started when deferred or resolvable by a non-owner role, and omitted when decided, except in an area that has no requirement rows (draft bar, backend hosting, banner lab, governance), where a decided decision is drawn as Designed only ("a decision, nothing built from it yet"); a decision with an exact id listed in an area's `decision_ids` goes to that area before any pattern is tried; a **backlog task** is Built (gate or review passed, never live) when its status says passed or done, Unknown when the status is not understood; an **active claim** is In progress; a **design document** is Designed only (a draft copy outside the repository is skipped when the repository has a document with the same name); a **card line** is Needs the owner or Not started (Next, Blocked) or Unknown (Now, Not verified), because a card is a map and can be stale.

**Roll-up.** A group, area or the root takes the most common status among the leaves below it; on a tie the less advanced status (rank: verified, built, in progress, designed, not started, needs the owner, unknown). Every node also shows a bar of its children by status. Documents and card lines are drawn but not counted when an area has requirement or task leaves.

**What next** lists requirements that are not verified: P0 first, then earliest milestone, then unblocked before owner-blocked, then id. **Needs the owner** is a separate list of open owner-only decisions, earliest milestone first, then those that block most requirements, with the question text and what closes it.

**Area taxonomy** (which module, domain, decision or document is drawn under which area) is configuration in the `AREAS` table of the tool. It places things; it never says how far along anything is. Anything it cannot place goes to an Unassigned area, never nowhere.

## Staleness and limits

The page shows "generated at TIME from commit SHA" and a red STALE banner when it is older than `--stale-hours` (default 6). A static file cannot observe a newer commit, so run `--check` or regenerate. It also warns when its checkout is behind `origin/main`.

Known limits: only 25 RTM rows have `implementation_refs` today and none has `evidence_refs`, so nothing can be Verified yet and most built work shows as Built, not verified; the Backlog table is prose written by the lead; a card or backlog line can be wrong. The map shows what the files say, not what a person would conclude from running the application.
