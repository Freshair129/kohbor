#!/usr/bin/env python3
"""Check the agent registry (docs/governance/AGENT-REGISTRY.json, schema 2.0.0) and answer routing questions.

REPORT-ONLY by default: problems are printed and the exit code stays 0, because turning a new check into a gate
failure is reserved for the owner. ``--enforce`` exits 1 on any problem.

Usage:
  python tools/validate_agents.py                                  check and report
  python tools/validate_agents.py --route implement --domain packs which role, model and sandbox does this go to
  python tools/validate_agents.py --brief FILE                     lint a handoff brief against its route (exit 1 on a mismatch)
  python tools/validate_agents.py --launch FILE                    print the codex exec command for a brief; never runs it
  python tools/validate_agents.py --write-doc                      refresh the generated tables in AGENT-REGISTRY.md
  (the taxonomy map is python tools/validate_domains.py --map)

Approval belongs to a route: the effective level of an answer is the weaker of the route's level and the profile's
level; only ``owner_chosen`` is usable without an owner instruction. Unknown is a state: a reviewer whose model is not
concrete is reported UNVERIFIABLE, not as a pass. Source-class evidence about documents only.
"""

from __future__ import annotations

import argparse
import json
import re
import shlex
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import validate_domains as vd  # noqa: E402

REGISTRY = "docs/governance/AGENT-REGISTRY.json"
DOC = "docs/governance/AGENT-REGISTRY.md"
LEVELS = ["excluded", "proposed", "in_use_unconfirmed", "owner_chosen"]
TOOLS = {"claude-code-session", "claude-code-subagent", "codex-exec", "t3-thread", "local-model"}
AGENT_WORK = {"normal", "design-only", "questions-only", "owner-only", "redirect"}
BEGIN = "<!-- GENERATED:agent-registry (python tools/validate_agents.py --write-doc) -->"
END = "<!-- /GENERATED:agent-registry -->"
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
ANSWERS_RE = re.compile(r"answers:\s*`([a-z][a-z0-9-]*)`")


def load(root: Path) -> dict:
    return json.loads((root / REGISTRY).read_text(encoding="utf-8-sig"))


def concrete(model: str | None) -> bool:
    return bool(model) and model != "session"


def level_index(level: str) -> int:
    return LEVELS.index(level) if level in LEVELS else -1


def domain_info(root: Path) -> dict[str, dict]:
    """Domain id -> tier, the domain that answers for it (tier C) and whether its write areas are an external root."""
    out: dict[str, dict] = {}
    for dom, row in vd.parse_registry(root, vd.Report()).items():
        m = ANSWERS_RE.search(row.get("answers", ""))
        external = False
        state = root / "docs" / "domains" / dom / "state.json"
        if state.is_file():
            try:
                areas = json.loads(state.read_text(encoding="utf-8-sig")).get("write_areas", [])
                external = bool(areas) and all(vd.LOGICAL_ROOT_RE.match(a) for a in areas)
            except ValueError:
                pass
        out[dom] = {"tier": row["tier"], "answers_for": m.group(1) if m else None, "external": external}
    return out


def evidence_ok(e) -> bool:
    """Complete evidence: a known level, a non-empty string source, a date and a boolean 'relayed'."""
    return (isinstance(e, dict) and e.get("level") in LEVELS and isinstance(e.get("source"), str) and bool(e["source"].strip())
            and isinstance(e.get("date"), str) and bool(DATE_RE.match(e["date"])) and isinstance(e.get("relayed"), bool))


def _find(items: list[dict], key: str, value) -> dict | None:
    return next((i for i in items if i.get(key) == value), None)


def redirect_chain(domains: dict[str, dict], domain: str) -> tuple[list[str], str | None]:
    """Follow tier C 'answers for' links to the end. Returns (chain, error); a cycle or a dangling target is an error."""
    chain = [domain]
    while True:
        info = domains.get(chain[-1])
        if info is None:
            return chain, f"redirect target {chain[-1]!r} is not a domain"
        nxt = info.get("answers_for") if info["tier"] == "C" else None
        if not nxt:
            return chain, None
        if nxt in chain:
            return chain, f"redirect cycle: {' -> '.join(chain + [nxt])}"
        chain.append(nxt)


