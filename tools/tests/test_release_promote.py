from __future__ import annotations

import sys
import unittest
from pathlib import Path


RELEASE_TOOLS = Path(__file__).resolve().parents[1] / "release"
sys.path.insert(0, str(RELEASE_TOOLS))

from promote_check import compare_promotion_bytes  # noqa: E402


class PromoteCheckTests(unittest.TestCase):
    def compare(self, installer_stable: bytes, signature_stable: bytes) -> list[str]:
        version = "0.3.0-dev.18"
        installer = b"approved installer bytes"
        signature = b"approved signature\n"
        manifest = (
            '{"version":"0.3.0-dev.18","platforms":{"windows-x86_64":'
            '{"signature":"approved signature\\n"}}}'
        ).encode("utf-8")
        stable_manifest = manifest.replace(
            b"approved signature\\n", signature_stable.decode("utf-8").encode("unicode_escape")
        )
        return compare_promotion_bytes(
            version,
            installer,
            installer_stable,
            signature,
            signature_stable,
            manifest,
            stable_manifest,
        )

    def test_passes_for_identical_assets_and_matching_manifest(self) -> None:
        self.assertEqual(self.compare(b"approved installer bytes", b"approved signature\n"), [])

    def test_fails_when_installer_changes_by_one_byte(self) -> None:
        reasons = self.compare(b"approved installer bytes!", b"approved signature\n")
        self.assertTrue(any("installer SHA-256" in reason for reason in reasons), reasons)

    def test_fails_when_signature_changes_by_one_byte(self) -> None:
        reasons = self.compare(b"approved installer bytes", b"approved signature\n!")
        self.assertTrue(any("signature SHA-256" in reason for reason in reasons), reasons)


if __name__ == "__main__":
    unittest.main()
