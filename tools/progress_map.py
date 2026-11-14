#!/usr/bin/env python3
"""Progress map generator: reads the real project sources and writes ONE self-contained HTML mind map.

It is a generator, not a hand-drawn page. Every status on the map is derived by the documented rules in
docs/governance/PROGRESS-MAP.md from files that exist: contracts/requirements.json (RTM.csv as fallback),
contracts/decisions.json, docs/domains/*/state.json and CARD.md, the coordination ledger (claims, registry), the
Backlog table of docs/delivery/SPRINT-ROADMAP.md, design documents, and git. A source that cannot be read is listed on
the page as not read; a node with no data is "unknown", never "done". It needs no network (``--github`` is optional) and
writes nothing inside the repository (default output: F:\\kohbor-local\\progress\\progress-map.html).

Usage: ``python tools/progress_map.py [--repo DIR] [--coord DIR] [--design-dir DIR] [--builds-dir DIR] [--out FILE] [--open] [--github]
[--now ISO8601] [--stale-hours N] [--check] [--json]``

Standard library only. The page uses inline CSS, JavaScript and SVG, no external font, script or image.
"""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import fnmatch
import glob
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import webbrowser
from pathlib import Path
from typing import Any, Callable

DEFAULT_OUT = r"F:\kohbor-local\progress\progress-map.html"
DEFAULT_COORD = r"F:\kohbor-coord"
DEFAULT_DESIGN = r"F:\kohbor-design"
DEFAULT_BUILDS = r"F:\kohbor-local"
STALE_HOURS = 6.0

# ---------------------------------------------------------------------------------------------------------------
# Status vocabulary (rules documented in docs/governance/PROGRESS-MAP.md). rank = how far along; ties in a roll-up go
# to the lower rank so a mixed group is never shown as better than its plurality.
# ---------------------------------------------------------------------------------------------------------------
STATUSES: list[dict[str, Any]] = [
    {"id": "verified", "en": "Verified", "th": "ยืนยันแล้ว", "glyph": "✓", "rank": 6},
    {"id": "built", "en": "Built, not verified", "th": "สร้างแล้ว ยังไม่ยืนยัน", "glyph": "◐", "rank": 5},
    {"id": "progress", "en": "In progress", "th": "กำลังทำ", "glyph": "▶", "rank": 4},
    {"id": "designed", "en": "Designed only", "th": "ออกแบบแล้ว", "glyph": "✎", "rank": 3},
    {"id": "notstarted", "en": "Not started", "th": "ยังไม่เริ่ม", "glyph": "○", "rank": 2},
    {"id": "blocked", "en": "Needs the owner", "th": "รอเจ้าของตัดสินใจ", "glyph": "!", "rank": 1},
    {"id": "unknown", "en": "Unknown", "th": "ไม่ทราบ", "glyph": "?", "rank": 0},
]
RANK = {s["id"]: s["rank"] for s in STATUSES}

# ---------------------------------------------------------------------------------------------------------------
# Area taxonomy. This is structure, not status: it says where a requirement module, domain, decision or document
# is drawn on the map. Nothing here says how far along anything is.
# ---------------------------------------------------------------------------------------------------------------
AREAS: list[dict[str, Any]] = [
    {"id": "core", "th": "แกนหลักและรับข้อมูลเกม", "en": "Core and GSI intake", "domains": ["host-core"],
     "modules": ["core", "data"], "requirements": ["QUALITY-002"], "exclude": ["CORE-003", "DATA-005", "DATA-006"],
     "docs": ["GSI-INTAKE-SPEC", "M1-STATUS", "M0-PLAN"], "decisions": [r"^DEC-(RESOURCE-BUDGET|OBS-BUDGET|BASELINE|NATIVE-EVIDENCE|TOOLCHAIN|GSI.*)$"],
     "tasks": [r"^S0-[3-7]\b", r"MVP-1 W0\b", r"MVP-1 integration"]},
    {"id": "announcer", "th": "ผู้ประกาศ เสียงและแพ็ก", "en": "Announcer audio and packs", "domains": ["events-audio", "packs"],
     "modules": ["media"], "requirements": ["ALERT-001", "ALERT-007", "QUALITY-001"], "exclude": [],
     "docs": ["EVENT-AUDIO-SPEC", "MEDIA-PACK-SPEC", "PACKAGE-FORMAT-REVIEW", "S3-5-HOST-CRITICAL-PATH", "S3-1-AUDIO-SPIKE"],
     "decisions": [r"^DEC-(PACK|CRITICAL-MEDIA|MEDIA|AUDIO|EVENT|FIRST-BLOOD|KILL|ANNOUNCER)"],
     "tasks": [r"^S[123]-"]},
    {"id": "vision", "th": "มินิแมปและประเมินความเสี่ยง", "en": "Minimap vision and risk", "domains": ["vision-risk"],
     "modules": ["vision"], "requirements": ["DATA-005", "DATA-006", "ALERT-002", "ALERT-003", "ALERT-004", "ALERT-005", "ALERT-006",
                                              "QUALITY-003", "QUALITY-004", "QUALITY-005"], "exclude": [],
     "docs": ["S4-OBSERVATION-BOUNDARY", "REMAINING-SYSTEM-DESIGN"], "decisions": [r"^DEC-(RISK|MODELS|GAME-DATA|OBSERV|CAPTURE|VALVE)"],
     "tasks": [r"^MVP-1 W[2-6]\b"]},
    {"id": "ui", "th": "Command Deck และโอเวอร์เลย์", "en": "Command Deck and overlay UI", "domains": ["ui-overlay"],
     "modules": ["ui"], "requirements": [], "exclude": [],
     "docs": ["UX-SPEC", "S6-OVERLAY-SLICE", "VISUAL-DESIGN-PROPOSAL", "DESIGN-NOTES"], "decisions": [r"^DEC-(VISUALS|BANNER|SESSION-MONITOR|UI)"],
     "tasks": [r"Live tab", r"MVP-2", r"overlay|Overlay"]},
    {"id": "draft", "th": "แถบดราฟต์", "en": "Draft bar", "domains": [], "modules": [], "requirements": [], "exclude": [],
     "docs": ["DRAFT-BAR-SPEC"], "decisions": [r"^DEC-DRAFT"], "tasks": [r"[Dd]raft"]},
    {"id": "account", "th": "บัญชีและแชร์กับเพื่อน", "en": "Account and sharing", "domains": ["account", "sharing"],
     "modules": ["social", "peer"], "requirements": ["CORE-003"], "exclude": [],
     "docs": ["ACCOUNT-IDENTITY-REVIEW", "SHARING-MODEL-REVIEW"], "decisions": [r"^DEC-(ACCOUNT|GID|SIGNIN|SHARE|SOCIAL|STEAM|GOOGLE|PEER)"],
     "tasks": []},
    {"id": "store", "th": "ร้านค้าและการส่งมอบแพ็ก", "en": "Store and commerce", "domains": ["commerce"],
     "modules": ["premium"], "requirements": [], "exclude": [],
     "docs": ["STORE-PACK-DELIVERY"], "decisions": [r"^DEC-(STORE|COMMERCE|PREMIUM)"], "tasks": []},
    {"id": "backend", "th": "เซิร์ฟเวอร์และโฮสติ้ง", "en": "Backend hosting", "domains": [], "modules": [], "requirements": [], "exclude": [],
     "docs": ["BACKEND-STACK", "BACKEND-WORKSTREAM-PROMPT", "KOHBOR-EXECUTION-PLAN", "ACCOUNT-CLIENT-PROMPT"],
     "decisions": [r"^DEC-(PRIVACY|RETENTION|BACKEND|HOSTING|RELAY)"], "decision_ids": ["DEC-ACCOUNT-ORIGIN", "DEC-FRONTEND-HOSTING", "DEC-STORE-HOSTING"],
     "tasks": []},
    {"id": "updater", "th": "อัปเดตและปล่อยเวอร์ชัน", "en": "Updater and release", "domains": ["release-ops"],
     "modules": ["update"], "requirements": ["QUALITY-008"], "exclude": [],
     "docs": ["UPDATER-DESIGN", "UPDATER-NOTE", "RUNBOOK", "RELEASE-RECEIPT", "RELEASE-RECEIPT-fix1", "RELEASE-RECEIPT-fix2", "RELEASE-RECEIPT-fix3"], "decisions": [r"^DEC-(UPDATE|RELEASE|SIGN)"], "tasks": [r"[Uu]pdater"]},
    {"id": "highlights", "th": "บันทึกและไฮไลต์", "en": "Local recording and highlights", "domains": [],
     "modules": ["obs"], "requirements": [], "exclude": [], "docs": [], "decisions": [r"^DEC-(OBS|RECORD)"], "tasks": []},
    {"id": "local-voice", "th": "ผู้ช่วยเสียงโต้ตอบและความจำ", "en": "Local voice, memory and advice", "domains": ["local-voice"],
     "modules": ["voice", "mem", "advice"], "requirements": ["QUALITY-006", "QUALITY-007"], "exclude": [], "docs": [],
     "decisions": [r"^DEC-(VOICE|STT|ADVICE|MEMORY|LLM)"], "tasks": []},
    {"id": "banner-lab", "th": "ห้องทดลองแบนเนอร์และเสียง", "en": "Banner lab", "domains": ["banner-lab", "banner-studio", "announcer-voice"],
     "modules": [], "requirements": [], "exclude": [], "docs": [], "decisions": [r"^DEC-(BANNER-LAB|VOICE-RIGHTS)"], "decision_ids": ["DEC-BANNER-WORKER", "DEC-BANNER-LAB-STYLE"], "tasks": []},
    {"id": "governance", "th": "กระบวนการและธรรมาภิบาล", "en": "Governance and process", "domains": ["governance"],
     "modules": [], "requirements": [], "exclude": [],
     "docs": ["AGENT-WORKFLOW", "AGENT-REGISTRY-DESIGN", "SPRINT-ROADMAP", "SDP", "SAD", "SDD", "DDD", "ICD", "MVP-PLAN", "MVP-PLAN.th", "WORKSTREAM-BRIEFS", "PREP-HANDOFF", "OWNER-HOWTO", "OWNER-HOWTO.th", "OWNER-HOWTO-fix1.th", "OWNER-HOWTO-fix2.th", "OWNER-HOWTO-fix3.th"], "decisions": [r"^DEC-(NAME|IDENTITY|AGENT|PROCESS|REGISTRY)"],
     "tasks": [r"^S0-[128]\b"]},
]
UNASSIGNED = {"id": "unassigned", "th": "ยังไม่จัดหมวด", "en": "Unassigned", "domains": [], "modules": [], "requirements": [],
              "exclude": [], "docs": [], "decisions": [], "tasks": []}

