"""Tests for tools/validate_domains.py. Synthetic fixtures only; run with: python -m unittest discover -s tools/tests"""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import validate_domains as vd  # noqa: E402

CARD = """---
domain: alpha
tier: A
card_version: 1
verified_at_commit: abc1234
verified_on: 2026-11-13
verified_by: test
---

# Alpha

## 1. Purpose and boundary

Owns alpha. Governed by DEC-ONE and MEDIA-001.

## 3. Now / Next / Blocked

- NOW: something (DEC-ONE).

## 4. Open questions

none
"""

REGISTRY = """# Registry

| id | tier | Card | Owns | Notes |
| --- | --- | --- | --- | --- |
| `alpha` | A | [card](alpha/CARD.md) | `crates/alpha` | test |
| `beta` | B | [card](beta/CARD.md) | nothing | test |
| `gamma` | C | answers: `alpha` | nothing | test |
"""


def sha(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


class Fixture:
    def __init__(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="vd-test-"))
        self.root = self.dir / "repo"
        self.write("contracts/requirements.json", json.dumps({"requirements": [{"id": "MEDIA-001"}, {"id": "CORE-001"}]}))
        self.write("contracts/decisions.json", json.dumps({"decisions": [{"id": "DEC-ONE"}, {"id": "DEC-TWO"}]}))
        self.write("docs/delivery/SPRINT-ROADMAP.md", "| Task | Work |\n| S1-1 `x` | y |\n| S2-3a z | y |\n")
        self.write("docs/INDEX.md", "# Index\n\n[Roadmap](delivery/SPRINT-ROADMAP.md)\n[Doc A](doc-a.md)\n[Registry](domains/README.md)\n[Readme](../README.md)\n")
        self.write("README.md", "# Readme\n")
        self.write("docs/doc-a.md", "# A\n")
        self.write("docs/domains/README.md", REGISTRY)
        self.write("docs/domains/alpha/CARD.md", CARD)
        self.write("docs/domains/beta/CARD.md", CARD.replace("domain: alpha", "domain: beta").replace("tier: A", "tier: B").replace("# Alpha", "# Beta"))
        self.state("alpha", artefacts=[{"path": "docs/doc-a.md", "type": "spec", "role": "governing", "reviewed_sha256": sha("# A\n")}])
        self.state("beta", tier="B", artefacts=[{"path": "docs/doc-a.md", "type": "spec", "role": "reference"}])

    def write(self, rel: str, text: str) -> None:
        p = self.root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8", newline="\n")

    def state(self, dom: str, tier: str = "A", **over) -> None:
        data = {
            "schema_version": "1.0.0", "domain": dom, "tier": tier, "verified_at_commit": "abc1234",
            "verified_on": "2026-11-13", "verified_by": "test", "write_areas": [], "requirement_ids": ["MEDIA-001"],
            "decision_ids": ["DEC-ONE"], "task_id_prefixes": ["S1-"], "artefacts": [], "open_questions": [],
        }
        data.update(over)
        self.write(f"docs/domains/{dom}/state.json", json.dumps(data, indent=2))

    def run(self, *extra: str):
        out = io.StringIO()
        argv = ["--root", str(self.root), "--no-git", "--coord", str(self.dir / "no-coord"), *extra]
        with contextlib.redirect_stdout(out):
            code = vd.main(argv)
        return code, out.getvalue()

    def close(self) -> None:
        shutil.rmtree(self.dir, ignore_errors=True)


class ValidateDomainsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fx = Fixture()
        self.addCleanup(self.fx.close)

    def assertFails(self, needle: str, *extra: str) -> None:
        code, out = self.fx.run(*extra)
        self.assertEqual(code, 1, out)
        self.assertIn(needle, out)

    def test_baseline_passes(self) -> None:
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)
        self.assertIn("PASS", out)

    def test_unindexed_document_fails(self) -> None:
        self.fx.write("docs/orphan.md", "# Orphan\n")
        self.assertFails("docs/orphan.md: not indexed")

    def test_removing_an_index_row_fails(self) -> None:
        self.fx.write("docs/other.md", "# Other\n")
        index = (self.fx.root / "docs/INDEX.md").read_text(encoding="utf-8")
        self.fx.write("docs/INDEX.md", index + "[Other](other.md)\n")
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)
        self.fx.write("docs/INDEX.md", index)
        self.assertFails("docs/other.md: not indexed")

    def test_artefact_of_a_card_counts_as_indexed(self) -> None:
        self.fx.write("docs/orphan.md", "# Orphan\n")
        self.fx.state("alpha", artefacts=[{"path": "docs/orphan.md", "type": "spec", "role": "governing"},
                                          {"path": "docs/doc-a.md", "type": "spec", "role": "governing"}])
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)

    def test_dangling_decision_in_state_fails(self) -> None:
        self.fx.state("alpha", decision_ids=["DEC-ONE", "DEC-MISSING"])
        self.assertFails("dangling decision id DEC-MISSING")

    def test_dangling_decision_in_card_text_fails(self) -> None:
        self.fx.write("docs/domains/alpha/CARD.md", CARD.replace("MEDIA-001.", "MEDIA-001 and DEC-GHOST."))
        self.assertFails("dangling decision id DEC-GHOST")

    def test_dangling_requirement_fails_but_proposed_id_is_allowed(self) -> None:
        self.fx.write("docs/domains/alpha/CARD.md", CARD.replace("MEDIA-001.", "MEDIA-001 and ACCOUNT-001."))
        self.assertFails("dangling requirement id ACCOUNT-001")
        self.fx.state("alpha", proposed_requirement_ids=["ACCOUNT-001"],
                      artefacts=[{"path": "docs/doc-a.md", "type": "spec", "role": "governing", "reviewed_sha256": sha("# A\n")}])
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)

    def test_unknown_requirement_in_state_fails(self) -> None:
        self.fx.state("alpha", requirement_ids=["MEDIA-999"])
        self.assertFails("dangling requirement id MEDIA-999")

    def test_unknown_task_prefix_fails(self) -> None:
        self.fx.state("alpha", task_id_prefixes=["S9-"])
        self.assertFails("task prefix 'S9-' is empty, malformed or matches no backlog task")

    def test_missing_artefact_fails_unless_planned(self) -> None:
        self.fx.state("beta", tier="B", artefacts=[{"path": "crates/none", "type": "code", "role": "reference"}])
        self.assertFails("crates/none' does not exist")
        self.fx.state("beta", tier="B", artefacts=[{"path": "crates/none", "type": "code", "role": "reference", "exists": False}])
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)

    def test_two_governing_cards_fail(self) -> None:
        self.fx.state("beta", tier="B", artefacts=[{"path": "docs/doc-a.md", "type": "spec", "role": "governing"}])
        self.assertFails("exactly one primary domain")

    def test_unknown_neighbour_fails(self) -> None:
        self.fx.state("beta", tier="B", artefacts=[{"path": "docs/doc-a.md", "type": "spec", "role": "neighbour:nowhere"}])
        self.assertFails("unknown neighbour domain")

    def test_missing_card_or_state_fails(self) -> None:
        (self.fx.root / "docs/domains/beta/state.json").unlink()
        self.assertFails("beta: state.json is missing")

    def test_folder_without_registry_row_fails(self) -> None:
        self.fx.write("docs/domains/delta/CARD.md", CARD)
        self.assertFails("docs/domains/delta: folder has no registry row")

    def test_tier_mismatch_fails(self) -> None:
        self.fx.state("beta", tier="A")
        self.assertFails("differs from the registry tier")

    def test_stale_card_is_flagged_and_can_fail(self) -> None:
        self.fx.write("docs/doc-a.md", "# A changed\n")
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)
        self.assertIn("STALE alpha: STALE (docs/doc-a.md changed since review)", out)
        self.assertFails("STALE alpha", "--fail-on-stale")

    def test_secrets_and_private_paths_in_cards_fail(self) -> None:
        for bad, needle in [
            ("Contact owner@example.com.", "email address"),
            ("See https://example.invalid/api for it.", "URL or endpoint"),
            ("Folder F:\\work\\thing holds it.", "drive-letter path"),
            ("Path C:\\Users\\someone\\x.", "drive-letter path"),
            ("Key sk-abcdefghijklmnopqrstuvwx.", "token-shaped string"),
            ("Host 10.1.2.3 serves it.", "IP address"),
            ("Token " + "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8S9t0U1v2W3x4Y5", "long opaque string"),
        ]:
            with self.subTest(bad=bad):
                self.fx.write("docs/domains/alpha/CARD.md", CARD.replace("Owns alpha.", "Owns alpha. " + bad))
                code, out = self.fx.run()
                self.assertEqual(code, 1, out)
                self.assertIn(needle, out)

    def test_secret_in_state_fails(self) -> None:
        self.fx.state("alpha", verified_by="ops@example.com")
        self.assertFails("email address")

    def test_repo_paths_and_hashes_do_not_trip_the_lint(self) -> None:
        self.fx.write("docs/domains/alpha/CARD.md", CARD.replace("Owns alpha.", "Owns crates/kohbor-audio-native/src/lib.rs and apps/desktop/src-tauri/src/host.rs."))
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)

    def test_card_size_limits(self) -> None:
        many = "\n".join(f"- NOW: line {i} (DEC-ONE)" for i in range(13))
        self.fx.write("docs/domains/alpha/CARD.md", CARD.replace("- NOW: something (DEC-ONE).", many))
        self.assertFails("'Now / Next / Blocked' has 13 lines")

    def test_logical_root_is_external_not_a_failure(self) -> None:
        self.fx.state("alpha", artefacts=[{"path": "lab:fable/README.md", "type": "spec", "role": "governing"},
                                          {"path": "docs/doc-a.md", "type": "spec", "role": "governing"}])
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)
        self.assertIn("1 artefact(s) under external root 'lab:', not checked here", out)

    def test_dangling_decision_in_open_question_fails(self) -> None:
        self.fx.state("alpha", open_questions=[{"id": "Q1", "owner": "x", "raised": "2026-11-13", "text": "Needs DEC-NOPE?"}],
                      artefacts=[{"path": "docs/doc-a.md", "type": "spec", "role": "governing", "reviewed_sha256": sha("# A\n")}])
        self.assertFails("dangling decision id DEC-NOPE")

    GOV = [{"path": "docs/doc-a.md", "type": "spec", "role": "governing", "reviewed_sha256": sha("# A\n")}]

    def test_unknown_logical_root_fails(self) -> None:
        self.fx.state("alpha", artefacts=self.GOV + [{"path": "zz:nothing/here.md", "type": "spec", "role": "reference"}])
        self.assertFails("unknown logical root 'zz:'")

    def test_artefact_path_cannot_leave_the_repository(self) -> None:
        for bad in ("../outside.md", "docs/../../outside.md", "docs\\..\\..\\x.md"):
            with self.subTest(bad=bad):
                self.fx.state("alpha", artefacts=self.GOV + [{"path": bad, "type": "spec", "role": "reference"}])
                self.assertFails("has '..' or leaves the repository")

    def test_other_spellings_of_one_governing_file_are_caught(self) -> None:
        for spelling in ("./docs/doc-a.md", "docs\\doc-a.md", "docs/DOC-A.md", "docs//doc-a.md"):
            with self.subTest(spelling=spelling):
                self.fx.state("beta", tier="B", artefacts=[{"path": spelling, "type": "spec", "role": "governing"}])
                code, out = self.fx.run()
                if sys.platform == "win32" or spelling != "docs/DOC-A.md":
                    self.assertEqual(code, 1, out)
                    self.assertIn("exactly one primary domain", out)

    def test_id_shapes_that_used_to_slip_through(self) -> None:
        for bad, needle in [("MEDIA-0999", "canonical requirement id"), ("media-999", "canonical requirement id"),
                            ("dec-ghost", "dangling decision id dec-ghost"), ("DEC-Ghost", "dangling decision id DEC-Ghost"),
                            ("SEC-001", "unknown family"), ("task S9-1", "matches no backlog task")]:
            with self.subTest(bad=bad):
                self.fx.write("docs/domains/alpha/CARD.md", CARD.replace("MEDIA-001.", "MEDIA-001 and " + bad + "."))
                self.assertFails(needle)

    def test_allow_listed_id_families_and_task_sub_ids_pass(self) -> None:
        self.fx.write("docs/domains/alpha/CARD.md", CARD.replace("MEDIA-001.", "MEDIA-001, SHA-256, PN-009, task S1-1b and S2-3a."))
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)

    def test_more_secret_shapes_fail(self) -> None:
        for bad, needle in [("sk_" + "live_" + "51Habcdefghijklmnopqrstuv", "token-shaped"), ("AIzaSyA1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q", "token-shaped"),
                            ("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abc", "token-shaped"), ("password: hunter2hunter2", "credential assignment"),
                            ("\\\\fileserver\\share", "UNC"), ("%USERPROFILE%\\y", "UNC"), ("~/secret", "UNC"), ("/root/.ssh/id_rsa", "UNC"),
                            ("file:///etc/passwd", "UNC"), ("a" * 20 + "0123456789abcdef" * 3, "long hex")]:
            with self.subTest(bad=bad):
                self.fx.write("docs/domains/alpha/CARD.md", CARD.replace("Owns alpha.", "Owns alpha. " + bad))
                code, out = self.fx.run()
                self.assertEqual(code, 1, out)
                self.assertIn(needle, out)

    def test_long_repo_path_is_not_a_secret(self) -> None:
        path = "src-tauri/src/media/inventory/announcer_pack_loader_module/very_long_file_name.rs"
        self.fx.write("docs/domains/alpha/CARD.md", CARD.replace("Owns alpha.", "Owns " + path + "."))
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)

    def test_markdown_anywhere_and_link_titles(self) -> None:
        self.fx.write("tools/NOTES.md", "# Notes\n")
        self.assertFails("tools/NOTES.md: not indexed")
        self.fx.write("tools/NOTES.md", "# Notes\n")
        index = (self.fx.root / "docs/INDEX.md").read_text(encoding="utf-8")
        self.fx.write("docs/INDEX.md", index + '[Notes](../tools/NOTES.md "a title")\n')
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)

    def test_bom_in_card_is_tolerated(self) -> None:
        (self.fx.root / "docs/domains/alpha/CARD.md").write_bytes(b"\xef\xbb\xbf" + CARD.encode("utf-8"))
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)

    def test_card_link_must_be_its_own_card(self) -> None:
        self.fx.write("docs/domains/README.md", REGISTRY.replace("alpha/CARD.md", "../../outside.md"))
        self.assertFails("card link of alpha must be alpha/CARD.md")

    def test_card_text_link_cannot_leave_the_repository(self) -> None:
        self.fx.write("docs/domains/alpha/CARD.md", CARD.replace("Owns alpha.", "See [o](../../../../outside.md)."))
        self.assertFails("leaves the repository")

    def test_machine_path_artefact_fails(self) -> None:
        self.fx.state("alpha", artefacts=[{"path": "F:/somewhere/x.md", "type": "spec", "role": "reference"},
                                          {"path": "docs/doc-a.md", "type": "spec", "role": "governing"}])
        self.assertFails("is a machine path")

    def test_tier_c_must_not_have_a_card_link(self) -> None:
        self.fx.write("docs/domains/README.md", REGISTRY.replace("answers: `alpha`", "[card](gamma/CARD.md)"))
        self.assertFails("tier C and must not have a card link")

    def test_expired_claim_warns_without_failing(self) -> None:
        coord = self.fx.dir / "coord"
        (coord / "claims").mkdir(parents=True)
        (coord / "claims" / "alpha__t.json").write_text(json.dumps({"status": "active", "expires": "2020-01-01T00:00:00+00:00"}), encoding="utf-8")
        (coord / "claims" / "beta__t.json").write_text('{"status": "active", "expires": "2020-01-01T00:00:00+00:00", "w": "F:\\x"}', encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = vd.main(["--root", str(self.fx.root), "--no-git", "--coord", str(coord)])
        self.assertEqual(code, 0, out.getvalue())
        self.assertIn("WARN claim alpha__t.json: expired", out.getvalue())
        self.assertIn("not valid JSON", out.getvalue())

    def test_json_output(self) -> None:
        code, out = self.fx.run("--json")
        self.assertEqual(code, 0)
        data = json.loads(out)
        self.assertEqual(data["status"], "PASS")
        self.assertIn("documents only", data["scope"])


@unittest.skipIf(shutil.which("git") is None or os.environ.get("KOHBOR_TOOL_TESTS_SKIP_GIT") == "1", "git is not available or the git tests are skipped (gate --quick)")
class GitFreshnessTests(unittest.TestCase):
    def test_commits_on_write_areas_make_a_card_stale(self) -> None:
        fx = Fixture()
        self.addCleanup(fx.close)

        def git(*args: str) -> str:
            return subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "core.autocrlf=false", *args], cwd=fx.root,
                                  capture_output=True, text=True, check=True).stdout

        fx.write("crates/alpha/lib.rs", "// 1\n")
        git("init", "-q")
        git("add", "-A")
        git("commit", "-q", "-m", "init")
        base = git("rev-parse", "--short", "HEAD").strip()
        fx.state("alpha", verified_at_commit=base, write_areas=["crates/alpha/**"],
                 artefacts=[{"path": "docs/doc-a.md", "type": "spec", "role": "governing", "reviewed_sha256": sha("# A\n")}])
        git("add", "-A")
        git("commit", "-q", "-m", "state")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = vd.main(["--root", str(fx.root), "--coord", str(fx.dir / "none"), "--stale-commit-limit", "0"])
        self.assertEqual(code, 0, out.getvalue())
        self.assertNotIn("STALE", out.getvalue())
        fx.write("crates/alpha/lib.rs", "// 2\n")
        git("add", "-A")
        git("commit", "-q", "-m", "change")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = vd.main(["--root", str(fx.root), "--coord", str(fx.dir / "none"), "--stale-commit-limit", "0"])
        self.assertEqual(code, 0, out.getvalue())
        self.assertIn("1 commits touched the write areas", out.getvalue())

    def test_untracked_document_is_checked_too(self) -> None:
        fx = Fixture()
        self.addCleanup(fx.close)
        subprocess.run(["git", "init", "-q"], cwd=fx.root, check=True)
        subprocess.run(["git", "-c", "core.autocrlf=false", "add", "-A"], cwd=fx.root, check=True, capture_output=True)
        fx.write("docs/new-untracked.md", "# New\n")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = vd.main(["--root", str(fx.root), "--coord", str(fx.dir / "none")])
        self.assertEqual(code, 1, out.getvalue())
        self.assertIn("docs/new-untracked.md: not indexed", out.getvalue())


    def test_non_ascii_document_names_are_listed_unquoted(self) -> None:
        fx = Fixture()
        self.addCleanup(fx.close)
        subprocess.run(["git", "init", "-q"], cwd=fx.root, check=True)
        fx.write("docs/ไทย.md", "# Thai\n")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = vd.main(["--root", str(fx.root), "--coord", str(fx.dir / "none")])
        self.assertEqual(code, 1, out.getvalue())
        self.assertIn("docs/ไทย.md: not indexed", out.getvalue())


