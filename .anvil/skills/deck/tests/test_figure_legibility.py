"""Tests for ``anvil.skills.deck.lib.figure_legibility``.

The gate flags figures whose embedded text falls below the projection
legibility floor *as displayed on the slide* (after the CSS
``max-height: 75vh`` clamp or an explicit ``h:NNNpx`` Marp keyword).
v1 is the no-deps heuristic (per the issue #563 curator's plan): read
the PNG's intrinsic dimensions from the IHDR chunk via stdlib
``struct.unpack`` and approximate displayed glyph height from a per-
diagram-type intrinsic font-size lookup.

Test matrix (per the curator's plan):

- ``thin_strip_default_clamp`` — 800x80 mermaid PNG referenced without
  any ``h:`` keyword. Falls back to the CSS ``max-height: 75vh`` clamp
  (~540 px). Aspect 10:1 means it's width-limited: displayed height
  = 1280 * (80/800) = 128 px. Displayed glyph height
  = 18 * (128/80) = 28.8 px → ABOVE the warning threshold. NOT flagged.

  This is a more important test than the raw goodboy.1 ratio: the
  width-limited case shows the gate correctly skips a thin strip that
  still produces legible glyphs once isotropic scaling kicks in.

- ``thin_strip_with_explicit_h_clamp`` — 800x80 mermaid PNG referenced
  with ``h:80px`` Marp keyword. Explicit clamp kicks in at 80 px.
  Displayed glyph height = 18 * (80/80) * (intrinsic_h / displayed_h)
  → at h:80px the displayed height equals the intrinsic height so the
  scale ratio is 1.0 and displayed glyph = 18 px ABOVE the threshold.

- ``thin_strip_severe_clamp`` — A genuinely illegible case: a tall
  intrinsic figure (200x1200) clamped via ``h:80px``. Displayed glyph
  = 18 * (80/1200) = 1.2 px → ERROR.

- ``tb_oriented_no_keyword`` — A reasonably-shaped 800x600 TB
  mermaid PNG, no ``h:`` keyword. Displayed height clamps to the
  CSS default 540 px → glyph 16.2 px ABOVE warning. NOT flagged.

- ``goodboy_raas_flywheel_repro`` — The canary fixture cited in the
  issue body: a 784x102 mermaid PNG (LR cycle that rendered as a thin
  strip). Width-limited displayed height = 1280 * (102/784) = 166 px.
  Displayed glyph height = 18 * (166/102) = 29.3 px. ABOVE warning.
  This is the BEFORE-#545 case; the figure now reads fine BECAUSE
  width-fill produces a tall enough display height. The legibility
  gate is NOT a substitute for the render-side aspect/orient fix.

- ``suppressed_via_escape_hatch`` — A genuinely illegible figure with
  ``<!-- anvil-figure-legibility-disable: <name> -->`` on the slide.
  Severity downgrades to ``info``.

- ``missing_figure_silently_skipped`` — A reference to a non-existent
  PNG. Gate skips silently (handled by step 6 reference validation,
  not this gate).

- ``matplotlib_chart_passes`` — A reasonably-shaped matplotlib chart
  (1200x800) with no clamp. Treated as ``matplotlib`` diagram type
  with 14 px intrinsic font. Displayed height clamps to 540 px;
  glyph 14 * (540/800) = 9.45 px → ERROR. Demonstrates the gate is
  not mermaid-specific.

- ``escape_hatch_whole_slide`` — Bare ``<!-- anvil-figure-legibility-
  disable -->`` (no name) suppresses every figure on that slide.

Runs under either ``python -m unittest discover anvil/skills/deck/tests/``
or ``pytest anvil/skills/deck/tests/``.
"""

from __future__ import annotations

import struct
import tempfile
import unittest
import zlib
from pathlib import Path

from anvil.skills.deck.lib.figure_legibility import (
    Geometry,
    lint_figures,
)


# ---------------------------------------------------------------------------
# Test helpers — minimal PNG synthesis
# ---------------------------------------------------------------------------


