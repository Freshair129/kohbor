from __future__ import annotations

import sys
import unittest
from pathlib import Path


RELEASE_TOOLS = Path(__file__).resolve().parents[1] / "release"
sys.path.insert(0, str(RELEASE_TOOLS))

from release_common import ReleaseError, compare_semver, require_higher_semver  # noqa: E402


class SemverComparisonTests(unittest.TestCase):
    def test_numeric_prerelease_identifiers_compare_numerically(self) -> None:
        self.assertGreater(compare_semver("0.4.0-alpha.2", "0.4.0-alpha.1"), 0)
        self.assertGreater(compare_semver("0.4.0-alpha.10", "0.4.0-alpha.2"), 0)

    def test_release_ranks_above_prerelease(self) -> None:
        self.assertGreater(compare_semver("0.4.0", "0.4.0-alpha.9"), 0)

    def test_prerelease_numeric_identifiers_rank_below_alphanumeric(self) -> None:
        self.assertLess(compare_semver("1.0.0-alpha.9", "1.0.0-alpha.beta"), 0)

    def test_prerelease_alphanumeric_identifiers_use_ascii_order(self) -> None:
        self.assertLess(compare_semver("1.0.0-alpha.beta", "1.0.0-alpha.rc"), 0)

    def test_build_metadata_does_not_change_precedence(self) -> None:
        self.assertEqual(compare_semver("1.0.0+build.1", "1.0.0+build.2"), 0)

    def test_equal_version_is_not_strictly_higher(self) -> None:
        self.assertEqual(compare_semver("0.4.0-alpha.1", "0.4.0-alpha.1"), 0)
        with self.assertRaisesRegex(ReleaseError, "strictly higher"):
            require_higher_semver("0.4.0-alpha.1", "0.4.0-alpha.1")

    def test_malformed_versions_are_refused(self) -> None:
        for version in ("0.4", "0.4.0-01", "01.4.0", "1.0.0+", "1.0.0-alpha..1"):
            with self.subTest(version=version), self.assertRaisesRegex(ReleaseError, "valid SemVer"):
                compare_semver(version, "0.4.0-alpha.1")
        with self.assertRaisesRegex(ReleaseError, "valid SemVer"):
            compare_semver("0.4.0-alpha.2", "0.4.0-alpha.01")


if __name__ == "__main__":
    unittest.main()
