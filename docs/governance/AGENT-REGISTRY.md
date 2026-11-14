---
title: "Agent Registry, Router and Enforcement"
doc_id: "AGENT-REGISTRY"
status: draft
version: "0.2.1"
updated: "2026-11-14"
authority: "Process only. The owner asked for this registry on 2026-11-13; this text implements the design in AGENT-REGISTRY-DESIGN.md, is an agent proposal and defines no product requirement"
owner_role: "Governance lead (integration owner)"
approval_status: "not_approved"
---

# Agent Registry, Router and Enforcement

The machine-readable source is [AGENT-REGISTRY.json](AGENT-REGISTRY.json) (schema 2.0.0). This page explains it and holds a generated copy of its tables. The review of the first draft, the design and the plan are in [AGENT-REGISTRY-DESIGN](AGENT-REGISTRY-DESIGN.md); terms are in the [Glossary](GLOSSARY.md); roles, patterns and risk tiers are defined in [Agent Workflow](../delivery/AGENT-WORKFLOW.md). Nothing here is approved by the owner: rows the owner did not choose say so.

## 1. Answers to the owner's questions

- **How many agents are stationed?** None. A **role** is filled for one task and ends with it; what it learned is written to files (the card, the claim, the backlog). Who is working now is generated from the claims ledger: `python tools/validate_agents.py --who` prints one line per active claim and the count of workers.
- **Is a model fixed per role?** A model (profile) is fixed per **route**, that is per task type and domain. A different model needs an owner instruction; the lead does not substitute one when a worker fails.
- **What did the owner choose?** Every profile and every route carries an evidence level and a source. `owner_chosen` is the only level usable without asking; `in_use_unconfirmed` (used or recommended after a trial) and `proposed` need an owner instruction; `excluded` must not be used. The level of an answer is the weaker of its route and its profile. `relayed` means the instruction reached this repository through another session, not first-hand.
- **Which route answers a question?** `python tools/validate_agents.py --route <task type> --domain <id>`. Domain policy is applied first (for example `commerce` allows only neutral questions for the owner), tier C domains redirect to the domain that answers for them, and an unknown domain is an error. Owner-reserved matters (taste, rights and legal, decisions, push and merge, release and money, charters) answer `owner`.
- **How is a run recorded?** `python tools/run_record.py add ...` appends one JSON line per worker run to the ledger (`runs/runs.jsonl`): task id, task type, domain, role, profile, the model and effort taken from the registry (never typed), session, what it reviews, SHA-256 of the brief and report, status, `off_route` and an optional owner instruction. `check` applies the separation-of-duties rule to the records.
- **How is a brief checked?** `python tools/validate_agents.py --brief FILE` compares ROLE, TASK_TYPE, DOMAIN, MODEL (and effort) and SCOPE with the route and exits 1 on a mismatch; `--launch FILE` prints the `codex exec` command and never runs it. The brief template in Agent Workflow section 4 does not yet carry ROLE and TASK_TYPE: adding them is an owner decision (design section 5, item 8).
- **Is the workflow enforced everywhere?** No. Section 3 lists what a tool checks, what is only reported and what is still text.

## 2. Registry and router (generated from the JSON; do not edit by hand)

Refresh: `python tools/validate_agents.py --write-doc`. The check fails when this block differs from the JSON.

<!-- GENERATED:agent-registry (python tools/validate_agents.py --write-doc) -->
Registry content: 9 profiles, 9 roles (1 human, 8 agent), 15 task types, 16 routes, 5 domain policies. Stationed agents: 0.

