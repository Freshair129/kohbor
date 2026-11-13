#!/usr/bin/env python3
"""Report-only janitor: lists branches, worktrees, stashes and private copies and proposes what the owner may remove.

It NEVER deletes, prunes, resets, pushes or writes inside a repository or a private copy. No standing deletion
authority exists: every item it proposes needs the owner's written decision for that item. The only write it can
make is with ``--register-missing``: it appends records for unregistered local branches to the coordination
registry (``registry/branches.json``), never changing or removing an existing record.

Usage: ``python tools/janitor.py [--repo DIR] [--coord DIR] [--main BRANCH] [--min-age-days N] [--json]
[--out FILE] [--register-missing]``

Classes: PROPOSE-TO-OWNER = merged into main and into origin/main, not checked out anywhere, no active claim, no
unpushed commit, not main. ``auto_eligible`` additionally requires the minimum age: only that subset could ever be
removed automatically, and only under a written standing authorization that does not exist today. Everything else is
REPORT with its reasons.
Red flags: any unregistered item, more than 3 worktrees, more than 15 local branches.
"""

from __future__ import annotations

import argparse
import datetime as dt
import fnmatch
import json
import os
import subprocess
import sys
from pathlib import Path

MAX_WORKTREES = 3
MAX_BRANCHES = 15
BS = chr(92)


def git(repo: Path, *args: str) -> str | None:
    try:
        done = subprocess.run(["git", "--no-optional-locks", *args], cwd=repo, capture_output=True, text=True,
                              encoding="utf-8", errors="replace", check=False)
    except OSError:
        return None
    return done.stdout if done.returncode == 0 else None


def lenient_json(path: Path):
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return None
    for candidate in (text, text.replace(BS, BS + BS)):
        try:
            return json.loads(candidate)
        except ValueError:
            continue
    return None


def norm(path: str) -> str:
    return os.path.normcase(os.path.normpath(path.replace("/", os.sep)))


def load_claims(coord: Path | None) -> list[dict]:
    claims: list[dict] = []
    if coord and (coord / "claims").is_dir():
        for f in sorted((coord / "claims").glob("*.json")):
            data = lenient_json(f)
            if isinstance(data, dict):
                data["_file"] = f.name
                claims.append(data)
    return claims


