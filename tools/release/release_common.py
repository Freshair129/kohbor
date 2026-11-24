"""Shared validation for the local Kohbor release helpers."""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path, PureWindowsPath
from typing import Any


SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)"
    r"(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)


class ReleaseError(Exception):
    """A release input or artifact did not meet its local contract."""


def require_semver(version: str) -> str:
    if not SEMVER_RE.fullmatch(version):
        raise ReleaseError(f"version is not valid SemVer: {version!r}")
    return version


def _compare_numeric_identifiers(left: str, right: str) -> int:
    # Comparing lengths and then ASCII digits avoids integer-size limits.
    if len(left) != len(right):
        return -1 if len(left) < len(right) else 1
    if left == right:
        return 0
    return -1 if left < right else 1


def compare_semver(left: str, right: str) -> int:
    """Compare SemVer 2.0 precedence, ignoring build metadata."""
    require_semver(left)
    require_semver(right)

    def split(value: str) -> tuple[list[str], list[str] | None]:
        without_build = value.split("+", 1)[0]
        core, separator, prerelease = without_build.partition("-")
        return core.split("."), prerelease.split(".") if separator else None

    left_core, left_prerelease = split(left)
    right_core, right_prerelease = split(right)
    for left_part, right_part in zip(left_core, right_core):
        result = _compare_numeric_identifiers(left_part, right_part)
        if result:
            return result

    if left_prerelease is None or right_prerelease is None:
        if left_prerelease is None and right_prerelease is None:
            return 0
        return 1 if left_prerelease is None else -1

    for left_part, right_part in zip(left_prerelease, right_prerelease):
        left_numeric = left_part.isascii() and left_part.isdigit()
        right_numeric = right_part.isascii() and right_part.isdigit()
        if left_numeric and right_numeric:
            result = _compare_numeric_identifiers(left_part, right_part)
        elif left_numeric != right_numeric:
            result = -1 if left_numeric else 1
        elif left_part == right_part:
            result = 0
        else:
            # SemVer prerelease identifiers are ASCII, so Python's ordering
            # matches the required ASCII lexicographic ordering here.
            result = -1 if left_part < right_part else 1
        if result:
            return result

    if len(left_prerelease) == len(right_prerelease):
        return 0
    return -1 if len(left_prerelease) < len(right_prerelease) else 1


def require_higher_semver(version: str, previous_version: str) -> str:
    """Require ``version`` to have strictly higher precedence than the previous release."""
    require_semver(version)
    require_semver(previous_version)
    if compare_semver(version, previous_version) <= 0:
        raise ReleaseError("candidate version must be strictly higher than --previous-version")
    return version


def require_sha256_hex(value: str, label: str) -> str:
    if not re.fullmatch(r"[0-9A-Fa-f]{64}", value):
        raise ReleaseError(f"{label} must be exactly 64 hexadecimal characters")
    return value.lower()


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ReleaseError(f"required file is missing: {path}") from exc
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ReleaseError(f"could not read JSON file {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise ReleaseError(f"JSON root must be an object: {path}")
    return value


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError as exc:
        raise ReleaseError(f"could not read artifact {path}: {exc}") from exc
    return digest.hexdigest()


def safe_filename(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise ReleaseError(f"candidate metadata has no {label} file name")
    if Path(value).name != value or PureWindowsPath(value).name != value:
        raise ReleaseError(f"candidate {label} file name must be a plain file name")
    return value


def candidate_metadata(candidate_dir: Path) -> dict[str, Any]:
    metadata = load_json(candidate_dir / "candidate.json")
    version = metadata.get("version")
    if not isinstance(version, str):
        raise ReleaseError("candidate.json has no version")
    require_semver(version)
    for kind in ("installer", "signature"):
        filename = safe_filename(metadata.get(f"{kind}_file"), kind)
        path = candidate_dir / filename
        if not path.is_file():
            raise ReleaseError(f"candidate {kind} file is missing: {filename}")
        expected_size = metadata.get(f"{kind}_size_bytes")
        expected_hash = metadata.get(f"{kind}_sha256")
        if not isinstance(expected_size, int) or isinstance(expected_size, bool):
            raise ReleaseError(f"candidate.json has no valid {kind} size")
        try:
            actual_size = path.stat().st_size
        except OSError as exc:
            raise ReleaseError(f"could not inspect candidate {kind} size: {exc}") from exc
        if actual_size != expected_size:
            raise ReleaseError(f"candidate {kind} size does not match candidate.json")
        if not isinstance(expected_hash, str) or sha256_file(path) != expected_hash:
            raise ReleaseError(f"candidate {kind} SHA-256 does not match candidate.json")
    return metadata


def manifest_signature(manifest_path: Path, expected_version: str) -> str:
    manifest = load_json(manifest_path)
    if manifest.get("version") != expected_version:
        raise ReleaseError(f"{manifest_path.name} version does not match the candidate")
    platforms = manifest.get("platforms")
    windows = platforms.get("windows-x86_64") if isinstance(platforms, dict) else None
    signature = windows.get("signature") if isinstance(windows, dict) else None
    if not isinstance(signature, str):
        raise ReleaseError(f"{manifest_path.name} has no Windows updater signature")
    return signature


def verify_manifest_signature(manifest_path: Path, signature_path: Path, version: str) -> None:
    signature = manifest_signature(manifest_path, version)
    try:
        signature_bytes = signature_path.read_bytes()
        manifest_bytes = signature.encode("utf-8")
    except (OSError, UnicodeError) as exc:
        raise ReleaseError(f"could not compare manifest signature with .sig: {exc}") from exc
    if manifest_bytes != signature_bytes:
        raise ReleaseError(f"{manifest_path.name} signature does not match the .sig file")
