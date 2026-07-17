"""Unit tests for ``anvil/skills/datasheet/lib/pinmap_check.py``.

The canary calibration set (issue #418): two pins double-assigned (power AND a
MIPI differential pair) while two others sat unassigned. These tests pin that
exact failure shape plus the clean case and the graceful-degradation paths.

Distinct filename per the #58 packaging convention; ``__init__.py`` chain in
this tests/ directory.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parents[4]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.skills.datasheet.lib.pinmap_check import (  # noqa: E402
    PinmapResult,
    check_pinmap,
)


def _block(rows: str, attrs: str = "package=QFN8 pins=8") -> str:
    return (
        "\\section{Pin Configuration and Functions}\n"
        "\\toprule\n"
        f"% anvil-pinmap-begin {attrs}\n"
        f"{rows}\n"
        "% anvil-pinmap-end\n"
        "\\bottomrule\n"
    )


_CLEAN_ROWS = "\n".join(
    f"{i} & SIG{i} & I/O & Signal {i} \\\\" for i in range(1, 9)
)


class TestCleanPinmap(unittest.TestCase):
    def test_clean_passes(self):
        result = check_pinmap(_block(_CLEAN_ROWS))
        self.assertTrue(result.found)
        self.assertTrue(result.passed)
        self.assertEqual(result.package, "QFN8")
        self.assertEqual(result.declared_pins, 8)
        self.assertEqual(len(result.rows), 8)

    def test_latex_rules_and_comments_ignored(self):
        rows = (
            "1 & VDD & P & Core supply \\\\\n"
            "\\midrule\n"
            "% a stray comment\n"
            "2 & VSS & G & Ground \\\\"
        )
        result = check_pinmap(_block(rows, attrs="package=X pins=2"))
        self.assertTrue(result.passed)
        self.assertEqual(len(result.rows), 2)


class TestDoubleAssignment(unittest.TestCase):
    def test_double_assigned_pin_fails(self):
        # The canary shape: a pin carrying both power and a MIPI lane.
        rows = (
            "1 & VDD_CORE & P & Core supply \\\\\n"
            "2 & VSS & G & Ground \\\\\n"
            "1 & MIPI_D0P & I/O & MIPI lane 0 positive \\\\"
        )
        result = check_pinmap(_block(rows, attrs="pins=3"))
        self.assertFalse(result.passed)
        kinds = {v.kind for v in result.violations}
        self.assertIn("double-assigned", kinds)
        double = next(
            v for v in result.violations if v.kind == "double-assigned"
        )
        self.assertIn("VDD_CORE", double.message)
        self.assertIn("MIPI_D0P", double.message)

    def test_two_doubles_and_two_unassigned(self):
        # Exactly the canary: two double-assigned, two unassigned.
        rows = (
            "1 & VDD & P & Supply \\\\\n"
            "1 & MIPI_D0P & I/O & Lane 0 P \\\\\n"
            "2 & VDDIO & P & IO supply \\\\\n"
            "2 & MIPI_D0N & I/O & Lane 0 N \\\\\n"
            "3 & GPIO0 & I/O & GPIO \\\\\n"
            "4 & GPIO1 & I/O & GPIO \\\\"
        )
        result = check_pinmap(_block(rows, attrs="pins=6"))
        self.assertFalse(result.passed)
        doubles = [v for v in result.violations if v.kind == "double-assigned"]
        unassigned = [v for v in result.violations if v.kind == "unassigned"]
        self.assertEqual(len(doubles), 2)
        self.assertEqual(len(unassigned), 1)
        self.assertIn("5, 6", unassigned[0].message)


class TestUnassignedPins(unittest.TestCase):
    def test_unassigned_pin_fails_when_count_declared(self):
        rows = "1 & VDD & P & Supply \\\\\n3 & VSS & G & Ground \\\\"
        result = check_pinmap(_block(rows, attrs="pins=3"))
        self.assertFalse(result.passed)
        unassigned = [v for v in result.violations if v.kind == "unassigned"]
        self.assertEqual(len(unassigned), 1)
        self.assertIn("2", unassigned[0].message)

    def test_no_declared_count_skips_coverage(self):
        # Without pins=N, only the exactly-once rule applies.
        rows = "1 & VDD & P & Supply \\\\\n3 & VSS & G & Ground \\\\"
        result = check_pinmap(_block(rows, attrs="package=QFN48"))
        self.assertTrue(result.passed)
        self.assertIsNone(result.declared_pins)

    def test_out_of_range_designator(self):
        rows = "1 & VDD & P & Supply \\\\\n2 & VSS & G & Ground \\\\\n9 & X & I & Stray \\\\"
        result = check_pinmap(_block(rows, attrs="pins=2"))
        self.assertFalse(result.passed)
        kinds = {v.kind for v in result.violations}
        self.assertIn("count-mismatch", kinds)


class TestBgaDesignators(unittest.TestCase):
    def test_non_numeric_designators_check_duplicates(self):
        rows = (
            "A1 & VDD & P & Supply \\\\\n"
            "A2 & VSS & G & Ground \\\\\n"
            "A1 & SCL & I/O & I2C clock \\\\"
        )
        result = check_pinmap(_block(rows, attrs="package=BGA"))
        self.assertFalse(result.passed)
        self.assertEqual(result.violations[0].kind, "double-assigned")

    def test_non_numeric_count_mismatch(self):
        rows = "A1 & VDD & P & Supply \\\\\nA2 & VSS & G & Ground \\\\"
        result = check_pinmap(_block(rows, attrs="package=BGA pins=3"))
        self.assertFalse(result.passed)
        self.assertEqual(result.violations[0].kind, "count-mismatch")


class TestGracefulDegradation(unittest.TestCase):
    def test_no_markers_inactive_and_passing(self):
        result = check_pinmap("\\section{Pinout}\n1 & VDD & P & x \\\\\n")
        self.assertIsInstance(result, PinmapResult)
        self.assertFalse(result.found)
        self.assertTrue(result.passed)

    def test_missing_end_marker_is_a_violation(self):
        text = "% anvil-pinmap-begin pins=2\n1 & VDD & P & x \\\\\n"
        result = check_pinmap(text)
        self.assertTrue(result.found)
        self.assertFalse(result.passed)

    def test_to_dict_shape(self):
        result = check_pinmap(_block(_CLEAN_ROWS))
        payload = result.to_dict()
        for key in (
            "found",
            "package",
            "declared_pins",
            "assigned_pins",
            "violations",
            "passed",
        ):
            self.assertIn(key, payload)
        self.assertTrue(payload["passed"])
        self.assertEqual(payload["assigned_pins"], 8)


if __name__ == "__main__":
    unittest.main()