def collect(repo: Path, coord: Path | None, main: str, min_age_days: int, now: dt.datetime) -> dict:
    registry = lenient_json(coord / "registry" / "branches.json") if coord and (coord / "registry").is_dir() else None
    registry = registry if isinstance(registry, dict) else {}
    claims = load_claims(coord)
    active_claims = [c for c in claims if c.get("status") == "active"]
    claim_branches = {str(c.get("branch")): c["_file"] for c in active_claims if c.get("branch")}
    claim_paths = {norm(str(c.get("worktree"))): c["_file"] for c in active_claims if c.get("worktree")}

    out: dict = {"generated": now.isoformat(timespec="seconds"), "repo": str(repo), "main": main, "branches": [],
                 "worktrees": [], "remote_merged": [], "stashes": 0, "private_copies": [], "red_flags": [],
                 "notes": [], "no_deletion_performed": True}
    has_origin_main = git(repo, "rev-parse", "--verify", "--quiet", f"origin/{main}") is not None
    base_remote = f"origin/{main}" if has_origin_main else None
    merged = set((git(repo, "branch", "--merged", main, "--format=%(refname:short)") or "").split())

    reg_branches = {b.get("name") for b in registry.get("local_branches", []) if isinstance(b, dict)}
    reg_worktree_branches = {w.get("branch") for w in registry.get("repo_F_kohbor", {}).get("worktrees", []) if isinstance(w, dict)}
    reg_worktree_paths = {norm(str(w.get("path"))) for w in registry.get("repo_F_kohbor", {}).get("worktrees", []) if isinstance(w, dict)}

    wt_raw = git(repo, "worktree", "list", "--porcelain") or ""
    worktrees: list[dict] = []
    cur: dict = {}
    for line in wt_raw.splitlines() + [""]:
        if not line.strip():
            if cur:
                worktrees.append(cur)
            cur = {}
            continue
        key, _, value = line.partition(" ")
        cur[key] = value
    checked_out: dict[str, str] = {}
    for w in worktrees:
        branch = w.get("branch", "").replace("refs/heads/", "") or None
        w["branch_short"] = branch
        if branch:
            checked_out[branch] = w.get("worktree", "")

    fmt = "%(refname:short)|%(objectname:short)|%(committerdate:iso-strict)|%(upstream:short)|%(upstream:track)"
    for line in (git(repo, "for-each-ref", "refs/heads", f"--format={fmt}") or "").splitlines():
        name, sha, date, upstream, track = (line.split("|") + ["", "", "", "", ""])[:5]
        try:
            age = (now - dt.datetime.fromisoformat(date.replace("Z", "+00:00"))).days
        except ValueError:
            age = -1
        item = {"name": name, "last_commit": sha, "last_commit_date": date[:10], "age_days": age, "upstream": upstream or None,
                "merged_into_main": name in merged, "reasons": [], "class": "REPORT", "registered": name in reg_branches or name in reg_worktree_branches}
        if name == main:
            item["class"] = "BASE"
            out["branches"].append(item)
            continue
        if name in checked_out:
            item["reasons"].append(f"checked out in a worktree ({Path(checked_out[name]).name})")
        if name not in merged:
            item["reasons"].append("not merged into " + main)
        if name in claim_branches:
            item["reasons"].append(f"active claim {claim_branches[name]}")
        if "ahead" in track:
            item["reasons"].append(f"unpushed commits ({track.strip('[]')})")
        if name in merged and not base_remote:
            item["reasons"].append(f"origin/{main} is unavailable: cannot show that the work is pushed")
        elif name in merged and git(repo, "merge-base", "--is-ancestor", name, base_remote) is None:
            item["reasons"].append(f"merged locally but not in {base_remote}")
        recent = not (age >= min_age_days)  # an unknown age (-1) counts as recent: never auto-eligible
        if not item["registered"]:
            item["reasons"].append("unregistered (no record in the coordination registry)")
        # an unregistered record alone does not block a proposal, it is reported separately
        blocking = [r for r in item["reasons"] if not r.startswith("unregistered")]
        item["class"] = "PROPOSE-TO-OWNER" if not blocking else "REPORT"
        item["auto_eligible"] = item["class"] == "PROPOSE-TO-OWNER" and not recent
        out["branches"].append(item)

    for w in worktrees:
        path = w.get("worktree", "")
        is_main_wt = worktrees and w is worktrees[0]
        status = git(Path(path), "status", "--porcelain") if Path(path).is_dir() else None
        entry = {"path": path, "branch": w.get("branch_short"), "head": (w.get("HEAD") or "")[:7], "main_worktree": bool(is_main_wt),
                 "dirty": bool(status and status.strip()) if status is not None else None,
                 "registered": norm(path) in reg_worktree_paths or norm(path) in claim_paths, "reasons": []}
        if not entry["registered"]:
            entry["reasons"].append("unregistered worktree")
        if entry["dirty"]:
            entry["reasons"].append("uncommitted changes")
        if norm(path) in claim_paths:
            entry["reasons"].append(f"active claim {claim_paths[norm(path)]}")
        if w.get("branch_short") and w["branch_short"] not in merged and not is_main_wt:
            entry["reasons"].append("branch not merged into " + main)
        out["worktrees"].append(entry)

    if base_remote:
        rm = (git(repo, "branch", "-r", "--merged", base_remote, "--format=%(refname:short)") or "").split()
        out["remote_merged"] = [r for r in rm if r not in (base_remote, "origin", "origin/HEAD")]
    out["stashes"] = len((git(repo, "stash", "list") or "").splitlines())
    if out["stashes"]:
        out["red_flags"].append(f"{out['stashes']} stash entries (the stack is shared between worktrees)")

    roots = registry.get("logical_roots", {}) if isinstance(registry.get("logical_roots"), dict) else {}
    globs = registry.get("private_copy_globs", {}) if isinstance(registry.get("private_copy_globs"), dict) else {}
    reg_copies = {c.get("path"): c for c in registry.get("private_copies_F_kohbor_banner_lab", []) if isinstance(c, dict)}
    for root_name, root_path in roots.items():
        p = Path(root_path)
        if not p.is_dir():
            out["notes"].append(f"logical root {root_name}: not present on this machine, not checked here")
            continue
        for child in sorted(x for x in p.iterdir() if x.is_dir()):
            if any(fnmatch.fnmatch(child.name, g) for g in globs.get(root_name, [])):
                rec = reg_copies.get(child.name)
                entry = {"root": root_name, "path": child.name, "registered": rec is not None,
                         "state": (rec or {}).get("state"), "dispose_when": (rec or {}).get("dispose_when"), "reasons": []}
                if rec is None:
                    entry["reasons"].append("unregistered private copy")
                out["private_copies"].append(entry)
        for name in reg_copies:
            if not (p / name).exists():
                out["notes"].append(f"registered private copy {root_name}:{name} no longer exists on disk (update the record, do not assume it was approved for removal)")

    unregistered = [b["name"] for b in out["branches"] if not b["registered"] and b["class"] != "BASE"]
    unregistered += [w["path"] for w in out["worktrees"] if not w["registered"]]
    unregistered += [c["path"] for c in out["private_copies"] if not c["registered"]]
    if unregistered:
        out["red_flags"].append(f"{len(unregistered)} unregistered item(s): " + ", ".join(Path(u).name if os.sep in u or "/" in u else u for u in unregistered))
    if len(out["worktrees"]) > MAX_WORKTREES:
        out["red_flags"].append(f"{len(out['worktrees'])} worktrees (limit {MAX_WORKTREES})")
    if len(out["branches"]) > MAX_BRANCHES:
        out["red_flags"].append(f"{len(out['branches'])} local branches (limit {MAX_BRANCHES})")
    out["claims_active"] = [c["_file"] for c in active_claims]
    out["claims_without_registry_note"] = [c["_file"] for c in claims if c.get("status") == "active" and c.get("expires") and _expired(c["expires"], now)]
    return out


