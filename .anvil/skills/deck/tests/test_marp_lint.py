"""Tests for ``anvil.skills.deck.lib.marp_lint``.

Each test exercises one fixture in ``tests/fixtures/marp_lint/`` against
``lint_deck``. The fixtures correspond to AC4 on issue #31:

- ``overflow_figure_plus_bullets.md`` reproduces the #24 pattern (image + 4
  bullets + footer line). Expected: 1 error.
- ``overflow_ask_h1_plus_h2.md`` reproduces the #25 pattern (`_class: ask`
  with both H1 and H2 plus bullets). Expected: 1 error.
- ``clean_figure_plus_supporting_line.md`` is the working idiom from #24
  (one figure + one italic supporting line). Expected: 0 errors, 0 warnings.
- ``borderline_dense_bullets.md`` is a dense slide that overflows the
  warning threshold but stays under the error threshold. Expected:
  0 errors, 1 warning.
- ``escape_hatch_disabled.md`` is the #24 overflow case with the
  ``<!-- anvil-lint-disable: slide-content-overflow -->`` directive — the
  finding must be downgraded to ``info`` (escape hatch tested for AC5).

Runs under either ``python -m unittest discover anvil/skills/deck/tests/``
or ``pytest anvil/skills/deck/tests/``.
"""

from __future__ import annotations

import struct
import tempfile
import unittest
from pathlib import Path

from anvil.lib.marp_lint import lint_deck, lint_source, Finding, LintResult

_HERE = Path(__file__).resolve().parent
_FIXTURES = _HERE / "fixtures" / "marp_lint"


def _write_png_header(path: Path, width: int, height: int) -> None:
    """Write a minimal PNG file whose IHDR declares ``width`` × ``height``.

    ``anvil.lib.marp_lint._read_png_dimensions`` reads only the first 24
    bytes (8-byte signature + IHDR length/type + big-endian width/height), so
    a header-only file is sufficient to exercise the aspect-aware image
    charge (issue #622) without generating megabytes of pixel data.
    """
    with open(path, "wb") as fh:
        fh.write(b"\x89PNG\r\n\x1a\n")
        fh.write(struct.pack(">I", 13))  # IHDR chunk length
        fh.write(b"IHDR")
        fh.write(struct.pack(">II", width, height))
        fh.write(b"\x08\x02\x00\x00\x00")  # bit depth, color type, etc.


class TestOverflowFigurePlusBullets(unittest.TestCase):
    """The #24 repro — one image + 4 bullets + footer line. Expected: 1 error."""

    def test_one_error_one_slide(self) -> None:
        result = lint_deck(_FIXTURES / "overflow_figure_plus_bullets.md")
        self.assertEqual(len(result.errors), 1)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(result.errors[0].slide, 1)
        self.assertEqual(result.errors[0].rule, "slide-content-overflow")
        self.assertEqual(result.errors[0].severity, "error")


class TestOverflowAskH1PlusH2(unittest.TestCase):
    """The #25 repro — `_class: ask` with H1 + H2 + bullets. Expected: 1 error."""

    def test_one_error_one_slide(self) -> None:
        result = lint_deck(_FIXTURES / "overflow_ask_h1_plus_h2.md")
        self.assertEqual(len(result.errors), 1)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(result.errors[0].slide, 1)
        self.assertEqual(result.errors[0].rule, "slide-content-overflow")

    def test_message_mentions_h1_and_h2(self) -> None:
        result = lint_deck(_FIXTURES / "overflow_ask_h1_plus_h2.md")
        msg = result.errors[0].message
        # The top-costs roll-up should surface both heading levels for the
        # H1+H2 anti-pattern slide — the heuristic exists precisely because
        # both headings contribute to the overflow.
        self.assertIn("h1", msg)
        self.assertIn("h2", msg)


class TestCleanFigurePlusSupportingLine(unittest.TestCase):
    """The working idiom — one figure + one italic line. No findings."""

    def test_no_findings(self) -> None:
        result = lint_deck(_FIXTURES / "clean_figure_plus_supporting_line.md")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(len(result.infos), 0)


