"""Tests for tools/janitor.py: classification and the guarantee that it only reports. Synthetic repos in temp folders."""

from __future__ import annotations

import contextlib
import datetime as dt
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import janitor  # noqa: E402

GIT = ["git", "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"]


@unittest.skipIf(shutil.which("git") is None or os.environ.get("KOHBOR_TOOL_TESTS_SKIP_GIT") == "1", "git is not available or the git tests are skipped (gate --quick)")
class JanitorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="janitor-test-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.origin = self.dir / "origin.git"
        self.repo = self.dir / "repo"
        self.coord = self.dir / "coord"
        (self.coord / "claims").mkdir(parents=True)
        (self.coord / "registry").mkdir(parents=True)
        self.run_git(self.dir, "init", "-q", "--bare", str(self.origin))
        self.run_git(self.dir, "init", "-q", "-b", "main", str(self.repo))
        self.run_git(self.repo, "remote", "add", "origin", str(self.origin))
        self.commit("base")
        self.run_git(self.repo, "push", "-q", "origin", "main")
        # merged into main and pushed
        self.run_git(self.repo, "checkout", "-q", "-b", "done-merged")
        self.commit("done work")
        self.run_git(self.repo, "checkout", "-q", "main")
        self.run_git(self.repo, "merge", "-q", "--ff-only", "done-merged")
        self.run_git(self.repo, "push", "-q", "origin", "main")
        # unmerged
        self.run_git(self.repo, "checkout", "-q", "-b", "wip-unmerged")
        self.commit("wip")
        self.run_git(self.repo, "checkout", "-q", "main")
        # merged but held by a claim
        self.run_git(self.repo, "branch", "held-by-claim", "main")
        (self.coord / "claims" / "x__y.json").write_text(json.dumps({"status": "active", "branch": "held-by-claim", "expires": "2999-01-01T00:00:00+00:00"}), encoding="utf-8")
        (self.coord / "registry" / "branches.json").write_text(json.dumps({"repo_F_kohbor": {"worktrees": []}}), encoding="utf-8")

    def run_git(self, cwd: Path, *args: str) -> str:
        return subprocess.run([*GIT, *args], cwd=cwd, capture_output=True, text=True, check=True).stdout

    def commit(self, msg: str) -> None:
        (self.repo / f"{msg.replace(' ', '_')}.txt").write_text(msg, encoding="utf-8")
        self.run_git(self.repo, "add", "-A")
        self.run_git(self.repo, "commit", "-q", "-m", msg)

    def collect(self, min_age: int = 0) -> dict:
        return janitor.collect(self.repo, self.coord, "main", min_age, dt.datetime.now(dt.timezone.utc))

    def by_name(self, rep: dict) -> dict:
        return {b["name"]: b for b in rep["branches"]}

    def snapshot(self) -> tuple[str, str, str]:
        return (self.run_git(self.repo, "for-each-ref"), self.run_git(self.repo, "worktree", "list"), self.run_git(self.repo, "stash", "list"))

    def test_classification(self) -> None:
        b = self.by_name(self.collect())
        self.assertEqual(b["main"]["class"], "BASE")
        self.assertEqual(b["done-merged"]["class"], "PROPOSE-TO-OWNER")
        self.assertEqual(b["wip-unmerged"]["class"], "REPORT")
        self.assertIn("not merged into main", b["wip-unmerged"]["reasons"])
        self.assertEqual(b["held-by-claim"]["class"], "REPORT")
        self.assertTrue(any("active claim x__y.json" in r for r in b["held-by-claim"]["reasons"]))

    def test_current_branch_and_worktree_are_never_proposed(self) -> None:
        self.run_git(self.repo, "checkout", "-q", "done-merged")
        b = self.by_name(self.collect())
        self.assertEqual(b["done-merged"]["class"], "REPORT")
        self.assertTrue(any("checked out" in r for r in b["done-merged"]["reasons"]))
        self.run_git(self.repo, "checkout", "-q", "main")
        wt = self.dir / "wt"
        self.run_git(self.repo, "worktree", "add", "-q", str(wt), "-b", "in-worktree")
        rep = self.collect()
        self.assertEqual(self.by_name(rep)["in-worktree"]["class"], "REPORT")
        unregistered = [w for w in rep["worktrees"] if "unregistered worktree" in w["reasons"]]
        self.assertTrue(unregistered)
        self.assertTrue(any("unregistered" in f for f in rep["red_flags"]))

    def test_unpushed_work_blocks_a_proposal(self) -> None:
        self.run_git(self.repo, "checkout", "-q", "done-merged")
        self.run_git(self.repo, "push", "-q", "-u", "origin", "done-merged")
        self.commit("more after push")
        self.run_git(self.repo, "checkout", "-q", "main")
        self.run_git(self.repo, "merge", "-q", "--ff-only", "done-merged")
        b = self.by_name(self.collect())["done-merged"]
        self.assertEqual(b["class"], "REPORT")
        self.assertTrue(any("unpushed" in r or "not in origin/main" in r for r in b["reasons"]))

    def test_local_only_merge_is_not_proposed(self) -> None:
        self.run_git(self.repo, "checkout", "-q", "-b", "local-merged")
        self.commit("local only")
        self.run_git(self.repo, "checkout", "-q", "main")
        self.run_git(self.repo, "merge", "-q", "--ff-only", "local-merged")  # main is now ahead of origin/main
        b = self.by_name(self.collect())["local-merged"]
        self.assertEqual(b["class"], "REPORT")
        self.assertTrue(any("not in origin/main" in r for r in b["reasons"]))

    def test_age_only_controls_the_auto_eligible_flag(self) -> None:
        young = self.by_name(self.collect(min_age=7))["done-merged"]
        self.assertEqual(young["class"], "PROPOSE-TO-OWNER")
        self.assertFalse(young["auto_eligible"])
        self.assertTrue(self.by_name(self.collect(min_age=0))["done-merged"]["auto_eligible"])

    def test_report_changes_nothing(self) -> None:
        before = self.snapshot()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = janitor.main(["--repo", str(self.repo), "--coord", str(self.coord), "--out", str(self.dir / "report.md")])
        self.assertEqual(code, 0)
        self.assertEqual(before, self.snapshot())
        text = out.getvalue()
        self.assertIn("report only: nothing was deleted", text)
        self.assertIn("git branch -d done-merged", text)  # shown, never run
        self.assertIn("done-merged", self.run_git(self.repo, "branch"))
        self.assertTrue((self.dir / "report.md").is_file())

    def test_register_missing_only_appends(self) -> None:
        reg = self.coord / "registry" / "branches.json"
        reg.write_text(json.dumps({"keep": "this", "local_branches": [{"name": "done-merged", "state": "parked", "note": "hand-written"}]}), encoding="utf-8")
        rep = self.collect()
        self.assertEqual(janitor.register_missing(self.coord, rep), 0)
        data = json.loads(reg.read_text(encoding="utf-8"))
        self.assertEqual(data["keep"], "this")
        names = {b["name"]: b for b in data["local_branches"]}
        self.assertEqual(names["done-merged"]["state"], "parked")
        self.assertEqual(names["done-merged"]["note"], "hand-written")
        self.assertEqual(names["wip-unmerged"]["state"], "active")
        self.assertNotIn("main", names)

    def test_registry_with_unescaped_backslashes_is_read(self) -> None:
        (self.coord / "registry" / "branches.json").write_text('{"repo_F_kohbor": {"worktrees": [{"path": "F:\\kohbor", "branch": "main"}]}}', encoding="utf-8")
        rep = self.collect()
        self.assertEqual(self.by_name(rep)["main"]["class"], "BASE")

    def test_red_flag_for_too_many_worktrees(self) -> None:
        for i in range(4):
            self.run_git(self.repo, "worktree", "add", "-q", str(self.dir / f"w{i}"), "-b", f"wt-branch-{i}")
        rep = self.collect()
        self.assertTrue(any("worktrees (limit 3)" in f for f in rep["red_flags"]))

    def test_missing_origin_blocks_a_proposal_but_is_not_an_error(self) -> None:
        self.run_git(self.repo, "remote", "remove", "origin")
        rep = self.collect()
        b = self.by_name(rep)["done-merged"]
        self.assertEqual(b["class"], "REPORT")
        self.assertFalse(b["auto_eligible"])
        self.assertTrue(any("cannot show that the work is pushed" in r for r in b["reasons"]))

    def test_unparsable_age_is_never_auto_eligible(self) -> None:
        rep = janitor.collect(self.repo, self.coord, "main", 7, dt.datetime.now(dt.timezone.utc) - dt.timedelta(days=400))
        # a "now" far in the past makes every age negative: unknown or future ages must not be eligible
        self.assertTrue(all(b["age_days"] < 0 for b in rep["branches"]))
        self.assertFalse(any(b.get("auto_eligible") for b in rep["branches"]))

    def test_registry_lock_blocks_a_concurrent_writer_and_keeps_a_backup(self) -> None:
        reg = self.coord / "registry" / "branches.json"
        reg.write_text(json.dumps({"keep": "this"}), encoding="utf-8")
        lock = reg.with_name(reg.name + ".lock")
        lock.write_text("held", encoding="utf-8")
        rep = self.collect()
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            self.assertEqual(janitor.register_missing(self.coord, rep), 1)
        self.assertIn("another session is writing", err.getvalue())
        self.assertEqual(json.loads(reg.read_text(encoding="utf-8")), {"keep": "this"})
        lock.unlink()
        self.assertEqual(janitor.register_missing(self.coord, rep), 0)
        self.assertFalse(lock.exists())
        self.assertEqual(json.loads(reg.with_name(reg.name + ".bak").read_text(encoding="utf-8")), {"keep": "this"})

    def test_out_inside_the_repository_is_refused(self) -> None:
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
            code = janitor.main(["--repo", str(self.repo), "--coord", str(self.coord), "--out", str(self.repo / "report.md")])
        self.assertEqual(code, 1)
        self.assertFalse((self.repo / "report.md").exists())