| Profile | Tool | Model (effort) | Level | Relayed | Source |
| --- | --- | --- | --- | --- | --- |
| `claude-session` | claude-code-session | session | owner_chosen | no | docs/delivery/AGENT-WORKFLOW.md section 1 |
| `fable52-xhigh` | claude-code-session | claude-fable-5-2 (xhigh) | owner_chosen | yes | Owner instruction 2026-11-13, relayed by the coordinating session (not seen first-hand) |
| `sonnet55-high` | claude-code-session | claude-sonnet-5-5 (high) | owner_chosen | yes | Owner instruction 2026-11-13, relayed by the coordinating session (not seen first-hand) |
| `luna-max` | codex-exec | gpt-6-luna (max) | owner_chosen | no | docs/delivery/AGENT-WORKFLOW.md section 1 (models and max effort are the owner's choice) |
| `terra-high` | codex-exec | gpt-6.1-terra (high) | owner_chosen | no | docs/delivery/AGENT-WORKFLOW.md section 1 |
| `astra-high` | codex-exec | gpt-6-astra (high) | in_use_unconfirmed | no | Banner-lab workspace guidance, one trial (card: docs/domains/banner-lab/CARD.md) |
| `sol-xhigh` | codex-exec | gpt-6.1-sol (xhigh) | owner_chosen | yes | owner message 2026-11-14 after Nakom comparison (relayed by the coordinating session) |
| `claude-subagent-ro` | claude-code-subagent | session | in_use_unconfirmed | no | docs/delivery/AGENT-WORKFLOW.md section 7.5 |
| `local-model` | local-model | local | excluded | no | Domain Steward design section 10 (a proposal, outside the repository; approved as part of decision 1 only in outline) |

| Role | Kind | Claims | Edits cards | Commits |
| --- | --- | --- | --- | --- |
| `owner` | human | yes | yes | yes |
| `lead` | agent | yes | yes | yes |
| `general-worker` | agent | no | no | no |
| `test-worker` | agent | no | no | no |
| `lab-worker` | agent | no | no | no |
| `lab-reviewer` | agent | no | no | no |
| `design-consultant` | agent | no | no | no |
| `consultant` | agent | no | no | no |
| `delegate-approver` | agent | no | no | no |

| Task type | Writes | Pattern | Review |
| --- | --- | --- | --- |
| `implement` | yes | T-L-R, L-R | R1+ |
| `doc-fixture-tooling` | yes | L | none at R0 |
| `code-question` | no | Q | none |
| `test-from-contract` | yes | T-L-R | none |
| `defect-repro` | yes | T | none |
| `harness` | yes | T | none |
| `review-diff` | no | T-L-R, L-R | none |
| `lab-build` | yes | L | lab review |
| `lab-review` | no | review | none |
| `design-plan` | no | consultant | the lead integrates |
| `consult` | no | consultant | the lead verifies |
| `card-backfill` | no | consultant | the lead verifies |
| `governance-change` | yes | L, L-R | R1 for tools |
| `integrate` | yes | lead | none |
| `approve` | no | approval | none |

| Task type | Domain | Role | Profile | Reviewer | Level | Relayed |
| --- | --- | --- | --- | --- | --- | --- |
| `implement` | * | `general-worker` | `luna-max` | `test-worker` (terra-high) | owner_chosen | no |
| `doc-fixture-tooling` | * | `general-worker` | `luna-max` | none | owner_chosen | no |
| `code-question` | * | `general-worker` | `luna-max` | none | owner_chosen | no |
| `test-from-contract` | * | `test-worker` | `terra-high` | none | owner_chosen | no |
| `defect-repro` | * | `test-worker` | `terra-high` | none | owner_chosen | no |
| `harness` | * | `test-worker` | `terra-high` | none | owner_chosen | no |
| `review-diff` | * | `test-worker` | `terra-high` | none | owner_chosen | no |
| `lab-build` | banner-lab | `lab-worker` | `sol-xhigh` | `test-worker` (terra-high) | owner_chosen | yes |
| `lab-review` | banner-lab | `lab-reviewer` | `sol-xhigh` | none | in_use_unconfirmed | no |
| `design-plan` | * | `design-consultant` | `fable52-xhigh` | none | in_use_unconfirmed | no |
| `design-plan` | governance | `lead` | `fable52-xhigh` | none | owner_chosen | yes |
| `consult` | * | `consultant` | `claude-subagent-ro` | none | in_use_unconfirmed | no |
| `card-backfill` | * | `consultant` | `claude-subagent-ro` | none | in_use_unconfirmed | no |
| `governance-change` | governance | `lead` | `sonnet55-high` | `test-worker` (terra-high) | owner_chosen | yes |
| `integrate` | * | `lead` | `claude-session` | none | owner_chosen | no |
| `approve` | governance | `delegate-approver` | none | none | proposed | no |

| Domain | Agent work | Allowed task types | Source |
| --- | --- | --- | --- |
| `commerce` | questions-only | consult, integrate | DEC-COMMERCE-INTENT; docs/domains/commerce/CARD.md section 7 |
| `banner-lab` | normal | lab-build, lab-review, design-plan, consult, card-backfill, integrate | docs/domains/banner-lab/CARD.md section 1 |
| `announcer-voice` | redirect | none | docs/domains/README.md; DEC-COMMERCE-INTENT |
| `release-ops` | redirect | none | docs/domains/README.md; AGENTS.md |
| `local-voice` | design-only | design-plan, consult, integrate | docs/domains/README.md (no card exists yet) |

| Owner-reserved | Meaning |
| --- | --- |
| `taste` | Voice, accent, banner look, wording feel (agents pre-screen only) |
| `rights-legal` | Voice rights, licences, privacy law |
| `decision` | Any DEC, requirement, scope or threshold change (an agent may draft) |
| `git-outward` | Push, pull request, merge to main, deleting a branch, worktree or folder, per item |
| `release-money` | Release, deployment, signing, publishing, payments, paid services |
| `charter` | Any charter or change to AGENTS.md |
<!-- /GENERATED:agent-registry -->

## 3. Enforcement status

Levels: E0 text only; E1 reported by a tool; E2 fails the gate the lead runs; E3 local git hook (shared by every worktree, bypassable); E4 technical control inside the agent tool; E5 server side.

| Rule | Level today | Mechanism | Change needs |
| --- | --- | --- | --- |
| Every document is indexed; cards cite existing ids and paths; no secrets in cards; dangling interface ids | E2 | `tools/validate_domains.py` in `tools/gate.py` | in force |
| Specification consistency, schemas, fixtures | E2 | `tools/validate_spec.py` in the gate | in force |
| Stale card | E1 | `validate_domains.py` prints STALE; `--fail-on-stale` exists, the gate does not pass it | owner |
| Primary domain of every requirement (conflict, unassigned), crate ownership | E1 | `validate_domains.py` warnings; `--map` | owner |
| Expired claim | E1 | `validate_domains.py` warning | n/a (liveness needs a person) |
| Registry, routes and policies consistent; reviewer model differs from author | E1 | `validate_agents.py` in the gate; `--enforce` exists, the gate does not pass it | owner |
| Who is working, and whether each holder or worker is a registry route | E1 | `validate_agents.py --who` (not in the gate) | lead |
| A branch's changes stay inside its claim | E1 | `check_claim_scope.py` in the gate; `--enforce` exists | owner |
| A worker's changes stay inside the brief's SCOPE | E1, lead-run | `check_claim_scope.py --brief FILE` (exit 1 on a path outside) | lead |
| A brief matches its route | E1, lead-run | `validate_agents.py --brief FILE` (exit 1 on a mismatch) | lead |
| One writer per area | E1, advisory | `tools/claim.py open` creates the claim exclusively and lists overlaps | n/a |
| The reviewer of a task was a different session and model | E1 | `tools/run_record.py check` reads the run records the lead appends to the ledger (`runs/runs.jsonl`); a model that is not concrete is UNVERIFIABLE; a run off its route is a problem unless an owner instruction is recorded; in the gate, report-only. Only as complete as the records the lead writes | lead |
| No push without the checks | E3, report-only | The pre-push hook (`tools/hooks/pre-push.sample`) is installed in the common git directory, shared by every worktree, bypassable with `--no-verify`; it prints the results and lets the push continue unless `KOHBOR_HOOK_ENFORCE=1` | owner (to enforce) |
| No push, merge or deletion by an agent | E0 in the repository; E4 and E5 outside it | AGENTS.md; per the owner's decision 7, GitHub branch protection on `main` and user-level Claude Code deny rules were set by the coordinating session (their exact settings were not seen here: UNVERIFIED) | owner |
| Branch, worktree and private-copy hygiene | E1 | `tools/janitor.py` (deletes nothing) | n/a |
| Continuous integration | none | not built | owner |

Everything at E1 is deliberate: turning a report into a gate failure changes what the gate accepts, which is reserved for the owner. The owner decided (item 5) that stale cards, registry consistency and claim scope fail the gate **at sprint exit, after one clean sprint**: `python tools/gate.py --sprint-exit` does that (`--fail-on-stale`, `--enforce`, `--enforce --include-done`); the everyday gate is unchanged. A tool test asserts the default adds no enforcement. Run records stay report-only in both modes.

## 4. Owner decisions and what is still open

The owner answered the ten decisions of [AGENT-REGISTRY-DESIGN](AGENT-REGISTRY-DESIGN.md) section 5 on 2026-11-13 (relayed by the coordinating session); sections 5 and 9 of that document record the answers and what each one changed. Still open: running a clean sprint before `--sprint-exit` is used; the tentative banner-lab models until the Nakom comparison finishes; a second independent review of the fix rounds (the limit of two fix rounds is used); the parked items of section 7 and 8; and the technical blocks outside the repository tools, whose exact settings this session did not see. The earlier list, kept for history: whether "design and planning on Fable 5.2, implementation on Sonnet 5.5" is a standing rule; the tentative banner-lab models; which model reviews governance work; whether the lead may write governance changes itself; which reports become gate failures and when (a `--sprint-exit` mode is proposed and not written); installing the pre-push hook; technical blocks outside the repository tools; ROLE and TASK_TYPE in the brief template; recording every worker run; and the default-module assignment of requirements with no card.
