"""Tests for tools/progress_map.py: every status rule, tolerance of missing sources, determinism, a self-contained page.

Synthetic repositories in temp folders; no git, no network, no real ledger.
"""

from __future__ import annotations

import contextlib
import datetime as dt
import io
import json
import re
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import progress_map as pm  # noqa: E402

NOW = dt.datetime.fromisoformat("2026-11-14T10:00:00+07:00")


def no_git(args: list[str], cwd: Path) -> str | None:
    return None


def fake_git(args: list[str], cwd: Path) -> str | None:
    table = {
        ("rev-parse", "--short", "HEAD"): "abc1234", ("rev-parse", "--abbrev-ref", "HEAD"): "main",
        ("log", "-1", "--format=%cI"): "2026-11-14T09:00:00+07:00", ("rev-parse", "--short", "origin/main"): "abc1234",
        ("log", "-1", "--format=%cI", "origin/main"): "2026-11-14T09:00:00+07:00", ("rev-list", "--count", "HEAD..origin/main"): "0",
        ("status", "--porcelain"): "",
    }
    key = tuple(a for a in args[2:])
    if key in table:
        return table[key]
    if key[:1] == ("log",) and "--first-parent" in key:
        return "abc1234\t2026-11-14T09:00:00+07:00\tAdd a thing (#7)\n"
    return None


def req(rid: str, **kw) -> dict:
    base = {"id": rid, "title": "Title " + rid, "module": "core", "scope": "mvp", "priority": "P0", "target_milestone": "M1",
            "statement": "s", "open_decisions": [], "product_need_ids": ["PN-001"], "design_refs": [], "contract_refs": [], "spec_refs": [],
            "verification": {"required_evidence": ["synthetic", "native"], "new_build_status": "NOT_RUN", "implementation_refs": [], "evidence_refs": []}}
    ver = kw.pop("verification", {})
    base["verification"].update(ver)
    base.update(kw)
    return base


def dec(did: str, status: str = "open", owner: str = "product owner", **kw) -> dict:
    d = {"id": did, "status": status, "milestone": "M3", "blocks": "blocks " + did, "owner_role": owner, "safe_interim": "x", "closure_evidence": "close " + did}
    d.update(kw)
    return d


class Fixture:
    """A throwaway repository plus ledger."""

    def __init__(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="progmap-test-"))
        self.repo = self.root / "repo"
        self.coord = self.root / "coord"
        (self.repo / "contracts").mkdir(parents=True)
        (self.repo / "docs" / "design").mkdir(parents=True)
        (self.repo / "docs" / "delivery").mkdir(parents=True)
        (self.repo / "docs" / "domains").mkdir(parents=True)
        (self.coord / "claims").mkdir(parents=True)
        (self.coord / "registry").mkdir(parents=True)

    def cleanup(self) -> None:
        shutil.rmtree(self.root, ignore_errors=True)

    def file(self, rel: str, text: str = "x") -> None:
        p = self.repo / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")

    def requirements(self, reqs: list[dict]) -> None:
        (self.repo / "contracts" / "requirements.json").write_text(json.dumps({"requirements": reqs}), encoding="utf-8")

    def decisions(self, decs: list[dict]) -> None:
        (self.repo / "contracts" / "decisions.json").write_text(json.dumps({"decisions": decs}), encoding="utf-8")

    def claim(self, name: str, domain: str, write_area: list[str], status: str = "active", **kw) -> None:
        c = {"domain": domain, "task": name, "write_area": write_area, "status": status, "branch": "b", "started": "2026-11-14T08:00:00+07:00",
             "expires": "2026-11-15T08:00:00+07:00"}
        c.update(kw)
        (self.coord / "claims" / f"{name}.json").write_text(json.dumps(c), encoding="utf-8")

    def domain(self, dom: str, card: str = "", **state) -> None:
        d = self.repo / "docs" / "domains" / dom
        d.mkdir(parents=True, exist_ok=True)
        st = {"domain": dom, "tier": "A", "verified_on": "2026-11-13", "verified_at_commit": "abc", "decision_ids": [], "requirement_ids": []}
        st.update(state)
        (d / "state.json").write_text(json.dumps(st), encoding="utf-8")
        (d / "CARD.md").write_text(card, encoding="utf-8")

    def build(self, **kw) -> dict:
        return pm.build_model(self.repo, self.coord, kw.pop("design_dirs", []), NOW, run=kw.pop("run", no_git), gate_dir=self.root / "nogate", **kw)


def leaf(model: dict, nid: str) -> dict:
    for n in pm.iter_nodes(model["tree"]):
        if n["i"] == nid:
            return n
    raise AssertionError("no node " + nid)


class Base(unittest.TestCase):
    def setUp(self) -> None:
        self.fx = Fixture()
        self.addCleanup(self.fx.cleanup)


