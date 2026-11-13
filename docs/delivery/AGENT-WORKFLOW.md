---
title: "Agent Workflow"
doc_id: "AGENT-WORKFLOW"
status: draft
version: "0.3.6"
updated: "2026-11-13"
authority: "SDP delivery strategy; AGENTS.md work policy. Process only: this page defines no product requirement"
owner_role: "Engineering lead"
approval_status: "not_approved"
---

# Agent Workflow

This page defines how one lead agent and Codex CLI workers build Kohbor. It applies the SDP delivery strategy and the AGENTS.md work policy; where they differ, those documents win. The sprint plan is in SPRINT-ROADMAP.md.

**Design goal (owner direction, 2026-11-11).** The lead spends as little of its quota as possible on reading source code. Workers read and write the code. The lead reads contracts, briefs, reports and gate results, and reads code only where section 6 requires it.

## 1. Roles and models

| Role | Agent | Model | Effort | Does |
| --- | --- | --- | --- | --- |
| Lead | Claude | session model | n/a | Decomposes work, writes briefs, runs the gate, accepts or returns work, owns shared contracts and generated files, reports to the owner |
| General worker | Codex CLI | `gpt-6-luna` | `max` | Implementation, refactoring, tooling, fixtures, documents, read-only questions about the code |
| Test worker | Codex CLI | `gpt-6.1-terra` | `high` | Tests written from a contract before the implementation, defect reproduction as a failing test, test harnesses, independent review of a diff |
| Owner | Product owner | n/a | n/a | Decisions, live matches, native runs on certified hardware, media rights, authority to commit, push and merge |

- The models and the `max` effort for the general worker are the owner's choice. The `high` effort for the test worker is the lead's default; the owner did not set it.
- Checked on 2026-11-11 with `codex-cli 0.162.1`: both model IDs are in the local model list, and each answered a one-line read-only prompt at the stated effort. No write task has run through this workflow yet. Sprint 0 is the pilot (SPRINT-ROADMAP).
- A different model needs an owner instruction. The lead does not substitute one when a worker fails.
- Every role, the model that fills it, its sandbox, the domains it may work in and the approval level of each row are listed in the [Agent Registry](../governance/AGENT-REGISTRY.md) (machine-readable: `docs/governance/AGENT-REGISTRY.json`; query with `python tools/validate_agents.py --route <task type> --domain <id>`). The registry adds no model: rows the owner did not choose are marked tentative or proposed and need an owner instruction before use.

## 2. Routing rule

Send a task to the **test worker** when its output is a test, or a judgment about someone else's code:

- tests derived from a contract, schema, fixture set or specification section;
- a failing test that reproduces a reported defect;
- a harness that measures something (latency, preemption order, resource use);
- review of a general-worker diff.

Send everything else to the **general worker**.

The two workers never hold the same files in one task. That separation is what lets the lead accept code without reading it: the tests come from the contract and from a different model than the implementation.

## 3. Task patterns

| Pattern | Use for | Steps |
| --- | --- | --- |
| **T-L-R** | Behavior with a testable contract (most product code) | 1. Test worker writes tests from the contract; they fail for the right reason. 2. General worker implements until they pass; the test files are outside its SCOPE. 3. Test worker reviews the diff read-only. 4. Lead runs the gate |
| **L-R** | Code where tests cannot come first (spikes, native glue, UI layout) | 1. General worker implements with its own tests. 2. Test worker reviews and adds missing tests. 3. Lead runs the gate |
| **L** | Documents, fixtures, mechanical edits, dev tooling | 1. General worker. 2. Lead runs the gate |
| **Q** | The lead needs a fact about the code | General worker, read-only sandbox, answer limited to 30 lines with `path:line` references. The lead does not open the files |

If the general worker believes a test is wrong, it reports a scope request. It does not edit the test. The lead sends the question to the test worker with the contract reference.

## 4. Handoff brief

One brief per task, in English, at most about 60 lines. Reference documents by path and section; do not paste them. The worker reads AGENTS.md and the referenced context itself.

