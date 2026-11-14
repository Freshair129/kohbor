"""Tests for tools/validate_agents.py (registry schema 2.0.0, router, generated tables). Synthetic fixtures only."""

from __future__ import annotations

import contextlib
import copy
import io
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import validate_agents as va  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]

REGISTRY_MD = """# Registry

| id | tier | Card | Owns | Notes |
| --- | --- | --- | --- | --- |
| `alpha` | A | [card](alpha/CARD.md) | `crates/alpha` | test |
| `beta` | B | [card](beta/CARD.md) | `lab:x` | test |
| `gamma` | C | answers: `alpha` | nothing | test |
| `delta` | C | none | nothing | test |
"""


def ev(level="owner_chosen", relayed=False, source="docs/source.md"):
    return {"level": level, "source": source, "date": "2026-11-13", "relayed": relayed}


BASE = {
    "schema_version": "2.0.0",
    "evidence_levels": ["excluded", "proposed", "in_use_unconfirmed", "owner_chosen"],
    "stationed_agents": 0,
    "profiles": [
        {"id": "p-write", "tool": "codex-exec", "model": "writer-model", "effort": "max", "evidence": ev()},
        {"id": "p-review", "tool": "codex-exec", "model": "review-model", "effort": "high", "note": "effort is a default", "evidence": ev()},
        {"id": "p-weak", "tool": "codex-exec", "model": "weak-model", "effort": None, "evidence": ev("in_use_unconfirmed")},
        {"id": "p-session", "tool": "claude-code-session", "model": "session", "effort": None, "evidence": ev()},
    ],
    "roles": [
        {"id": "owner", "kind": "human", "may_claim": True, "may_edit_cards": True, "may_commit": True},
        {"id": "lead", "kind": "agent", "may_claim": True, "may_edit_cards": True, "may_commit": True},
        {"id": "writer", "kind": "agent", "may_claim": False, "may_edit_cards": False, "may_commit": False},
        {"id": "reviewer", "kind": "agent", "may_claim": False, "may_edit_cards": False, "may_commit": False},
        {"id": "helper", "kind": "agent", "may_claim": False, "may_edit_cards": False, "may_commit": False},
    ],
    "task_types": [
        {"id": "implement", "writes": True, "pattern": ["L-R"], "needs_review": "R1", "meaning": "x"},
        {"id": "review", "writes": False, "pattern": ["R"], "needs_review": "none", "meaning": "x"},
        {"id": "ask", "writes": False, "pattern": ["Q"], "needs_review": "none", "meaning": "x"},
        {"id": "integrate", "writes": True, "pattern": ["lead"], "needs_review": "none", "meaning": "x"},
    ],
    "routes": [
        {"task_type": "implement", "domain": "*", "role": "writer", "profile": "p-write", "reviewer": {"role": "reviewer", "profile": "p-review"}, "evidence": ev()},
        {"task_type": "implement", "domain": "beta", "role": "helper", "profile": "p-weak", "reviewer": None, "evidence": ev("in_use_unconfirmed")},
        {"task_type": "review", "domain": "*", "role": "reviewer", "profile": "p-review", "reviewer": None, "evidence": ev()},
        {"task_type": "ask", "domain": "*", "role": "helper", "profile": "p-weak", "reviewer": None, "evidence": ev()},
        {"task_type": "integrate", "domain": "*", "role": "lead", "profile": "p-session", "reviewer": None, "evidence": ev()},
    ],
    "domain_policies": [
        {"domain": "alpha", "agent_work": "normal", "allowed_task_types": [], "source": "DEC-ONE", "notes": "alpha note"},
        {"domain": "gamma", "agent_work": "redirect", "allowed_task_types": [], "source": "docs/source.md", "notes": "gamma rights note"},
        {"domain": "delta", "agent_work": "design-only", "allowed_task_types": ["ask", "integrate"], "source": "DEC-ONE"},
    ],
    "owner_reserved": {"source": "AGENTS.md", "ids": ["taste"], "meaning": {"taste": "owner only"}},
    "exclusions": [],
}


