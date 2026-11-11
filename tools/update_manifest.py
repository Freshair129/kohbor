#!/usr/bin/env python3
"""Refresh package-manifest.json from the Git-tracked working tree."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from typing import Any


MANIFEST_NAME = "package-manifest.json"
EXCLUDED_TRACKED_PATHS = {MANIFEST_NAME}
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Refresh package-manifest.json from Git-tracked working-tree files.",
        epilog=(
            "When tools/validate_spec.py rewrites acceptance/spec-validation.json, "
            "run the validator, then this script, then the validator again until "
            "two consecutive validator runs produce no change."
        ),
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="compare the manifest with the tracked tree")
    mode.add_argument("--apply", action="store_true", help="write refreshed manifest content")
    parser.add_argument(
        "--output",
        type=Path,
        help="with --apply, write to this path instead (use a temporary path outside the repository for a dry run)",
    )
    return parser.parse_args()


def is_within(path: Path, root: Path) -> bool:
    """Return whether path is beneath root, using the host's path rules."""
    path_text = os.path.normcase(os.path.abspath(path))
    root_text = os.path.normcase(os.path.abspath(root))
    try:
        return os.path.commonpath((path_text, root_text)) == root_text
    except ValueError:
        return False


def repo_root_from_cwd() -> Path:
    cwd = Path.cwd().resolve()
    try:
        completed = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=cwd,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
    except OSError as exc:
        raise RuntimeError(f"cannot run git: {exc}") from exc
    if completed.returncode != 0:
        detail = completed.stderr.strip() or "not inside a Git working tree"
        raise RuntimeError(f"cannot locate Git repository root: {detail}")

    root = Path(completed.stdout.strip()).resolve()
    if cwd != root:
        raise RuntimeError(f"run from the Git repository root ({root}), not {cwd}")

    script_root = Path(__file__).resolve().parents[1]
    if script_root != root:
        raise RuntimeError(f"this script belongs to {script_root}, not the current repository {root}")
    return root


def read_manifest(root: Path) -> tuple[dict[str, Any], bytes]:
    path = root / MANIFEST_NAME
    resolved = path.resolve(strict=True)
    if not is_within(resolved, root):
        raise RuntimeError(f"manifest resolves outside the repository: {path}")
    raw = resolved.read_bytes()
    try:
        manifest = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise RuntimeError(f"cannot parse {MANIFEST_NAME}: {exc}") from exc
    if not isinstance(manifest, dict) or not isinstance(manifest.get("files"), list):
        raise RuntimeError(f"{MANIFEST_NAME} must be a JSON object with a files array")
    seen: set[str] = set()
    for index, entry in enumerate(manifest["files"]):
        if not isinstance(entry, dict):
            raise RuntimeError(f"{MANIFEST_NAME} files[{index}] must be an object")
        name = entry.get("path")
        if not isinstance(name, str) or not name:
            raise RuntimeError(f"{MANIFEST_NAME} files[{index}].path must be a non-empty string")
        if (
            name.startswith("/")
            or "\\" in name
            or any(part in ("", ".", "..") for part in name.split("/"))
        ):
            raise RuntimeError(f"unsafe manifest path: {name!r}")
        if name in seen:
            raise RuntimeError(f"duplicate manifest path: {name}")
        seen.add(name)
        if not isinstance(entry.get("bytes"), int) or isinstance(entry.get("bytes"), bool):
            raise RuntimeError(f"{MANIFEST_NAME} files[{index}].bytes must be an integer")
        if entry["bytes"] < 0:
            raise RuntimeError(f"{MANIFEST_NAME} files[{index}].bytes must be non-negative")
        if not isinstance(entry.get("sha256"), str) or not SHA256_RE.fullmatch(entry["sha256"]):
            raise RuntimeError(f"{MANIFEST_NAME} files[{index}].sha256 must be lowercase SHA-256 hex")
    return manifest, raw