def resolve(reg: dict, domains: dict[str, dict], task_type: str, domain: str | None) -> dict:
    """Return the routing answer for one task type and an optional domain. Pure: no file access.

    The policies and exclusions of EVERY domain on the redirect chain apply; the last domain of the chain is the answering domain.
    """
    ans: dict = {"task_type": task_type, "domain": domain}
    reserved = reg.get("owner_reserved", {})
    if task_type in reserved.get("ids", []):
        ans.update(status="owner-only", role="owner", usable=False, level="owner_chosen",
                   message=reserved.get("meaning", {}).get(task_type, "Reserved to the owner"))
        return ans
    tt = _find(reg["task_types"], "id", task_type)
    if tt is None:
        ans.update(status="error", message=f"unknown task type {task_type!r}", usable=False)
        return ans
    chain: list[str] = []
    answering = domain
    if domain is not None:
        if domain not in domains:
            ans.update(status="error", message=f"unknown domain {domain!r}", usable=False)
            return ans
        chain, err = redirect_chain(domains, domain)
        if err:
            ans.update(status="error", message=err, usable=False)
            return ans
        answering = chain[-1]
        if len(chain) > 1:
            ans["redirected_from"] = domain
            ans["redirect_chain"] = chain
    ans["answering_domain"] = answering
    notes: list[str] = []
    for dom in chain:
        for pol in reg.get("domain_policies", []):
            if pol["domain"] == dom and pol.get("notes"):
                notes.append(f"{dom}: {pol['notes']}")
    ans["notes"] = sorted(set(notes))
    for dom in chain:
        policy = _find(reg.get("domain_policies", []), "domain", dom)
        if policy and policy["agent_work"] != "redirect":
            allowed = policy.get("allowed_task_types", [])
            if policy["agent_work"] == "owner-only" or (allowed and task_type not in allowed) or (not allowed and policy["agent_work"] != "normal"):
                ans.update(status="blocked-by-policy", usable=False, role="owner",
                           message=f"domain {dom} policy {policy['agent_work']} ({policy['source']}) does not allow {task_type}; allowed: {', '.join(allowed) or 'none'}")
                return ans
    routes = [r for r in reg["routes"] if r["task_type"] == task_type and (r["domain"] == "*" or r["domain"] == answering)]
    routes.sort(key=lambda r: r["domain"] == "*")
    if not routes:
        ans.update(status="no-route", usable=False, message=f"no route for {task_type} in {answering or 'any domain'}: ask the owner")
        return ans
    route = routes[0]
    role = _find(reg["roles"], "id", route.get("role"))
    if role is None:
        ans.update(status="error", usable=False, message=f"route role {route.get('role')!r} is not defined")
        return ans
    profile = _find(reg["profiles"], "id", route.get("profile")) if route.get("profile") else None
    r_ev = route.get("evidence") if isinstance(route.get("evidence"), dict) else {}
    p_ev = (profile or {}).get("evidence") if isinstance((profile or {}).get("evidence"), dict) else {}
    level = r_ev.get("level") if r_ev.get("level") in LEVELS else "proposed"
    if profile and level_index(p_ev.get("level", "proposed")) < level_index(level):
        level = p_ev.get("level", "proposed")
    excluded = next((x for x in reg.get("exclusions", []) if x["profile"] == route.get("profile") and x["task_type"] == task_type
                     and (x.get("domain") in (None, "*") or x.get("domain") in chain)), None)
    if excluded:
        level = "excluded"
        ans["notes"] = ans["notes"] + [f"excluded: {excluded['source']}"]
    if role.get("kind") == "human":
        sandbox = "n/a"
    elif not tt["writes"]:
        sandbox = "read-only"
    elif answering and domains.get(answering, {}).get("external"):
        sandbox = "workspace-write inside one lab folder"
    else:
        sandbox = "workspace-write"
    ans.update(status="ok", role=route["role"], profile=route.get("profile"), tool=(profile or {}).get("tool"),
               model=(profile or {}).get("model"), effort=(profile or {}).get("effort"), writes=tt["writes"], sandbox=sandbox,
               level=level, relayed=bool(r_ev.get("relayed") or p_ev.get("relayed")),
               usable=level == "owner_chosen", pattern=tt.get("pattern"), evidence_source=r_ev.get("source"))
    problems = []
    if not evidence_ok(route.get("evidence")):
        problems.append("the route's evidence is incomplete")
    if route.get("profile") and (profile is None or not evidence_ok(profile.get("evidence"))):
        problems.append("the profile is undefined or its evidence is incomplete")
    if role.get("kind") == "agent" and not route.get("profile") and level != "proposed":
        problems.append("an agent route has no profile")
    if problems:  # a route is never usable on evidence that is missing, malformed or refers to an undefined profile
        ans["usable"] = False
        ans["notes"] = ans["notes"] + ["not usable without the owner: " + "; ".join(problems)]
    if route.get("scope_note"):
        ans["notes"] = ans["notes"] + [route["scope_note"]]
    if profile and profile.get("note"):
        ans["notes"] = ans["notes"] + [f"profile {profile['id']}: {profile['note']}"]
    if not ans["usable"]:
        ans["message"] = f"level {level}: needs an owner instruction before use"
    rev = route.get("reviewer")
    if rev:
        rprofile = _find(reg["profiles"], "id", rev.get("profile")) or {}
        if rev["role"] == route["role"]:
            sep = "same role"
        elif not concrete(rprofile.get("model")) or not concrete((profile or {}).get("model")):
            sep = "UNVERIFIABLE (a model is not concrete)"
        elif rprofile.get("model") == (profile or {}).get("model"):
            sep = "same model"
        else:
            sep = "differs"
        ans["reviewer"] = {"role": rev["role"], "profile": rev.get("profile"), "model": rprofile.get("model"), "effort": rprofile.get("effort"), "model_separation": sep}
    if route.get("alternatives"):
        ans["alternatives"] = route["alternatives"]
    return ans


