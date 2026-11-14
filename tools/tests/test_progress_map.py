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
        for name in ("contracts/requirements.json", "contracts/decisions.json", "coordination claims", "SPRINT-ROADMAP.md"):
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

    def test_board_matches_the_counted_totals(self) -> None:
        self.fx.file("src/a.rs")
        self.fx.requirements([req("CORE-001", verification={"implementation_refs": ["src/a.rs"]}), req("CORE-002"), req("CORE-003", open_decisions=["DEC-X"])])
        self.fx.decisions([dec("DEC-X")])
        self.fx.domain("host-core", "## 3. Now / Next / Blocked\n\n- NOW: a statement\n")
        m = self.fx.build()
        for s in pm.STATUSES:
            self.assertEqual(sum(c["n"] for c in m["board"][s["id"]]), m["totals"][s["id"]], s["id"])
        self.assertEqual(sum(sum(c["n"] for c in col) for col in m["board"].values()), sum(m["totals"].values()))  # uncounted card lines are not on the board

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