class StatusRules(Base):
    def test_verified_needs_evidence_and_pass(self) -> None:
        self.fx.file("src/a.rs")
        self.fx.requirements([req("CORE-001", verification={"implementation_refs": ["src/a.rs"], "evidence_refs": ["docs/e.json"], "new_build_status": "PASS"})])
        self.assertEqual(leaf(self.fx.build(), "req:CORE-001")["s"], "verified")

    def test_evidence_without_pass_is_not_verified(self) -> None:
        self.fx.file("src/a.rs")
        self.fx.requirements([req("CORE-001", verification={"implementation_refs": ["src/a.rs"], "evidence_refs": ["docs/e.json"], "new_build_status": "PARTIAL"})])
        m = self.fx.build()
        self.assertEqual(leaf(m, "req:CORE-001")["s"], "built")
        self.assertIn("not PASS", m["details"]["req:CORE-001"]["why"])

    def test_pass_without_evidence_refs_is_not_verified(self) -> None:
        self.fx.file("src/a.rs")
        self.fx.requirements([req("CORE-001", verification={"implementation_refs": ["src/a.rs"], "new_build_status": "PASS"})])
        self.assertEqual(leaf(self.fx.build(), "req:CORE-001")["s"], "built")

    def test_built_not_verified(self) -> None:
        self.fx.file("src/a.rs")
        self.fx.requirements([req("CORE-001", verification={"implementation_refs": ["src/a.rs"]})])
        self.assertEqual(leaf(self.fx.build(), "req:CORE-001")["s"], "built")

    def test_missing_implementation_file_does_not_count(self) -> None:
        self.fx.file("docs/design/SDD.md")
        self.fx.requirements([req("CORE-001", design_refs=["docs/design/SDD.md"], verification={"implementation_refs": ["src/ghost.rs"]})])
        m = self.fx.build()
        self.assertEqual(leaf(m, "req:CORE-001")["s"], "designed")
        self.assertIn("missing on disk", m["details"]["req:CORE-001"]["why"])
        self.assertFalse(m["details"]["req:CORE-001"]["files"][0]["ok"])

    def test_in_progress_needs_a_claim_and_no_code_yet(self) -> None:
        self.fx.requirements([req("CORE-001", verification={"implementation_refs": ["crates/new/src/lib.rs"]})])
        self.fx.claim("c1", "host-core", ["crates/new/**"])
        self.assertEqual(leaf(self.fx.build(), "req:CORE-001")["s"], "progress")

    def test_in_progress_by_claim_that_names_the_requirement(self) -> None:
        self.fx.requirements([req("CORE-001")])
        self.fx.claim("c1", "host-core", ["somewhere/**"], note="working on CORE-001 now")
        self.assertEqual(leaf(self.fx.build(), "req:CORE-001")["s"], "progress")

    def test_a_claim_does_not_demote_built_code(self) -> None:
        self.fx.file("crates/a/src/lib.rs")
        self.fx.requirements([req("CORE-001", verification={"implementation_refs": ["crates/a/src/lib.rs"]})])
        self.fx.claim("c1", "host-core", ["crates/a/**"])
        m = self.fx.build()
        n = leaf(m, "req:CORE-001")
        self.assertEqual(n["s"], "built")
        self.assertEqual(n["a"], 1)
        self.assertIn("active claim also touches", m["details"]["req:CORE-001"]["why"])

    def test_inactive_claims_do_not_count(self) -> None:
        self.fx.requirements([req("CORE-001")])
        self.fx.claim("c1", "host-core", ["x/**"], status="done", note="CORE-001")
        self.assertNotEqual(leaf(self.fx.build(), "req:CORE-001")["s"], "progress")

    def test_blocked_on_open_owner_decision_when_nothing_is_built(self) -> None:
        self.fx.requirements([req("CORE-001", open_decisions=["DEC-X"])])
        self.fx.decisions([dec("DEC-X")])
        m = self.fx.build()
        self.assertEqual(leaf(m, "req:CORE-001")["s"], "blocked")
        self.assertEqual(m["details"]["req:CORE-001"]["decisions"][0]["question"], "blocks DEC-X")

    def test_open_decision_that_is_not_owner_only_does_not_block(self) -> None:
        self.fx.requirements([req("CORE-001", open_decisions=["DEC-X"])])
        self.fx.decisions([dec("DEC-X", owner="architect")])
        self.assertNotEqual(leaf(self.fx.build(), "req:CORE-001")["s"], "blocked")

    def test_decided_decision_does_not_block(self) -> None:
        self.fx.requirements([req("CORE-001", open_decisions=["DEC-X"])])
        self.fx.decisions([dec("DEC-X", status="decided")])
        self.assertNotEqual(leaf(self.fx.build(), "req:CORE-001")["s"], "blocked")

    def test_built_stays_built_when_a_decision_is_open(self) -> None:
        self.fx.file("src/a.rs")
        self.fx.requirements([req("CORE-001", open_decisions=["DEC-X"], verification={"implementation_refs": ["src/a.rs"]})])
        self.fx.decisions([dec("DEC-X")])
        self.assertEqual(leaf(self.fx.build(), "req:CORE-001")["s"], "built")

    def test_designed_only_and_not_started(self) -> None:
        self.fx.file("docs/design/SDD.md")
        self.fx.requirements([req("CORE-001", design_refs=["docs/design/SDD.md"]), req("CORE-002", design_refs=["docs/design/GONE.md"])])
        m = self.fx.build()
        self.assertEqual(leaf(m, "req:CORE-001")["s"], "designed")
        self.assertEqual(leaf(m, "req:CORE-002")["s"], "notstarted")

    def test_unknown_when_a_requirement_has_no_data_at_all(self) -> None:
        self.fx.requirements([req("CORE-001", priority="", module="")])
        self.assertEqual(leaf(self.fx.build(), "req:CORE-001")["s"], "unknown")

    def test_decision_statuses(self) -> None:
        self.fx.decisions([dec("DEC-OPEN"), dec("DEC-DEFER", status="deferred"), dec("DEC-ARCH", owner="architect"), dec("DEC-PROV", status="provisional"),
                           dec("DEC-DONE", status="decided")])
        m = self.fx.build()
        self.assertEqual(leaf(m, "dec:DEC-OPEN")["s"], "blocked")
        self.assertEqual(leaf(m, "dec:DEC-PROV")["s"], "blocked")
        self.assertEqual(leaf(m, "dec:DEC-DEFER")["s"], "notstarted")
        self.assertEqual(leaf(m, "dec:DEC-ARCH")["s"], "notstarted")
        self.assertFalse(any(n["i"] == "dec:DEC-DONE" for n in pm.iter_nodes(m["tree"])))

    def test_backlog_rows_map_to_built_never_verified(self) -> None:
        self.fx.file("docs/delivery/SPRINT-ROADMAP.md",
                     "## Backlog status\n\n| Task | Status | Report | Commit |\n| --- | --- | --- | --- |\n"
                     "| S1-3 detector | passed the gate (T-L-R 2) | DONE | `ace21a0` |\n| S9-1 odd | wibble | ? | |\n")
        m = self.fx.build()
        nodes = {n["t"]: n["s"] for n in pm.iter_nodes(m["tree"]) if n["k"] == "task"}
        self.assertEqual(nodes["S1-3 detector"], "built")
        self.assertEqual(nodes["S9-1 odd"], "unknown")

    def test_rollup_is_plurality_with_ties_going_to_the_less_advanced(self) -> None:
        self.assertEqual(pm.rollup({"built": 2, "designed": 1}), "built")
        self.assertEqual(pm.rollup({"built": 1, "designed": 1}), "designed")
        self.assertEqual(pm.rollup({"verified": 1, "unknown": 1}), "unknown")
        self.assertEqual(pm.rollup({}), "unknown")

    def test_card_items_are_parsed_in_both_formats_and_never_claim_done(self) -> None:
        card = "## 3. Now / Next / Blocked\n\n- NOW: something exists\n- NEXT (x): do it\n- BLOCKED: waits for DEC-X\n\nNow\n- bare now\n\n## 4. Open\n- ignore"
        items = pm.parse_card_items(card)
        self.assertEqual([i["kind"] for i in items], ["NOW", "NEXT", "BLOCKED", "NOW"])
        self.fx.decisions([dec("DEC-X")])
        self.fx.domain("host-core", card)
        m = self.fx.build()
        s = {n["i"]: n["s"] for n in pm.iter_nodes(m["tree"]) if n["k"] == "card"}
        self.assertEqual(s["card:host-core:00"], "unknown")
        self.assertEqual(s["card:host-core:01"], "notstarted")
        self.assertEqual(s["card:host-core:02"], "blocked")


