#!/usr/bin/env python3
"""Record every worker run and check separation of duties from the records (owner decision 9, 2026-11-13).

Records are appended, one JSON object per line, to ``<ledger>/runs/runs.jsonl`` (outside the repository). The model and
effort of a record come from the registry profile, never from the command line. A record whose (role, profile, task type,
domain) is not a registry route, or an alternative of one, is stored with ``off_route: true``: it is recorded honestly and
reported by ``check``.

Usage:
  python tools/run_record.py add --claim NAME --task-id ID --task-type TYPE --domain D --role R --profile P --session S
        [--brief FILE] [--report FILE] [--reviews TASK_ID] [--status DONE|PARTIAL|BLOCKED] [--owner-instruction TEXT] [--note TEXT] [--coord DIR]
  python tools/run_record.py check [--enforce] [--coord DIR]
  python tools/run_record.py list [--coord DIR]

``check`` is REPORT-ONLY unless ``--enforce``. Rule: for a record with ``reviews: <task id>`` the reviewer's session AND
model must differ from every writing run of that task id; a model that is not concrete is UNVERIFIABLE (reported, not a
pass). A writing run with a route that names a reviewer, and no review record, is reported. Process tooling; no product evidence.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import validate_agents as va  # noqa: E402

REQUIRED = ("run_id", "task_id", "task_type", "domain", "role", "profile", "model", "session", "recorded")
STATUSES = ("DONE", "PARTIAL", "BLOCKED")


def runs_file(opt: Path | None) -> Path:
    coord = opt or (Path(os.environ["KOHBOR_COORD_DIR"]) if os.environ.get("KOHBOR_COORD_DIR") else Path("F:/kohbor-coord"))
    if not coord.is_dir():
        raise SystemExit(f"run_record: coordination ledger {coord} not found; this tool does not create it")
    return coord / "runs" / "runs.jsonl"


def sha256_file(path: Path | None) -> str | None:
    return hashlib.sha256(path.read_bytes()).hexdigest() if path else None


def read_records(path: Path) -> tuple[list[dict], list[str]]:
    records: list[dict] = []
    problems: list[str] = []
    if not path.is_file():
        return records, problems
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            rec = json.loads(line)
        except ValueError:
            problems.append(f"runs.jsonl line {n}: not valid JSON")
            continue
        if not isinstance(rec, dict) or any(k not in rec for k in REQUIRED):
            problems.append(f"runs.jsonl line {n}: missing one of {', '.join(REQUIRED)}")
            continue
        records.append(rec)
    return records, problems


def build_record(reg: dict, domains: dict, a: dict) -> dict:
    prof = va._find(reg["profiles"], "id", a["profile"])
    if prof is None:
        raise ValueError(f"profile {a['profile']!r} is not in the registry")
    off_route = not va._is_route(reg, domains, a["domain"], a["task_type"], a["role"], a["profile"])
    tt = va._find(reg["task_types"], "id", a["task_type"])
    return {
        "run_id": f"{dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')}-{a['task_id']}",
        "claim": a.get("claim"), "task_id": a["task_id"], "task_type": a["task_type"], "domain": a["domain"],
        "role": a["role"], "profile": a["profile"], "tool": prof["tool"], "model": prof["model"], "effort": prof.get("effort"),
        "writes": bool(tt and tt["writes"]), "session": a["session"], "reviews": a.get("reviews"),
        "brief_sha256": sha256_file(a.get("brief")), "report_sha256": sha256_file(a.get("report")),
        "status": a.get("status"), "off_route": off_route, "owner_instruction": a.get("owner_instruction"), "note": a.get("note") or "",
        "recorded": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
    }


def check_records(records: list[dict], reg: dict | None = None) -> tuple[list[str], list[str]]:
    """Return (problems, notes). Pure: no file access."""
    problems: list[str] = []
    notes: list[str] = []
    seen: set[str] = set()
    for r in records:
        if r["run_id"] in seen:
            problems.append(f"run {r['run_id']}: duplicate run_id")
        seen.add(r["run_id"])
        if r.get("off_route") and r.get("owner_instruction"):
            notes.append(f"run {r['run_id']}: off the registry route by owner instruction: {r['owner_instruction']}")
        elif r.get("off_route"):
            problems.append(f"run {r['run_id']}: ({r['role']}, {r['profile']}, {r['task_type']}) in {r['domain']} is not a registry route")
        if r.get("status") not in (None, *STATUSES):
            problems.append(f"run {r['run_id']}: unknown status {r.get('status')!r}")
    writers: dict[str, list[dict]] = {}
    for r in records:
        if not r.get("reviews") and r.get("writes"):
            writers.setdefault(r["task_id"], []).append(r)
    reviewed: set[str] = set()
    for r in records:
        target = r.get("reviews")
        if not target:
            continue
        reviewed.add(target)
        authors = writers.get(target, [])
        if not authors:
            problems.append(f"run {r['run_id']}: reviews task {target}, which has no writing run recorded")
            continue
        for a in authors:
            if r["session"] == a["session"]:
                problems.append(f"run {r['run_id']}: the reviewer of {target} is the same session as the author run {a['run_id']} ({r['session']})")
            if r["model"] == a["model"] and va.concrete(r["model"]):
                problems.append(f"run {r['run_id']}: the reviewer of {target} used the same model as the author run {a['run_id']} ({r['model']})")
            elif not (va.concrete(r["model"]) and va.concrete(a["model"])):
                notes.append(f"run {r['run_id']}: model separation from {a['run_id']} is UNVERIFIABLE (a model is not concrete)")
    if reg is not None:
        for tid, authors in writers.items():
            a = authors[-1]
            route = next((x for x in reg["routes"] if x["task_type"] == a["task_type"] and x["domain"] in ("*", a["domain"])), None)
            if route and route.get("reviewer") and tid not in reviewed:
                notes.append(f"task {tid}: its route names a reviewer ({route['reviewer']['role']}) and no review run is recorded")
    return problems, notes


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    ap.add_argument("--coord", type=Path, default=None)
    sub = ap.add_subparsers(dest="command", required=True)
    add = sub.add_parser("add")
    for name in ("task-id", "task-type", "domain", "role", "profile", "session"):
        add.add_argument(f"--{name}", required=True)
    add.add_argument("--claim", default=None)
    add.add_argument("--brief", type=Path, default=None)
    add.add_argument("--report", type=Path, default=None)
    add.add_argument("--reviews", default=None)
    add.add_argument("--status", choices=STATUSES, default=None)
    add.add_argument("--note", default="")
    add.add_argument("--owner-instruction", default=None, help="quote or summary of the owner instruction that chose a model off the registry route")
    chk = sub.add_parser("check")
    chk.add_argument("--enforce", action="store_true")
    sub.add_parser("list")
    opts = ap.parse_args(argv)
    root = opts.root.resolve()
    path = runs_file(opts.coord)
    if opts.command == "add":
        reg, domains = va.load(root), va.domain_info(root)
        a = {k.replace("-", "_"): v for k, v in vars(opts).items()}
        try:
            rec = build_record(reg, domains, a)
        except ValueError as exc:
            print(f"run_record: {exc}", file=sys.stderr)
            return 2
        path.parent.mkdir(exist_ok=True)
        with open(path, "a", encoding="utf-8", newline="\n") as handle:
            handle.write(json.dumps(rec, ensure_ascii=False) + "\n")
        print(f"recorded {rec['run_id']}: {rec['role']}/{rec['profile']} {rec['model']} {rec['effort'] or ''}".rstrip() + (" OFF-ROUTE" if rec["off_route"] else ""))
        return 0
    records, problems = read_records(path)
    if opts.command == "list":
        for r in records:
            print(f"{r['run_id']}  {r['task_id']}  {r['role']}/{r['model']}  session {r['session']}" + (f"  reviews {r['reviews']}" if r.get("reviews") else ""))
        print(f"run_record: {len(records)} run(s)")
        return 0
    try:
        reg = va.load(root)
    except (OSError, ValueError):
        reg = None
    more, notes = check_records(records, reg)
    problems += more
    for p in problems:
        print("PROBLEM " + p)
    for n in notes:
        print("NOTE " + n)
    mode = "ENFORCED" if opts.enforce else "REPORT-ONLY"
    print(f"run_record: {mode}: {len(records)} run(s), {len(problems)} problem(s), {len(notes)} note(s)")
    return 1 if problems and opts.enforce else 0


if __name__ == "__main__":
    raise SystemExit(main())