def _make_minimal_png(width: int, height: int) -> bytes:
    """Build a valid PNG of the requested dimensions, no Pillow required.

    The gate reads only the IHDR chunk (bytes 16-24); the image data
    is irrelevant. We synthesize a single-colour image as cheaply as
    possible: one scanline of zero filter + N white RGB triples,
    repeated for ``height`` rows, zlib-compressed.

    Mirrors the ``_make_tiny_png`` helper in test_imagegen.py
    (parameterized on dimensions).
    """
    sig = b"\x89PNG\r\n\x1a\n"

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    # IHDR: width, height, bit depth 8, colour type 2 (RGB), 0/0/0.
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)

    # One scanline = filter byte (0) + W * 3 bytes RGB. All white.
    scanline = b"\x00" + b"\xff" * (width * 3)
    raw = scanline * height
    idat = zlib.compress(raw)

    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


def _write_deck(
    tmp_path: Path,
    figure_refs: list[str],
    *,
    suppress_directive: str | None = None,
) -> Path:
    """Write a minimal deck.md with the given figure references.

    Each entry in ``figure_refs`` is the literal markdown after the
    standalone-image-line convention, e.g. ``![alt](figures/foo.png)``.
    """
    lines = [
        "---",
        "marp: true",
        "size: 16:9",
        "theme: anvil-deck",
        "---",
        "",
    ]
    if suppress_directive:
        lines.append(suppress_directive)
        lines.append("")
    lines.append("# Slide 1")
    lines.append("")
    for ref in figure_refs:
        lines.append(ref)
        lines.append("")
    deck = tmp_path / "deck.md"
    deck.write_text("\n".join(lines), encoding="utf-8")
    return deck