class DecisionPlacement(Base):
    def test_decided_decisions_are_drawn_only_in_areas_without_requirement_rows(self) -> None:
        self.fx.decisions([dec("DEC-FRONTEND-HOSTING", status="decided", decided_on="2026-11-14"), dec("DEC-UPDATE-ENDPOINT", status="decided")])
        m = self.fx.build()
        self.assertEqual(leaf(m, "dec:DEC-FRONTEND-HOSTING")["s"], "designed")  # backend area: no RTM rows
        self.assertIn("nothing is built", m["details"]["dec:DEC-FRONTEND-HOSTING"]["why"])
        self.assertFalse(any(n["i"] == "dec:DEC-UPDATE-ENDPOINT" for n in pm.iter_nodes(m["tree"])))  # updater has RTM modules

    def test_exact_decision_id_beats_pattern(self) -> None:
        self.fx.decisions([dec("DEC-ACCOUNT-ORIGIN"), dec("DEC-STORE-HOSTING"), dec("DEC-ACCOUNT-OTHER")])
        m = self.fx.build()
        under = {a["i"]: {n["i"] for n in pm.iter_nodes(a)} for a in m["tree"]["c"]}
        self.assertIn("dec:DEC-ACCOUNT-ORIGIN", under["area:backend"])
        self.assertIn("dec:DEC-STORE-HOSTING", under["area:backend"])
        self.assertIn("dec:DEC-ACCOUNT-OTHER", under["area:account"])

    def test_banner_decisions_go_to_the_banner_lab_area(self) -> None:
        self.fx.decisions([dec("DEC-BANNER-WORKER", status="decided"), dec("DEC-BANNER-LAB-STYLE", status="decided")])
        m = self.fx.build()
        under = {a["i"]: {n["i"] for n in pm.iter_nodes(a)} for a in m["tree"]["c"]}
        self.assertIn("dec:DEC-BANNER-WORKER", under["area:banner-lab"])
        self.assertIn("dec:DEC-BANNER-LAB-STYLE", under["area:banner-lab"])

    def test_provisional_decision_still_needs_the_owner(self) -> None:
        self.fx.decisions([dec("DEC-ACCOUNT-ORIGIN", status="provisional")])
        m = self.fx.build()
        self.assertEqual(leaf(m, "dec:DEC-ACCOUNT-ORIGIN")["s"], "blocked")
        self.assertEqual([o["id"] for o in m["owner"]], ["DEC-ACCOUNT-ORIGIN"])

    def test_external_draft_copy_yields_to_the_repository_document(self) -> None:
        self.fx.file("docs/design/BACKEND-STACK.md", "# Repo stack")
        ext = self.fx.root / "design"
        ext.mkdir()
        (ext / "BACKEND-STACK.md").write_text("# Draft copy", encoding="utf-8")
        m = self.fx.build(design_dirs=[ext])
        docs = [n for n in pm.iter_nodes(m["tree"]) if n["k"] == "document" and n["i"].startswith("doc:BACKEND-STACK")]
        self.assertEqual([n["t"] for n in docs], ["Repo stack"])


class Rollups(Base):
    def test_totals_and_what_next_order(self) -> None:
        self.fx.file("src/a.rs")
        self.fx.requirements([
            req("CORE-001", priority="extension", target_milestone="M1"),
            req("CORE-002", priority="P0", target_milestone="M3"),
            req("CORE-003", priority="P0", target_milestone="M2", open_decisions=["DEC-X"]),
            req("CORE-004", priority="P0", target_milestone="M2"),
            req("CORE-005", priority="P0", target_milestone="M1", verification={"implementation_refs": ["src/a.rs"], "evidence_refs": ["e"], "new_build_status": "PASS"}),
        ])
        self.fx.decisions([dec("DEC-X")])
        m = self.fx.build()
        order = [x["i"] for x in m["next"]]
        self.assertNotIn("req:CORE-005", order)  # verified work is not "next"
        self.assertEqual(order, ["req:CORE-004", "req:CORE-003", "req:CORE-002", "req:CORE-001"])  # P0 first, then milestone, unblocked before blocked
        self.assertEqual(m["req_totals"]["verified"], 1)
        self.assertEqual(sum(m["req_totals"].values()), 5)

    def test_owner_list_is_separate_sorted_and_carries_the_question(self) -> None:
        self.fx.requirements([req("CORE-001", open_decisions=["DEC-B"]), req("CORE-002", open_decisions=["DEC-B"]), req("CORE-003", open_decisions=["DEC-A"])])
        self.fx.decisions([dec("DEC-A", milestone="M3"), dec("DEC-B", milestone="M3", open_questions=["Which number?"]), dec("DEC-C", owner="architect")])
        m = self.fx.build()
        self.assertEqual([o["id"] for o in m["owner"]], ["DEC-B", "DEC-A"])
        self.assertEqual(m["owner"][0]["question"], "Which number?")
        self.assertEqual(m["owner"][0]["reqs"], 2)

    def test_areas_without_rtm_rows_are_present_and_honest(self) -> None:
        self.fx.file("docs/design/DRAFT-BAR-SPEC.md", "# Draft Bar\n\nstatus: draft\n")
        m = self.fx.build()
        area = leaf(m, "area:draft")
        self.assertEqual(area["s"], "designed")
        self.assertIn("No requirement rows", " ".join(m["details"]["area:draft"]["notes"]))
        self.assertEqual(leaf(m, "area:store")["s"], "unknown")  # nothing at all: unknown, never done

    def test_unmapped_requirement_goes_to_unassigned_not_nowhere(self) -> None:
        self.fx.requirements([req("ZZZ-001", module="zzz")])
        m = self.fx.build()
        self.assertIn("req:ZZZ-001", m["details"])
        self.assertTrue(any(n["i"] == "area:unassigned" for n in pm.iter_nodes(m["tree"])))

    def test_active_claim_leaf_counts_as_in_progress(self) -> None:
        self.fx.claim("c1", "host-core", ["x/**"])
        m = self.fx.build()
        self.assertEqual(m["totals"]["progress"], 1)
        self.assertEqual(m["active_claims"][0]["name"], "c1")

    def test_expired_active_claim_still_counts_and_is_flagged(self) -> None:
        self.fx.claim("old", "host-core", ["x/**"], expires="2026-11-13T08:00:00+07:00")
        m = self.fx.build()
        self.assertTrue(m["active_claims"][0]["expired"])
        self.assertEqual(m["totals"]["progress"], 1)