class TestBorderlineDenseBullets(unittest.TestCase):
    """A slide near but below the error threshold. One warning, no error."""

    def test_one_warning_no_error(self) -> None:
        result = lint_deck(_FIXTURES / "borderline_dense_bullets.md")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 1)
        self.assertEqual(result.warnings[0].slide, 1)
        self.assertEqual(result.warnings[0].severity, "warning")


class TestEscapeHatchDisabled(unittest.TestCase):
    """``anvil-lint-disable`` downgrades the finding to ``info``."""

    def test_finding_downgraded_to_info(self) -> None:
        result = lint_deck(_FIXTURES / "escape_hatch_disabled.md")
        # The slide would normally be an error (it's a copy of the #24 case);
        # the directive must downgrade it.
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(len(result.infos), 1)
        self.assertEqual(result.infos[0].slide, 1)
        self.assertEqual(result.infos[0].severity, "info")

    def test_review_is_not_blocked(self) -> None:
        """The escape hatch must mean ``advance`` is not forced false.

        We don't simulate the review pipeline here, but we do assert the
        ``errors`` list is empty — which is the input the review machinery
        uses to set ``advance: false``. If this is empty, advance is not
        forced false by the lint.
        """
        result = lint_deck(_FIXTURES / "escape_hatch_disabled.md")
        self.assertEqual(len(result.errors), 0)


class TestLintResultShape(unittest.TestCase):
    """AC1: ``LintResult`` exposes structured ``Finding``s with the documented schema."""

    def test_finding_fields(self) -> None:
        result = lint_deck(_FIXTURES / "overflow_figure_plus_bullets.md")
        finding = result.errors[0]
        self.assertIsInstance(finding, Finding)
        self.assertIsInstance(finding.slide, int)
        self.assertIsInstance(finding.line, int)
        self.assertIsInstance(finding.rule, str)
        self.assertIsInstance(finding.severity, str)
        self.assertIsInstance(finding.message, str)
        # ``line`` should point inside the source file (1-based).
        self.assertGreaterEqual(finding.line, 1)

    def test_to_summary_shape(self) -> None:
        result = lint_deck(_FIXTURES / "overflow_figure_plus_bullets.md")
        summary = result.to_summary()
        self.assertTrue(summary["ran"])
        self.assertEqual(summary["errors"], 1)
        self.assertEqual(summary["warnings"], 0)
        self.assertIn("errors_by_slide", summary)
        self.assertEqual(summary["errors_by_slide"][0]["slide"], 1)
        self.assertEqual(summary["errors_by_slide"][0]["rule"], "slide-content-overflow")


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
    """``PORTED_RULES`` advertises the new rule alongside the existing two."""

    def test_rule_name_in_ported_rules(self) -> None:
        from anvil.lib.marp_lint import PORTED_RULES  # noqa: WPS433
        self.assertIn("inline-display-style-dropped", PORTED_RULES)
        self.assertIn("slide-content-overflow", PORTED_RULES)
        self.assertIn("figure-italic-supporting-line-too-long", PORTED_RULES)


class TestMultiSlideSource(unittest.TestCase):
    """Multi-slide source: only the offending slides emit findings.

    Confirms the slide numbering is 1-based across an entire deck (not just
    a single-slide fixture).
    """

    def test_per_slide_findings(self) -> None:
        # Three slides: clean / overflow / clean. Expected: 1 error on slide 2.
        source = """---
marp: true
size: 16:9
---

## Clean intro slide

A single sentence introducing the deck.

---

## Market — TAM / SAM / SOM

![TAM / SAM / SOM](figures/market-sizing.png)

- **TAM**: $8.3B hardware → $11.9B by 2028 (Mordor Intelligence)
- **SAM**: $30B addressable across HNW + HENRY consumers
- **SOM Yr 3**: $5–10M (300 units × $20K, Pagani-shape)
- **Growth driver**: 18.8% CAGR in adjacent data-layer segment

_Source: Mordor Intelligence._

---

## Clean closing slide

_Thanks for your time — questions?_
"""
        result = lint_source(source)
        self.assertEqual(len(result.errors), 1)
        self.assertEqual(result.errors[0].slide, 2)


