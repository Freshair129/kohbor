"""Verify that the installed DEM parser is exactly the pinned gem-dota build.

The exporter is a Lab/dev tool (DEC-COACH-REVIEW). It runs only against the
gem-dota copy that already exists on the owner's machine; nothing is
downloaded. This check compares the installed distribution with PIN.json:
the version, the SHA-256 of the wheel's RECORD file, and every file hash
listed in RECORD. Any difference refuses the export.

Standard library only.
"""

from __future__ import annotations

import base64
import hashlib
import json
from importlib import metadata
from pathlib import Path

PIN_FILE = Path(__file__).with_name("PIN.json")


def load_pin() -> dict:
    return json.loads(PIN_FILE.read_text(encoding="utf-8"))


def _b64_sha256(data: bytes) -> str:
    return base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode("ascii")


def verify_installed(pin: dict | None = None) -> list[str]:
    """Return a list of problems; an empty list means the pin holds."""
    pin = pin or load_pin()
    problems: list[str] = []
    try:
        dist = metadata.distribution(pin["distribution"])
    except metadata.PackageNotFoundError:
        return [f"{pin['distribution']} is not installed in this interpreter"]
    if dist.version != pin["version"]:
        problems.append(f"version {dist.version} != pinned {pin['version']}")
    record_text = dist.read_text("RECORD")
    if record_text is None:
        return problems + ["RECORD file missing"]
    record_path = Path(str(dist.locate_file(f"{dist.metadata['Name'].replace('-', '_')}-{dist.version}.dist-info/RECORD")))
    try:
        record_bytes = record_path.read_bytes()
    except OSError:
        record_bytes = record_text.encode("utf-8")
    actual = hashlib.sha256(record_bytes).hexdigest()
    if actual != pin["record_sha256"]:
        problems.append(f"RECORD sha256 {actual} != pinned {pin['record_sha256']}")
    checked = 0
    for line in record_text.splitlines():
        parts = line.rsplit(",", 2)
        if len(parts) != 3 or not parts[1].startswith("sha256="):
            continue
        target = Path(str(dist.locate_file(parts[0])))
        try:
            digest = _b64_sha256(target.read_bytes())
        except OSError:
            problems.append(f"missing file {parts[0]}")
            continue
        if digest != parts[1][len("sha256="):]:
            problems.append(f"modified file {parts[0]}")
        checked += 1
    if checked == 0:
        problems.append("RECORD lists no hashed files")
    return problems


if __name__ == "__main__":
    issues = verify_installed()
    if issues:
        for issue in issues:
            print("PIN FAIL:", issue)
        raise SystemExit(1)
    p = load_pin()
    print(f"PIN OK: {p['distribution']} {p['version']} RECORD {p['record_sha256']}")
