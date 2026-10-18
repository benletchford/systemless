#!/usr/bin/env python3
"""Check native selection without starting a window manager or guest."""
import json
from pathlib import Path
import subprocess
import sys
import unittest
SCRIPT = str(Path(__file__).with_name("verify-native-window.py"))


class Cases(unittest.TestCase):
    def run_cases(self, *args, valid=True):
        result = subprocess.run([sys.executable, SCRIPT, "--dry-run", *args], capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, valid, result.stderr)
        return json.loads(result.stdout) if valid else None

    def test_normal_matrix_is_complete(self):
        self.assertEqual(len(self.run_cases()), 34)

    def test_soak_defaults_to_six_cases(self):
        self.assertEqual(len(self.run_cases("--soak-only", "--soak-seconds", "30")), 6)

    def test_long_session_selects_exact_owner(self):
        for runtime in ["inline", "thread"]:
            for presenter in ["platform", "wgpu", "software"]:
                result = self.run_cases("--soak-only", "--soak-seconds", "7200", "--runtime", runtime, "--presenter", presenter)
                self.assertEqual(result, [[runtime, presenter == "wgpu", "", "software" if presenter == "software" else None, None]])

    def test_both_fallbacks_and_software_recreation_are_exercised(self):
        recovery = self.run_cases("--recovery-only")
        self.assertEqual({case[4] for case in recovery}, {None, "software"})
        self.assertEqual(sum(case[2] == "software-recreate" for case in self.run_cases()), 2)

    def test_unfiltered_or_partial_long_matrix_is_rejected(self):
        self.run_cases("--soak-only", "--soak-seconds", "7200", valid=False)
        self.run_cases("--soak-only", "--soak-seconds", "7200", "--runtime", "inline", valid=False)

    def test_selectors_cannot_weaken_normal_matrix(self):
        self.run_cases("--runtime", "inline", valid=False)
        self.run_cases("--recovery-only", "--presenter", "wgpu", valid=False)

    def test_main_qualifier_rejects_unselected_long_session(self):
        qualifier = str(Path(__file__).with_name("qualify-renderers.py"))
        result = subprocess.run([sys.executable, qualifier, "--output", "unused-long-case-output",
                                 "--soak-seconds", "7200"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("require --soak-runtime", result.stderr)

    def test_limits_and_recovery_constraints(self):
        self.run_cases("--soak-only", "--soak-seconds", "21601", "--runtime", "inline", "--presenter", "wgpu", valid=False)
        self.run_cases("--soak-seconds", "-1", valid=False)
        self.run_cases("--recovery-only", "--soak-seconds", "1", valid=False)
        self.assertEqual(len(self.run_cases("--recovery-only")), 20)


if __name__ == "__main__":
    unittest.main()