def _expired(value: str, now: dt.datetime) -> bool:
    try:
        when = dt.datetime.fromisoformat(str(value))
    except ValueError:
        return False
    if when.tzinfo is None:
        when = when.replace(tzinfo=dt.timezone.utc)
    return when < now


def render(rep: dict) -> str:
    safe = [b for b in rep["branches"] if b["class"] == "PROPOSE-TO-OWNER"]
    rest = [b for b in rep["branches"] if b["class"] == "REPORT"]
    lines = [f"# Janitor report {rep['generated'][:10]} (report only: nothing was deleted)", "",
             f"Repository: {Path(rep['repo']).name}; base branch: {rep['main']}; local branches: {len(rep['branches'])}; "
             f"worktrees: {len(rep['worktrees'])}; stashes: {rep['stashes']}; active claims: {len(rep['claims_active'])}.", ""]
    lines.append("## Red flags")
    lines += [f"- {f}" for f in rep["red_flags"]] or ["- none"]
    lines += ["", f"## Branches proposed for the owner's decision ({len(safe)}): merged into {rep['main']} and origin, not checked out, no claim, nothing unpushed",
              "Removal needs the owner's written decision for each item (no standing authorization exists). The command is shown, never run.",
              f"`auto` marks the items old enough for a future standing authorization; none is applied today.", ""]
    for b in safe:
        lines.append(f"- `{b['name']}` last commit {b['last_commit']} on {b['last_commit_date']} ({b['age_days']} day(s) old){' auto' if b['auto_eligible'] else ''}: `git branch -d {b['name']}`")
    if not safe:
        lines.append("- none")
    lines += ["", f"## Branches kept in the report ({len(rest)})"]
    for b in rest:
        lines.append(f"- `{b['name']}` ({b['last_commit_date']}): " + "; ".join(b["reasons"]))
    if not rest:
        lines.append("- none")
    lines += ["", "## Worktrees"]
    for w in rep["worktrees"]:
        flag = "; ".join(w["reasons"]) or "ok"
        lines.append(f"- `{Path(w['path']).name}` on `{w['branch']}`{' (main checkout)' if w['main_worktree'] else ''}: {flag}")
    lines += ["", "## Private copies outside the repository"]
    for c in rep["private_copies"]:
        lines.append(f"- {c['root']}:{c['path']} state={c['state']} dispose_when={c['dispose_when']}" + (" ; " + "; ".join(c["reasons"]) if c["reasons"] else ""))
    if not rep["private_copies"]:
        lines.append("- none found (or the root is not present here)")
    if rep["remote_merged"]:
        lines += ["", f"## Remote branches already merged into origin/{rep['main']} ({len(rep['remote_merged'])}; informational, never deleted by this tool)"]
        lines += [f"- `{r}`" for r in rep["remote_merged"]]
    if rep["claims_without_registry_note"]:
        lines += ["", "## Claims past their expiry (an expired claim does not free its area)"]
        lines += [f"- {c}" for c in rep["claims_without_registry_note"]]
    if rep["notes"]:
        lines += ["", "## Notes"] + [f"- {n}" for n in rep["notes"]]
    return "\n".join(lines) + "\n"


