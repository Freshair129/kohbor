"""Create the static Tauri v2 updater manifest for a candidate."""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote

from release_common import ReleaseError, candidate_metadata, require_semver


UPDATES_REPO = "Freshair129/kohbor-updates"


def create_latest_json(
    candidate_dir: Path,
    notes_file: Path,
    tag: str,
    now: datetime | None = None,
) -> Path:
    candidate_dir = candidate_dir.resolve()
    metadata = candidate_metadata(candidate_dir)
    version = metadata["version"]
    require_semver(version)
    if tag != f"v{version}":
        raise ReleaseError(f"tag must exactly match candidate version v{version}")

    installer_name = metadata["installer_file"]
    signature_path = candidate_dir / metadata["signature_file"]
    try:
        notes = notes_file.read_bytes().decode("utf-8")
        signature = signature_path.read_bytes().decode("utf-8")
    except (OSError, UnicodeError) as exc:
        raise ReleaseError(f"could not read notes or signature as UTF-8: {exc}") from exc
    if not signature:
        raise ReleaseError("candidate .sig is empty")

    manifest = build_manifest(version, installer_name, notes, signature, tag, now)
    manifest_path = candidate_dir / "latest.json"
    if manifest_path.exists():
        raise ReleaseError("latest.json already exists; refusing to overwrite a candidate manifest")
    try:
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    except OSError as exc:
        raise ReleaseError(f"could not write latest.json: {exc}") from exc
    return manifest_path


def build_manifest(
    version: str,
    installer_name: str,
    notes: str,
    signature: str,
    tag: str,
    now: datetime | None = None,
) -> dict[str, object]:
    require_semver(version)
    if tag != f"v{version}":
        raise ReleaseError(f"tag must exactly match candidate version v{version}")
    published_at = now or datetime.now(timezone.utc)
    if published_at.tzinfo is None:
        raise ReleaseError("pub_date clock must include a timezone")
    pub_date = published_at.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
    encoded_tag = quote(tag, safe="")
    encoded_name = quote(installer_name, safe="")
    return {
        "version": version,
        "notes": notes,
        "pub_date": pub_date,
        "platforms": {
            "windows-x86_64": {
                "signature": signature,
                "url": f"https://github.com/{UPDATES_REPO}/releases/download/{encoded_tag}/{encoded_name}",
            }
        },
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate_dir", type=Path)
    parser.add_argument("--notes-file", required=True, type=Path)
    parser.add_argument("--tag", required=True)
    args = parser.parse_args(argv)
    try:
        path = create_latest_json(args.candidate_dir, args.notes_file, args.tag)
        print(f"latest.json created: {path}")
        return 0
    except ReleaseError as exc:
        print(f"refusing: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
