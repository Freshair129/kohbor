---
title: "Domain Registry"
doc_id: "DOMAIN-REGISTRY"
status: draft
version: "0.1.0"
updated: "2026-11-13"
authority: "Process only. The owner approved nine decisions of the domain-steward design on 2026-11-13 (see CHANGELOG 0.3.6); this registry text was written by an agent, is not reviewed by a human and defines no product requirement"
owner_role: "Governance lead (integration owner)"
approval_status: "not_approved"
---

# Domain Registry

A **domain** is a set of write areas (paths) with its own decisions and documents. A **steward** is a role, not a long-running agent: the session that holds the domain's claim, reading the domain's **card**, and writing the card back at task end. A card is a **map, not evidence**: it holds facts that have no other home plus ids that point to the home. The authority order is unchanged: decisions, PRD/SRS, contracts, code and tests, then the card.

Path: `docs/INDEX.md` (all documents) → this registry → `docs/domains/<id>/CARD.md` (readers) and `state.json` (tools).

## 1. Registry

Tier A and B have a card. Tier C is a registry row only; the "answers for" column names the card to read.

| id | tier | Card | Owns (write areas, short) | Answers for / notes | Default modules |
| --- | --- | --- | --- | --- | --- |
| `events-audio` | A | [card](events-audio/CARD.md) | `crates/kohbor-events`, `kohbor-audio`, `kohbor-audio-native`; host critical path in `apps/desktop/src-tauri/src/{audio,media,host.rs,commands.rs,clock.rs}` | Event detection, arbitration, native audio, critical warning path | none |
| `packs` | A | [card](packs/CARD.md) | `crates/kohbor-pack`, `kohbor-pack-author`; media-manifest and install schemas | Verified package store, authoring tool, trust domains | none |
| `account` | A | [card](account/CARD.md) | Account and sign-in design (no crate exists yet) | Account, identity, friend code; sharing is a row below | none |
| `banner-lab` | A | [card](banner-lab/CARD.md) | `lab:` workspace outside the repository | Banner production lab; hands a clip folder to `kohbor-pack-build` | none |
| `governance` | A | [card](governance/CARD.md) | `docs/governance/**`, `docs/INDEX.md`, `docs/domains/**`, `docs/delivery/{AGENT-WORKFLOW,SPRINT-ROADMAP}.md`, `contracts/{requirements,decisions,product-needs}.json`, generated RTM/SRS/manifest, `tools/*.py`, `AGENTS.md` | Integration owner of every other domain | none |
| `commerce` | B | [card](commerce/CARD.md) | Nothing yet | Store, wallet, payments, entitlement: not designed | premium |
| `host-core` | B | [card](host-core/CARD.md) | `crates/kohbor-gsi`, `kohbor-gsi-capture`; host startup, containment, settings, runtime status | Game-state intake and host shell | none |
| `vision-risk` | B | [card](vision-risk/CARD.md) | Future capture, vision and risk crates | M3 work; nothing built | none |
| `ui-overlay` | B | [card](ui-overlay/CARD.md) | `apps/desktop/src/**`, design tokens, `docs/design/visuals/**` | Renderer, overlay window, visual design | none |
| `sharing` | C | answers: `account` | Peer relay runtime (future) | Split from `account` when M6 work is planned | peer |
| `banner-studio` | C | answers: `banner-lab` | Nothing built | Promote when the owner approves the first Studio increment | none |
| `announcer-voice` | C | answers: `banner-lab` | `lab:` voice pipeline | Synthetic announcer voice. Rights belong to the owner's rights team: agents record statements, never decide | none |
| `local-voice` | C | none | Nothing yet | Local push-to-talk conversation (VOICE-*, M5) | voice, mem, advice, QUALITY-006 |
| `release-ops` | C | answers: `governance` | `docs/operations/RUNBOOK.md`, updater (future) | Signing and publishing need owner authorization | update, obs, QUALITY-008 |

