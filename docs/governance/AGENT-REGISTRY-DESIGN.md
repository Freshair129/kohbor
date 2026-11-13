---
title: "Agent Registry, Router, Taxonomy and Enforcement: review and final design"
doc_id: "AGENT-REGISTRY-DESIGN"
status: draft
version: "0.2.0"
updated: "2026-11-13"
authority: "Process only. Design and plan written in one design turn at the owner's instruction (relayed 2026-11-13); an agent design, not owner approval. Defines no product requirement"
owner_role: "Governance lead (integration owner)"
approval_status: "not_approved"
---

# Agent registry, router, taxonomy and enforcement: review and final design

**Implementation status (2026-11-13, commits `9ea6049` to P6):** P1 to P6 of section 4 are implemented and the first draft is replaced; P7 (an independent review of the tools diff) was done on 2026-11-13 by Terra at the owner's choice: see section 7. Section 1 describes the first draft, sections 2 and 3 are the design, and sections 4 to 6 are kept as the record. Where the implementation differs from the text: a domain policy also carries `allowed_task_types` (the lab uses only lab task types; `commerce` and `local-voice` also allow `integrate` so the lead can write the card back); the map lives in `validate_domains.py --map`; the brief template of Agent Workflow section 4 is unchanged until decision 8; `--sprint-exit` in the gate is not written.

**What this is.** A critical review of the first draft (commits `92bd2cf`, `4334d35`: [AGENT-REGISTRY](AGENT-REGISTRY.md), `AGENT-REGISTRY.json`, [GLOSSARY](GLOSSARY.md), `tools/validate_agents.py`, `tools/check_claim_scope.py`, `tools/hooks/pre-push.sample`), the final design, and the implementation plan. **Nothing in sections 2 to 4 is implemented.** The first draft stays in the tree, report-only, until the plan in section 4 replaces it.

Labels: **VERIFIED** = checked today in a file or by running a read-only command. **UNVERIFIED** = not checked. **PROPOSED** = this document's choice, inside the lead's delegation unless listed in section 5.

## 1. Review of the first draft

Evidence was gathered read-only: the draft's own tools were run with queries, the registries were counted with a throwaway script, and the sources the draft cites were re-read.

| id | Severity | Defect | Evidence (VERIFIED) | Disposition |
| --- | --- | --- | --- | --- |
| D1 | major | Approval is attached to the role, so a route nobody chose reads as owner-approved | `--route governance-doc --domain governance` answers `usable_without_owner: true`; no owner record chooses "the lead writes governance documents itself" | Redo: approval on the route; effective level = weakest of route and profile |
| D2 | major | The level `owner_documented_tentative` overstates the owner's part | The banner-lab guidance is agent-written and calls itself tentative after one trial per model | Redo: `in_use_unconfirmed`, with source, date and whether the instruction was relayed |
| D3 | major | One model per role; the lead is "session model" | The owner now routes by phase: design and planning on Fable 5.2 at xhigh, implementation on Sonnet 5.5 (relayed 2026-11-13). The draft cannot express it | Redo: a profile (tool, model, effort) per route |
| D4 | major | "Reviewer differs in model from author" compares ids | Lead (`session model`) and consultant (`claude subagent`) pass although both may be the same model | Redo: UNVERIFIABLE unless both models are concrete; the real check reads run records |
| D5 | major | Restrictions with no source | `domain_notes.account` says no worker may implement until the owner authorises the build; the account card lists next steps that need no owner. Worker `allowed_domains` omit `account` with no source | Redo: a domain policy must cite a card or a DEC id; otherwise Agent Workflow applies (no domain restriction) |
| D6 | major | The router ignores domain policy | `--route design-doc-draft --domain commerce` resolves to the design consultant; the commerce card allows only neutral questions for the owner | Redo: policy is applied before the route |
| D7 | medium | An unknown domain id is accepted when the role allows `*` | `--route design-question --domain no-such-domain` returns no error | Redo: unknown domain is an error |
| D8 | medium | Tier C rows are not routed | `--route implement --domain announcer-voice` says "ask the owner" instead of naming the answering domain | Redo: redirect to the answering domain; rights stay owner-only |
| D9 | medium | Sandbox is free text on the role | "workspace-write for tests; read-only for reviews"; Agent Workflow section 5 sets the sandbox per task | Redo: the task type says whether it writes |
| D10 | medium | 26 task types, several redundant, none carried by a brief | `banner-implement` duplicates the `implement` override; the brief template has no task type | Redo: 15 closed types; the brief carries `TASK_TYPE` and `ROLE` |
| D11 | medium | Only the static half exists | "How many agents, bound to which domain" is answered by the claims ledger, whose `model` field is free text | Add: structured holder and workers in a claim; a generated "who is working now" view |
| D12 | medium | A script is listed as a role; counts are typed by hand | `janitor` role; "10 roles, 7 model rows" in the page text | Redo: roles are people or models; counts are generated |
| D13 | medium | The glossary describes a mapping nothing checks | 46 of 87 requirements are listed by a card and 41 by none; 3 are listed by more than one card with no primary; 7 of 16 components have no domain; of 436 tracked files 230 have one owning write area, 204 have none, 0 have several | Redo: section 2.5 |
| D14 | medium | Claim-scope tool: wrong base, weak rules | Default base `main` reports 166 paths outside the claim for this stacked branch; the gate passes `--include-done`, so a closed claim authorises later work; a whole-checkout claim makes the check vacuous silently; paths are not read with `-z`; the gate line is cut before the "outside" count | Redo: section 2.7 |
| D15 | minor | Hook template | It says `cp ... .git/hooks/pre-push`; in a worktree `.git` is a file. It does not say that hooks are shared by every worktree of the repository | Fix the text; installing stays the owner's action |
| D16 | minor | Source-guard tests are skipped by `gate --quick` | They sit in classes that need git | Move them to classes that always run |
| D17 | minor | The owner-only list now exists in three places | AGENTS.md, the charter, the registry | The registry points to AGENTS.md |
| D18 | minor | "Registry" names five files | Requirement, decision, domain, agent and coordination registries | The glossary names all five |