BRIEF_KEY_RE = re.compile(r"^([A-Z][A-Z_]+):[ \t]*(.*)$")
BRIEF_MODEL_RE = re.compile(r"^\s*([A-Za-z0-9._-]+)\s*(?:\(\s*([A-Za-z0-9_-]+)\s*\))?\s*$")


def parse_brief(text: str) -> dict[str, str]:
    """Read the KEY: value lines of a handoff brief (Agent Workflow section 4). The first occurrence of a key wins."""
    out: dict[str, str] = {}
    for line in text.splitlines():
        m = BRIEF_KEY_RE.match(line)
        if m and m.group(1) not in out:
            out[m.group(1)] = m.group(2).strip()
    return out


def lint_brief(reg: dict, domains: dict[str, dict], brief: dict[str, str]) -> tuple[list[str], dict | None]:
    """Compare ROLE, TASK_TYPE, DOMAIN, MODEL and SCOPE of a brief with the route. Returns (problems, route answer)."""
    problems: list[str] = []
    for key in ("ROLE", "TASK_TYPE", "DOMAIN", "MODEL"):
        if not brief.get(key):
            problems.append(f"the brief has no {key} line, so it cannot be checked against a route")
    if problems:
        return problems, None
    ans = resolve(reg, domains, brief["TASK_TYPE"], brief["DOMAIN"])
    if ans["status"] != "ok":
        return [f"route for {brief['TASK_TYPE']} in {brief['DOMAIN']} is {ans['status']}: {ans.get('message', '')}"], ans
    expected = [(ans["role"], ans["profile"], ans["model"], ans["effort"], "the route")]
    for alt in ans.get("alternatives", []):
        prof = _find(reg["profiles"], "id", alt["profile"]) or {}
        expected.append((alt["role"], alt["profile"], prof.get("model"), prof.get("effort"), "an alternative of the route"))
    role_matches = [e for e in expected if e[0] == brief["ROLE"]]
    if not role_matches:
        problems.append(f"ROLE {brief['ROLE']} does not match the route {ans['role']} (profile {ans['profile']}) for {brief['TASK_TYPE']} in {brief['DOMAIN']}")
        return problems, ans
    role, profile_id, model, effort, where = role_matches[0]
    m = BRIEF_MODEL_RE.match(brief["MODEL"])
    if not m:
        problems.append(f"MODEL {brief['MODEL']!r} must name exactly one model, for example 'gpt-6-luna (max)'")
        return problems, ans
    got_model, got_effort = m.group(1), m.group(2)
    if concrete(model) and got_model.lower() != str(model).lower():
        problems.append(f"MODEL {got_model} differs from {model} of {where} (profile {profile_id})")
    if effort and got_effort and got_effort.lower() != str(effort).lower():
        problems.append(f"MODEL effort {got_effort} differs from {effort} of {where} (profile {profile_id})")
    if effort and not got_effort:
        problems.append(f"MODEL does not state the effort {effort} of {where} (profile {profile_id})")
    if where == "the route":
        if not ans["usable"]:
            problems.append(f"the route is {ans['level']}: it needs an owner instruction before use")
    else:
        problems.append(f"the brief uses an alternative of the route ({role}, {profile_id}); it needs an owner instruction")
    scope = brief.get("SCOPE", "").strip()
    explanation = re.sub(r"(?i)^(none|n/a|-)\b[:.;,\s-]*", "", scope) if re.match(r"(?i)^(none|n/a|-)\b", scope) else None
    if explanation and re.search(r"[/\\]|\.\w|\b(write|edit|modify|create)\b|[,;]", explanation, re.I):
        problems.append(f"SCOPE {scope[:60]!r} starts with none but adds something to change")
    if not ans["writes"] and scope and explanation is None:
        problems.append(f"SCOPE {scope[:60]!r} names something to change but task type {brief['TASK_TYPE']} is read-only (write SCOPE: none)")
    return problems, ans


