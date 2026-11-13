#!/usr/bin/env python3
"""Report whether the changed paths of a branch stay inside the write areas of its claim(s).

REPORT-ONLY by default (exit 0): making this a gate failure is reserved for the owner. ``--enforce`` exits 1 when a
changed path is outside every claimed write area, or when the branch has no active claim. Reads only: it runs
``git diff --name-only`` and ``git status --porcelain`` with ``--no-optional-locks`` and reads claim files.

Usage: ``python tools/check_claim_scope.py [--repo DIR] [--coord DIR] [--base REF] [--claim FILE ...] [--include-done] [--enforce] [--json]``
       ``python tools/check_claim_scope.py --brief FILE``   (a worker's uncommitted changes against the SCOPE of its brief; exit 1 on a path outside)

- Changed paths = commits on HEAD that are not on the base, plus uncommitted and untracked changes (read with ``-z``). The
  base is ``--base``, else ``KOHBOR_SCOPE_BASE``, else the ``base_commit`` of the earliest claim that has one, else ``main``
  (or ``origin/main``). A closed (``done``) claim counts only with ``--include-done`` and only while the tree is clean and HEAD is its ``last_commit``;
  otherwise it is reported as WORK AFTER CLOSE and covers nothing. A claim that covers the whole checkout makes the check vacuous and says so.
- Claims = ``--claim`` files, or every claim file in the coordination ledger whose ``status`` is ``active`` (or ``done``
  when ``--include-done``) and whose ``branch`` is the current branch.
- A ``write_area`` entry counts as a pattern when it looks like a repository path or glob (``**``, ``*``, ``{a,b}``);
  the text after `` (`` is a comment. An entry that names the whole checkout (the repository root) matches everything.
  Other entries are listed as "not a path pattern" and ignored.
- Shared integration files are always allowed (docs/delivery/AGENT-WORKFLOW.md section 9.1 item 6).

Source-class evidence about process only.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

SHARED = [
    "package-manifest.json",
    "docs/requirements/RTM.csv",
    "docs/requirements/SRS.md",
    "acceptance/spec-validation.json",
    "acceptance/scenarios.feature",
    "docs/INDEX.md",
    "docs/governance/CHANGELOG.md",
]
BS = chr(92)


def git(repo: Path, *args: str) -> str | None:
    try:
        done = subprocess.run(["git", "--no-optional-locks", *args], cwd=repo, capture_output=True, text=True, encoding="utf-8", errors="replace", check=False)
    except OSError:
        return None
    return done.stdout if done.returncode == 0 else None


def lenient_json(path: Path):
    try:
        text = path.read_text(encoding="utf-8-sig")
    except OSError:
        return None
    for candidate in (text, text.replace(BS, BS + BS)):
        try:
            return json.loads(candidate)
        except ValueError:
            continue
    return None


def expand_braces(pattern: str) -> list[str]:
    m = re.search(r"\{([^{}]*)\}", pattern)
    if not m:
        return [pattern]
    out: list[str] = []
    for part in m.group(1).split(","):
        out += expand_braces(pattern[:m.start()] + part.strip() + pattern[m.end():])
    return out


def glob_to_regex(pattern: str) -> re.Pattern:
    pattern = pattern.strip().replace(BS, "/")
    while pattern.startswith("./"):
        pattern = pattern[2:]
    pattern = pattern.rstrip("/")
    i, out = 0, ""
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out += "(?:.*/)?"
            i += 3
        elif pattern.startswith("**", i):
            out += ".*"
            i += 2
        elif pattern[i] == "*":
            out += "[^/]*"
            i += 1
        else:
            out += re.escape(pattern[i])
            i += 1
    # a pattern without a glob also covers everything below it when it names a folder
    return re.compile("^" + out + "(?:/.*)?$" if "*" not in pattern else "^" + out + "$", re.I if os.name == "nt" else 0)


PROSE_WORDS = re.compile(r"\b(the|for|and|except|of|with|to|a|an|or|in|on|above|below|tests?)\b", re.I)
MATCH_ALL = {"**", "**/*", "*", "**/**", "./**", "."}


def looks_like_path(entry: str) -> bool:
    """A repository path or glob (any Unicode, spaces and apostrophes allowed), as opposed to a prose entry."""
    if not entry or entry != entry.strip() or re.search(r'[()<>|"?;:]', entry) or entry.lower().startswith("git "):
        return False
    if " " in entry and PROSE_WORDS.search(entry) and not re.search(r"[/\\*]", entry):
        return False
    if " " in entry and PROSE_WORDS.search(entry):
        return False
    if re.search(r"[\\/*]", entry):
        return True
    return bool(re.search(r"\.[^\s.]{1,8}$", entry))


def patterns_from_claim(claim: dict, repo_roots: list[str]) -> tuple[list[str], list[str], bool]:
    """Return (path patterns, ignored entries, whole_checkout)."""
    patterns: list[str] = []
    ignored: list[str] = []
    whole = False
    area = claim.get("write_area", [])
    for raw in area if isinstance(area, list) else [area]:
        entry = str(raw).split(" (", 1)[0].strip().strip("`")
        norm = os.path.normcase(os.path.normpath(entry.replace("/", os.sep))) if entry else ""
        if norm and norm in repo_roots:
            whole = True
            continue
        absolute = re.match(r"^[A-Za-z]:[\\/]", entry) or entry.startswith("/")
        if absolute:
            inside = next((root for root in repo_roots if norm.startswith(root + os.sep)), None)
            if inside:
                patterns.append(norm[len(inside) + 1:].replace(os.sep, "/"))
            else:
                ignored.append(str(raw))
            continue
        if entry.replace(BS, "/").lstrip("./") in MATCH_ALL or entry in MATCH_ALL:
            whole = True  # a match-all glob covers everything: the check is vacuous
            continue
        if looks_like_path(entry):
            patterns += expand_braces(entry)
        else:
            ignored.append(str(raw))
    return patterns, ignored, whole


def nul_list(text: str | None) -> list[str]:
    return [p for p in (text or "").split("\0") if p]


def status_paths(repo: Path) -> tuple[list[str], bool]:
    """Uncommitted and untracked paths, read with -z so spaces, quotes and Thai names survive. Returns (paths, clean)."""
    out = git(repo, "status", "--porcelain=v1", "-z", "--untracked-files=all")
    if out is None:
        raise RuntimeError("git status failed: the working tree cannot be read, so the scope cannot be checked")
    entries = nul_list(out)
    paths: list[str] = []
    i = 0
    while i < len(entries):
        entry = entries[i]
        xy, path = entry[:2], entry[3:]
        paths.append(path)
        if "R" in xy or "C" in xy:  # a rename or copy record is followed by the original path
            i += 1
            if "R" in xy and i < len(entries):  # a rename removes the original: both ends changed; a copy leaves it alone
                paths.append(entries[i])
        i += 1
    return paths, not entries


def changed_paths(repo: Path, bases: "str | list[str] | None") -> tuple[list[str], str, bool]:
    paths: set[str] = set()
    used = "working tree only"
    base_list = [bases] if isinstance(bases, str) else list(bases or [])
    for base in base_list:  # the union of every base's diff: an override can add coverage, never remove it
        out = git(repo, "diff", "--name-only", "--no-renames", "-z", f"{base}...HEAD")  # --no-renames: both ends of a rename are listed
        if out is None:
            raise RuntimeError(f"git diff against base {base!r} failed: the committed changes cannot be read, so the scope cannot be checked")
        paths.update(nul_list(out))
    if base_list:
        used = f"{' + '.join(base_list)}...HEAD plus the working tree"
    status, clean = status_paths(repo)
    paths.update(status)
    return sorted(paths), used, clean


def parse_brief_lines(text: str) -> dict[str, str]:
    out: dict[str, str] = {}
    for line in text.splitlines():
        m = re.match(r"^([A-Z][A-Z_]+):[ \t]*(.*)$", line)
        if m and m.group(1) not in out:
            out[m.group(1)] = m.group(2).strip()
    return out


def split_outside_braces(text: str) -> list[str]:
    parts, depth, cur = [], 0, ""
    for ch in text:
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth = max(0, depth - 1)
        if ch == "," and depth == 0:
            parts.append(cur.strip())
            cur = ""
        else:
            cur += ch
    parts.append(cur.strip())
    return [p for p in parts if p]


def run_brief(opts) -> dict:
    """Check the working tree of a worker against the SCOPE of its brief. Uncommitted work only: workers do not commit."""
    repo = opts.repo.resolve()
    brief = parse_brief_lines(opts.brief.read_text(encoding="utf-8-sig"))
    raw_scope = brief.get("SCOPE", "").strip()
    explicit_none = bool(re.match(r"(?i)^(none|n/a|-)\b", raw_scope))
    scope = [] if explicit_none else split_outside_braces(raw_scope)
    patterns, ignored, whole = patterns_from_claim({"write_area": scope}, [])
    regexes = [glob_to_regex(p) for p in patterns]
    paths, clean = status_paths(repo)
    inside, outside = [], []
    for path in sorted(set(paths)):
        (inside if whole or any(r.match(path) for r in regexes) else outside).append(path)
    return {"mode_brief": True, "task": brief.get("TASK_ID", "?"), "scope_patterns": patterns, "ignored_entries": ignored, "changed": len(inside) + len(outside),
            "inside": inside, "outside": outside, "explicit_no_write_scope": explicit_none, "no_scope": not patterns and not whole and not explicit_none}


def run(opts) -> dict:
    repo = opts.repo.resolve()
    top = (git(repo, "rev-parse", "--show-toplevel") or str(repo)).strip()
    roots = {os.path.normcase(os.path.normpath(top))}
    for line in (git(repo, "worktree", "list", "--porcelain") or "").splitlines():
        if line.startswith("worktree "):
            roots.add(os.path.normcase(os.path.normpath(line[9:].strip())))
    branch = (git(repo, "rev-parse", "--abbrev-ref", "HEAD") or "").strip()
    head = (git(repo, "rev-parse", "--short", "HEAD") or "").strip()
    _, clean = status_paths(repo)
    claims: list[tuple[str, dict]] = []
    work_after_close: list[str] = []
    branch_mismatch: list[str] = []
    candidates: list[tuple[str, dict, bool]] = []  # (file name, claim, explicitly given)
    if opts.claim:
        for f in opts.claim:
            data = lenient_json(f)
            if isinstance(data, dict):
                candidates.append((f.name, data, True))
    elif opts.coord and (opts.coord / "claims").is_dir():
        for f in sorted((opts.coord / "claims").glob("*.json")):
            data = lenient_json(f)
            if isinstance(data, dict):
                candidates.append((f.name, data, False))
    wanted = {"active", "done"} if opts.include_done else {"active"}
    head_full = (git(repo, "rev-parse", "HEAD") or "").strip()
    for name, data, explicit in candidates:
        if data.get("status") not in wanted:
            continue
        if data.get("branch") != branch:
            if explicit:
                branch_mismatch.append(name)  # an explicit claim of another branch covers nothing here
            continue
        if data.get("status") == "done":
            last = str(data.get("last_commit") or "")
            last_full = (git(repo, "rev-parse", "--verify", "--quiet", f"{last}^{{commit}}") or "").strip() if last else ""
            if not (clean and last_full and head_full and last_full == head_full):  # last_commit must resolve to HEAD itself
                work_after_close.append(name)  # a closed claim only covers the tree it was closed at
                continue
        claims.append((name, data))
    override, override_source = opts.base, "--base"
    if override is None and os.environ.get("KOHBOR_SCOPE_BASE"):
        override, override_source = os.environ["KOHBOR_SCOPE_BASE"], "KOHBOR_SCOPE_BASE"
    base, base_source, override_ignored = None, "", None
    bases: list[str] = []
    dated = sorted((c for _, c in claims if c.get("base_commit")), key=lambda c: str(c.get("started", "")))
    for c in dated:
        if git(repo, "rev-parse", "--verify", "--quiet", f"{c['base_commit']}^{{commit}}") is not None:
            base, base_source = str(c["base_commit"]), "claim base_commit"
            break
    if dated and base is None and not override:
        raise RuntimeError(f"the claim's base_commit {dated[0]['base_commit']!r} does not resolve in this repository: the committed changes cannot be checked")
    if base is not None:
        bases = [base]
        if override:
            if git(repo, "merge-base", "--is-ancestor", override, base) is not None:  # an ancestor: its diff is added to the claim base's diff
                bases.append(override)
                base_source = f"claim base_commit + {override_source}"
            else:
                override_ignored = f"{override_source} {override!r} is not an ancestor of the claim's base_commit {base} or does not resolve; ignored"
    elif override:
        if git(repo, "rev-parse", "--verify", "--quiet", f"{override}^{{commit}}") is None:
            raise RuntimeError(f"{override_source} {override!r} does not resolve in this repository")
        base, base_source, bases = override, override_source, [override]
    if base is None:
        base = next((b for b in ("main", "origin/main") if git(repo, "rev-parse", "--verify", "--quiet", b) is not None), None)
        base_source = "default branch"
        if base is None:
            raise RuntimeError("no base to diff against: there is no claim base_commit, no --base and no main or origin/main")
        bases = [base]
    patterns: list[str] = []
    ignored: list[str] = []
    whole = False
    for _, claim in claims:
        p, ig, w = patterns_from_claim(claim, sorted(roots))
        patterns += p
        ignored += ig
        whole = whole or w
    regexes = [glob_to_regex(p) for p in patterns]
    shared = [glob_to_regex(p) for p in SHARED]
    paths, used, _ = changed_paths(repo, bases)
    inside, outside, shared_hits = [], [], []
    for path in paths:
        if any(r.match(path) for r in shared):
            shared_hits.append(path)
        elif whole or any(r.match(path) for r in regexes):
            inside.append(path)
        else:
            outside.append(path)
    return {"branch": branch, "diff": used, "base": base, "base_source": base_source, "claims": [name for name, _ in claims], "patterns": patterns,
            "ignored_entries": ignored, "whole_checkout_claim": whole, "vacuous": whole, "changed": len(paths), "inside": inside,
            "shared_integration_files": shared_hits, "outside": outside, "no_claim": not claims, "work_after_close": work_after_close,
            "claim_branch_mismatch": branch_mismatch, "base_override_ignored": override_ignored}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    ap.add_argument("--coord", type=Path, default=None)
    ap.add_argument("--base", default=None)
    ap.add_argument("--claim", type=Path, action="append")
    ap.add_argument("--brief", type=Path, help="check the working tree against the SCOPE of a handoff brief (lead-side lint: exit 1 on a path outside)")
    ap.add_argument("--include-done", action="store_true", help="also use claims closed as done, but only while the tree is clean and HEAD is the claim's last_commit")
    ap.add_argument("--enforce", action="store_true", help="exit 1 on a path outside the claim or on a missing claim (owner decision)")
    ap.add_argument("--json", action="store_true")
    opts = ap.parse_args(argv)
    if opts.brief:
        try:
            rep = run_brief(opts)
        except RuntimeError as exc:
            print(f"PROBLEM {exc}")
            return 1
        if opts.json:
            print(json.dumps(rep, indent=2, ensure_ascii=False))
        else:
            for path in rep["outside"]:
                print("OUTSIDE " + path)
            for entry in rep["ignored_entries"]:
                print("NOTE SCOPE entry is not a path pattern and was ignored: " + entry)
            if rep["no_scope"]:
                print("PROBLEM the brief has no usable SCOPE line")
            print(f"check_claim_scope: BRIEF {rep['task']}: {rep['changed']} changed path(s), {len(rep['inside'])} inside SCOPE, {len(rep['outside'])} outside")
        return 1 if rep["outside"] or rep["no_scope"] else 0
    coord = opts.coord or (Path(os.environ["KOHBOR_COORD_DIR"]) if os.environ.get("KOHBOR_COORD_DIR") else Path("F:/kohbor-coord"))
    opts.coord = coord if coord.is_dir() else None
    try:
        rep = run(opts)
    except Exception as exc:  # a report-only tool must not break the gate by crashing
        rep = {"error": repr(exc), "outside": [], "no_claim": True, "changed": 0, "claims": [], "inside": [], "shared_integration_files": [], "ignored_entries": [],
               "branch": "?", "diff": "?", "base": "?", "base_source": "?", "vacuous": False, "work_after_close": [], "claim_branch_mismatch": [],
               "base_override_ignored": None}
    mode = "ENFORCED" if opts.enforce else "REPORT-ONLY"
    bad = bool(rep["outside"]) or rep["no_claim"] or "error" in rep
    if opts.json:
        print(json.dumps(dict(rep, mode=mode), indent=2, ensure_ascii=False))
    else:
        if "error" in rep:
            print("PROBLEM check_claim_scope crashed: " + rep["error"])
        for name in rep["work_after_close"]:
            print(f"WORK AFTER CLOSE claim {name} is closed and the tree is dirty or HEAD is not its last_commit: it no longer covers this work")
        for name in rep["claim_branch_mismatch"]:
            print(f"PROBLEM explicit claim {name} belongs to another branch and covers nothing here")
        if rep["base_override_ignored"]:
            print("NOTE " + rep["base_override_ignored"])
        if rep["no_claim"] and "error" not in rep:
            print(f"PROBLEM branch {rep['branch']} has no {'active or valid done' if opts.include_done else 'active'} claim in the coordination ledger")
        if rep["vacuous"]:
            print("NOTE scope check is vacuous: a claim covers the whole checkout, so every path counts as inside")
        for path in rep["outside"]:
            print("OUTSIDE " + path)
        for entry in rep["ignored_entries"]:
            print("NOTE write_area entry is not a path pattern and was ignored: " + entry)
        print(f"check_claim_scope: {mode}: {rep['changed']} changed, {len(rep['inside'])} inside, {len(rep['shared_integration_files'])} shared, "
              f"{len(rep['outside'])} outside; branch {rep['branch']}; base {rep['base']} ({rep['base_source']}); claims {', '.join(rep['claims']) or 'none'}")
    return 1 if bad and opts.enforce else 0


if __name__ == "__main__":
    raise SystemExit(main())