**Kept from the draft:** report-only by default with `--enforce`; a report tool never crashes the gate; generated tables in the page; the `--route` query; `crates` and `planned_crates`; the shared-integration-file list; no model added or reassigned; "a different model needs an owner instruction".

## 2. Final design

### 2.1 Principles

1. **Static and live are different files.** The registry says what is *allowed* (roles, profiles, routes). The claims ledger says what is *happening* (which session fills which role on which domain now). Nobody is stationed; the live answer is always generated from the ledger.
   - **ANSWERED 2026-11-13 (Owner decision 2026-11-13 on AGENT-REGISTRY-DESIGN.md section 5, relayed by the coordinating session (not seen first-hand); owner message: "items 1 to 10 accepted"):** Standing rule for foundation work only (governance, registry and similar long-lived design): design and planning on Fable 5.2 xhigh, implementation on Sonnet 5.5 high. Not a rule for product code. The route `design-plan`/governance and the `governance-change` route carry it.
2. **Approval belongs to a route**, with evidence. A row without an owner record is never usable without asking.
   - **ANSWERED 2026-11-13 (Owner decision 2026-11-13 on AGENT-REGISTRY-DESIGN.md section 5, relayed by the coordinating session (not seen first-hand); owner message: "items 1 to 10 accepted"):** The banner-lab profiles (`gpt-6-astra`, `gpt-6.1-sol`, `gpt-6.1-terra` for checklists) stay `in_use_unconfirmed` until the Nakom comparison finishes.
3. **Unknown is a state.** A model that is not concrete, a check that cannot be made, a requirement with no domain: each is reported as such, never as a pass.
4. **One source per fact.** Routing rules stay in Agent Workflow sections 1 to 3; the registry encodes them and adds only what they do not say. Derived views are generated, not stored.
   - **ANSWERED 2026-11-13 (Owner decision 2026-11-13 on AGENT-REGISTRY-DESIGN.md section 5, relayed by the coordinating session (not seen first-hand); owner message: "items 1 to 10 accepted"):** The lead may write governance documents and tools itself; product code goes to the general worker (`gpt-6-luna`). The `governance-change` route is now the lead on `sonnet55-high`, owner_chosen (relayed), with Terra reviewing tools.
5. **Report first, enforce by owner decision.** A new check is a report or a warning. Making it fail the gate is listed in section 5.
   - **ANSWERED 2026-11-13 (Owner decision 2026-11-13 on AGENT-REGISTRY-DESIGN.md section 5, relayed by the coordinating session (not seen first-hand); owner message: "items 1 to 10 accepted"):** Stale cards, registry consistency and claim scope become gate failures at sprint exit, after one clean sprint. Implemented as report-only wiring plus a documented sprint-exit switch (`python tools/gate.py --sprint-exit`, exits non-zero on those three); the everyday gate is unchanged and nothing the gate accepts today changed.

### 2.2 Registry schema (version 2.0.0, PROPOSED)

File `docs/governance/AGENT-REGISTRY.json`. No separate JSON Schema file: the validator is the schema (standard library only), and this section is its specification.

```json
{
  "schema_version": "2.0.0",
  "evidence_levels": ["excluded", "proposed", "in_use_unconfirmed", "owner_chosen"],
  "profiles": [
    {"id": "luna-max", "tool": "codex-exec", "model": "gpt-6-luna", "effort": "max",
     "evidence": {"level": "owner_chosen", "source": "docs/delivery/AGENT-WORKFLOW.md section 1", "date": "2026-11-11", "relayed": false}}
  ],
  "roles": [
    {"id": "general-worker", "kind": "agent", "may_claim": false, "may_edit_cards": false, "may_commit": false}
  ],
  "task_types": [
    {"id": "implement", "writes": true, "pattern": ["T-L-R", "L-R"], "needs_review": "R1+"}
  ],
  "routes": [
    {"task_type": "implement", "domain": "*", "role": "general-worker", "profile": "luna-max",
     "reviewer": {"role": "test-worker", "profile": "terra-high"},
     "evidence": {"level": "owner_chosen", "source": "docs/delivery/AGENT-WORKFLOW.md sections 1 to 3", "date": "2026-11-11", "relayed": false}}
  ],
  "domain_policies": [
    {"domain": "commerce", "agent_work": "questions-only", "source": "DEC-COMMERCE-INTENT; docs/domains/commerce/CARD.md section 7"}
  ],
  "owner_reserved": {"source": "AGENTS.md, Work policy", "ids": ["taste", "rights-legal", "decision", "git-outward", "release-money", "charter"]},
  "exclusions": [
    {"profile": "luna-max", "task_type": "lab-review", "domain": "banner-lab", "source": "banner-lab workspace guidance (tentative)"}
  ]
}
```