Name collisions that already misled searches: **store** means the local package store (`Store::install` in `kohbor-pack`) and also the shop: use `pack store` and `commerce`. **voice** means the local conversation (VOICE-*) and also the synthetic announcer: use `local-voice` and `announcer-voice`.

**Default modules** name the requirement modules (or single requirement ids) whose requirements belong to that domain when no card lists them. The primary domain of a requirement is the one card that lists it in `requirement_ids`, else the one row that names its module or id here, else it is UNASSIGNED (reported). A card that depends on a requirement owned elsewhere lists it in `interface_requirement_ids`. The map is generated: `python tools/validate_domains.py --map`.

A domain gets a card when it (1) has a write area that can be claimed separately, (2) has had two sessions or will in the next sprint, or has three or more governing documents, and (3) has at least one owner decision or open DEC id. A card nobody touched for two sprints is merged or retired; nothing is deleted without the owner.

## 2. Card format

`CARD.md` (at most 120 lines; the hand-written "Now / Next / Blocked" block at most 12 lines; every line ends with a DEC id, requirement id, task id or path):

```text
front matter: domain, tier, card_version, verified_at_commit, verified_on, verified_by
1. Purpose and boundary   owns / must not touch / interfaces (neighbour domain ids)
2. Owner decisions        DEC ids only, grouped decided / open / deferred. The text lives in contracts/decisions.json
3. Now / Next / Blocked   hand-written
4. Open questions         only questions with no DEC id and no assumption id; each has an owner and a date
5. Artefacts              short list; the machine list is state.json
6. Traps and lessons      at most 10 bullets, each cites where it was learned
7. Who may write, how to claim, how to ask
```

`state.json` (machine-read by `tools/validate_domains.py`): `schema_version`, `domain`, `tier`, `verified_at_commit`, `verified_on`, `verified_by`, `write_areas[]`, `requirement_ids[]`, `proposed_requirement_ids[]` (not in the SRS, not checked), `decision_ids[]`, `task_id_prefixes[]`, `artefacts[]` (`path`, `type`, `role` of `governing`, `reference`, `source-of-truth` or `neighbour:<id>`, optional `reviewed_sha256`, optional `exists: false` for planned paths), `open_questions[]`, `last_handoff`.

Never copy into a card: a requirement status, a decision text, a backlog row, a hash list of the tree. Never put in a card: a secret, a token, an email address, a device name, a drive-letter path outside the repository, a service endpoint. Use a logical root (`lab:`, `packs-src:`); the mapping to machine paths lives in the coordination ledger (`registry\branches.json`, key `logical_roots`).

## 3. How to ask (summon ladder)

1. **Read the card** (and `state.json` if you need a path or hash). Stop if it answers.
2. **Read-only consultant session** primed with the card and the listed documents, when the card is stale, or the question needs several documents, or the lead must keep its context small. A consultant never writes state; its answer is a claim the lead verifies.
3. **Message the session that holds the domain's active claim** only for in-flight facts not yet in files. The answer is a claim, not evidence.
4. **Ask the owner** when the answer is a decision, taste, rights, money, or something only a person can hear or see.

Never use step 3 for history and never step 2 for a decision. The consultant prompt skeleton is in `DOMAIN-STEWARDS.md` section 7 (design, outside the repository); a repository copy is part of increment I5.

## 4. Claims, handover, charters

- Claims and the branch, worktree and private-copy registry live outside the repository in `F:\kohbor-coord\` (README there). One file per claim, written before the first write, closed by the lead at task end.
- The task-end handover rule is in [Agent Workflow](../delivery/AGENT-WORKFLOW.md) section 9. Workers never edit cards: they report what a card needs in their report.
- Delegated approval is possible only under a charter, `docs/domains/<id>/CHARTER.json`. Today exactly one exists, written by an agent and disabled until the owner confirms it: [governance](governance/CHARTER.json). The kill switch is `"enabled": false` in a charter, or a file named `DELEGATION-OFF` in the coordination folder; if the folder cannot be read, delegation is off.
- Janitor: `tools/janitor.py` reports branches, worktrees and private copies. It deletes nothing.