class TestMarpImageSizingKeywords(unittest.TestCase):
    """Issue #562 — Marp image sizing keywords reduce phantom overflow errors.

    Pre-#562: ``marp_lint`` charged a fixed ``image_units = 7.0u`` for every
    standalone image regardless of its declared sizing. The GoodBoy deck used
    ``![bg right:55%]`` hero panels (background → zero body flow) and
    ``![h:230px]`` clamped figures (~3u vertical) heavily; the lint charged
    them all at 7.0u and fired 6–8 phantom ``slide-content-overflow`` errors
    per pass. The reviewer was forced to hand-confirm against the rendered
    PDF on every revision — defeating the deterministic gate.

    Post-#562: ``_image_cost_units`` parses the alt-string for Marp sizing
    keywords (``bg``, ``h:N``, ``w:N``) and returns:
      - 0u for any ``bg`` form (background image; behind body content)
      - ``h_px / body_line_height_px`` for ``h:NNNpx``
      - ``(pct/100) * capacity_units`` for ``h:N%``
      - the legacy ``image_small_units`` for ``w:`` <50% (preserved)
      - the legacy ``image_units`` for unannotated standalone images

    These regression cases anchor the false-positive killer (AC1, AC2) and
    a positive control (AC3 — a ``bg`` slide with overflowing bullets must
    still flag; the background is free but the bullets blow budget on their
    own).
    """

    def test_bg_right_panel_does_not_overflow(self) -> None:
        """AC1 — ``![bg right:55%]`` hero panel + H1 + 2 bullets stays clean.

        Background images consume zero body flow. The pre-#562 model
        charged 7.0u; the slide also has an H1 (3.2u) + 2 bullets (~2.2u)
        + a paragraph-break (0.4u) ≈ 5.8u of real content, well under
        the 13u budget. Post-#562 must report zero ``slide-content-
        overflow`` findings.
        """
        source = """---
marp: true
size: 16:9
---

# Hero panel

![bg right:55%](figures/hero.png)

- One bullet here
- Another bullet here
"""
        result = lint_source(source)
        overflow_findings = [
            f for f in result.errors + result.warnings
            if f.rule == "slide-content-overflow"
        ]
        self.assertEqual(
            overflow_findings, [],
            f"bg right:N% panel must not trigger overflow; got "
            f"{[f.message for f in overflow_findings]}",
        )

    def test_bg_left_panel_does_not_overflow(self) -> None:
        """The ``bg left:N%`` variant matches the same panel grammar."""
        source = """---
marp: true
size: 16:9
---

# Hero panel

![bg left:40%](figures/hero.png)

- One bullet
- Another bullet
"""
        result = lint_source(source)
        overflow_findings = [
            f for f in result.errors + result.warnings
            if f.rule == "slide-content-overflow"
        ]
        self.assertEqual(overflow_findings, [])

    def test_bg_vertical_panel_does_not_overflow(self) -> None:
        """The ``bg vertical:N%`` form (split-panel vertical) is also free."""
        source = """---
marp: true
size: 16:9
---

# Section

![bg vertical:30%](figures/hero.png)

- One bullet
- Another bullet
"""
        result = lint_source(source)
        overflow_findings = [
            f for f in result.errors + result.warnings
            if f.rule == "slide-content-overflow"
        ]
        self.assertEqual(overflow_findings, [])

    def test_h_clamped_figure_does_not_overflow(self) -> None:
        """AC2 — ``![h:230px]`` clamped figure + H2 + 3 bullets stays clean.

        ``h:230px`` at ``body_line_height_px = 40`` translates to 5.75u of
        vertical cost (vs the pre-#562 full-image 7.0u). With an H2 (2.0u),
        three bullets (~3.3u), and a paragraph-break (0.4u), total is
        ~11.5u — within the 13u budget. Post-#562 must report zero
        ``slide-content-overflow`` findings.
        """
        source = """---
marp: true
size: 16:9
---

## Clamped figure

![h:230px](figures/clamped.png)

- First point
- Second point
- Third point
"""
        result = lint_source(source)
        overflow_findings = [
            f for f in result.errors + result.warnings
            if f.rule == "slide-content-overflow"
        ]
        self.assertEqual(
            overflow_findings, [],
            f"h:230px clamped figure must not trigger overflow; got "
            f"{[f.message for f in overflow_findings]}",
        )

    def test_h_percent_clamped_figure_does_not_overflow(self) -> None:
        """``h:40%`` reduces vertical cost to ~5.2u, well within budget."""
        source = """---
marp: true
size: 16:9
---

## Percent-clamped figure

![h:40%](figures/clamped.png)

- First point
- Second point
"""
        result = lint_source(source)
        overflow_findings = [
            f for f in result.errors + result.warnings
            if f.rule == "slide-content-overflow"
        ]
        self.assertEqual(overflow_findings, [])

    def test_bg_full_bleed_plus_overflowing_bullets_still_flags(self) -> None:
        """AC3 — ``![bg]`` (full-bleed) is free, but enough bullets still flag.

        Regression guard for the positive control: the background is free,
        but bullets on top of it that legitimately exceed the budget must
        still raise ``slide-content-overflow``. This proves the fix did
        not over-correct into a blanket "ignore image-bearing slides"
        rule.
        """
        source = """---
marp: true
size: 16:9
---

# Heavy overlay slide

![bg](figures/full-bleed.png)

- A very long first bullet that wraps around the budget enough to count
- Another long bullet that wraps and adds to the budget enough to count
- Yet another long bullet that wraps and adds to the budget enough to count
- A fourth long bullet that wraps and adds to the budget enough to count
- A fifth long bullet that wraps and adds to the budget enough to count
- A sixth long bullet that wraps and adds to the budget enough to count
- A seventh long bullet that wraps and adds to the budget enough to count
- An eighth long bullet that wraps and adds to the budget enough to count
- A ninth long bullet that wraps and adds to the budget enough to count
"""
        result = lint_source(source)
        overflow_findings = [
            f for f in result.errors
            if f.rule == "slide-content-overflow"
        ]
        # The background is free, but 9 wrapping bullets + H1 + paragraph
        # breaks exceed the 13u budget on their own — the slide must
        # still flag.
        self.assertGreaterEqual(
            len(overflow_findings), 1,
            "background + overflowing bullets must still raise overflow",
        )

    def test_w_only_keyword_still_works_legacy(self) -> None:
        """``![w:30%]`` (no h:, no bg) preserves the legacy 'small' downgrade.

        Backward-compat check — the pre-#562 width-only heuristic still
        applies when no ``h:`` keyword is present. A small image leaves
        room for additional body content.
        """
        source = """---
marp: true
size: 16:9
---

## Small image

![w:30%](figures/small.png)

- First point
- Second point
- Third point
- Fourth point
- Fifth point
- Sixth point
"""
        result = lint_source(source)
        # With image-small (3.0u) instead of image (7.0u), there's room
        # for ~6 bullets in the budget.
        overflow_findings = [
            f for f in result.errors
            if f.rule == "slide-content-overflow"
        ]
        self.assertEqual(overflow_findings, [])

    def test_h_and_w_both_present_h_wins(self) -> None:
        """When both ``h:`` and ``w:`` are present, ``h:`` is the source of cost.

        The ``h:`` keyword is the direct vertical-cost signal; ``w:`` is
        only a fallback used when ``h:`` is absent. For ``h:200px w:40%``,
        the cost is ``200/40 = 5.0u`` (the h: keyword), not 3.0u (the
        legacy w: small downgrade).
        """
        source = """---
marp: true
size: 16:9
---

## Both keywords

![w:40% h:200px](figures/both.png)

- First point
"""
        result = lint_source(source)
        # Either way, no overflow on a slide this sparse — but the test
        # exists to document the precedence rule. We just verify clean.
        overflow_findings = [
            f for f in result.errors + result.warnings
            if f.rule == "slide-content-overflow"
        ]
        self.assertEqual(overflow_findings, [])

    def test_unannotated_image_uses_full_cost(self) -> None:
        """An image with NO sizing keywords falls back to the full 7.0u cost.

        Backward-compat regression guard — the documented behaviour for
        unannotated standalone images (full-width assumption) must not
        change. The existing ``overflow_figure_plus_bullets.md`` fixture
        already exercises this path; this test pins the same contract
        explicitly to make the intent legible at the test surface.
        """
        source = """---
marp: true
size: 16:9
---

## Unannotated image

![alt text](figures/unannotated.png)

- First point that wraps around the budget enough to count toward the total
- Second point that wraps around the budget enough to count toward the total
- Third point that wraps around the budget enough to count toward the total
- Fourth point that wraps around the budget enough to count toward the total
- Fifth point that wraps around the budget enough to count toward the total
"""
        result = lint_source(source)
        # Unannotated image (~7u) + H2 (2u) + 5 wrapping bullets (~7u) +
        # paragraph break (0.4u) = ~16u, over the 13u budget.
        overflow_findings = [
            f for f in result.errors
            if f.rule == "slide-content-overflow"
        ]
        self.assertGreaterEqual(
            len(overflow_findings), 1,
            "unannotated image with overflowing bullets must still flag",
        )


