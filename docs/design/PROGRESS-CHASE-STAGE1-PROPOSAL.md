---
title: "Progress Chase: Stage 1 Wording Proposal"
doc_id: "PROGRESS-CHASE-STAGE1"
version: "0.1.0"
status: "draft"
updated: "2026-11-14"
owner: "Governance lead (integration owner)"
approval_status: "not_approved"
---

# Progress Chase: Stage 1 wording proposal

**Agent proposal, not an owner decision, not applied.** Stage 0 (tooling) is built: claim tags, tag checks, the chase list. Stage 1 would make the working rules in [Agent Workflow](../delivery/AGENT-WORKFLOW.md) use them. This page only drafts the wording. `AGENTS.md` and `AGENT-WORKFLOW.md` are owner-reserved (a change to either is a change to the process); nothing here edits them, and each proposal says what it would replace or add.

## What Stage 0 provides (tooling only)

| Piece | Where | Behaviour |
| --- | --- | --- |
| Claim tags | `tools/claim.py` (`--tag`, `tag NAME --add`, `renew NAME --hours N`, `close --done-tag ID=PATH --open-tag ID`) | A claim names what it serves. Pool: a requirement id, `TASK:<sprint-table task>`, a `DEC-` id, or `PROC:<slug>` (process work, counted separately, never product progress). Closing computes the evidence file's SHA-256 and the commit and prints one `Tags:` line |
| Tag checks | `tools/check_claim_scope.py` | T1 an active claim on the branch has no valid tag; T2 a tag is not in the pool; T3 the branch changes non-shared paths and no claim on it has a valid tag; T4 a `tags_done` evidence file is missing or its hash differs. Report-only; they fail only under `--enforce`, which `tools/gate.py --sprint-exit` already passes |
| Chase list | `tools/progress_map.py --ref origin/main --chase` | `chase.md`: unowned P0 and P1 requirements, claims more than 24 h past expiry, T1, T2 and T4 findings, the newest janitor report, and the decisions that need the owner |

A tag is never compared with a claim's write area: the tag says what the work serves, the write area says where it writes.

## Proposed wording

Each block is the proposed new text. Quote marks are mine, not part of the text.

### 1. Section 9.1 item 3 (create the claim): add the tag

Replace the sentence that lists the claim fields with:

> Create `<domain>__<task>.json` in `claims\` with "fail if exists" semantics, using `tools/claim.py open`. Fields: `domain`, `task`, `write_area`, `session`, `model`, `started`, `expires` (a day at most), `status`, `branch`, `worktree`, `note`, **and `tags`: at least one tag from the pool of `tools/tagpool.py`: a requirement id, `TASK:<sprint task>`, a `DEC-` id, or `PROC:<slug>` for process work**. Add a tag later with `claim.py tag`; extend the expiry with `claim.py renew`. Only the lead claims; workers are covered by the lead's claim.

### 2. Section 9.1: a new item 7 (read the chase list)

> 7. Before choosing work, read `F:\kohbor-local\progress\chase.md`. If it is more than a day old, regenerate it (`python tools/progress_map.py --ref origin/main --chase`). Prefer a P0 or P1 requirement listed as unowned over a new topic, and tag your claim with it. The list is advice: it assigns nothing, and the owner's instruction for the task still wins. A claim listed as more than 24 h past expiry is not free to take: apply item 4.

### 3. Section 9.2 item 6 (close the claim): record what finished

Replace item 6 with:

> 6. Close the claim with `claim.py close NAME --done-tag ID=EVIDENCE_PATH` for every tag the task finished (the evidence file is a receipt, a test log or a document that shows it) and `--open-tag ID` for every tag it did not; paste the printed `Tags:` line into the handoff. A tag you do not name stays open. Then register any branch, worktree or private copy you created.

### 4. Section 6 (acceptance gate): sprint exit fails on tag findings

Add after the gate steps:

> **Sprint exit.** `tools/gate.py --sprint-exit` runs the claim-scope check with `--enforce`. Besides a path outside the claim, it then fails on T1 (an active claim with no valid tag), T2 (an unknown tag), T3 (a branch that changes non-shared paths with no tagged claim) and T4 (a `tags_done` evidence file that is missing or changed). Process work tagged `PROC:` passes the checks but is counted separately and never as product progress.

### 5. Handoff template

Add one line after "Status": `- Tags: (the line printed by claim.py close)`.

## Costs and risks to weigh before Stage 1

- **Migration.** All active claims in the ledger today are untagged (the measured number is in the Stage 0 report). Before sprint-exit enforcement is on, each needs `claim.py tag`, or it closes and a tagged one opens. Turning enforcement on first would fail every sprint exit.
- **Tag pool drift.** The pool is read from the requirement registry, the decision register and the sprint tables. A renamed requirement or a re-numbered sprint task makes an old tag T2. The check says so; it does not guess.
- **T3 is blunt by design.** A branch that edits only shared integration files is exempt; any other change needs a tagged claim. A claim with a `PROC:` tag satisfies it, which is the intended escape for process work, and the reason such work is counted separately.
- **Gaming.** A tag proves nothing about the work. T4 only checks that the named evidence file exists and still has the hash recorded at close; it does not judge what the file says. Verified status still needs the requirement registry's `evidence_refs` and `new_build_status`.
- **Cost for every task.** One option on open, one or two at close, one file to read at task start.
- **The scheduled job** that regenerates `chase.md` is the owner's to create (see `docs/operations/PROGRESS-CHASE-TH.md`); without it the file goes stale and item 7 says to regenerate it by hand.

## Questions for the owner

1. Apply the five wording changes above, a subset, or none?
2. When should `--sprint-exit` start failing on tags: from the next sprint, or after the active claims are tagged?
3. Is `PROC:` the right escape for process work, and should the chase list count it somewhere (it does not today)?
4. Who creates the scheduled job, and should `chase.md` also be copied anywhere an agent reads first?
