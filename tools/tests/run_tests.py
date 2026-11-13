#!/usr/bin/env python3
"""Run the tests of the repository tools (standard library only): ``python tools/tests/run_tests.py``."""

from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path

if __name__ == "__main__":
    if "--skip-git" in sys.argv:
        os.environ["KOHBOR_TOOL_TESTS_SKIP_GIT"] = "1"
    here = Path(__file__).resolve().parent
    suite = unittest.defaultTestLoader.discover(str(here), pattern="test_*.py")
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    print(f"tool tests: {result.testsRun} run, {len(result.failures)} failed, {len(result.errors)} errors")
    sys.exit(0 if result.wasSuccessful() else 1)