```text
TASK_ID:        S1-T2
MODEL:          gpt-6-luna (max) | gpt-6.1-terra (high)
PATTERN:        T-L-R step 2
ROLE:           general-worker   (a role of docs/governance/AGENT-REGISTRY.json)
TASK_TYPE:      implement        (a task type of the same registry)
GOAL:           One or two sentences: the outcome, not the method.
REQUIREMENTS:   MEDIA-002 (contracts/requirements.json)
CONTEXT:        docs/design/EVENT-AUDIO-SPEC.md section 2; contracts/event-catalog.json
WORKSPACE:      F:\kohbor, branch m2/events, single writer: this task
DOMAIN:         events-audio   (card: docs/domains/events-audio/CARD.md)
CLAIM:          events-audio__S1-T2   (claim file in the coordination ledger)
SCOPE:          crates/kohbor-events/src/**            (write)
READ_ONLY:      crates/kohbor-events/tests/**, contracts/**, fixtures/**
OUT_OF_SCOPE:   contracts, generated documents, package-manifest.json, git commit/branch/push
RISK:           R1
ACCEPTANCE:     python tools/gate.py --quick           (exit 0)
EVIDENCE_CLASS: synthetic
FAILURE:        What the code must do when input is missing, stale or invalid.
OPEN_DEPENDENCIES: assumption E-4 unconfirmed -> multi_kill stays unavailable at runtime
REPORT:         JSON per tools/agent/worker-report.schema.json
```

Standing rules, valid for every brief without being repeated:

1. Follow `F:\kohbor\AGENTS.md`. Unknown is a real state; never fabricate data, readiness or evidence.
2. Write only inside SCOPE. A needed change outside it is a `scope_requests` entry.
3. Do not commit, switch branches, push, publish or delete user data.
4. Do not start the desktop app, build the release profile or write under `target\release` while the owner's build may be running. Debug builds and tests are allowed.
5. Do not add or upgrade a dependency unless the brief names it.
6. Do not weaken a threshold, delete a test or mark a skipped check as passed.
7. Report what actually ran. `not_run` lists every required check that did not run, with the reason.
8. Read the card of your DOMAIN first. A card is a map, not evidence: if it disagrees with a decision, a requirement, the code or a test, report the gap. Never edit a card, the registry or a claim file; report what the card needs in `open_questions`, `deviations` and `files_changed`.

## 5. Running a worker

The lead launches workers non-interactively and in the background, then continues with the next brief. Brief, log and report files live in a run directory outside the repository. The durable record is the backlog row in SPRINT-ROADMAP.md and the commit.

```bash
codex exec -m gpt-6-luna -c model_reasoning_effort=max -s workspace-write -C F:/kohbor --color never --output-schema tools/agent/worker-report.schema.json -o "$RUN/S1-T2.report.json" - < "$RUN/S1-T2.brief.md" > "$RUN/S1-T2.log" 2>&1
```

```bash
codex exec -m gpt-6.1-terra -c model_reasoning_effort=high -s read-only -C F:/kohbor --color never --output-schema tools/agent/worker-report.schema.json -o "$RUN/S1-T3.report.json" - < "$RUN/S1-T3.brief.md" > "$RUN/S1-T3.log" 2>&1
```

- **Sandbox.** Writers run with `-s workspace-write`; questions and reviews run with `-s read-only`. The user-level Codex default on this machine is wider (`danger-full-access`). The lead passes the narrower mode on every launch. If a build cannot run inside `workspace-write`, the lead reports that to the owner and does not widen the sandbox on its own.
- **One writer.** Only one writing worker runs at a time in the checkout. A read-only reviewer or question may run beside it. No worktree is created per task (AGENTS.md).
- **Fix rounds.** A returned task goes back to the same session with `codex exec resume <session-id>`, so the worker keeps its context and the lead sends only the findings. The session id is in the first lines of the log.
- **Reassess.** When a fix round produces no new evidence, the lead re-examines the brief, the specification and the environment before sending another round.

## 6. Acceptance gate and the lead's reading budget

Every task has a risk tier in its brief. The tier decides what the lead reads.

| Tier | Applies to | The lead reads |
| --- | --- | --- |
| R0 | Documents, fixtures, mechanical edits, dev tooling | Report, changed-path list, gate summary |
| R1 | Product code inside one module, no trust boundary | R0 items, the test worker's review findings, and only the hunks named in `review_focus` or in a finding |
| R2 | A trust boundary (list below), or any change to a contract or schema | R1 items and the **full diff of the R2 files** |

