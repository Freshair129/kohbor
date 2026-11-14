#!/usr/bin/env python3
"""The claim tag pool and the per-claim tag checks (Stage 0 of the progress chase). Standard library only; reads files, writes nothing.

A claim names what it serves with tags. A tag is one of:

- a requirement id from ``contracts/requirements.json`` (``MEDIA-004``);
- ``TASK:<first token of a sprint-table Task cell>`` from ``docs/delivery/SPRINT-ROADMAP.md`` (``TASK:S1-1``);
- a decision id from ``contracts/decisions.json`` (``DEC-ACCOUNT-ORIGIN``);
- ``PROC:<slug>`` for process work (counted separately, never product progress; needs no source).

Used by claim.py (validation on open and ``tag``), check_claim_scope.py (T1..T4) and progress_map.py (the chase list).
Findings: T1 an active claim has no valid tag; T2 a tag is not in the pool; T4 a ``tags_done`` evidence file is missing or its
hash differs. (T3 is a property of the branch, not of one claim, and lives in check_claim_scope.py.)
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path
from typing import Any

PROC_RE = re.compile(r"^PROC:[a-z0-9][a-z0-9-]{0,59}$")
SPRINT_TASK = re.compile(r"^S\d+-\d+[a-z]?$")


def _read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def sprint_task_tokens(text: str) -> set[str]:
    """First token of the Task cell of every row of a table inside a ``## Sprint N`` section (not the Backlog table)."""
    out: set[str] = set()
    inside = False
    for line in text.splitlines():
        if line.startswith("## "):
            inside = line.startswith("## Sprint ")
            continue
        if inside and line.startswith("|") and not re.match(r"^\|\s*-+", line):
            cell = line.strip().strip("|").split("|", 1)[0].strip()
            token = re.split(r"[\s,;]+", cell, maxsplit=1)[0] if cell else ""
            if SPRINT_TASK.match(token):
                out.add(token)
    return out


def load_pool(repo: Path) -> dict[str, Any]:
    pool: dict[str, Any] = {"requirements": set(), "decisions": set(), "tasks": set(), "errors": []}
    for key, rel, field in (("requirements", "contracts/requirements.json", "requirements"), ("decisions", "contracts/decisions.json", "decisions")):
        p = repo / rel
        if not p.is_file():
            pool["errors"].append(f"{rel} not found")
            continue
        try:
            pool[key] = {x["id"] for x in _read_json(p).get(field, []) if isinstance(x, dict) and x.get("id")}
        except (OSError, ValueError, AttributeError) as exc:
            pool["errors"].append(f"{rel} unreadable: {str(exc)[:80]}")
    p = repo / "docs" / "delivery" / "SPRINT-ROADMAP.md"
    if p.is_file():
        try:
            pool["tasks"] = sprint_task_tokens(p.read_text(encoding="utf-8-sig"))
        except OSError as exc:
            pool["errors"].append(f"docs/delivery/SPRINT-ROADMAP.md unreadable: {str(exc)[:80]}")
    else:
        pool["errors"].append("docs/delivery/SPRINT-ROADMAP.md not found")
    return pool


def kind_of(tag: str) -> str | None:
    if tag.startswith("PROC:"):
        return "process"
    if tag.startswith("TASK:"):
        return "task"
    if tag.startswith("DEC-"):
        return "decision"
    if re.match(r"^[A-Z]{3,7}-\d{3}$", tag):
        return "requirement"
    return None


def is_valid(tag: str, pool: dict[str, Any]) -> bool:
    kind = kind_of(tag) if isinstance(tag, str) else None
    if kind == "process":
        return bool(PROC_RE.match(tag))
    if kind == "task":
        return tag[5:] in pool["tasks"]
    if kind == "decision":
        return tag in pool["decisions"]
    if kind == "requirement":
        return tag in pool["requirements"]
    return False


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def resolve_evidence(repo: Path, path_text: str) -> Path:
    p = Path(path_text)
    return p if p.is_absolute() else repo / p


def claim_findings(name: str, claim: dict[str, Any], pool: dict[str, Any], repo: Path) -> list[dict[str, str]]:
    """T1, T2 and T4 for one claim. T1 is for active claims only; T2 and T4 apply to any claim that carries tags."""
    out: list[dict[str, str]] = []
    tags = claim.get("tags") if isinstance(claim.get("tags"), list) else []
    pool_ok = not pool["errors"]
    valid = [t for t in tags if is_valid(t, pool)]
    unknown = [t for t in tags if not is_valid(t, pool)]
    judgeable = pool_ok or not tags or all(kind_of(x) is None for x in tags)  # with an incomplete pool a tag of an unreadable kind cannot be judged
    if claim.get("status") == "active" and not valid and judgeable:
        out.append({"code": "T1", "claim": name, "message": f"active claim {name} has no valid tag"})
    if unknown and (pool_ok or any(kind_of(t) is None for t in unknown)):
        out.append({"code": "T2", "claim": name, "message": f"claim {name} has a tag that is not in the pool: {', '.join(unknown)}"})
    done = claim.get("tags_done") if isinstance(claim.get("tags_done"), dict) else {}
    for tag, ev in sorted(done.items()):
        if not isinstance(ev, dict) or not ev.get("path"):
            out.append({"code": "T4", "claim": name, "message": f"claim {name}: tags_done {tag} has no evidence path"})
            continue
        target = resolve_evidence(repo, str(ev["path"]))
        if not target.is_file():
            out.append({"code": "T4", "claim": name, "message": f"claim {name}: evidence file for {tag} is missing: {ev['path']}"})
        elif ev.get("sha256") and sha256_file(target) != ev["sha256"]:
            out.append({"code": "T4", "claim": name, "message": f"claim {name}: evidence file for {tag} hash mismatch: {ev['path']}"})
    return out