class Fixture:
    def __init__(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="vag-test-"))
        self.root = self.dir / "repo"
        self.write("docs/domains/README.md", REGISTRY_MD)
        self.write("contracts/decisions.json", json.dumps({"decisions": [{"id": "DEC-ONE"}]}))
        self.write("docs/source.md", "# s\n")
        self.write("AGENTS.md", "# a\n")
        self.write("docs/domains/alpha/state.json", json.dumps({"requirement_ids": [], "crates": ["alpha"], "write_areas": ["crates/alpha/**"]}))
        self.write("docs/domains/beta/state.json", json.dumps({"requirement_ids": [], "planned_crates": ["beta-next"], "write_areas": ["lab:x/**"]}))
        self.write("crates/alpha/lib.rs", "// x\n")
        self.registry(BASE)

    def write(self, rel: str, text: str) -> None:
        p = self.root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8", newline="\n")

    def registry(self, data: dict) -> None:
        self.write(va.REGISTRY, json.dumps(data, indent=1))
        try:
            tables = va.render_tables(data)
        except (KeyError, TypeError):
            tables = "(incomplete registry)"
        self.write(va.DOC, "# Doc\n\n" + va.BEGIN + "\n" + tables + "\n" + va.END + "\n")

    def run(self, *extra: str):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = va.main(["--root", str(self.root), *extra])
        return code, out.getvalue()

    def route(self, task: str, domain: str | None = None, reg: dict | None = None) -> dict:
        return va.resolve(reg or BASE, va.domain_info(self.root), task, domain)


class RegistryCheckTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fx = Fixture()
        self.addCleanup(shutil.rmtree, self.fx.dir, True)

    def problem(self, mutate, needle: str) -> None:
        data = copy.deepcopy(BASE)
        mutate(data)
        self.fx.registry(data)
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)  # report-only never fails
        self.assertIn(needle, out)
        code, out = self.fx.run("--enforce")
        self.assertEqual(code, 1, out)

    def test_baseline_is_clean(self) -> None:
        code, out = self.fx.run("--enforce")
        self.assertEqual(code, 0, out)
        self.assertIn("0 problem(s)", out)

    def test_evidence_must_be_complete(self) -> None:
        self.problem(lambda d: d["profiles"][0]["evidence"].update(level="great"), "unknown evidence level 'great'")
        self.problem(lambda d: d["profiles"][0]["evidence"].update(source=""), "evidence has no source")
        self.problem(lambda d: d["profiles"][0]["evidence"].update(date="yesterday"), "evidence date must be YYYY-MM-DD")
        self.problem(lambda d: d["profiles"][0]["evidence"].update(relayed="no"), "'relayed' must be true or false")
        self.problem(lambda d: d["routes"][0].pop("evidence"), "evidence is missing")

    def test_unknown_references_are_problems(self) -> None:
        self.problem(lambda d: d["routes"][0].update(task_type="fly"), "route fly/*: unknown task type")
        self.problem(lambda d: d["routes"][0].update(domain="nowhere"), "unknown domain")
        self.problem(lambda d: d["routes"][0].update(role="ghost"), "role 'ghost' is not defined")
        self.problem(lambda d: d["routes"][0].update(profile="ghost"), "profile 'ghost' is not defined")
        self.problem(lambda d: d["routes"][0]["reviewer"].update(role="ghost"), "reviewer role 'ghost' is not defined")
        self.problem(lambda d: d["routes"][0].update(alternatives=[{"role": "ghost", "profile": "p-write"}]), "refers to an undefined role or profile")
        self.problem(lambda d: d["domain_policies"][0].update(domain="nowhere"), "policy nowhere: unknown domain")
        self.problem(lambda d: d["domain_policies"][0].update(allowed_task_types=["fly"]), "unknown task type 'fly'")

    def test_duplicates_and_reserved_ids(self) -> None:
        self.problem(lambda d: d["routes"].append(copy.deepcopy(d["routes"][0])), "is defined twice")
        self.problem(lambda d: d["profiles"].append(copy.deepcopy(d["profiles"][0])), "profile id p-write is defined twice")
        self.problem(lambda d: d["owner_reserved"]["ids"].append("implement"), "owner-reserved id implement is also a task type")

    def test_reviewer_model_separation(self) -> None:
        self.problem(lambda d: d["routes"][0]["reviewer"].update(profile="p-write"), "reviewer uses the same model as the author")
        self.problem(lambda d: d["routes"][0]["reviewer"].update(role="writer"), "the author role reviews itself")
        data = copy.deepcopy(BASE)
        data["routes"][0]["reviewer"]["profile"] = "p-session"
        self.fx.registry(data)
        code, out = self.fx.run("--enforce")
        self.assertEqual(code, 0, out)  # unknown is a note, not a pass and not a failure
        self.assertIn("UNVERIFIABLE", out)
        self.assertEqual(self.fx.route("implement", "alpha", data)["reviewer"]["model_separation"].split()[0], "UNVERIFIABLE")

    def test_agent_route_without_profile_must_be_proposed(self) -> None:
        self.problem(lambda d: d["routes"][3].update(profile=None), "an agent route without a profile must be 'proposed'")
        self.problem(lambda d: d["routes"].append({"task_type": "review", "domain": "alpha", "role": "owner", "profile": "p-write", "reviewer": None, "evidence": ev()}), "a human role cannot have a profile")

    def test_policy_must_cite_an_existing_source(self) -> None:
        self.problem(lambda d: d["domain_policies"][0].update(source=""), "the source must cite an existing DEC id or repository document")
        self.problem(lambda d: d["domain_policies"][0].update(source="DEC-GHOST"), "dangling decision id DEC-GHOST")
        self.problem(lambda d: d["domain_policies"][0].update(source="docs/nothing.md"), "the source must cite")
        self.problem(lambda d: d["domain_policies"][0].update(agent_work="sometimes"), "unknown agent_work")

    def test_redirect_needs_an_answering_domain(self) -> None:
        self.problem(lambda d: d["domain_policies"].append({"domain": "delta", "agent_work": "redirect", "allowed_task_types": [], "source": "DEC-ONE"}) if False else d["domain_policies"][2].update(agent_work="redirect"), "redirect needs the domain registry row")

    def test_exclusions_are_checked(self) -> None:
        self.problem(lambda d: d.update(exclusions=[{"profile": "ghost", "task_type": "implement", "domain": "alpha", "source": "x"}]), "refers to an undefined profile")

    def test_policy_source_must_cite_only_existing_documents(self) -> None:
        self.problem(lambda d: d["domain_policies"][0].update(source="docs/source.md; docs/missing.md"), "cites docs/missing.md, which does not exist")

    def test_generated_tables_and_counts_must_match(self) -> None:
        data = copy.deepcopy(BASE)
        data["profiles"][0]["effort"] = "changed"
        self.fx.write(va.REGISTRY, json.dumps(data))
        code, out = self.fx.run("--enforce")
        self.assertEqual(code, 1, out)
        self.assertIn("generated tables differ", out)
        self.assertEqual(self.fx.run("--write-doc")[0], 0)
        self.assertEqual(self.fx.run("--enforce")[0], 0)
        text = (self.fx.root / va.DOC).read_text(encoding="utf-8")
        self.assertIn("Registry content: 4 profiles, 5 roles (1 human, 4 agent), 4 task types, 5 routes, 3 domain policies.", text)

    def test_unreadable_or_old_registry_is_reported_not_raised(self) -> None:
        self.fx.write(va.REGISTRY, "{not json")
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)
        self.assertIn("cannot read", out)
        self.fx.write(va.REGISTRY, json.dumps({"schema_version": "1.0.0"}))
        self.assertIn("expected 2.0.0", self.fx.run()[1])


class ResolveTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fx = Fixture()
        self.addCleanup(shutil.rmtree, self.fx.dir, True)

    def test_effective_level_is_the_weaker_of_route_and_profile(self) -> None:
        data = copy.deepcopy(BASE)
        a = self.fx.route("ask", "alpha", data)
        self.assertEqual((a["level"], a["usable"]), ("in_use_unconfirmed", False))  # route owner_chosen, profile weaker
        self.assertIn("needs an owner instruction", a["message"])
        b = self.fx.route("implement", "alpha", data)
        self.assertEqual((b["level"], b["usable"], b["role"], b["model"], b["effort"]), ("owner_chosen", True, "writer", "writer-model", "max"))
        data["routes"][0]["evidence"]["level"] = "proposed"
        self.assertEqual(self.fx.route("implement", "alpha", data)["level"], "proposed")

    def test_relayed_is_shown(self) -> None:
        data = copy.deepcopy(BASE)
        data["routes"][0]["evidence"]["relayed"] = True
        self.assertTrue(self.fx.route("implement", "alpha", data)["relayed"])

    def test_most_specific_route_wins(self) -> None:
        self.assertEqual(self.fx.route("implement", "beta")["role"], "helper")
        self.assertEqual(self.fx.route("implement", "alpha")["role"], "writer")

    def test_sandbox_is_derived_from_the_task_type_and_domain(self) -> None:
        self.assertEqual(self.fx.route("review", "alpha")["sandbox"], "read-only")
        self.assertEqual(self.fx.route("implement", "alpha")["sandbox"], "workspace-write")
        self.assertEqual(self.fx.route("implement", "beta")["sandbox"], "workspace-write inside one lab folder")

    def test_unknown_domain_and_task_are_errors(self) -> None:
        self.assertEqual(self.fx.route("implement", "nowhere")["status"], "error")
        self.assertIn("unknown domain", self.fx.route("implement", "nowhere")["message"])
        self.assertIn("unknown task type", self.fx.route("fly", "alpha")["message"])

    def test_tier_c_redirects_and_keeps_both_policy_notes(self) -> None:
        r = self.fx.route("implement", "gamma")
        self.assertEqual((r["status"], r["answering_domain"], r["redirected_from"], r["role"]), ("ok", "alpha", "gamma", "writer"))
        self.assertTrue(any("gamma rights note" in n for n in r["notes"]))
        self.assertTrue(any("alpha note" in n for n in r["notes"]))

    def test_policy_is_applied_before_the_route(self) -> None:
        r = self.fx.route("implement", "delta")
        self.assertEqual((r["status"], r["usable"], r["role"]), ("blocked-by-policy", False, "owner"))
        self.assertIn("design-only", r["message"])
        self.assertEqual(self.fx.route("ask", "delta")["status"], "ok")

    def test_owner_reserved_is_owner_only(self) -> None:
        r = self.fx.route("taste", "alpha")
        self.assertEqual((r["status"], r["role"], r["usable"]), ("owner-only", "owner", False))

    def test_exclusion_sets_the_level_to_excluded(self) -> None:
        data = copy.deepcopy(BASE)
        data["exclusions"] = [{"profile": "p-write", "task_type": "implement", "domain": "alpha", "source": "docs/source.md"}]
        r = self.fx.route("implement", "alpha", data)
        self.assertEqual((r["level"], r["usable"]), ("excluded", False))
        self.assertEqual(self.fx.route("implement", "beta", data)["level"], "in_use_unconfirmed")

    def test_no_route_means_ask_the_owner(self) -> None:
        data = copy.deepcopy(BASE)
        data["routes"] = [r for r in data["routes"] if r["task_type"] != "review"]
        self.assertEqual(self.fx.route("review", "alpha", data)["status"], "no-route")

    def test_cli_route_prints_json(self) -> None:
        code, out = self.fx.run("--route", "implement", "--domain", "alpha")
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out)["model"], "writer-model")
        self.assertEqual(self.fx.run("--route", "implement", "--domain", "nowhere", "--enforce")[0], 1)

    def test_incomplete_evidence_is_never_usable(self) -> None:
        data = copy.deepcopy(BASE)
        data["routes"][0]["evidence"]["source"] = ""
        a = self.fx.route("implement", "alpha", data)
        self.assertEqual((a["level"], a["usable"]), ("owner_chosen", False))
        self.assertTrue(any("evidence is incomplete" in n for n in a["notes"]))
        data = copy.deepcopy(BASE)
        data["profiles"][0]["evidence"]["date"] = "soon"
        self.assertFalse(self.fx.route("implement", "alpha", data)["usable"])
        data = copy.deepcopy(BASE)
        data["routes"][0]["profile"] = "ghost"
        self.assertFalse(self.fx.route("implement", "alpha", data)["usable"])

    def test_null_profile_null_source_and_undefined_role_are_never_usable(self) -> None:
        data = copy.deepcopy(BASE)
        data["routes"][0]["profile"] = None
        self.assertFalse(self.fx.route("implement", "alpha", data)["usable"])
        data = copy.deepcopy(BASE)
        data["routes"][0]["evidence"]["source"] = None
        self.assertFalse(self.fx.route("implement", "alpha", data)["usable"])
        data = copy.deepcopy(BASE)
        data["routes"][0]["role"] = "ghost"
        a = self.fx.route("implement", "alpha", data)
        self.assertEqual((a["status"], a["usable"]), ("error", False))
        data = copy.deepcopy(BASE)
        del data["routes"][0]["evidence"]
        self.assertFalse(self.fx.route("implement", "alpha", data)["usable"])

    def test_redirect_chains_cycles_and_dangling_targets(self) -> None:
        domains = va.domain_info(self.fx.root)
        domains["hop1"] = {"tier": "C", "answers_for": "gamma", "external": False}
        a = va.resolve(BASE, domains, "implement", "hop1")
        self.assertEqual((a["status"], a["answering_domain"], a["redirect_chain"]), ("ok", "alpha", ["hop1", "gamma", "alpha"]))
        domains["loop"] = {"tier": "C", "answers_for": "loop", "external": False}
        self.assertIn("redirect cycle", va.resolve(BASE, domains, "implement", "loop")["message"])
        domains["dangling"] = {"tier": "C", "answers_for": "ghost", "external": False}
        self.assertIn("is not a domain", va.resolve(BASE, domains, "implement", "dangling")["message"])
        problems, _ = va.check(self.fx.root)  # the fixture registry itself has no cycle
        self.assertEqual(problems, [])

    def test_policies_and_exclusions_of_every_domain_on_the_chain_apply(self) -> None:
        data = copy.deepcopy(BASE)
        data["domain_policies"][1].update(agent_work="questions-only", allowed_task_types=["ask"])  # gamma: redirects to alpha AND restricts itself
        a = self.fx.route("implement", "gamma", data)
        self.assertEqual(a["status"], "blocked-by-policy")
        self.assertIn("domain gamma policy", a["message"])
        data = copy.deepcopy(BASE)
        data["exclusions"] = [{"profile": "p-write", "task_type": "implement", "domain": "gamma", "source": "docs/source.md"}]
        self.assertEqual(self.fx.route("implement", "gamma", data)["level"], "excluded")

    def test_redirect_cycle_in_the_domain_registry_is_a_problem(self) -> None:
        self.fx.write("docs/domains/README.md", REGISTRY_MD.replace("answers: `alpha`", "answers: `gamma`"))
        code, out = self.fx.run()
        self.assertIn("redirect cycle", out)



