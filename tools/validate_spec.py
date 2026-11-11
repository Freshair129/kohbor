#!/usr/bin/env python3
"""Check specification consistency and synthetic contract examples, never app acceptance."""
import argparse
import csv
import hashlib
import json
import math
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath

SKIP_DIRS={"node_modules","__pycache__","target",".git","spike-out"}

def strict_pairs(pairs):
    result={}
    for key,value in pairs:
        if key in result:raise ValueError("Duplicate JSON key: "+key)
        result[key]=value
    return result

def read_json(p):
    return json.loads(p.read_text(encoding="utf-8"),object_pairs_hook=strict_pairs)

def semantic(rule,data):
    if rule=="hp_bounds":
        stats=data.get("own",data)
        if stats is None:return True
        if "fields" in stats:
            hp=stats["fields"]["hp"]["value"];max_hp=stats["fields"]["max_hp"]["value"]
            return hp is None or max_hp is None or hp<=max_hp
        return stats["hp"]<=stats["max_hp"]
    if rule=="peer_publish_age":
        stats=data["sample"]
        return stats is None or (data["scope"] is not None and stats["source_age_ms"]<=2000 and stats["hp"]<=stats["max_hp"])
    if rule=="manifest_paths":
        paths=[f["path"] for f in data["files"]]
        if len(paths)!=len(set(paths)):return False
        for name in paths:
            p=PurePosixPath(name)
            if p.is_absolute() or "\\" in name or any(x in (".","..","") for x in name.split("/")):return False
        roles={f["path"]:f["role"] for f in data["files"]}
        for b in data["bindings"]:
            if roles.get(b["mixed_audio_path"])!="mixed_audio" or roles.get(b["silent_visual_path"])!="silent_visual":return False
            if "identity_audio_path" in b and roles.get(b["identity_audio_path"])!="identity_audio":return False
        return True
    if rule=="event_catalog":
        events=data["events"]
        ids=[e["id"] for e in events]
        if len(ids)!=len(set(ids)) or set(ids)&set(data["retired_ids"]):return False
        ranks=[]
        for e in events:
            rank=e["ordinary_rank"]
            voiced_ordinary=e["category"]=="ordinary" and e["voiced_default"]
            if voiced_ordinary!=(rank is not None):return False
            if rank is not None:ranks.append(rank)
            if e["category"]=="test" and e["status"]!="test_only":return False
            for t in e["tunables"].values():
                if t["min"]>t["max"]:return False
                values=t["default"] if isinstance(t["default"],list) else [t["default"]]
                if any(v<t["min"] or v>t["max"] for v in values):return False
            tun=e["tunables"]
            if "threshold_percent" in tun and "rearm_percent" in tun and tun["rearm_percent"]["default"]<=tun["threshold_percent"]["default"]:return False
            if "max_hp_percent" in tun and "rearm_percent" in tun and tun["rearm_percent"]["default"]<=tun["max_hp_percent"]["default"]:return False
        return len(ranks)==len(set(ranks))
    raise ValueError("Unknown semantic check: "+rule)

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--forbidden-token",action="append",default=[],help="Optional external audit token; never written into the package.")
    ap.add_argument("--report",type=Path)
    args=ap.parse_args()
    root=Path(__file__).resolve().parents[1]
    errors=[]
    json_files=[p for p in root.rglob("*.json") if not set(p.relative_to(root).parts)&SKIP_DIRS]
    parsed={}
    for path in json_files:
        try:parsed[path.relative_to(root).as_posix()]=read_json(path)
        except Exception as exc:errors.append(f"JSON {path.relative_to(root)}: {exc}")
    if errors:
        print(json.dumps({"status":"FAIL","errors":errors},indent=2));return 1
    reqs=parsed["contracts/requirements.json"]["requirements"]
    ids=[r["id"] for r in reqs]
    if len(ids)!=len(set(ids)):errors.append("Duplicate requirement ID.")
    known=set(ids)
    needs=parsed["contracts/product-needs.json"]["needs"]
    need_ids=[n["id"] for n in needs]
    if len(need_ids)!=len(set(need_ids)):errors.append("Duplicate product need ID.")
    needs_by_requirement={rid:[] for rid in ids}
    prd=(root/"docs/product/PRD.md").read_text(encoding="utf-8")
    for need in needs:
        if not need["requirement_ids"]:errors.append(need["id"]+": unmapped product need.")
        for rid in need["requirement_ids"]:
            if rid not in known:errors.append(need["id"]+": unknown requirement "+rid)
            else:needs_by_requirement[rid].append(need["id"])
        prd_row=re.search(r"^\| "+re.escape(need["id"])+r" \|.*$",prd,re.M)
        if not prd_row or set(re.findall(r"\b[A-Z]+-\d{3}\b",prd_row.group(0)))-{need["id"]}!=set(need["requirement_ids"]):
            errors.append(need["id"]+": PRD/structured need mapping mismatch.")
    sdd=(root/"docs/design/SDD.md").read_text(encoding="utf-8")
    component_ids=set(re.findall(r"^### (CMP-[A-Z]+)$",sdd,re.M))
    primary_allocations={}
    for section in re.split(r"^### ",sdd,flags=re.M)[1:]:
        component=section.splitlines()[0]
        if component not in component_ids:continue
        allocation=re.search(r"\*\*Primary SRS allocation:\*\* (.*)",section)
        if not allocation:
            errors.append(component+": no primary requirements allocation.")
            continue
        for rid in re.findall(r"`([A-Z]+-\d{3})`",allocation.group(1)):
            if rid in primary_allocations:errors.append(rid+": duplicate primary design allocation.")
            primary_allocations[rid]=component
    if set(primary_allocations)!=known:errors.append("SDD primary requirement coverage mismatch.")
    test_plan=(root/"docs/quality/TEST-PLAN.md").read_text(encoding="utf-8")
    uat_plan=(root/"docs/quality/UAT-PLAN.md").read_text(encoding="utf-8")
    procedure_ids=set(re.findall(r"\*\*(TP-\d{2}) —",test_plan)) | set(re.findall(r"^### (UAT-\d{2}) —",uat_plan,re.M))
    decisions=parsed["contracts/decisions.json"]["decisions"]
    decision_ids={d["id"] for d in decisions}
    diagrams=parsed["docs/diagrams/catalog.json"]["diagrams"]
    diagram_ids={d["id"] for d in diagrams}
    if len(diagram_ids)!=len(diagrams):errors.append("Duplicate diagram ID.")
    diagram_report=parsed["acceptance/diagram-validation.json"]
    rendered_diagrams={d["id"]:d for d in diagram_report["diagrams"]}
    if set(rendered_diagrams)!=diagram_ids:errors.append("Diagram render coverage mismatch.")
    if diagram_report["status"]!="PASS":errors.append("Diagram rendering has failures.")
    for d in diagrams:
        for name in ("source","rendered","owner_doc"):
            if not (root/d[name]).is_file():errors.append(d["id"]+": missing "+name)
        if not set(d["requirement_ids"])<=known:errors.append(d["id"]+": unknown requirement reference.")
        if not set(d["decision_ids"])<=decision_ids:errors.append(d["id"]+": unknown decision reference.")
        if not d["purpose"] or not d["notes"]:errors.append(d["id"]+": missing purpose/guard notes.")
        source=(root/d["source"]).read_text(encoding="utf-8")
        if re.search(r"%%\{\s*init|^\s*click\s|<br\b|<script\b",source,re.I|re.M):errors.append(d["id"]+": unsupported directive/HTML.")
        if "sequenceDiagram" in source and len(re.findall(r"^\s*(?:actor|participant)\s",source,re.M))>5:
            errors.append(d["id"]+": more than five sequence participants.")
        result=rendered_diagrams.get(d["id"],{})
        if result.get("status")!="PASS":errors.append(d["id"]+": rendering not passed.")
        for name,key in (("source","source_sha256"),("rendered","svg_sha256")):
            if hashlib.sha256((root/d[name]).read_bytes()).hexdigest()!=result.get(key):errors.append(d["id"]+": stale "+name+" hash.")
    for r in reqs:
        for path in r["contract_refs"]+r["spec_refs"]+r["design_refs"]+r["verification"]["procedure_refs"]+r["diagram_refs"]:
            if not (root/path).is_file():errors.append(f'{r["id"]}: missing {path}')
        related=[d for d in diagrams if r["id"] in d["requirement_ids"]]
        if r["diagram_ids"]!=[d["id"] for d in related] or r["diagram_refs"]!=[d["source"] for d in related]:
            errors.append(r["id"]+": diagram traceability mismatch.")
        if not r["product_need_ids"] or set(r["product_need_ids"])!=set(needs_by_requirement[r["id"]]):
            errors.append(r["id"]+": product need traceability mismatch.")
        if not r["design_component_ids"] or not set(r["design_component_ids"])<=component_ids:
            errors.append(r["id"]+": missing/undefined design component.")
        if r["primary_design_component_id"]!=primary_allocations.get(r["id"]) or r["primary_design_component_id"] not in r["design_component_ids"]:
            errors.append(r["id"]+": SDD/registry primary allocation mismatch.")
        if not r["verification"]["procedure_ids"] or not set(r["verification"]["procedure_ids"])<=procedure_ids:
            errors.append(r["id"]+": missing/undefined verification procedure.")
        for did in r["open_decisions"]:
            if did not in decision_ids:errors.append(f'{r["id"]}: unknown decision {did}')
        if r.get("target_milestone") not in {"M1","M2","M3","M4a","M4b","M5","M6","M7","M8"}:errors.append(r["id"]+": missing/unknown target milestone.")
        if not r["statement"] or not r["failure_behavior"] or not r["acceptance"]["then"]:errors.append(r["id"]+": incomplete acceptance.")
    catalog=parsed["contracts/event-catalog.json"]
    production_ids={e["id"] for e in catalog["events"] if e["status"]!="test_only"}
    if set(parsed["contracts/event.schema.json"]["properties"]["event_type"]["enum"])!=production_ids:
        errors.append("event.schema event_type enum does not match the canonical event catalog.")
    if not semantic("event_catalog",catalog):errors.append("Canonical event catalog fails semantic rules.")
    rendered=subprocess.run([sys.executable,str(root/"tools/render_requirements.py"),"--check"],capture_output=True,text=True)
    if rendered.returncode:errors.append(rendered.stdout.strip() or rendered.stderr.strip())
    with (root/"docs/requirements/RTM.csv").open(encoding="utf-8",newline="") as f:
        trace=list(csv.DictReader(f))
    trace_ids=[r["requirement_id"] for r in trace]
    if set(trace_ids)!=known or len(trace_ids)!=len(known):errors.append("Traceability coverage mismatch.")
    scenario_text=(root/"acceptance/scenarios.feature").read_text()
    for r in reqs:
        if len(re.findall(r"Scenario: "+re.escape(r["acceptance"]["scenario_id"])+r"\b",scenario_text))!=1:
            errors.append(r["id"]+": scenario missing/duplicate.")
    link_count=0
    for p in root.rglob("*.md"):
        if set(p.relative_to(root).parts)&SKIP_DIRS:continue
        content=p.read_text(encoding="utf-8")
        if content.count(chr(96)*3)%2:errors.append(str(p.relative_to(root))+": unbalanced code fences.")
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)",content):
            target=target.strip().split("#",1)[0]
            if not target or re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:",target):continue
            link_count+=1
            if not (p.parent/target).exists():errors.append(str(p.relative_to(root))+": broken link "+target)
    forbidden_hits=0
    for p in root.rglob("*"):
        if not p.is_file() or set(p.relative_to(root).parts)&SKIP_DIRS:continue
        try:content=p.read_text(encoding="utf-8").casefold()
        except UnicodeDecodeError:continue
        for token in args.forbidden_token:
            if token.casefold() in content or token.casefold() in p.name.casefold():forbidden_hits+=1
    if forbidden_hits:errors.append(f"Forbidden-token matches in {forbidden_hits} file/token combinations.")
    try:
        import jsonschema
    except ImportError:
        jsonschema=None
    fixture_cases=parsed["fixtures/index.json"]["cases"]
    schema_passed=0
    semantic_passed=0
    if jsonschema:
        for path,data in parsed.items():
            if path.endswith(".schema.json"):
                try:jsonschema.Draft202012Validator.check_schema(data)
                except Exception as exc:errors.append(path+": invalid schema "+str(exc))
        for case in fixture_cases:
            if case["requirement_id"] not in known:errors.append(case["id"]+": unknown requirement.")
            data=parsed[case["fixture"]]
            v=jsonschema.Draft202012Validator(parsed[case["schema"]],format_checker=jsonschema.FormatChecker())
            actual=not list(v.iter_errors(data))
            if actual!=case["schema_valid"]:errors.append(case["id"]+": structural result mismatch.")
            else:schema_passed+=1
            if "semantic" in case:
                actual_semantic=semantic(case["semantic"],data)
                if actual_semantic!=case["semantic_valid"]:errors.append(case["id"]+": semantic result mismatch.")
                else:semantic_passed+=1
    else:
        for case in fixture_cases:
            if "semantic" in case:
                if semantic(case["semantic"],parsed[case["fixture"]])==case["semantic_valid"]:semantic_passed+=1
                else:errors.append(case["id"]+": semantic result mismatch.")
    golden=parsed["fixtures/risk-golden.json"]
    risk_passed=0
    for case in golden["single_hero"]:
        s=case["missing_seconds"]
        actual=0.0 if s<5 else ((s-5)/7)*0.7 if s<=12 else max(0.7-(s-12)*0.03,0.1)
        if not math.isclose(actual,case["expected_score"],rel_tol=0,abs_tol=golden["absolute_tolerance"]):errors.append("Risk formula golden mismatch.")
        else:risk_passed+=1
    for case in golden["aggregate"]:
        actual=1-math.prod(1-r for r in case["scores"])
        if len(case["scores"])>=2:actual=min(actual*1.15,1.0)
        if not math.isclose(actual,case["expected_score"],rel_tol=0,abs_tol=golden["absolute_tolerance"]):errors.append("Risk aggregate golden mismatch.")
        else:risk_passed+=1
    hd=parsed["fixtures/health-danger-golden.json"]
    hd_passed=0
    prm=hd["parameters"]
    for hd_trace in hd["traces"]:
        armed=True;last_cue=None;cues=[];window=[]
        for sample in hd_trace["samples"]:
            t=sample["t_ms"]
            if not sample["alive"]:
                armed=True;window=[];continue
            if sample["availability"]!="fresh" or sample["hp"] is None or sample["max_hp"] is None:continue
            pct=sample["hp"]*100/sample["max_hp"]
            window=[(wt,wp) for wt,wp in window if wt>=t-prm["window_ms"]]+[(t,pct)]
            if pct>=prm["rearm_percent"]:armed=True
            drop=max(wp for _,wp in window)-pct
            if armed and pct<=prm["max_hp_percent"] and drop>=prm["drop_points"] and (last_cue is None or t-last_cue>=prm["cooldown_ms"]):
                cues.append(t);last_cue=t;armed=False
        if cues!=hd_trace["expected_cue_t_ms"]:errors.append("Health-danger golden trace mismatch: "+hd_trace["id"])
        else:hd_passed+=1
    result={
      "status":"FAIL" if errors else ("PASS" if jsonschema else "PARTIAL"),
      "health_danger_traces_passed":hd_passed,
      "scope":"Specification consistency and synthetic contract validation only; no application execution.",
      "requirements":len(reqs),"traceability_rows":len(trace),"acceptance_scenarios":len(reqs),
      "product_needs":len(needs),"design_components":len(component_ids),"verification_procedures":len(procedure_ids),
      "requirements_with_product_need_mapping":sum(bool(r["product_need_ids"]) for r in reqs),
      "diagram_views":len(diagrams),"diagram_source_and_svg_hashes":"PASS" if not any("hash" in e for e in errors) else "FAIL",
      "requirements_with_diagram_views":sum(bool(r["diagram_ids"]) for r in reqs),
      "decision_records":len(decisions),"json_files_parsed":len(json_files),
      "json_schemas":sum(x.endswith(".schema.json") for x in parsed),
      "schema_fixture_cases":len(fixture_cases),"schema_fixture_cases_passed":schema_passed,
      "semantic_fixture_cases_passed":semantic_passed,"risk_formula_examples_passed":risk_passed,
      "relative_links_checked":link_count,"forbidden_token_checks":len(args.forbidden_token),
      "forbidden_token_matches":forbidden_hits,
      "schema_engine":"jsonschema draft2020-12" if jsonschema else "NOT_RUN: install jsonschema to validate schemas",
      "application_native_live_acoustic_resource_tests":"NOT_RUN",
      "errors":errors
    }
    print(json.dumps(result,ensure_ascii=False,indent=2))
    if args.report:
        args.report.parent.mkdir(parents=True,exist_ok=True)
        args.report.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    return 1 if errors else 0

if __name__=="__main__":
    raise SystemExit(main())
