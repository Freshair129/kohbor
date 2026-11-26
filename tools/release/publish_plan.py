"""Print draft GitHub release commands; this helper never runs gh."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

from release_common import ReleaseError, candidate_metadata, verify_manifest_signature


PUBLIC_REPO = "Freshair129/kohbor-updates"
PRIVATE_REPO = "Freshair129/kohbor-releases"


def release_commands(candidate_dir: Path) -> tuple[str, str]:
    candidate_dir = candidate_dir.resolve()
    metadata = candidate_metadata(candidate_dir)
    version = metadata["version"]
    tag = f"v{version}"
    assets = [
        candidate_dir / metadata["installer_file"],
        candidate_dir / metadata["signature_file"],
        candidate_dir / "latest.json",
    ]
    if not all(asset.is_file() for asset in assets):
        raise ReleaseError("candidate must include installer, .sig and latest.json before drafting releases")
    verify_manifest_signature(assets[2], assets[1], version)
    title = f"Kohbor {tag}"
    public_args = ["gh", "release", "create", tag, *(str(p) for p in assets), "--repo", PUBLIC_REPO, "--draft", "--title", title]
    # Keep this installer draft on every release; invite-gated downloads use kohbor-releases as the latest normal release.
    private_args = ["gh", "release", "create", tag, str(assets[0]), "--repo", PRIVATE_REPO, "--draft", "--title", title]
    return subprocess.list2cmdline(public_args), subprocess.list2cmdline(private_args)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate_dir", type=Path)
    args = parser.parse_args(argv)
    try:
        public_command, private_command = release_commands(args.candidate_dir)
    except ReleaseError as exc:
        print(f"refusing: {exc}", file=sys.stderr)
        return 2
    print("Public updates repo draft (installer, .sig, latest.json):")
    print(public_command)
    print("Private invite-gated download draft (installer only):")
    print(private_command)
    print("These are plans only; no GitHub command was run.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
