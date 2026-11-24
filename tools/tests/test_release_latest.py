from __future__ import annotations

import sys
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path


RELEASE_TOOLS = Path(__file__).resolve().parents[1] / "release"
sys.path.insert(0, str(RELEASE_TOOLS))

from make_latest_json import build_manifest  # noqa: E402
from release_common import ReleaseError  # noqa: E402


class MakeLatestJsonTests(unittest.TestCase):
    def test_shape_url_and_signature_passthrough(self) -> None:
        note_text = "Release notes\r\nSecond line.\n"
        signature = "synthetic updater signature\r\n"
        fixed_time = datetime(2026, 11, 23, 12, 34, 56, tzinfo=timezone(timedelta(hours=7)))
        manifest = build_manifest(
            "0.3.0-dev.18",
            "Kohbor + setup.exe",
            note_text,
            signature,
            "v0.3.0-dev.18",
            fixed_time,
        )

        self.assertEqual(set(manifest), {"version", "notes", "pub_date", "platforms"})
        self.assertEqual(manifest["version"], "0.3.0-dev.18")
        self.assertEqual(manifest["notes"], note_text)
        self.assertEqual(manifest["pub_date"], "2026-11-23T05:34:56Z")
        self.assertEqual(
            manifest["platforms"]["windows-x86_64"]["url"],
            "https://github.com/Freshair129/kohbor-updates/releases/download/"
            "v0.3.0-dev.18/Kohbor%20%2B%20setup.exe",
        )
        self.assertEqual(manifest["platforms"]["windows-x86_64"]["signature"], signature)

    def test_rejects_non_semver_version(self) -> None:
        with self.assertRaisesRegex(ReleaseError, "valid SemVer"):
            build_manifest("0.3.0-01", "installer.exe", "notes", "sig", "v0.3.0-01")

    def test_rejects_tag_mismatch(self) -> None:
        with self.assertRaisesRegex(ReleaseError, "tag must exactly match"):
            build_manifest("0.3.0-dev.18", "installer.exe", "notes", "sig", "v0.3.0-dev.17")


if __name__ == "__main__":
    unittest.main()
