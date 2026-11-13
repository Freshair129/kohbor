"""Tests for tools/check_claim_scope.py. Synthetic fixtures only."""

from __future__ import annotations

import contextlib
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

class ScopePatternTests(unittest.TestCase):
    ROOT = os.path.normcase(os.path.normpath("F:/repo" if os.name == "nt" else "/repo"))

    def match(self, pattern: str, path: str) -> bool:
        return bool(ccs.glob_to_regex(pattern).match(path))

    def test_globs(self) -> None:
        self.assertTrue(self.match("docs/domains/**", "docs/domains/a/CARD.md"))
        self.assertTrue(self.match("docs/**/x.md", "docs/x.md"))
        self.assertTrue(self.match("tools/*.py", "tools/gate.py"))
        self.assertFalse(self.match("tools/*.py", "tools/tests/t.py"))
        self.assertTrue(self.match("crates/kohbor-pack", "crates/kohbor-pack/src/lib.rs"))
        self.assertFalse(self.match("crates/kohbor-pack", "crates/kohbor-pack-author/src/lib.rs"))
        self.assertTrue(self.match("AGENTS.md", "AGENTS.md"))
        self.assertFalse(self.match("AGENTS.md", "docs/AGENTS.md"))
        self.assertEqual(ccs.expand_braces("src/{a,b}/x"), ["src/a/x", "src/b/x"])

    def test_claim_entries(self) -> None:
        root_text = "F:\\repo" if os.name == "nt" else "/repo"
        claim = {"write_area": ["docs/domains/** (cards)", "tools/gate.py", "tests for the above", "src/{a,b}.rs",
                                root_text + ("\\crates\\x" if os.name == "nt" else "/crates/x"), "git worktree somewhere (branch y)"]}
        patterns, ignored, whole = ccs.patterns_from_claim(claim, [self.ROOT])
        self.assertEqual(patterns, ["docs/domains/**", "tools/gate.py", "src/a.rs", "src/b.rs", "crates/x"])
        self.assertEqual(ignored, ["tests for the above", "git worktree somewhere (branch y)"])
        self.assertFalse(whole)
        _, _, whole = ccs.patterns_from_claim({"write_area": [root_text + " (main checkout, branch z)"]}, [self.ROOT])
        self.assertTrue(whole)


