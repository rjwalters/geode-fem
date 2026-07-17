"""Tests for ``anvil.skills.slides.lib.marp_lint``.

The slides-side ``marp_lint`` is a re-export of the deck-side module (see
``anvil/skills/slides/lib/marp_lint.py``). These tests run the same fixtures
through the slides-side entry point to confirm:

1. The re-export gives the slides skill identical behaviour to the deck skill.
2. The fixtures appropriate to a slides-style talk (academic/conference) still
   trigger the right finding counts under the shared heuristic — the lint is
   renderer-pinned (Marp), not skill-pinned, so the same overflow patterns are
   defects in either context.

Mirrors ``anvil/skills/deck/tests/test_marp_lint.py``.
"""

from __future__ import annotations

import unittest
from pathlib import Path

from anvil.lib.marp_lint import (
    Finding,
    LintResult,
    UPSTREAM_SHA,
    PORTED_RULES,
    lint_deck,
    lint_source,
)

_HERE = Path(__file__).resolve().parent
_FIXTURES = _HERE / "fixtures" / "marp_lint"


class TestSlidesMirror(unittest.TestCase):
    """The slides-side re-export must expose the same public surface."""

    def test_public_api(self) -> None:
        # AC1 contract: lint_deck + LintResult + structured Findings.
        self.assertTrue(callable(lint_deck))
        self.assertTrue(callable(lint_source))
        # The slides skill mirrors the deck skill's marp_lint module, so
        # whatever rules the deck side ships also appear here. The contract
        # this test pins is the marp-vscode-ported rule (the one with a
        # tracked ``UPSTREAM_SHA``); Anvil-original rules grow the tuple
        # additively as they land in the deck skill.
        self.assertIn("slide-content-overflow", PORTED_RULES)
        # The upstream SHA pin is shared between deck and slides.
        self.assertTrue(UPSTREAM_SHA)
        self.assertEqual(len(UPSTREAM_SHA), 40)


class TestOverflowFigurePlusBullets(unittest.TestCase):
    """Slides analog of the #24 overflow pattern. Expected: 1 error."""

    def test_one_error_one_slide(self) -> None:
        result = lint_deck(_FIXTURES / "overflow_figure_plus_bullets.md")
        self.assertEqual(len(result.errors), 1)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(result.errors[0].slide, 1)
        self.assertEqual(result.errors[0].rule, "slide-content-overflow")


class TestOverflowAskH1PlusH2(unittest.TestCase):
    """Slides analog of the #25 H1 + H2 pattern. Expected: 1 error."""

    def test_one_error_one_slide(self) -> None:
        result = lint_deck(_FIXTURES / "overflow_ask_h1_plus_h2.md")
        self.assertEqual(len(result.errors), 1)
        self.assertEqual(result.errors[0].slide, 1)


class TestCleanFigurePlusSupportingLine(unittest.TestCase):
    """Working idiom — figure + one italic supporting line. No findings."""

    def test_no_findings(self) -> None:
        result = lint_deck(_FIXTURES / "clean_figure_plus_supporting_line.md")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(len(result.infos), 0)


class TestBorderlineDenseBullets(unittest.TestCase):
    """Just-above-threshold dense slide: 0 errors, 1 warning."""

    def test_one_warning_no_error(self) -> None:
        result = lint_deck(_FIXTURES / "borderline_dense_bullets.md")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 1)


class TestEscapeHatchDisabled(unittest.TestCase):
    """``anvil-lint-disable`` downgrades the slide-content-overflow hit."""

    def test_finding_downgraded_to_info(self) -> None:
        result = lint_deck(_FIXTURES / "escape_hatch_disabled.md")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(len(result.infos), 1)
        self.assertEqual(result.infos[0].severity, "info")


# ---------------------------------------------------------------------------
# inline-display-style-dropped — mirrors deck-side TestInlineDisplay* classes.
#
# Pins the slides-side re-export's behaviour for the rule landed by deck PR
# #134 (issue #128). The lint module is a re-export shim
# (``anvil/skills/slides/lib/marp_lint.py``), so these tests guard against
# any silent divergence between the deck source of truth and the slides
# import path — fixtures are byte-identical to the deck-side ones.
# ---------------------------------------------------------------------------


class TestInlineDisplayGridDropped(unittest.TestCase):
    """``<div style="display:grid;...">`` — silently dropped by Marp foreignObject SVG render."""

    def test_one_warning(self) -> None:
        result = lint_deck(_FIXTURES / "inline_display_grid_dropped.md")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 1)
        self.assertEqual(result.warnings[0].slide, 1)
        self.assertEqual(result.warnings[0].rule, "inline-display-style-dropped")
        self.assertEqual(result.warnings[0].severity, "warning")

    def test_message_includes_detected_value(self) -> None:
        result = lint_deck(_FIXTURES / "inline_display_grid_dropped.md")
        msg = result.warnings[0].message
        self.assertIn("display:grid", msg)
        self.assertIn("foreignObject", msg)