Rules the validator applies:

- `tool` is one of `claude-code-session`, `claude-code-subagent`, `codex-exec`, `t3-thread`. `model` is a concrete id or the word `session` (not concrete).
- A route matches a task type and either one domain or `*`; the most specific match wins; no match means "no route: ask the owner".
- **Effective level** of an answer = the weaker of the route's and the profile's level. Only `owner_chosen` is usable without an owner instruction. `relayed: true` is shown in every answer.
- **Sandbox** is derived: `writes: false` gives read-only; `writes: true` gives workspace-write for a repository domain and "one lab folder" for an external root.
- **Domain policy** is applied first. `agent_work`: `normal`, `design-only`, `questions-only`, `owner-only`. A policy must cite a card section or a DEC id.
- A tier C domain redirects to the domain that answers for it and repeats that domain's policy.
- Roles are `human` or `agent`. Scripts (gate, janitor, validators) are tools, not roles.

### 2.3 Router content (the data the implementation writes)

Profiles and their evidence today:

| Profile | Tool, model, effort | Level | Source |
| --- | --- | --- | --- |
| `claude-session` | Claude Code session, model `session` | owner_chosen (Claude leads) | Agent Workflow section 1 |
| `fable52-xhigh` | Claude Code session, `claude-fable-5-2`, xhigh | owner_chosen, relayed, for design and planning of this governance increment | Owner instruction 2026-11-13 through the coordinating session |
| `sonnet55-high` | Claude Code session, `claude-sonnet-5-5`, high | owner_chosen, relayed, for implementing this increment (the effort was not restated) | Same |
| `luna-max` | Codex, `gpt-6-luna`, max | owner_chosen | Agent Workflow section 1 |
| `terra-high` | Codex, `gpt-6.1-terra`, high | model owner_chosen; the effort is the lead's default | Agent Workflow section 1 |
| `astra-high` | Codex, `gpt-6-astra`, high | in_use_unconfirmed | Banner-lab guidance, one trial |
| `sol-xhigh` | Codex, `gpt-6.1-sol`, xhigh | in_use_unconfirmed | Banner-lab guidance, one trial |
| `claude-subagent-ro` | Claude Code subagent, read-only, model `session` or as launched | in_use_unconfirmed (used 2026-11-13 for card drafts and two reviews; the lead chose it) | Agent Workflow section 7.5 |
| local model (Ollama) | none | excluded for cards, decisions, reviews and Thai text | Domain Steward design section 10 (a proposal) |

Task types (15) and default routes for every repository domain:

| Task type | Writes | Role, profile | Reviewer | Level |
| --- | --- | --- | --- | --- |
| `implement` (includes refactor) | yes | general-worker, `luna-max` | test-worker, `terra-high` | owner_chosen |
| `doc-fixture-tooling` | yes | general-worker, `luna-max` | none at R0 | owner_chosen |
| `code-question` | no | general-worker, `luna-max` | none | owner_chosen |
| `test-from-contract`, `defect-repro`, `harness` | yes | test-worker, `terra-high` | none | owner_chosen |
| `review-diff` | no | test-worker, `terra-high` | none | owner_chosen |
| `lab-build` | yes, one lab folder | lab-worker, `astra-high` | lab-reviewer, `sol-xhigh` | in_use_unconfirmed |
| `lab-review` (measurement, checklist, hashes) | no | lab-reviewer, `sol-xhigh`; alternative `terra-high` | none | in_use_unconfirmed |
| `design-plan` | no (drafts only) | design-consultant, `fable52-xhigh` | the lead integrates | in_use_unconfirmed in general; owner_chosen (relayed) for domain `governance` in this increment |
| `consult`, `card-backfill` | no | consultant, `claude-subagent-ro` | the lead verifies | in_use_unconfirmed |
| `governance-change` | yes | general-worker, `luna-max` (Agent Workflow pattern L); alternative: the lead directly on `sonnet55-high` | test-worker for tools | default owner_chosen; the alternative is owner_chosen (relayed) for this increment only |
| `integrate` | yes | lead only (gate, registries, generated files, card write-back, commit under the owner's authority) | not routable | owner_chosen |
| `approve` | no | delegate-approver, no profile | n/a | proposed; the charter is disabled |

Domain policies (each cites its source): `commerce` questions-only (DEC-COMMERCE-INTENT); `banner-lab` uses the lab task types, repository workers do not write there, taste is owner-only; `announcer-voice` and `banner-studio` redirect to `banner-lab`, rights are owner-only; `sharing` redirects to `account`; `release-ops` redirects to `governance`, signing and publishing are owner-only; `account` normal for the local build and documents, hosted deployment is blocked by its open decisions (the draft's restriction is withdrawn); `local-voice` has no card: design-only until one exists.

Ten answers the implementation must reproduce (acceptance fixture):

| Query | Expected answer |
| --- | --- |
| `implement`, `packs` | general-worker, gpt-6-luna max, workspace-write, reviewer test-worker gpt-6.1-terra, owner_chosen, usable |
| `review-diff`, `events-audio` | test-worker, gpt-6.1-terra high, read-only, usable, note "effort is the lead's default" |
| `code-question`, `host-core` | general-worker, gpt-6-luna max, read-only, usable |
| `lab-build`, `banner-lab` | lab-worker, gpt-6-astra high, in_use_unconfirmed, needs an owner instruction |
| `implement`, `commerce` | blocked by policy questions-only; owner |
| `design-plan`, `governance` | design-consultant or lead on claude-fable-5-2 xhigh, owner_chosen, relayed, scoped to this increment |
| `implement`, `announcer-voice` | redirect to `banner-lab`; rights owner-only |
| `implement`, `no-such-domain` | error: unknown domain |
| `taste` (owner-reserved id) | owner |
| `consult`, `account` | consultant, read-only, in_use_unconfirmed, needs an owner instruction |

### 2.4 Live binding: who is working now

A claim gains two structured fields (the ledger README is the format reference; old claims stay valid):

```json
"holder":  {"role": "lead", "profile": "sonnet55-high", "model": "claude-sonnet-5-5", "effort": "high", "session": "kohbor-gov"},
"workers": [{"role": "general-worker", "profile": "luna-max", "task_type": "implement", "session": "<codex session id>"}],
"base_commit": "4334d35"
```

`python tools/validate_agents.py --who` prints one line per active claim (domain, task, role, model, effort, expires) and reports a holder or worker whose role, profile or task type is not a registry route. This is the answer to "how many agents, bound to which domain": a count of active claims and their workers, generated at the moment of asking.

`tools/claim.py` (new, small): `open` writes a valid JSON claim with "fail if exists" and lists overlapping active claims on the same branch; `close` sets `done`, `finished`, `last_commit`; `list` prints them. It writes only inside the ledger. Claims are hand-written today, and all five seeded files were invalid JSON.

### 2.5 Taxonomy: one spine, generated views

The spine is the **requirement id**. Everything else hangs from it or from a path:

```text
requirement id --module-->            requirement module        (contracts/requirements.json: module)
requirement id --primary component--> component CMP-*           (contracts/requirements.json: primary_design_component_id; SDD)
requirement id --primary domain-->    domain                    (NEW rule below)
domain         --owns-->              crates, write areas       (state.json: crates, planned_crates, write_areas)
tracked file   --owned by-->          exactly one domain        (matched against write_areas; NEW report)
claim          --covers-->            a subset of write areas, on one branch, from a base commit
```

**Primary domain of a requirement (PROPOSED rule, checkable):** (1) the one card that lists it in `requirement_ids`; else (2) the registry row whose new `Default modules` column contains its module; else (3) UNASSIGNED. A card that depends on a requirement owned elsewhere lists it in a new field `interface_requirement_ids`. Today: 46 primary by rule 1, 3 conflicts to resolve (MEDIA-004, MEDIA-007, QUALITY-004), 41 need rule 2 (voice 7, mem 5, advice 4, peer 10, obs 5, update 4, premium 3, quality 3). Proposed defaults: `peer` to `sharing`; `voice`, `mem`, `advice` to `local-voice`; `update`, `obs` to `release-ops`; `premium` to `commerce`; the three `quality` ids are assigned one by one in a card.

Components and modules are then derived per domain (a component or module that serves several domains is normal and is printed, not flagged). Only three things are stored: `Default modules` (registry table), `interface_requirement_ids` and `crates` (state.json). The five files called "registry" are named in the glossary: requirement registry, decision registry, domain registry, agent registry, coordination registry.

**File coverage:** `python tools/validate_domains.py --map` prints, per domain, modules, components, crates and the count of tracked files it owns, plus the files with no owner (204 today: diagrams 41, `apps/desktop` 34, `spikes/audio` 12, design documents 11, contract schemas and root files) and files with several owners (0 today). Filling the gaps is card work (write areas), done by the lead that touches each domain.

### 2.6 Enforcement ladder

Levels: **E0** text only; **E1** reported by a tool; **E2** fails the gate the lead runs; **E3** local git hook (shared by every worktree of the repository, bypassable); **E4** technical control inside the agent tool; **E5** server side.

| Rule | Today | Target | Mechanism | Who raises it |
| --- | --- | --- | --- | --- |
| Indexed documents, existing ids and paths, no secrets in cards, spec consistency | E2 | E2 | `validate_domains.py`, `validate_spec.py` | in force |
| Stale card | E1 | E2 at sprint exit | `--fail-on-stale` | owner |
| Expired claim | E1 | E1 | liveness needs a person | n/a |
| Registry and routes consistent | E1 | E2 after one clean sprint | `validate_agents.py --enforce` | owner |
| Branch changes inside the claim | E1 | E2 at sprint exit, E3 before push | `check_claim_scope.py --enforce` | owner |
| Worker changes inside the brief's SCOPE | E0 (the lead reads `git status`) | E1, then E4 | `check_claim_scope.py --brief FILE`; Codex `-C <dir>` and `--add-dir <dir>` (the flags exist in codex-cli 0.167.2, VERIFIED from its help; that `-C <subfolder>` limits writes to that folder is UNVERIFIED) | lead for E1; owner for E4 |
| Role, model and effort of a launch match the route | E0 | E1 | `validate_agents.py --brief FILE` before launch; `--who` on claims | lead |
| Reviewer is another session and model | E0 | E1 | run records (section 5 item 9); UNVERIFIABLE until they exist | owner |
| One writer per area | E1 advisory | E1 with atomic claim creation | `tools/claim.py open` | lead |
| No push, merge or deletion by an agent | E0 (AGENTS.md) | E4, E5 | Claude Code permission rules and GitHub branch protection: owner actions, never set by an agent (exact settings UNVERIFIED) | owner |
| Branch and worktree hygiene | E1 | E1 | `janitor.py` | n/a |
| Continuous integration | none | E5 | a hosted runner is a service choice | owner |

### 2.7 Tool changes (specification for the implementation turn)

- **`tools/validate_agents.py` (rewrite of the registry part):** schema 2.0.0 checks of section 2.2; `resolve(task_type, domain)` with policy, tier C redirect, unknown-domain error, most-specific route, effective level, derived sandbox; `--route`; `--who`; `--brief FILE` (ROLE, TASK_TYPE, DOMAIN, MODEL and sandbox of a brief against the route; exit 1 on a mismatch, because it is a lead-side lint outside the gate); `--launch FILE` prints the `codex exec` command for a brief and never runs it; `--write-doc` generates tables and counts. The crate check and `--map` move out. Report-only in the gate, crash-safe, `--enforce` kept.
- **`tools/validate_domains.py` (additions, all WARN class, exit code unchanged):** parse `Default modules`; primary-domain rule with UNASSIGNED and CONFLICT warnings; `interface_requirement_ids` must exist and be primary elsewhere (a dangling id here fails, as dangling ids already do); crate ownership; file-coverage counts; `--map` and `--map --json`.
- **`tools/check_claim_scope.py`:** base = the claim's `base_commit`, else `--base`, else `main`; read paths with `-z`; a whole-checkout claim prints "scope check is vacuous"; a `done` claim counts only when the tree is clean and HEAD equals its `last_commit`, otherwise "work after the claim was closed"; `--brief FILE` checks the working tree against a brief's SCOPE; the summary starts with the counts.
- **`tools/claim.py`:** section 2.4.
- **`tools/gate.py`:** no change to what it accepts. The gate stops passing `--include-done`. A `--sprint-exit` mode stays a proposal.
- **`tools/hooks/pre-push.sample`:** correct the install text (`$(git rev-parse --git-common-dir)/hooks/pre-push`; shared by all worktrees). Still not installed.
- **Tests:** source-guard tests always run; new cases for every rule above and the ten answers of section 2.3.
- **Documents:** `AGENT-REGISTRY.md` and `GLOSSARY.md` rewritten from this design; the brief template in Agent Workflow section 4 gains `ROLE:` and `TASK_TYPE:` (section 5 item 8); the ledger README gains the claim fields.

## 3. What the first draft's files become

| File | Action |
| --- | --- |
| `docs/governance/AGENT-REGISTRY.json` | Rewritten to schema 2.0.0 with the content of section 2.3 |
| `docs/governance/AGENT-REGISTRY.md` | Rewritten; tables and counts generated; the enforcement table of section 2.6 |
| `docs/governance/GLOSSARY.md` | Rewritten around section 2.5; names the five registries |
| `docs/domains/README.md` | New column `Default modules` |
| `docs/domains/*/state.json` | `interface_requirement_ids` where needed; the three conflicts resolved |
| `tools/validate_agents.py`, `tools/validate_domains.py`, `tools/check_claim_scope.py`, `tools/claim.py` (new), `tools/hooks/pre-push.sample`, `tools/tests/*` | Section 2.7 |
| `docs/delivery/AGENT-WORKFLOW.md` | Section 1 pointer kept; section 4 template lines only after the owner agrees |
| Coordination ledger `README.md` | Claim fields `holder`, `workers`, `base_commit` |
| This document | Kept as the design record; superseded sections marked when implemented |

## 4. Implementation plan (for the implementation turn; R1 tooling, R0 documents)

Each step ends with `python tools/tests/run_tests.py` passing and `python tools/gate.py --quick` showing the same checks as before with no new FAIL. One commit per step, local only.

| Step | Work | Acceptance check |
| --- | --- | --- |
| P1 | Registry 2.0.0 data and the `validate_agents.py` core with tests | The ten answers of section 2.3 are a test fixture and pass; `python tools/validate_agents.py --enforce` exits 0 on the shipped registry; each defect D1 to D10 and D12 has a failing-then-passing test |
| P2 | `--brief` and `--launch`; a sample brief under `tools/tests/fixtures/` | A brief whose MODEL differs from its route exits 1 and names the route; `--launch` prints a command that contains the sandbox flag and runs nothing (a test asserts no subprocess call) |
| P3 | `tools/claim.py`, claim fields, `--who`, ledger README | `open` twice with one name fails the second time; the written file parses as JSON; `--who` against a temporary ledger lists holder and workers and reports a profile that is not a route |
| P4 | Taxonomy: `Default modules`, `interface_requirement_ids`, WARN checks, `--map` | `--map --json` shows 87 requirements with 0 CONFLICT and 0 UNASSIGNED; the file-coverage counts are printed; `python tools/validate_domains.py` still exits 0 and prints the same failure count as before (0) |
| P5 | `check_claim_scope.py` changes | On this branch with the claim's `base_commit` it reports 0 outside; a commit after a claim is closed is reported; a whole-checkout claim prints the vacuous note; a path with spaces and a Thai file name are handled (tests) |
| P6 | Hook text, source-guard tests moved, documents rewritten and regenerated, INDEX, governance card, changelog, manifest | `python tools/update_manifest.py --check` matches; the listing of the common hooks folder is unchanged before and after (nothing installed); `git status` is clean |
| P7 | Independent read-only review of the tools diff by the test worker (the owner-chosen route for `review-diff`), findings accepted, rejected or parked with a reason | The review report is recorded as `agent_review`; if the test worker cannot be run from the session, stop and ask through the coordinating session instead of choosing another model |

Order and size: P1, P5 and P4 are independent of each other; P2 and P3 need P1; P6 and P7 are last. About 500 lines of tool code change and 40 new tests (estimate, UNVERIFIED). Out of scope: installing a hook, any `--enforce` in the gate, permission settings, branch protection, CI, a change to `contracts/**`.

## 5. Decisions for the owner

1. Is "design and planning on Fable 5.2 xhigh, implementation on Sonnet 5.5" a standing rule for lead sessions, for governance only, or for this increment only? State the implementation effort.
2. Confirm, replace or leave unconfirmed the banner-lab profiles: `gpt-6-astra` high for builds, `gpt-6.1-sol` xhigh for review, `gpt-6.1-terra` for checklists.
3. Which model reviews governance work: the test worker (already chosen for diff review), or a read-only Claude subagent of a named model?
   - **ANSWERED 2026-11-13 (owner decision, relayed by the coordinating session; not seen first-hand):** "terra high". The reviewer of governance tooling is the test worker, `gpt-6.1-terra` at high effort, read-only. This matches the existing `review-diff` route; the `claude-subagent-ro` profile stays in_use_unconfirmed. Used for P7 (section 7).
4. May the lead write governance documents and tools itself, as it has, or do they go to the general worker as pattern L says?
5. Which reports become gate failures, and when: stale cards, registry consistency, claim scope (proposal: at sprint exit, after one clean sprint).
6. Install the pre-push hook? It applies to every worktree of the repository, including the Sprint 3 checkout.
   - **ANSWERED 2026-11-13 (Owner decision 2026-11-13 on AGENT-REGISTRY-DESIGN.md section 5, relayed by the coordinating session (not seen first-hand); owner message: "items 1 to 10 accepted"):** Install the pre-push hook now that PR #10 is merged (main contains it). Installed by the lead after the P7 review and the tests; see section 9.
7. Technical blocks outside the repository tools: Claude Code permission rules against push and deletion, branch protection on `main`, continuous integration.
   - **ANSWERED 2026-11-13 (Owner decision 2026-11-13 on AGENT-REGISTRY-DESIGN.md section 5, relayed by the coordinating session (not seen first-hand); owner message: "items 1 to 10 accepted"):** Done by the coordinating session, outside this repository: GitHub branch protection on `main` (pull request required, no force push, no deletion, 0 approvals, admins not enforced) and user-level Claude Code deny rules for force push, remote and branch deletion, worktree removal, `reset --hard` and `git clean`. The exact settings were not seen by this session (UNVERIFIED here).
8. Add `ROLE:` and `TASK_TYPE:` to the brief template (a change to Agent Workflow section 4).
   - **ANSWERED 2026-11-13 (Owner decision 2026-11-13 on AGENT-REGISTRY-DESIGN.md section 5, relayed by the coordinating session (not seen first-hand); owner message: "items 1 to 10 accepted"):** Add `ROLE:` and `TASK_TYPE:` to the brief template: done, two lines in Agent Workflow section 4 (an owner-reserved file: listed for owner audit).
9. Record every worker run (role, model, session, brief hash) so separation of duties can be checked from records.
   - **ANSWERED 2026-11-13 (Owner decision 2026-11-13 on AGENT-REGISTRY-DESIGN.md section 5, relayed by the coordinating session (not seen first-hand); owner message: "items 1 to 10 accepted"):** Record every worker run: done as a run record format and a check that the reviewer's session and model differ from the author's; see section 9.
10. The default-module assignment of section 2.5 for the 41 requirements with no card (the lead can decide it as internal structure; listed so the owner can object).
   - **ANSWERED 2026-11-13 (Owner decision 2026-11-13 on AGENT-REGISTRY-DESIGN.md section 5, relayed by the coordinating session (not seen first-hand); owner message: "items 1 to 10 accepted"):** The lead decides the 41 default modules: kept as assigned in section 2.5.

## 6. Not verified

- Whether Codex `-C <subfolder>` limits writes to that folder, and whether `workspace-write` blocks the network. Only the existence of the flags was read from the help text.
- The Claude Code setting names for permission rules; branch-protection details.
- Whether a worker report schema can carry optional actor fields under structured output.
- The owner's instructions of 2026-11-13 were relayed by the coordinating session; this session did not see the owner's words first-hand.
- No tool in this document exists yet. The numbers in D13 and D14 were measured on commit `4334d35` and will change.

## 7. P7: independent review of the tools diff (agent_review, 2026-11-13)

**Label: `agent_review`. It is not owner approval and not human review.** Reviewer: the test worker, `gpt-6.1-terra` at effort `high`, read-only sandbox, launched with the command `python tools/validate_agents.py --launch` printed for the brief (`codex exec -m gpt-6.1-terra -c model_reasoning_effort=high -s read-only ... --output-schema tools/agent/worker-report.schema.json`); the owner chose this reviewer ("terra high", decision 3, relayed). Author: the lead on `claude-sonnet-5-5` high, a different model. Diff reviewed: `git diff 9ea6049~1..6d8e505 -- tools`. The brief and the report are in a run directory outside the repository (report SHA-256 `89446c1e3f3b185a8db852b973db86e0657dd56e913047d5878e050faa4387e9`, brief `2441fe86913233a73e0fb4627d17e298a715ba59450ae8e7c0f5de43f7355e08`). The brief had no cargo resource block to copy: the review builds nothing; the last passing brief is in a run directory this session could not locate (UNVERIFIED).

The reviewer read the diff and ran read-only probes (marked RAN); it did not run the unit tests (the sandbox forbids temporary files). Result: status DONE, no file changed, 16 findings (9 major, 7 minor). Every finding was checked by the lead before it was accepted. Fix round 1 (the only round used) is one commit; the fixes carry tests.

| Id | Finding (short) | Disposition | Reason and fix |
| --- | --- | --- | --- |
| F1 | A committed rename hides the deleted source path | accepted | `git diff --no-renames`: both ends are listed; test added |
| F2 | A failing `git status` became an empty, clean result | accepted | The tool now raises and reports PROBLEM; `--enforce` exits 1 |
| F3 | `--claim` skipped the done/branch rules | accepted | The same rules as for ledger claims; a claim of another branch is a PROBLEM |
| F4 | A bad `--base` silently narrowed the check | accepted | A failing diff is a PROBLEM, never a fallback |
| F5 | `resolve` could report `usable` on empty evidence or an undefined profile | accepted | `evidence_ok`; incomplete evidence is never usable; tests |
| F6 | `claim close` used a fixed `.tmp` name | accepted in part | A unique temporary file of its own; an existing `.tmp` is never touched. Concurrent closes of one claim: last write wins; a lock or version check is **parked** (one holder closes its own claim) |
| F7 | `--launch` put the workspace into the shell command unquoted | accepted | `shlex.quote` for workspace, model and effort; test with `; echo injected` |
| F8 | `--base` ranked above the claim's `base_commit`, against the design | accepted, with an exception | The claim's base comes first; an override is used only when it is an ancestor of the claim base (it widens the range), otherwise it is ignored with a NOTE. The reviewer's alternative (refuse any override) is parked |
| F9 | Claim entries with spaces, Thai names or a single folder name were ignored | accepted in part | Spaces and Unicode are accepted when the entry has a `/`, `*` or an extension. A bare folder name (`src`) stays ignored because it cannot be told from prose: **parked**, write `src/**` |
| F10 | The MODEL regex matched only a prefix | accepted | Full match; a suffix, a second model or shell text is a mismatch |
| F11 | Read-only task with a SCOPE to change passed unless it said `(write)` | accepted | A read-only task must have SCOPE `none`; test |
| F12 | `--map` crashed on a malformed `state.json` | accepted | Reported as INCOMPLETE with the domain names |
| F13 | `git ls-files` without `-z` can quote non-ASCII names | accepted | `-z` in both places; test with a Thai file name |
| F14 | The hook template could write `__pycache__` | accepted | `PYTHONDONTWRITEBYTECODE=1` and `python -B`; shell syntax checked |
| F15 | A policy source with one real and one missing path passed | accepted | Every cited path must exist; test |
| F16 | Two cards planning one crate gave no warning | accepted | `planned_crates` ownership is checked; test |

Rejected: none. Parked: F6 locking, the F8 alternative, F9 bare folder names. After the fixes: `python tools/tests/run_tests.py` ran 149 tests, 0 failed; `python tools/gate.py --quick` is recorded in the changelog entry. The fixes were not re-reviewed by Terra (the brief allowed at most two fix rounds; a second review is the owner's call).


## 8. P7, second review by gpt-6.1-sol (agent_review, 2026-11-13)

**Label: `agent_review`; not owner approval, not human review.** The owner asked for a second review by "gpt 6.1 sol" (a direct owner instruction, relayed in the chat session; the effort `xhigh` is the `sol-xhigh` profile's, chosen by the lead because the owner did not state one). The registry route for `review-diff` is Terra, so `validate_agents.py --brief` reports a mismatch for this brief: that is the owner instruction overriding the route, recorded here. Reviewer: `gpt-6.1-sol`, read-only sandbox, a different model from the author (Sonnet 5.5) and from the first reviewer (Terra). Reviewed commit `53c9344`; diff `git diff 9ea6049~1..53c9344 -- tools`. Report SHA-256 `4c0032ba0729a8c2f9e7f1e0dff1ce4902de3724beff92d012142a92123aa8fc` (outside the repository). It did not run the unit tests or the gate (sandbox), and its Bash could not start inside the sandbox, so the hook was not executed (UNVERIFIED by the reviewer).

**First review items re-judged by Sol:** 9 FIXED, 7 PARTLY (R2, R3, R5, R8, R9, R11, R12). The remaining parts are fixed in this round (F5, F7, F1/F4, F6, F11, F10, F15/F16). 23 new findings: 9 major, 14 minor. Every one was checked by the lead; **all 23 are accepted** and fixed in fix round 2 with tests (the second and last round allowed). Rejected: none. Parked (unchanged): locking of concurrent claim close; refusing any base override (replaced by "an override only adds coverage"); bare folder names in claim entries.

| Finding | Disposition and fix |
| --- | --- |
| F1 route usable on null profile, null source or undefined role | accepted: strict evidence (string source), defined role and profile required, agent route needs a profile |
| F2 tier C redirect followed one unchecked edge | accepted: the chain is followed to its end; a cycle or dangling target is an error in `resolve` and a problem in `check` |
| F3 policies of the requested domain dropped on redirect | accepted: policies and exclusions of every domain on the chain apply |
| F4 `--brief`/`--launch` failures exited 0 | accepted: any read, parse or registry failure exits 1 |
| F5 unresolved baseline silently checked nothing | accepted: an unresolved claim base, override or missing default base is a PROBLEM |
| F6 replacing the claim base with an ancestor could hide a reverted edit | accepted: the diffs of the claim base and the override are united |
| F7 `--claim` implied done claims; `last_commit` matched any prefix | accepted: `--include-done` is required; `last_commit` must resolve to HEAD itself |
| F8 claim overlap ignored absolute write areas | accepted: both claims are normalized against the repository and worktree roots |
| F9 unknown Default modules token failed the validator, against section 2.7 | accepted: it is a WARN (a dangling interface id still fails) |
| F10 `none, edit x` passed as no scope | accepted: `none` may only be followed by an explanation without paths or write words |
| F11 names with Thai marks, quotes or root names with spaces ignored | accepted: a prose test replaces the ASCII whitelist |
| F12 brace groups split by commas in a brief SCOPE | accepted: split outside braces |
| F13 explicit `SCOPE: none` counted as a missing scope | accepted: a clean read-only brief passes, any change fails |
| F14 a copy record added its unchanged source | accepted: only renames add both ends |
| F15 valid JSON of the wrong shape crashed `--map` | accepted: reported as incomplete; `run()` fails cleanly on a non-object |
| F16 `--map` dropped its input diagnostics | accepted: `input_problems` and `incomplete` in the output |
| F17 a dangling interface id raised a second error | accepted: guarded |
| F18 escaped pipes shifted the registry columns | accepted: split on unescaped pipes |
| F19 filesystem fallback was labelled tracked files | accepted: `file_source` and an honest label |
| F20 `--who` skipped an unknown domain | accepted: reported as a problem |
| F21 the hook's `-B` broke interpreter paths with spaces | accepted: `"$py" -B` with the executable alone in the variable |
| F22 `**` claims were not reported as vacuous | accepted: match-all globs set the vacuous note |
| F23 the ledger-only-write test could not fail | accepted: a recursive snapshot, and a probe write proves it can fail |

After the fixes: `python tools/tests/run_tests.py` ran 169 tests, 0 failed. These fixes were not re-reviewed (the limit of two fix rounds is used).


## 9. Owner decisions 5 to 9: what was built (2026-11-13)

Owner message (relayed by the coordinating session): items 1 to 10 accepted as recommended; item 6 (install the hook) after PR #10 merged into `main` (merge commit `aed720d`, reported by the coordinating session; not checked here). Order followed: record the answers (section 5), the Sol review and its fixes (section 8), then the items below, then the hook.

- **Item 8, brief template.** Two lines in Agent Workflow section 4, `ROLE:` and `TASK_TYPE:`, directly after `PATTERN:`. That file is owner-reserved: nothing else in it changed; listed here for owner audit (`git diff` of the commit shows exactly two added lines). `validate_agents.py --brief` already checks both lines.
- **Item 9, run records.** `tools/run_record.py` and the ledger file `runs/runs.jsonl` (outside the repository). The check reads the records: a reviewer must differ from the writing run in session and in concrete model; a model that is not concrete is UNVERIFIABLE; a run off its route is a problem unless an owner instruction is recorded; a writing run whose route names a reviewer and has no review record is a note. The P1 to P6 work was recorded retroactively (author the lead on `claude-sonnet-5-5`, reviewers Terra and, at the owner's instruction, Sol); that records what this session knows and is no more than the lead's own record. The format is not part of the worker-report schema (its structured output requires every field and forbids extras): the lead writes the record after the run.
- **Item 5, sprint exit.** `python tools/gate.py --sprint-exit` runs the domain validator with `--fail-on-stale`, the registry check with `--enforce` and the claim-scope check with `--enforce --include-done`. Without the flag the gate runs exactly what it ran before (the three checks in report mode, plus the run-record report). No clean sprint has run yet: the owner's condition "after one clean sprint" is for the lead to apply when a sprint closes.
- **Item 6, hook.** INSTALLED on 2026-11-13 after both review rounds and the tests, by the lead (the owner decided it; relayed): `F:/kohbor/.git/hooks/pre-push`, a byte-identical copy of `tools/hooks/pre-push.sample` at commit `206951f` (SHA-256 `65363ac8087d8fcb3a0895e1445f9312ea63ce0b6313d3e133c26f4ae8da6001`). No `core.hooksPath` is set; before the copy the hooks folder held no active hook. **It is shared by every worktree of the repository**, including the checkout of another session. Verified: syntax checked, a dry run from this worktree printed the reports and exited 0, a run from a checkout without the tools skipped them and exited 0. Not verified: a real `git push` (none was made). To remove it, delete that one file. The installed hook is a copy of `tools/hooks/pre-push.sample`; it is report-only unless `KOHBOR_HOOK_ENFORCE=1` is set, it skips a tool that the pushing checkout lacks, and it is shared by every worktree of the repository.