class Tolerance(Base):
    def test_empty_repository_does_not_crash_and_reports_every_missing_source(self) -> None:
        shutil.rmtree(self.fx.repo / "contracts")
        shutil.rmtree(self.fx.repo / "docs")
        shutil.rmtree(self.fx.coord)
        m = self.fx.build()
        missing = {s["name"] for s in m["sources"] if s["status"] == "missing"}
        for name in ("contracts/requirements.json", "contracts/decisions.json", "coordination claims", "SPRINT-ROADMAP.md (plan)"):
            self.assertIn(name, missing)
        self.assertEqual(sum(m["totals"].values()), 0)
        self.assertEqual(m["tree"]["s"], "unknown")

    def test_corrupt_json_is_reported_not_fatal(self) -> None:
        (self.fx.repo / "contracts" / "requirements.json").write_text("{not json", encoding="utf-8")
        (self.fx.repo / "contracts" / "decisions.json").write_text("[1,2", encoding="utf-8")
        m = self.fx.build()
        errs = {s["name"] for s in m["sources"] if s["status"] == "error"}
        self.assertEqual(errs, {"contracts/requirements.json", "contracts/decisions.json"})

    def test_rtm_csv_is_the_fallback_for_requirements(self) -> None:
        (self.fx.repo / "docs" / "requirements").mkdir(parents=True)
        (self.fx.repo / "docs" / "requirements" / "RTM.csv").write_text(
            "product_need_ids,requirement_id,module,scope,priority,target_milestone,design_refs,contract_refs,spec_refs,required_evidence,implementation_refs,evidence_refs,open_decisions,new_build_status\n"
            "PN-1,CORE-001,core,mvp,P0,M1,docs/design/SDD.md,,,synthetic,src/a.rs;src/b.rs,,,NOT_RUN\n", encoding="utf-8")
        self.fx.file("src/a.rs")
        m = self.fx.build()
        self.assertEqual(leaf(m, "req:CORE-001")["s"], "built")
        self.assertEqual(len(m["details"]["req:CORE-001"]["files"]), 2)

    def test_no_git_still_builds(self) -> None:
        m = self.fx.build()
        self.assertFalse(m["git"]["available"])
        self.assertTrue(any(s["status"] == "skipped" and s["name"] == "git" for s in m["sources"]))

    def test_github_is_optional_and_failure_is_tolerated(self) -> None:
        m = self.fx.build(github=True)
        self.assertEqual(m["prs"], [])
        self.assertTrue(any(s["name"] == "GitHub pull requests" and s["status"] == "error" for s in m["sources"]))

    def test_github_output_is_read_when_available(self) -> None:
        def run(args: list[str], cwd: Path) -> str | None:
            return json.dumps([{"number": 7, "title": "T", "state": "MERGED", "mergedAt": "x", "headRefName": "b", "url": "u"}]) if args[0] == "gh" else None
        m = self.fx.build(github=True, run=run)
        self.assertEqual(m["prs"][0]["number"], 7)

    def test_git_information_and_recent_merges(self) -> None:
        m = self.fx.build(run=fake_git)
        self.assertEqual(m["git"]["sha"], "abc1234")
        self.assertEqual(m["git"]["merged"][0]["pr"], "7")

    def test_gate_log_is_read_when_cached(self) -> None:
        gd = self.fx.root / "gate"
        gd.mkdir()
        (gd / "kohbor-gate-1.log").write_text("noise\nPASS Rust format 1s\nFAIL Something 2s\nNOT FULLY VERIFIED: 1 skipped\n", encoding="utf-8")
        m = pm.build_model(self.fx.repo, self.fx.coord, [], NOW, run=no_git, gate_dir=gd)
        self.assertEqual(len(m["gate"]["lines"]), 3)

    def test_documents_outside_the_repository_are_read_when_present(self) -> None:
        ext = self.fx.root / "design"
        ext.mkdir()
        (ext / "UPDATER-DESIGN.md").write_text("# Auto-update design\n\nStatus: design, NOT implemented\n", encoding="utf-8")
        m = self.fx.build(design_dirs=[ext, self.fx.root / "absent"])
        self.assertEqual(leaf(m, "doc:UPDATER-DESIGN")["s"], "designed")
        self.assertTrue(any(s["name"] == "design documents" and s["status"] == "missing" for s in m["sources"]))


class Rendering(Base):
    def test_same_inputs_give_identical_html(self) -> None:
        self.fx.file("src/a.rs")
        self.fx.requirements([req("CORE-001", verification={"implementation_refs": ["src/a.rs"]}), req("CORE-002")])
        self.fx.decisions([dec("DEC-X")])
        self.fx.claim("c1", "host-core", ["src/**"])
        a = pm.render_html(self.fx.build(run=fake_git))
        b = pm.render_html(self.fx.build(run=fake_git))
        self.assertEqual(a, b)

    def test_only_the_stamp_changes_with_the_clock(self) -> None:
        self.fx.requirements([req("CORE-001")])
        a = pm.render_html(pm.build_model(self.fx.repo, self.fx.coord, [], NOW, run=no_git, gate_dir=self.fx.root / "g"))
        b = pm.render_html(pm.build_model(self.fx.repo, self.fx.coord, [], NOW + dt.timedelta(hours=1), run=no_git, gate_dir=self.fx.root / "g"))
        self.assertNotEqual(a, b)
        strip = lambda s: re.sub(r'"generated_at":"[^"]*"', "", s)
        self.assertEqual(strip(a), strip(b))

    def test_page_is_self_contained_and_data_is_valid_json(self) -> None:
        self.fx.requirements([req("CORE-001", title="</script><b>x</b>")])
        html = pm.render_html(self.fx.build())
        self.assertNotRegex(html, r"""(src|href)\s*=\s*["']https?://""")
        self.assertNotRegex(html, r"@import|<link\b|url\(\s*['\"]?https?:")
        self.assertEqual(html.count("</script>"), 2)  # the data block and the code block; the hostile title is escaped
        data = re.search(r'<script type="application/json" id="data">(.*?)</script>', html, re.S).group(1)
        model = json.loads(data)
        self.assertIn("tree", model)
        self.assertIn("color-scheme:dark", html)  # dark glass by default; light only through the in-page theme button
        self.assertIn('data-theme', html)
        self.assertNotRegex(html, r"https?://(?!www\.w3\.org/2000/svg)")  # no URL at all except the SVG namespace
        for view in ("overview", "map", "board", "decisions", "agents", "builds"):
            self.assertIn('id:"%s"' % view, html)

    def test_stale_threshold_is_embedded_for_the_page(self) -> None:
        m = self.fx.build(stale_hours=3.0)
        self.assertEqual(m["stale_hours"], 3.0)
        self.assertIn("STALE", pm.TEMPLATE_PATH.read_text(encoding="utf-8"))