R2 paths today: `apps/desktop/src-tauri/src/{intake,secrets,containment,instance,atomic}.rs`, `apps/desktop/src-tauri/tauri.conf.json` and capability files, `crates/kohbor-gsi/**` admission and ownership code, anything that writes outside the app data directory, package verification, signature and trust-domain code, `contracts/**`. The lead extends this list when a new boundary appears.

Gate steps, in order. A failed step returns the task.

1. **Report.** Read the report JSON. `BLOCKED` or `PARTIAL` goes to the owner or back to the worker with the stated resume condition.
2. **Scope.** `git status --porcelain` and `git diff --stat`: every changed path is inside SCOPE. A path outside it fails the gate regardless of content.
3. **Checks.** The lead runs the acceptance command itself on the resulting tree. A worker's claim that a check passed is not evidence.
4. **Review.** R1 and R2: the test worker reviews the diff read-only and returns findings with `path:line`. The lead reads the findings and the named hunks.
5. **R2 read.** The lead reads the full diff of R2 files.
6. **Integrate.** The lead updates `implementation_refs` in the registry, regenerates RTM/SRS, refreshes the validation report and the package manifest, and records the result in the backlog.
7. **Commit.** Only under the commit authority the owner gave for the sprint. Push, pull request and merge always need a separate owner instruction.

The acceptance command is one script, `tools/gate.py` (built in Sprint 0). It runs formatting, lints, tests, the frontend build and the specification validator, prints one line per check and at most 20 lines in total, and writes full logs to a file. Until it exists, the lead runs the commands listed in M1-STATUS section 3.2.

**Evidence classes.** A worker run produces source or synthetic evidence. Native observations, live matches, acoustic timing and resource/FPS measurements are separate classes that the owner's machine or certified hardware produces. The gate never converts one class into another.

## 7. Rules that keep the lead's cost low

1. The lead does not open source files to explore. It asks a **Q** task.
2. Briefs point to documents; they do not quote them.
3. Worker output goes to files. The lead reads the report JSON and the gate summary, not the log. It searches the log only after a failure.
4. Sprint state lives in SPRINT-ROADMAP.md, not in the conversation.
5. Fix rounds resume the worker's session; the lead does not restate the task.
6. Review is delegated. The lead reads findings and named hunks, and full diffs only for R2.
7. Generated files change through their scripts, never by hand.

The limits above (60-line brief, 30-line answer, 20-line gate summary) are targets. Sprint 0 was the first run of the workflow; its observations follow. Token use was **not** measured: the counts below are rounds and reads, which is what the lead can observe.

### 7.1 Sprint 0 observations (2026-11-11 to 2026-11-12)

| Observation | Rule it produces |
| --- | --- |
| Fifteen worker runs. Every brief was 13–22 lines, so the 60-line target was met, but the lines are long (a field often carries a paragraph); a line count does not measure brief size. The resource-admission statement and the lead's design decision were the long parts and prevented rework. | Measure briefs in words, not lines (target: instructions under 400 words, plus the resource and decision blocks). |
| T-L-R produced real value: the independent reviews found a resume path that overwrote the hero baseline, a listener timeout that might end the accept loop on Linux (measured later: it does not on Windows), `settings_get` returning defaults for a corrupt file, a `gsi_setup` revision bump that reset 7.0 to 2, and a silently changed Thai string. Luna's own reports mentioned none of these. | Never accept an implementation report as the review. R2 always gets a Terra read-only review before commit. |
| Lead edits that were still needed: a one-line behaviour regression (a mismatch lost its reason code), a lint attribute, a revision-bump recovery rule, a wrong cold-start test caused by the lead's own brief. | The lead reads every R2 diff in full (as designed) and owns defects in its own briefs. State the spec section a test encodes; a brief that guesses a requirement produces a wrong test. |
| A worker report can claim a wrong cause ("existing clippy errors" were introduced by the previous task). | Re-run the check and compare against the previous commit before accepting a cause. |
| Workers could not build or run Vite inside the narrow sandbox; the owner's resource-admission rule blocked them until the brief carried the measured floor, the owned lane and `--add-dir`. | Every build task brief states: lane path, measured free space and floor, expected peak write, the lock file, and passes `--add-dir <lane>`. The lead runs the Vite build and anything needing process spawn outside the sandbox. |
| Workers sometimes report in Thai or paste a Thai summary. | Briefs say "Write the report in English" (done from S0-3 on). |
| Terra does not need `danger-full-access` for tests when the lane is added with `--add-dir`. | Keep `workspace-write` for tests, `read-only` for review. |
| Fix rounds were new sessions with the full brief restated, not resumed sessions, because `codex exec` was used each time. | Cost is higher than rule 5 assumes. Try `codex exec resume` for fix rounds in Sprint 1 and compare. |