class JanitorSourceGuardTests(unittest.TestCase):
    """Static guard: runs in every gate mode, including --quick, because it needs no git."""

    def test_the_tool_contains_no_delete_command(self) -> None:
        import ast

        source = (Path(__file__).resolve().parents[1] / "janitor.py").read_text(encoding="utf-8")
        tree = ast.parse(source)
        forbidden = {"-d", "-D", "remove", "prune", "reset", "push", "clean", "checkout", "gc", "rm", "update-ref", "worktree-remove", "stash-drop", "--force"}
        git_calls = 0
        for node in ast.walk(tree):
            if isinstance(node, ast.Call):
                name = getattr(node.func, "id", getattr(node.func, "attr", ""))
                strings = {c.value for c in ast.walk(node) if isinstance(c, ast.Constant) and isinstance(c.value, str)}
                if name == "git":
                    git_calls += 1
                    self.assertFalse(forbidden & strings, ast.dump(node))
                    # the only worktree/stash/branch sub-commands allowed are the read-only ones
                    if "worktree" in strings:
                        self.assertEqual(strings & {"list", "add"}, {"list"})
                    if "stash" in strings:
                        self.assertEqual(strings & {"list", "drop", "pop", "apply", "clear"}, {"list"})
                if name in {"rmtree", "remove", "rmdir", "removedirs"}:
                    self.fail("deletion call in janitor.py: " + name)
                if name == "unlink":
                    # only the janitor's own lock file may be unlinked
                    self.assertEqual(ast.unparse(node.func), "lock.unlink")
                if name == "run" and getattr(node.func, "value", None) is not None and getattr(node.func.value, "id", "") == "subprocess":
                    self.assertIn("--no-optional-locks", ast.unparse(node))
        self.assertGreater(git_calls, 5)
        self.assertIn("--no-optional-locks", source)


if __name__ == "__main__":
    unittest.main()