class TestImageCostUnits(unittest.TestCase):
    """Unit tests for the ``_image_cost_units`` helper (issue #562).

    The helper is a pure function on (alt_text, geometry) — testable in
    isolation without constructing a full slide.
    """

    def test_bg_keyword_returns_zero(self) -> None:
        from anvil.lib.marp_lint import Geometry, _image_cost_units
        units, label = _image_cost_units("bg", Geometry())
        self.assertEqual(units, 0.0)
        self.assertEqual(label, "image-background")

    def test_bg_right_panel_returns_zero(self) -> None:
        from anvil.lib.marp_lint import Geometry, _image_cost_units
        units, label = _image_cost_units("bg right:55%", Geometry())
        self.assertEqual(units, 0.0)
        self.assertEqual(label, "image-background")

    def test_bg_left_panel_returns_zero(self) -> None:
        from anvil.lib.marp_lint import Geometry, _image_cost_units
        units, label = _image_cost_units("bg left:40%", Geometry())
        self.assertEqual(units, 0.0)

    def test_bg_vertical_returns_zero(self) -> None:
        from anvil.lib.marp_lint import Geometry, _image_cost_units
        units, label = _image_cost_units("bg vertical:30%", Geometry())
        self.assertEqual(units, 0.0)

    def test_h_pixels_translates_to_units(self) -> None:
        from anvil.lib.marp_lint import Geometry, _image_cost_units
        geo = Geometry()
        # h:200px @ body_line_height_px=40 → 5.0u
        units, label = _image_cost_units("h:200px", geo)
        self.assertAlmostEqual(units, 5.0, places=1)
        self.assertIn("h:200px", label)

    def test_h_percent_translates_to_units(self) -> None:
        from anvil.lib.marp_lint import Geometry, _image_cost_units
        geo = Geometry()
        # h:50% @ capacity_units=13.0 → 6.5u
        units, label = _image_cost_units("h:50%", geo)
        self.assertAlmostEqual(units, 6.5, places=1)

    def test_w_only_small_downgrades(self) -> None:
        from anvil.lib.marp_lint import Geometry, _image_cost_units
        geo = Geometry()
        units, label = _image_cost_units("w:30%", geo)
        self.assertEqual(units, geo.image_small_units)
        self.assertEqual(label, "image-small")

    def test_no_keywords_returns_full(self) -> None:
        from anvil.lib.marp_lint import Geometry, _image_cost_units
        geo = Geometry()
        units, label = _image_cost_units("alt text only", geo)
        self.assertEqual(units, geo.image_units)
        self.assertEqual(label, "image")

    def test_empty_alt_returns_full(self) -> None:
        from anvil.lib.marp_lint import Geometry, _image_cost_units
        geo = Geometry()
        units, label = _image_cost_units("", geo)
        self.assertEqual(units, geo.image_units)


