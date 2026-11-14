"""Tests for the claim tags (Stage 0 of the progress chase): the tag pool, claim.py tag/renew/done-tag, and the T1..T4 checks of
check_claim_scope.py. Temporary repositories and ledgers only; nothing touches the real ledger."""

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
import check_claim_scope as ccs  # noqa: E402
import claim as cl  # noqa: E402
import tagpool as tp  # noqa: E402

GIT = ["git", "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"]
ROADMAP = """# Sprint Roadmap

## Sprint 1 — Event detector (M2)

| Task | Work | Model | Pattern | Risk | Evidence |
| --- | --- | --- | --- | --- | --- |
| S1-1 | Tests | Terra | T-L-R 1 | R1 | synthetic |
| S1-2, S1-3 | More | Luna | L-R | R1 | synthetic |

## Backlog status

| Task | Status | Report | Commit |
| --- | --- | --- | --- |
| S9-9 not a sprint table row | passed | x | `abc1234` |
"""


class Base(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="tags-test-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.repo = self.dir / "repo"
        self.coord = self.dir / "coord"
        (self.coord / "claims").mkdir(parents=True)
        subprocess.run([*GIT, "init", "-q", "-b", "main", str(self.repo)], check=True)
        self.put("contracts/requirements.json", json.dumps({"requirements": [{"id": "MEDIA-004"}, {"id": "CORE-001"}]}))
        self.put("contracts/decisions.json", json.dumps({"decisions": [{"id": "DEC-ACCOUNT-ORIGIN"}, {"id": "DEC-X"}]}))
        self.put("docs/delivery/SPRINT-ROADMAP.md", ROADMAP)
        self.put("README.md", "x")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "base")
        self.git("checkout", "-q", "-b", "feat/x")

    def git(self, *args: str) -> str:
        return subprocess.run([*GIT, *args], cwd=self.repo, capture_output=True, text=True, check=True).stdout

    def put(self, rel: str, text: str) -> None:
        p = self.repo / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")

    def commit(self, msg: str = "work") -> None:
        self.git("add", "-A")
        self.git("commit", "-q", "-m", msg)


class PoolTests(Base):
    def test_pool_reads_requirements_decisions_and_sprint_table_tasks(self) -> None:
        pool = tp.load_pool(self.repo)
        self.assertEqual(pool["requirements"], {"MEDIA-004", "CORE-001"})
        self.assertEqual(pool["decisions"], {"DEC-ACCOUNT-ORIGIN", "DEC-X"})
        self.assertEqual(pool["tasks"], {"S1-1", "S1-2"})  # the first token of each sprint-table Task cell; Backlog rows are not sprint tables
        self.assertEqual(pool["errors"], [])

    def test_validity_by_kind(self) -> None:
        pool = tp.load_pool(self.repo)
        for ok in ("MEDIA-004", "DEC-X", "TASK:S1-1", "PROC:progress-chase", "PROC:a1"):
            self.assertTrue(tp.is_valid(ok, pool), ok)
        for bad in ("MEDIA-999", "DEC-NOPE", "TASK:S9-9", "TASK:", "PROC:Bad Slug", "PROC:", "proc:x", "random", "", "media-004", "TASK:S1-3"):
            self.assertFalse(tp.is_valid(bad, pool), bad)
        self.assertEqual([tp.kind_of(t) for t in ("MEDIA-004", "DEC-X", "TASK:S1-1", "PROC:x", "zzz")], ["requirement", "decision", "task", "process", None])

    def test_missing_files_leave_a_smaller_pool_and_say_so(self) -> None:
        (self.repo / "contracts" / "decisions.json").unlink()
        pool = tp.load_pool(self.repo)
        self.assertEqual(pool["decisions"], set())
        self.assertTrue(any("decisions.json" in e for e in pool["errors"]))
        self.assertTrue(tp.is_valid("PROC:x", pool))  # a process tag never needs a source

    def test_corrupt_json_is_reported_not_fatal(self) -> None:
        self.put("contracts/requirements.json", "{not json")
        pool = tp.load_pool(self.repo)
        self.assertEqual(pool["requirements"], set())
        self.assertTrue(any("requirements.json" in e for e in pool["errors"]))