@unittest.skipIf(shutil.which("git") is None or os.environ.get("KOHBOR_TOOL_TESTS_SKIP_GIT") == "1", "git is not available or the git tests are skipped (gate --quick)")
class ScopeGitTests(unittest.TestCase):
    GIT = ["git", "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"]

    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="scope-test-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.repo = self.dir / "repo"
        self.coord = self.dir / "coord"
        (self.coord / "claims").mkdir(parents=True)
        self.git(self.dir, "init", "-q", "-b", "main", str(self.repo))
        self.put("README.md", "x")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "base")
        self.git(self.repo, "checkout", "-q", "-b", "feat/x")

    def git(self, cwd: Path, *args: str) -> str:
        return subprocess.run([*self.GIT, *args], cwd=cwd, capture_output=True, text=True, check=True).stdout

    def put(self, rel: str, text: str) -> None:
        p = self.repo / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")

    def claim(self, name: str, **fields) -> None:
        data = {"status": "active", "branch": "feat/x", "write_area": ["crates/a/**"]}
        data.update(fields)
        (self.coord / "claims" / name).write_text(json.dumps(data), encoding="utf-8")

    def run_tool(self, *extra: str):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = ccs.main(["--repo", str(self.repo), "--coord", str(self.coord), *extra])
        return code, out.getvalue()

    def test_inside_outside_shared_and_untracked(self) -> None:
        self.claim("a__t.json")
        self.put("crates/a/src/lib.rs", "x")
        self.put("package-manifest.json", "{}")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "work")
        code, out = self.run_tool()
        self.assertEqual(code, 0, out)
        self.assertIn("2 changed, 1 inside, 1 shared, 0 outside", out)
        self.put("crates/b/src/lib.rs", "x")  # untracked, outside the claim
        code, out = self.run_tool()
        self.assertEqual(code, 0, out)  # report-only
        self.assertIn("OUTSIDE crates/b/src/lib.rs", out)
        code, out = self.run_tool("--enforce")
        self.assertEqual(code, 1, out)

    def test_no_claim_is_reported_and_fails_only_when_enforced(self) -> None:
        self.put("crates/a/x.rs", "x")
        code, out = self.run_tool()
        self.assertEqual(code, 0, out)
        self.assertIn("has no active claim", out)
        self.assertEqual(self.run_tool("--enforce")[0], 1)

    def test_claim_of_another_branch_does_not_count(self) -> None:
        self.claim("other.json", branch="feat/y", write_area=["**"])
        self.put("crates/a/x.rs", "x")
        code, out = self.run_tool("--enforce")
        self.assertEqual(code, 1, out)
        self.assertIn("has no active claim", out)

    def test_a_closed_claim_covers_only_the_tree_it_was_closed_at(self) -> None:
        self.put("crates/a/x.rs", "x")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "work")
        head = self.git(self.repo, "rev-parse", "--short", "HEAD").strip()
        self.claim("done.json", status="done", last_commit=head)
        self.assertEqual(self.run_tool("--enforce")[0], 1)  # --include-done is required
        code, out = self.run_tool("--enforce", "--include-done")
        self.assertEqual(code, 0, out)
        self.put("crates/a/y.rs", "y")  # dirty tree after the claim was closed
        code, out = self.run_tool("--enforce", "--include-done")
        self.assertEqual(code, 1, out)
        self.assertIn("WORK AFTER CLOSE claim done.json", out)
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "later")  # HEAD moved on
        code, out = self.run_tool("--enforce", "--include-done")
        self.assertEqual(code, 1, out)
        self.assertIn("WORK AFTER CLOSE", out)

    def test_base_commit_of_the_claim_is_the_default_base(self) -> None:
        self.git(self.repo, "checkout", "-q", "main")
        self.put("elsewhere/m.txt", "m")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "main moves on")
        self.git(self.repo, "checkout", "-q", "feat/x")
        self.git(self.repo, "checkout", "-q", "-b", "stacked")
        base = self.git(self.repo, "rev-parse", "--short", "HEAD").strip()
        self.put("crates/a/s.rs", "s")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "stacked work")
        self.claim("s.json", branch="stacked", base_commit=base, started="2026-11-13T00:00:00+00:00")
        code, out = self.run_tool("--enforce")
        self.assertEqual(code, 0, out)
        self.assertIn(f"base {base} (claim base_commit)", out)
        code, out = self.run_tool("--enforce", "--base", "main")  # main is not an ancestor of the claim base: the override would narrow it
        self.assertIn(f"base {base} (claim base_commit)", out)
        self.assertIn("is not an ancestor of the claim", out)
        root = self.git(self.repo, "rev-list", "--max-parents=0", "HEAD").strip()
        code, out = self.run_tool("--enforce", "--base", root)  # an ancestor widens the range
        self.assertIn("claim base_commit + --base", out)
        self.assertIn("crates/a/s.rs", self.run_tool("--json", "--base", root)[1])

    def test_committed_rename_reports_both_ends(self) -> None:
        self.put("protected/secret.py", "x")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "add")
        self.git(self.repo, "checkout", "-q", "-b", "mover")
        (self.repo / "tools_dir").mkdir()
        self.git(self.repo, "mv", "protected/secret.py", "tools_dir/secret.py")
        self.git(self.repo, "commit", "-q", "-m", "rename")
        self.claim("m.json", branch="mover", write_area=["tools_dir/**"])
        data = json.loads(self.run_tool("--json", "--base", "feat/x")[1])
        self.assertIn("protected/secret.py", data["outside"])
        self.assertIn("tools_dir/secret.py", data["inside"])

    def test_git_failure_is_a_problem_not_an_empty_result(self) -> None:
        self.claim("a__t.json")
        code, out = self.run_tool("--enforce", "--base", "no-such-ref")
        self.assertEqual(code, 1, out)
        self.assertIn("PROBLEM check_claim_scope crashed", out)
        self.assertIn("does not resolve", out)
        code, out = ccs_run_outside_repo(self)
        self.assertEqual(code, 1, out)

    def test_explicit_claim_file_follows_the_same_rules(self) -> None:
        self.claim("done.json", status="done", last_commit="0000000")
        self.put("crates/a/x.rs", "x")
        f = self.coord / "claims" / "done.json"
        code, out = self.run_tool("--claim", str(f), "--enforce", "--include-done")
        self.assertEqual(code, 1, out)
        self.assertIn("WORK AFTER CLOSE claim done.json", out)
        self.claim("other.json", branch="another")
        code, out = self.run_tool("--claim", str(self.coord / "claims" / "other.json"), "--enforce")
        self.assertEqual(code, 1, out)
        self.assertIn("belongs to another branch", out)

    def test_pattern_entries_with_spaces_and_thai_names_are_paths(self) -> None:
        patterns, ignored, _ = ccs.patterns_from_claim({"write_area": ["tools/My File.py", "tools/ไทย.py", "tests for the above"]}, [])
        self.assertEqual(patterns, ["tools/My File.py", "tools/ไทย.py"])
        self.assertEqual(ignored, ["tests for the above"])

    def test_unresolved_baseline_is_a_problem(self) -> None:
        self.claim("a__t.json", base_commit="deadbeef")
        code, out = self.run_tool("--enforce")
        self.assertEqual(code, 1, out)
        self.assertIn("base_commit 'deadbeef' does not resolve", out)

    def test_an_ancestor_override_adds_coverage_and_never_removes_it(self) -> None:
        first = self.git(self.repo, "rev-parse", "--short", "HEAD").strip()
        self.put("protected/x.txt", "A")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "old")
        old = self.git(self.repo, "rev-parse", "--short", "HEAD").strip()
        self.put("protected/x.txt", "B")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "claim base changes it")
        base = self.git(self.repo, "rev-parse", "--short", "HEAD").strip()
        self.put("protected/x.txt", "A")  # the task reverts it to the OLD content
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "task reverts")
        self.claim("a__t.json", base_commit=base, write_area=["crates/a/**"])
        data = json.loads(self.run_tool("--json", "--base", old)[1])
        self.assertIn("protected/x.txt", data["outside"])  # present in the claim-base diff even though --base OLD sees no net change
        self.assertIn("claim base_commit + --base", data["base_source"])

    def test_done_claim_needs_the_flag_and_the_exact_head_commit(self) -> None:
        self.put("crates/a/x.rs", "x")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "work")
        head = self.git(self.repo, "rev-parse", "--short", "HEAD").strip()
        self.claim("done.json", status="done", last_commit=head[:1])  # a one-character prefix is not a commit
        code, out = self.run_tool("--include-done", "--enforce")
        self.assertEqual(code, 1, out)
        self.assertIn("WORK AFTER CLOSE", out)
        self.claim("done.json", status="done", last_commit=head + "NOT-A-COMMIT")
        self.assertEqual(self.run_tool("--include-done", "--enforce")[0], 1)
        self.claim("done.json", status="done", last_commit=head)
        self.assertEqual(self.run_tool("--include-done", "--enforce")[0], 0)
        self.assertEqual(self.run_tool("--claim", str(self.coord / "claims" / "done.json"), "--enforce")[0], 1)  # --claim does not imply --include-done

    def test_paths_with_unicode_marks_quotes_and_root_names_are_claim_patterns(self) -> None:
        for entry in ("My File.py", "docs/ชื่อไทย.md", "docs/qu'ote.md", "ภาษาไทย ที่.md", "src/{a,b}.rs"):
            with self.subTest(entry=entry):
                patterns, ignored, _ = ccs.patterns_from_claim({"write_area": [entry]}, [])
                self.assertEqual(ignored, [], entry)
                self.assertTrue(patterns)
        for prose in ("tests for the above", "git worktree F:\\x (branch y)", "all the docs", "see the card"):
            with self.subTest(prose=prose):
                self.assertEqual(ccs.patterns_from_claim({"write_area": [prose]}, [])[0], [])

    def test_match_all_globs_are_vacuous(self) -> None:
        for glob in ("**", "**/*", "*"):
            with self.subTest(glob=glob):
                self.claim("a__t.json", write_area=[glob])
                self.put("protected/x.py", "x")
                data = json.loads(self.run_tool("--json")[1])
                self.assertTrue(data["vacuous"], glob)

    def test_brief_scope_keeps_brace_groups_and_accepts_an_explicit_none(self) -> None:
        brief = self.dir / "b.txt"
        brief.write_text("TASK_ID: T1\nSCOPE: src/{a,b}.rs, docs/x.md\n", encoding="utf-8")
        self.put("src/a.rs", "x")
        data = json.loads(self.run_brief_tool(brief)[1])
        self.assertEqual((data["outside"], data["inside"]), ([], ["src/a.rs"]))
        (self.repo / "src" / "a.rs").unlink()
        brief.write_text("TASK_ID: T2\nSCOPE: none: you change nothing\n", encoding="utf-8")
        code, out = self.run_brief_tool(brief)
        self.assertEqual(code, 0, out)  # a clean read-only brief is not a missing scope
        self.put("src/new.rs", "x")
        self.assertEqual(self.run_brief_tool(brief)[0], 1)

    def run_brief_tool(self, brief):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = ccs.main(["--repo", str(self.repo), "--brief", str(brief), "--json"])
        return code, out.getvalue()

    def test_a_copy_record_does_not_add_the_unchanged_source(self) -> None:
        entries = ["C  tools_dir/copy.py", "protected/source.py", "R  new/name.py", "old/name.py"]
        self.assertEqual(ccs.nul_list("\0".join(entries)), entries)
        self.put("protected/source.py", "x")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "src")
        (self.repo / "tools_dir").mkdir()
        shutil.copy(self.repo / "protected" / "source.py", self.repo / "tools_dir" / "copy.py")
        self.git(self.repo, "add", "-A")
        paths, _ = ccs.status_paths(self.repo)
        self.assertEqual(paths, ["tools_dir/copy.py"])

    def test_whole_checkout_claim_says_the_check_is_vacuous(self) -> None:
        self.claim("whole.json", write_area=[str(self.repo) + " (main checkout)"])
        self.put("anything/at/all.txt", "x")
        code, out = self.run_tool()
        self.assertIn("scope check is vacuous", out)

    def test_paths_with_spaces_quotes_thai_names_and_renames_are_read(self) -> None:
        self.claim("a__t.json", write_area=["crates/a/**"])
        names = ["crates/a/with space.rs", "crates/a/qu'ote.rs", "crates/a/ไทย.rs", "outside dir/ไทย file.txt"]
        for n in names:
            self.put(n, "x")
        code, out = self.run_tool("--json")
        data = json.loads(out)
        self.assertEqual(sorted(data["inside"]), sorted(names[:3]))
        self.assertEqual(data["outside"], [names[3]])
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "names")
        self.git(self.repo, "mv", "crates/a/with space.rs", "crates/a/renamed.rs")
        data = json.loads(self.run_tool("--json")[1])
        self.assertIn("crates/a/renamed.rs", data["inside"])
        self.assertIn("crates/a/with space.rs", data["inside"])

    def test_brief_checks_uncommitted_work_against_scope(self) -> None:
        brief = self.dir / "brief.txt"
        brief.write_text("TASK_ID: T9\nSCOPE: crates/a/src/**   (write)\nREAD_ONLY: crates/a/tests/**\n", encoding="utf-8")
        self.put("crates/a/src/x.rs", "x")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(ccs.main(["--repo", str(self.repo), "--brief", str(brief)]), 0)
        self.assertIn("BRIEF T9: 1 changed path(s), 1 inside SCOPE, 0 outside", out.getvalue())
        self.put("crates/a/tests/t.rs", "x")
        self.put("package-manifest.json", "{}")  # not exempt in a brief
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(ccs.main(["--repo", str(self.repo), "--brief", str(brief)]), 1)
        self.assertIn("OUTSIDE crates/a/tests/t.rs", out.getvalue())
        self.assertIn("OUTSIDE package-manifest.json", out.getvalue())
        brief.write_text("TASK_ID: T9\nGOAL: none\n", encoding="utf-8")
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(ccs.main(["--repo", str(self.repo), "--brief", str(brief)]), 1)

    def test_whole_checkout_claim_and_no_repository_change(self) -> None:
        self.claim("whole.json", write_area=[str(self.repo) + " (main checkout)"])
        self.put("anything/at/all.txt", "x")
        before = self.git(self.repo, "status", "--porcelain")
        code, out = self.run_tool("--enforce")
        self.assertEqual(code, 0, out)
        self.assertEqual(before, self.git(self.repo, "status", "--porcelain"))


def ccs_run_outside_repo(case):
    import tempfile
    d = tempfile.mkdtemp(prefix="notrepo-")
    case.addCleanup(shutil.rmtree, d, True)
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = ccs.main(["--repo", d, "--coord", str(case.coord)])
    return (1 if "PROBLEM" in out.getvalue() else code), out.getvalue()


class ScopeSourceGuardTests(unittest.TestCase):
    """Static guard: runs in every gate mode, including --quick, because it needs no git."""

    def test_tool_runs_no_writing_git_command(self) -> None:
        import ast

        for name in ("check_claim_scope.py", "validate_agents.py"):
            tree = ast.parse((Path(__file__).resolve().parents[1] / name).read_text(encoding="utf-8"))
            for node in ast.walk(tree):
                if isinstance(node, ast.Call) and getattr(node.func, "id", "") == "git":
                    strings = {c.value for c in ast.walk(node) if isinstance(c, ast.Constant) and isinstance(c.value, str)}
                    self.assertFalse(strings & {"commit", "push", "reset", "checkout", "add", "rm", "clean", "branch", "stash", "merge", "rebase", "config"}, ast.dump(node))


if __name__ == "__main__":
    unittest.main()