RECEIPT = """# Test build receipt (outside the repo; evidence class: build + gate only)

- Source: branch b, commit %(commit)s, tree clean when built.
- Exe: F:/kohbor-local/b1/kohbor-desktop.exe, 11 bytes
- SHA-256 kohbor-desktop.exe: %(h1)s
- SHA-256 kohbor-capture-probe.exe (debug): %(h2)s
- Gate: `python tools/gate.py` PASS 12 of 12 on this tree.
- NOT run, unsigned, no installer.
"""


class MissionControlViews(Base):
    def make_builds(self) -> tuple[Path, Path, str, str]:
        builds = self.fx.root / "builds"
        (builds / "b1").mkdir(parents=True)
        (builds / "b2").mkdir(parents=True)
        (builds / "b1" / "kohbor-desktop.exe").write_bytes(b"known build")
        (builds / "b2" / "kohbor-desktop.exe").write_bytes(b"unclaimed build")
        h1 = pm.sha256_file(builds / "b1" / "kohbor-desktop.exe")
        receipts = self.fx.root / "design"
        receipts.mkdir()
        (receipts / "RELEASE-RECEIPT-b1.md").write_text(RECEIPT % {"commit": "a" * 40, "h1": h1, "h2": "c" * 64}, encoding="utf-8")
        return builds, receipts, h1, pm.sha256_file(builds / "b2" / "kohbor-desktop.exe")

    def test_receipt_parsing(self) -> None:
        r = pm.parse_receipt(RECEIPT % {"commit": "b" * 40, "h1": "d" * 64, "h2": "e" * 64}, "x/RELEASE-RECEIPT-z.md")
        self.assertEqual(r["commit"], "b" * 40)
        self.assertEqual(r["exe_paths"], ["F:/kohbor-local/b1/kohbor-desktop.exe"])
        self.assertEqual(r["hashes"]["d" * 64], "kohbor-desktop.exe")
        self.assertEqual(r["hashes"]["e" * 64], "kohbor-capture-probe.exe")
        self.assertTrue(r["gate"].startswith("Gate"))
        self.assertTrue(any("unsigned" in x for x in r["limits"]))

    def test_build_matches_a_receipt_by_hash_only(self) -> None:
        builds, receipts, h1, h2 = self.make_builds()

        def run(args: list[str], cwd: Path) -> str | None:
            return "" if args[:2] == ["git", "--no-optional-locks"] and "rev-list" in args else None
        m = self.fx.build(builds_dir=builds, receipt_dirs=[receipts], run=run)
        by = {b["rel"]: b for b in m["builds"]}
        known, other = by["b1/kohbor-desktop.exe"], by["b2/kohbor-desktop.exe"]
        self.assertEqual(known["sha256"], h1)
        self.assertEqual(known["receipt"]["commit"], "a" * 40)
        self.assertEqual(known["receipt"]["commit_state"], "in origin/main")
        self.assertTrue(known["receipt"]["limits"])
        self.assertIsNone(other["receipt"])  # unknown, never claimed by a receipt
        self.assertEqual(other["sha256"], h2)

    def test_commit_states(self) -> None:
        builds, receipts, _h1, _h2 = self.make_builds()
        not_in_main = self.fx.build(builds_dir=builds, receipt_dirs=[receipts], run=lambda a, c: "deadbee\n" if "rev-list" in a else None)
        self.assertEqual({b["rel"]: b for b in not_in_main["builds"]}["b1/kohbor-desktop.exe"]["receipt"]["commit_state"], "not in origin/main")
        unknown = self.fx.build(builds_dir=builds, receipt_dirs=[receipts], run=no_git)
        self.assertEqual({b["rel"]: b for b in unknown["builds"]}["b1/kohbor-desktop.exe"]["receipt"]["commit_state"], "commit not found in this checkout")

    def test_missing_builds_folder_and_receipts_are_tolerated(self) -> None:
        m = self.fx.build(builds_dir=self.fx.root / "absent")
        self.assertEqual(m["builds"], [])
        self.assertTrue(any(s["name"] == "owner builds" and s["status"] == "missing" for s in m["sources"]))
        none = self.fx.build()
        self.assertTrue(any(s["name"] == "owner builds" and s["status"] == "missing" for s in none["sources"]))

    def test_build_view_is_deterministic(self) -> None:
        builds, receipts, _a, _b = self.make_builds()
        one = pm.render_html(self.fx.build(builds_dir=builds, receipt_dirs=[receipts]))
        two = pm.render_html(self.fx.build(builds_dir=builds, receipt_dirs=[receipts]))
        self.assertEqual(one, two)

    def test_decision_table_covers_every_record_with_the_owner_flag(self) -> None:
        self.fx.decisions([dec("DEC-A", open_questions=["Which?"]), dec("DEC-B", status="decided", decided_on="2026-11-14"), dec("DEC-C", owner="architect"),
                           dec("DEC-D", status="deferred")])
        m = self.fx.build()
        rows = {d["id"]: d for d in m["decisions"]}
        self.assertEqual(set(rows), {"DEC-A", "DEC-B", "DEC-C", "DEC-D"})
        self.assertTrue(rows["DEC-A"]["owner_only"] and rows["DEC-A"]["pending"])
        self.assertEqual(rows["DEC-A"]["question"], "Which?")
        self.assertFalse(rows["DEC-C"]["owner_only"])
        self.assertFalse(rows["DEC-B"]["pending"])
        self.assertEqual(m["decisions"][0]["id"], "DEC-A")  # pending owner decisions first
        self.assertEqual(m["stats"]["decisions_total"], 4)
        self.assertEqual(m["stats"]["owner_needed"], 1)

    def test_claims_view_has_model_branch_and_expiry(self) -> None:
        self.fx.claim("c-live", "host-core", ["x/**"], model="gpt-6-luna (max)", branch="feat/x", expires="2026-11-14T22:00:00+07:00")
        self.fx.claim("c-old", "host-core", ["y/**"], status="done", branch="feat/y")
        self.fx.claim("c-late", "host-core", ["z/**"], expires="2026-11-14T08:00:00+07:00")
        m = self.fx.build()
        by = {c["name"]: c for c in m["claims"]}
        self.assertEqual(by["c-live"]["model"], "gpt-6-luna (max)")
        self.assertEqual(by["c-live"]["hours_left"], 12.0)
        self.assertTrue(by["c-late"]["expired"])
        self.assertLess(by["c-late"]["hours_left"], 0)
        self.assertIsNone(by["c-old"]["hours_left"])
        self.assertEqual([c["name"] for c in m["claims"]][-1], "c-old")  # finished claims after live ones
        self.assertEqual(m["stats"]["claims_expired"], 1)
        self.assertEqual(m["stats"]["claims_active"], 2)

    def test_stats_and_area_bars(self) -> None:
        self.fx.requirements([req("CORE-001", priority="P0"), req("CORE-002")])
        self.fx.claim("c1", "host-core", ["x/**"])
        m = self.fx.build()
        self.assertEqual(m["stats"]["req_total"], 2)
        self.assertEqual(m["stats"]["req_verified"], 0)
        self.assertEqual(m["stats"]["areas_in_progress"], 1)
        self.assertEqual(m["stats"]["areas_total"], len(m["area_bars"]))
        core = next(a for a in m["area_bars"] if a["i"] == "area:core")
        self.assertEqual(sum(core["n"].values()), 3)  # two requirements and the active claim

    def test_activity_groups_merges_by_day(self) -> None:
        def run(args: list[str], cwd: Path) -> str | None:
            if "--first-parent" in args:
                return "a1\t2026-11-14T09:00:00+07:00\tOne (#1)\na2\t2026-11-14T10:00:00+07:00\tTwo (#2)\na3\t2026-11-13T10:00:00+07:00\tThree\n"
            return fake_git(args, cwd)
        m = self.fx.build(run=run)
        self.assertEqual(m["activity"], [{"day": "2026-11-13", "n": 1}, {"day": "2026-11-14", "n": 2}])