class TestWideAspectImageCharge(unittest.TestCase):
    """Issue #622 — a keyword-less wide-aspect PNG is charged its true height.

    Pre-#622 a standalone ``![](fig.png)`` with no ``bg``/``h:``/``w:`` keyword
    was charged the flat ``image_units = 7.0u`` regardless of aspect ratio.
    A wide-aspect mermaid PNG (e.g. a 3096×684 flow chart, ~4.5:1) renders at
    only ~258px in a 1168px content area — ~6.5u — and a very-wide 10:1 chart
    renders at ~3u. Charging 7.0u for those over-fired the overflow gate on
    keyword-less decks (the default drafter output). Post-#622 the charge is
    the width-normalized rendered height, capped at 7.0u, when the image is a
    locally-resolvable PNG relative to the deck file.
    """

    def test_wide_aspect_png_charge_below_flat(self) -> None:
        """AC1 — 3096×684 PNG at full content width charges ≈6.5u (< 7.0u)."""
        from anvil.lib.marp_lint import Geometry, _image_cost_units

        with tempfile.TemporaryDirectory() as tmp:
            deck_dir = Path(tmp)
            _write_png_header(deck_dir / "wide.png", 3096, 684)
            units, label = _image_cost_units(
                "",
                Geometry(),
                image_path="wide.png",
                deck_path=deck_dir / "deck.md",
            )
        # 684 * (1168 / 3096) / 40 ≈ 6.45u
        self.assertAlmostEqual(units, 6.45, places=1)
        self.assertLess(units, Geometry().image_units)
        self.assertIn("3096x684", label)

    def test_wide_aspect_png_slide_no_overflow(self) -> None:
        """AC1 — a slide carrying only the wide PNG produces no overflow."""
        with tempfile.TemporaryDirectory() as tmp:
            deck_dir = Path(tmp)
            _write_png_header(deck_dir / "wide.png", 3096, 684)
            deck = deck_dir / "deck.md"
            deck.write_text(
                "---\nmarp: true\nsize: 16:9\n---\n\n"
                "## Architecture\n\n![](wide.png)\n",
                encoding="utf-8",
            )
            result = lint_deck(deck)
        overflow = [
            f for f in result.errors + result.warnings
            if f.rule == "slide-content-overflow"
        ]
        self.assertEqual(overflow, [])

    def test_very_wide_png_reduction_is_load_bearing(self) -> None:
        """A 10:1 PNG + bullets that overflow at 7.0u stay clean at ~3u.

        This proves the aspect refinement actually changes the verdict, not
        just the reported number: the same slide overflows when the image is
        unresolvable (flat 7.0u) but fits when the wide PNG is measured.
        """
        body = (
            "---\nmarp: true\nsize: 16:9\n---\n\n"
            "## Flow\n\n![](flow.png)\n\n"
            "- First point that wraps around the budget enough to count here\n"
            "- Second point that wraps around the budget enough to count here\n"
            "- Third point that wraps around the budget enough to count here\n"
            "- Fourth point that wraps around the budget enough to count here\n"
            "- Fifth point that wraps around the budget enough to count here\n"
        )
        with tempfile.TemporaryDirectory() as tmp:
            deck_dir = Path(tmp)
            _write_png_header(deck_dir / "flow.png", 3000, 300)  # 10:1
            deck = deck_dir / "deck.md"
            deck.write_text(body, encoding="utf-8")
            resolved = lint_deck(deck)
        # With the flat 7.0u charge (no deck_path), the same slide overflows.
        unresolved = lint_source(body)
        self.assertEqual(
            [f for f in resolved.errors if f.rule == "slide-content-overflow"],
            [],
            "wide PNG measured → slide fits",
        )
        self.assertGreaterEqual(
            len([f for f in unresolved.errors if f.rule == "slide-content-overflow"]),
            1,
            "flat 7.0u charge → same slide overflows",
        )

    def test_deck_path_none_falls_back_to_flat(self) -> None:
        """AC2 — ``deck_path=None`` keeps the legacy flat 7.0u charge."""
        from anvil.lib.marp_lint import Geometry, _image_cost_units

        units, label = _image_cost_units(
            "", Geometry(), image_path="wide.png", deck_path=None
        )
        self.assertEqual(units, Geometry().image_units)
        self.assertEqual(label, "image")

    def test_missing_file_falls_back_to_flat(self) -> None:
        """AC2/AC3 — an unresolvable image path falls back to 7.0u."""
        from anvil.lib.marp_lint import Geometry, _image_cost_units

        with tempfile.TemporaryDirectory() as tmp:
            units, _ = _image_cost_units(
                "",
                Geometry(),
                image_path="does-not-exist.png",
                deck_path=Path(tmp) / "deck.md",
            )
        self.assertEqual(units, Geometry().image_units)

    def test_corrupt_png_falls_back_to_flat(self) -> None:
        """AC3 — a file with a bad signature falls back without raising."""
        from anvil.lib.marp_lint import Geometry, _image_cost_units

        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / "bad.png"
            bad.write_bytes(b"this is not a valid png header at all really")
            units, _ = _image_cost_units(
                "", Geometry(), image_path="bad.png", deck_path=Path(tmp) / "deck.md"
            )
        self.assertEqual(units, Geometry().image_units)

    def test_jpeg_falls_back_to_flat(self) -> None:
        """AC3 — a JPEG (no parser implemented) falls back to 7.0u."""
        from anvil.lib.marp_lint import Geometry, _image_cost_units

        with tempfile.TemporaryDirectory() as tmp:
            jpg = Path(tmp) / "photo.jpg"
            # JPEG SOI marker + junk — not a PNG signature.
            jpg.write_bytes(b"\xff\xd8\xff\xe0\x00\x10JFIF" + b"\x00" * 32)
            units, _ = _image_cost_units(
                "", Geometry(), image_path="photo.jpg", deck_path=Path(tmp) / "deck.md"
            )
        self.assertEqual(units, Geometry().image_units)

    def test_url_scheme_falls_back_to_flat(self) -> None:
        """Edge case — an ``https://`` image reference is not resolved."""
        from anvil.lib.marp_lint import Geometry, _image_cost_units

        with tempfile.TemporaryDirectory() as tmp:
            units, _ = _image_cost_units(
                "",
                Geometry(),
                image_path="https://example.com/wide.png",
                deck_path=Path(tmp) / "deck.md",
            )
        self.assertEqual(units, Geometry().image_units)

    def test_zero_dimension_png_falls_back(self) -> None:
        """Edge case — a PNG whose IHDR declares 0 width guards division."""
        from anvil.lib.marp_lint import Geometry, _image_cost_units

        with tempfile.TemporaryDirectory() as tmp:
            _write_png_header(Path(tmp) / "z.png", 0, 0)
            units, label = _image_cost_units(
                "", Geometry(), image_path="z.png", deck_path=Path(tmp) / "deck.md"
            )
        self.assertEqual(units, Geometry().image_units)
        self.assertEqual(label, "image")

    def test_path_traversal_reference_resolves(self) -> None:
        """Edge case — a ``../figures/fig.png`` reference resolves upward."""
        from anvil.lib.marp_lint import Geometry, _image_cost_units

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "figures").mkdir()
            (root / "thread").mkdir()
            _write_png_header(root / "figures" / "fig.png", 3096, 684)
            units, label = _image_cost_units(
                "",
                Geometry(),
                image_path="../figures/fig.png",
                deck_path=root / "thread" / "deck.md",
            )
        self.assertAlmostEqual(units, 6.45, places=1)