def register_missing(coord: Path, rep: dict) -> int:
    path = coord / "registry" / "branches.json"
    data = lenient_json(path)
    if not isinstance(data, dict):
        print("cannot read the registry; nothing written", file=sys.stderr)
        return 1
    existing = {b.get("name") for b in data.get("local_branches", []) if isinstance(b, dict)}
    wt_branches = {w.get("branch") for w in data.get("repo_F_kohbor", {}).get("worktrees", []) if isinstance(w, dict)}
    added = []
    for b in rep["branches"]:
        if b["class"] == "BASE" or b["name"] in existing or b["name"] in wt_branches:
            continue
        added.append({"name": b["name"], "state": "merged" if b["merged_into_main"] else "active", "last_commit": b["last_commit"],
                      "registered_by": "janitor --register-missing", "registered_on": rep["generated"][:10],
                      "dispose_when": "owner's written decision; the janitor only proposes"})
    if not added:
        print("registry already lists every local branch")
        return 0
    lock = path.with_name(path.name + ".lock")
    try:
        fd = os.open(str(lock), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except FileExistsError:
        print(f"{lock.name} exists: another session is writing the registry; nothing written (remove the lock only if you are sure it is stale)", file=sys.stderr)
        return 1
    try:
        os.close(fd)
        fresh = lenient_json(path)  # re-read under the lock so a concurrent append is not lost
        if not isinstance(fresh, dict):
            print("cannot re-read the registry; nothing written", file=sys.stderr)
            return 1
        have = {b.get("name") for b in fresh.get("local_branches", []) if isinstance(b, dict)}
        to_add = [a for a in added if a["name"] not in have]
        if not to_add:
            print("registry already lists every local branch")
            return 0
        fresh.setdefault("local_branches", []).extend(to_add)
        backup = path.with_name(path.name + ".bak")
        backup.write_bytes(path.read_bytes())
        tmp = path.with_name(path.name + ".tmp")
        tmp.write_text(json.dumps(fresh, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
        os.replace(str(tmp), str(path))
        print(f"registered {len(to_add)} branch record(s); previous file kept as {backup.name}; none was deleted")
        return 0
    finally:
        try:
            lock.unlink()
        except OSError:
            pass


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    ap.add_argument("--coord", type=Path, default=None)
    ap.add_argument("--main", default="main")
    ap.add_argument("--min-age-days", type=int, default=7)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--out", type=Path, default=None, help="also write the report to this file (outside the repository)")
    ap.add_argument("--register-missing", action="store_true")
    opts = ap.parse_args(argv)
    coord = opts.coord or (Path(os.environ["KOHBOR_COORD_DIR"]) if os.environ.get("KOHBOR_COORD_DIR") else Path("F:/kohbor-coord"))
    coord = coord if coord.is_dir() else None
    rep = collect(opts.repo.resolve(), coord, opts.main, opts.min_age_days, dt.datetime.now(dt.timezone.utc))
    text = json.dumps(rep, indent=1, ensure_ascii=False) if opts.json else render(rep)
    print(text)
    if opts.out:
        try:
            opts.out.resolve().relative_to(opts.repo.resolve())
            print("--out must be outside the repository; report not written", file=sys.stderr)
            return 1
        except ValueError:
            pass
        opts.out.parent.mkdir(parents=True, exist_ok=True)
        opts.out.write_text(text, encoding="utf-8")
    if opts.register_missing:
        if coord is None:
            print("no coordination ledger found; nothing written", file=sys.stderr)
            return 1
        return register_missing(coord, rep)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
