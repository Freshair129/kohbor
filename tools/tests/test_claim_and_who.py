"""Tests for tools/claim.py and validate_agents.py --who. Temporary ledgers only; nothing touches the real ledger."""

from __future__ import annotations

import contextlib
import io
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import claim as cl  # noqa: E402
import validate_agents as va  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]


class ClaimTests(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="claim-test-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        (self.dir / "claims").mkdir()

    def run_cli(self, *args: str):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = cl.main(["--coord", str(self.dir), "--repo", str(REPO_ROOT), *args])
        return code, out.getvalue(), err.getvalue()

    def open(self, task="t1", area="docs/x/**", branch="b", **extra) -> tuple[int, str, str]:
        args = ["open", "--domain", "d", "--task", task, "--write-area", area, "--branch", branch, "--session", "s",
                "--role", "lead", "--profile", "sonnet55-high", "--model", "claude-sonnet-5-5", "--effort", "high"]
        for k, v in extra.items():
            args += [f"--{k.replace('_', '-')}", v]
        return self.run_cli(*args)

    def test_open_writes_valid_json_with_structured_fields(self) -> None:
        code, out, _ = self.open(task_type="governance-change")
        self.assertEqual(code, 0, out)
        data = json.loads((self.dir / "claims" / "d__t1.json").read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "active")
        self.assertEqual(data["holder"], {"role": "lead", "profile": "sonnet55-high", "model": "claude-sonnet-5-5", "effort": "high", "session": "s", "task_type": "governance-change"})
        self.assertEqual(data["workers"], [])
        self.assertTrue(data["base_commit"])
        self.assertTrue(data["expires"] > data["started"])

    def test_open_twice_with_one_name_fails_the_second_time_and_keeps_the_first(self) -> None:
        self.assertEqual(self.open(note="first")[0], 0)
        before = (self.dir / "claims" / "d__t1.json").read_text(encoding="utf-8")
        code, _, err = self.open(area="other/**", note="second")
        self.assertEqual(code, 1)
        self.assertIn("already exists", err)
        self.assertEqual((self.dir / "claims" / "d__t1.json").read_text(encoding="utf-8"), before)

    def test_overlapping_active_claim_on_the_same_branch_is_listed_not_refused(self) -> None:
        self.open(task="t1", area="docs/x/**")
        code, out, _ = self.open(task="t2", area="docs/x/y.md")
        self.assertEqual(code, 0)
        self.assertIn("OVERLAP with active claim d__t1.json", out)
        code, out, _ = self.open(task="t3", area="docs/x/z.md", branch="other-branch")
        self.assertNotIn("OVERLAP", out)
        code, out, _ = self.open(task="t4", area="crates/**")
        self.assertNotIn("OVERLAP", out)

    def test_overlap_sees_absolute_write_areas_and_whole_checkout_claims(self) -> None:
        repo = str(REPO_ROOT)
        self.open(task="t1", area=repo + " (whole checkout)")
        code, out, _ = self.open(task="t2", area="docs/x.md")
        self.assertIn("OVERLAP with active claim d__t1.json", out)
        self.open(task="t3", area=repo + "/docs/**", branch="other")
        code, out, _ = self.open(task="t4", area="docs/y.md", branch="other")
        self.assertIn("OVERLAP with active claim d__t3.json", out)

    def test_bad_names_are_refused_and_nothing_is_written(self) -> None:
        code, _, err = self.run_cli("open", "--domain", "../evil", "--task", "t", "--write-area", "x", "--branch", "b", "--session", "s", "--role", "lead", "--profile", "p", "--model", "m")
        self.assertEqual(code, 2)
        self.assertEqual(list((self.dir / "claims").iterdir()), [])

    def test_close_sets_status_and_keeps_the_file(self) -> None:
        self.open()
        code, out, _ = self.run_cli("close", "d__t1", "--last-commit", "abc1234", "--note", "finished")
        self.assertEqual(code, 0, out)
        data = json.loads((self.dir / "claims" / "d__t1.json").read_text(encoding="utf-8"))
        self.assertEqual((data["status"], data["last_commit"]), ("done", "abc1234"))
        self.assertIn("finished", data["note"])
        self.assertTrue(data["finished"])
        self.assertEqual(self.run_cli("close", "d__missing")[0], 1)

    def test_close_rewrites_a_legacy_claim_as_valid_json(self) -> None:
        (self.dir / "claims" / "old__x.json").write_text('{"status": "active", "worktree": "F:\\kohbor"}', encoding="utf-8")
        self.assertEqual(self.run_cli("close", "old__x", "--last-commit", "abc")[0], 0)
        json.loads((self.dir / "claims" / "old__x.json").read_text(encoding="utf-8"))

    def test_list_shows_active_claims_and_all_with_the_flag(self) -> None:
        self.open(task="t1")
        self.open(task="t2")
        self.run_cli("close", "d__t1", "--last-commit", "abc")
        self.assertIn("1 claim(s) listed", self.run_cli("list")[1])
        self.assertIn("2 claim(s) listed", self.run_cli("list", "--all")[1])

    def test_close_does_not_touch_an_existing_tmp_file_and_leaves_none_behind(self) -> None:
        self.open()
        stale = self.dir / "claims" / "d__t1.json.tmp"
        stale.write_text("not mine", encoding="utf-8")
        self.assertEqual(self.run_cli("close", "d__t1", "--last-commit", "abc")[0], 0)
        self.assertEqual(stale.read_text(encoding="utf-8"), "not mine")
        self.assertEqual(sorted(p.name for p in (self.dir / "claims").iterdir()), ["d__t1.json", "d__t1.json.tmp"])

    def test_missing_ledger_is_not_created(self) -> None:
        with self.assertRaises(SystemExit):
            cl.main(["--coord", str(self.dir / "nowhere"), "list"])
        self.assertFalse((self.dir / "nowhere").exists())

    def test_tool_writes_only_inside_the_ledger(self) -> None:
        def snapshot() -> dict:
            return {str(p.relative_to(REPO_ROOT)): (p.stat().st_size, p.stat().st_mtime_ns) for p in REPO_ROOT.rglob("*")
                    if p.is_file() and not {".git", "__pycache__", "node_modules", "target"} & set(p.relative_to(REPO_ROOT).parts)}

        before = snapshot()
        self.open()
        self.assertEqual(snapshot(), before)
        # the assertion must be able to fail: a write below an existing directory changes the snapshot
        probe = REPO_ROOT / "tools" / "tests" / "fixtures" / "_probe.tmp"
        probe.write_text("x", encoding="utf-8")
        try:
            self.assertNotEqual(snapshot(), before)
        finally:
            probe.unlink()


class WhoTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.reg = va.load(REPO_ROOT)
        cls.domains = va.domain_info(REPO_ROOT)

    def who(self, *claims: dict):
        return va.who(self.reg, self.domains, [(f"c{i}", c) for i, c in enumerate(claims)])

    GOOD = {"status": "active", "domain": "packs", "task": "S9", "branch": "b", "expires": "2999-01-01T00:00:00+00:00",
            "holder": {"role": "lead", "profile": "claude-session", "model": "x", "task_type": "integrate"},
            "workers": [{"role": "general-worker", "profile": "luna-max", "model": "gpt-6-luna", "effort": "max", "task_type": "implement"}]}

    def test_counts_active_claims_and_workers_and_stationed_agents(self) -> None:
        lines, problems, _ = self.who(self.GOOD, dict(self.GOOD, task="S10", workers=[]), dict(self.GOOD, status="done"))
        self.assertEqual(problems, [])
        self.assertEqual(lines[-1], "2 active claim(s), 1 worker(s), 0 stationed agent(s)")
        self.assertTrue(lines[0].startswith("packs | S9 | lead x"))

    def test_a_profile_or_role_that_is_not_in_the_registry_is_a_problem(self) -> None:
        bad = dict(self.GOOD, workers=[{"role": "ghost", "profile": "luna-max", "task_type": "implement"},
                                       {"role": "general-worker", "profile": "ghost", "task_type": "implement"}])
        _, problems, _ = self.who(bad)
        self.assertTrue(any("role 'ghost' is not a registry role" in p for p in problems), problems)
        self.assertTrue(any("profile 'ghost' is not a registry profile" in p for p in problems), problems)

    def test_a_worker_that_is_not_the_route_is_a_problem(self) -> None:
        bad = dict(self.GOOD, workers=[{"role": "test-worker", "profile": "terra-high", "task_type": "implement"}])
        _, problems, _ = self.who(bad)
        self.assertTrue(any("is not a registry route in domain packs" in p for p in problems), problems)

    def test_model_effort_and_task_type_must_match(self) -> None:
        bad = dict(self.GOOD, workers=[{"role": "general-worker", "profile": "luna-max", "model": "gpt-6.1-terra", "effort": "high", "task_type": "implement"},
                                       {"role": "general-worker", "profile": "luna-max"}])
        _, problems, _ = self.who(bad)
        self.assertTrue(any("model gpt-6.1-terra differs from gpt-6-luna" in p for p in problems), problems)
        self.assertTrue(any("effort high differs from max" in p for p in problems), problems)
        self.assertTrue(any("a worker needs a task_type" in p for p in problems), problems)

    def test_unstructured_legacy_claims_are_a_note_not_a_problem(self) -> None:
        lines, problems, notes = self.who({"status": "active", "domain": "x", "task": "t", "model": "claude (session)", "branch": "b"})
        self.assertEqual(problems, [])
        self.assertTrue(any("holder is unstructured" in n for n in notes))
        self.assertIn("unstructured: claude (session)", lines[0])

    def test_a_claim_for_an_unknown_domain_is_a_problem(self) -> None:
        _, problems, _ = self.who(dict(self.GOOD, domain="ghost"))
        self.assertTrue(any("domain 'ghost' is not a registry domain" in p for p in problems), problems)

    def test_cli_who_reads_a_ledger_and_enforce_exits_1_on_a_problem(self) -> None:
        d = Path(tempfile.mkdtemp(prefix="who-"))
        self.addCleanup(shutil.rmtree, d, True)
        (d / "claims").mkdir()
        (d / "claims" / "a.json").write_text(json.dumps(self.GOOD), encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(va.main(["--root", str(REPO_ROOT), "--who", "--coord", str(d), "--enforce"]), 0)
        self.assertIn("1 active claim(s), 1 worker(s)", out.getvalue())
        bad = dict(self.GOOD, workers=[{"role": "ghost", "profile": "luna-max", "task_type": "implement"}])
        (d / "claims" / "a.json").write_text(json.dumps(bad), encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(va.main(["--root", str(REPO_ROOT), "--who", "--coord", str(d), "--enforce"]), 1)
            self.assertEqual(va.main(["--root", str(REPO_ROOT), "--who", "--coord", str(d)]), 0)


if __name__ == "__main__":
    unittest.main()
