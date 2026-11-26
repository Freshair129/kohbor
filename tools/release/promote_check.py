"""Check that stable promotion reuses the candidate's exact signed bytes."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from release_common import (
    ReleaseError,
    candidate_metadata,
    sha256_bytes,
)


def check_promotion(candidate_dir: Path, stable_dir: Path) -> list[str]:
    candidate_dir = candidate_dir.resolve()
    stable_dir = stable_dir.resolve()
    metadata = candidate_metadata(candidate_dir)
    version = metadata["version"]
    reasons: list[str] = []
    for kind in ("installer", "signature"):
        name = metadata[f"{kind}_file"]
        candidate_file = candidate_dir / name
        stable_file = stable_dir / name
        if not stable_file.is_file():
            reasons.append(f"stable {kind} file is missing: {name}")
            continue
        if not candidate_file.is_file():
            reasons.append(f"candidate {kind} file is missing: {name}")

    candidate_manifest = candidate_dir / "latest.json"
    stable_manifest = stable_dir / "latest.json"
    if not candidate_manifest.is_file():
        reasons.append("candidate latest.json is missing")
    if not stable_manifest.is_file():
        reasons.append("stable latest.json is missing")
    required_paths = [
        candidate_dir / metadata["installer_file"],
        stable_dir / metadata["installer_file"],
        candidate_dir / metadata["signature_file"],
        stable_dir / metadata["signature_file"],
        candidate_manifest,
        stable_manifest,
    ]
    if any(not path.is_file() for path in required_paths):
        return reasons
    try:
        manifest_bytes = (required_paths[4].read_bytes(), required_paths[5].read_bytes())
        reasons = compare_promotion_bytes(
            version,
            required_paths[0].read_bytes(),
            required_paths[1].read_bytes(),
            required_paths[2].read_bytes(),
            required_paths[3].read_bytes(),
            *manifest_bytes,
        )
        if manifest_bytes[0] != manifest_bytes[1]:
            reasons.append("stable latest.json does not match candidate latest.json bytes")
        return reasons
    except OSError as exc:
        raise ReleaseError(f"could not read promotion artifacts: {exc}") from exc


def compare_promotion_bytes(
    version: str,
    candidate_installer: bytes,
    stable_installer: bytes,
    candidate_signature: bytes,
    stable_signature: bytes,
    candidate_manifest: bytes,
    stable_manifest: bytes,
) -> list[str]:
    reasons: list[str] = []
    if sha256_bytes(candidate_installer) != sha256_bytes(stable_installer):
        reasons.append("stable installer SHA-256 does not match candidate")
    if sha256_bytes(candidate_signature) != sha256_bytes(stable_signature):
        reasons.append("stable signature SHA-256 does not match candidate")
    for label, raw_manifest, raw_signature in (
        ("candidate", candidate_manifest, candidate_signature),
        ("stable", stable_manifest, stable_signature),
    ):
        try:
            manifest = json.loads(raw_manifest.decode("utf-8"))
        except (UnicodeError, json.JSONDecodeError):
            reasons.append(f"{label} latest.json is not valid UTF-8 JSON")
            continue
        platforms = manifest.get("platforms") if isinstance(manifest, dict) else None
        windows = platforms.get("windows-x86_64") if isinstance(platforms, dict) else None
        signature = windows.get("signature") if isinstance(windows, dict) else None
        if not isinstance(manifest, dict) or manifest.get("version") != version:
            reasons.append(f"{label} latest.json version does not match the candidate")
        try:
            signature_matches = isinstance(signature, str) and signature.encode("utf-8") == raw_signature
        except UnicodeError:
            signature_matches = False
        if not signature_matches:
            reasons.append(f"{label} latest.json signature does not match the .sig file")
    return reasons


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate_dir", type=Path)
    parser.add_argument("stable_dir", type=Path)
    args = parser.parse_args(argv)
    try:
        reasons = check_promotion(args.candidate_dir, args.stable_dir)
    except ReleaseError as exc:
        print(f"promotion refused: {exc}", file=sys.stderr)
        return 2
    if reasons:
        for reason in reasons:
            print(f"promotion refused: {reason}", file=sys.stderr)
        return 1
    print("promotion check passed: installer and .sig SHA-256 match; latest.json signature matches .sig")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
