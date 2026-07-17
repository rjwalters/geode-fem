"""Unit tests for ``anvil/skills/datasheet/lib/buswidth_check.py``.

The canary calibration case (issue #418): a 5-bit field claiming a 0–79 index
range — capacity 2^5 = 32, so the bus cannot represent its own stated value
set. That exact case MUST fail.

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

from anvil.skills.datasheet.lib.buswidth_check import (  # noqa: E402
    bus_capacity,
    check_bus,
    check_buswidths,
)


class TestPurePredicates(unittest.TestCase):
    def test_capacity(self):
        self.assertEqual(bus_capacity(0), 1)
        self.assertEqual(bus_capacity(5), 32)
        self.assertEqual(bus_capacity(7), 128)

    def test_negative_width_raises(self):
        with self.assertRaises(ValueError):
            bus_capacity(-1)

    def test_canary_case_five_bit_claiming_0_to_79_fails(self):
        # THE canary case: 5-bit field, claimed index range 0–79.
        self.assertFalse(check_bus(5, max_value=79))

    def test_seven_bit_covers_0_to_79(self):
        self.assertTrue(check_bus(7, max_value=79))

    def test_max_at_capacity_boundary(self):
        self.assertTrue(check_bus(5, max_value=31))
        self.assertFalse(check_bus(5, max_value=32))

    def test_value_count_semantics(self):
        # values= is a cardinality: a 5-bit field holds exactly 32 values.
        self.assertTrue(check_bus(5, value_count=32))
        self.assertFalse(check_bus(5, value_count=33))

    def test_both_claims_must_hold(self):
        self.assertFalse(check_bus(5, max_value=10, value_count=40))


class TestMarkerScanning(unittest.TestCase):
    def test_canary_marker_fails(self):
        tex = "Some prose.\n% anvil-bus: name=roi_index width=5 max=79\n"
        result = check_buswidths(tex)
        self.assertTrue(result.found)
        self.assertFalse(result.passed)
        self.assertEqual(len(result.violations), 1)
        v = result.violations[0]
        self.assertEqual(v.name, "roi_index")
        self.assertIn("capacity 32", v.message)

    def test_clean_markers_pass(self):
        tex = (
            "% anvil-bus: name=roi_index width=7 max=79\n"
            "% anvil-bus: name=ch_sel width=3 range=0-7\n"
            "% anvil-bus: name=layer_id width=6 values=64\n"
        )
        result = check_buswidths(tex)
        self.assertTrue(result.found)
        self.assertTrue(result.passed)
        self.assertEqual(len(result.declarations), 3)

    def test_range_claim_counts_values(self):
        # range=1-32 is 32 distinct values: fits in 5 bits.
        ok = check_buswidths("% anvil-bus: name=a width=5 range=1-32\n")
        self.assertTrue(ok.passed)
        # range=0-32 is 33 distinct values: does not.
        bad = check_buswidths("% anvil-bus: name=a width=5 range=0-32\n")
        self.assertFalse(bad.passed)

    def test_no_markers_inactive_and_passing(self):
        result = check_buswidths("A datasheet with no bus declarations.\n")
        self.assertFalse(result.found)
        self.assertTrue(result.passed)
        self.assertEqual(result.declarations, [])

    def test_malformed_width_is_a_violation(self):
        result = check_buswidths("% anvil-bus: name=x width=five max=3\n")
        self.assertTrue(result.found)
        self.assertFalse(result.passed)
        self.assertIn("width", result.violations[0].message)

    def test_missing_claim_is_a_violation(self):
        result = check_buswidths("% anvil-bus: name=x width=4\n")
        self.assertFalse(result.passed)
        self.assertIn("no claim", result.violations[0].message)

    def test_inverted_range_is_a_violation(self):
        result = check_buswidths("% anvil-bus: name=x width=4 range=7-0\n")
        self.assertFalse(result.passed)

    def test_to_dict_shape(self):
        result = check_buswidths(
            "% anvil-bus: name=roi_index width=5 max=79\n"
        )
        payload = result.to_dict()
        for key in ("found", "declarations", "violations", "passed"):
            self.assertIn(key, payload)
        self.assertFalse(payload["passed"])
        self.assertEqual(payload["declarations"][0]["capacity"], 32)


if __name__ == "__main__":
    unittest.main()