MODULE_LABELS: dict[str, tuple[str, str]] = {
    "core": ("เริ่มระบบและกรอบการทำงาน", "Core"), "data": ("ข้อมูลเกมและความสด", "Game data"),
    "media": ("สื่อ เสียง แพ็ก", "Media"), "alert": ("การแจ้งเตือน", "Alerts"), "vision": ("วิชัน", "Vision"),
    "ui": ("หน้าจอ", "UI"), "peer": ("แชร์กับเพื่อน", "Peer"), "social": ("บัญชีสังคม", "Social"),
    "premium": ("พรีเมียม", "Premium"), "update": ("อัปเดต", "Update"), "obs": ("บันทึกและไฮไลต์", "Recording"),
    "voice": ("เสียงโต้ตอบ", "Voice"), "mem": ("ความจำ", "Memory"), "advice": ("คำแนะนำ", "Advice"),
    "quality": ("คุณภาพและประสิทธิภาพ", "Quality"),
}
GENERIC_SPEC = ("docs/product/PRD.md", "docs/requirements/SRS.md")
OWNER_PAT = re.compile(r"product owner|^owner\b", re.I)
PENDING_DEC = {"open", "proposed", "provisional"}


# ---------------------------------------------------------------------------------------------------------------
# Source loading. Every loader returns data or None and records what happened in ``Sources``.
# ---------------------------------------------------------------------------------------------------------------
class Sources:
    def __init__(self) -> None:
        self.items: list[dict[str, Any]] = []

    def add(self, name: str, path: str, status: str, note: str = "") -> None:
        self.items.append({"name": name, "path": path, "status": status, "note": note})

    def read(self, name: str, path: Path | str, note: str = "") -> None:
        self.add(name, str(path), "read", note)

    def missing(self, name: str, path: Path | str, note: str = "not found") -> None:
        self.add(name, str(path), "missing", note)

    def error(self, name: str, path: Path | str, note: str) -> None:
        self.add(name, str(path), "error", note)

    def skipped(self, name: str, note: str) -> None:
        self.add(name, "", "skipped", note)


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def as_list(value: Any) -> list[Any]:
    if value is None or value == "":
        return []
    return value if isinstance(value, list) else [value]


def split_semi(value: str | None) -> list[str]:
    return [p.strip() for p in (value or "").split(";") if p.strip()]


def load_requirements(repo: Path, src: Sources) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    jpath, cpath = repo / "contracts" / "requirements.json", repo / "docs" / "requirements" / "RTM.csv"
    if jpath.is_file():
        try:
            for q in read_json(jpath).get("requirements", []):
                v = q.get("verification") or {}
                out.append({
                    "id": q.get("id", ""), "title": q.get("title", ""), "module": q.get("module", ""), "priority": q.get("priority", ""),
                    "milestone": q.get("target_milestone", ""), "impl": as_list(v.get("implementation_refs")),
                    "evidence": as_list(v.get("evidence_refs")), "required": as_list(v.get("required_evidence")),
                    "build_status": v.get("new_build_status", ""), "decisions": as_list(q.get("open_decisions")),
                    "design": as_list(q.get("design_refs")), "contracts": as_list(q.get("contract_refs")), "spec": as_list(q.get("spec_refs")),
                    "needs": as_list(q.get("product_need_ids")), "statement": q.get("statement", ""),
                })
            src.read("contracts/requirements.json", jpath, f"{len(out)} requirements")
        except (OSError, ValueError) as exc:
            src.error("contracts/requirements.json", jpath, str(exc)[:160])
            out = []
    else:
        src.missing("contracts/requirements.json", jpath)
    if cpath.is_file():
        try:
            with cpath.open(encoding="utf-8-sig", newline="") as fh:
                rows = list(csv.DictReader(fh))
            src.read("docs/requirements/RTM.csv", cpath, f"{len(rows)} rows")
            if not out:  # fallback only: the registry is the primary source
                for r in rows:
                    out.append({
                        "id": r.get("requirement_id", ""), "title": "", "module": r.get("module", ""), "priority": r.get("priority", ""),
                        "milestone": r.get("target_milestone", ""), "impl": split_semi(r.get("implementation_refs")),
                        "evidence": split_semi(r.get("evidence_refs")), "required": split_semi(r.get("required_evidence")),
                        "build_status": r.get("new_build_status", ""), "decisions": split_semi(r.get("open_decisions")),
                        "design": split_semi(r.get("design_refs")), "contracts": split_semi(r.get("contract_refs")),
                        "spec": split_semi(r.get("spec_refs")), "needs": split_semi(r.get("product_need_ids")), "statement": "",
                    })
            elif len(rows) != len(out):
                src.add("RTM consistency", str(cpath), "read", f"RTM has {len(rows)} rows, requirements.json has {len(out)}: regenerate the RTM")
        except (OSError, csv.Error) as exc:
            src.error("docs/requirements/RTM.csv", cpath, str(exc)[:160])
    else:
        src.missing("docs/requirements/RTM.csv", cpath)
    return [q for q in out if q["id"]]


def load_decisions(repo: Path, src: Sources) -> list[dict[str, Any]]:
    path = repo / "contracts" / "decisions.json"
    if not path.is_file():
        src.missing("contracts/decisions.json", path)
        return []
    try:
        data = read_json(path).get("decisions", [])
        src.read("contracts/decisions.json", path, f"{len(data)} decisions")
        return [d for d in data if d.get("id")]
    except (OSError, ValueError) as exc:
        src.error("contracts/decisions.json", path, str(exc)[:160])
        return []


