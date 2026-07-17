"""Static, CI-safe guard for the ask-slide table contrast fix (issue #50).

A markdown table placed on an ``_class: ask`` slide used to render
white-on-white: the imported Marp ``default`` theme paints data-cell
backgrounds light, the ``section.ask`` cascade recolors text to ``#ffffff``,
and there was no ``section.ask table`` override to defeat the painted cell
background — so the funding rows became invisible on the navy ask slide.

A true visual regression needs a Marp render plus a pixel/contrast check,
which is not assumed available in CI. Instead this test reads
``anvil-deck.css`` directly and asserts the ask-table override rules are
present and correctly scoped. It is a low-fidelity guard, but it runs with
no renderer and directly prevents silent deletion/regression of the fix.

The companion repro deck lives at
``tests/fixtures/ask_table/use-of-funds.md`` (the minimal Marp document from
issue #50) so the bug is captured as an artifact for manual render
verification.

Runs under either ``python -m unittest discover anvil/skills/deck/tests/``
or ``pytest anvil/skills/deck/tests/``.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path


_HERE = Path(__file__).resolve().parent
_CSS = _HERE.parent / "assets" / "anvil-deck.css"
_FIXTURE = _HERE / "fixtures" / "ask_table" / "use-of-funds.md"


def _read_css() -> str:
    return _CSS.read_text(encoding="utf-8")


def _ask_block(css: str) -> str:
    """Return the slice of the stylesheet from the first ``section.ask``
    rule to the start of the appendix overrides.

    The fix must live *within* the ``section.ask`` override group (scoped to
    ask slides), so we assert the new rules appear in this slice rather than
    anywhere in the file. The appendix comment is a stable anchor that
    immediately follows the ask block.
    """
    start = css.index("section.ask")
    end = css.index("section.appendix")
    self_check = start < end
    assert self_check, "expected section.ask rules to precede section.appendix"
    return css[start:end]


class TestAskTableCssFixExists(unittest.TestCase):
    """The transparent-cell + white-text override must exist and be scoped."""

    def test_css_file_present(self) -> None:
        self.assertTrue(_CSS.is_file(), f"missing stylesheet: {_CSS}")

    def test_th_and_td_cells_are_transparent_and_white(self) -> None:
        block = _ask_block(_read_css())
        # The combined th/td selector that defeats the imported theme's
        # painted cell background and recolors the text.
        self.assertIn("section.ask table th,", block)
        self.assertIn("section.ask table td {", block)
        # Both background + background-color set (belt-and-suspenders against
        # the imported theme's shorthand vs longhand override quirks).
        self.assertIn("background: transparent;", block)
        self.assertIn("background-color: transparent;", block)
        self.assertIn("color: #ffffff;", block)

    def test_borders_recolored_to_translucent_white(self) -> None:
        block = _ask_block(_read_css())
        # th border matches the existing section.ask h2 treatment (0.4);
        # td border is a lighter translucent white (0.2).
        self.assertRegex(
            block,
            r"section\.ask table th\s*\{[^}]*border-bottom-color:\s*rgba\(255,\s*255,\s*255,\s*0\.4\)",
        )
        self.assertRegex(
            block,
            r"section\.ask table td\s*\{[^}]*border-bottom-color:\s*rgba\(255,\s*255,\s*255,\s*0\.2\)",
        )

    def test_zebra_striping_defeated(self) -> None:
        block = _ask_block(_read_css())
        # The tr/tbody/nth-child group defeats any zebra striping the
        # imported default theme applies on ask slides.
        self.assertIn("section.ask table tr,", block)
        self.assertIn("section.ask table tbody,", block)
        self.assertIn("section.ask table tbody tr:nth-child(odd),", block)
        self.assertIn("section.ask table tbody tr:nth-child(even) {", block)

    def test_no_hardcoded_navy_reintroduced_for_cells(self) -> None:
        """Cells must be transparent so ``var(--anvil-bg-ask)`` shows through.

        The ask-table override block must not re-state the navy background
        (#1f4e7a) on cells — that would defeat the point and couple the table
        to the literal token value.
        """
        css = _read_css()
        # Isolate just the ask-table override block (from its leading comment
        # to the appendix anchor) and assert no #1f4e7a appears there.
        marker = "/* Tables on the ask slide"
        self.assertIn(marker, css, "ask-table override block comment missing")
        block = css[css.index(marker) : css.index("section.appendix")]
        self.assertNotIn("#1f4e7a", block)

    def test_rules_scoped_to_table(self) -> None:
        """Selectors must be ``section.ask table ...`` — never bare
        ``section.ask td`` — so they cannot collide with future non-table
        ``td``-like layout on ask slides."""
        css = _read_css()
        marker = "/* Tables on the ask slide"
        block = css[css.index(marker) : css.index("section.appendix")]
        # No bare "section.ask td"/"section.ask th" (i.e. not followed by
        # " table") inside the override block.
        self.assertNotRegex(block, r"section\.ask t[dh]\b(?!\w)")


class TestReproFixturePresent(unittest.TestCase):
    """The minimal repro deck from issue #50 is committed as an artifact."""

    def test_fixture_exists(self) -> None:
        self.assertTrue(_FIXTURE.is_file(), f"missing repro fixture: {_FIXTURE}")

    def test_fixture_is_ask_slide_with_table(self) -> None:
        text = _FIXTURE.read_text(encoding="utf-8")
        # The repro must exercise an ask slide carrying a markdown table —
        # the exact shape that regressed (use-of-funds breakdown).
        self.assertIn("_class: ask", text)
        # A pipe-delimited table row with the data cells from the repro.
        self.assertRegex(text, r"\|\s*50%\s*\|")


if __name__ == "__main__":
    unittest.main()
