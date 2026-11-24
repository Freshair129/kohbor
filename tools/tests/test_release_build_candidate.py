from __future__ import annotations

import io
import json
import os
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


RELEASE_TOOLS = Path(__file__).resolve().parents[1] / "release"
sys.path.insert(0, str(RELEASE_TOOLS))

import build_candidate  # noqa: E402
from release_common import ReleaseError  # noqa: E402


class BuildCandidateTests(unittest.TestCase):
    def arguments(
        self,
        out: Path,
        dry_run: bool = False,
        previous_version: str = "0.4.0-alpha.1",
    ) -> list[str]:
        args = [
            "--tauri-cli",
            "F:\\fake\\tauri.cmd",
            "--out",
            str(out),
            "--previous-version",
            previous_version,
        ]
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
                patch.object(build_candidate, "workspace_version", return_value="0.4.0-alpha.2"),
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
                patch.object(build_candidate, "workspace_version", return_value="0.4.0-alpha.2"),
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
            self.assertEqual(result, 2)
            self.assertIn(f"CANDIDATE_VERSION: {build_candidate.workspace_version(Path(__file__).resolve().parents[2])}", output)
            self.assertIn("PREVIOUS_VERSION: 0.4.0-alpha.1", output)
            self.assertIn("VERSION_GATE: would refuse", output)
            self.assertIn("build --bundles nsis --features lab,updater", output)
            self.assertIn(f"--config {build_candidate.RELEASE_CONFIG.resolve()}", output)
            self.assertIn("--locked", output)
            self.assertIn("CARGO_NET_OFFLINE", output)
            self.assertIn("COREPACK_ENABLE_NETWORK", output)
            self.assertIn("PNPM_CONFIG_OFFLINE", output)
            self.assertIn("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", output)
            for value in secrets.values():
                self.assertNotIn(value, output)

    def test_previous_version_is_required(self) -> None:
        stderr = io.StringIO()
        with redirect_stderr(stderr), self.assertRaises(SystemExit) as raised:
            build_candidate.main(
                ["--tauri-cli", "F:\\fake\\tauri.cmd", "--out", "F:\\fake\\out", "--dry-run"]
            )
        self.assertEqual(raised.exception.code, 2)
        self.assertIn("--previous-version", stderr.getvalue())

    def test_release_config_override_contains_release_bundle_settings(self) -> None:
        root = Path(__file__).resolve().parents[2]
        base = json.loads((root / "apps" / "desktop" / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))
        override = json.loads(build_candidate.RELEASE_CONFIG.read_text(encoding="utf-8"))
        # The base config stays non-release: no installer target and no updater artifacts.
        self.assertIs(base["bundle"]["active"], False)
        self.assertNotIn("targets", base["bundle"])
        self.assertNotIn("createUpdaterArtifacts", base["bundle"])
        self.assertEqual(
            set(override["bundle"]),
            {"active", "targets", "createUpdaterArtifacts", "windows"},
        )
        self.assertTrue(override["bundle"]["active"])
        self.assertEqual(override["bundle"]["targets"], ["nsis"])
        self.assertTrue(override["bundle"]["createUpdaterArtifacts"])
        self.assertEqual(override["bundle"]["windows"]["nsis"]["languages"], ["English", "Thai"])

    def test_rejects_malformed_expected_cargo_lock_hash(self) -> None:
        stderr = io.StringIO()
        with redirect_stderr(stderr):
            result = build_candidate.main(
                [
                    *self.arguments(Path("F:\\fake\\out"), dry_run=True),
                    "--expect-cargo-lock-sha256",
                    "not-a-sha256",
                ]
            )
        self.assertEqual(result, 2)
        self.assertIn("64 hexadecimal characters", stderr.getvalue())

    def test_cargo_lock_hash_mismatch_refuses_before_build(self) -> None:
        root = Path(__file__).resolve().parents[2]
        environment = {
            "TAURI_SIGNING_PRIVATE_KEY": "synthetic-key-value",
            "TAURI_SIGNING_PRIVATE_KEY_PASSWORD": "synthetic-password-value",
        }
        with (
            patch.object(build_candidate, "REPO_ROOT", root),
            patch.object(build_candidate, "workspace_version", return_value="0.4.0-alpha.2"),
            patch.object(build_candidate, "sha256_file", return_value="a" * 64),
            patch.dict(os.environ, environment, clear=False),
            patch.object(build_candidate.subprocess, "run") as run_build,
        ):
            with self.assertRaisesRegex(ReleaseError, "does not match"):
                build_candidate.perform_build(
                    Path(__file__),
                    Path("F:\\nonexistent\\candidate-output"),
                    "0.4.0-alpha.1",
                    "b" * 64,
                )
        run_build.assert_not_called()

    def test_previous_version_gate_refuses_before_build(self) -> None:
        root = Path(__file__).resolve().parents[2]
        with (
            patch.object(build_candidate, "REPO_ROOT", root),
            patch.object(build_candidate, "workspace_version", return_value="0.4.0-alpha.1"),
            patch.object(build_candidate.subprocess, "run") as run_build,
        ):
            with self.assertRaisesRegex(ReleaseError, "strictly higher"):
                build_candidate.perform_build(
                    Path(__file__),
                    Path("F:\\nonexistent\\candidate-output"),
                    "0.4.0-alpha.1",
                )
        run_build.assert_not_called()

    def test_git_warnings_on_stderr_are_tolerated(self) -> None:
        with patch.object(
            build_candidate.subprocess,
            "run",
            return_value=SimpleNamespace(returncode=0, stdout="abc123\n", stderr="warning: CRLF notice\n"),
        ):
            self.assertEqual(build_candidate.git_text(Path("F:\\repo"), "rev-parse", "HEAD"), "abc123")

    def test_git_nonzero_exit_still_fails(self) -> None:
        with patch.object(
            build_candidate.subprocess,
            "run",
            return_value=SimpleNamespace(returncode=1, stdout="", stderr="warning: CRLF notice\n"),
        ):
            with self.assertRaisesRegex(ReleaseError, "exit code 1"):
                build_candidate.git_text(Path("F:\\repo"), "status")


if __name__ == "__main__":
    unittest.main()