CARD_ITEM = re.compile(r"^-\s+(NOW|NEXT|BLOCKED|NOT VERIFIED)\b[^:]*:\s*(.+)$", re.I)
CARD_HEAD = re.compile(r"^(NOW|NEXT|BLOCKED)\s*:?\s*$", re.I)


def parse_card_items(text: str) -> list[dict[str, str]]:
    """Items of "## 3. Now / Next / Blocked": either "- NOW: text" bullets or bullets under a bare Now/Next/Blocked line."""
    items: list[dict[str, str]] = []
    inside = False
    cur = ""
    for line in text.splitlines():
        if line.startswith("## "):
            inside = bool(re.match(r"##\s+3\.?\s+Now\s*/\s*Next\s*/\s*Blocked", line, re.I))
            cur = ""
            continue
        if not inside:
            continue
        s = line.strip()
        m = CARD_ITEM.match(s)
        if m:
            items.append({"kind": m.group(1).upper(), "text": m.group(2).strip()})
            continue
        h = CARD_HEAD.match(s)
        if h:
            cur = h.group(1).upper()
        elif cur and s.startswith("- "):
            items.append({"kind": cur, "text": s[2:].strip()})
    return items


def load_domains(repo: Path, src: Sources) -> dict[str, dict[str, Any]]:
    out: dict[str, dict[str, Any]] = {}
    base = repo / "docs" / "domains"
    if not base.is_dir():
        src.missing("docs/domains/*", base)
        return out
    for sp in sorted(base.glob("*/state.json")):
        dom = sp.parent.name
        try:
            st = read_json(sp)
        except (OSError, ValueError) as exc:
            src.error(f"docs/domains/{dom}/state.json", sp, str(exc)[:120])
            continue
        items: list[dict[str, str]] = []
        card = sp.parent / "CARD.md"
        if card.is_file():
            try:
                items = parse_card_items(card.read_text(encoding="utf-8-sig"))
            except OSError:
                items = []
        st["_card_items"] = items
        out[dom] = st
    src.read("docs/domains/*/state.json + CARD.md", base, f"{len(out)} domain cards")
    return out


def load_claims(coord: Path, src: Sources) -> list[dict[str, Any]]:
    cdir = coord / "claims"
    if not cdir.is_dir():
        src.missing("coordination claims", cdir)
        return []
    out: list[dict[str, Any]] = []
    for p in sorted(cdir.glob("*.json")):
        try:
            c = read_json(p)
            if isinstance(c, dict):
                c["_name"] = p.stem
                out.append(c)
        except (OSError, ValueError):
            continue
    src.read("coordination claims", cdir, f"{len(out)} claim files")
    return out


def load_branches(coord: Path, src: Sources) -> list[dict[str, Any]]:
    path = coord / "registry" / "branches.json"
    if not path.is_file():
        src.missing("coordination registry", path)
        return []
    try:
        data = read_json(path)
    except (OSError, ValueError) as exc:
        src.error("coordination registry", path, str(exc)[:120])
        return []
    wts: list[dict[str, Any]] = []
    for v in data.values() if isinstance(data, dict) else []:
        if isinstance(v, dict) and isinstance(v.get("worktrees"), list):
            wts.extend(w for w in v["worktrees"] if isinstance(w, dict))
    src.read("coordination registry", path, f"{len(wts)} worktrees")
    return wts


def split_row(line: str) -> list[str]:
    cells, cur, tick = [], "", False
    for ch in line.strip().strip("|"):
        if ch == "`":
            tick = not tick
        if ch == "|" and not tick:
            cells.append(cur.strip())
            cur = ""
        else:
            cur += ch
    cells.append(cur.strip())
    return cells


def load_roadmap(repo: Path, src: Sources) -> tuple[list[dict[str, str]], list[dict[str, str]]]:
    path = repo / "docs" / "delivery" / "SPRINT-ROADMAP.md"
    if not path.is_file():
        src.missing("SPRINT-ROADMAP.md", path)
        return [], []
    try:
        lines = path.read_text(encoding="utf-8-sig").splitlines()
    except OSError as exc:
        src.error("SPRINT-ROADMAP.md", path, str(exc)[:120])
        return [], []
    phases: list[dict[str, str]] = []
    tasks: list[dict[str, str]] = []
    mode = ""
    for line in lines:
        if line.startswith("## "):
            mode = "phase" if line.lower().startswith("## phase overview") else "backlog" if line.lower().startswith("## backlog status") else ""
            continue
        if not line.startswith("|") or re.match(r"^\|\s*-+", line):
            continue
        cells = split_row(line)
        if mode == "phase" and len(cells) >= 4 and cells[0].lower() != "phase":
            phases.append({"phase": cells[0], "sprint": cells[1], "milestone": cells[2], "outcome": cells[3]})
        elif mode == "backlog" and len(cells) >= 3 and cells[0].lower() != "task":
            tasks.append({"task": cells[0], "status": cells[1], "note": cells[2], "commit": cells[3] if len(cells) > 3 else ""})
    src.read("SPRINT-ROADMAP.md", path, f"{len(phases)} phases, {len(tasks)} backlog rows")
    return phases, tasks


def heading_of(path: Path) -> tuple[str, str]:
    title, status = "", ""
    try:
        for i, line in enumerate(path.read_text(encoding="utf-8-sig").splitlines()):
            if not title and line.startswith("# "):
                title = line[2:].strip()
            m = re.match(r"^\**(status|approval_status)\**\s*[:=]\s*(.+)$", line.strip(), re.I)
            if m and not status:
                status = m.group(2).strip().strip('"')[:160]
            if i > 40 and title:
                break
    except OSError:
        pass
    return title or path.stem, status


def load_docs(repo: Path, design_dirs: list[Path], src: Sources) -> list[dict[str, Any]]:
    docs: list[dict[str, Any]] = []
    roots: list[tuple[Path, str]] = [(repo / "docs" / "design", "repo"), (repo / "docs" / "delivery", "repo"), (repo / "docs" / "operations", "repo")]
    roots += [(d, "design-dir") for d in design_dirs]
    for root, kind in roots:
        if not root.is_dir():
            if kind == "design-dir":
                src.missing("design documents", root, "optional directory not found")
            continue
        found = 0
        for p in sorted(root.rglob("*.md")):
            if "node_modules" in p.parts:
                continue
            title, status = heading_of(p)
            docs.append({"stem": p.stem, "path": str(p), "title": title, "status": status, "external": kind == "design-dir"})
            found += 1
        src.read("design documents", root, f"{found} markdown files")
    return docs