def launch_command(reg: dict, domains: dict[str, dict], brief: dict[str, str], default_workspace: str) -> tuple[str | None, list[str]]:
    """Return the codex exec command a brief maps to (text only; nothing is run) and warnings."""
    problems, ans = lint_brief(reg, domains, brief)
    if ans is None or ans["status"] != "ok":
        return None, problems or ["no route"]
    if ans.get("tool") != "codex-exec":
        return None, problems + [f"profile {ans.get('profile')} is launched with {ans.get('tool')}, not codex exec: no command to print"]
    task = re.sub(r"[^A-Za-z0-9._-]", "_", brief.get("TASK_ID", "TASK"))
    workspace = brief.get("WORKSPACE", "").split(",")[0].strip().replace("\\", "/") or default_workspace
    workspace = shlex.quote(workspace)  # a path with spaces or shell syntax stays one argument
    sandbox = "read-only" if ans["sandbox"] == "read-only" else "workspace-write"
    cmd = (f'codex exec -m {shlex.quote(str(ans["model"]))} -c model_reasoning_effort={shlex.quote(str(ans["effort"]))} -s {sandbox} -C {workspace} --color never '
           f'--output-schema tools/agent/worker-report.schema.json -o "$RUN/{task}.report.json" - < "$RUN/{task}.brief.md" > "$RUN/{task}.log" 2>&1')
    warnings = list(problems)
    if ans["sandbox"].startswith("workspace-write inside one lab folder"):
        warnings.append("lab work: -C must be the one lab folder; whether workspace-write limits writes to it is UNVERIFIED")
    return cmd, warnings


def _is_route(reg: dict, domains: dict[str, dict], domain: str | None, task_type: str | None, role: str, profile: str) -> bool:
    """True when (role, profile) is the route, or an alternative of it, for the task type in the domain."""
    if not task_type:
        return True
    ans = resolve(reg, domains, task_type, domain)
    if ans["status"] != "ok":
        return False
    if (ans["role"], ans["profile"]) == (role, profile):
        return True
    return any((a["role"], a["profile"]) == (role, profile) for a in ans.get("alternatives", []))


def who(reg: dict, domains: dict[str, dict], claims: list[tuple[str, dict]]) -> tuple[list[str], list[str], list[str]]:
    """One line per active claim, plus problems for a holder or worker that is not a registry route. Pure."""
    lines: list[str] = []
    problems: list[str] = []
    notes: list[str] = []
    workers = 0
    profiles = {p["id"]: p for p in reg["profiles"]}
    roles = {r["id"] for r in reg["roles"]}
    active = [(n, c) for n, c in claims if c.get("status") == "active"]
    for name, claim in active:
        dom = claim.get("domain")
        holder = claim.get("holder")
        if not isinstance(holder, dict):
            notes.append(f"claim {name}: holder is unstructured (free text: {claim.get('model')!r}); add holder and workers")
            lines.append(f"{dom} | {claim.get('task')} | unstructured: {claim.get('model')} | workers 0 | expires {claim.get('expires')} | {claim.get('branch')}")
            continue
        if dom not in domains:
            problems.append(f"claim {name}: domain {dom!r} is not a registry domain")
        wlist = [w for w in claim.get("workers", []) if isinstance(w, dict)]
        workers += len(wlist)
        lines.append(f"{dom} | {claim.get('task')} | {holder.get('role')} {holder.get('model')} {holder.get('effort') or ''}".rstrip()
                     + f" | workers {len(wlist)} | expires {claim.get('expires')} | {claim.get('branch')}")
        for label, actor in [("holder", holder)] + [(f"worker {i + 1}", w) for i, w in enumerate(wlist)]:
            where = f"claim {name} {label}"
            if actor.get("role") not in roles:
                problems.append(f"{where}: role {actor.get('role')!r} is not a registry role")
                continue
            prof = profiles.get(actor.get("profile"))
            if prof is None:
                problems.append(f"{where}: profile {actor.get('profile')!r} is not a registry profile")
                continue
            if concrete(prof["model"]) and actor.get("model") and actor["model"] != prof["model"]:
                problems.append(f"{where}: model {actor['model']} differs from {prof['model']} of profile {prof['id']}")
            if prof.get("effort") and actor.get("effort") and actor["effort"] != prof["effort"]:
                problems.append(f"{where}: effort {actor['effort']} differs from {prof['effort']} of profile {prof['id']}")
            if label != "holder" and not actor.get("task_type"):
                problems.append(f"{where}: a worker needs a task_type")
            if actor.get("task_type") and dom in domains and not _is_route(reg, domains, dom, actor["task_type"], actor["role"], actor["profile"]):
                problems.append(f"{where}: ({actor['role']}, {actor['profile']}, {actor['task_type']}) is not a registry route in domain {dom}")
    lines.append(f"{len(active)} active claim(s), {workers} worker(s), {reg.get('stationed_agents', 0)} stationed agent(s)")
    return lines, problems, notes


