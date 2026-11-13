#!/usr/bin/env python3
"""Open, close and list claims in the coordination ledger (outside the repository).

It writes only inside ``<ledger>/claims`` and never touches the repository, a branch or a worktree. Claims are advisory,
not locks: ``open`` refuses only an exact file-name collision (created with exclusive mode) and LISTS overlapping
active claims on the same branch; it does not stop you.

Usage:
  python tools/claim.py open --domain D --task T --write-area PATH [--write-area PATH ...] --branch B
        [--worktree PATH] --session NAME --role lead --profile P --model M [--effort E] [--task-type TYPE]
        [--hours 24] [--base-commit REF] [--note TEXT] [--coord DIR] [--repo DIR]
  python tools/claim.py close NAME [--last-commit REF] [--note TEXT] [--status done|abandoned|parked]
  python tools/claim.py list [--all]

NAME is the claim file name without ``.json`` (``<domain>__<task>``). Source-class process tooling.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_claim_scope as ccs  # noqa: E402

NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")


def coord_dir(opt: Path | None) -> Path:
    path = opt or (Path(os.environ["KOHBOR_COORD_DIR"]) if os.environ.get("KOHBOR_COORD_DIR") else Path("F:/kohbor-coord"))
    if not (path / "claims").is_dir():
        raise SystemExit(f"claim: no claims folder in {path}; this tool does not create the ledger")
    return path


def short_head(repo: Path, ref: str = "HEAD") -> str | None:
    try:
        done = subprocess.run(["git", "--no-optional-locks", "rev-parse", "--short", ref], cwd=repo, capture_output=True, text=True, check=False)
    except OSError:
        return None
    return done.stdout.strip() if done.returncode == 0 and done.stdout.strip() else None


def static_prefix(pattern: str) -> str:
    cut = re.split(r"[*{]", pattern.replace("\\", "/"), maxsplit=1)[0]
    return cut.lstrip("./").rstrip("/")


def overlaps(a: list[str], a_whole: bool, b: list[str], b_whole: bool) -> list[str]:
    """Pattern pairs whose static prefixes contain one another (an advisory approximation)."""
    if a_whole or b_whole:
        return ["whole checkout"]
    hits = []
    for pa in a:
        for pb in b:
            sa, sb = static_prefix(pa), static_prefix(pb)
            if not sa or not sb or sa == sb or sa.startswith(sb + "/") or sb.startswith(sa + "/"):
                hits.append(f"{pa} ~ {pb}")
    return hits


def write_json_atomic(path: Path, data: dict, *, exclusive: bool) -> None:
    text = json.dumps(data, indent=1, ensure_ascii=False) + "\n"
    if exclusive:
        with open(path, "x", encoding="utf-8", newline="\n") as handle:  # fails if the file exists
            handle.write(text)
        return
    fd, tmp_name = tempfile.mkstemp(prefix="." + path.name + ".", suffix=".tmp", dir=str(path.parent))  # a unique file of our own
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(text)
        os.replace(tmp_name, str(path))
    except BaseException:
        try:
            os.unlink(tmp_name)
        except OSError:
            pass
        raise


def cmd_open(opts, coord: Path) -> int:
    for label, value in (("domain", opts.domain), ("task", opts.task)):
        if not NAME_RE.match(value):
            print(f"claim: {label} {value!r} must match {NAME_RE.pattern}", file=sys.stderr)
            return 2
    name = f"{opts.domain}__{opts.task}"
    path = coord / "claims" / f"{name}.json"
    now = dt.datetime.now().astimezone()
    repo = opts.repo.resolve()
    base = short_head(repo, opts.base_commit or "HEAD")
    claim = {
        "domain": opts.domain, "task": opts.task, "write_area": opts.write_area,
        "session": {"kind": opts.session_kind, "name": opts.session},
        "holder": {"role": opts.role, "profile": opts.profile, "model": opts.model, "effort": opts.effort, "session": opts.session,
                   **({"task_type": opts.task_type} if opts.task_type else {})},
        "workers": [],
        "model": f"{opts.model}" + (f" ({opts.effort})" if opts.effort else ""),
        "started": now.isoformat(timespec="seconds"), "expires": (now + dt.timedelta(hours=opts.hours)).isoformat(timespec="seconds"),
        "status": "active", "branch": opts.branch, "worktree": opts.worktree, "base_commit": base, "note": opts.note or "",
    }
    # overlapping active claims on the same branch are listed before the file is written
    roots = sorted({os.path.normcase(os.path.normpath(str(repo))), os.path.normcase(os.path.normpath(str(opts.worktree)))} if opts.worktree else {os.path.normcase(os.path.normpath(str(repo)))})
    mine, _, my_whole = ccs.patterns_from_claim(claim, roots)
    for other in sorted((coord / "claims").glob("*.json")):
        data = ccs.lenient_json(other)
        if isinstance(data, dict) and data.get("status") == "active" and data.get("branch") == opts.branch and other.name != path.name:
            theirs, _, their_whole = ccs.patterns_from_claim(data, roots)
            hits = overlaps(mine, my_whole, theirs, their_whole)
            if hits:
                print(f"OVERLAP with active claim {other.name} on branch {opts.branch}: {'; '.join(hits[:4])}")
    try:
        write_json_atomic(path, claim, exclusive=True)
    except FileExistsError:
        print(f"claim: {path.name} already exists; nothing written (close or rename the old claim, never overwrite it)", file=sys.stderr)
        return 1
    print(f"opened {path.name} (expires {claim['expires']}, base {base})")
    return 0


def cmd_close(opts, coord: Path) -> int:
    name = opts.name[:-5] if opts.name.endswith(".json") else opts.name
    path = coord / "claims" / f"{name}.json"
    if not NAME_RE.match(name) or not path.is_file():
        print(f"claim: no claim file {name}.json", file=sys.stderr)
        return 1
    data = ccs.lenient_json(path)
    if not isinstance(data, dict):
        print(f"claim: {path.name} cannot be read as JSON", file=sys.stderr)
        return 1
    data["status"] = opts.status
    data["finished"] = dt.date.today().isoformat()
    last = opts.last_commit or short_head(opts.repo.resolve())
    if last:
        data["last_commit"] = last
    if opts.note:
        data["note"] = (str(data.get("note", "")) + " " + opts.note).strip()
    write_json_atomic(path, data, exclusive=False)
    print(f"closed {path.name} as {opts.status}" + (f" at {last}" if last else ""))
    return 0


def cmd_list(opts, coord: Path) -> int:
    rows = 0
    for path in sorted((coord / "claims").glob("*.json")):
        data = ccs.lenient_json(path)
        if not isinstance(data, dict):
            print(f"{path.stem}  UNREADABLE")
            continue
        if data.get("status") != "active" and not opts.all:
            continue
        holder = data.get("holder") or {}
        who = f"{holder.get('role')}/{holder.get('model')}" if holder else f"unstructured: {data.get('model')}"
        print(f"{path.stem}  {data.get('status')}  {data.get('branch')}  {who}  expires {data.get('expires')}")
        rows += 1
    print(f"claim: {rows} claim(s) listed")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--coord", type=Path, default=None)
    ap.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    sub = ap.add_subparsers(dest="command", required=True)
    o = sub.add_parser("open")
    o.add_argument("--domain", required=True)
    o.add_argument("--task", required=True)
    o.add_argument("--write-area", action="append", required=True)
    o.add_argument("--branch", required=True)
    o.add_argument("--worktree", default=None)
    o.add_argument("--session", required=True)
    o.add_argument("--session-kind", default="claude-desktop-session")
    o.add_argument("--role", required=True)
    o.add_argument("--profile", required=True)
    o.add_argument("--model", required=True)
    o.add_argument("--effort", default=None)
    o.add_argument("--task-type", default=None)
    o.add_argument("--hours", type=float, default=24.0)
    o.add_argument("--base-commit", default=None)
    o.add_argument("--note", default="")
    c = sub.add_parser("close")
    c.add_argument("name")
    c.add_argument("--last-commit", default=None)
    c.add_argument("--note", default="")
    c.add_argument("--status", choices=["done", "abandoned", "parked"], default="done")
    ls = sub.add_parser("list")
    ls.add_argument("--all", action="store_true")
    opts = ap.parse_args(argv)
    coord = coord_dir(opts.coord)
    return {"open": cmd_open, "close": cmd_close, "list": cmd_list}[opts.command](opts, coord)


if __name__ == "__main__":
    raise SystemExit(main())