def _make_figure(
    tmp_path: Path,
    name: str,
    width: int,
    height: int,
    *,
    diagram_type: str = "mermaid",
) -> Path:
    """Create a figures/<name>.png plus a sibling src/<name>.<ext> for type."""
    figures = tmp_path / "figures"
    figures.mkdir(exist_ok=True)
    src = figures / "src"
    src.mkdir(exist_ok=True)

    png_path = figures / f"{name}.png"
    png_path.write_bytes(_make_minimal_png(width, height))

    # Sibling source for diagram-type classification.
    if diagram_type == "mermaid":
        (src / f"{name}.mmd").write_text("flowchart LR\nA --> B\n", encoding="utf-8")
    elif diagram_type == "matplotlib":
        (src / f"{name}.py").write_text("import matplotlib\n", encoding="utf-8")
    # 'unknown' = no sibling source.

    return png_path


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestThinStripDefaultClamp(unittest.TestCase):
    """800x80 mermaid PNG, no h: keyword. Width-limited; passes.

    The gate must NOT fire on a thin strip that's saved by width-fill
    scaling. This case is the "width-limited" branch of the
    displayed-height computation.
    """

    def test_no_findings_when_width_limited_scaling_makes_text_legible(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _make_figure(root, "thin_strip", 800, 80, diagram_type="mermaid")
            deck = _write_deck(root, ["![alt](figures/thin_strip.png)"])

            result = lint_figures(deck)

            self.assertEqual(len(result.errors), 0, result.to_summary())
            self.assertEqual(len(result.warnings), 0, result.to_summary())


class TestThinStripSevereClamp(unittest.TestCase):
    """Tall figure clamped via h:80px. Drives displayed glyph well under 11 px.

    A 200x1200 mermaid PNG (height-limited because aspect is tall) with
    an explicit ``h:80px`` clamp. Scale ratio = 80/1200 = 0.067;
    displayed glyph = 18 * 0.067 = 1.2 px → ERROR.
    """

    def test_emits_error_with_correct_rule_and_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _make_figure(root, "tall_clamped", 200, 1200, diagram_type="mermaid")
            deck = _write_deck(
                root, ["![h:80px alt](figures/tall_clamped.png)"]
            )

            result = lint_figures(deck)

            self.assertEqual(len(result.errors), 1, result.to_summary())
            self.assertEqual(len(result.warnings), 0, result.to_summary())
            finding = result.errors[0]
            self.assertEqual(finding.rule, "figure-legibility-floor")
            self.assertEqual(finding.severity, "error")
            self.assertEqual(finding.slide, 1)
            self.assertIn("figures/tall_clamped.png", finding.message)


class TestTbOrientedNoKeyword(unittest.TestCase):
    """A reasonable 800x600 TB mermaid PNG with no h: keyword. Passes.

    Per AC2 on the issue: a TB-aspect figure with no override emits
    zero findings.
    """

    def test_no_findings_on_well_formed_figure(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _make_figure(root, "tb_diagram", 800, 600, diagram_type="mermaid")
            deck = _write_deck(root, ["![alt](figures/tb_diagram.png)"])

            result = lint_figures(deck)

            self.assertEqual(len(result.errors), 0)
            self.assertEqual(len(result.warnings), 0)
            self.assertEqual(len(result.infos), 0)


class TestGoodboyRaasFlywheelRepro(unittest.TestCase):
    """The canary fixture: 784x102 mermaid PNG (the cited goodboy.1 case).

    Width-limited displayed height = 1280 * (102/784) ≈ 166 px.
    Displayed glyph height = 18 * (166/102) ≈ 29.3 px → ABOVE warning.

    This is the BEFORE-#545 case; the figure now reads fine BECAUSE the
    width-fill produces a tall enough display height. The legibility
    gate is NOT a substitute for the render-side aspect/orient fix —
    that's #545. This test pins the boundary so a regression that
    makes the gate over-fire on width-limited thin strips is caught.
    """

    def test_width_limited_thin_strip_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _make_figure(root, "raas_flywheel", 784, 102, diagram_type="mermaid")
            deck = _write_deck(root, ["![alt](figures/raas_flywheel.png)"])

            result = lint_figures(deck)

            self.assertEqual(len(result.errors), 0, result.to_summary())
            self.assertEqual(len(result.warnings), 0, result.to_summary())


class TestExplicitHClampOnThinStripPushesUnderFloor(unittest.TestCase):
    """A thin-strip figure with an aggressive ``h:`` keyword IS flagged.

    784x102 mermaid PNG referenced with ``h:60px`` Marp keyword.
    Clamped displayed height = 60 px; scale = 60/102 ≈ 0.588;
    displayed glyph = 18 * 0.588 ≈ 10.6 px → ERROR.
    """

    def test_h_clamp_drives_under_error_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _make_figure(root, "thin_clamped", 784, 102, diagram_type="mermaid")
            deck = _write_deck(
                root, ["![h:60px alt](figures/thin_clamped.png)"]
            )

            result = lint_figures(deck)

            self.assertEqual(len(result.errors), 1, result.to_summary())
            self.assertEqual(result.errors[0].rule, "figure-legibility-floor")


class TestSuppressedViaEscapeHatch(unittest.TestCase):
    """Per-figure ``anvil-figure-legibility-disable: <name>`` downgrades to info."""

    def test_named_suppression_downgrades_to_info(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _make_figure(root, "tall_clamped", 200, 1200, diagram_type="mermaid")
            deck = _write_deck(
                root,
                ["![h:80px alt](figures/tall_clamped.png)"],
                suppress_directive="<!-- anvil-figure-legibility-disable: tall_clamped -->",
            )

            result = lint_figures(deck)

            self.assertEqual(len(result.errors), 0)
            self.assertEqual(len(result.warnings), 0)
            self.assertEqual(len(result.infos), 1)
            self.assertEqual(result.infos[0].severity, "info")
            self.assertEqual(result.infos[0].rule, "figure-legibility-floor")


class TestEscapeHatchWholeSlide(unittest.TestCase):
    """Bare ``anvil-figure-legibility-disable`` suppresses every figure on the slide."""

    def test_bare_directive_suppresses_all(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _make_figure(root, "f1", 200, 1200, diagram_type="mermaid")
            _make_figure(root, "f2", 200, 1200, diagram_type="mermaid")
            deck = _write_deck(
                root,
                [
                    "![h:80px alt](figures/f1.png)",
                    "![h:80px alt](figures/f2.png)",
                ],
                suppress_directive="<!-- anvil-figure-legibility-disable -->",
            )

            result = lint_figures(deck)

            self.assertEqual(len(result.errors), 0)
            self.assertEqual(len(result.warnings), 0)
            self.assertEqual(len(result.infos), 2)


class TestMissingFigureSilentlySkipped(unittest.TestCase):
    """A referenced-but-missing PNG is silently skipped by this gate.

    Missing-file handling is owned by step 6 of ``deck-figures``
    (reference validation). The legibility gate must skip cleanly so
    it doesn't double-fire on missing files.
    """

    def test_missing_file_yields_no_findings(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            # Note: no figure created at this path
            deck = _write_deck(root, ["![alt](figures/does_not_exist.png)"])

            result = lint_figures(deck)

            self.assertEqual(len(result.errors), 0)
            self.assertEqual(len(result.warnings), 0)
            self.assertEqual(len(result.infos), 0)


class TestMatplotlibChartNotMermaidSpecial(unittest.TestCase):
    """The gate is diagram-type-aware but not mermaid-only.

    A 1200x800 matplotlib chart (14 px intrinsic font, the matplotlib
    default axis-label height) referenced with no clamp displays at
    540 px (the CSS default cap). Scale = 540/800 = 0.675;
    displayed glyph = 14 * 0.675 ≈ 9.45 px → ERROR.
    """

    def test_matplotlib_chart_under_floor_emits_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _make_figure(root, "chart", 1200, 800, diagram_type="matplotlib")
            deck = _write_deck(root, ["![alt](figures/chart.png)"])

            result = lint_figures(deck)

            self.assertEqual(len(result.errors), 1, result.to_summary())
            self.assertEqual(result.errors[0].rule, "figure-legibility-floor")
            self.assertIn("matplotlib", result.errors[0].message)


class TestWorstCaseAcrossSlides(unittest.TestCase):
    """A figure referenced from N slides with different clamps reports the worst.

    Per the curator's edge-case note: 'Figure referenced from multiple
    slides with different `h:` overrides → check against the smallest
    display height (worst-case).'
    """

    def test_worst_case_clamp_picked(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _make_figure(root, "f", 200, 1200, diagram_type="mermaid")

            # Two slides referencing the same figure with different h: clamps.
            deck = root / "deck.md"
            deck.write_text(
                "\n".join(
                    [
                        "---",
                        "marp: true",
                        "theme: anvil-deck",
                        "---",
                        "",
                        "# Slide 1",
                        "",
                        "![h:400px alt](figures/f.png)",
                        "",
                        "---",
                        "",
                        "# Slide 2",
                        "",
                        "![h:80px alt](figures/f.png)",
                        "",
                    ]
                ),
                encoding="utf-8",
            )

            result = lint_figures(deck)

            # The h:80px on slide 2 is the worst case → that's what we report.
            self.assertEqual(len(result.errors), 1, result.to_summary())
            self.assertEqual(result.errors[0].slide, 2)


class TestNonPngSilentlySkipped(unittest.TestCase):
    """References to non-PNG files (e.g. SVG) are silently skipped."""

    def test_svg_reference_yields_no_findings(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            figures = root / "figures"
            figures.mkdir()
            # Write a file with a `.png` extension but non-PNG content
            # to exercise the IHDR-signature reject path.
            (figures / "fake.png").write_bytes(b"<svg>not a real png</svg>")
            deck = _write_deck(root, ["![alt](figures/fake.png)"])

            result = lint_figures(deck)

            self.assertEqual(len(result.errors), 0)
            self.assertEqual(len(result.warnings), 0)


class TestGeometryOverride(unittest.TestCase):
    """A consumer with a custom CSS cap can pass a Geometry override."""

    def test_tighter_max_height_drives_under_floor(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            # 200x1200 figure, no clamp.
            _make_figure(root, "tall", 200, 1200, diagram_type="mermaid")
            deck = _write_deck(root, ["![alt](figures/tall.png)"])

            # Default cap is 75vh = 540 px; with that cap, displayed glyph
            # ≈ 18 * (540/1200) = 8.1 px → already an error.
            # Verify by overriding to a *generous* cap (>= 1200) so the
            # default-default case becomes legible.
            result = lint_figures(deck)
            self.assertEqual(len(result.errors), 1)

            # Now widen the cap: img_max_height_vh = 100 vh = 720 px, but
            # the figure is taller than that → still clamped to 720 px,
            # displayed glyph = 18 * (720/1200) = 10.8 px → still an
            # error, just barely. To make this *pass*, widen the slide
            # height geometry itself to e.g. 1440 px:
            geo = Geometry(slide_height_px=1440)
            # Now the 75vh cap = 1080 px; the figure's intrinsic 1200 px
            # height means it's height-limited to 1080. Glyph = 18 *
            # (1080/1200) = 16.2 px → ABOVE warning.
            result_wide = lint_figures(deck, geometry=geo)
            self.assertEqual(len(result_wide.errors), 0)
            self.assertEqual(len(result_wide.warnings), 0)


if __name__ == "__main__":
    unittest.main()