Limits after Sprint 0: briefs are bounded in words (instructions under 400 words, plus the resource and decision blocks); a worker report is read through its `summary`, `deviations`, `scope_requests` and `open_questions` only; the lead reads the log only after a failure; a task that needs more than two fix rounds is stopped and re-planned.

### 7.2 Sprint 1 observations (2026-11-12)

| Observation | Rule it produces |
| --- | --- |
| Nine worker runs (Terra 2, Luna 7). Terra again could not compile its tests because the brief named the lane but not the complete admission basis; the lead compiled and ran the red tests. | The brief's resource block is copied from the last passing brief and the lane lock file is updated for every task before launching. |
| Run S1-5 ended BLOCKED because the lane `LOCK` file still named the previous task. The worker was right to stop. | Update the lock file in the same command that launches the worker. |
| Tests written from the specification passed while the specification rule was wrong for the real game (`hero.alive` is not a death signal). Replaying six recorded matches found it; the review by reading did not. | A detector slice is not closed on spec-derived tests alone: replay a real corpus and compare with ground truth before the gate. |
| The reviewer proposed seven points. The lead accepted five, rejected one because availability belongs to the host status, and kept one as an open spec question. | A review finding is accepted, rejected or parked with a stated reason, never ignored. |
| The lead ran one `cargo check` without the lane variable and wrote debug build output into `F:/kohbor/target/debug` (the release binary was not touched). | Set `CARGO_TARGET_DIR` in the same shell command as every cargo call, including lead checks. |
| `codex exec resume` for fix rounds was still not tried; every fix round restated the brief. | Carry this experiment to Sprint 2. |

### 7.3 Sprint 2 observations (2026-11-12)

| Observation | Rule it produces |
| --- | --- |
| Terra again could not build in its first run: I wrote the resource block by hand instead of copying the last passing brief, which was the rule written in 7.2. | Copy the resource block verbatim from the last passing brief; never retype it. |
| `codex exec resume --last` works for fix rounds and kept the whole context, but the resumed run lost the `--add-dir` writable root, so the worker could not build. Terra stopped PARTIAL twice on large test matrices (16 of about 57 tests, then 39 of about 150). | Pass `-c 'sandbox_workspace_write.writable_roots=["<lane>"]'` on resume; ask for test counts in the brief and compare them with the report. |
| Terra's test fixtures had two defects (ZIP entries not in the required order; an unsafe entry placed after the ordering violation). Luna correctly reported them as scope requests instead of editing tests. | The lead fixes a test fixture only after the worker proves the cause; run the full suite after each fix. |
| A read-only review (Terra, no cargo) found one real blocker (files not flushed before the registry commit) that the step-boundary crash tests cannot see. | Crash tests at step boundaries do not cover power loss; say so in the evidence and keep the native item open. |
| Two workers ran in parallel (review read-only, authoring tool in a new crate) without conflict because the review did not touch the lane. | Parallel work is safe when scopes are disjoint and only one worker holds the cargo lane. |

### 7.4 Sprint 3 observations (2026-11-12)

