"""Unit tests for ``anvil/skills/spec/lib/constant_consistency.py``.

The canary calibration set (epic #697, botho): a block-time floor stated 3\\,s
in one section and 5\\,s in another, plus a ring-size / byte-count figure that
disagreed with itself across sections. These tests pin those exact failure
shapes plus the clean case, graceful degradation, malformed markers, the
unit-mismatch distinction, and the ``\\newcommand`` duplicate-conflict path.

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

from anvil.skills.spec.lib.constant_consistency import (  # noqa: E402
    ConstantConsistencyResult,
    check_constant_consistency,
    check_constant_consistency_multi,
    normalize_value,
)


def _section(title: str, marker: str) -> str:
    return f"\\section{{{title}}}\nBody text.\n{marker}\nMore prose.\n"


class TestBothoBlockTimeFloor(unittest.TestCase):
    """The motivating botho mismatch: block-time floor 3\\,s vs 5\\,s."""

    def test_value_mismatch_across_sections(self):
        src = (
            _section(
                "Consensus timing",
                "% anvil-const: name=block_time_floor value=3 unit=s",
            )
            + _section(
                "Block production",
                "% anvil-const: name=block_time_floor value=5 unit=s",
            )
        )
        result = check_constant_consistency(src)
        self.assertTrue(result.found)
        self.assertFalse(result.passed)
        mismatches = [
            v for v in result.violations if v.kind == "value-mismatch"
        ]
        self.assertEqual(len(mismatches), 1)
        msg = mismatches[0].message
        self.assertIn("block_time_floor", msg)
        self.assertIn("3", msg)
        self.assertIn("5", msg)
        # Both location anchors present.
        self.assertIn("Consensus timing", msg)
        self.assertIn("Block production", msg)

    def test_multi_file_variant(self):
        # Same mismatch spread across two files, via the multi-file entry point.
        sources = {
            "sections/timing.tex": (
                "\\section{Timing}\n"
                "% anvil-const: name=block_time_floor value=3 unit=s\n"
            ),
            "sections/production.tex": (
                "\\section{Production}\n"
                "% anvil-const: name=block_time_floor value=5 unit=s\n"
            ),
        }
        result = check_constant_consistency_multi(sources)
        self.assertFalse(result.passed)
        mismatch = next(
            v for v in result.violations if v.kind == "value-mismatch"
        )
        self.assertIn("sections/timing.tex", mismatch.message)
        self.assertIn("sections/production.tex", mismatch.message)


class TestBothoRingSize(unittest.TestCase):
    """The second botho mismatch: ring-size / byte-count disagreement."""

    def test_ring_size_byte_count_mismatch(self):
        sources = {
            "sections/memory.tex": (
                "\\section{Memory layout}\n"
                "% anvil-const: name=ring_size value=1024 unit=bytes\n"
            ),
            "sections/buffers.tex": (
                "\\section{Buffer sizing}\n"
                "% anvil-const: name=ring_size value=2048 unit=bytes\n"
            ),
        }
        result = check_constant_consistency_multi(sources)
        self.assertFalse(result.passed)
        mismatch = next(
            v for v in result.violations if v.kind == "value-mismatch"
        )
        self.assertEqual(mismatch.name, "ring_size")
        self.assertIn("1024", mismatch.message)
        self.assertIn("2048", mismatch.message)


class TestInlineTableRowMarker(unittest.TestCase):
    def test_inline_suffix_matches(self):
        # The inline table-row-comment variant — marker text after a table row.
        src = (
            "\\begin{tabular}{lll}\n"
            "Block time floor & 3\\,s & \\S2.1 \\\\ "
            "% anvil-const: name=block_time_floor value=3 unit=s\n"
            "\\end{tabular}\n"
            "\\section{Elsewhere}\n"
            "% anvil-const: name=block_time_floor value=5 unit=s\n"
        )
        result = check_constant_consistency(src)
        self.assertTrue(result.found)
        self.assertFalse(result.passed)
        self.assertEqual(
            [v.kind for v in result.violations], ["value-mismatch"]
        )


class TestCleanPass(unittest.TestCase):
    def test_identical_declarations_pass(self):
        sources = {
            "a.tex": (
                "\\section{A}\n"
                "% anvil-const: name=block_time_floor value=3 unit=s\n"
            ),
            "b.tex": (
                "\\section{B}\n"
                "% anvil-const: name=block_time_floor value=3 unit=s\n"
            ),
        }
        result = check_constant_consistency_multi(sources)
        self.assertTrue(result.found)
        self.assertTrue(result.passed)
        self.assertEqual(result.violations, [])

    def test_normalized_spacing_passes(self):
        # 3\,s vs 3 s vs 3s all normalize equal.
        src = (
            "% anvil-const: name=x value=3\\,s\n"
            "% anvil-const: name=x value=3s\n"
        )
        result = check_constant_consistency(src)
        self.assertTrue(result.passed)

    def test_thousands_separator_normalized(self):
        src = (
            "% anvil-const: name=ring value=1,024\n"
            "% anvil-const: name=ring value=1024\n"
        )
        result = check_constant_consistency(src)
        self.assertTrue(result.passed)


class TestGracefulDegradation(unittest.TestCase):
    def test_no_markers_inactive_and_passing(self):
        result = check_constant_consistency(
            "\\section{Timing}\nThe block floor is three seconds.\n"
        )
        self.assertIsInstance(result, ConstantConsistencyResult)
        self.assertFalse(result.found)
        self.assertTrue(result.passed)

    def test_empty_sources(self):
        result = check_constant_consistency_multi({})
        self.assertFalse(result.found)
        self.assertTrue(result.passed)


class TestMalformedDeclaration(unittest.TestCase):
    def test_missing_value_is_malformed(self):
        result = check_constant_consistency(
            "% anvil-const: name=block_time_floor unit=s\n"
        )
        self.assertTrue(result.found)
        self.assertFalse(result.passed)
        self.assertEqual(
            [v.kind for v in result.violations], ["malformed-declaration"]
        )
        self.assertIn("value", result.violations[0].message)

    def test_missing_name_is_malformed(self):
        result = check_constant_consistency(
            "% anvil-const: value=3 unit=s\n"
        )
        self.assertTrue(result.found)
        self.assertFalse(result.passed)
        self.assertEqual(result.violations[0].kind, "malformed-declaration")


class TestUnitMismatch(unittest.TestCase):
    def test_same_value_different_unit_is_unit_mismatch(self):
        # 3 s vs 3 ms — same numeric token, different unit. NOT a value
        # mismatch (3s != 3ms, but we don't convert), a distinct lower-severity
        # finding kind.
        src = (
            "% anvil-const: name=timeout value=3 unit=s\n"
            "% anvil-const: name=timeout value=3 unit=ms\n"
        )
        result = check_constant_consistency(src)
        self.assertFalse(result.passed)
        kinds = [v.kind for v in result.violations]
        self.assertIn("unit-mismatch", kinds)
        self.assertNotIn("value-mismatch", kinds)

    def test_unit_mismatch_does_not_mask_across_names(self):
        # A unit mismatch on one name must not suppress a value mismatch on
        # another.
        src = (
            "% anvil-const: name=a value=3 unit=s\n"
            "% anvil-const: name=a value=3 unit=ms\n"
            "% anvil-const: name=b value=1 unit=x\n"
            "% anvil-const: name=b value=2 unit=x\n"
        )
        result = check_constant_consistency(src)
        kinds = {v.kind for v in result.violations}
        self.assertEqual(kinds, {"unit-mismatch", "value-mismatch"})


class TestNewcommandDuplicateConflict(unittest.TestCase):
    def test_conflicting_redefinition_across_files(self):
        sources = {
            "macros.tex": "\\newcommand{\\blockfloor}{3\\,s}\n",
            "overrides.tex": "\\newcommand{\\blockfloor}{5\\,s}\n",
        }
        result = check_constant_consistency_multi(sources)
        self.assertTrue(result.found)
        self.assertFalse(result.passed)
        mismatch = next(
            v for v in result.violations if v.kind == "value-mismatch"
        )
        self.assertEqual(mismatch.name, "blockfloor")

    def test_consistent_macro_uses_pass(self):
        # A single macro definition (used everywhere) never disagrees.
        sources = {
            "macros.tex": "\\newcommand{\\blockfloor}{3\\,s}\n",
            "body.tex": "The floor is \\blockfloor{} per block.\n",
        }
        result = check_constant_consistency_multi(sources)
        self.assertTrue(result.found)
        self.assertTrue(result.passed)

    def test_renewcommand_conflict(self):
        src = (
            "\\newcommand{\\x}{10}\n"
            "\\renewcommand{\\x}{20}\n"
        )
        result = check_constant_consistency(src)
        self.assertFalse(result.passed)
        self.assertEqual(result.violations[0].kind, "value-mismatch")


class TestNormalizeValue(unittest.TestCase):
    def test_mathmode_stripped(self):
        self.assertEqual(normalize_value("$3$"), "3")

    def test_spacing_and_separators(self):
        self.assertEqual(normalize_value("1,024\\,B"), "1024B")
        self.assertEqual(normalize_value("3\\,s"), "3s")
        self.assertEqual(normalize_value(" 3 s "), "3s")


class TestToDictShape(unittest.TestCase):
    def test_to_dict_shape(self):
        src = (
            "% anvil-const: name=block_time_floor value=3 unit=s\n"
            "% anvil-const: name=block_time_floor value=5 unit=s\n"
        )
        payload = check_constant_consistency(src).to_dict()
        for key in ("found", "declarations", "violations", "passed"):
            self.assertIn(key, payload)
        self.assertTrue(payload["found"])
        self.assertFalse(payload["passed"])
        self.assertEqual(payload["violations"][0]["kind"], "value-mismatch")
        # Declarations carry name/value/unit/kind.
        decl = payload["declarations"][0]
        for key in ("name", "value", "unit", "section", "source", "kind"):
            self.assertIn(key, decl)

    def test_clean_to_dict_serializable(self):
        import json

        payload = check_constant_consistency(
            "% anvil-const: name=x value=1\n"
        ).to_dict()
        # Round-trips through JSON (the _gate.json contract).
        json.loads(json.dumps(payload))


if __name__ == "__main__":
    unittest.main()
