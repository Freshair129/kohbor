#!/usr/bin/env python3
"""Run the repository's configured automated checks.

Usage: ``python tools/gate.py [--quick]``. The default runs the full gate;
``--quick`` runs specification validation and checks that do not build. Each
check is summarized on stdout and its complete output is kept in a temp log.
"""

from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import time
from typing import BinaryIO, Iterator


ROOT = Path(__file__).resolve().parents[1]
FRONTEND = ROOT / "apps" / "desktop"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run Kohbor repository checks.")
    parser.add_argument(
        "--quick",
        action="store_true",
        help="run spec validation and fast checks; omit Rust and frontend builds",
    )
    return parser.parse_args()


def external_temp_dir() -> Path:
    """Return a system temp directory that is definitely outside this repo."""
    path = Path(tempfile.gettempdir()).resolve()
    if path == ROOT or ROOT in path.parents:
        raise OSError(f"system temp directory is inside the repository: {path}")
    return path


def format_command(args: list[str]) -> str:
    if os.name == "nt":
        return subprocess.list2cmdline(args)
    return shlex.join(args)


def command_runner(args: list[str]) -> tuple[list[str] | str, bool]:
    """Use cmd.exe only for Windows command shims such as corepack.cmd."""
    if os.name == "nt" and Path(args[0]).suffix.lower() in {".cmd", ".bat"}:
        return subprocess.list2cmdline(args), True
    return args, False


def output_for_check(log_path: Path, offset: int) -> str:
    try:
        with log_path.open("rb") as stream:
            stream.seek(offset)
            return stream.read().decode("utf-8", errors="replace")
    except OSError:
        return ""


def first_output_line(output: str) -> str:
    for raw_line in output.splitlines():
        line = raw_line.strip()
        if line:
            return line[:220]
    return "command exited unsuccessfully"


def add_result(
    results: list[dict[str, object]],
    log: BinaryIO,
    name: str,
    status: str,
    elapsed: float,
    reason: str = "",
    *,
    selected: bool = True,
) -> None:
    result: dict[str, object] = {
        "name": name,
        "status": status,
        "seconds": elapsed,
        "reason": reason,
        "selected": selected,
    }
    results.append(result)
    suffix = f" - {reason}" if reason else ""
    line = f"{status} {name} {elapsed:.2f}s{suffix}"
    print(line, flush=True)
    log.write((line + "\n").encode("utf-8", errors="replace"))
    log.flush()


def run_check(
    results: list[dict[str, object]],
    log_path: Path,
    log: BinaryIO,
    *,
    name: str,
    args: list[str],
    cwd: Path = ROOT,
    env: dict[str, str] | None = None,
    required_tool: str,
) -> None:
    started = time.monotonic()
    executable = shutil.which(required_tool)
    if executable is None:
        reason = f"required tool '{required_tool}' was not found on PATH"
        log.write((f"\n### {name}\n{reason}\n").encode("utf-8"))
        log.flush()
        add_result(results, log, name, "FAIL", time.monotonic() - started, reason)
        return

    args = [executable if item == f"{{tool:{required_tool}}}" else item for item in args]
    command, use_shell = command_runner(args)
    header = f"\n### {name}\n$ {format_command(args)}\n"
    log.write(header.encode("utf-8", errors="replace"))
    log.flush()
    output_offset = log.tell()
    try:
        completed = subprocess.run(
            command,
            cwd=str(cwd),
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
            shell=use_shell,
            check=False,
        )
    except FileNotFoundError:
        reason = f"required tool '{required_tool}' could not be started"
        add_result(results, log, name, "FAIL", time.monotonic() - started, reason)
    except OSError as exc:
        reason = f"could not run check: {exc}"
        add_result(results, log, name, "SKIP", time.monotonic() - started, reason)
    else:
        elapsed = time.monotonic() - started
        if completed.returncode == 0:
            add_result(results, log, name, "PASS", elapsed)
        else:
            output = output_for_check(log_path, output_offset)
            if "could not acquire lock" in output.casefold():
                cause = next(
                    (line.strip() for line in output.splitlines() if "cause:" in line.casefold()),
                    "the tool could not acquire its cache lock",
                )
                add_result(results, log, name, "SKIP", elapsed, f"tool cache lock unavailable: {cause[:180]}")
                return
            detail = first_output_line(output)
            reason = f"exit code {completed.returncode}: {detail}"
            add_result(results, log, name, "FAIL", elapsed, reason)


def skip_result(
    results: list[dict[str, object]],
    log: BinaryIO,
    name: str,
    reason: str,
    *,
    selected: bool = True,
) -> None:
    log.write((f"\n### {name}\nSKIP: {reason}\n").encode("utf-8", errors="replace"))
    log.flush()
    add_result(results, log, name, "SKIP", 0.0, reason, selected=selected)


