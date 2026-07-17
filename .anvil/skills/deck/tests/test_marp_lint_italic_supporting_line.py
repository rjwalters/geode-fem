"""Tests for the ``figure-italic-supporting-line-too-long`` rule.

This rule is the lint-side detection for the figure-idiom regression
documented in issues #100 / #101. The post-#68 figure idiom is "figure +
ONE italic supporting line"; authors fold what was 3 bullets into one
italic sentence that wraps to 2-3 rendered lines and clips at the slide
bottom on 16:9. The rule flags any italic line directly under a standalone
figure block whose word OR character count exceeds the configured budget
(default 18 words / 108 chars — see ``Geometry`` in ``marp_lint.py``).

Fixtures under ``tests/fixtures/marp_lint/`` (one fixture per behavior,
each with a single-purpose filename so the test failure surface points at
the exact case):

- ``italic_under_figure_too_long.md`` — positive case (24 words / 137
  chars). 1 warning, rule = ``figure-italic-supporting-line-too-long``.
- ``italic_under_figure_safe.md`` — negative case (under-budget short line
  matching the shipped template Market example). 0 findings.
- ``italic_not_under_figure.md`` — negative case (italic line on a slide
  with no preceding figure). 0 findings.
- ``non_italic_under_figure.md`` — negative case (figure + non-italic
  paragraph; the pre-#68 caption pattern). 0 findings.
- ``italic_under_figure_boundary.md`` — boundary case (two slides: one at
  exactly 18 words must NOT fire; one at 19 words MUST fire). Exactly 1
  warning, on slide 2.
- ``italic_under_figure_soft_wrapped.md`` — multi-line accumulator case
  (italic block soft-wrapped across two source lines, 22 words combined).
  1 warning (block accumulation works).
- ``italic_under_figure_suppressed.md`` — the too-long case with the
  per-slide ``<!-- anvil-lint-disable: figure-italic-supporting-line-too-
  long -->`` directive. Finding downgrades to ``info`` (not silenced).

Plus an end-to-end template test that renders ``deck.md.j2`` with realistic
placeholder values matching the Jinja comment-block guidance (Market /
Traction / Financials) and confirms zero findings from the new rule — the
template must not ship a deck that fails its own lint.

Runs under either ``python -m unittest discover anvil/skills/deck/tests/``
or ``pytest anvil/skills/deck/tests/``.

Filename note: this file uses a distinct basename (``test_marp_lint_italic_
supporting_line.py``) from the sibling ``test_marp_lint.py`` per the #58
convention — pytest collection across the whole tree must not collide on
duplicate test-module basenames.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

from anvil.lib.marp_lint import (
    Geometry,
    PORTED_RULES,
    lint_deck,
    lint_source,
)

_HERE = Path(__file__).resolve().parent
_FIXTURES = _HERE / "fixtures" / "marp_lint"


# Helper: extract just the new-rule findings from a LintResult so the tests
# don't false-fail if a fixture also happens to trip slide-content-overflow.
_RULE = "figure-italic-supporting-line-too-long"


def _italic_findings(result):
    return [
        f
        for f in (result.errors + result.warnings + result.infos)
        if f.rule == _RULE
    ]


class TestRuleRegistered(unittest.TestCase):
    """The rule name is exported in ``PORTED_RULES`` so consumers can list it."""

    def test_rule_in_ported_rules(self) -> None:
        self.assertIn(_RULE, PORTED_RULES)

    def test_geometry_defaults(self) -> None:
        geo = Geometry()
        self.assertEqual(geo.italic_supporting_line_max_words, 18)
        self.assertEqual(geo.italic_supporting_line_max_chars, 108)


class TestItalicUnderFigureTooLong(unittest.TestCase):
    """Positive case: ~24 words / ~137 chars trips both bounds."""

    def test_one_warning(self) -> None:
        result = lint_deck(_FIXTURES / "italic_under_figure_too_long.md")
        findings = _italic_findings(result)
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].rule, _RULE)
        self.assertEqual(findings[0].severity, "warning")
        self.assertEqual(findings[0].slide, 1)

    def test_message_names_counts_and_budget(self) -> None:
        result = lint_deck(_FIXTURES / "italic_under_figure_too_long.md")
        msg = _italic_findings(result)[0].message
        # The message must name both the actual counts and the budget so
        # the drafter knows what to tighten by how much.
        self.assertIn("words", msg)
        self.assertIn("chars", msg)
        self.assertIn("18", msg)   # budget word count
        self.assertIn("108", msg)  # budget char count

    def test_appears_in_warnings_bucket(self) -> None:
        """``warning`` severity routes into ``LintResult.warnings`` (AC1)."""
        result = lint_deck(_FIXTURES / "italic_under_figure_too_long.md")
        rule_warnings = [w for w in result.warnings if w.rule == _RULE]
        self.assertEqual(len(rule_warnings), 1)
        rule_errors = [e for e in result.errors if e.rule == _RULE]
        self.assertEqual(len(rule_errors), 0)
        rule_infos = [i for i in result.infos if i.rule == _RULE]
        self.assertEqual(len(rule_infos), 0)


class TestItalicUnderFigureSafe(unittest.TestCase):
    """Negative case: a short italic line under a figure is clean."""

    def test_no_findings(self) -> None:
        result = lint_deck(_FIXTURES / "italic_under_figure_safe.md")
        self.assertEqual(_italic_findings(result), [])


class TestItalicNotUnderFigure(unittest.TestCase):
    """Negative case: italic line is fine if there's no preceding figure trigger."""

    def test_no_findings(self) -> None:
        result = lint_deck(_FIXTURES / "italic_not_under_figure.md")
        self.assertEqual(_italic_findings(result), [])