def check(root: Path) -> tuple[list[str], list[str]]:
    problems: list[str] = []
    notes: list[str] = []
    try:
        reg = load(root)
    except (OSError, ValueError) as exc:
        return [f"{REGISTRY}: cannot read: {exc}"], notes
    if reg.get("schema_version") != "2.0.0":
        problems.append(f"schema_version is {reg.get('schema_version')!r}, expected 2.0.0")
        return problems, notes
    domains = domain_info(root)
    decisions = vd.read_json(root / "contracts" / "decisions.json", vd.Report())
    known_dec = {d["id"] for d in decisions["decisions"]} if decisions else set()
    if reg.get("evidence_levels") != LEVELS:
        problems.append(f"evidence_levels must be {LEVELS}")

    def ev(label: str, e: dict | None) -> None:
        if not isinstance(e, dict):
            problems.append(f"{label}: evidence is missing")
            return
        if e.get("level") not in LEVELS:
            problems.append(f"{label}: unknown evidence level {e.get('level')!r}")
        if not e.get("source"):
            problems.append(f"{label}: evidence has no source")
        if not DATE_RE.match(str(e.get("date", ""))):
            problems.append(f"{label}: evidence date must be YYYY-MM-DD")
        if not isinstance(e.get("relayed"), bool):
            problems.append(f"{label}: evidence 'relayed' must be true or false")

    def unique(kind: str, items: list[dict]) -> dict[str, dict]:
        seen: dict[str, dict] = {}
        for i in items:
            if i.get("id") in seen:
                problems.append(f"{kind} id {i.get('id')} is defined twice")
            seen[i.get("id")] = i
        return seen

    profiles = unique("profile", reg.get("profiles", []))
    roles = unique("role", reg.get("roles", []))
    types = unique("task type", reg.get("task_types", []))
    for p in profiles.values():
        if p.get("tool") not in TOOLS:
            problems.append(f"profile {p['id']}: unknown tool {p.get('tool')!r}")
        if not p.get("model"):
            problems.append(f"profile {p['id']}: model is empty")
        ev(f"profile {p['id']}", p.get("evidence"))
    for r in roles.values():
        if r.get("kind") not in ("human", "agent"):
            problems.append(f"role {r['id']}: kind must be human or agent")
        for flag in ("may_claim", "may_edit_cards", "may_commit"):
            if not isinstance(r.get(flag), bool):
                problems.append(f"role {r['id']}: {flag} must be true or false")
    for t in types.values():
        if not isinstance(t.get("writes"), bool):
            problems.append(f"task type {t['id']}: writes must be true or false")
    reserved = reg.get("owner_reserved", {})
    for rid in reserved.get("ids", []):
        if rid in types:
            problems.append(f"owner-reserved id {rid} is also a task type")
    if not reserved.get("source"):
        problems.append("owner_reserved has no source")
    seen_routes: set[tuple] = set()
    routed: set[str] = set()
    for rt in reg.get("routes", []):
        label = f"route {rt.get('task_type')}/{rt.get('domain')}"
        key = (rt.get("task_type"), rt.get("domain"))
        if key in seen_routes:
            problems.append(f"{label} is defined twice")
        seen_routes.add(key)
        if rt.get("task_type") not in types:
            problems.append(f"{label}: unknown task type")
        else:
            routed.add(rt["task_type"])
        if rt.get("domain") != "*" and rt.get("domain") not in domains:
            problems.append(f"{label}: unknown domain")
        role = roles.get(rt.get("role"))
        if role is None:
            problems.append(f"{label}: role {rt.get('role')!r} is not defined")
        pid = rt.get("profile")
        if pid is not None and pid not in profiles:
            problems.append(f"{label}: profile {pid!r} is not defined")
        ev(label, rt.get("evidence"))
        if pid is None and role and role.get("kind") == "agent" and (rt.get("evidence") or {}).get("level") != "proposed":
            problems.append(f"{label}: an agent route without a profile must be 'proposed'")
        if pid in profiles and role and role.get("kind") == "human":
            problems.append(f"{label}: a human role cannot have a profile")
        if pid in profiles and level_index(profiles[pid]["evidence"]["level"]) < level_index((rt.get("evidence") or {}).get("level", "")) :
            notes.append(f"{label}: the profile {pid} is weaker ({profiles[pid]['evidence']['level']}) than the route ({(rt.get('evidence') or {}).get('level')}); the effective level is the weaker")
        rev = rt.get("reviewer")
        if rev:
            if rev.get("role") not in roles:
                problems.append(f"{label}: reviewer role {rev.get('role')!r} is not defined")
            if rev.get("profile") not in profiles:
                problems.append(f"{label}: reviewer profile {rev.get('profile')!r} is not defined")
            elif rev.get("role") == rt.get("role"):
                problems.append(f"{label}: the author role reviews itself")
            elif pid in profiles:
                rm, am = profiles[rev["profile"]]["model"], profiles[pid]["model"]
                if concrete(rm) and concrete(am) and rm == am:
                    problems.append(f"{label}: reviewer uses the same model as the author ({am})")
                elif not (concrete(rm) and concrete(am)):
                    notes.append(f"{label}: reviewer model separation is UNVERIFIABLE (a model is not concrete)")
        for alt in rt.get("alternatives", []):
            if alt.get("role") not in roles or alt.get("profile") not in profiles:
                problems.append(f"{label}: alternative {alt!r} refers to an undefined role or profile")
        if (rt.get("evidence") or {}).get("level") != "owner_chosen":
            notes.append(f"{label} -> {rt.get('role')} is {(rt.get('evidence') or {}).get('level')}: needs an owner instruction before use")
    for tid in types:
        if tid not in routed:
            notes.append(f"task type {tid} has no route")
    seen_pol: set[str] = set()
    for pol in reg.get("domain_policies", []):
        label = f"policy {pol.get('domain')}"
        if pol.get("domain") not in domains:
            problems.append(f"{label}: unknown domain")
        if pol.get("domain") in seen_pol:
            problems.append(f"{label} is defined twice")
        seen_pol.add(pol.get("domain"))
        if pol.get("agent_work") not in AGENT_WORK:
            problems.append(f"{label}: unknown agent_work {pol.get('agent_work')!r}")
        for t in pol.get("allowed_task_types", []):
            if t not in types:
                problems.append(f"{label}: unknown task type {t!r}")
        src = str(pol.get("source", ""))
        cited_dec = [d for d in vd.DEC_RE.findall(src)]
        all_paths = re.findall(r"[A-Za-z0-9_./-]+\.(?:md|json)", src)
        cited_path = [p for p in all_paths if (root / p).is_file()]
        if not src or (not any(d in known_dec for d in cited_dec) and not cited_path):
            problems.append(f"{label}: the source must cite an existing DEC id or repository document")
        for missing in sorted(set(all_paths) - set(cited_path)):
            problems.append(f"{label}: the source cites {missing}, which does not exist")
        for d in cited_dec:
            if d not in known_dec:
                problems.append(f"{label}: source cites dangling decision id {d}")
        if pol.get("agent_work") == "redirect" and domains.get(pol.get("domain"), {}).get("answers_for") is None:
            problems.append(f"{label}: redirect needs the domain registry row to say 'answers: `<domain>`'")
    for x in reg.get("exclusions", []):
        if x.get("profile") not in profiles or x.get("task_type") not in types or (x.get("domain") not in (None, "*") and x.get("domain") not in domains):
            problems.append(f"exclusion {x!r} refers to an undefined profile, task type or domain")
        if not x.get("source"):
            problems.append(f"exclusion {x!r} has no source")
    for dom in sorted(domains):
        _, err = redirect_chain(domains, dom)
        if err:
            problems.append(f"domain {dom}: {err}")
    for dom, info in sorted(domains.items()):
        if info["tier"] == "C":
            continue
        try:
            r = resolve(reg, domains, "implement", dom)
        except (KeyError, TypeError):
            notes.append(f"domain {dom}: 'implement' cannot be resolved while the registry is incomplete")
            continue
        if r["status"] != "ok":
            notes.append(f"domain {dom}: 'implement' is {r['status']} ({r.get('message', '')[:90]})")
    doc = root / DOC
    if not doc.is_file():
        problems.append(f"{DOC}: missing")
    else:
        text = doc.read_text(encoding="utf-8-sig")
        if BEGIN in text and END in text:
            try:
                fresh = render_tables(reg).strip()
            except (KeyError, TypeError):
                fresh = None
                notes.append(f"{DOC}: the generated tables cannot be compared while the registry is incomplete")
            if fresh is not None and text.split(BEGIN, 1)[1].split(END, 1)[0].strip() != fresh:
                problems.append(f"{DOC}: the generated tables differ from the registry; run python tools/validate_agents.py --write-doc")
        else:
            problems.append(f"{DOC}: generated-table markers are missing")
    return problems, notes