class ClaimCliTests(Base):
    def run_cli(self, *args: str):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = cl.main(["--coord", str(self.coord), "--repo", str(self.repo), *args])
        return code, out.getvalue(), err.getvalue()

    def open(self, task: str = "t1", *extra: str):
        return self.run_cli("open", "--domain", "d", "--task", task, "--write-area", "docs/x/**", "--branch", "feat/x", "--session", "s", "--role", "lead",
                            "--profile", "p", "--model", "m", *extra)

    def data(self, name: str = "d__t1") -> dict:
        return json.loads((self.coord / "claims" / f"{name}.json").read_text(encoding="utf-8"))

    def test_open_with_tags_validates_and_records_them(self) -> None:
        code, out, err = self.open("t1", "--tag", "MEDIA-004", "--tag", "PROC:progress-chase", "--tag", "MEDIA-004")
        self.assertEqual(code, 0, err)
        d = self.data()
        self.assertEqual(d["tags"], ["MEDIA-004", "PROC:progress-chase"])  # deduplicated, order kept
        self.assertEqual(d["tags_done"], {})

    def test_open_without_tags_still_works_and_records_an_empty_list(self) -> None:
        self.assertEqual(self.open("t1")[0], 0)
        self.assertEqual(self.data()["tags"], [])

    def test_open_refuses_an_unknown_tag_and_writes_nothing(self) -> None:
        code, _, err = self.open("t1", "--tag", "MEDIA-999")
        self.assertEqual(code, 2)
        self.assertIn("MEDIA-999", err)
        self.assertFalse((self.coord / "claims" / "d__t1.json").exists())

    def test_tag_add(self) -> None:
        self.open("t1")
        code, out, err = self.run_cli("tag", "d__t1", "--add", "TASK:S1-1", "--add", "DEC-X")
        self.assertEqual(code, 0, err)
        self.assertEqual(self.data()["tags"], ["TASK:S1-1", "DEC-X"])
        self.run_cli("tag", "d__t1", "--add", "DEC-X")
        self.assertEqual(self.data()["tags"], ["TASK:S1-1", "DEC-X"])  # idempotent
        code, _, err = self.run_cli("tag", "d__t1", "--add", "NOPE-1")
        self.assertEqual(code, 2)
        self.assertEqual(self.data()["tags"], ["TASK:S1-1", "DEC-X"])  # a refused add changes nothing

    def test_tag_needs_an_existing_active_claim(self) -> None:
        self.assertEqual(self.run_cli("tag", "d__missing", "--add", "DEC-X")[0], 1)
        self.open("t1")
        self.run_cli("close", "d__t1")
        code, _, err = self.run_cli("tag", "d__t1", "--add", "DEC-X")
        self.assertEqual(code, 1)
        self.assertIn("not active", err)

    def test_renew_extends_from_now_and_records_it(self) -> None:
        self.open("t1", "--hours", "1")
        before = self.data()["expires"]
        code, out, err = self.run_cli("renew", "d__t1", "--hours", "48")
        self.assertEqual(code, 0, err)
        d = self.data()
        self.assertGreater(d["expires"], before)
        self.assertEqual(len(d["renewed"]), 1)
        self.assertEqual(d["renewed"][0]["hours"], 48.0)
        self.assertEqual(self.run_cli("renew", "d__t1", "--hours", "0")[0], 2)
        self.assertEqual(self.run_cli("renew", "d__t1", "--hours", "-5")[0], 2)

    def test_renew_refuses_a_closed_claim(self) -> None:
        self.open("t1")
        self.run_cli("close", "d__t1")
        self.assertEqual(self.run_cli("renew", "d__t1", "--hours", "5")[0], 1)

    def test_close_with_done_and_open_tags_computes_hash_and_commit(self) -> None:
        self.open("t1", "--tag", "MEDIA-004", "--tag", "CORE-001")
        self.put("docs/evidence/receipt.json", '{"result": "PASS"}')
        self.commit("evidence")
        head = self.git("rev-parse", "--short", "HEAD").strip()
        code, out, err = self.run_cli("close", "d__t1", "--done-tag", "MEDIA-004=docs/evidence/receipt.json", "--open-tag", "CORE-001")
        self.assertEqual(code, 0, err)
        d = self.data()
        want = hashlib.sha256((self.repo / "docs/evidence/receipt.json").read_bytes()).hexdigest()
        self.assertEqual(d["tags_done"]["MEDIA-004"], {"path": "docs/evidence/receipt.json", "sha256": want, "commit": head})
        self.assertEqual(d["tags_open"], ["CORE-001"])
        tags_line = [l for l in out.splitlines() if l.startswith("Tags:")]
        self.assertEqual(len(tags_line), 1)
        self.assertIn("done MEDIA-004=docs/evidence/receipt.json@" + head, tags_line[0])
        self.assertIn(want[:12], tags_line[0])
        self.assertIn("open CORE-001", tags_line[0])

    def test_close_refuses_a_done_tag_that_is_not_on_the_claim_or_has_no_file(self) -> None:
        self.open("t1", "--tag", "MEDIA-004")
        self.put("a.txt", "x")
        code, _, err = self.run_cli("close", "d__t1", "--done-tag", "CORE-001=a.txt")
        self.assertEqual(code, 2)
        self.assertIn("not a tag of this claim", err)
        code, _, err = self.run_cli("close", "d__t1", "--done-tag", "MEDIA-004=nope.txt")
        self.assertEqual(code, 2)
        self.assertIn("nope.txt", err)
        self.assertEqual(self.data()["status"], "active")  # nothing was closed
        code, _, err = self.run_cli("close", "d__t1", "--done-tag", "no-equals-sign")
        self.assertEqual(code, 2)

    def test_close_without_tag_options_still_prints_a_tags_line_for_the_handoff(self) -> None:
        self.open("t1", "--tag", "DEC-X")
        code, out, _ = self.run_cli("close", "d__t1")
        self.assertEqual(code, 0)
        self.assertIn("Tags: open DEC-X", [l for l in out.splitlines() if l.startswith("Tags:")][0])  # an untouched tag stays open, never silently done
        self.assertEqual(self.data()["tags_open"], ["DEC-X"])

    def test_close_of_an_untagged_claim_says_none(self) -> None:
        self.open("t1")
        _, out, _ = self.run_cli("close", "d__t1")
        self.assertIn("Tags: none", out)

    def test_list_shows_tags(self) -> None:
        self.open("t1", "--tag", "DEC-X")
        _, out, _ = self.run_cli("list")
        self.assertIn("DEC-X", out)