ROADMAP = """# Sprint Roadmap

## Phase overview

| Phase | Sprint | Milestone | Outcome | Main model mix |
| --- | --- | --- | --- | --- |
| A. Foundation | 0 | M1 close-out | Foundation outcome | x |
| B. Events | 1 | M2 | Events outcome | x |
| B. Events | 2 | M2 | Package outcome | x |
| C. Vision | 3 | M3 | Vision outcome | x |
| C. Vision | 4 | M3 | Risk outcome | x |
| E. Later | 9+ | M5–M6 | Later outcome | x |

## Sprint 0 — Pilot and M1 close-out

Some prose without a goal line.

## Sprint 1 — Event detector (M2)

**Goal.** Detect events. Requirements: CORE-002, MEDIA-001.

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S1-1 | Tests for MEDIA-001 events | Terra | T-L-R 1 | R1 | synthetic |
| S1-2 | Arbiter | Luna | L-R | R1 | synthetic |
| S1-3 | Never done | Luna | L-R | R1 | synthetic |

- **Owner input.** A capture with a double kill.
- **Exit gate.** All detector tests pass.

## Sprint 2 — Package cache (M2)

**Goal.** Install packs.

## Sprint 3 — Vision (M3)

Requirements: VISION-001.

## Sprint 4 — Risk (M3)

Slices only.

## Phase E — Later milestones

| Milestone | Scope | Planning note |
| --- | --- | --- |
| M5 | VOICE-001..002 | Starts with a spike |
| M6 | SOCIAL-001 | Optional |

## Backlog status

| Task | Status | Report disposition | Commit |
| --- | --- | --- | --- |
| S1-1, S1-2 tests | passed the gate | DONE | `abc1234` |
| S1-3 odd | passed the gate | DONE | `deadbee` |
"""

SDP = """# Software Development Plan (SDP)

## Delivery strategy

| Milestone | Deliverable | Entry dependencies | Exit gate |
| --- | --- | --- | --- |
| M1 | Local startup | M0 | Cold offline startup |
| M2 | Pack cache and events | M1 | Preemption fixtures |

## Definition of ready

A task has requirement IDs.

## Definition of done

All linked requirements have implementation evidence.

## Backlog record template
"""