| Observation | Rule it produces |
| --- | --- |
| `codex exec resume --last` selected the newest session in the directory, which was another running worker's. The system refused (active writer), but a finished session would have been resumed by mistake. | With parallel workers always resume by explicit session id (printed as `session id:` in the log). |
| A worker set `CARGO_TARGET_DIR` to a wrong lane in its first command and created a stray directory. | Type the lane in every cargo command of the brief; the lead checks `F:\cargo-lanes` after each run. |
| A worker that added a dependency offline got a Cargo.lock that silently downgraded 11 unrelated packages. | The lead reads `git diff Cargo.lock` after every dependency change and rebuilds the lock edit by hand if needed; briefs say "STOP and report BLOCKED" on any other version change. |
| Terra stopped PARTIAL on large test matrices (16 of 57, 32 of 70, 39 of 150). A resume with the missing groups listed completed them every time. | Ask for counts per group in the brief and compare with the report; resume with the missing groups named. |
| Two workers in one crate break each other's builds while one edit is half done. | One writer per crate at a time; parallel only across crates (different lanes) or with docs and renderer work. |
| Review by reading found a blocker the step-boundary tests cannot see (device may outlive the deadline). | Keep a read-only review step before the lead's own diff read on every R2 slice. |
| Unit tests used `env!("CARGO_TARGET_TMPDIR")`, which exists only for integration tests; the worker hid it with an environment variable on its own command line. | Run the plain gate command (no extra environment) before accepting a worker's tests. |

### 7.5 Domain card cold-read test (2026-11-13, process observation, not product evidence)

One fresh read-only Claude subagent (Sonnet) was given only the registry and five cards (account, events-audio, packs, commerce, banner-lab) and five questions written by the card author. The author scored the answers against the documents. One run, one model, questions written by someone who knew the answers: the result is an indication, not a measurement. It cost about 129,000 tokens and 46 seconds (harness figures). Only 5 of the 9 cards were tested (governance, host-core, vision-risk and ui-overlay were not) and the cards were edited afterwards without a retest: the I1 exit criterion (five questions per card) is NOT met, and neither is the I3 criterion (one sprint task run through claim, work, handover and release).

| Observation | Rule or card change it produced |
| --- | --- |
| All five answers were correct and cited the card section; every unverified point was labelled INFERRED or UNKNOWN; no statement was wrong. | None needed. |
| Q2 (is the critical warning audible today) needed a join across two cards. | The events-audio card now states the answer in one line with its basis. |
| Q5 (what a decision does not approve) was UNKNOWN: cards held ids only. | Cards state the limit of a decision when a neighbour could misread it (banner-lab: DEC-BANNER-LAB-STYLE). |
| "Decided" next to "owner still confirms" read as provisional. | A decision made by delegation to an agent review is labelled as such in the card. |
| The commerce card forbade design but did not say what an agent may do. | It now says: draft neutral questions from repository facts, nothing else. |

## 8. What stays with the owner

- Open decisions in `contracts/decisions.json`. An open decision blocks only the tasks that name it.
- Live matches and captures that confirm detector assumptions.
- Native and acoustic measurements.
- Media rights and package provenance.
- Commit authority per sprint; every push, pull request, merge, release or cleanup.

## 9. Domains, claims and handover

Added 2026-11-13 under the owner-approved Domain Steward design (process only: no requirement or product behaviour changes). Terms and files: [Domain Registry](../domains/README.md) (the registry of domains; not the requirement registry). A **steward** is a role held for one task by the lead that owns the domain's claim; it is not a long-running agent. Coordination state (claims, branch and worktree registry) lives outside the repository in `F:\kohbor-coord\`; its README is the format reference.

### 9.1 Task start (four reads, then a claim)