def cargo_target_dir() -> tuple[Path | None, str]:
    configured = os.environ.get("KOHBOR_CARGO_TARGET_DIR")
    if configured:
        target = Path(configured).expanduser().resolve()
    elif ROOT.drive:
        target = Path(ROOT.drive + os.sep) / "cargo-lanes" / "kohbor-gate"
    else:
        return None, "cannot resolve an F: Cargo lane from this workspace path"

    try:
        resolved = target.resolve()
        if not ROOT.drive or resolved.drive.casefold() != ROOT.drive.casefold():
            return None, f"Cargo target must be on {ROOT.drive or 'F:'}: {resolved}"
        if resolved == ROOT or ROOT in resolved.parents:
            return None, f"Cargo target must be outside the repository: {resolved}"
        lane_root = Path(ROOT.drive + os.sep) / "cargo-lanes"
        if resolved == lane_root or lane_root not in resolved.parents:
            return None, f"Cargo target must use a dedicated directory under {lane_root}: {resolved}"
        return resolved, ""
    except OSError as exc:
        return None, f"cannot resolve Cargo target path: {exc}"


@contextlib.contextmanager
def cargo_lane_lock(target: Path) -> Iterator[None]:
    """Serialize gate Cargo builds sharing this dedicated target directory."""
    target.mkdir(parents=True, exist_ok=True)
    lock_path = target / ".gate.lock"
    with lock_path.open("a+b") as lock_file:
        lock_file.seek(0, os.SEEK_END)
        if lock_file.tell() == 0:
            lock_file.write(b"\0")
            lock_file.flush()
        lock_file.seek(0)
        if os.name == "nt":
            import msvcrt

            try:
                msvcrt.locking(lock_file.fileno(), msvcrt.LK_NBLCK, 1)
            except OSError as exc:
                raise BlockingIOError("Cargo gate lane is locked by another process") from exc
            try:
                yield
            finally:
                lock_file.seek(0)
                msvcrt.locking(lock_file.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl

            try:
                fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            except OSError as exc:
                raise BlockingIOError("Cargo gate lane is locked by another process") from exc
            try:
                yield
            finally:
                fcntl.flock(lock_file.fileno(), fcntl.LOCK_UN)


def package_scripts() -> tuple[dict[str, object] | None, str, str]:
    manifest = FRONTEND / "package.json"
    try:
        data = json.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        return None, "", f"cannot read apps/desktop/package.json: {exc}"
    if not isinstance(data, dict) or not isinstance(data.get("scripts"), dict):
        return None, "", "apps/desktop/package.json has no valid scripts object"
    package_manager = data.get("packageManager")
    if not isinstance(package_manager, str):
        return data["scripts"], "", "packageManager must pin a pnpm version"
    match = re.fullmatch(r"pnpm@([^+\s]+)(?:\+.*)?", package_manager)
    if not match:
        return data["scripts"], "", "packageManager must pin a pnpm version"
    return data["scripts"], match.group(1), ""


def rust_lint_command() -> list[str]:
    toolchain_file = ROOT / "rust-toolchain.toml"
    try:
        contents = toolchain_file.read_text(encoding="utf-8")
    except OSError:
        contents = ""
    components = re.search(r"(?m)^components\s*=\s*\[([^\]]*)\]", contents)
    has_clippy = components is not None and re.search(r"['\"]clippy['\"]", components.group(1)) is not None
    if has_clippy:
        return [
            "{tool:cargo}",
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]
    return ["{tool:cargo}", "check", "--workspace", "--all-targets"]


def main() -> int:
    args = parse_args()
    try:
        temp_root = external_temp_dir()
        log_fd, log_name = tempfile.mkstemp(prefix="kohbor-gate-", suffix=".log", dir=temp_root)
    except OSError as exc:
        print(f"FAIL gate setup 0.00s - cannot create external log file: {exc}")
        return 1

    log_path = Path(log_name)
    results: list[dict[str, object]] = []
    started = dt.datetime.now(dt.timezone.utc).isoformat()

    with os.fdopen(log_fd, "wb") as log:
        log.write(f"Kohbor gate started {started}\nRepository: {ROOT}\nMode: {'quick' if args.quick else 'full'}\n".encode("utf-8"))
        log.flush()

        run_check(
            results,
            log_path,
            log,
            name="Rust format",
            args=["{tool:cargo}", "fmt", "--all", "--", "--check"],
            required_tool="cargo",
        )
        if not args.quick:
            cargo_build_checks = [
                ("Rust clippy/check", rust_lint_command()),
                ("Rust workspace tests", ["{tool:cargo}", "test", "--workspace"]),
            ]
            if shutil.which("cargo") is None:
                for name, command_args in cargo_build_checks:
                    run_check(
                        results,
                        log_path,
                        log,
                        name=name,
                        args=command_args,
                        required_tool="cargo",
                    )
            else:
                target, target_error = cargo_target_dir()
                if target is None:
                    for name, _ in cargo_build_checks:
                        skip_result(results, log, name, target_error)
                else:
                    cargo_env = os.environ.copy()
                    # Only the child Cargo processes receive this F: lane setting.
                    cargo_env["CARGO_TARGET_DIR"] = str(target)
                    try:
                        with cargo_lane_lock(target):
                            for name, command_args in cargo_build_checks:
                                run_check(
                                    results,
                                    log_path,
                                    log,
                                    name=name,
                                    args=command_args,
                                    env=cargo_env,
                                    required_tool="cargo",
                                )
                    except (BlockingIOError, OSError) as exc:
                        reason = str(exc) or f"cannot initialize Cargo lane at {target}"
                        for name, _ in cargo_build_checks:
                            skip_result(results, log, name, reason)

        scripts, pnpm_version, manifest_error = package_scripts()
        frontend_names = ["typecheck", "lint", "test"]
        if not args.quick:
            frontend_names.append("build")
        if scripts is None:
            for name in frontend_names:
                skip_result(results, log, f"Frontend {name}", manifest_error)
        else:
            configured_names = [name for name in frontend_names if name in scripts]
            if configured_names:
                if shutil.which("corepack") is None:
                    run_check(
                        results,
                        log_path,
                        log,
                        name="Frontend package manager",
                        args=["{tool:corepack}", f"pnpm@{pnpm_version}", "--version"],
                        required_tool="corepack",
                    )
                elif manifest_error:
                    skip_result(results, log, "Frontend package manager", manifest_error)
                else:
                    run_check(
                        results,
                        log_path,
                        log,
                        name="Frontend package manager",
                        args=["{tool:corepack}", f"pnpm@{pnpm_version}", "--version"],
                        required_tool="corepack",
                    )
            for name in frontend_names:
                if name not in scripts:
                    skip_result(
                        results,
                        log,
                        f"Frontend {name}",
                        f"no '{name}' script is configured in apps/desktop/package.json",
                        selected=False,
                    )
                    continue
                if shutil.which("corepack") is None:
                    run_check(
                        results,
                        log_path,
                        log,
                        name=f"Frontend {name}",
                        args=["{tool:corepack}", f"pnpm@{pnpm_version}", "--dir", str(FRONTEND), "run", name],
                        cwd=ROOT,
                        required_tool="corepack",
                    )
                    continue
                if manifest_error:
                    skip_result(results, log, f"Frontend {name}", manifest_error)
                    continue
                if not (FRONTEND / "node_modules").is_dir():
                    skip_result(
                        results,
                        log,
                        f"Frontend {name}",
                        "apps/desktop/node_modules is missing; dependency installation is not part of the gate",
                    )
                    continue
                command_args = [
                    "{tool:corepack}",
                    f"pnpm@{pnpm_version}",
                    "--dir",
                    str(FRONTEND),
                    "run",
                    name,
                ]
                run_check(
                    results,
                    log_path,
                    log,
                    name=f"Frontend {name}",
                    args=command_args,
                    cwd=ROOT,
                    required_tool="corepack",
                )

        if shutil.which("uv") is None:
            run_check(
                results,
                log_path,
                log,
                name="Spec validation",
                args=["{tool:uv}"],
                required_tool="uv",
            )
        else:
            report_path: Path | None = None
            try:
                report_fd, report_name = tempfile.mkstemp(
                    prefix="kohbor-gate-spec-", suffix=".json", dir=temp_root
                )
                os.close(report_fd)
                report_path = Path(report_name)
            except OSError as exc:
                skip_result(results, log, "Spec validation", f"cannot create an external report path: {exc}")
            if report_path is not None:
                try:
                    run_check(
                        results,
                        log_path,
                        log,
                        name="Spec validation",
                        args=[
                            "{tool:uv}",
                            "run",
                            "--no-project",
                            "--with",
                            "jsonschema",
                            "python",
                            str(ROOT / "tools" / "validate_spec.py"),
                            "--report",
                            str(report_path),
                        ],
                        required_tool="uv",
                    )
                finally:
                    try:
                        report_path.unlink()
                    except FileNotFoundError:
                        pass
                    except OSError as exc:
                        log.write((f"\nCould not remove temporary spec report {report_path}: {exc}\n").encode("utf-8"))
                        log.flush()

        selected_failures = sum(
            result["selected"] and result["status"] == "FAIL" for result in results
        )
        selected_skips = sum(
            result["selected"] and result["status"] == "SKIP" for result in results
        )
        all_skips = sum(result["status"] == "SKIP" for result in results)
        passed = sum(result["selected"] and result["status"] == "PASS" for result in results)

        if selected_failures:
            summary = f"FAIL: {selected_failures} selected check(s) failed"
            if all_skips:
                summary += f"; NOT FULLY VERIFIED ({all_skips} check(s) skipped)"
        elif selected_skips:
            summary = f"NOT FULLY VERIFIED: {selected_skips} selected check(s) skipped"
        else:
            summary = f"PASS: all {passed} selected check(s) passed"
            if all_skips:
                summary += f"; NOT FULLY VERIFIED ({all_skips} check(s) not configured)"
        summary += f". Full log: {log_path}"
        print(summary, flush=True)
        log.write((summary + "\n").encode("utf-8", errors="replace"))
        log.flush()

    return 1 if selected_failures or selected_skips else 0


if __name__ == "__main__":
    raise SystemExit(main())