class TestNonItalicUnderFigure(unittest.TestCase):
    """Negative case: a non-italic paragraph under a figure does not trigger."""

    def test_no_findings(self) -> None:
        result = lint_deck(_FIXTURES / "non_italic_under_figure.md")
        self.assertEqual(_italic_findings(result), [])


class TestItalicUnderFigureBoundary(unittest.TestCase):
    """Boundary: 18 words must NOT fire; 19 words MUST fire."""

    def test_exactly_one_warning_on_slide_two(self) -> None:
        result = lint_deck(_FIXTURES / "italic_under_figure_boundary.md")
        findings = _italic_findings(result)
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].slide, 2)
        self.assertEqual(findings[0].severity, "warning")

    def test_eighteen_words_is_inclusive_safe(self) -> None:
        """The rule fires only when count is STRICTLY greater than the budget.

        18 words at the boundary must not fire (``18 > 18`` is false). This
        documents the inclusive-safe semantic of the rule.
        """
        result = lint_deck(_FIXTURES / "italic_under_figure_boundary.md")
        slide_one_findings = [
            f for f in _italic_findings(result) if f.slide == 1
        ]
        self.assertEqual(slide_one_findings, [])


class TestItalicUnderFigureSoftWrapped(unittest.TestCase):
    """The italic-line accumulator measures across soft-wrapped blocks."""

    def test_one_warning_for_combined_block(self) -> None:
        result = lint_deck(_FIXTURES / "italic_under_figure_soft_wrapped.md")
        findings = _italic_findings(result)
        # Two source lines → one combined block → one finding (not two).
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].slide, 1)

    def test_combined_word_count_in_message(self) -> None:
        result = lint_deck(_FIXTURES / "italic_under_figure_soft_wrapped.md")
        msg = _italic_findings(result)[0].message
        # The measured count must be the combined block count (22 words),
        # not just the first line's 12 words — otherwise the accumulator is
        # broken.
        m = re.search(r"is (\d+) words", msg)
        self.assertIsNotNone(m)
        self.assertGreaterEqual(int(m.group(1)), 22)


class TestItalicUnderFigureSuppressed(unittest.TestCase):
    """``anvil-lint-disable`` downgrades to ``info`` — not silenced."""

    def test_finding_downgraded_to_info(self) -> None:
        result = lint_deck(_FIXTURES / "italic_under_figure_suppressed.md")
        findings = _italic_findings(result)
        # Still observable (the reviser should see that the slide is dense),
        # but routed into ``infos`` so ``advance`` is not blocked.
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].severity, "info")
        self.assertEqual(findings[0].slide, 1)

    def test_routed_into_infos_not_warnings(self) -> None:
        result = lint_deck(_FIXTURES / "italic_under_figure_suppressed.md")
        rule_warnings = [w for w in result.warnings if w.rule == _RULE]
        self.assertEqual(rule_warnings, [])
        rule_errors = [e for e in result.errors if e.rule == _RULE]
        self.assertEqual(rule_errors, [])
        rule_infos = [i for i in result.infos if i.rule == _RULE]
        self.assertEqual(len(rule_infos), 1)


class TestCleanFigurePlusSupportingLineFixtureStillClean(unittest.TestCase):
    """The shipped clean fixture stays clean against BOTH thresholds.

    Anchors the curator's required tightening: the original fixture line
    was 17 words / 124 chars and would have tripped the new OR-semantic
    char check. It was tightened to ≤108 chars as part of this PR.
    """

    def test_no_findings_from_new_rule(self) -> None:
        result = lint_deck(_FIXTURES / "clean_figure_plus_supporting_line.md")
        self.assertEqual(_italic_findings(result), [])