class TestInlineDisplayFlexDropped(unittest.TestCase):
    """``<div style="display:flex;...">`` — silently dropped, same path."""

    def test_one_warning(self) -> None:
        result = lint_deck(_FIXTURES / "inline_display_flex_dropped.md")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 1)
        self.assertEqual(result.warnings[0].slide, 1)
        self.assertEqual(result.warnings[0].rule, "inline-display-style-dropped")
        self.assertIn("display:flex", result.warnings[0].message)


class TestInlineDisplayInlineGridDropped(unittest.TestCase):
    """``display:inline-grid`` variant — case-insensitive, no whitespace around ``:``."""

    def test_one_warning(self) -> None:
        result = lint_deck(_FIXTURES / "inline_display_inline_grid_dropped.md")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 1)
        self.assertEqual(result.warnings[0].rule, "inline-display-style-dropped")
        self.assertIn("display:inline-grid", result.warnings[0].message)


class TestInlineDisplaySafe(unittest.TestCase):
    """Frontmatter ``style: |`` + ``<div class="row">`` — the workaround. No findings."""

    def test_no_findings(self) -> None:
        result = lint_deck(_FIXTURES / "inline_display_safe.md")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(len(result.infos), 0)


class TestInlineDisplayOtherStyleSafe(unittest.TestCase):
    """Inline ``style="color: red"`` etc. — the rule must NOT fire on non-`display:` rules."""

    def test_no_findings(self) -> None:
        result = lint_deck(_FIXTURES / "inline_display_other_style_safe.md")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(
            len([f for f in result.warnings if f.rule == "inline-display-style-dropped"]),
            0,
        )


class TestInlineDisplaySuppressed(unittest.TestCase):
    """``anvil-lint-disable: inline-display-style-dropped`` downgrades the finding."""

    def test_finding_downgraded_to_info(self) -> None:
        result = lint_deck(_FIXTURES / "inline_display_suppressed.md")
        self.assertEqual(len(result.errors), 0)
        # No warnings from THIS rule (the lint should have downgraded).
        self.assertEqual(
            len([f for f in result.warnings if f.rule == "inline-display-style-dropped"]),
            0,
        )
        inline_infos = [
            f for f in result.infos if f.rule == "inline-display-style-dropped"
        ]
        self.assertEqual(len(inline_infos), 1)
        self.assertEqual(inline_infos[0].severity, "info")


class TestInlineDisplayInCodeFenceIgnored(unittest.TestCase):
    """A ``style="display:grid"`` inside a fenced code block is documentation, not a render bug."""

    def test_no_findings_in_code_fence(self) -> None:
        source = """---
marp: true
size: 16:9
---

## Documentation slide

Here is the broken pattern:

```html
<div style="display: grid; grid-template-columns: 1fr 1fr;">
  <div>a</div>
  <div>b</div>
</div>
```

This documents the pattern but does not render it.
"""
        result = lint_source(source)
        self.assertEqual(
            len([f for f in result.warnings if f.rule == "inline-display-style-dropped"]),
            0,
        )


class TestInlineDisplaySingleQuoted(unittest.TestCase):
    """``<div style='display:grid;...'>`` — single-quoted attribute also matches."""

    def test_single_quoted_fires(self) -> None:
        source = """---
marp: true
---

## Two-column

<div style='display: grid; grid-template-columns: 1fr 1fr;'>
  <div>a</div>
  <div>b</div>
</div>
"""
        result = lint_source(source)
        inline = [
            f for f in result.warnings if f.rule == "inline-display-style-dropped"
        ]
        self.assertEqual(len(inline), 1)


class TestInlineDisplayCaseInsensitive(unittest.TestCase):
    """``style="DISPLAY: Grid"`` — the regex must be case-insensitive."""

    def test_uppercase_display_fires(self) -> None:
        source = """---
marp: true
---

## Two-column

<div style="DISPLAY: Grid; grid-template-columns: 1fr 1fr;">
  <div>a</div>
  <div>b</div>
</div>
"""
        result = lint_source(source)
        inline = [
            f for f in result.warnings if f.rule == "inline-display-style-dropped"
        ]
        self.assertEqual(len(inline), 1)


class TestPortedRulesIncludesInlineDisplay(unittest.TestCase):
    """The slides-side ``PORTED_RULES`` re-export advertises the new rule.

    Mirrors the deck-side assertion: the re-export shim must surface every
    rule the deck source-of-truth ships, so the slides skill's lint and the
    deck skill's lint stay behaviourally identical.
    """

    def test_rule_name_in_ported_rules(self) -> None:
        self.assertIn("inline-display-style-dropped", PORTED_RULES)
        self.assertIn("slide-content-overflow", PORTED_RULES)
        self.assertIn("figure-italic-supporting-line-too-long", PORTED_RULES)


if __name__ == "__main__":
    unittest.main()