def tracked_paths(root: Path) -> list[str]:
    try:
        completed = subprocess.run(
            ["git", "ls-files", "--cached", "-z"],
            cwd=root,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except OSError as exc:
        raise RuntimeError(f"cannot enumerate Git-tracked files: {exc}") from exc
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(f"git ls-files failed: {detail or completed.returncode}")

    names: list[str] = []
    for raw_name in completed.stdout.split(b"\0"):
        if not raw_name:
            continue
        name = os.fsdecode(raw_name)
        if name in EXCLUDED_TRACKED_PATHS:
            continue
        if name.startswith("/") or "\\" in name or any(part in ("", ".", "..") for part in name.split("/")):
            raise RuntimeError(f"unsafe Git-tracked path: {name!r}")
        names.append(name)
    return sorted(names, key=os.fsencode)


def file_bytes(root: Path, name: str) -> bytes | None:
    candidate = root.joinpath(*name.split("/"))
    try:
        resolved = candidate.resolve(strict=False)
    except OSError as exc:
        raise RuntimeError(f"cannot resolve tracked path {name}: {exc}") from exc
    if not is_within(resolved, root):
        raise RuntimeError(f"refusing to follow a symlink outside the repository: {name} -> {resolved}")
    if not resolved.exists():
        return None
    try:
        resolved = candidate.resolve(strict=True)
    except FileNotFoundError:
        return None
    except OSError as exc:
        raise RuntimeError(f"cannot resolve tracked path {name}: {exc}") from exc
    if not is_within(resolved, root):
        raise RuntimeError(f"refusing to follow a symlink outside the repository: {name} -> {resolved}")
    if not resolved.is_file():
        raise RuntimeError(f"tracked path is not a regular file: {name}")
    try:
        return resolved.read_bytes()
    except OSError as exc:
        raise RuntimeError(f"cannot read tracked path {name}: {exc}") from exc


def build_records(root: Path, names: list[str]) -> tuple[list[dict[str, Any]], list[str]]:
    records: list[dict[str, Any]] = []
    disappeared: list[str] = []
    for name in names:
        content = file_bytes(root, name)
        if content is None:
            disappeared.append(name)
            continue
        records.append(
            {
                "path": name,
                "bytes": len(content),
                "sha256": hashlib.sha256(content).hexdigest(),
            }
        )
    return records, disappeared


def render_manifest(manifest: dict[str, Any], records: list[dict[str, Any]], original: bytes) -> bytes:
    rendered_manifest = dict(manifest)
    rendered_manifest["files"] = records
    text = json.dumps(rendered_manifest, ensure_ascii=False, indent=2)

    crlf_count = original.count(b"\r\n")
    lf_count = original.count(b"\n")
    if crlf_count and crlf_count != lf_count:
        raise RuntimeError(f"{MANIFEST_NAME} mixes newline styles; refusing to rewrite it")
    newline = "\r\n" if crlf_count else "\n"
    if newline != "\n":
        text = text.replace("\n", newline)
    if original.endswith((b"\n", b"\r")):
        text += newline
    return text.encode("utf-8")


def differences(
    old_records: list[dict[str, Any]],
    new_records: list[dict[str, Any]],
    disappeared: list[str],
) -> list[tuple[str, str]]:
    old = {entry["path"]: entry for entry in old_records}
    new = {entry["path"]: entry for entry in new_records}
    result: list[tuple[str, str]] = []
    for name in sorted(old.keys() - new.keys(), key=os.fsencode):
        result.append(("DISAPPEARED", name))
    for name in sorted(new.keys() - old.keys(), key=os.fsencode):
        result.append(("NEW", name))
    for name in sorted(old.keys() & new.keys(), key=os.fsencode):
        if old[name] != new[name]:
            result.append(("CHANGED", name))
    known = {name for _, name in result}
    for name in sorted(set(disappeared) - known, key=os.fsencode):
        result.append(("MISSING_FROM_WORKTREE", name))
    return result


def write_output(path: Path, content: bytes, root: Path, explicit_output: bool) -> None:
    target = path.absolute()
    resolved_parent = target.parent.resolve(strict=False)
    resolved_target = resolved_parent / target.name
    if explicit_output and is_within(resolved_target, root):
        raise RuntimeError("--output must be outside the repository; use it only for an external dry run")
    if explicit_output and resolved_target == (root / MANIFEST_NAME).resolve(strict=False):
        raise RuntimeError("refusing to use --output to target package-manifest.json")
    temporary: Path | None = None
    try:
        resolved_parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(
            mode="wb", prefix=target.name + ".", suffix=".tmp", dir=resolved_parent, delete=False
        ) as stream:
            temporary = Path(stream.name)
            stream.write(content)
        os.replace(temporary, target)
    except OSError as exc:
        if temporary is not None:
            try:
                temporary.unlink()
            except OSError:
                pass
        raise RuntimeError(f"cannot write manifest output {target}: {exc}") from exc


def main() -> int:
    args = parse_args()
    try:
        root = repo_root_from_cwd()
        manifest, original = read_manifest(root)
        names = tracked_paths(root)
        records, disappeared = build_records(root, names)
        generated = render_manifest(manifest, records, original)
        diffs = differences(manifest["files"], records, disappeared)

        if args.check:
            if generated != original:
                if diffs:
                    print("Manifest differs from the Git-tracked working tree:", file=sys.stderr)
                    for kind, name in diffs:
                        print(f"{kind}: {name}", file=sys.stderr)
                else:
                    print(f"Manifest formatting or metadata differs: {MANIFEST_NAME}", file=sys.stderr)
                return 1
            print(f"Manifest matches the Git-tracked working tree ({len(records)} files).")
            return 0

        output = args.output or (root / MANIFEST_NAME)
        if diffs:
            for kind, name in diffs:
                print(f"{kind}: {name}")
        if generated == original and output.resolve(strict=False) == (root / MANIFEST_NAME).resolve(strict=False):
            print(f"Manifest already matches the Git-tracked working tree ({len(records)} files).")
            return 0
        write_output(output, generated, root, explicit_output=args.output is not None)
        print(f"Wrote manifest for {len(records)} files to {output.absolute()}.")
        return 0
    except (OSError, RuntimeError) as exc:
        print(f"update_manifest: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