TAX_REGISTRY = """# Registry

| id | tier | Card | Owns | Notes | Default modules |
| --- | --- | --- | --- | --- | --- |
| `alpha` | A | [card](alpha/CARD.md) | `crates/alpha` | test | none |
| `beta` | B | [card](beta/CARD.md) | nothing | test | none |
| `gamma` | C | answers: `alpha` | nothing | test | peer, QUALITY-006 |
| `delta` | C | none | nothing | test | voice |
"""

TAX_REQS = [
    {"id": "MEDIA-001", "module": "media", "primary_design_component_id": "CMP-MEDIA", "target_milestone": "M2"},
    {"id": "CORE-001", "module": "core", "primary_design_component_id": "CMP-STARTUP", "target_milestone": "M1"},
    {"id": "PEER-001", "module": "peer", "primary_design_component_id": "CMP-PEER", "target_milestone": "M6"},
    {"id": "VOICE-001", "module": "voice", "primary_design_component_id": "CMP-VOICE", "target_milestone": "M5"},
    {"id": "QUALITY-006", "module": "quality", "primary_design_component_id": "CMP-PERFORMANCE", "target_milestone": "M5"},
]


class TaxonomyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fx = Fixture()
        self.addCleanup(self.fx.close)
        self.fx.write("contracts/requirements.json", json.dumps({"requirements": TAX_REQS}))
        self.fx.write("docs/domains/README.md", TAX_REGISTRY)
        art = [{"path": "docs/doc-a.md", "type": "spec", "role": "governing", "reviewed_sha256": sha("# A\n")}]
        self.fx.state("alpha", requirement_ids=["MEDIA-001", "CORE-001"], write_areas=["crates/alpha/**"], crates=["alpha"], artefacts=art)
        self.fx.state("beta", tier="B", requirement_ids=[], interface_requirement_ids=["MEDIA-001"], planned_crates=["beta-next"])
        self.fx.write("crates/alpha/lib.rs", "// x\n")

    def map(self) -> dict:
        code, out = self.fx.run("--map", "--json")
        self.assertEqual(code, 0, out)
        return json.loads(out)

    def test_primary_domain_rules_and_a_clean_map(self) -> None:
        rep = self.map()
        got = {rid: (t["primary"], t["status"]) for rid, t in rep["requirements"].items()}
        self.assertEqual(got, {"MEDIA-001": ("alpha", "card"), "CORE-001": ("alpha", "card"), "PEER-001": ("gamma", "default"),
                               "VOICE-001": ("delta", "default"), "QUALITY-006": ("gamma", "default")})
        self.assertEqual(rep["requirement_status"], {"card": 2, "default": 3})
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)
        self.assertNotIn("WARN", out)

    def test_conflict_and_unassigned_are_warnings_not_failures(self) -> None:
        self.fx.state("beta", tier="B", requirement_ids=["MEDIA-001"])
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)
        self.assertIn("WARN requirement MEDIA-001 has no single primary domain (candidates: alpha, beta)", out)
        self.fx.write("docs/domains/README.md", TAX_REGISTRY.replace("peer, QUALITY-006", "QUALITY-006"))
        code, out = self.fx.run()
        self.assertIn("WARN requirement PEER-001 (peer) is UNASSIGNED", out)
        self.assertEqual(code, 0, out)

    def test_two_default_rows_for_one_module_conflict(self) -> None:
        self.fx.write("docs/domains/README.md", TAX_REGISTRY.replace("| none |\n| `delta`", "| none |\n| `delta`").replace("| test | voice |", "| test | voice, peer |"))
        code, out = self.fx.run()
        self.assertIn("WARN requirement PEER-001 has no single primary domain (candidates: delta, gamma)", out)

    def test_interface_ids_are_checked(self) -> None:
        self.fx.state("beta", tier="B", requirement_ids=[], interface_requirement_ids=["MEDIA-999"])
        code, out = self.fx.run()
        self.assertEqual(code, 1, out)
        self.assertIn("dangling interface requirement id MEDIA-999", out)
        self.fx.state("beta", tier="B", requirement_ids=["CORE-001"], interface_requirement_ids=["CORE-001"])
        self.assertIn("is in both requirement_ids and interface_requirement_ids", self.fx.run()[1])
        self.fx.state("alpha", requirement_ids=["MEDIA-001"], interface_requirement_ids=["MEDIA-001"], write_areas=["crates/alpha/**"], crates=["alpha"],
                      artefacts=[{"path": "docs/doc-a.md", "type": "spec", "role": "governing", "reviewed_sha256": sha("# A\n")}])
        self.assertIn("MEDIA-001 is primary here; remove it from interface_requirement_ids", self.fx.run()[1])

    def test_unknown_default_module_is_a_warning(self) -> None:
        self.fx.write("docs/domains/README.md", TAX_REGISTRY.replace("peer, QUALITY-006", "peerx, QUALITY-006"))
        code, out = self.fx.run()
        self.assertEqual(code, 0, out)  # design section 2.7: WARN class
        self.assertIn("WARN registry: Default modules of gamma names 'peerx'", out)

    def test_crate_ownership_is_a_warning(self) -> None:
        self.fx.write("crates/orphan/lib.rs", "// x\n")
        out = self.fx.run()[1]
        self.assertIn("WARN crate orphan is not named in any domain card", out)
        self.fx.state("beta", tier="B", requirement_ids=[], crates=["alpha", "missing"], planned_crates=["alpha"])
        out = self.fx.run()[1]
        self.assertIn("crate alpha is claimed by more than one card: alpha, beta", out)
        self.assertIn("crate 'missing' does not exist", out)
        self.assertIn("planned crate 'alpha' now exists", out)
        self.assertEqual(self.fx.run()[0], 0)

    def test_file_ownership_counts(self) -> None:
        self.fx.write("tools/stray.py", "x\n")
        self.fx.write("package-manifest.json", "{}")
        rep = self.map()
        f = rep["files"]
        self.assertNotIn("crates/alpha/lib.rs", f["no_owner"])
        self.assertIn("tools/stray.py", f["no_owner"])
        self.assertNotIn("package-manifest.json", f["no_owner"])
        self.assertEqual(f["shared_integration_files"], 2)  # docs/INDEX.md and package-manifest.json
        self.assertEqual(f["several_owners"], {})
        self.assertEqual({d["domain"]: d["tracked_files"] for d in rep["domains"]}["alpha"], 1)

    def test_files_with_several_owners_are_reported(self) -> None:
        self.fx.state("beta", tier="B", requirement_ids=[], write_areas=["crates/**"])
        rep = self.map()
        self.assertEqual(rep["files"]["several_owners"], {"crates/alpha/lib.rs": ["alpha", "beta"]})

    def test_text_map_prints_the_table(self) -> None:
        code, out = self.fx.run("--map")
        self.assertEqual(code, 0)
        self.assertIn("| alpha | A | 2 | core, media | CMP-MEDIA, CMP-STARTUP | alpha | 1 |", out)
        self.assertIn("Requirements: 5 total; card 2, default 3", out)

    def test_malformed_state_json_does_not_crash_the_map(self) -> None:
        (self.fx.root / "docs/domains/beta/state.json").write_text("{broken", encoding="utf-8")
        code, out = self.fx.run("--map", "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["unreadable_state"], ["beta"])
        self.assertIn("INCOMPLETE: state.json is not a valid JSON object of the expected shape for beta", self.fx.run("--map")[1])

    def test_two_cards_planning_the_same_crate_is_a_warning(self) -> None:
        self.fx.state("alpha", requirement_ids=["MEDIA-001", "CORE-001"], planned_crates=["future"], write_areas=["crates/alpha/**"],
                      artefacts=[{"path": "docs/doc-a.md", "type": "spec", "role": "governing", "reviewed_sha256": sha("# A\n")}])
        self.fx.state("beta", tier="B", requirement_ids=[], planned_crates=["future"])
        self.assertIn("WARN planned crate future is claimed by more than one card: alpha, beta", self.fx.run()[1])

    def test_state_json_of_the_wrong_shape_is_reported_not_raised(self) -> None:
        (self.fx.root / "docs/domains/beta/state.json").write_text("[]", encoding="utf-8")
        code, out = self.fx.run("--map", "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["unreadable_state"], ["beta"])
        code, out = self.fx.run()
        self.assertEqual(code, 1, out)
        self.assertIn("state.json: must be a JSON object", out)
        (self.fx.root / "docs/domains/beta/state.json").write_text(json.dumps({"requirement_ids": "MEDIA-001"}), encoding="utf-8")
        self.assertEqual(json.loads(self.fx.run("--map", "--json")[1])["unreadable_state"], ["beta"])

    def test_map_reports_missing_inputs_as_incomplete(self) -> None:
        (self.fx.root / "contracts/requirements.json").unlink()
        code, out = self.fx.run("--map", "--json")
        self.assertEqual(code, 0, out)
        data = json.loads(out)
        self.assertTrue(data["incomplete"])
        self.assertTrue(any("requirements.json" in p for p in data["input_problems"]))
        self.assertIn("INCOMPLETE", self.fx.run("--map")[1])

    def test_dangling_interface_id_does_not_raise_a_second_error(self) -> None:
        self.fx.state("beta", tier="B", requirement_ids=["GHOST-001"], interface_requirement_ids=["GHOST-001"])
        code, out = self.fx.run()
        self.assertEqual(code, 1, out)
        self.assertIn("dangling interface requirement id GHOST-001", out)
        self.assertNotIn("Traceback", out)

    def test_escaped_pipe_in_a_registry_cell_does_not_shift_the_columns(self) -> None:
        self.fx.write("docs/domains/README.md", TAX_REGISTRY.replace("| test | none |\n| `beta`", "| a \\| b | none |\n| `beta`").replace("| `gamma` | C | answers: `alpha` | nothing | test | peer, QUALITY-006 |", "| `gamma` | C | answers: `alpha` | nothing | x \\| y | peer, QUALITY-006 |"))
        rep = self.map()
        gamma = {d["domain"]: d for d in rep["domains"]}["gamma"]
        self.assertEqual(gamma["default_modules"], ["peer", "QUALITY-006"])

    def test_filesystem_fallback_is_labelled(self) -> None:
        self.fx.write("scratch.py", "x\n")
        code, out = self.fx.run("--map")
        self.assertIn("Files scanned on disk (git unavailable", out)
        self.assertEqual(json.loads(self.fx.run("--map", "--json")[1])["files"]["file_source"], "filesystem")

    def test_registry_without_the_column_still_parses(self) -> None:
        self.fx.write("docs/domains/README.md", REGISTRY)
        code, out = self.fx.run("--map", "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["requirement_status"], {"card": 2, "unassigned": 3})


if __name__ == "__main__":
    unittest.main()