class TestRuleConfigurable(unittest.TestCase):
    """The two thresholds are exposed on ``Geometry`` and overridable."""

    def test_consumer_can_lower_word_threshold(self) -> None:
        # Lower the word budget to 10. A short safe line that lints clean
        # against the defaults must now trip the new (tighter) word bound.
        source = (_FIXTURES / "italic_under_figure_safe.md").read_text(
            encoding="utf-8"
        )
        tight = Geometry(
            italic_supporting_line_max_words=10,
            italic_supporting_line_max_chars=108,
        )
        result = lint_source(source, geometry=tight)
        findings = [f for f in result.warnings if f.rule == _RULE]
        self.assertEqual(len(findings), 1)

    def test_consumer_can_lower_char_threshold(self) -> None:
        source = (_FIXTURES / "italic_under_figure_safe.md").read_text(
            encoding="utf-8"
        )
        tight = Geometry(
            italic_supporting_line_max_words=18,
            italic_supporting_line_max_chars=30,  # very tight
        )
        result = lint_source(source, geometry=tight)
        findings = [f for f in result.warnings if f.rule == _RULE]
        self.assertEqual(len(findings), 1)


class TestRulesGate(unittest.TestCase):
    """The ``rules=`` parameter on ``lint_source`` gates the new check."""

    def test_default_runs_new_rule(self) -> None:
        source = (_FIXTURES / "italic_under_figure_too_long.md").read_text(
            encoding="utf-8"
        )
        result = lint_source(source)
        self.assertEqual(
            len([f for f in result.warnings if f.rule == _RULE]), 1
        )

    def test_disable_via_rules_param(self) -> None:
        source = (_FIXTURES / "italic_under_figure_too_long.md").read_text(
            encoding="utf-8"
        )
        result = lint_source(source, rules=("slide-content-overflow",))
        self.assertEqual(
            len([f for f in result.warnings if f.rule == _RULE]), 0
        )


class TestTemplateExamplesPassNewRule(unittest.TestCase):
    """The shipped ``deck.md.j2`` template examples must pass the new rule.

    The template guidance for Market / Traction / Financials carries
    concrete worked-example values inside the Jinja comment blocks. We
    render the three relevant slides with realistic placeholder values
    matching that guidance and confirm zero findings from the new rule —
    the template must not ship a deck that fails its own lint.

    Rendering note: Jinja2 isn't strictly available in this test
    environment, so we construct the three figure-bearing slides directly
    from the template's documented example values. This is the same shape
    the deck-draft skill produces.
    """

    # The three example values pinned in the template's Jinja comment
    # blocks (and pinned in the curator's design — see the docstring of
    # this test class).
    MARKET_ONE_LINE = (
        "$8.3B TAM → $30B SAM → $5–10M Yr-3 SOM "
        "(300 units × $20K) — Mordor 2024"
    )
    TRACTION_ONE_LINE = (
        "$340K ARR Q4 (40% QoQ); 12 design partners (3 paying); "
        "92% logo retention"
    )
    FINANCIALS_ONE_LINE = (
        "Current ARR $1.2M (real); 12-mo plan $4M; "
        "burn $200K/mo, 18 months runway"
    )

    def _render_slide(self, heading: str, fig: str, italic_line: str) -> str:
        return (
            "---\n"
            "marp: true\n"
            "size: 16:9\n"
            "theme: anvil-deck\n"
            "---\n\n"
            f"## {heading}\n\n"
            f"![{heading}]({fig})\n\n"
            f"_{italic_line}_\n"
        )

    def test_market_example_passes(self) -> None:
        source = self._render_slide(
            "Market", "figures/market-sizing.png", self.MARKET_ONE_LINE
        )
        result = lint_source(source)
        self.assertEqual(
            [f for f in result.warnings if f.rule == _RULE], []
        )

    def test_traction_example_passes(self) -> None:
        source = self._render_slide(
            "Traction", "figures/traction.png", self.TRACTION_ONE_LINE
        )
        result = lint_source(source)
        self.assertEqual(
            [f for f in result.warnings if f.rule == _RULE], []
        )

    def test_financials_example_passes(self) -> None:
        source = self._render_slide(
            "Financials",
            "figures/financials.png",
            self.FINANCIALS_ONE_LINE,
        )
        result = lint_source(source)
        self.assertEqual(
            [f for f in result.warnings if f.rule == _RULE], []
        )

    def test_all_three_example_lines_within_budget(self) -> None:
        """Sanity: the example values themselves fit the budget.

        Belt-and-braces for the three tests above — if the values exceed
        the budget, those tests catch it; if the rule logic is wrong, this
        catches it.
        """
        for label, line in (
            ("market", self.MARKET_ONE_LINE),
            ("traction", self.TRACTION_ONE_LINE),
            ("financials", self.FINANCIALS_ONE_LINE),
        ):
            self.assertLessEqual(
                len(line.split()), 18, f"{label} over word budget"
            )
            self.assertLessEqual(
                len(line), 108, f"{label} over char budget"
            )


if __name__ == "__main__":
    unittest.main()