class TestFlexContainerCostHalving(unittest.TestCase):
    """Issue #622 — a frontmatter-CSS flex/grid container is cost-halved.

    A ``<div class="two-col">`` whose ``two-col`` class is defined
    ``display:flex`` in the frontmatter ``style:`` block renders its children
    side-by-side. The sequential capacity model stacks them and over-counts;
    post-#622 the container's inner cost is scaled by
    ``flex_container_cost_factor`` (default 0.5). This eliminated the
    seed-deck.1 slides-1-and-13 false positives.
    """

    def test_flex_fixture_no_overflow(self) -> None:
        """AC4 — the two-column fixture produces no overflow finding."""
        result = lint_deck(_FIXTURES / "flex_container_two_col.md")
        overflow = [
            f for f in result.errors + result.warnings
            if f.rule == "slide-content-overflow"
        ]
        self.assertEqual(overflow, [])

    def test_same_content_without_flex_class_overflows(self) -> None:
        """The halving is load-bearing: drop the flex class → the slide errors.

        Same body, but the ``.two-col`` ruleset no longer declares
        ``display:flex`` — the container is invisible to the heuristic and
        the stacked columns blow the budget.
        """
        source = (_FIXTURES / "flex_container_two_col.md").read_text(
            encoding="utf-8"
        )
        no_flex = source.replace("display: flex; ", "")
        result = lint_source(no_flex)
        overflow = [
            f for f in result.errors if f.rule == "slide-content-overflow"
        ]
        self.assertGreaterEqual(len(overflow), 1)

    def test_grid_display_also_halves(self) -> None:
        """A ``display:grid`` class is treated the same as ``display:flex``."""
        source = """---
marp: true
size: 16:9
html: true
style: |
  .cols { display: grid; grid-template-columns: 1fr 1fr; }
---

## Grid overview

<div class="cols">

### Left

- Alpha bullet on the left column that is reasonably wordy here
- Beta bullet on the left column that is reasonably wordy here
- Gamma bullet on the left column that is reasonably wordy here
- Delta bullet on the left column that is reasonably wordy here

### Right

- Alpha bullet on the right column that is reasonably wordy here
- Beta bullet on the right column that is reasonably wordy here
- Gamma bullet on the right column that is reasonably wordy here
- Delta bullet on the right column that is reasonably wordy here

</div>
"""
        result = lint_source(source)
        overflow = [
            f for f in result.errors if f.rule == "slide-content-overflow"
        ]
        self.assertEqual(overflow, [])

    def test_flex_escape_hatch_still_downgrades(self) -> None:
        """AC5 — the escape hatch still downgrades a flex slide that overflows.

        A flex container whose halved cost STILL exceeds the budget, with the
        per-slide ``anvil-lint-disable`` directive, must downgrade to ``info``.
        """
        # Build a flex container so dense that even 0.5× overflows, then
        # suppress it.
        big_cols = "\n".join(
            f"- Bullet number {k} carrying enough words to wrap the budget line here"
            for k in range(20)
        )
        source = f"""---
marp: true
size: 16:9
html: true
style: |
  .two-col {{ display: flex; }}
---

<!-- anvil-lint-disable: slide-content-overflow -->

## Dense flex

<div class="two-col">

{big_cols}

</div>
"""
        result = lint_source(source)
        self.assertEqual(
            [f for f in result.errors if f.rule == "slide-content-overflow"], []
        )
        infos = [f for f in result.infos if f.rule == "slide-content-overflow"]
        self.assertEqual(len(infos), 1)
        self.assertEqual(infos[0].severity, "info")


