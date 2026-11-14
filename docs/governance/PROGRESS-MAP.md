---
title: "Progress Map"
doc_id: "PROGRESS-MAP"
version: "0.2.0"
status: "draft"
updated: "2026-11-14"
owner: "Governance lead (integration owner)"
approval_status: "not_approved"
---

# Progress Map

`python tools/progress_map.py` reads the real project sources and writes **one self-contained HTML dashboard ("Mission Control")**: a mind map of every area of Kohbor and its status, with six views. It is a generator: nothing on the page is typed by hand, and the page is a snapshot that can go stale. Agent tool, not reviewed by a human; it defines no product requirement.

```text
python tools/progress_map.py [--out FILE] [--open] [--github] [--check] [--json] [--now ISO] [--stale-hours N]
                             [--repo DIR] [--coord DIR] [--design-dir DIR] [--builds-dir DIR]
```

Default output `F:\kohbor-local\progress\progress-map.html` (outside the repository, never committed). The page needs no network, font, script or image from outside; it follows `prefers-color-scheme`. `--github` also reads pull requests with the `gh` CLI and is optional. `--check` exits 1 when the existing file was built from another commit than HEAD. The tool never writes inside the repository.

## Views

The look is a dark glass dashboard (teal and purple accent, a thin collapsible icon rail, pill navigation, rounded cards, a monospace status line); it is dark by default and has a light palette behind the theme button (the system colour preference is not used, so the page looks the same on every machine). It is original inline CSS, JavaScript and SVG: no chart library, font or script is loaded, and no code of any reference template is copied. The template lives next to the tool in `tools/progress_map_template.html`; the model it draws is built by `tools/progress_map.py`, and the view is chosen with the URL hash (`#overview`, `#map`, `#board`, `#decisions`, `#agents`, `#builds`).

| View | Shows | Data |
| --- | --- | --- |
| Overview | four stat cards (requirements verified of total and how many are only built; areas with active work; decisions that need the owner; active claims and how many are past expiry), status by area, the distribution, requirements by milestone, What next, Needs the owner | the tree, owner list, claims |
| Mind map | the collapsible node tree and a details panel with the rule that fired, files, evidence, open decisions and claims | the tree |
| Roadmap | read-only columns per status with one card per area (item count, P0 count, the first items); a click opens the area in the mind map | the counted leaves |
| Decisions | every record of `contracts/decisions.json` with a filter (open or provisional or proposed, decided, deferred, all), an owner-only switch, a search and the question, closure and safe interim | decisions |
| Agents & work | claims with domain, model, role, branch, state and hours to expiry (measured at generation time), merged commits per day, recent merges from `git log` (offline), registry worktrees, pull requests with `--github` | ledger, git |
| Builds | every `.exe` under the builds folder with size, time, SHA-256 and the release receipt that names that hash, the receipt's commit and whether it is in `origin/main`, its gate line, and the lines that say what was NOT done | builds folder, receipts |

## Sources (each may be missing; a missing source is listed on the page as not read)

| Source | Gives |
| --- | --- |
| `contracts/requirements.json` (fallback `docs/requirements/RTM.csv`) | requirement id, title, module, priority, milestone, implementation, evidence, open decisions, design refs |
| `contracts/decisions.json` | decisions that are open, proposed, provisional or deferred, their owner role and the question |
| `docs/domains/*/state.json` and `CARD.md` section 3 | domains, and the Now / Next / Blocked lines |
| Coordination ledger `claims/` and `registry/branches.json` (`--coord`, `KOHBOR_COORD_DIR`) | active work, who, branch |
| `docs/delivery/SPRINT-ROADMAP.md` | phases and the Backlog status table |
| `docs/design`, `docs/delivery`, `docs/operations`, and `--design-dir` folders (`KOHBOR_DESIGN_DIR`, default `F:\kohbor-design\*`) | design documents that exist |
| git | HEAD, branch, distance from `origin/main`, recent merged titles, and whether a receipt's commit is in `origin/main` |
| Owner builds folder (`--builds-dir`, `KOHBOR_BUILDS_DIR`, default `F:\kohbor-local`) and `RELEASE-RECEIPT*.md` in the design folders | executables, hashed by the generator (never in the browser), matched to a receipt by SHA-256 only |
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

**Builds.** A build is matched to a receipt only when its SHA-256 equals a hash written in that receipt; a file no receipt names is shown as unknown ("no receipt for this hash"), and a receipt names no verification beyond what it says: every receipt so far records build and gate only, so the page lists its NOT-run lines and never marks a build verified. The commit state is read offline: "in origin/main", "not in origin/main", or "commit not found in this checkout".

Known limits: only 25 RTM rows have `implementation_refs` today and none has `evidence_refs`, so nothing can be Verified yet and most built work shows as Built, not verified; the Backlog table is prose written by the lead; a card or backlog line can be wrong. The map shows what the files say, not what a person would conclude from running the application.
