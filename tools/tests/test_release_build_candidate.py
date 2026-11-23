from __future__ import annotations

import io
import os
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch


RELEASE_TOOLS = Path(__file__).resolve().parents[1] / "release"
sys.path.insert(0, str(RELEASE_TOOLS))

import build_candidate  # noqa: E402


class BuildCandidateTests(unittest.TestCase):
    def arguments(self, out: Path, dry_run: bool = False) -> list[str]:
        args = ["--tauri-cli", "F:\\fake\\tauri.cmd", "--out", str(out)]
        if dry_run:
            args.append("--dry-run")
        return args

    def test_refuses_dirty_worktree(self) -> None:
        root = Path(__file__).resolve().parents[2]
        with self.subTest("dirty worktree"):
            environment = {
                "TAURI_SIGNING_PRIVATE_KEY": "synthetic-key-value",
                "TAURI_SIGNING_PRIVATE_KEY_PASSWORD": "synthetic-password-value",
            }
            stdout = io.StringIO()
            stderr = io.StringIO()
            with (
                patch.object(build_candidate, "REPO_ROOT", root),
                patch.object(build_candidate, "git_text", return_value=" M tracked.py"),
                patch.dict(os.environ, environment, clear=False),
                redirect_stdout(stdout),
                redirect_stderr(stderr),
            ):
                result = build_candidate.main(self.arguments(root / "out"))
            self.assertEqual(result, 2)
            self.assertIn("worktree is dirty", stderr.getvalue())
            self.assertNotIn(environment["TAURI_SIGNING_PRIVATE_KEY"], stdout.getvalue() + stderr.getvalue())
            self.assertNotIn(environment["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"], stdout.getvalue() + stderr.getvalue())

    def test_refuses_missing_signing_variables_by_name(self) -> None:
        root = Path(__file__).resolve().parents[2]
        with self.subTest("missing signing variables"):
            stdout = io.StringIO()
            stderr = io.StringIO()
            with (
                patch.object(build_candidate, "REPO_ROOT", root),
                patch.object(build_candidate, "git_text", return_value=""),
                patch.dict(os.environ, {}, clear=True),
                redirect_stdout(stdout),
                redirect_stderr(stderr),
            ):
                result = build_candidate.main(self.arguments(root / "out"))
            self.assertEqual(result, 2)
            message = stdout.getvalue() + stderr.getvalue()
            self.assertIn("TAURI_SIGNING_PRIVATE_KEY", message)
            self.assertIn("TAURI_SIGNING_PRIVATE_KEY_PATH", message)
            self.assertIn("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", message)

    def test_dry_run_lists_command_and_names_without_secret_values(self) -> None:
        root = Path(__file__).resolve().parents[2]
        with self.subTest("dry run"):
            secrets = {
                "TAURI_SIGNING_PRIVATE_KEY": "synthetic-key-value-never-print",
                "TAURI_SIGNING_PRIVATE_KEY_PASSWORD": "synthetic-password-never-print",
            }
            stdout = io.StringIO()
            stderr = io.StringIO()
            with (
                patch.dict(os.environ, secrets, clear=False),
                redirect_stdout(stdout),
                redirect_stderr(stderr),
            ):
                result = build_candidate.main(self.arguments(root / "out", dry_run=True))
            output = stdout.getvalue() + stderr.getvalue()
            self.assertEqual(result, 0)
            self.assertIn("build --bundles nsis --features updater", output)
            self.assertIn("--locked", output)
            self.assertIn("CARGO_NET_OFFLINE", output)
            self.assertIn("COREPACK_ENABLE_NETWORK", output)
            self.assertIn("PNPM_CONFIG_OFFLINE", output)
            self.assertIn("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", output)
            for value in secrets.values():
                self.assertNotIn(value, output)


if __name__ == "__main__":
    unittest.main()