class Roadmap(Base):
    def setup_plan(self) -> None:
        self.fx.file("docs/delivery/SPRINT-ROADMAP.md", ROADMAP)
        self.fx.file("docs/delivery/SDP.md", SDP)
        self.fx.file("docs/quality/TEST-PLAN.md", "**TP-01 — Startup and platform.** CORE-001. Launch the app.\n")
        self.fx.file("docs/quality/UAT-PLAN.md", "### UAT-01 — Start independently\n\nThe player starts it.\n")

    def reqs(self) -> None:
        self.fx.file("src/a.rs")
        self.fx.file("crates/x/tests/t.rs")
        self.fx.file("docs/design/SDD.md")
        accept = {"scenario_id": "AC-1", "given": "g", "when": "w", "then": "the then clause"}
        self.fx.requirements([
            req("CORE-001", target_milestone="M1", verification={"procedure_ids": ["TP-01", "UAT-01"], "implementation_refs": ["src/a.rs", "crates/x/tests/t.rs"]},
                acceptance=accept, design_refs=["docs/design/SDD.md"]),
            req("CORE-002", target_milestone="M1"),
            req("MEDIA-001", module="media", target_milestone="M2"),
            req("MEDIA-002", module="media", target_milestone="M2"),
            req("VISION-001", module="vision", target_milestone="M3"),
            req("VISION-002", module="vision", target_milestone="M3"),
            req("VOICE-001", module="voice", target_milestone="M5"),
            req("VOICE-002", module="voice", target_milestone="M5", priority="extension"),
            req("SOCIAL-001", module="social", target_milestone="M6"),
            req("PEER-001", module="peer", target_milestone="M6"),
            req("UPDATE-001", module="update", target_milestone="M8"),
        ])

    def leaves(self, node: dict) -> list[str]:
        return [n["i"][7:] for n in pm.iter_nodes(node) if n["k"] == "task"]

    def find(self, m: dict, nid: str) -> dict:
        return next(n for n in pm.iter_nodes(m["roadmap"]) if n["i"] == nid)

    def test_id_and_milestone_ranges(self) -> None:
        self.assertEqual(pm.expand_ids("VOICE-001..003 and CORE-001–002, ZZZ-001", {"VOICE-001", "VOICE-002", "VOICE-003", "CORE-001", "CORE-002", "CORE-009"}),
                         ["VOICE-001", "VOICE-002", "VOICE-003", "CORE-001", "CORE-002"])
        self.assertEqual(pm.all_ms("M5–M8"), ["M5", "M6", "M7", "M8"])
        self.assertEqual(pm.all_ms("M1 close-out"), ["M1"])

    def test_every_requirement_is_placed_exactly_once(self) -> None:
        self.setup_plan()
        self.reqs()
        m = self.fx.build()
        ids = self.leaves(m["roadmap"])
        self.assertEqual(sorted(ids), sorted(q["id"] for q in [{"id": i} for i in ("CORE-001", "CORE-002", "MEDIA-001", "MEDIA-002", "VISION-001", "VISION-002",
                                                                                      "VOICE-001", "VOICE-002", "SOCIAL-001", "PEER-001", "UPDATE-001")]))
        self.assertEqual(sum(m["roadmap"]["n"].values()), 11)

    def test_placement_rules(self) -> None:
        self.setup_plan()
        self.reqs()
        m = self.fx.build()
        why = lambda rid: m["roadmap_details"]["rm:req:" + rid]["placement"]
        self.assertIn("named in the Sprint 1", why("MEDIA-001"))
        self.assertIn("named in the Sprint 1", why("CORE-002"))
        self.assertIn("the only sprint of milestone M1", why("CORE-001"))  # sprint 0 is the only M1 sprint; its header has no (M..), the phase table gives M1
        self.assertIn("several sprints share milestone M2", why("MEDIA-002"))
        self.assertIn("named in the Sprint 3", why("VISION-001"))
        self.assertIn("several sprints share milestone M3", why("VISION-002"))  # sprints 3 and 4 share M3: not guessed
        self.assertIn("named in the Phase E table (M5)", why("VOICE-001"))
        self.assertIn("scope text does not name it", why("PEER-001"))  # M6 row exists but does not name PEER-001
        self.assertIn("its milestone is not in the plan", why("UPDATE-001"))  # M8 is nowhere in the plan

    def test_phase_sprint_task_hierarchy_and_counts(self) -> None:
        self.setup_plan()
        self.reqs()
        m = self.fx.build()
        phases = [(n["i"], n["t"]) for n in m["roadmap"]["c"]]
        self.assertEqual(phases[0], ("rm:phase:A", "A. Foundation"))
        sprint1 = self.find(m, "rm:sprint:1")
        self.assertEqual(sorted(self.leaves(sprint1)), ["CORE-002", "MEDIA-001"])
        self.assertEqual(sum(sprint1["n"].values()), 2)
        self.assertEqual(m["roadmap"]["n"], {k: sum(c["n"][k] for c in m["roadmap"]["c"]) for k in m["roadmap"]["n"]})

    def test_progress_counts_verified_separately_from_built(self) -> None:
        self.setup_plan()
        self.fx.file("src/a.rs")
        self.fx.requirements([
            req("MEDIA-001", module="media", target_milestone="M2", verification={"implementation_refs": ["src/a.rs"], "evidence_refs": ["e.json"], "new_build_status": "PASS"}),
            req("MEDIA-002", module="media", target_milestone="M2", verification={"implementation_refs": ["src/a.rs"]}),
        ])
        m = self.fx.build()
        self.assertEqual((m["roadmap"]["n"]["verified"], m["roadmap"]["n"]["built"]), (1, 1))
        self.assertEqual(m["roadmap_details"]["rm:all"]["verified"], 1)
        self.assertEqual(m["roadmap_details"]["rm:all"]["built"], 1)

    def test_task_criteria_are_read_never_invented(self) -> None:
        self.setup_plan()
        self.reqs()
        m = self.fx.build()
        d = m["roadmap_details"]["rm:req:CORE-001"]
        self.assertEqual(d["acceptance"]["then"], "the then clause")
        self.assertEqual(d["success"], "the then clause")
        self.assertIn("implementation evidence", d["dod"])
        self.assertEqual(d["exit"]["milestone"], "Cold offline startup")
        self.assertIsNone(d["exit"]["sprint"])  # sprint 0 has no Exit gate line: unknown, not filled in
        self.assertEqual([p["id"] for p in d["links"]["procedures"]], ["TP-01", "UAT-01"])
        self.assertEqual(d["links"]["procedures"][0]["title"], "Startup and platform")
        self.assertEqual(d["links"]["tests"], ["crates/x/tests/t.rs"])
        self.assertEqual(d["links"]["code"], ["src/a.rs"])
        empty = m["roadmap_details"]["rm:req:MEDIA-002"]
        self.assertIsNone(empty["acceptance"])
        self.assertIsNone(empty["success"])
        self.assertEqual({c["label"]: c["state"] for c in empty["checks"]}["Acceptance scenario defined"], "unknown")

    def test_checkboxes_tick_only_with_data(self) -> None:
        self.setup_plan()
        self.reqs()
        d = self.fx.build()["roadmap_details"]["rm:req:CORE-001"]
        by = {c["label"]: c["state"] for c in d["checks"]}
        self.assertEqual(by["Acceptance scenario defined"], "done")
        self.assertEqual(by["Implementation files exist"], "done")
        self.assertEqual(by["Evidence class synthetic: receipt"], "open")  # no evidence_refs: never ticked
        self.assertEqual(by["Evidence class native: receipt"], "open")
        self.assertEqual(by["Verification procedures named and found"], "done")
        passing = self.fx.build()  # nothing changes without a receipt
        self.assertNotIn("done", [c["state"] for c in passing["roadmap_details"]["rm:req:CORE-002"]["checks"] if c["label"].startswith("Evidence")])

    def test_evidence_checkbox_ticks_with_a_receipt(self) -> None:
        self.setup_plan()
        self.fx.file("src/a.rs")
        self.fx.requirements([req("CORE-001", target_milestone="M1", verification={"implementation_refs": ["src/a.rs"], "evidence_refs": ["r.json"], "new_build_status": "PASS"})])
        d = self.fx.build()["roadmap_details"]["rm:req:CORE-001"]
        self.assertTrue(all(c["state"] == "done" for c in d["checks"] if c["label"].startswith("Evidence")))

    def test_sprint_work_items_tick_only_with_an_existing_commit(self) -> None:
        self.setup_plan()
        self.reqs()

        def git_with_abc(args: list[str], cwd: Path) -> str | None:
            if "cat-file" in args:
                return "commit\n" if args[-1] == "abc1234" else None
            return None
        m = self.fx.build(run=git_with_abc)
        items = {i["id"]: i for i in m["roadmap_details"]["rm:sprint:1"]["work_items"]}
        self.assertEqual(items["S1-1"]["state"], "done")
        self.assertEqual(items["S1-3"]["state"], "unknown")  # status says passed but its commit is not in this checkout
        self.assertIn("none of its commits is found", items["S1-3"]["basis"])
        nogit = self.fx.build()
        self.assertEqual({i["state"] for i in nogit["roadmap_details"]["rm:sprint:1"]["work_items"]}, {"unknown"})
        self.assertEqual(m["roadmap_details"]["rm:req:MEDIA-001"]["sprint_tasks"][0]["id"], "S1-1")

    def test_sprint_and_phase_details(self) -> None:
        self.setup_plan()
        self.reqs()
        m = self.fx.build()
        s1 = m["roadmap_details"]["rm:sprint:1"]
        self.assertEqual(s1["exit"], "All detector tests pass.")
        self.assertEqual(s1["owner_input"], "A capture with a double kill.")
        self.assertEqual(s1["milestone_info"]["exit"], "Preemption fixtures")
        s2 = m["roadmap_details"]["rm:sprint:2"]
        self.assertIsNone(s2["exit"])
        self.assertEqual(s2["goal"], "Install packs.")
        s0 = m["roadmap_details"]["rm:sprint:0"]
        self.assertEqual((s0["goal"], s0["goal_source"]), ("Foundation outcome", "the Outcome column of the phase overview"))
        phase_b = m["roadmap_details"]["rm:phase:B"]
        self.assertEqual([x["sprint"] for x in phase_b["outcomes"]], ["1", "2"])

    def test_missing_plan_documents_leave_everything_unknown_not_invented(self) -> None:
        self.reqs()
        m = self.fx.build()
        self.assertEqual(sum(m["roadmap"]["n"].values()), 11)  # tasks still come from the registry
        d = m["roadmap_details"]["rm:req:CORE-001"]
        self.assertIsNone(d["dod"])
        self.assertEqual(d["exit"], {"sprint": None, "milestone": None})
        self.assertTrue(any(s["status"] == "missing" and s["name"] == "SDP.md" for s in m["sources"]))
        self.assertTrue(any(s["status"] == "missing" and s["name"] == "SPRINT-ROADMAP.md (plan)" for s in m["sources"]))
        self.assertTrue(all(n["i"].startswith("rm:bucket:unplaced") or n["i"] in ("rm:all",) or n["k"] == "task" for n in pm.iter_nodes(m["roadmap"])))

    def test_changelog_commits_and_lines(self) -> None:
        self.setup_plan()
        self.reqs()
        self.fx.file("docs/governance/CHANGELOG.md", "- CORE-001 now starts offline.\n- unrelated\n- CORE-0012 is another id\n")

        def run(args: list[str], cwd: Path) -> str | None:
            return "abc1234\t2026-11-14T09:00:00+07:00\tAdd startup\n" if "log" in args and "--format=%h%x09%cI%x09%s" in args else None
        d = self.fx.build(run=run)["roadmap_details"]["rm:req:CORE-001"]
        self.assertEqual(d["changelog"]["commits"], [{"sha": "abc1234", "time": "2026-11-14T09:00:00+07:00", "title": "Add startup"}])
        self.assertEqual(d["changelog"]["lines"], ["- CORE-001 now starts offline."])

    def test_roadmap_view_is_deterministic(self) -> None:
        self.setup_plan()
        self.reqs()
        self.assertEqual(pm.render_html(self.fx.build(run=fake_git)), pm.render_html(self.fx.build(run=fake_git)))


