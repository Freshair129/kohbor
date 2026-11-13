#!/usr/bin/env python3
"""Check the domain registry, the domain cards and the document index.

Source-class evidence about documents only: it says nothing about the application.

Usage: ``python tools/validate_domains.py [--root DIR] [--json] [--fail-on-stale]
[--no-git] [--coord DIR] [--stale-commit-limit N]``.

FAIL (exit 1): an unindexed document; a missing card, state file or artefact; a
dangling requirement, decision, task-prefix or domain reference; two cards that both
claim one document as ``governing``; a secret, email, URL, personal or drive-letter
path in a card.
WARN (exit 0, unless ``--fail-on-stale``): a stale card (a governing artefact changed
since it was reviewed, or many commits touched the write areas since the card was
verified); an expired claim; an external root that was not checked here.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath

SKIP_DIRS = {"node_modules", "__pycache__", "target", ".git", "spike-out", ".claude"}
# Modules of the requirement registry are read from contracts/requirements.json; ACCOUNT is the proposed module.
EXTRA_REQ_MODULES = {"ACCOUNT"}
# Id families that look like requirement ids but are other things (or other registries); never flagged as dangling.
NON_REQ_PREFIXES = {"SHA", "AES", "RFC", "ISO", "UTF", "CVE", "TLS", "SSL", "PCM", "WAV", "PN", "DGM", "CMP", "MP", "TP", "UAT", "AC", "ADR", "DEC"}
ID_LIKE_RE = re.compile(r"(?<![A-Za-z0-9])([A-Za-z]{2,10})-(\d{3,4})(?![A-Za-z0-9])")
DEC_RE = re.compile(r"(?<![A-Za-z0-9])[Dd][Ee][Cc]-[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*")
TASK_RE = re.compile(r"^\|\s*(S\d+-[0-9A-Za-z]+)")
TASK_ID_IN_TEXT_RE = re.compile(r"(?<![A-Za-z0-9])S\d+-[0-9][0-9A-Za-z]*")
LOGICAL_ROOT_RE = re.compile(r"^([a-z][a-z0-9-]+):(?![\\/])")
KNOWN_ROOTS = {"lab", "coord", "packs-src"}
DOMAIN_ID_RE = re.compile(r"^[a-z][a-z0-9-]*$")
ROLE_RE = re.compile(r"^(governing|reference|source-of-truth|neighbour:[a-z][a-z0-9-]*)$")
SECRET_PATTERNS = [
    ("email address", re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+\.[A-Za-z]{2,}")),
    ("URL or endpoint", re.compile(r"\bhttps?://\S+", re.I)),
    ("drive-letter path", re.compile(r"(?<![A-Za-z0-9])[A-Za-z]:[\\/]")),
    ("personal or home path", re.compile(r"(?:/Users/|/home/|\\Users\\|AppData)", re.I)),
    ("IP address", re.compile(r"\b\d{1,3}(?:\.\d{1,3}){3}\b")),
    ("token-shaped string", re.compile(r"\b(?:sk[-_][A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9-]{10,}|AIza[0-9A-Za-z_-]{30,}|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.)")),
    ("private key block", re.compile(r"-----BEGIN [A-Z ]*KEY-----")),
    ("bearer credential", re.compile(r"\bBearer\s+[A-Za-z0-9._~+/-]{16,}")),
    ("credential assignment", re.compile(r"\b(?:password|passwd|secret|api[_-]?key|token)\s*[:=]\s*\S{6,}", re.I)),
    ("UNC, home or environment path", re.compile(r"(?:\\\\[A-Za-z0-9_.-]+\\|%[A-Za-z_]+%|(?<![A-Za-z0-9.])~[\\/]|(?<![A-Za-z0-9_])/root/|\bfile:)")),
    ("long hex string (a key or hash does not belong in a card)", re.compile(r"(?<![0-9A-Za-z])[0-9a-fA-F]{32,}(?![0-9A-Za-z])")),
    ("long opaque string", re.compile(r"(?<![0-9A-Za-z])[A-Za-z0-9+/_.-]{48,}={0,2}(?![0-9A-Za-z])")),
]


class Report:
    def __init__(self) -> None:
        self.fails: list[str] = []
        self.warns: list[str] = []
        self.stale: list[str] = []
        self.notes: list[str] = []

    def fail(self, msg: str) -> None:
        self.fails.append(msg)

    def warn(self, msg: str) -> None:
        self.warns.append(msg)

    def mark_stale(self, msg: str) -> None:
        self.stale.append(msg)


def read_json(path: Path, report: Report):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        report.fail(f"{path.as_posix()}: cannot read JSON: {exc}")
        return None


def git(root: Path, *args: str) -> str | None:
    try:
        done = subprocess.run(["git", *args], cwd=root, capture_output=True, text=True, encoding="utf-8", errors="replace", check=False)
    except OSError:
        return None
    return done.stdout if done.returncode == 0 else None


def list_markdown(root: Path, use_git: bool) -> list[str]:
    """Return repo-relative posix paths of Markdown files the index must cover."""
    names: list[str] | None = None
    if use_git:
        out = git(root, "ls-files", "-z", "--cached", "--others", "--exclude-standard")
        if out is not None:
            names = [n for n in out.split("\0") if n.lower().endswith(".md")]
    if names is None:
        names = [p.relative_to(root).as_posix() for p in root.rglob("*") if p.suffix.lower() == ".md" and not set(p.relative_to(root).parts) & SKIP_DIRS]
    keep = []
    for name in names:
        parts = PurePosixPath(name).parts
        if set(parts) & SKIP_DIRS or not (root / name).is_file():
            continue
        keep.append(name)  # every Markdown file in the tree (except skipped build and tool folders)
    return sorted(set(keep))


def linked_from_index(root: Path) -> set[str]:
    index = root / "docs" / "INDEX.md"
    found: set[str] = set()
    if not index.is_file():
        return found
    for target in re.findall(r"\]\(([^)\s]+)(?:\s[^)]*)?\)", index.read_text(encoding="utf-8-sig")):
        target = target.split("#", 1)[0]
        if not target or re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target):
            continue
        norm = os.path.normpath(os.path.join("docs", target)).replace("\\", "/")
        found.add(norm)
    return found


def parse_registry(root: Path, report: Report) -> dict[str, dict]:
    reg = root / "docs" / "domains" / "README.md"
    rows: dict[str, dict] = {}
    if not reg.is_file():
        report.fail("docs/domains/README.md: the domain registry is missing")
        return rows
    for line in reg.read_text(encoding="utf-8-sig").splitlines():
        m = re.match(r"^\|\s*`([a-z][a-z0-9-]*)`\s*\|\s*([ABC])\s*\|\s*(.*?)\s*\|", line)
        if not m:
            continue
        dom, tier, card = m.groups()
        link = re.search(r"\]\(([^)]+)\)", card)
        if dom in rows:
            report.fail(f"registry: duplicate domain id {dom}")
        cells = [c.replace("\\|", "|").strip() for c in re.split(r"(?<!\\)\|", line.strip().strip("|"))]
        defaults = [t.strip().strip("`") for t in (cells[5] if len(cells) >= 6 else "").split(",")]
        defaults = [t for t in defaults if t and t.lower() not in ("none", "-")]
        rows[dom] = {"tier": tier, "card_link": link.group(1) if link else None, "answers": card, "default_modules": defaults}
    if not rows:
        report.fail("docs/domains/README.md: no registry rows found")
    return rows


def sha256_of(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def lint_text(label: str, text: str, report: Report, *, skip_keys: tuple[str, ...] = ()) -> None:
    for name, pattern in SECRET_PATTERNS:
        for m in pattern.finditer(text):
            snippet = m.group(0)
            if name == "long opaque string" and re.fullmatch(r"[A-Za-z0-9_./-]+", snippet) and "/" in snippet:
                continue  # a long repo-relative path, not a credential
            report.fail(f"{label}: card lint: {name} ({snippet[:40]!r})")
            break


def inside(root: Path, path: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


def safe_repo_path(root: Path, raw: str) -> tuple[str, Path] | None:
    """Return (repo-relative posix path, full path) or None when raw is absolute, has '..' or escapes the repo."""
    cleaned = raw.replace("\\", "/")
    if cleaned.startswith("/") or re.match(r"^[A-Za-z]:", cleaned) or ".." in PurePosixPath(cleaned).parts:
        return None
    full = root / cleaned
    if not inside(root, full):
        return None
    return PurePosixPath(os.path.normpath(cleaned).replace("\\", "/")).as_posix(), full


def check_ids(text: str, label: str, known_req: set[str], known_dec: set[str], own_req: set[str], task_ids: set[str], report: Report) -> None:
    modules = {r.split("-", 1)[0] for r in known_req} | EXTRA_REQ_MODULES
    seen: set[str] = set()
    for m in ID_LIKE_RE.finditer(text):
        prefix, digits = m.group(1), m.group(2)
        up = prefix.upper()
        raw = m.group(0)
        if up in modules:
            ident = f"{up}-{digits}"
            if raw != ident or len(digits) != 3:
                report.fail(f"{label}: id {raw!r} is not a canonical requirement id such as {up}-001")
            elif ident not in known_req and ident not in own_req and ident not in seen:
                report.fail(f"{label}: dangling requirement id {ident}")
            seen.add(ident)
        elif prefix == up and len(prefix) >= 3 and len(digits) == 3 and up not in NON_REQ_PREFIXES:
            if raw not in seen:
                seen.add(raw)
                report.fail(f"{label}: id {raw} has an unknown family; add the family to the checker's allow-list if it is not a requirement id")
    for m in DEC_RE.finditer(text):
        raw = m.group(0)
        if raw not in known_dec:
            report.fail(f"{label}: dangling decision id {raw}")
    for m in TASK_ID_IN_TEXT_RE.finditer(text):
        tid = m.group(0)
        if task_ids and not any(tid.startswith(t) or t.startswith(tid) for t in task_ids):
            report.fail(f"{label}: task id {tid} matches no backlog task")


def check_card_text(root: Path, rel: str, dom: str, state: dict, known_req: set[str], known_dec: set[str],
                    task_ids: set[str], report: Report) -> None:
    text = (root / rel).read_text(encoding="utf-8-sig")
    lint_text(rel, text, report)
    own_req = set(state.get("requirement_ids", [])) | set(state.get("proposed_requirement_ids", []))
    check_ids(text, rel, known_req, known_dec, own_req, task_ids, report)
    for target in re.findall(r"\]\(([^)\s]+)(?:\s[^)]*)?\)", text):
        t = target.split("#", 1)[0]
        if not t or re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", t):
            continue
        linked = (root / rel).parent.joinpath(t)
        if not inside(root, linked):
            report.fail(f"{rel}: link {target} leaves the repository")
        elif not linked.exists():
            report.fail(f"{rel}: broken link {target}")
    m = re.search(r"^## 3\..*?\n(.*?)(?=^## )", text, re.M | re.S)
    if m:
        lines = [ln for ln in m.group(1).splitlines() if ln.strip().startswith("- ")]
        limit = 12 if state.get("tier") == "A" else 8
        if len(lines) > limit:
            report.fail(f"{rel}: 'Now / Next / Blocked' has {len(lines)} lines; the limit is {limit}")
    limit_lines = 120 if state.get("tier") == "A" else 80
    if len(text.splitlines()) > limit_lines:
        report.fail(f"{rel}: {len(text.splitlines())} lines; the limit is {limit_lines}")
    fm = re.match(r"^---\n(.*?)\n---\n", text, re.S)
    if not fm:
        report.fail(f"{rel}: front matter is missing")
    else:
        for key in ("domain", "tier", "card_version", "verified_at_commit", "verified_on", "verified_by"):
            if not re.search(rf"^{key}:", fm.group(1), re.M):
                report.fail(f"{rel}: front matter lacks {key}")
        dm = re.search(r"^domain:\s*(\S+)", fm.group(1), re.M)
        if dm and dm.group(1) != dom:
            report.fail(f"{rel}: front matter domain {dm.group(1)} does not match {dom}")


def check_state(root: Path, dom: str, registry: dict[str, dict], state: dict, known_req: set[str], known_dec: set[str],
                task_ids: set[str], governing: dict[str, str], opts, report: Report) -> None:
    rel = f"docs/domains/{dom}/state.json"
    if state.get("domain") != dom:
        report.fail(f"{rel}: domain is {state.get('domain')!r}, expected {dom!r}")
    if state.get("tier") != registry[dom]["tier"]:
        report.fail(f"{rel}: tier {state.get('tier')!r} differs from the registry tier {registry[dom]['tier']!r}")
    for key in ("schema_version", "verified_at_commit", "verified_on", "verified_by", "write_areas", "decision_ids", "artefacts"):
        if key not in state:
            report.fail(f"{rel}: missing key {key}")
    for rid in state.get("requirement_ids", []):
        if rid not in known_req:
            report.fail(f"{rel}: dangling requirement id {rid}")
    for did in state.get("decision_ids", []):
        if did not in known_dec:
            report.fail(f"{rel}: dangling decision id {did}")
    for prefix in state.get("task_id_prefixes", []):
        if not re.match(r"^S\d+-", str(prefix)) or not any(t.startswith(prefix) for t in task_ids):
            report.fail(f"{rel}: task prefix {prefix!r} is empty, malformed or matches no backlog task")
    for area in state.get("write_areas", []):
        if re.match(r"^[A-Za-z]:[\\/]", area):
            report.fail(f"{rel}: write area {area!r} is a machine path; use a repo path or a logical root")
    own_req = set(state.get("requirement_ids", [])) | set(state.get("proposed_requirement_ids", []))
    for oq in state.get("open_questions", []):
        check_ids(json.dumps(oq, ensure_ascii=False), f"{rel} open question {oq.get('id', '?')}", known_req, known_dec, own_req, task_ids, report)
    stale_reasons: list[str] = []
    external: dict[str, int] = {}
    for art in state.get("artefacts", []):
        path = art.get("path", "")
        role = art.get("role", "")
        if not ROLE_RE.match(role):
            report.fail(f"{rel}: artefact {path!r} has invalid role {role!r}")
        if role.startswith("neighbour:") and role.split(":", 1)[1] not in registry:
            report.fail(f"{rel}: artefact {path!r} names unknown neighbour domain {role}")
        lm = LOGICAL_ROOT_RE.match(path)
        if lm:
            if lm.group(1) not in KNOWN_ROOTS:
                report.fail(f"{rel}: artefact {path!r} uses unknown logical root '{lm.group(1)}:' (known: {', '.join(sorted(KNOWN_ROOTS))})")
            else:
                external[lm.group(1)] = external.get(lm.group(1), 0) + 1
            continue
        safe = safe_repo_path(root, path.split("#", 1)[0])
        if safe is None:
            report.fail(f"{rel}: artefact {path!r} is a machine path, has '..' or leaves the repository")
            continue
        file_path, full = safe
        if art.get("exists") is False:
            if full.exists():
                report.fail(f"{rel}: artefact {path!r} is marked exists:false but exists")
            continue
        if not full.exists():
            report.fail(f"{rel}: artefact {path!r} does not exist")
            continue
        if role == "governing" and file_path.endswith((".md", ".json", ".html")) and full.is_file():
            key = os.path.normcase(str(full.resolve()))
            other = governing.get(key)
            if other and other != dom:
                report.fail(f"{rel}: {file_path} is 'governing' for both {other} and {dom}; exactly one primary domain")
            governing[key] = dom
        reviewed = art.get("reviewed_sha256")
        if reviewed and full.is_file() and sha256_of(full) != reviewed:
            stale_reasons.append(f"{file_path} changed since review")
    if not opts.no_git and state.get("verified_at_commit"):
        areas = [a.replace("/**", "").rstrip("/") for a in state.get("write_areas", []) if not LOGICAL_ROOT_RE.match(a)]
        areas = [a for a in areas if a and not a.startswith("git ")]
        existing = [a for a in areas if (root / a).exists()]
        if existing:
            commit = state["verified_at_commit"]
            out = git(root, "rev-list", "--count", f"{commit}..HEAD", "--", *existing)
            if out is None:
                report.warn(f"{rel}: verified_at_commit {commit} is not a commit of this repository; cannot measure freshness")
            elif int(out.strip() or 0) > opts.stale_commit_limit:
                stale_reasons.append(f"{out.strip()} commits touched the write areas since {commit}")
    for name, count in sorted(external.items()):
        report.notes.append(f"{rel}: {count} artefact(s) under external root '{name}:', not checked here")
    if stale_reasons:
        report.mark_stale(f"{dom}: STALE ({'; '.join(stale_reasons)})")


def check_claims(coord: Path | None, report: Report) -> None:
    if coord is None or not coord.is_dir():
        report.notes.append("coordination ledger: external, not checked here")
        return
    claims = coord / "claims"
    if not claims.is_dir():
        report.notes.append("coordination ledger: no claims folder")
        return
    now = dt.datetime.now(dt.timezone.utc)
    for f in sorted(claims.glob("*.json")):
        try:
            text = f.read_text(encoding="utf-8")
            try:
                claim = json.loads(text)
            except ValueError:
                claim = json.loads(text.replace("\\", "\\\\"))
                report.warn(f"claim {f.name}: not valid JSON (unescaped backslashes); parsed leniently")
        except (OSError, ValueError):
            report.warn(f"claim {f.name}: unreadable")
            continue
        if claim.get("status") != "active":
            continue
        try:
            expires = dt.datetime.fromisoformat(str(claim.get("expires")))
            if expires.tzinfo is None:
                expires = expires.replace(tzinfo=dt.timezone.utc)
            if expires < now:
                report.warn(f"claim {f.name}: expired {claim.get('expires')}; an expired claim does not free its area, check the holder and ask the owner")
        except (TypeError, ValueError):
            report.warn(f"claim {f.name}: no valid expires value")


def build_taxonomy(root: Path, registry: dict[str, dict], states: dict[str, dict], reqs: list[dict]) -> dict:
    """Primary domain of every requirement, derived from the cards (rule 1) and the registry's Default modules (rule 2).

    Rule 1: exactly one card lists the requirement in requirement_ids. Rule 2: exactly one registry row names its module
    (or its id) in Default modules. Several candidates is a CONFLICT; none is UNASSIGNED. Nothing is stored.
    """
    out_reqs: dict[str, dict] = {}
    for r in reqs:
        rid = r["id"]
        listed = sorted(d for d, s in states.items() if rid in s.get("requirement_ids", []))
        defaults = sorted(d for d, row in registry.items() if r.get("module") in row.get("default_modules", []) or rid in row.get("default_modules", []))
        if len(listed) == 1:
            primary, status = listed[0], "card"
        elif len(listed) > 1:
            primary, status = None, "conflict"
        elif len(defaults) == 1:
            primary, status = defaults[0], "default"
        elif len(defaults) > 1:
            primary, status = None, "conflict"
        else:
            primary, status = None, "unassigned"
        out_reqs[rid] = {"module": r.get("module"), "component": r.get("primary_design_component_id"), "milestone": r.get("target_milestone"),
                         "primary": primary, "status": status, "listed_by": listed, "default_candidates": defaults}
    return out_reqs


def file_ownership(root: Path, states: dict[str, dict], use_git: bool) -> dict:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import check_claim_scope as ccs

    regexes: dict[str, list] = {}
    for dom, s in states.items():
        pats = [p for a in s.get("write_areas", []) if not LOGICAL_ROOT_RE.match(a) for p in ccs.expand_braces(a)]
        regexes[dom] = [ccs.glob_to_regex(p) for p in pats]
    shared = [ccs.glob_to_regex(p) for p in ccs.SHARED]
    out = git(root, "ls-files", "-z") if use_git else None
    source = "git" if out is not None else "filesystem"  # without git, untracked files are counted too
    files = [f for f in out.split("\0") if f] if out is not None else [p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file() and not set(p.relative_to(root).parts) & SKIP_DIRS]
    owned: dict[str, int] = {d: 0 for d in states}
    no_owner: list[str] = []
    several: dict[str, list[str]] = {}
    shared_files = 0
    for f in files:
        owners = [d for d, rs in regexes.items() if any(r.match(f) for r in rs)]
        if len(owners) > 1:
            several[f] = sorted(owners)
        for d in owners:
            owned[d] += 1
        if not owners:
            if any(s.match(f) for s in shared):
                shared_files += 1
            else:
                no_owner.append(f)
    return {"file_source": source, "tracked_files": len(files), "owned_by_one": sum(1 for f in files if sum(1 for rs in regexes.values() if any(r.match(f) for r in rs)) == 1),
            "per_domain": owned, "no_owner": no_owner, "several_owners": several, "shared_integration_files": shared_files}


def crate_checks(root: Path, states: dict[str, dict], report: Report) -> None:
    owners: dict[str, list[str]] = {}
    planned: dict[str, list[str]] = {}
    for dom, s in states.items():
        for crate in s.get("crates", []):
            owners.setdefault(crate, []).append(dom)
            if not (root / "crates" / crate).is_dir():
                report.warn(f"docs/domains/{dom}/state.json: crate {crate!r} does not exist under crates/")
        for crate in s.get("planned_crates", []):
            planned.setdefault(crate, []).append(dom)
            if (root / "crates" / crate).is_dir():
                report.warn(f"docs/domains/{dom}/state.json: planned crate {crate!r} now exists; move it to 'crates'")
    for crate, doms in sorted(owners.items()):
        if len(doms) > 1:
            report.warn(f"crate {crate} is claimed by more than one card: {', '.join(sorted(doms))}")
    for crate, doms in sorted(planned.items()):
        if len(doms) > 1:
            report.warn(f"planned crate {crate} is claimed by more than one card: {', '.join(sorted(doms))}")
    crates_dir = root / "crates"
    if crates_dir.is_dir():
        for child in sorted(p.name for p in crates_dir.iterdir() if p.is_dir()):
            if child not in owners:
                report.warn(f"crate {child} is not named in any domain card (state.json 'crates')")


def taxonomy_checks(root: Path, registry: dict[str, dict], states: dict[str, dict], reqs: list[dict], report: Report) -> dict:
    tax = build_taxonomy(root, registry, states, reqs)
    for rid, t in tax.items():
        if t["status"] == "conflict":
            cands = t["listed_by"] or t["default_candidates"]
            report.warn(f"requirement {rid} has no single primary domain (candidates: {', '.join(cands)}); keep it in one card and move the others to interface_requirement_ids")
        elif t["status"] == "unassigned":
            report.warn(f"requirement {rid} ({t['module']}) is UNASSIGNED: list it in one card or add its module to a registry row's Default modules")
    known = set(tax)
    for dom, s in states.items():
        for rid in s.get("interface_requirement_ids", []):
            if rid not in known:
                report.fail(f"docs/domains/{dom}/state.json: dangling interface requirement id {rid}")
                continue
            if tax[rid]["primary"] == dom:
                report.warn(f"docs/domains/{dom}/state.json: {rid} is primary here; remove it from interface_requirement_ids")
            elif tax[rid]["primary"] is None:
                report.warn(f"docs/domains/{dom}/state.json: interface requirement {rid} has no primary domain elsewhere")
            if rid in s.get("requirement_ids", []) and tax[rid]["primary"] != dom:
                report.warn(f"docs/domains/{dom}/state.json: {rid} is in both requirement_ids and interface_requirement_ids")
    for dom, row in registry.items():
        for tok in row.get("default_modules", []):
            if tok not in known and tok not in {r.get("module") for r in reqs}:
                report.warn(f"registry: Default modules of {dom} names {tok!r}, which is neither a requirement module nor a requirement id")
    return tax


def taxonomy_report(root: Path, opts) -> dict:
    report = Report()
    req_file = read_json(root / "contracts" / "requirements.json", report)
    reqs = req_file["requirements"] if req_file else []
    registry = parse_registry(root, report)
    states: dict[str, dict] = {}
    unreadable: list[str] = []
    for dom, row in registry.items():
        p = root / "docs" / "domains" / dom / "state.json"
        if row["tier"] in ("A", "B") and p.is_file():
            try:
                data = json.loads(p.read_text(encoding="utf-8-sig"))
            except ValueError:
                data = None
            if not isinstance(data, dict) or any(not isinstance(data.get(k, []), list) for k in ("requirement_ids", "interface_requirement_ids", "crates", "planned_crates", "write_areas")):
                states[dom] = {}
                unreadable.append(dom)  # not an object of the expected shape: the map is incomplete for this domain
            else:
                states[dom] = data
    tax = build_taxonomy(root, registry, states, reqs)
    doms = []
    for dom, row in sorted(registry.items()):
        mine = [t for t in tax.values() if t["primary"] == dom]
        s = states.get(dom, {})
        doms.append({"domain": dom, "tier": row["tier"], "requirements": len(mine), "modules": sorted({t["module"] for t in mine}),
                     "components": sorted({t["component"] for t in mine}), "crates": s.get("crates", []), "planned_crates": s.get("planned_crates", []),
                     "default_modules": row.get("default_modules", [])})
    files = file_ownership(root, states, not opts.no_git)
    for d in doms:
        d["tracked_files"] = files["per_domain"].get(d["domain"], 0)
    counts: dict[str, int] = {}
    for t in tax.values():
        counts[t["status"]] = counts.get(t["status"], 0) + 1
    return {"requirements_total": len(tax), "requirement_status": counts, "requirements": tax, "domains": doms, "unreadable_state": unreadable, "input_problems": list(report.fails),
            "incomplete": bool(unreadable or report.fails or not reqs or not registry),
            "files": {k: v for k, v in files.items() if k != "per_domain"}}


def print_map(rep: dict) -> None:
    print("| Domain | Tier | Primary reqs | Modules | Components | Crates | Files owned |")
    print("| --- | --- | --- | --- | --- | --- | --- |")
    for d in rep["domains"]:
        crates = ", ".join(d["crates"] + [c + " (planned)" for c in d["planned_crates"]]) or "none"
        print(f"| {d['domain']} | {d['tier']} | {d['requirements']} | {', '.join(d['modules']) or 'none'} | {', '.join(d['components']) or 'none'} | {crates} | {d['tracked_files']} |")
    if rep.get("unreadable_state"):
        print("\nINCOMPLETE: state.json is not a valid JSON object of the expected shape for " + ", ".join(rep["unreadable_state"]))
    for line in rep.get("input_problems", []):
        print("INCOMPLETE: " + line)
    if rep.get("incomplete") and not rep.get("input_problems") and not rep.get("unreadable_state"):
        print("INCOMPLETE: no requirements or no domain registry rows were found")
    print(f"\nRequirements: {rep['requirements_total']} total; " + ", ".join(f"{k} {v}" for k, v in sorted(rep["requirement_status"].items())))
    f = rep["files"]
    top: dict[str, int] = {}
    for p in f["no_owner"]:
        key = "/".join(p.split("/")[:2]) if "/" in p else p
        top[key] = top.get(key, 0) + 1
    label = "Tracked files" if f.get("file_source") == "git" else "Files scanned on disk (git unavailable: untracked files are counted)"
    print(f"{label}: {f['tracked_files']}; owned by one write area: {f['owned_by_one']}; no owner: {len(f['no_owner'])}; "
          f"several owners: {len(f['several_owners'])}; shared integration files: {f['shared_integration_files']}")
    if top:
        print("No-owner folders: " + ", ".join(f"{k} ({v})" for k, v in sorted(top.items(), key=lambda kv: -kv[1])[:12]))


def run(root: Path, opts) -> Report:
    report = Report()
    states: dict[str, dict] = {}
    req_file = read_json(root / "contracts" / "requirements.json", report)
    dec_file = read_json(root / "contracts" / "decisions.json", report)
    known_req = {r["id"] for r in req_file["requirements"]} if req_file else set()
    known_dec = {d["id"] for d in dec_file["decisions"]} if dec_file else set()
    roadmap = root / "docs" / "delivery" / "SPRINT-ROADMAP.md"
    task_ids: set[str] = set()
    if roadmap.is_file():
        for line in roadmap.read_text(encoding="utf-8-sig").splitlines():
            m = TASK_RE.match(line)
            if m:
                task_ids.add(m.group(1))
    registry = parse_registry(root, report)
    governing: dict[str, str] = {}
    artefact_paths: set[str] = set()
    domains_dir = root / "docs" / "domains"
    for dom, row in sorted(registry.items()):
        if row["tier"] in ("A", "B"):
            if not row["card_link"]:
                report.fail(f"registry: {dom} is tier {row['tier']} but has no card link")
                continue
            card = (domains_dir / row["card_link"]).resolve()
            state_path = card.parent / "state.json"
            if card.parent != (domains_dir / dom).resolve() or card.name != "CARD.md":
                report.fail(f"registry: card link of {dom} must be {dom}/CARD.md, found {row['card_link']}")
                continue
            rel_card = card.relative_to(root.resolve()).as_posix()
            if not card.is_file():
                report.fail(f"registry: card for {dom} is missing: {rel_card}")
                continue
            if not state_path.is_file():
                report.fail(f"{dom}: state.json is missing")
                continue
            state = read_json(state_path, report)
            if state is None:
                continue
            if not isinstance(state, dict):
                report.fail(f"docs/domains/{dom}/state.json: must be a JSON object")
                continue
            states[dom] = state
            check_state(root, dom, registry, state, known_req, known_dec, task_ids, governing, opts, report)
            check_card_text(root, rel_card, dom, state, known_req, known_dec, task_ids, report)
            lint_text(f"docs/domains/{dom}/state.json", json.dumps({k: v for k, v in state.items() if k != "artefacts"}) +
                      json.dumps([{k: v for k, v in a.items() if k != "reviewed_sha256"} for a in state.get("artefacts", [])]), report)
            for art in state.get("artefacts", []):
                artefact_paths.add(art.get("path", "").split("#", 1)[0])
        else:
            if row["card_link"]:
                report.fail(f"registry: {dom} is tier C and must not have a card link")
    if domains_dir.is_dir():
        for child in sorted(p for p in domains_dir.iterdir() if p.is_dir()):
            if child.name not in registry:
                report.fail(f"docs/domains/{child.name}: folder has no registry row")
            elif registry[child.name]["tier"] == "C":
                report.fail(f"docs/domains/{child.name}: tier C domains have no folder")
            charter = child / "CHARTER.json"
            if charter.is_file():
                lint_text(f"docs/domains/{child.name}/CHARTER.json", charter.read_text(encoding="utf-8"), report)
    if req_file:
        taxonomy_checks(root, registry, states, req_file["requirements"], report)
    crate_checks(root, states, report)
    indexed = linked_from_index(root)
    for name in list_markdown(root, not opts.no_git):
        if name == "docs/INDEX.md" or (name.startswith("docs/domains/") and name.endswith("/CARD.md")):
            continue
        if name not in indexed and name not in artefact_paths:
            report.fail(f"{name}: not indexed (add it to docs/INDEX.md or to exactly one domain card)")
    check_claims(opts.coord_path, report)
    return report


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    ap.add_argument("--json", action="store_true", help="print a JSON result")
    ap.add_argument("--fail-on-stale", action="store_true", help="treat a stale card as a failure (sprint exit)")
    ap.add_argument("--map", action="store_true", help="print the taxonomy map (domain, requirements, modules, components, crates, file ownership); never fails")
    ap.add_argument("--no-git", action="store_true", help="do not use git (file listing and commit freshness)")
    ap.add_argument("--coord", type=Path, default=None, help="coordination ledger folder (default: KOHBOR_COORD_DIR or F:\\kohbor-coord)")
    ap.add_argument("--stale-commit-limit", type=int, default=40, help="commits on the write areas after which a card is stale")
    opts = ap.parse_args(argv)
    root = opts.root.resolve()
    if opts.map:
        rep = taxonomy_report(root, opts)
        if opts.json:
            print(json.dumps(rep, indent=2, ensure_ascii=False))
        else:
            print_map(rep)
        return 0
    coord = opts.coord or (Path(os.environ["KOHBOR_COORD_DIR"]) if os.environ.get("KOHBOR_COORD_DIR") else Path("F:/kohbor-coord"))
    opts.coord_path = coord if coord.is_dir() else None
    report = run(root, opts)
    failures = list(report.fails) + (list(report.stale) if opts.fail_on_stale else [])
    status = "FAIL" if failures else "PASS"
    summary = (f"validate_domains: {status}: {len(report.fails)} failure(s), {len(report.stale)} stale card(s), "
               f"{len(report.warns)} warning(s), {len(report.notes)} external item(s) not checked here")
    if opts.json:
        print(json.dumps({"status": status, "failures": report.fails, "stale": report.stale, "warnings": report.warns,
                          "external_not_checked": report.notes, "scope": "documents only; no application evidence"}, indent=2))
    else:
        for line in report.fails:
            print("FAIL " + line)
        for line in report.stale:
            print("STALE " + line)
        for line in report.warns:
            print("WARN " + line)
        for line in report.notes:
            print("NOTE " + line)
        print(summary)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
