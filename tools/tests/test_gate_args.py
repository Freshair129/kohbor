"""The everyday gate must accept what it always accepted; only --sprint-exit adds enforcement (owner decision 5, 2026-11-13)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import gate  # noqa: E402


class SprintExitTests(unittest.TestCase):
    def test_everyday_gate_adds_no_enforcement(self) -> None:
        self.assertEqual(gate.sprint_exit_args(False), {"domains": [], "agents": [], "scope": []})

    def test_sprint_exit_enforces_exactly_the_three_checks_of_the_decision(self) -> None:
        extra = gate.sprint_exit_args(True)
        self.assertEqual(extra["domains"], ["--fail-on-stale"])
        self.assertEqual(extra["agents"], ["--enforce"])
        self.assertEqual(extra["scope"], ["--enforce", "--include-done"])

    def test_the_flag_is_off_by_default(self) -> None:
        with mock.patch.object(sys, "argv", ["gate.py", "--quick"]):
            self.assertFalse(gate.parse_args().sprint_exit)
        with mock.patch.object(sys, "argv", ["gate.py", "--quick", "--sprint-exit"]):
            self.assertTrue(gate.parse_args().sprint_exit)

    def test_run_records_stay_report_only_in_both_modes(self) -> None:
        source = (Path(gate.__file__)).read_text(encoding="utf-8")
        self.assertIn('"run_record.py"), "check"]', source)
        self.assertNotIn('"run_record.py"), "check", "--enforce"', source)


if __name__ == "__main__":
    unittest.main()