class TestFlexContainerCostFactorGeometry(unittest.TestCase):
    """Issue #622 — the new ``Geometry.flex_container_cost_factor`` field."""

    def test_default_factor(self) -> None:
        from anvil.lib.marp_lint import Geometry
        self.assertEqual(Geometry().flex_container_cost_factor, 0.5)

    def test_custom_factor_applied(self) -> None:
        """A lower factor charges even less of the container's stacked cost."""
        from anvil.lib.marp_lint import Geometry
        source = (_FIXTURES / "flex_container_two_col.md").read_text(
            encoding="utf-8"
        )
        # A factor of 0.0 means the container contributes nothing — definitely
        # no overflow.
        result = lint_source(source, geometry=Geometry(flex_container_cost_factor=0.0))
        self.assertEqual(
            [f for f in result.errors if f.rule == "slide-content-overflow"], []
        )


class TestLintSourceBackwardsCompatible(unittest.TestCase):
    """AC8 — ``lint_source`` behavior is byte-identical for keyword/no-image inputs."""

    def test_keyword_annotated_images_unchanged_without_deck_path(self) -> None:
        source = """---
marp: true
size: 16:9
---

# Hero

![bg right:55%](figures/hero.png)

![h:230px](figures/clamped.png)

- One bullet
"""
        # deck_path present vs absent must not change a keyword-only deck.
        with tempfile.TemporaryDirectory() as tmp:
            deck = Path(tmp) / "deck.md"
            deck.write_text(source, encoding="utf-8")
            with_path = lint_deck(deck)
        without_path = lint_source(source)
        self.assertEqual(
            [f.to_dict() for f in with_path.errors],
            [f.to_dict() for f in without_path.errors],
        )
        self.assertEqual(
            [f.to_dict() for f in with_path.warnings],
            [f.to_dict() for f in without_path.warnings],
        )


if __name__ == "__main__":
    unittest.main()