1. `AGENTS.md`, then `docs/INDEX.md` and `docs/domains/README.md`; read the card of every domain you will touch.
2. List the claim files in the coordination ledger. Do not open a claim whose `write_area` overlaps an active one, or for a task id that has an active claim or a `passed` backlog row. Work with no task id gets one first (SDP backlog template).
3. Create `<domain>__<task>.json` in `claims\` with "fail if exists" semantics. Fields: `domain`, `task`, `write_area`, `session`, `model`, `started`, `expires` (a day at most), `status`, `branch`, `worktree`, `note`. Only the lead claims; workers are covered by the lead's claim.
4. An expired claim does not free its area. Check whether the holder session still runs, then ask the owner before taking over.
5. The ledger is advisory, not a lock: only the exact same claim file name is refused. Qualify `write_area` by branch or worktree, so two claims on different branches do not collide.
6. Shared integration files (`package-manifest.json`, RTM, the SRS atomic section, `acceptance/spec-validation.json`, `docs/INDEX.md`, `docs/governance/CHANGELOG.md`) are written by every lead on its own branch. They are not exclusive to the governance claim; whoever merges regenerates the generated ones after the merge (`tools/update_manifest.py --apply`, `tools/render_requirements.py`).

### 9.2 Task end (extends gate step 6, before the commit)

The lead that holds the claim does these in order; a task is not done until `tools/validate_domains.py` passes.

1. Backlog row in SPRINT-ROADMAP.md.
2. `implementation_refs` and `evidence_refs` in the registry; regenerate RTM and SRS.
3. `contracts/decisions.json` and the decision log, only if the owner decided something.
4. The card: add new artefacts, refresh `reviewed_sha256` of the artefacts the task read in full, rewrite "Now / Next / Blocked" if it changed, move answered questions out, set `verified_at_commit`. Empty when the task added no artefact and changed no status.
5. Manifest refresh (`tools/update_manifest.py --apply`).
6. Close the claim (`status: done`; the file is kept) and register any branch, worktree or private copy you created.
7. Memory: write nothing, unless the owner stated a preference about how agents should work.

Source-class evidence only: the validator says nothing about the application.

### 9.3 Branches, worktrees, private copies (preventive)

- One branch per sprint or slice, named `<domain>/<task-or-slice>-<slug>`; existing sprint names stay valid. Never one per task or per review.
- A worktree only for a demonstrated concurrent write need (AGENTS.md): two writers, disjoint write areas, both needed now. Never for a reviewer, a consultant or a small task. More than 3 worktrees in total is a red flag that goes to the owner (the design proposed a cap of 2 extra worktrees; 3 in total is the janitor's current red-flag line, to be tightened by the owner). Folder `<repo>-wt\<domain>__<task>__<session>` when you choose the location yourself.
- Start sessions on the existing checkout where you can: read-only sessions never need a worktree. The Claude desktop app can create a worktree for a session by itself (observed 2026-11-13: `.claude\worktrees\<name>`); the tool setting that prevents it is UNVERIFIED. A tool-created worktree is recorded with its reason in the same task, is reported to the owner, and a third worktree needs the owner first. The janitor reports an unregistered one the same day.
- Private copies for comparisons: `<source>__cmp-<purpose>__<model>`, registered before creation with source, purpose, session and disposal condition.
- Push, pull request, merge and every deletion need the owner (section 8). After a merge the registry record becomes `merged`; `parked` needs a resume condition and a review date; `abandoned` needs the owner.

### 9.4 Janitor (report only)

`tools/janitor.py` lists branches, worktrees, stashes and registered private copies, joins them with the registry and the claims, and prints a report. It deletes, prunes and resets nothing and has no option to. Items it proposes (merged into main and origin, not checked out, no claim, nothing unpushed) need the owner's written decision for each item; no standing deletion authority exists. Run it at every sprint exit and when a red flag is suspected: any unregistered item, more than 3 worktrees, more than 15 branches. Reports are stored outside the repository (`registry\janitor-reports\`).

### 9.5 Delegated approval (pilot)

Only WORK approval (code and documents) can be delegated, only by the owner, and only under a versioned charter `docs/domains/<id>/CHARTER.json` that the owner has confirmed (`enabled` true). Pilot: [governance](../domains/governance/CHARTER.json) only; it is written by an agent and ships disabled until the owner confirms it. R0 and R1 approvals follow the charter; an R2 approval is only what the charter allows and is always flagged for owner audit. A delegated approval never grants commit, push, pull request or merge authority. The kill switch is `"enabled": false` in a charter or a file `DELEGATION-OFF` in the coordination ledger; check both at task start and before each approval. If the ledger cannot be read, delegation is off. When it turns off, pending requests go to the owner and approvals already recorded are listed for re-audit. Records use exactly `agent_review`, `agent_delegated_approval` or `owner_approval`; never write an agent result as human or owner approval. The author, the reviewer and the approver are different sessions, and the reviewer is a different model from the author. Taste, rights and legal questions, merge to main, push or pull request, release, deployment, payments and deleting user data or unregistered work stay with the owner.