class Cli(Base):
    def run_cli(self, *extra: str) -> tuple[int, str]:
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = pm.main(["--repo", str(self.fx.repo), "--coord", str(self.fx.coord), "--design-dir", str(self.fx.root / "none"),
                            "--builds-dir", str(self.fx.root / "nobuilds"), *extra])
        return code, out.getvalue()

    def test_writes_one_file_to_out(self) -> None:
        self.fx.requirements([req("CORE-001")])
        dest = self.fx.root / "deep" / "map.html"
        code, text = self.run_cli("--out", str(dest), "--now", "2026-11-14T10:00:00+07:00")
        self.assertEqual(code, 0)
        self.assertTrue(dest.is_file())
        self.assertIn("progress_map: wrote", text)
        self.assertEqual(list(dest.parent.iterdir()), [dest])  # no temp file left behind

    def test_check_mode(self) -> None:
        dest = self.fx.root / "map.html"
        code, _ = self.run_cli("--out", str(dest), "--check")
        self.assertEqual(code, 1)  # missing
        # a page built from another commit is stale
        dest.write_text('..."head_sha":"deadbee","x":1...', encoding="utf-8")
        self.assertEqual(pm.embedded_sha(dest), "deadbee")

    def test_json_mode_prints_the_model(self) -> None:
        code, text = self.run_cli("--json", "--now", "2026-11-14T10:00:00+07:00")
        self.assertEqual(code, 0)
        self.assertIn('"tree"', text)

    def test_the_tool_never_writes_inside_the_repository(self) -> None:
        self.fx.requirements([req("CORE-001")])
        before = sorted(p.relative_to(self.fx.repo).as_posix() for p in self.fx.repo.rglob("*"))
        self.run_cli("--out", str(self.fx.root / "out.html"))
        after = sorted(p.relative_to(self.fx.repo).as_posix() for p in self.fx.repo.rglob("*"))
        self.assertEqual(before, after)


class Helpers(unittest.TestCase):
    def test_ranks(self) -> None:
        self.assertLess(pm.prio_rank("P0"), pm.prio_rank("extension"))
        self.assertLess(pm.prio_rank("extension"), pm.prio_rank("later"))
        self.assertLess(pm.ms_rank("M4a"), pm.ms_rank("M4b"))
        self.assertLess(pm.ms_rank("M4b"), pm.ms_rank("M5"))
        self.assertEqual(pm.ms_rank("unscheduled"), 99.0)

    def test_claim_paths(self) -> None:
        c = {"write_area": ["crates/kohbor-vision/**", "apps\\desktop\\src\\App.tsx"]}
        self.assertTrue(pm.claim_hits_path(c, "crates/kohbor-vision/src/lib.rs"))
        self.assertTrue(pm.claim_hits_path(c, "apps/desktop/src/App.tsx"))
        self.assertTrue(pm.claim_hits_path(c, "crates"))  # a listed directory that contains the claim area
        self.assertFalse(pm.claim_hits_path(c, "crates/kohbor-audio/src/lib.rs"))
        self.assertFalse(pm.claim_hits_path({"write_area": []}, "anything"))

    def test_claim_cites_whole_ids_only(self) -> None:
        self.assertTrue(pm.claim_cites({"note": "does CORE-001."}, "CORE-001"))
        self.assertFalse(pm.claim_cites({"note": "CORE-0012"}, "CORE-001"))
        self.assertFalse(pm.claim_cites({"note": "XCORE-001"}, "CORE-001"))

    def test_exists_rejects_escapes(self) -> None:
        d = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, d, True)
        self.assertFalse(pm.exists(d, "../x"))
        self.assertFalse(pm.exists(d, str(d)))
        self.assertFalse(pm.exists(d, ""))

    def test_split_row_keeps_pipes_inside_code_spans(self) -> None:
        self.assertEqual(pm.split_row("| a | `x | y` | c |"), ["a", "`x | y`", "c"])


if __name__ == "__main__":
    unittest.main()
