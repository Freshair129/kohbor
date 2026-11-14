#!/usr/bin/env python3
"""Open, tag, renew, close and list claims in the coordination ledger (outside the repository).

It writes only inside ``<ledger>/claims`` and never touches the repository, a branch or a worktree. Claims are advisory,
not locks: ``open`` refuses only an exact file-name collision (created with exclusive mode) and LISTS overlapping
active claims on the same branch; it does not stop you.

Usage:
  python tools/claim.py open --domain D --task T --write-area PATH [--write-area PATH ...] --branch B
        [--worktree PATH] --session NAME --role lead --profile P --model M [--effort E] [--task-type TYPE]
        [--hours 24] [--base-commit REF] [--note TEXT] [--tag ID ...] [--coord DIR] [--repo DIR]
  python tools/claim.py tag NAME --add ID [--add ID ...]
  python tools/claim.py renew NAME --hours N
  python tools/claim.py close NAME [--last-commit REF] [--note TEXT] [--status done|abandoned|parked]
        [--done-tag ID=EVIDENCE_PATH ...] [--open-tag ID ...]
  python tools/claim.py list [--all]

Tags say what a claim serves: a requirement id, ``TASK:<sprint-table task>``, a ``DEC-`` id, or ``PROC:<slug>`` (process work, counted
separately from product progress). An unknown tag is refused (see tools/tagpool.py). ``close --done-tag`` stores the evidence file's
SHA-256 and the commit at close; it prints one ``Tags:`` line for the handoff. A tag not named done or open at close stays open.

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
import tagpool  # noqa: E402

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


def unknown_tags(repo: Path, tags: list[str]) -> list[str]:
    pool = tagpool.load_pool(repo)
    return [t for t in tags if not tagpool.is_valid(t, pool)]


def load_claim(coord: Path, name: str) -> tuple[Path | None, dict | None, str]:
    name = name[:-5] if name.endswith(".json") else name
    path = coord / "claims" / f"{name}.json"
    if not NAME_RE.match(name) or not path.is_file():
        return None, None, f"claim: no claim file {name}.json"
    data = ccs.lenient_json(path)
    if not isinstance(data, dict):
        return None, None, f"claim: {path.name} cannot be read as JSON"
    return path, data, ""


def cmd_tag(opts, coord: Path) -> int:
    path, data, err = load_claim(coord, opts.name)
    if data is None or path is None:
        print(err, file=sys.stderr)
        return 1
    if data.get("status") != "active":
        print(f"claim: {path.name} is not active ({data.get('status')}); tags are added to active claims only", file=sys.stderr)
        return 1
    bad = unknown_tags(opts.repo.resolve(), opts.add)
    if bad:
        print(f"claim: unknown tag(s) {', '.join(bad)}; {path.name} unchanged", file=sys.stderr)
        return 2
    tags = list(data.get("tags") or [])
    for tag in opts.add:
        if tag not in tags:
            tags.append(tag)
    data["tags"] = tags
    data.setdefault("tags_done", {})
    write_json_atomic(path, data, exclusive=False)
    print(f"tagged {path.name}: {', '.join(tags)}")
    return 0


def cmd_renew(opts, coord: Path) -> int:
    if not opts.hours > 0:
        print("claim: --hours must be greater than zero", file=sys.stderr)
        return 2
    path, data, err = load_claim(coord, opts.name)
    if data is None or path is None:
        print(err, file=sys.stderr)
        return 1
    if data.get("status") != "active":
        print(f"claim: {path.name} is not active ({data.get('status')}); only an active claim can be renewed", file=sys.stderr)
        return 1
    now = dt.datetime.now().astimezone()
    data["expires"] = (now + dt.timedelta(hours=opts.hours)).isoformat(timespec="seconds")
    data.setdefault("renewed", []).append({"on": now.isoformat(timespec="seconds"), "hours": float(opts.hours)})
    write_json_atomic(path, data, exclusive=False)
    print(f"renewed {path.name} until {data['expires']}")
    return 0


def cmd_open(opts, coord: Path) -> int:
    for label, value in (("domain", opts.domain), ("task", opts.task)):
        if not NAME_RE.match(value):
            print(f"claim: {label} {value!r} must match {NAME_RE.pattern}", file=sys.stderr)
            return 2
    name = f"{opts.domain}__{opts.task}"
    path = coord / "claims" / f"{name}.json"
    now = dt.datetime.now().astimezone()
    repo = opts.repo.resolve()
    tags: list[str] = []
    for tag in opts.tag or []:
        if tag not in tags:
            tags.append(tag)
    bad = unknown_tags(repo, tags)
    if bad:
        print(f"claim: unknown tag(s) {', '.join(bad)}; nothing written (pool: requirement ids, TASK:<sprint task>, DEC- ids, PROC:<slug>)", file=sys.stderr)
        return 2
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
        "tags": tags, "tags_done": {},
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
    repo = opts.repo.resolve()
    claim_tags = list(data.get("tags") or [])
    done: dict[str, dict[str, str]] = dict(data.get("tags_done") or {})
    last = opts.last_commit or short_head(repo)
    for spec in opts.done_tag or []:
        tag, sep, evidence = spec.partition("=")
        if not sep or not tag or not evidence:
            print(f"claim: --done-tag {spec!r} must be ID=EVIDENCE_PATH", file=sys.stderr)
            return 2
        if tag not in claim_tags:
            print(f"claim: {tag} is not a tag of this claim ({', '.join(claim_tags) or 'no tags'}); nothing closed", file=sys.stderr)
            return 2
        target = tagpool.resolve_evidence(repo, evidence)
        if not target.is_file():
            print(f"claim: evidence file for {tag} not found: {evidence}; nothing closed", file=sys.stderr)
            return 2
        try:
            shown = target.resolve().relative_to(repo).as_posix()
        except ValueError:
            shown = str(target.resolve())
        done[tag] = {"path": shown, "sha256": tagpool.sha256_file(target), "commit": short_head(repo) or ""}
    for tag in opts.open_tag or []:
        if tag not in claim_tags:
            print(f"claim: --open-tag {tag} is not a tag of this claim; nothing closed", file=sys.stderr)
            return 2
    data["status"] = opts.status
    data["finished"] = dt.date.today().isoformat()
    if last:
        data["last_commit"] = last
    if opts.note:
        data["note"] = (str(data.get("note", "")) + " " + opts.note).strip()
    open_tags = [t for t in claim_tags if t not in done]
    data["tags_done"] = done
    data["tags_open"] = open_tags
    write_json_atomic(path, data, exclusive=False)
    print(f"closed {path.name} as {opts.status}" + (f" at {last}" if last else ""))
    print(tags_line(done, open_tags))
    return 0


def tags_line(done: dict[str, dict[str, str]], open_tags: list[str]) -> str:
    """One line for the handoff template: ``Tags: done ID=path@commit#sha256:12hex; open ID, ID`` (or ``Tags: none``)."""
    parts = []
    if done:
        parts.append("done " + ", ".join(f"{tag}={ev['path']}@{ev.get('commit', '')}#sha256:{str(ev.get('sha256', ''))[:12]}" for tag, ev in sorted(done.items())))
    if open_tags:
        parts.append("open " + ", ".join(open_tags))
    return "Tags: " + ("; ".join(parts) if parts else "none")


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
        tags = data.get("tags") or []
        print(f"{path.stem}  {data.get('status')}  {data.get('branch')}  {who}  expires {data.get('expires')}  tags {', '.join(tags) if tags else '-'}")
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
    o.add_argument("--tag", action="append", default=None, help="what the claim serves (repeatable); see tools/tagpool.py")
    tg = sub.add_parser("tag")
    tg.add_argument("name")
    tg.add_argument("--add", action="append", required=True)
    rn = sub.add_parser("renew")
    rn.add_argument("name")
    rn.add_argument("--hours", type=float, required=True)
    c = sub.add_parser("close")
    c.add_argument("name")
    c.add_argument("--last-commit", default=None)
    c.add_argument("--note", default="")
    c.add_argument("--status", choices=["done", "abandoned", "parked"], default="done")
    c.add_argument("--done-tag", action="append", default=None, metavar="ID=PATH", help="a tag finished, with its evidence file (sha256 and commit are recorded)")
    c.add_argument("--open-tag", action="append", default=None, metavar="ID", help="a tag left open (any tag not named done is open)")
    ls = sub.add_parser("list")
    ls.add_argument("--all", action="store_true")
    opts = ap.parse_args(argv)
    coord = coord_dir(opts.coord)
    return {"open": cmd_open, "tag": cmd_tag, "renew": cmd_renew, "close": cmd_close, "list": cmd_list}[opts.command](opts, coord)


if __name__ == "__main__":
    raise SystemExit(main())