def cell(text) -> str:
    return str(text if text is not None else "").replace("|", "\\|").replace("\n", " ")


def render_tables(reg: dict) -> str:
    roles = reg["roles"]
    lines = [f"Registry content: {len(reg['profiles'])} profiles, {len(roles)} roles ({sum(r['kind'] == 'human' for r in roles)} human, "
             f"{sum(r['kind'] == 'agent' for r in roles)} agent), {len(reg['task_types'])} task types, {len(reg['routes'])} routes, "
             f"{len(reg.get('domain_policies', []))} domain policies. Stationed agents: {reg.get('stationed_agents', 0)}.", ""]
    lines += ["| Profile | Tool | Model (effort) | Level | Relayed | Source |", "| --- | --- | --- | --- | --- | --- |"]
    for p in reg["profiles"]:
        e = p["evidence"]
        eff = f" ({p['effort']})" if p.get("effort") else ""
        lines.append(f"| `{p['id']}` | {p['tool']} | {cell(p['model'])}{eff} | {e['level']} | {'yes' if e['relayed'] else 'no'} | {cell(e['source'])} |")
    lines += ["", "| Role | Kind | Claims | Edits cards | Commits |", "| --- | --- | --- | --- | --- |"]
    for r in roles:
        lines.append(f"| `{r['id']}` | {r['kind']} | {'yes' if r['may_claim'] else 'no'} | {'yes' if r['may_edit_cards'] else 'no'} | {'yes' if r['may_commit'] else 'no'} |")
    lines += ["", "| Task type | Writes | Pattern | Review |", "| --- | --- | --- | --- |"]
    for t in reg["task_types"]:
        lines.append(f"| `{t['id']}` | {'yes' if t['writes'] else 'no'} | {', '.join(t['pattern'])} | {cell(t['needs_review'])} |")
    lines += ["", "| Task type | Domain | Role | Profile | Reviewer | Level | Relayed |", "| --- | --- | --- | --- | --- | --- | --- |"]
    for r in reg["routes"]:
        rev = f"`{r['reviewer']['role']}` ({r['reviewer']['profile']})" if r.get("reviewer") else "none"
        lines.append(f"| `{r['task_type']}` | {r['domain']} | `{r['role']}` | {('`' + r['profile'] + '`') if r.get('profile') else 'none'} | {rev} | {r['evidence']['level']} | {'yes' if r['evidence']['relayed'] else 'no'} |")
    lines += ["", "| Domain | Agent work | Allowed task types | Source |", "| --- | --- | --- | --- |"]
    for p in reg.get("domain_policies", []):
        lines.append(f"| `{p['domain']}` | {p['agent_work']} | {', '.join(p.get('allowed_task_types', [])) or 'none'} | {cell(p['source'])} |")
    lines += ["", "| Owner-reserved | Meaning |", "| --- | --- |"]
    for rid in reg["owner_reserved"]["ids"]:
        lines.append(f"| `{rid}` | {cell(reg['owner_reserved']['meaning'].get(rid, ''))} |")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    ap.add_argument("--enforce", action="store_true", help="exit 1 on any problem (owner decision; the gate does not pass this)")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--route", metavar="TASK_TYPE")
    ap.add_argument("--domain")
    ap.add_argument("--who", action="store_true", help="one line per active claim: who is working now, checked against the registry")
    ap.add_argument("--coord", type=Path, default=None, help="coordination ledger (default: KOHBOR_COORD_DIR or F:/kohbor-coord)")
    ap.add_argument("--brief", type=Path, help="lint a handoff brief against its route (exit 1 on a mismatch)")
    ap.add_argument("--launch", type=Path, help="print the codex exec command for a brief; never runs it")
    ap.add_argument("--write-doc", action="store_true", help="refresh the generated tables in AGENT-REGISTRY.md")
    opts = ap.parse_args(argv)
    root = opts.root.resolve()
    try:
        if opts.route:
            answer = resolve(load(root), domain_info(root), opts.route, opts.domain)
            print(json.dumps(answer, indent=2, ensure_ascii=False))
            return 1 if answer["status"] == "error" and opts.enforce else 0
        if opts.who:
            import os
            import check_claim_scope as ccs

            coord = opts.coord or (Path(os.environ["KOHBOR_COORD_DIR"]) if os.environ.get("KOHBOR_COORD_DIR") else Path("F:/kohbor-coord"))
            if not (coord / "claims").is_dir():
                print("validate_agents --who: coordination ledger not found here; not checked")
                return 0
            claims = [(f.stem, d) for f in sorted((coord / "claims").glob("*.json")) if isinstance(d := ccs.lenient_json(f), dict)]
            lines, problems, notes = who(load(root), domain_info(root), claims)
            if opts.json:
                print(json.dumps({"active": lines, "problems": problems, "notes": notes}, indent=2, ensure_ascii=False))
            else:
                for line in lines:
                    print(line)
                for p in problems:
                    print("PROBLEM " + p)
                for n in notes:
                    print("NOTE " + n)
            return 1 if problems and opts.enforce else 0
        if opts.brief or opts.launch:
            path = opts.brief or opts.launch
            try:
                brief = parse_brief(path.read_text(encoding="utf-8-sig"))
                reg, domains = load(root), domain_info(root)
                resolve(reg, domains, "implement", None)  # fail early on a broken registry
            except (OSError, ValueError, KeyError, TypeError) as exc:
                print(f"MISMATCH cannot lint the brief: {exc!r}")
                return 1
            if opts.launch:
                cmd, warns = launch_command(reg, domains, brief, root.as_posix())
                for w in warns:
                    print("WARNING " + w, file=sys.stderr)
                if cmd is None:
                    return 1
                print(cmd)
                return 1 if any(not w.startswith("lab work") for w in warns) else 0
            problems, ans = lint_brief(reg, domains, brief)
            for p in problems:
                print("MISMATCH " + p)
            route = f"; route {ans['role']}/{ans.get('profile')} level {ans.get('level')}" if ans and ans.get("status") == "ok" else ""
            print(f"validate_agents: brief {brief.get('TASK_ID', '?')}: {'OK' if not problems else str(len(problems)) + ' mismatch(es)'}{route}")
            return 1 if problems else 0
        if opts.write_doc:
            doc = root / DOC
            text = doc.read_text(encoding="utf-8-sig")
            head, rest = text.split(BEGIN, 1)
            tail = rest.split(END, 1)[1]
            doc.write_text(head + BEGIN + "\n" + render_tables(load(root)) + "\n" + END + tail, encoding="utf-8", newline="\n")
            print(f"refreshed the generated tables in {DOC}")
            return 0
        problems, notes = check(root)
    except Exception as exc:  # report-only tools must not break the gate by crashing
        problems, notes = [f"validate_agents crashed: {exc!r}"], []
    mode = "ENFORCED" if opts.enforce else "REPORT-ONLY"
    summary = f"validate_agents: {mode}: {len(problems)} problem(s), {len(notes)} note(s)"
    if opts.json:
        print(json.dumps({"mode": mode, "problems": problems, "notes": notes, "scope": "documents only; no application evidence"}, indent=2, ensure_ascii=False))
    else:
        for p in problems:
            print("PROBLEM " + p)
        for n in notes:
            print("NOTE " + n)
        print(summary)
    return 1 if problems and opts.enforce else 0


if __name__ == "__main__":
    raise SystemExit(main())
