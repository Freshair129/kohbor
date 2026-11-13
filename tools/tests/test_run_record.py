"""Tests for tools/run_record.py: the record format and the separation-of-duties check. Temporary ledgers only."""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run_record as rr  # noqa: E402
import validate_agents as va  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]


def rec(run_id, task, session, model, role="general-worker", writes=True, reviews=None, **extra):
    base = {"run_id": run_id, "task_id": task, "task_type": "implement", "domain": "packs", "role": role, "profile": "p", "model": model,
            "session": session, "recorded": "2026-11-13T00:00:00+00:00", "writes": writes, "reviews": reviews, "off_route": False}
    base.update(extra)
    return base


class CheckTests(unittest.TestCase):
    def test_a_review_by_another_session_and_model_is_clean(self) -> None:
        problems, notes = rr.check_records([rec("a", "T1", "s1", "gpt-6-luna"), rec("b", "T1", "s2", "gpt-6.1-terra", role="test-worker", writes=False, reviews="T1")])
        self.assertEqual((problems, notes), ([], []))

    def test_same_session_or_same_model_is_a_problem(self) -> None:
        problems, _ = rr.check_records([rec("a", "T1", "s1", "gpt-6-luna"), rec("b", "T1", "s1", "gpt-6.1-terra", writes=False, reviews="T1")])
        self.assertTrue(any("same session as the author run a" in p for p in problems), problems)
        problems, _ = rr.check_records([rec("a", "T1", "s1", "gpt-6-luna"), rec("b", "T1", "s2", "gpt-6-luna", writes=False, reviews="T1")])
        self.assertTrue(any("same model as the author run a" in p for p in problems), problems)

    def test_a_model_that_is_not_concrete_is_unverifiable_not_a_pass(self) -> None:
        problems, notes = rr.check_records([rec("a", "T1", "s1", "session"), rec("b", "T1", "s2", "session", writes=False, reviews="T1")])
        self.assertEqual(problems, [])
        self.assertTrue(any("UNVERIFIABLE" in n for n in notes), notes)

    def test_review_without_a_writing_run_and_duplicates_and_off_route(self) -> None:
        problems, _ = rr.check_records([rec("b", "T9", "s2", "m", writes=False, reviews="T9")])
        self.assertTrue(any("no writing run recorded" in p for p in problems))
        problems, _ = rr.check_records([rec("a", "T1", "s1", "m"), rec("a", "T2", "s1", "m")])
        self.assertTrue(any("duplicate run_id" in p for p in problems))
        problems, _ = rr.check_records([rec("a", "T1", "s1", "m", off_route=True)])
        self.assertTrue(any("is not a registry route" in p for p in problems))

    def test_a_writing_run_whose_route_names_a_reviewer_needs_a_review_record(self) -> None:
        reg = va.load(REPO_ROOT)
        _, notes = rr.check_records([rec("a", "T1", "s1", "gpt-6-luna")], reg)
        self.assertTrue(any("task T1: its route names a reviewer" in n for n in notes), notes)
        _, notes = rr.check_records([rec("a", "T1", "s1", "gpt-6-luna"), rec("b", "T1", "s2", "gpt-6.1-terra", writes=False, reviews="T1")], reg)
        self.assertFalse(any("task T1" in n for n in notes))


class LedgerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="runrec-"))
        self.addCleanup(shutil.rmtree, self.dir, True)

    def cli(self, *args: str):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = rr.main(["--root", str(REPO_ROOT), "--coord", str(self.dir), *args])
        return code, out.getvalue(), err.getvalue()

    def add(self, task="T1", role="general-worker", profile="luna-max", task_type="implement", session="s1", **extra):
        args = ["add", "--task-id", task, "--task-type", task_type, "--domain", "packs", "--role", role, "--profile", profile, "--session", session]
        for k, v in extra.items():
            args += [f"--{k.replace('_', '-')}", str(v)]
        return self.cli(*args)

    def test_add_takes_model_and_effort_from_the_registry_and_hashes_the_files(self) -> None:
        brief = self.dir / "brief.txt"
        brief.write_text("TASK_ID: T1\n", encoding="utf-8")
        code, out, _ = self.add(brief=brief)
        self.assertEqual(code, 0, out)
        r = json.loads((self.dir / "runs" / "runs.jsonl").read_text(encoding="utf-8").splitlines()[0])
        self.assertEqual((r["model"], r["effort"], r["tool"], r["writes"], r["off_route"]), ("gpt-6-luna", "max", "codex-exec", True, False))
        self.assertEqual(r["brief_sha256"], hashlib.sha256(brief.read_bytes()).hexdigest())
        self.assertIsNone(r["report_sha256"])

    def test_off_route_is_recorded_and_reported(self) -> None:
        code, out, _ = self.add(role="test-worker", profile="terra-high")
        self.assertEqual(code, 0)
        self.assertIn("OFF-ROUTE", out)
        code, out, _ = self.cli("check")
        self.assertEqual(code, 0, out)
        self.assertIn("is not a registry route", out)
        self.assertEqual(self.cli("check", "--enforce")[0], 1)

    def test_an_owner_instruction_turns_the_off_route_problem_into_a_note(self) -> None:
        self.add(task="T1", session="s1")
        self.add(task="T1", role="test-worker", profile="sol-xhigh", task_type="review-diff", session="s2", reviews="T1", owner_instruction="owner asked for sol")
        code, out, _ = self.cli("check", "--enforce")
        self.assertEqual(code, 0, out)
        self.assertIn("off the registry route by owner instruction: owner asked for sol", out)

    def test_unknown_profile_is_refused_and_writes_nothing(self) -> None:
        code, _, err = self.add(profile="ghost")
        self.assertEqual(code, 2)
        self.assertIn("not in the registry", err)
        self.assertFalse((self.dir / "runs" / "runs.jsonl").exists())

    def test_check_over_a_real_ledger_flags_a_same_model_review(self) -> None:
        self.add(task="T1", session="s1")
        self.add(task="T1", role="test-worker", profile="terra-high", task_type="review-diff", session="s2", reviews="T1")
        code, out, _ = self.cli("check", "--enforce")
        self.assertEqual(code, 0, out)
        self.add(task="T2", session="s1")
        self.add(task="T2", role="general-worker", profile="luna-max", task_type="code-question", session="s3", reviews="T2")
        code, out, _ = self.cli("check", "--enforce")
        self.assertEqual(code, 1, out)
        self.assertIn("same model as the author run", out)

    def test_bad_lines_are_reported(self) -> None:
        (self.dir / "runs").mkdir()
        (self.dir / "runs" / "runs.jsonl").write_text('{broken\n{"run_id": "x"}\n', encoding="utf-8")
        code, out, _ = self.cli("check")
        self.assertIn("line 1: not valid JSON", out)
        self.assertIn("line 2: missing one of", out)

    def test_missing_ledger_is_not_created(self) -> None:
        with self.assertRaises(SystemExit):
            rr.main(["--coord", str(self.dir / "nowhere"), "list"])
        self.assertFalse((self.dir / "nowhere").exists())


if __name__ == "__main__":
    unittest.main()