def default_runner(args: list[str], cwd: Path) -> str | None:
    try:
        done = subprocess.run(args, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace", check=False, timeout=60)
    except (OSError, subprocess.SubprocessError):
        return None
    return done.stdout if done.returncode == 0 else None


Runner = Callable[[list[str], Path], "str | None"]


def load_git(repo: Path, run: Runner, src: Sources) -> dict[str, Any]:
    def g(*a: str) -> str | None:
        out = run(["git", "--no-optional-locks", *a], repo)
        return out.strip() if out is not None else None

    sha = g("rev-parse", "--short", "HEAD")
    if sha is None:
        src.skipped("git", "not a git repository or git unavailable")
        return {"available": False}
    info: dict[str, Any] = {"available": True, "sha": sha, "branch": g("rev-parse", "--abbrev-ref", "HEAD") or "",
                            "head_time": g("log", "-1", "--format=%cI") or "", "origin_sha": g("rev-parse", "--short", "origin/main") or "",
                            "origin_time": (g("log", "-1", "--format=%cI", "origin/main") or ""),
                            "behind": g("rev-list", "--count", "HEAD..origin/main") or "", "dirty": bool(g("status", "--porcelain"))}
    ref = "origin/main" if info["origin_sha"] else "HEAD"
    log = g("log", ref, "--first-parent", "-n", "30", "--pretty=%h%x09%cI%x09%s") or ""
    merged = []
    for line in log.splitlines():
        parts = line.split("\t", 2)
        if len(parts) == 3:
            m = re.search(r"\(#(\d+)\)|pull request #(\d+)", parts[2])
            merged.append({"sha": parts[0], "time": parts[1], "title": parts[2], "pr": (m.group(1) or m.group(2)) if m else ""})
    info["merged"] = merged
    info["merged_ref"] = ref
    src.read("git", repo, f"HEAD {sha} on {info['branch']}; {len(merged)} recent commits of {ref}")
    return info


def load_github(repo: Path, run: Runner, src: Sources) -> list[dict[str, Any]]:
    out = run(["gh", "pr", "list", "--state", "all", "--limit", "25", "--json", "number,title,state,mergedAt,headRefName,url"], repo)
    if out is None:
        src.error("GitHub pull requests", "gh pr list", "gh unavailable, not signed in or offline; the map does not need it")
        return []
    try:
        data = json.loads(out)
    except ValueError:
        src.error("GitHub pull requests", "gh pr list", "unreadable output")
        return []
    src.read("GitHub pull requests", "gh pr list", f"{len(data)} pull requests")
    return data


def load_gate_log(gate_dir: Path | None, src: Sources) -> dict[str, Any] | None:
    base = gate_dir or Path(tempfile.gettempdir())
    logs = sorted(glob.glob(str(base / "kohbor-gate-*.log")), key=lambda p: (os.path.getmtime(p), p))
    if not logs:
        src.skipped("tools/gate.py last result", "no cached gate log found")
        return None
    p = Path(logs[-1])
    try:
        text = p.read_text(encoding="utf-8", errors="replace")
    except OSError:
        src.skipped("tools/gate.py last result", "gate log unreadable")
        return None
    lines = [l.strip() for l in text.splitlines() if re.match(r"^(PASS|FAIL|SKIP|NOT FULLY VERIFIED|ALL CHECKS)", l.strip())][-14:]
    when = dt.datetime.fromtimestamp(os.path.getmtime(p)).astimezone().isoformat(timespec="seconds")
    src.read("tools/gate.py last result", p, "newest cached gate log; it may not belong to the current commit")
    return {"lines": lines, "time": when, "path": str(p)}


def parse_receipt(text: str, path: str) -> dict[str, Any]:
    """A release receipt written by a build session (outside the repository): commit, exe path, SHA-256 values, gate line, and the lines that say what was NOT done."""
    title = ""
    commit = ""
    exe_paths: list[str] = []
    hashes: dict[str, str] = {}
    gate = ""
    limits: list[str] = []
    for line in text.splitlines():
        s = line.strip()
        if not title and s.startswith("# "):
            title = s[2:].strip()
        if not commit:
            m = re.search(r"\bcommit\s+([0-9a-f]{40})\b", s)
            if m and (s.startswith("- Source") or "Source" in s[:12]):
                commit = m.group(1)
        if s.startswith("- Exe:"):
            m = re.search(r"([A-Za-z]:[/\\][^\s,)`]+\.exe)", s)
            if m:
                exe_paths.append(m.group(1).replace("\\", "/"))
        for m in re.finditer(r"SHA-256(?:\s+([A-Za-z0-9._-]+))?\s*(?:\([^)]*\))?\s*:\s*`?([0-9a-f]{64})`?", s):
            hashes[m.group(2)] = m.group(1) or "exe"
        if s.startswith("- Gate"):
            gate = s[2:].strip()[:220]
        if re.search(r"\bNOT RUN\b|\bNOT run\b|\bnot run\b|\bNot signed\b|\bunsigned\b|no installer", s) and not s.startswith("#"):
            limits.append(s.lstrip("- ").strip()[:260])
    return {"name": Path(path).name, "title": title, "commit": commit, "exe_paths": exe_paths, "hashes": hashes, "gate": gate, "limits": limits[:3]}


def load_receipts(dirs: list[Path], src: Sources) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for d in dirs:
        if not d.is_dir():
            continue
        for p in sorted(d.glob("RELEASE-RECEIPT*.md")):
            try:
                out.append(parse_receipt(p.read_text(encoding="utf-8-sig"), str(p)))
            except OSError:
                continue
    if out:
        src.read("release receipts", f"{len(dirs)} folder(s)", f"{len(out)} receipt files")
    else:
        src.skipped("release receipts", "no RELEASE-RECEIPT*.md found in the design folders")
    return out


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def scan_builds(builds_dir: Path | None, receipts: list[dict[str, Any]], repo: Path, run: Runner, src: Sources) -> list[dict[str, Any]]:
    """Owner builds: every .exe below the builds folder, hashed here (never in the browser), matched to a receipt by SHA-256 only."""
    if builds_dir is None or not builds_dir.is_dir():
        src.missing("owner builds", builds_dir or "(no builds folder)", "builds folder not found")
        return []
    by_hash: dict[str, tuple[dict[str, Any], str]] = {}
    for r in receipts:
        for h, label in r["hashes"].items():
            by_hash.setdefault(h, (r, label))
    out: list[dict[str, Any]] = []
    for p in sorted(builds_dir.rglob("*.exe")):
        rel = p.relative_to(builds_dir)
        if len(rel.parts) > 4:
            continue
        try:
            st = p.stat()
            digest = sha256_file(p)
        except OSError:
            continue
        hit = by_hash.get(digest)
        rec = None
        if hit:
            r, label = hit
            commit_state = "no commit in the receipt"
            if r["commit"]:
                got = run(["git", "--no-optional-locks", "rev-list", r["commit"], "--not", "origin/main", "--max-count=1"], repo)
                commit_state = "commit not found in this checkout" if got is None else ("in origin/main" if not got.strip() else "not in origin/main")
            rec = {"file": r["name"], "title": r["title"], "label": label, "commit": r["commit"], "commit_state": commit_state, "gate": r["gate"], "limits": r["limits"]}
        out.append({"path": str(p), "rel": rel.as_posix(), "group": rel.parts[0] if len(rel.parts) > 1 else "", "name": p.name, "size": st.st_size,
                    "modified": dt.datetime.fromtimestamp(st.st_mtime).astimezone().isoformat(timespec="seconds"), "sha256": digest, "receipt": rec})
    out.sort(key=lambda b: (b["modified"], b["rel"]), reverse=True)
    src.read("owner builds", builds_dir, f"{len(out)} exe files hashed; {sum(1 for b in out if b['receipt'])} match a receipt hash")
    return out


# ---------------------------------------------------------------------------------------------------------------
# Derivation
# ---------------------------------------------------------------------------------------------------------------
def prio_rank(p: str) -> int:
    m = re.match(r"^P(\d)$", p or "")
    if m:
        return int(m.group(1))
    return {"extension": 5, "later": 6}.get(p or "", 9)


def ms_rank(m: str) -> float:
    mm = re.match(r"^M(\d+)([a-z]?)", m or "")
    if not mm:
        return 99.0
    return int(mm.group(1)) + (0.1 * (ord(mm.group(2)) - 96) if mm.group(2) else 0.0)


def norm_pattern(p: str) -> str:
    p = p.replace("\\", "/").strip()
    while p.endswith("*") or p.endswith("/"):
        p = p[:-1]
    return p


def claim_hits_path(claim: dict[str, Any], path: str) -> bool:
    path = path.replace("\\", "/").rstrip("/")
    for wa in as_list(claim.get("write_area") or claim.get("write_areas")):
        pat = norm_pattern(str(wa))
        if pat and (path == pat or path.startswith(pat + "/") or pat.startswith(path + "/") or fnmatch.fnmatch(path, str(wa).replace("\\", "/"))):
            return True
    return False


def claim_cites(claim: dict[str, Any], rid: str) -> bool:
    text = " ".join(str(claim.get(k, "")) for k in ("task", "note"))
    return re.search(r"(?<![A-Z0-9-])" + re.escape(rid) + r"(?![0-9])", text) is not None


def claim_active(claim: dict[str, Any]) -> bool:
    return str(claim.get("status", "")).lower() == "active"


def claim_expired(claim: dict[str, Any], now: dt.datetime) -> bool:
    try:
        return dt.datetime.fromisoformat(str(claim.get("expires", ""))) < now
    except ValueError:
        return False


def decision_pending_owner(d: dict[str, Any]) -> bool:
    return d.get("status") in PENDING_DEC and bool(OWNER_PAT.search(str(d.get("owner_role", ""))))


def decision_question(d: dict[str, Any]) -> str:
    oq = as_list(d.get("open_questions"))
    if oq:
        first = oq[0]
        return str(first.get("text", first) if isinstance(first, dict) else first)
    return str(d.get("remaining_decision") or d.get("blocks") or "")


def exists(repo: Path, ref: str) -> bool:
    ref = ref.replace("\\", "/").strip()
    if not ref or ".." in Path(ref).parts or os.path.isabs(ref):
        return False
    return (repo / ref).exists()


def derive_requirement(q: dict[str, Any], repo: Path, claims: list[dict[str, Any]], decs: dict[str, dict[str, Any]]) -> tuple[str, str, dict[str, Any]]:
    impl_ok = [r for r in q["impl"] if exists(repo, r)]
    impl_missing = [r for r in q["impl"] if not exists(repo, r)]
    live = [c for c in claims if claim_active(c) and (any(claim_hits_path(c, r) for r in q["impl"]) or claim_cites(c, q["id"]))]
    open_decs = [decs[d] for d in q["decisions"] if d in decs and decision_pending_owner(decs[d])]
    unknown_decs = [d for d in q["decisions"] if d not in decs]
    design_ok = [r for r in q["design"] + q["contracts"] if exists(repo, r)]
    extra = {"impl_ok": impl_ok, "impl_missing": impl_missing, "claims": [c["_name"] for c in live], "open_decs": [d["id"] for d in open_decs],
             "unknown_decs": unknown_decs, "design_ok": design_ok}
    if q["evidence"] and q["build_status"] == "PASS":
        return "verified", "evidence_refs present and new_build_status is PASS for the required classes " + "/".join(q["required"]), extra
    if live and not impl_ok and not q["evidence"]:
        return "progress", "no implementation yet; an active claim covers its planned files or names it: " + ", ".join(c["_name"] for c in live), extra
    if impl_ok or q["evidence"]:
        why = f"{len(impl_ok)} implementation file(s) exist"
        why += f"; evidence refs exist but new_build_status is {q['build_status'] or 'empty'}, not PASS" if q["evidence"] else "; no evidence_refs"
        if q["build_status"]:
            why += f" (new_build_status {q['build_status']})"
        if impl_missing:
            why += f"; {len(impl_missing)} listed file(s) missing on disk"
        if live:
            why += "; an active claim also touches these files: " + ", ".join(c["_name"] for c in live)
        return "built", why, extra
    if open_decs:
        return "blocked", "no implementation; waits for the owner: " + ", ".join(d["id"] for d in open_decs), extra
    if design_ok:
        why = "no implementation_refs; design or contract exists: " + ", ".join(design_ok[:3])
        if impl_missing:
            why += f"; {len(impl_missing)} listed implementation file(s) missing on disk"
        return "designed", why, extra
    if q["priority"] or q["module"]:
        return "notstarted", "requirement exists, no implementation and no design file found on disk", extra
    return "unknown", "no data about this requirement", extra


def derive_task(row: dict[str, str]) -> tuple[str, str]:
    s = row["status"].lower()
    if "passed" in s or re.search(r"\bdone\b", s):
        return "built", "Backlog says: " + row["status"] + " (gate or review passed; this is not a live, native or acceptance result)"
    if "progress" in s or "running" in s:
        return "progress", "Backlog says: " + row["status"]
    if "blocked" in s or "parked" in s:
        return "blocked" if "owner" in s else "notstarted", "Backlog says: " + row["status"]
    return "unknown", "Backlog status not understood: " + row["status"]


def rollup(counts: dict[str, int]) -> str:
    total = sum(counts.values())
    if not total:
        return "unknown"
    best = max(counts.values())
    tied = [s for s, n in counts.items() if n == best]
    return min(tied, key=lambda s: RANK[s])


def slug(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")[:48] or "x"


def area_for_requirement(q: dict[str, Any], areas: list[dict[str, Any]]) -> str:
    for a in areas:
        if q["id"] in a["requirements"]:
            return a["id"]
    for a in areas:
        if q["module"] in a["modules"] and q["id"] not in a["exclude"]:
            return a["id"]
    return "unassigned"


def area_for_decision(d: dict[str, Any], areas: list[dict[str, Any]], dom_area: dict[str, str], dec_dom: dict[str, str]) -> str:
    for a in areas:  # an exact id wins over any pattern
        if d["id"] in a.get("decision_ids", []):
            return a["id"]
    for a in areas:
        if any(re.search(p, d["id"]) for p in a["decisions"]):
            return a["id"]
    dom = dec_dom.get(d["id"])
    if dom and dom in dom_area:
        return dom_area[dom]
    return "unassigned"


def build_model(repo: Path, coord: Path, design_dirs: list[Path], now: dt.datetime, run: Runner = default_runner,
                builds_dir: Path | None = None, receipt_dirs: list[Path] | None = None, github: bool = False, gate_dir: Path | None = None, stale_hours: float = STALE_HOURS) -> dict[str, Any]:
    src = Sources()
    reqs = load_requirements(repo, src)
    decs_list = load_decisions(repo, src)
    domains = load_domains(repo, src)
    claims = load_claims(coord, src)
    worktrees = load_branches(coord, src)
    phases, task_rows = load_roadmap(repo, src)
    docs = load_docs(repo, design_dirs, src)
    git = load_git(repo, run, src)
    prs = load_github(repo, run, src) if github else []
    if not github:
        src.skipped("GitHub pull requests", "not requested (use --github); the map does not need it")
    gate = load_gate_log(gate_dir, src)

    decs = {d["id"]: d for d in decs_list}
    areas = [dict(a) for a in AREAS]
    area_by_id = {a["id"]: a for a in areas}
    dom_area = {dom: a["id"] for a in areas for dom in a["domains"]}
    dec_dom = {did: dom for dom, st in domains.items() for did in as_list(st.get("decision_ids"))}
    req_dec_count: dict[str, int] = {}
    for q in reqs:
        for d in q["decisions"]:
            req_dec_count[d] = req_dec_count.get(d, 0) + 1

    buckets: dict[str, dict[str, list[dict[str, Any]]]] = {}

    def add(area: str, group: str, leaf: dict[str, Any]) -> None:
        buckets.setdefault(area, {}).setdefault(group, []).append(leaf)

    details: dict[str, dict[str, Any]] = {}
    root_path = str(repo)

    # Requirements ---------------------------------------------------------------------------------------------------
    for q in sorted(reqs, key=lambda x: (ms_rank(x["milestone"]), prio_rank(x["priority"]), x["id"])):
        status, why, ex = derive_requirement(q, repo, claims, decs)
        area = area_for_requirement(q, areas)
        nid = "req:" + q["id"]
        label = f"{q['id']} {q['title']}".strip()
        det = {"kind": "requirement", "status": status, "why": why, "title": label,
               "fields": {"priority": q["priority"], "milestone": q["milestone"], "module": q["module"], "required evidence": "/".join(q["required"]),
                          "new_build_status": q["build_status"] or "(empty)", "product needs": ", ".join(q["needs"])},
               "statement": q["statement"], "files": [{"path": r, "ok": r in ex["impl_ok"]} for r in q["impl"]],
               "design": [{"path": r, "ok": exists(repo, r)} for r in q["design"] + q["contracts"] + q["spec"]],
               "evidence": q["evidence"], "claims": ex["claims"],
               "decisions": [{"id": d, "status": decs[d]["status"] if d in decs else "not in decisions.json",
                              "question": decision_question(decs[d]) if d in decs else "", "owner": decs[d].get("owner_role", "") if d in decs else ""}
                             for d in q["decisions"]], "notes": []}
        if ex["unknown_decs"]:
            det["notes"].append("open_decisions names ids not found in contracts/decisions.json: " + ", ".join(ex["unknown_decs"]))
        details[nid] = det
        group = q["module"] or "other"
        add(area, "req:" + group, {"i": nid, "t": label, "k": "requirement", "s": status, "p": q["priority"], "m": q["milestone"], "rollup": True,
                                   "a": 1 if ex["claims"] and status == "built" else 0})

    # Decisions ------------------------------------------------------------------------------------------------------
    for d in sorted(decs_list, key=lambda x: (ms_rank(str(x.get("milestone", "")).split("-")[0]), x["id"])):
        area = area_for_decision(d, areas, dom_area, dec_dom)
        a_cfg = next((a for a in areas if a["id"] == area), UNASSIGNED)
        decided_shown = d.get("status") == "decided" and area != "unassigned" and not a_cfg["modules"] and not a_cfg["requirements"]  # areas without RTM rows: decisions are their structure
        if d.get("status") not in PENDING_DEC | {"deferred"} and not decided_shown:
            continue
        owner = bool(OWNER_PAT.search(str(d.get("owner_role", ""))))
        if decided_shown:
            status, why = "designed", f"decided by the owner on {d.get('decided_on', 'an unrecorded date')}; a decision, nothing is built from it yet"
        elif d["status"] in PENDING_DEC and owner:
            status, why = "blocked", f"decision is {d['status']} and its owner role is {d.get('owner_role')}: only the owner can close it"
        elif d["status"] == "deferred":
            status, why = "notstarted", "deferred: the owner chose not to decide now"
        else:
            status, why = "notstarted", f"decision is {d['status']}; owner role is {d.get('owner_role')}: an agent or engineer can resolve it"
        nid = "dec:" + d["id"]
        details[nid] = {"kind": "decision", "status": status, "why": why, "title": d["id"],
                        "fields": {"decision status": d["status"], "owner role": str(d.get("owner_role", "")), "milestone": str(d.get("milestone", "")),
                                   "requirements waiting": str(req_dec_count.get(d["id"], 0))},
                        "statement": decision_question(d), "closure": str(d.get("closure_evidence", "")), "safe_interim": str(d.get("safe_interim", "")),
                        "files": [], "design": [], "evidence": [], "claims": [], "decisions": [], "notes": []}
        add(area, "dec", {"i": nid, "t": d["id"] + " " + str(d.get("blocks", ""))[:90], "k": "decision", "s": status, "p": "", "m": str(d.get("milestone", "")), "rollup": True})

    # Backlog tasks ----------------------------------------------------------------------------------------------------
    for n, row in enumerate(task_rows):
        area = "unassigned"
        for a in areas:
            if any(re.search(p, row["task"]) for p in a["tasks"]):
                area = a["id"]
                break
        status, why = derive_task(row)
        nid = f"task:{n:03d}-{slug(row['task'])}"
        details[nid] = {"kind": "delivered task", "status": status, "why": why, "title": row["task"],
                        "fields": {"backlog status": row["status"], "commit": row["commit"]}, "statement": row["note"], "files": [], "design": [],
                        "evidence": [], "claims": [], "decisions": [], "notes": ["Source: docs/delivery/SPRINT-ROADMAP.md, Backlog status table"]}
        add(area, "task", {"i": nid, "t": row["task"], "k": "task", "s": status, "p": "", "m": "", "rollup": True})

    # Documents ------------------------------------------------------------------------------------------------------
    by_stem = {a_doc: a["id"] for a in areas for a_doc in a["docs"]}
    unmapped_docs = []
    seen_paths: set[str] = set()
    stem_n: dict[str, int] = {}
    repo_stems = {x["stem"] for x in docs if not x["external"]}
    for dcm in docs:
        if dcm["path"] in seen_paths or (dcm["external"] and dcm["stem"] in repo_stems):  # a draft copy outside the repository yields to the repository document
            continue
        seen_paths.add(dcm["path"])
        aid = by_stem.get(dcm["stem"])
        if not aid:
            unmapped_docs.append(dcm["stem"])
            continue
        stem_n[dcm["stem"]] = stem_n.get(dcm["stem"], 0) + 1
        nid = "doc:" + dcm["stem"] + (f"~{stem_n[dcm['stem']]}" if stem_n[dcm["stem"]] > 1 else "")
        details[nid] = {"kind": "document", "status": "designed", "why": "the document exists" + (f"; its status line: {dcm['status']}" if dcm["status"] else ""),
                        "title": dcm["title"], "fields": {"stem": dcm["stem"], "outside the repository": "yes" if dcm["external"] else "no"},
                        "statement": "", "files": [{"path": dcm["path"], "ok": True, "abs": True}], "design": [], "evidence": [], "claims": [], "decisions": [], "notes": []}
        add(aid, "doc", {"i": nid, "t": dcm["title"], "k": "document", "s": "designed", "p": "", "m": "", "rollup": False})

    # Active claims --------------------------------------------------------------------------------------------------
    active = [c for c in claims if claim_active(c)]
    for c in active:
        area = dom_area.get(str(c.get("domain", "")), "unassigned")
        nid = "claim:" + c["_name"]
        expired = claim_expired(c, now)
        details[nid] = {"kind": "active claim", "status": "progress", "why": "an active claim in the coordination ledger" + ("; its expiry has passed, but an expired claim does not free its area" if expired else ""),
                        "title": c["_name"], "fields": {"domain": str(c.get("domain", "")), "task": str(c.get("task", "")), "branch": str(c.get("branch", "")),
                                                         "holder": json.dumps(c.get("holder", c.get("model", "")), ensure_ascii=False)[:120], "started": str(c.get("started", "")),
                                                         "expires": str(c.get("expires", ""))},
                        "statement": str(c.get("note", "")), "files": [{"path": str(w), "ok": True, "abs": False} for w in as_list(c.get("write_area") or c.get("write_areas"))],
                        "design": [], "evidence": [], "claims": [], "decisions": [], "notes": [f"worktree: {c.get('worktree', '')}"]}
        add(area, "claim", {"i": nid, "t": f"{c.get('task', c['_name'])} ({c.get('branch', '?')})" + (" [expired]" if expired else ""), "k": "claim", "s": "progress", "p": "", "m": "", "rollup": True})

    # Card items -----------------------------------------------------------------------------------------------------
    for dom, st in sorted(domains.items()):
        aid = dom_area.get(dom, "unassigned")
        dom_active = any(claim_active(c) and c.get("domain") == dom for c in claims)
        for n, item in enumerate(st.get("_card_items", [])):
            k = item["kind"]
            owner_dec = [t for t in re.findall(r"DEC-[A-Z0-9-]+", item["text"]) if t in decs and decision_pending_owner(decs[t])]
            if k == "NOW":
                status, why = "unknown", "a card statement of the current state, not a measured status" + ("; an active claim exists in this domain" if dom_active else "; no active claim exists in this domain, so the card may be stale")
            elif k == "NEXT":
                status, why = "notstarted", "card lists it as next"
            elif k == "BLOCKED":
                status, why = ("blocked", "card says blocked and names an open owner decision: " + ", ".join(owner_dec)) if owner_dec else ("notstarted", "card says blocked, not on an open owner decision")
            else:
                status, why = "unknown", "card says this is not verified"
            nid = f"card:{dom}:{n:02d}"
            details[nid] = {"kind": "card " + k.lower(), "status": status, "why": why, "title": f"{dom} card: {k}",
                            "fields": {"domain": dom, "card verified on": str(st.get("verified_on", "")), "card verified at commit": str(st.get("verified_at_commit", ""))},
                            "statement": item["text"], "files": [{"path": f"docs/domains/{dom}/CARD.md", "ok": True}], "design": [], "evidence": [], "claims": [], "decisions": [],
                            "notes": ["A card is a map, not evidence; it can be stale."]}
            add(aid, "card", {"i": nid, "t": f"{k}: {item['text'][:90]}", "k": "card", "s": status, "p": "", "m": "", "rollup": False})

    # Tree -----------------------------------------------------------------------------------------------------------
    area_list = [a for a in areas] + ([UNASSIGNED] if buckets.get("unassigned") else [])
    GROUP_ORDER = {"claim": 0, "task": 5, "dec": 6, "doc": 7, "card": 8}
    GROUP_LABEL = {"claim": ("งานที่กำลังทำ", "Active work"), "task": ("งานที่ส่งมอบแล้ว", "Delivered tasks"), "dec": ("การตัดสินใจ", "Decisions"),
                   "doc": ("เอกสารออกแบบ", "Design documents"), "card": ("จากการ์ดโดเมน", "Domain card items")}
    root_children: list[dict[str, Any]] = []
    totals = {s["id"]: 0 for s in STATUSES}
    req_totals = {s["id"]: 0 for s in STATUSES}
    for a in area_list:
        groups = buckets.get(a["id"], {})
        has_work = any(g.startswith("req:") or g == "task" for g in groups)
        gnodes: list[dict[str, Any]] = []
        for gname in sorted(groups, key=lambda g: (GROUP_ORDER.get(g, 1 if g.startswith("req:") else 9), g)):
            leaves = groups[gname]
            for lf in leaves:
                if lf["k"] == "document":
                    lf["rollup"] = not has_work
            if gname.startswith("req:"):
                mod = gname[4:]
                th, en = MODULE_LABELS.get(mod, (mod, mod))
                label = f"{th} [{mod}]"
            else:
                th, en = GROUP_LABEL.get(gname, (gname, gname))
                label = f"{th} ({en})"
            gnodes.append(make_node(f"grp:{a['id']}:{gname}", label, "group", leaves))
        anode = make_node("area:" + a["id"], f"{a['th']} [{a['id']}]", "area", gnodes)
        anode["en"] = a["en"]
        if not sum(anode["n"].values()) and any(c.get("domain") in a["domains"] for c in active):
            anode["s"] = "progress"  # nothing countable, but an active claim exists in its domain
        # active claim badge and the domains the area reads
        adom = a["domains"]
        act = [c["_name"] for c in active if c.get("domain") in adom]
        anode["claims"] = len(act)
        details[anode["i"]] = {"kind": "area", "status": anode["s"], "why": "roll-up of the leaves below: the most common status; on a tie the less advanced one (see PROGRESS-MAP.md)",
                               "title": f"{a['th']} [{a['id']}] / {a['en']}", "fields": {"domains": ", ".join(adom) or "(none: no domain card)",
                                                                                    "requirement modules": ", ".join(a["modules"]) or "(none: no RTM rows)",
                                                                                    "active claims": ", ".join(act) or "(none)"},
                               "statement": "", "files": [], "design": [], "evidence": [], "claims": act, "decisions": [], "notes": [] }
        if not any(g.startswith("req:") for g in groups):
            details[anode["i"]]["notes"].append("No requirement rows (RTM) exist for this area; its status comes from decisions, documents, tasks and cards only.")
        root_children.append(anode)
    root = make_node("root", "Kohbor / เกาะเบาะ", "root", root_children)
    for lf in iter_leaves(root):
        if lf.get("rollup", True):
            totals[lf["s"]] += 1
            if lf["k"] == "requirement":
                req_totals[lf["s"]] += 1

    # What next and needs the owner ----------------------------------------------------------------------------------
    req_nodes = [lf for lf in iter_leaves(root) if lf["k"] == "requirement" and lf["s"] != "verified"]
    req_nodes.sort(key=lambda lf: (prio_rank(lf["p"]), ms_rank(lf["m"]), 1 if lf["s"] == "blocked" else 0, lf["t"]))
    nxt = [{"i": lf["i"], "t": lf["t"], "s": lf["s"], "p": lf["p"], "m": lf["m"]} for lf in req_nodes]
    area_of = {lf["i"]: a["t"] for a in root["c"] for lf in iter_leaves(a)}
    owner_list = []
    for d in decs_list:
        if decision_pending_owner(d):
            nid = "dec:" + d["id"]
            owner_list.append({"i": nid, "id": d["id"], "status": d["status"], "milestone": str(d.get("milestone", "")), "reqs": req_dec_count.get(d["id"], 0),
                               "question": decision_question(d), "closure": str(d.get("closure_evidence", "")), "area": area_of.get(nid, "")})
    owner_list.sort(key=lambda x: (ms_rank(x["milestone"].split("-")[0]), -x["reqs"], x["id"]))

    # Milestones -----------------------------------------------------------------------------------------------------
    ms: dict[str, dict[str, Any]] = {}
    for lf in iter_leaves(root):
        if lf["k"] == "requirement" and lf["m"]:
            e = ms.setdefault(lf["m"], {"id": lf["m"], "n": {s["id"]: 0 for s in STATUSES}, "outcome": ""})
            e["n"][lf["s"]] += 1
    for ph in phases:
        for m in re.findall(r"M\d+[ab]?", ph["milestone"]):
            if m in ms and not ms[m]["outcome"]:
                ms[m]["outcome"] = ph["outcome"]
    milestones = sorted(ms.values(), key=lambda e: ms_rank(e["id"]))

    # Views for the dashboard (all derived from the data above) ---------------------------------------------------------
    area_label = {a["i"]: a["t"] for a in root["c"]}
    decisions_view = []
    for d in sorted(decs_list, key=lambda x: (0 if decision_pending_owner(x) else 1, ms_rank(str(x.get("milestone", "")).split("-")[0]), x["id"])):
        aid = area_for_decision(d, areas, dom_area, dec_dom)
        decisions_view.append({
            "id": d["id"], "status": d.get("status", ""), "owner": str(d.get("owner_role", "")), "owner_only": bool(OWNER_PAT.search(str(d.get("owner_role", "")))),
            "pending": d.get("status") in PENDING_DEC, "milestone": str(d.get("milestone", "")), "area": area_label.get("area:" + aid, aid),
            "blocks": str(d.get("blocks", "")), "question": decision_question(d), "closure": str(d.get("closure_evidence", "")),
            "interim": str(d.get("safe_interim", "")), "decided_on": str(d.get("decided_on", "")), "reqs": req_dec_count.get(d["id"], 0),
            "node": "dec:" + d["id"] if ("dec:" + d["id"]) in details else ""})
    order = {"active": 0, "parked": 1}
    claims_view = []
    for c in sorted(claims, key=lambda x: (order.get(str(x.get("status", "")).lower(), 2), str(x.get("started", "")), x["_name"]), reverse=False):
        holder = c.get("holder") if isinstance(c.get("holder"), dict) else {}
        left = None
        try:
            if claim_active(c):
                left = round((dt.datetime.fromisoformat(str(c.get("expires", ""))) - now).total_seconds() / 3600.0, 1)
        except ValueError:
            left = None
        claims_view.append({"name": c["_name"], "domain": str(c.get("domain", "")), "task": str(c.get("task", "")), "status": str(c.get("status", "")),
                            "branch": str(c.get("branch", "")), "model": str(c.get("model") or holder.get("model", "")), "role": str(holder.get("role", "")),
                            "session": str((c.get("session") or {}).get("name", "") if isinstance(c.get("session"), dict) else c.get("session", "")),
                            "started": str(c.get("started", "")), "expires": str(c.get("expires", "")), "hours_left": left,
                            "expired": claim_expired(c, now) and claim_active(c), "worktree": str(c.get("worktree") or ""), "note": str(c.get("note", ""))[:240]})
    done = [c for c in claims_view if c["status"].lower() not in ("active", "parked")]
    live = [c for c in claims_view if c["status"].lower() in ("active", "parked")]
    done.sort(key=lambda c: c["started"], reverse=True)
    claims_view = live + done[:12]

    board: dict[str, list[dict[str, Any]]] = {s["id"]: [] for s in STATUSES}
    for a in root["c"]:
        per: dict[str, list[dict[str, Any]]] = {}
        for lf in iter_leaves(a):
            if lf.get("rollup", True):
                per.setdefault(lf["s"], []).append(lf)
        for st_id, items in per.items():
            items.sort(key=lambda lf: (prio_rank(lf["p"]), ms_rank(lf["m"]), lf["t"]))
            board[st_id].append({"area": a["i"], "t": a["t"], "n": len(items), "p0": sum(1 for lf in items if lf["p"] == "P0"),
                                 "items": [{"i": lf["i"], "t": lf["t"], "p": lf["p"], "m": lf["m"]} for lf in items[:4]]})
    for col in board.values():
        col.sort(key=lambda c: (-c["n"], c["t"]))

    area_bars = [{"i": a["i"], "t": a["t"], "en": a.get("en", ""), "n": a["n"], "claims": a.get("claims", 0)} for a in root["c"]]
    areas_in_progress = sum(1 for a in root["c"] if a.get("claims"))
    req_total = sum(req_totals.values())
    stats = {"req_total": req_total, "req_verified": req_totals["verified"], "req_built": req_totals["built"], "areas_total": len(root["c"]),
             "areas_in_progress": areas_in_progress, "owner_needed": len(owner_list), "claims_active": len(active),
             "claims_expired": sum(1 for c in active if claim_expired(c, now)), "decisions_total": len(decs_list)}
    per_day: dict[str, int] = {}
    for m in git.get("merged", []):
        day = m["time"][:10]
        per_day[day] = per_day.get(day, 0) + 1
    activity = [{"day": k, "n": v} for k, v in sorted(per_day.items())]
    builds_view = scan_builds(builds_dir, load_receipts(receipt_dirs or [], src), repo, run, src)

    for n in iter_nodes(root):  # the counting flag is internal; the page never needs it
        n.pop("rollup", None)

    generated = now.isoformat(timespec="seconds")
    newest = max([t for t in (git.get("head_time"), git.get("origin_time")) if t] or [""])
    model = {
        "generated_at": generated, "head_sha": git.get("sha", ""), "stale_hours": stale_hours, "repo": root_path, "statuses": STATUSES, "tree": root, "details": details,
        "totals": totals, "req_totals": req_totals, "next": nxt, "owner": owner_list, "milestones": milestones,
        "git": git, "prs": prs, "gate": gate, "sources": src.items, "unmapped_docs": sorted(set(unmapped_docs)),
        "worktrees": [{"path": w.get("path", ""), "branch": w.get("branch", ""), "state": w.get("state", ""), "claim": w.get("owner_claim", "")} for w in worktrees],
        "newest_commit_at": newest, "builds": builds_view, "decisions": decisions_view, "claims": claims_view, "board": board, "area_bars": area_bars,
        "stats": stats, "activity": activity,
        "active_claims": [{"name": c["_name"], "domain": c.get("domain", ""), "task": c.get("task", ""), "branch": c.get("branch", ""), "expired": claim_expired(c, now)} for c in active],
    }
    return model


def make_node(nid: str, label: str, kind: str, children: list[dict[str, Any]]) -> dict[str, Any]:
    """A group, area or root node. Leaf dicts get their own one-item count so every roll-up is uniform."""
    counts = {s["id"]: 0 for s in STATUSES}
    for ch in children:
        if "n" not in ch:
            ch["n"] = {s["id"]: 0 for s in STATUSES}
            if ch.get("rollup", True):
                ch["n"][ch["s"]] = 1
        for s, n in ch["n"].items():
            counts[s] += n
    node: dict[str, Any] = {"i": nid, "t": label, "k": kind, "c": children, "n": counts, "rollup": True}
    node["s"] = rollup({s: n for s, n in counts.items() if n})
    if not any(counts.values()) and children:  # only uncounted leaves (documents, claims, card items): colour by them, count nothing
        shown: dict[str, int] = {}
        for ch in children:
            shown[ch["s"]] = shown.get(ch["s"], 0) + 1
        node["s"] = rollup(shown)
    return node


def iter_nodes(n: dict[str, Any]):
    yield n
    for c in n.get("c", []):
        yield from iter_nodes(c)


def iter_leaves(n: dict[str, Any]):
    if "c" in n and n["k"] in ("root", "area", "group"):
        for c in n["c"]:
            yield from iter_leaves(c)
    else:
        yield n


# ---------------------------------------------------------------------------------------------------------------
# HTML
# ---------------------------------------------------------------------------------------------------------------
TEMPLATE_PATH = Path(__file__).with_name("progress_map_template.html")


def safe_json(obj: Any) -> str:
    text = json.dumps(obj, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return text.replace("</", "<\\/").replace("\u2028", "\\u2028").replace("\u2029", "\\u2029").replace("<!--", "<\\!--")


def render_html(model: dict[str, Any]) -> str:
    template = TEMPLATE_PATH.read_text(encoding="utf-8")
    return template.replace("/*__DATA__*/", safe_json(model))




# ---------------------------------------------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------------------------------------------
def parse_now(value: str | None) -> dt.datetime:
    if value:
        t = dt.datetime.fromisoformat(value)
        return t if t.tzinfo else t.astimezone()
    return dt.datetime.now().astimezone()


def write_atomic(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(text, encoding="utf-8", newline="\n")
    os.replace(tmp, path)


def embedded_sha(path: Path) -> str | None:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return None
    m = re.search(r'"head_sha":"([0-9a-f]+)"', text)
    return m.group(1) if m else None


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Generate the Kohbor progress map (one self-contained HTML file).")
    ap.add_argument("--repo", default=str(Path(__file__).resolve().parents[1]))
    ap.add_argument("--coord", default=os.environ.get("KOHBOR_COORD_DIR", DEFAULT_COORD))
    ap.add_argument("--design-dir", action="append", default=None,
                    help="extra design folder (repeatable); default: KOHBOR_DESIGN_DIR or F:\\kohbor-design when it exists")
    ap.add_argument("--builds-dir", default=os.environ.get("KOHBOR_BUILDS_DIR", DEFAULT_BUILDS),
                    help="folder of owner builds (every .exe below it is hashed and matched to release receipts); default KOHBOR_BUILDS_DIR or F:\\kohbor-local")
    ap.add_argument("--out", default=DEFAULT_OUT)
    ap.add_argument("--open", action="store_true", help="open the file in the default browser")
    ap.add_argument("--github", action="store_true", help="also read pull requests with the gh CLI (optional, needs network)")
    ap.add_argument("--now", help="ISO 8601 time to stamp (tests and reproducible runs)")
    ap.add_argument("--stale-hours", type=float, default=STALE_HOURS)
    ap.add_argument("--check", action="store_true", help="do not write: exit 1 when the existing file is missing or built from another commit")
    ap.add_argument("--json", action="store_true", help="print the model as JSON instead of writing HTML")
    args = ap.parse_args(argv)

    repo = Path(args.repo).resolve()
    design = args.design_dir
    if design is None:
        env = os.environ.get("KOHBOR_DESIGN_DIR", DEFAULT_DESIGN)
        base = Path(env)
        design = [str(p) for p in sorted(base.glob("*")) if p.is_dir()] if base.is_dir() else []
    model = build_model(repo, Path(args.coord), [Path(d) for d in design], parse_now(args.now), builds_dir=Path(args.builds_dir), receipt_dirs=[Path(d) for d in design],
                        github=args.github, stale_hours=args.stale_hours)
    out = Path(args.out)
    if args.json:
        print(json.dumps(model, ensure_ascii=False, indent=1, sort_keys=True))
        return 0
    if args.check:
        have = embedded_sha(out)
        sha = model["git"].get("sha")
        if have is None:
            print(f"progress_map: {out} is missing or unreadable")
            return 1
        if sha and have != sha:
            print(f"progress_map: STALE: {out} was built from {have}, HEAD is {sha}")
            return 1
        print(f"progress_map: {out} matches HEAD {sha}")
        return 0
    write_atomic(out, render_html(model))
    t = model["totals"]
    print(f"progress_map: wrote {out} ({out.stat().st_size} bytes) from {model['git'].get('sha', 'no git')}; "
          + ", ".join(f"{s['id']} {t[s['id']]}" for s in STATUSES))
    missing = [s for s in model["sources"] if s["status"] in ("missing", "error")]
    for s in missing:
        print(f"progress_map: not read: {s['name']}: {s['note']}")
    if args.open:
        webbrowser.open(out.as_uri())
    return 0


if __name__ == "__main__":
    sys.exit(main())
