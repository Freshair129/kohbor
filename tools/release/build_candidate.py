"""Build one signed NSIS candidate on the owner's Windows machine."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tomllib
from datetime import datetime, timezone
from pathlib import Path

from release_common import (
    ReleaseError,
    require_higher_semver,
    require_semver,
    require_sha256_hex,
    sha256_file,
)


REPO_ROOT = Path(__file__).resolve().parents[2]
APP_DIR = REPO_ROOT / "apps" / "desktop"
SIGNING_KEY_NAMES = ("TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PATH")
SIGNING_PASSWORD_NAME = "TAURI_SIGNING_PRIVATE_KEY_PASSWORD"
OFFLINE_ENV_NAMES = ("CARGO_NET_OFFLINE", "COREPACK_ENABLE_NETWORK", "PNPM_CONFIG_OFFLINE")
RELEASE_CONFIG = Path(__file__).resolve().with_name("release.conf.json")
BUILD_ARGS = [
    "build",
    "--bundles",
    "nsis",
    "--features",
    # DEC-ALPHA-SUPERUSER (owner, 2026-11-24): the super-user alpha ships the Lab build with the updater.
    "lab,updater",
    "--config",
    str(RELEASE_CONFIG),
    "--",
    "--locked",
]


def build_command(cli_path: Path) -> list[str] | str:
    raw = [str(cli_path), *BUILD_ARGS]
    if os.name == "nt" and cli_path.suffix.lower() in {".cmd", ".bat"}:
        if '"' in str(cli_path):
            raise ReleaseError("tauri CLI path cannot contain a quote")
        return '"' + str(cli_path) + '" ' + subprocess.list2cmdline(BUILD_ARGS)
    return raw


def command_for_display(command: list[str] | str) -> str:
    return command if isinstance(command, str) else subprocess.list2cmdline(command)


def print_dry_run(cli_path: Path, out_dir: Path, previous_version: str, repo_root: Path) -> bool:
    candidate_version = require_semver(workspace_version(repo_root))
    require_semver(previous_version)
    try:
        require_higher_semver(candidate_version, previous_version)
    except ReleaseError as exc:
        version_gate_passed = False
        version_gate_result = f"VERSION_GATE: would refuse: {exc}"
    else:
        version_gate_passed = True
        version_gate_result = "VERSION_GATE: pass"
    command = build_command(cli_path.resolve())
    print(f"CANDIDATE_VERSION: {candidate_version}")
    print(f"PREVIOUS_VERSION: {previous_version}")
    print(f"WORKING_DIRECTORY: {APP_DIR}")
    print(f"OUTPUT_ROOT: {out_dir}")
    print(f"COMMAND: {command_for_display(command)}")
    names = [*OFFLINE_ENV_NAMES, "TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH", SIGNING_PASSWORD_NAME]
    print(f"ENVIRONMENT_VARIABLE_NAMES: {', '.join(names)}")
    print(version_gate_result)
    return version_gate_passed


def git_text(repo_root: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(repo_root), *args],
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if completed.returncode != 0:
        raise ReleaseError(f"git command failed with exit code {completed.returncode}")
    return completed.stdout.strip()


def workspace_version(repo_root: Path) -> str:
    try:
        cargo = tomllib.loads((repo_root / "Cargo.toml").read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise ReleaseError(f"could not read workspace Cargo.toml: {exc}") from exc
    version = cargo.get("workspace", {}).get("package", {}).get("version")
    if version is None:
        version = cargo.get("package", {}).get("version")
    if not isinstance(version, str):
        raise ReleaseError("workspace Cargo.toml has no package version")
    return version


def signing_variables_present() -> bool:
    has_key = any(name in os.environ for name in SIGNING_KEY_NAMES)
    has_password = SIGNING_PASSWORD_NAME in os.environ
    return has_key and has_password


def require_clean_tree(repo_root: Path) -> None:
    status = git_text(repo_root, "status", "--porcelain", "--untracked-files=all")
    if status:
        raise ReleaseError("worktree is dirty; commit or otherwise resolve all tracked and untracked changes first")


def locate_built_installer(repo_root: Path, version: str, build_started: float) -> tuple[Path, Path]:
    roots: list[Path] = []
    configured_target = os.environ.get("CARGO_TARGET_DIR")
    if configured_target:
        target_path = Path(configured_target)
        app_dir = repo_root / "apps" / "desktop"
        roots.append(target_path if target_path.is_absolute() else app_dir / target_path)
    roots.extend([repo_root / "target", repo_root / "apps" / "desktop" / "src-tauri" / "target"])

    nsis_dirs: set[Path] = set()
    for target_root in roots:
        nsis_dirs.add(target_root / "release" / "bundle" / "nsis")
        try:
            nsis_dirs.update(target_root.glob("*/release/bundle/nsis"))
        except OSError:
            continue

    pairs: list[tuple[Path, Path]] = []
    for nsis_dir in sorted(nsis_dirs):
        if not nsis_dir.is_dir():
            continue
        for installer in nsis_dir.glob("*.exe"):
            signature = installer.with_name(installer.name + ".sig")
            try:
                if version in installer.name and installer.stat().st_mtime >= build_started and signature.is_file():
                    pairs.append((installer, signature))
            except OSError:
                continue
    if len(pairs) != 1:
        raise ReleaseError(f"expected one newly built NSIS installer and .sig for {version}; found {len(pairs)}")
    return pairs[0]


def perform_build(
    cli_path: Path,
    out_dir: Path,
    previous_version: str,
    expected_cargo_lock_sha256: str | None = None,
) -> Path:
    repo_root = REPO_ROOT.resolve()
    version = require_higher_semver(workspace_version(repo_root), previous_version)
    expected_hash = (
        require_sha256_hex(expected_cargo_lock_sha256, "--expect-cargo-lock-sha256")
        if expected_cargo_lock_sha256 is not None
        else None
    )

    cargo_lock = repo_root / "Cargo.lock"
    pnpm_lock = repo_root / "apps" / "desktop" / "pnpm-lock.yaml"
    if not cargo_lock.is_file() or not pnpm_lock.is_file():
        raise ReleaseError("Cargo.lock or apps/desktop/pnpm-lock.yaml is missing")
    cargo_lock_sha256 = sha256_file(cargo_lock)
    if expected_hash is not None and cargo_lock_sha256 != expected_hash:
        raise ReleaseError("Cargo.lock SHA-256 does not match --expect-cargo-lock-sha256")

    require_clean_tree(repo_root)
    if not signing_variables_present():
        raise ReleaseError(
            "missing required signing environment variable names: "
            "TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH, "
            "and TAURI_SIGNING_PRIVATE_KEY_PASSWORD"
        )
    if not cli_path.is_file():
        raise ReleaseError("the supplied tauri CLI path does not exist")

    output = out_dir.resolve() / version / "candidate"
    if output.exists():
        raise ReleaseError(f"candidate output already exists; refusing to overwrite: {output}")

    commit = git_text(repo_root, "rev-parse", "HEAD")
    if not commit:
        raise ReleaseError("could not determine the candidate commit")

    command = build_command(cli_path.resolve())
    environment = os.environ.copy()
    environment.update(
        {
            "CARGO_NET_OFFLINE": "true",
            "COREPACK_ENABLE_NETWORK": "0",
            "PNPM_CONFIG_OFFLINE": "true",
        }
    )
    build_started = datetime.now(timezone.utc).timestamp()
    try:
        if isinstance(command, str):
            completed = subprocess.run(command, cwd=APP_DIR, env=environment, check=False, shell=True)
        else:
            completed = subprocess.run(command, cwd=APP_DIR, env=environment, check=False)
    except OSError as exc:
        raise ReleaseError(f"could not launch tauri build: {exc}") from exc
    if completed.returncode != 0:
        raise ReleaseError(f"tauri build failed with exit code {completed.returncode}")
    if sha256_file(cargo_lock) != cargo_lock_sha256:
        raise ReleaseError("Cargo.lock changed during the build")

    installer, signature = locate_built_installer(repo_root, version, build_started)
    if installer.stat().st_size == 0 or signature.stat().st_size == 0:
        raise ReleaseError("built NSIS installer and updater signature must both be non-empty")
    output.mkdir(parents=True, exist_ok=False)
    installer_out = output / installer.name
    signature_out = output / signature.name
    shutil.copy2(installer, installer_out)
    shutil.copy2(signature, signature_out)

    metadata = {
        "version": version,
        "commit": commit,
        "cargo_lock_sha256": cargo_lock_sha256,
        "pnpm_lock_sha256": sha256_file(pnpm_lock),
        "installer_file": installer_out.name,
        "installer_size_bytes": installer_out.stat().st_size,
        "installer_sha256": sha256_file(installer_out),
        "signature_file": signature_out.name,
        "signature_size_bytes": signature_out.stat().st_size,
        "signature_sha256": sha256_file(signature_out),
        "build_time": datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
    }
    (output / "candidate.json").write_text(json.dumps(metadata, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    return output


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tauri-cli", required=True, type=Path, help="path to the installed Tauri CLI")
    parser.add_argument("--out", required=True, type=Path, help="candidate output root")
    parser.add_argument("--previous-version", required=True, help="currently offered SemVer; candidate must be higher")
    parser.add_argument(
        "--expect-cargo-lock-sha256",
        help="optional expected Cargo.lock SHA-256; mismatch refuses before the build",
    )
    parser.add_argument("--dry-run", action="store_true", help="print the exact command and environment variable names")
    args = parser.parse_args(argv)
    try:
        require_semver(args.previous_version)
        if args.expect_cargo_lock_sha256 is not None:
            require_sha256_hex(args.expect_cargo_lock_sha256, "--expect-cargo-lock-sha256")
        if args.dry_run:
            return 0 if print_dry_run(args.tauri_cli, args.out, args.previous_version, REPO_ROOT) else 2
        output = perform_build(
            args.tauri_cli,
            args.out,
            args.previous_version,
            args.expect_cargo_lock_sha256,
        )
        print(f"candidate: {output}")
        return 0
    except ReleaseError as exc:
        print(f"refusing: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