class ScopeTagTests(Base):
    def claim(self, name: str, **fields) -> None:
        data = {"status": "active", "branch": "feat/x", "write_area": ["crates/a/**"], "tags": [], "tags_done": {}}
        data.update(fields)
        (self.coord / "claims" / name).write_text(json.dumps(data), encoding="utf-8")

    def run_tool(self, *extra: str):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = ccs.main(["--repo", str(self.repo), "--coord", str(self.coord), *extra])
        return code, out.getvalue()

    def test_t1_active_claim_without_a_valid_tag(self) -> None:
        self.claim("a__t.json", tags=[])
        code, out = self.run_tool()
        self.assertEqual(code, 0, out)  # report-only
        self.assertIn("T1", out)
        self.assertIn("a__t.json", out)
        self.assertEqual(self.run_tool("--enforce")[0], 1)

    def test_a_valid_tag_clears_t1_and_t3(self) -> None:
        self.claim("a__t.json", tags=["MEDIA-004"])
        self.put("crates/a/src/lib.rs", "x")
        self.commit()
        code, out = self.run_tool("--enforce")
        self.assertEqual(code, 0, out)
        self.assertNotIn("T1", out)
        self.assertNotIn("T3", out)

    def test_t2_unknown_tag(self) -> None:
        self.claim("a__t.json", tags=["MEDIA-004", "MEDIA-999"])
        code, out = self.run_tool()
        self.assertEqual(code, 0, out)
        self.assertIn("T2", out)
        self.assertIn("MEDIA-999", out)
        self.assertEqual(self.run_tool("--enforce")[0], 1)

    def test_t3_branch_touches_non_shared_paths_without_a_tagged_claim(self) -> None:
        self.claim("a__t.json", tags=[], write_area=["crates/a/**"])
        self.put("crates/a/src/lib.rs", "x")
        self.commit()
        code, out = self.run_tool()
        self.assertIn("T3", out)
        self.assertEqual(self.run_tool("--enforce")[0], 1)

    def test_t3_does_not_fire_for_shared_integration_files_only(self) -> None:
        self.claim("a__t.json", tags=["MEDIA-004"])
        self.put("package-manifest.json", "{}")
        self.commit()
        _, out = self.run_tool()
        self.assertNotIn("T3", out)

    def test_t3_with_no_claim_at_all_is_reported_next_to_the_existing_no_claim_problem(self) -> None:
        self.put("crates/a/src/lib.rs", "x")
        self.commit()
        code, out = self.run_tool()
        self.assertIn("has no active claim", out)
        self.assertIn("T3", out)

    def test_t4_evidence_file_missing_or_hash_mismatch(self) -> None:
        self.put("docs/evidence/r.json", "one")
        good = hashlib.sha256(b"one").hexdigest()
        self.claim("a__t.json", tags=["MEDIA-004"], tags_done={"MEDIA-004": {"path": "docs/evidence/r.json", "sha256": good, "commit": "abc"}})
        _, out = self.run_tool()
        self.assertNotIn("T4", out)
        self.put("docs/evidence/r.json", "two")
        code, out = self.run_tool("--enforce")
        self.assertIn("T4", out)
        self.assertIn("hash mismatch", out)
        self.assertEqual(code, 1)
        (self.repo / "docs" / "evidence" / "r.json").unlink()
        _, out = self.run_tool()
        self.assertIn("T4", out)
        self.assertIn("missing", out)

    def test_t4_also_checks_a_closed_claim_on_the_branch(self) -> None:
        self.claim("a__t.json", status="done", tags=["MEDIA-004"], tags_done={"MEDIA-004": {"path": "gone.json", "sha256": "0" * 64, "commit": "abc"}})
        _, out = self.run_tool()
        self.assertIn("T4", out)

    def test_a_claim_of_another_branch_is_not_checked(self) -> None:
        self.claim("other.json", branch="feat/y", tags=[])
        _, out = self.run_tool()
        self.assertNotIn("T1", out)

    def test_tag_is_not_compared_with_the_write_area(self) -> None:
        self.claim("a__t.json", tags=["DEC-X"], write_area=["tools/**"])
        self.put("tools/z.py", "x")
        self.commit()
        code, out = self.run_tool("--enforce")
        self.assertEqual(code, 0, out)  # a decision tag on tooling is fine: tags say what it serves, the write area says where

    def test_json_report_carries_the_tag_findings(self) -> None:
        self.claim("a__t.json", tags=["NOPE-1"])
        _, out = self.run_tool("--json")
        rep = json.loads(out)
        self.assertEqual([f["code"] for f in rep["tag_findings"]], ["T1", "T2"])  # its only tag is invalid, so it has no valid tag either

    def test_missing_pool_sources_do_not_flag_every_tag_unknown(self) -> None:
        (self.repo / "contracts" / "requirements.json").unlink()
        self.claim("a__t.json", tags=["MEDIA-004"])
        _, out = self.run_tool()
        self.assertNotIn("T2", out)  # the registry could not be read: the tag cannot be judged, so the tool says so instead
        self.assertIn("tag pool", out)


class TagpoolFindings(Base):
    def test_claim_findings_are_shared_by_scope_and_chase(self) -> None:
        pool = tp.load_pool(self.repo)
        self.assertEqual([f["code"] for f in tp.claim_findings("n", {"status": "active", "tags": []}, pool, self.repo)], ["T1"])
        self.assertEqual([f["code"] for f in tp.claim_findings("n", {"status": "active", "tags": ["MEDIA-004"]}, pool, self.repo)], [])
        self.assertEqual([f["code"] for f in tp.claim_findings("n", {"status": "done", "tags": []}, pool, self.repo)], [])  # T1 is for active claims
        self.assertEqual([f["code"] for f in tp.claim_findings("n", {"status": "active", "tags": ["X-1"]}, pool, self.repo)], ["T1", "T2"])

    def test_old_claims_without_a_tags_field_are_t1(self) -> None:
        pool = tp.load_pool(self.repo)
        self.assertEqual([f["code"] for f in tp.claim_findings("n", {"status": "active"}, pool, self.repo)], ["T1"])


if __name__ == "__main__":
    unittest.main()
