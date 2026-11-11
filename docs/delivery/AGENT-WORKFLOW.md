---
title: "Agent Workflow"
doc_id: "AGENT-WORKFLOW"
status: draft
version: "0.3.6"
updated: "2026-11-11"
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
GOAL:           One or two sentences: the outcome, not the method.
REQUIREMENTS:   MEDIA-002 (contracts/requirements.json)
CONTEXT:        docs/design/EVENT-AUDIO-SPEC.md section 2; contracts/event-catalog.json
WORKSPACE:      F:\kohbor, branch m2/events, single writer: this task
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

## 8. What stays with the owner

- Open decisions in `contracts/decisions.json`. An open decision blocks only the tasks that name it.
- Live matches and captures that confirm detector assumptions.
- Native and acoustic measurements.
- Media rights and package provenance.
- Commit authority per sprint; every push, pull request, merge, release or cleanup.