class ShippedRegistryTests(unittest.TestCase):
    """The ten answers of AGENT-REGISTRY-DESIGN section 2.3 against the registry that ships in the repository."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.reg = va.load(REPO_ROOT)
        cls.domains = va.domain_info(REPO_ROOT)

    def r(self, task: str, domain: str | None = None) -> dict:
        return va.resolve(self.reg, self.domains, task, domain)

    def test_the_shipped_registry_has_no_problem(self) -> None:
        problems, _ = va.check(REPO_ROOT)
        self.assertEqual(problems, [])

    def test_implement_packs(self) -> None:
        a = self.r("implement", "packs")
        self.assertEqual((a["role"], a["model"], a["effort"], a["sandbox"], a["level"], a["usable"]), ("general-worker", "gpt-6-luna", "max", "workspace-write", "owner_chosen", True))
        self.assertEqual((a["reviewer"]["role"], a["reviewer"]["model"], a["reviewer"]["model_separation"]), ("test-worker", "gpt-6.1-terra", "differs"))

    def test_review_diff_events_audio(self) -> None:
        a = self.r("review-diff", "events-audio")
        self.assertEqual((a["role"], a["model"], a["effort"], a["sandbox"], a["usable"]), ("test-worker", "gpt-6.1-terra", "high", "read-only", True))
        self.assertTrue(any("lead's default" in n for n in a["notes"]))

    def test_code_question_host_core(self) -> None:
        a = self.r("code-question", "host-core")
        self.assertEqual((a["role"], a["model"], a["effort"], a["sandbox"], a["usable"]), ("general-worker", "gpt-6-luna", "max", "read-only", True))

    def test_lab_build_banner_lab(self) -> None:
        a = self.r("lab-build", "banner-lab")
        self.assertEqual((a["role"], a["model"], a["effort"], a["level"], a["usable"]), ("lab-worker", "gpt-6.1-sol", "xhigh", "owner_chosen", True))  # owner decision 2026-11-14 (DEC-BANNER-WORKER)
        self.assertEqual(a["reviewer"]["profile"], "terra-high")
        self.assertEqual(a["reviewer"]["model_separation"], "differs")

    def test_lab_review_is_still_unconfirmed_and_astra_is_not_the_banner_default(self) -> None:
        a = self.r("lab-review", "banner-lab")
        self.assertEqual((a["model"], a["level"], a["usable"]), ("gpt-6.1-sol", "in_use_unconfirmed", False))
        self.assertIn("owner instruction", a["message"])

    def test_implement_commerce_is_blocked(self) -> None:
        a = self.r("implement", "commerce")
        self.assertEqual((a["status"], a["role"], a["usable"]), ("blocked-by-policy", "owner", False))

    def test_design_plan_governance(self) -> None:
        a = self.r("design-plan", "governance")
        self.assertIn(a["role"], ("lead", "design-consultant"))
        self.assertEqual((a["model"], a["effort"], a["level"], a["relayed"], a["usable"]), ("claude-fable-5-2", "xhigh", "owner_chosen", True, True))
        self.assertTrue(any("foundation work" in n for n in a["notes"]))
        self.assertEqual(self.r("design-plan", "packs")["level"], "in_use_unconfirmed")

    def test_implement_announcer_voice_redirects_and_keeps_rights_with_the_owner(self) -> None:
        a = self.r("implement", "announcer-voice")
        self.assertEqual((a["answering_domain"], a["redirected_from"]), ("banner-lab", "announcer-voice"))
        self.assertTrue(any("rights" in n.lower() and "owner-only" in n for n in a["notes"]))
        self.assertEqual(a["status"], "blocked-by-policy")  # repository workers do not write in the lab

    def test_implement_unknown_domain_is_an_error(self) -> None:
        self.assertEqual(self.r("implement", "no-such-domain")["status"], "error")

    def test_taste_is_the_owner(self) -> None:
        self.assertEqual((self.r("taste")["status"], self.r("taste")["role"]), ("owner-only", "owner"))

    def test_consult_account(self) -> None:
        a = self.r("consult", "account")
        self.assertEqual((a["role"], a["sandbox"], a["level"], a["usable"]), ("consultant", "read-only", "in_use_unconfirmed", False))

    def test_every_route_of_the_registry_resolves_or_is_blocked_on_purpose(self) -> None:
        for rt in self.reg["routes"]:
            domain = None if rt["domain"] == "*" else rt["domain"]
            self.assertIn(self.r(rt["task_type"], domain)["status"], ("ok", "blocked-by-policy"), rt)

    def test_no_route_is_usable_without_owner_evidence(self) -> None:
        for dom in self.domains:
            for tt in self.reg["task_types"]:
                a = self.r(tt["id"], dom)
                if a["status"] == "ok" and a["usable"]:
                    self.assertEqual(a["level"], "owner_chosen", (tt["id"], dom))


SAMPLE = REPO_ROOT / "tools" / "tests" / "fixtures" / "brief-implement.txt"


class BriefTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.reg = va.load(REPO_ROOT)
        cls.domains = va.domain_info(REPO_ROOT)
        cls.sample = SAMPLE.read_text(encoding="utf-8")

    def lint(self, **changes: str | None) -> list[str]:
        brief = va.parse_brief(self.sample)
        for k, v in changes.items():
            if v is None:
                brief.pop(k, None)
            else:
                brief[k] = v
        return va.lint_brief(self.reg, self.domains, brief)[0]

    def test_sample_brief_matches_its_route(self) -> None:
        self.assertEqual(self.lint(), [])

    def test_parse_reads_key_lines_and_the_first_occurrence_wins(self) -> None:
        b = va.parse_brief("TASK_ID: A\nnot a key\nMODEL:   gpt-6-luna (max)\nTASK_ID: B\n  INDENTED: x\n")
        self.assertEqual(b, {"TASK_ID": "A", "MODEL": "gpt-6-luna (max)"})

    def test_wrong_model_effort_or_role_names_the_route(self) -> None:
        out = self.lint(MODEL="gpt-6.1-terra (high)")
        self.assertTrue(any("MODEL gpt-6.1-terra differs from gpt-6-luna" in p and "profile luna-max" in p for p in out), out)
        self.assertTrue(any("effort high differs from max" in p for p in self.lint(MODEL="gpt-6-luna (high)")))
        self.assertTrue(any("does not state the effort max" in p for p in self.lint(MODEL="gpt-6-luna")))
        self.assertTrue(any("ROLE test-worker does not match the route general-worker" in p for p in self.lint(ROLE="test-worker")))

    def test_missing_lines_cannot_be_checked(self) -> None:
        for key in ("ROLE", "TASK_TYPE", "DOMAIN", "MODEL"):
            with self.subTest(key=key):
                self.assertTrue(any(f"no {key} line" in p for p in self.lint(**{key: None})))

    def test_unusable_or_blocked_routes_are_mismatches(self) -> None:
        out = self.lint(DOMAIN="banner-lab", TASK_TYPE="lab-review", ROLE="lab-reviewer", MODEL="gpt-6.1-sol (xhigh)")
        self.assertTrue(any("needs an owner instruction before use" in p for p in out), out)
        astra = self.lint(DOMAIN="banner-lab", TASK_TYPE="lab-build", ROLE="lab-worker", MODEL="gpt-6-astra (high)")
        self.assertTrue(any("gpt-6-astra differs from gpt-6.1-sol" in p for p in astra), astra)  # astra is no longer the banner build default
        self.assertTrue(any("blocked-by-policy" in p for p in self.lint(DOMAIN="commerce")))
        self.assertTrue(any("unknown domain" in p for p in self.lint(DOMAIN="nowhere")))

    def test_read_only_task_cannot_ask_for_write_scope(self) -> None:
        out = self.lint(TASK_TYPE="code-question", SCOPE="crates/kohbor-pack/src/**   (write)")
        self.assertTrue(any("read-only" in p for p in out), out)

    def test_an_alternative_role_needs_an_owner_instruction(self) -> None:
        out = self.lint(DOMAIN="governance", TASK_TYPE="governance-change", ROLE="general-worker", MODEL="gpt-6-luna (max)")
        self.assertTrue(any("uses an alternative of the route" in p for p in out), out)

    def test_model_line_must_name_exactly_one_model(self) -> None:
        for bad in ("gpt-6-luna (max) MALICIOUS", "gpt-6-luna (max) | gpt-6.1-terra (high)", "gpt-6-luna (max); rm -rf x"):
            with self.subTest(bad=bad):
                self.assertTrue(any("must name exactly one model" in p for p in self.lint(MODEL=bad)), bad)

    def test_read_only_task_must_not_name_a_scope_to_change(self) -> None:
        self.assertEqual(self.lint(TASK_TYPE="review-diff", ROLE="test-worker", MODEL="gpt-6.1-terra (high)", SCOPE="none: you change nothing"), [])
        out = self.lint(TASK_TYPE="review-diff", ROLE="test-worker", MODEL="gpt-6.1-terra (high)", SCOPE="edit tools/example.py")
        self.assertTrue(any("is read-only" in p for p in out), out)

    def test_launch_quotes_the_workspace(self) -> None:
        brief = va.parse_brief(self.sample)
        brief["WORKSPACE"] = "F:/my work; echo injected"
        cmd, _ = va.launch_command(self.reg, self.domains, brief, "F:/kohbor")
        self.assertIn("-C 'F:/my work; echo injected' ", cmd)
        brief["WORKSPACE"] = "F:/plain"
        self.assertIn("-C F:/plain ", va.launch_command(self.reg, self.domains, brief, "F:/kohbor")[0])

    def test_none_followed_by_more_scope_is_still_a_write_scope(self) -> None:
        base = dict(TASK_TYPE="review-diff", ROLE="test-worker", MODEL="gpt-6.1-terra (high)")
        for bad in ("none, edit tools/claim.py", "n/a, tools/claim.py", "none: crates/a/x.rs"):
            with self.subTest(bad=bad):
                self.assertTrue(any("adds something to change" in p for p in self.lint(SCOPE=bad, **base)), bad)
        self.assertEqual(self.lint(SCOPE="none: you change nothing", **base), [])

    def test_missing_brief_file_exits_nonzero(self) -> None:
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(va.main(["--root", str(REPO_ROOT), "--brief", str(SAMPLE.with_name("no-such-brief.txt"))]), 1)
            self.assertEqual(va.main(["--root", str(REPO_ROOT), "--launch", str(SAMPLE.with_name("no-such-brief.txt"))]), 1)
        self.assertIn("MISMATCH cannot lint the brief", out.getvalue())

    def test_cli_brief_exit_codes(self) -> None:
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(va.main(["--root", str(REPO_ROOT), "--brief", str(SAMPLE)]), 0)
        self.assertIn("brief S9-T1: OK", out.getvalue())
        bad = Path(tempfile.mkdtemp(prefix="brief-")) / "bad.txt"
        self.addCleanup(shutil.rmtree, bad.parent, True)
        bad.write_text(self.sample.replace("gpt-6-luna (max)", "gpt-6.1-sol (xhigh)"), encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(va.main(["--root", str(REPO_ROOT), "--brief", str(bad)]), 1)
        self.assertIn("MISMATCH MODEL gpt-6.1-sol differs", out.getvalue())

    def test_launch_prints_the_command_and_runs_nothing(self) -> None:
        import os
        import subprocess

        def boom(*a, **k):
            raise AssertionError("--launch must not start a process")

        saved = (subprocess.run, subprocess.Popen, os.system)
        subprocess.run, subprocess.Popen, os.system = boom, boom, boom
        try:
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                code = va.main(["--root", str(REPO_ROOT), "--launch", str(SAMPLE)])
        finally:
            subprocess.run, subprocess.Popen, os.system = saved
        self.assertEqual(code, 0)
        cmd = out.getvalue().strip()
        for part in ("codex exec", "-m gpt-6-luna", "model_reasoning_effort=max", "-s workspace-write", "-C F:/kohbor", "--output-schema tools/agent/worker-report.schema.json", "S9-T1.report.json"):
            self.assertIn(part, cmd)

    def test_launch_uses_read_only_for_read_only_tasks_and_refuses_other_tools(self) -> None:
        brief = va.parse_brief(self.sample)
        brief.update(TASK_TYPE="code-question")
        cmd, _ = va.launch_command(self.reg, self.domains, brief, "F:/kohbor")
        self.assertIn("-s read-only", cmd)
        brief.update(DOMAIN="governance", TASK_TYPE="design-plan", ROLE="lead", MODEL="claude-fable-5-2 (xhigh)")
        cmd, warns = va.launch_command(self.reg, self.domains, brief, "F:/kohbor")
        self.assertIsNone(cmd)
        self.assertTrue(any("not codex exec" in w for w in warns))

    def test_validator_source_starts_no_process(self) -> None:
        import ast

        tree = ast.parse((Path(__file__).resolve().parents[1] / "validate_agents.py").read_text(encoding="utf-8"))
        names = {n.id for n in ast.walk(tree) if isinstance(n, ast.Name)} | {n.attr for n in ast.walk(tree) if isinstance(n, ast.Attribute)}
        self.assertFalse(names & {"subprocess", "Popen", "system", "popen", "exec", "spawn"}, names & {"subprocess", "Popen", "system"})


if __name__ == "__main__":
    unittest.main()
