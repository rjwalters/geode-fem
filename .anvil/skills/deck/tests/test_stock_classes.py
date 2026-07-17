"""Static, CI-safe guard for the stock layout classes shipped in
``anvil-deck.css`` (issue #165).

PR #134 closed issue #128 with documentation + an `inline-display-style-dropped`
lint rule, but consumers still had to paste a frontmatter ``style: |`` block
defining their own ``.row`` class into every deck — the ergonomic gap that
prompted issue #165. v0 ships two stock classes in the theme baseline:

- ``section .row`` — flex container, gap-aware. Covers the diptych and
  triptych cases without separate ``.row.two`` / ``.row.three`` arity
  modifiers.
- ``section .split`` — explicit 50/50 grid. Use when ``.row``'s flex
  auto-distribution is undesirable.

Both selectors are scoped to ``section`` to mirror the existing
``section.title`` / ``section.ask`` / ``section.appendix`` / ``section.section``
override pattern in the same file, keeping the rules slide-local and
preventing collision with any consumer who happens to use ``.row`` outside a
deck context.

A true visual regression needs a Marp render plus pixel inspection, which is
not assumed available in CI. Instead this test reads ``anvil-deck.css``
directly and asserts the two new selectors are present with the expected
``display:`` properties. Pure presence/regex check — keeps the test
deterministic and decoupled from CSS parsing.

Runs under either ``python -m unittest discover anvil/skills/deck/tests/``
or ``pytest anvil/skills/deck/tests/``.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path


_HERE = Path(__file__).resolve().parent
_CSS = _HERE.parent / "assets" / "anvil-deck.css"


def _read_css() -> str:
    return _CSS.read_text(encoding="utf-8")


class TestStockClassesPresent(unittest.TestCase):
    """The two stock layout classes ship in the shipped CSS."""

    def test_css_file_present(self) -> None:
        self.assertTrue(_CSS.is_file(), f"missing stylesheet: {_CSS}")

    def test_row_selector_present_with_flex_display(self) -> None:
        """`.row` must declare ``display: flex`` inside a ``section`` scope.

        The flex auto-distribution is the core promise of the class — pour
        any number of columns and let them flex evenly.
        """
        css = _read_css()
        self.assertRegex(
            css,
            r"section\s+\.row\s*\{[^}]*display:\s*flex",
            "section .row must declare `display: flex` in anvil-deck.css",
        )

    def test_row_children_flex_one(self) -> None:
        """``section .row > *`` must set ``flex: 1`` so columns distribute
        evenly without per-child width hints."""
        css = _read_css()
        self.assertRegex(
            css,
            r"section\s+\.row\s*>\s*\*\s*\{[^}]*flex:\s*1",
            "section .row > * must declare `flex: 1`",
        )

    def test_split_selector_present_with_grid_display(self) -> None:
        """``.split`` must declare ``display: grid`` with ``1fr 1fr`` columns
        — the hard 50/50 alternative to ``.row``'s flex auto-distribution."""
        css = _read_css()
        # The selector + display: grid + the 1fr 1fr columns must all appear
        # together inside the rule body. Use a single regex over the rule
        # block to avoid coupling to whitespace/property-order quirks.
        match = re.search(
            r"section\s+\.split\s*\{([^}]*)\}",
            css,
        )
        self.assertIsNotNone(
            match,
            "section .split selector missing from anvil-deck.css",
        )
        body = match.group(1)
        self.assertRegex(
            body,
            r"display:\s*grid",
            "section .split must declare `display: grid`",
        )
        self.assertRegex(
            body,
            r"grid-template-columns:\s*1fr\s+1fr",
            "section .split must declare `grid-template-columns: 1fr 1fr` "
            "(the hard 50/50 split)",
        )

    def test_selectors_scoped_to_section(self) -> None:
        """Both selectors must be scoped to ``section`` — bare ``.row`` /
        ``.split`` at the top level would collide with any consumer who
        happens to use those class names outside a deck context, and would
        depart from the existing ``section.title`` / ``section.ask`` pattern
        in the same file.
        """
        css = _read_css()
        # The stock-classes section is the trailing block; isolate it via
        # the "Stock layout classes" comment anchor so we don't false-fail
        # if some upstream rule mentions `.row` in a different context.
        marker = "/* --- Stock layout classes"
        self.assertIn(
            marker,
            css,
            "stock-classes section header comment missing from anvil-deck.css",
        )
        block = css[css.index(marker) :]
        # No bare `.row {` or `.split {` at the start of a line inside the
        # stock-classes block — must always be `section .row` / `section .split`.
        self.assertNotRegex(
            block,
            r"(?m)^\s*\.row\s*\{",
            "bare `.row {` at top level — must be scoped as `section .row`",
        )
        self.assertNotRegex(
            block,
            r"(?m)^\s*\.split\s*\{",
            "bare `.split {` at top level — must be scoped as `section .split`",
        )


class TestStockClassesOrdering(unittest.TestCase):
    """The stock classes must ship *after* the slide-class overrides so the
    file remains single-source and the existing ``section.<class>`` block
    structure is preserved (curator decision: no separate
    ``stock-classes.css`` file)."""

    def test_stock_classes_appear_after_section_section(self) -> None:
        css = _read_css()
        # `section.section` is the last existing slide-class override block
        # before the stock classes are appended.
        section_section_pos = css.index("section.section")
        stock_marker_pos = css.index("/* --- Stock layout classes")
        self.assertLess(
            section_section_pos,
            stock_marker_pos,
            "stock layout classes must appear after the existing "
            "section.<class> override blocks",
        )


if __name__ == "__main__":
    unittest.main()
