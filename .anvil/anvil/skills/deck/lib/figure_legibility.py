"""Figure legibility-at-display-size gate for the deck skill.

A figure can be present and well-formed yet illegible at the size it is
displayed on the slide. The render flag (issue #545) catches the
intrinsic-aspect failure (thin LR strip rendered ~1600x100); this gate
catches the orthogonal failure: a correctly-rendered figure whose embedded
text falls below the projection-legibility floor once the slide's
``max-height`` clamp (or an explicit ``h:NNNpx`` Marp keyword) scales it
down.

Mechanical, cheap, deterministic — fires *before* the expensive
``deck-design`` VLM critic per the framework's
"deterministic pre-flight before judgment" principle (precedent:
``marp_lint.slide-content-overflow`` and ``auto_shrink_detector``).

Design
------

For each ``![alt](figures/<name>.png)`` reference in ``deck.md``:

1. Resolve the intrinsic PNG dimensions from the IHDR chunk (stdlib
   ``struct.unpack`` — no Pillow). Mirrors the precedent in
   ``anvil/lib/render_gate.py::_read_png_dimensions``.
2. Compute the displayed height ``H_disp`` on the slide:

   - If the reference carries a Marp ``h:NNNpx`` (or ``h:NNN``) size
     keyword, use that as the explicit clamp.
   - Otherwise, fall back to the CSS ``max-height`` cap from the deck
     theme — currently ``75vh`` on a 720 px slide ≈ 540 px (raised from
     60vh in issue #545).
   - The actual displayed height is then
     ``min(H_disp_clamp, intrinsic_h * (slide_width / intrinsic_w))``
     so a wide-and-thin figure that is width-limited rather than
     height-limited scales down by the width ratio.

3. Estimate the displayed text-glyph height. For mermaid PNGs produced
   by our pinned theme, the source font is 18 px tall (after issue #563
   B.2). The displayed glyph height is
   ``intrinsic_text_h * (H_disp / intrinsic_png_h)``.

4. Threshold: ``< 14 px`` displayed → warning; ``< 11 px`` displayed →
   error. The "11 pt at projection scale" rule of thumb: at 1280x720
   Marp output, body text is 26 px ≈ 14.6 px at 96 DPI ≈ 11 pt. Any
   glyph rendered below that floor as displayed on the slide is the
   legibility breach.

5. Escape hatch: ``<!-- anvil-figure-legibility-disable: <name> -->``
   suppresses the rule for one figure (downgrades the finding to
   ``info`` so the reviser still sees it). Mirrors
   ``anvil-lint-disable: slide-content-overflow``. The bare directive
   ``<!-- anvil-figure-legibility-disable -->`` (no name) suppresses
   the rule for every figure on that slide.

Escalation hooks (NOT shipped in v1)
------------------------------------

This module ships **Option 1** from the curator's plan: a no-deps
heuristic. The intrinsic text height is approximated from the diagram
type, not measured. False positives on diagrams with intentionally tiny
labels are mitigated via the escape hatch.

If empirical false-positive rate is unacceptable, the curator's plan
documents two escalations, both kept out of v1:

- **Option 2** — Pillow + numpy connected-component text-height
  histogram. Would declare a new ``[legibility_lint]`` extra alongside
  the existing ``[image_lint]`` / ``[auto_shrink]`` extras
  (``pyproject.toml`` lines 105-127). Reuses the corner-sampling
  pattern from ``auto_shrink_detector``. The public API
  (``lint_figures(deck_md, figures_dir, geometry=...)``) does not
  change.
- **Option 3** — Tesseract OCR subprocess. Defers indefinitely;
  documented but not recommended.

The intrinsic-text-height-by-diagram-type knob
(``_INTRINSIC_TEXT_HEIGHT_PX_BY_DIAGRAM_TYPE`` below) is the seam where
a future image-measurement implementation would plug in: it would
replace the type-based lookup with the measured median bounding-box
height.

Why skill-local
---------------

This is the first consumer. Per the anvil skill-local-first convention
(``CLAUDE.md`` "Working on this repo"), new primitives ship under
``anvil/skills/<skill>/lib/`` until a second consumer materializes; the
``marp_lint.py`` lift (#318) is the precedent. When the slides skill
(or a future skill) adopts this gate, lift to
``anvil/lib/figure_legibility.py`` is mechanical.

Public API
----------

``lint_figures(deck_md_path: Path, figures_dir: Path,
               geometry: Geometry | None = None) -> LintResult``
    Run the gate against a deck.md plus a figures/ directory.
"""

from __future__ import annotations

import re
import struct
from dataclasses import dataclass
from pathlib import Path

from anvil.lib.marp_lint import Finding, LintResult


# Module-level metadata --------------------------------------------------------

#: Rule identifier emitted by this gate.
RULE_ID: str = "figure-legibility-floor"


# Geometry model ---------------------------------------------------------------


@dataclass(frozen=True)
class Geometry:
    """Slide-display geometry + legibility constants.

    Defaults match ``anvil/skills/deck/assets/anvil-deck.css``: 1280x720
    16:9 slide, ``section img { max-height: 75vh; }``. The 75vh cap was
    raised from 60vh in issue #545 so taller portrait figures fit
    without operator-supplied ``h:`` keyword overrides.

    The ``intrinsic_text_h_px_by_diagram_type`` mapping is the
    type-based proxy v1 uses in lieu of true image measurement (see the
    module docstring "Escalation hooks"). The keys are heuristic
    classifications of the PNG source — ``mermaid`` is the only one
    that matters today (matplotlib's 200 DPI default produces text
    large enough that this gate would not fire on its own; the same
    threshold still applies and will catch e.g. a stretched-down chart).
    """

    # Slide geometry (mirrors the deck Marp config + CSS).
    slide_width_px: int = 1280
    slide_height_px: int = 720
    img_max_height_vh: float = 75.0  # `section img { max-height: 75vh }`

    # Legibility thresholds, in displayed px. The "11 pt at projection
    # scale" rule of thumb: 11 pt ≈ 14.6 px at 96 DPI ≈ ~55% of the
    # default body-text baseline (26 px). Glyphs displayed at less than
    # the warning threshold are likely illegible at projection scale.
    warning_threshold_px: float = 14.0
    error_threshold_px: float = 11.0

    # Intrinsic text height (in source PNG pixels) approximated per
    # diagram type. The mermaid value matches the post-#563 theme
    # `themeVariables.fontSize = "18px"` knob (Piece B.2). Default for
    # an unknown source type is conservative: assume 16 px (mermaid's
    # stock default) so we don't under-flag.
    intrinsic_text_h_px_by_diagram_type: tuple[tuple[str, float], ...] = (
        ("mermaid", 18.0),
        ("matplotlib", 14.0),  # matplotlib default axis-label font
        ("unknown", 16.0),
    )

    def intrinsic_text_h_for(self, diagram_type: str) -> float:
        """Return the heuristic intrinsic text-glyph height, in px."""
        for key, value in self.intrinsic_text_h_px_by_diagram_type:
            if key == diagram_type:
                return value
        for key, value in self.intrinsic_text_h_px_by_diagram_type:
            if key == "unknown":
                return value
        return 16.0


_DEFAULT_GEOMETRY = Geometry()


# PNG header parsing -----------------------------------------------------------
#
# Bytes 16-24 of a signature-verified PNG carry big-endian u32
# width/height (the IHDR chunk is mandated first by the PNG spec).
# Mirrors ``anvil/lib/render_gate.py::_read_png_dimensions``; promotion
# to a shared helper waits for the second consumer per #58 / #318
# convention.

_PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def _read_png_dimensions(data: bytes) -> tuple[int, int] | None:
    """Return ``(width, height)`` from a PNG IHDR, or ``None``."""
    if len(data) < 24 or not data.startswith(_PNG_SIGNATURE):
        return None
    if data[12:16] != b"IHDR":
        return None
    width, height = struct.unpack(">II", data[16:24])
    if width <= 0 or height <= 0:
        return None
    return (int(width), int(height))


# Deck-source parsing ----------------------------------------------------------
#
# Marp image-with-modifier syntax permits any number of size/background
# keywords inside the alt text, e.g.:
#
#   ![w:600px](figures/foo.png)
#   ![bg fit](assets/cover.png)
#   ![h:200px w:auto](figures/bar.png)
#   ![Some descriptive alt h:80px](figures/baz.png)
#
# The ``h:`` keyword can be ``Npx``, ``N`` (bare number == px), or
# ``auto`` (no clamp). Same shape for ``w:``. We only need the height
# clamp for legibility math.

_IMAGE_REF_RE = re.compile(r"!\[(?P<alt>[^\]]*)\]\((?P<path>[^)\s]+)(?:\s+\"[^\"]*\")?\)")

_H_KEYWORD_RE = re.compile(
    r"\bh:(?P<value>auto|\d+(?:\.\d+)?(?:px|%)?)\b",
    re.IGNORECASE,
)
_W_KEYWORD_RE = re.compile(
    r"\bw:(?P<value>auto|\d+(?:\.\d+)?(?:px|%)?)\b",
    re.IGNORECASE,
)

# Anvil escape-hatch directive. Two forms:
#   <!-- anvil-figure-legibility-disable -->                  (whole slide)
#   <!-- anvil-figure-legibility-disable: name1, name2 -->    (named figures)
# Names may be bare (``raas-flywheel``) or the figure stem; we match
# against the filename without extension. Comma-separated.
_FIGURE_DISABLE_RE = re.compile(
    r"<!--\s*anvil-figure-legibility-disable"
    r"(?::\s*(?P<names>[a-zA-Z0-9_,\-\s.]+?))?"
    r"\s*-->",
)

# Marp slide separator (mirrors marp_lint behaviour: a `---` on its own line).
_SLIDE_BREAK_RE = re.compile(r"^---\s*$")


def _parse_h_keyword(alt: str) -> float | None:
    """Return the ``h:`` clamp in px, or ``None`` if unset / ``auto``.

    Accepts ``h:200px``, ``h:200``, ``h:auto``. A bare integer is
    treated as px. A percentage (``h:50%``) is interpreted relative to
    the slide height — though Marp's own behaviour for ``h:%`` is
    poorly specified and consumer code generally uses absolute px.
    """
    m = _H_KEYWORD_RE.search(alt)
    if not m:
        return None
    value = m.group("value").lower()
    if value == "auto":
        return None
    if value.endswith("px"):
        try:
            return float(value[:-2])
        except ValueError:
            return None
    if value.endswith("%"):
        # Treated as percent of slide height; the caller knows the
        # slide height geometry.
        try:
            pct = float(value[:-1])
        except ValueError:
            return None
        return pct  # caller multiplies by slide_height_px / 100
    try:
        return float(value)
    except ValueError:
        return None


def _parse_w_keyword(alt: str) -> float | None:
    """Return the ``w:`` clamp in px, or ``None`` if unset / ``auto``."""
    m = _W_KEYWORD_RE.search(alt)
    if not m:
        return None
    value = m.group("value").lower()
    if value == "auto":
        return None
    if value.endswith("px"):
        try:
            return float(value[:-2])
        except ValueError:
            return None
    if value.endswith("%"):
        try:
            pct = float(value[:-1])
        except ValueError:
            return None
        return pct  # caller multiplies by slide_width_px / 100
    try:
        return float(value)
    except ValueError:
        return None


def _h_keyword_is_percent(alt: str) -> bool:
    m = _H_KEYWORD_RE.search(alt)
    if not m:
        return False
    return m.group("value").lower().endswith("%")


def _w_keyword_is_percent(alt: str) -> bool:
    m = _W_KEYWORD_RE.search(alt)
    if not m:
        return False
    return m.group("value").lower().endswith("%")


def _classify_diagram_type(figure_path: Path) -> str:
    """Best-effort diagram-type classification by sibling source file.

    The figurer (``deck-figures``) renders ``figures/src/<name>.mmd``
    into ``figures/<name>.png`` for mermaid and
    ``figures/src/<name>.py`` into ``figures/<name>.png`` for
    matplotlib. We use the sibling source file's extension to
    classify. If neither source exists (e.g., a hand-placed PNG), fall
    back to "unknown" — the conservative case.
    """
    stem = figure_path.stem
    src_dir = figure_path.parent / "src"
    if (src_dir / f"{stem}.mmd").exists():
        return "mermaid"
    if (src_dir / f"{stem}.py").exists():
        return "matplotlib"
    return "unknown"


# Slide model ------------------------------------------------------------------


@dataclass
class _ImageOccurrence:
    """One ``![alt](path)`` reference resolved on one slide."""

    slide: int
    line: int  # 1-based file-level line
    alt: str
    path: str  # raw path as written in deck.md (e.g. "figures/foo.png")
    h_clamp_px: float | None  # explicit height clamp, in px (None = use CSS default)
    w_clamp_px: float | None  # explicit width clamp, in px (None = slide width)
    suppressed: bool  # True if this slide has a disable directive matching this figure


def _split_slides_with_lines(source: str) -> list[tuple[int, list[tuple[int, str]]]]:
    """Split a deck source into ``(slide_index, [(line_no, line_text), ...])``.

    Mirrors the frontmatter / slide-break handling in
    ``anvil/lib/marp_lint._split_slides`` but preserves per-line offsets
    so we can attribute findings to source-file line numbers.
    """
    lines = source.splitlines()
    n = len(lines)

    # Frontmatter
    start_idx = 0
    if n > 0 and lines[0].strip() == "---":
        for j in range(1, n):
            if lines[j].strip() == "---":
                start_idx = j + 1
                break

    slides: list[tuple[int, list[tuple[int, str]]]] = []
    current: list[tuple[int, str]] = []
    slide_num = 0
    for idx in range(start_idx, n):
        line = lines[idx]
        if _SLIDE_BREAK_RE.match(line):
            if current:
                slide_num += 1
                slides.append((slide_num, current))
                current = []
            continue
        # 1-based file-level line number
        current.append((idx + 1, line))
    if current:
        # Only count non-empty slides
        if any(text.strip() for _, text in current):
            slide_num += 1
            slides.append((slide_num, current))

    return slides


def _collect_image_occurrences(
    source: str,
    geo: Geometry,
) -> list[_ImageOccurrence]:
    """Parse the deck.md source into per-image-reference records."""
    occurrences: list[_ImageOccurrence] = []
    slides = _split_slides_with_lines(source)
    for slide_index, slide_lines in slides:
        # Gather suppression directives across the slide first.
        suppress_all = False
        suppressed_names: set[str] = set()
        slide_text = "\n".join(text for _, text in slide_lines)
        for m in _FIGURE_DISABLE_RE.finditer(slide_text):
            names = m.group("names")
            if names is None:
                suppress_all = True
            else:
                for raw in names.split(","):
                    nm = raw.strip()
                    if nm:
                        suppressed_names.add(nm)

        # Walk lines and collect image references.
        for line_no, line in slide_lines:
            for m in _IMAGE_REF_RE.finditer(line):
                alt = m.group("alt")
                path = m.group("path")
                # Resolve h:/w: clamps. Percent values are deferred to
                # the caller so they translate against the right axis.
                h_val = _parse_h_keyword(alt)
                if h_val is not None and _h_keyword_is_percent(alt):
                    h_val = h_val * geo.slide_height_px / 100.0
                w_val = _parse_w_keyword(alt)
                if w_val is not None and _w_keyword_is_percent(alt):
                    w_val = w_val * geo.slide_width_px / 100.0

                fig_stem = Path(path).stem
                this_suppressed = suppress_all or (
                    fig_stem in suppressed_names
                    or path in suppressed_names
                )

                occurrences.append(
                    _ImageOccurrence(
                        slide=slide_index,
                        line=line_no,
                        alt=alt,
                        path=path,
                        h_clamp_px=h_val,
                        w_clamp_px=w_val,
                        suppressed=this_suppressed,
                    )
                )
    return occurrences


# Legibility math --------------------------------------------------------------


def _displayed_height_px(
    intrinsic_w: int,
    intrinsic_h: int,
    h_clamp_px: float | None,
    w_clamp_px: float | None,
    geo: Geometry,
) -> float:
    """Compute the figure's displayed height on the slide.

    Two clamps apply: a width clamp (``w:`` keyword, else the slide
    width minus padding) and a height clamp (``h:`` keyword, else the
    CSS ``max-height: 75vh`` default ≈ 540 px). The image preserves
    aspect ratio (``object-fit: contain`` per the deck CSS), so the
    displayed height is the lesser of the two clamps' implied heights.
    """
    # CSS default cap.
    css_default_h = geo.slide_height_px * (geo.img_max_height_vh / 100.0)
    effective_h_clamp = h_clamp_px if h_clamp_px is not None else css_default_h

    # Width clamp: if explicit, use it; otherwise the figure can occupy
    # up to the slide width. (The deck CSS sets max-width: 100%; padding
    # narrows that, but the legibility math is most pessimistic when we
    # assume the figure has the largest possible displayed width — the
    # rendered text is biggest then.)
    effective_w_clamp = w_clamp_px if w_clamp_px is not None else float(geo.slide_width_px)

    aspect = intrinsic_h / intrinsic_w  # height / width

    # Height-limited displayed height when scaling preserves aspect:
    h_from_h_clamp = effective_h_clamp
    h_from_w_clamp = effective_w_clamp * aspect

    return min(h_from_h_clamp, h_from_w_clamp)


def _displayed_text_height_px(
    intrinsic_w: int,
    intrinsic_h: int,
    displayed_h_px: float,
    diagram_type: str,
    geo: Geometry,
) -> float:
    """Estimate the displayed text-glyph height, in px.

    The scale ratio is ``displayed_h_px / intrinsic_h`` (the figure is
    scaled isotropically by ``object-fit: contain``), so the displayed
    glyph height is ``intrinsic_text_h * scale_ratio``.
    """
    if intrinsic_h <= 0:
        return 0.0
    scale = displayed_h_px / intrinsic_h
    return geo.intrinsic_text_h_for(diagram_type) * scale


# Public API -------------------------------------------------------------------


def lint_figures(
    deck_md_path: Path,
    figures_dir: Path | None = None,
    *,
    geometry: Geometry | None = None,
) -> LintResult:
    """Run the figure-legibility gate against a deck.md.

    Parameters
    ----------
    deck_md_path
        Path to the deck.md source. Image references inside the source
        are resolved relative to its parent directory unless they begin
        with an absolute path.
    figures_dir
        Optional override for the figures directory. Defaults to
        ``deck_md_path.parent / "figures"``. References that resolve to
        outside this directory (e.g. ``assets/...``) are still checked
        if the file exists on disk; the gate is content-agnostic.
    geometry
        Optional geometry override. Defaults to the shipped deck
        geometry (1280x720, ``max-height: 75vh``, mermaid 18 px source
        font).

    Returns
    -------
    LintResult
        Same shape as ``marp_lint.lint_deck``. Findings are emitted with
        ``rule="figure-legibility-floor"``. Severity is ``error`` below
        the error threshold (11 px displayed), ``warning`` below the
        warning threshold (14 px displayed), ``info`` when suppressed
        via the per-figure escape hatch (regardless of magnitude).

    Behaviour notes
    ---------------
    - Missing PNG files (referenced but absent) are silently skipped —
      that case is handled by step 6 of ``deck-figures`` (reference
      validation), not this gate.
    - Non-PNG references (e.g., SVG, JPG) are silently skipped — the
      IHDR-only reader returns ``None`` and the figure is not measured.
    - A single figure referenced from N slides yields one finding per
      *worst-case* slide (the smallest displayed height across all
      references). This matches the curator's "check against the
      smallest display height (worst-case)" guidance.
    """
    if not isinstance(deck_md_path, Path):
        deck_md_path = Path(deck_md_path)
    geo = geometry or _DEFAULT_GEOMETRY

    source = deck_md_path.read_text(encoding="utf-8")
    occurrences = _collect_image_occurrences(source, geo)

    # Per-figure worst-case selection: group by resolved path; pick the
    # occurrence with the smallest displayed text height.
    by_path: dict[str, _ImageOccurrence] = {}
    by_path_size: dict[str, tuple[int, int]] = {}
    by_path_display: dict[str, float] = {}
    by_path_text: dict[str, float] = {}
    by_path_type: dict[str, str] = {}

    figures_root = figures_dir if figures_dir is not None else deck_md_path.parent / "figures"

    for occ in occurrences:
        ref_path = occ.path
        # Resolve the figure on disk. The deck.md references are usually
        # relative paths like ``figures/<name>.png``; resolve relative to
        # deck_md_path.parent. An explicit ``figures_dir`` override
        # rewrites only the ``figures/...`` prefix.
        candidate = Path(ref_path)
        if not candidate.is_absolute():
            candidate = deck_md_path.parent / ref_path
        if not candidate.exists():
            # Try the override.
            stem = Path(ref_path).name
            override = figures_root / stem
            if override.exists():
                candidate = override
            else:
                continue  # missing file — handled elsewhere

        # Only PNGs are measurable today.
        try:
            data = candidate.read_bytes()
        except OSError:
            continue
        dims = _read_png_dimensions(data)
        if dims is None:
            continue
        intrinsic_w, intrinsic_h = dims

        diagram_type = _classify_diagram_type(candidate)
        displayed_h = _displayed_height_px(
            intrinsic_w, intrinsic_h, occ.h_clamp_px, occ.w_clamp_px, geo
        )
        displayed_text = _displayed_text_height_px(
            intrinsic_w, intrinsic_h, displayed_h, diagram_type, geo
        )

        # Worst-case across references: keep the occurrence with the
        # smallest displayed text height (most likely to flag). Tie-break
        # by first-seen.
        prior_text = by_path_text.get(ref_path)
        if prior_text is None or displayed_text < prior_text:
            by_path[ref_path] = occ
            by_path_size[ref_path] = (intrinsic_w, intrinsic_h)
            by_path_display[ref_path] = displayed_h
            by_path_text[ref_path] = displayed_text
            by_path_type[ref_path] = diagram_type

    result = LintResult()
    for ref_path, occ in by_path.items():
        intrinsic_w, intrinsic_h = by_path_size[ref_path]
        displayed_h = by_path_display[ref_path]
        displayed_text = by_path_text[ref_path]
        diagram_type = by_path_type[ref_path]

        if displayed_text >= geo.warning_threshold_px:
            continue

        # Severity: suppressed → info; below error threshold → error;
        # else warning.
        if occ.suppressed:
            severity = "info"
        elif displayed_text < geo.error_threshold_px:
            severity = "error"
        else:
            severity = "warning"

        intrinsic_text = geo.intrinsic_text_h_for(diagram_type)
        message = (
            f"Figure `{ref_path}` displays at ~{displayed_h:.0f} px tall "
            f"on slide {occ.slide} (intrinsic {intrinsic_w}x{intrinsic_h}, "
            f"diagram type: {diagram_type}). Estimated displayed glyph "
            f"height ~{displayed_text:.1f} px ({intrinsic_text:.0f} px "
            f"source font × scale {displayed_h/intrinsic_h:.3f}); "
            f"projection legibility floor is "
            f"{geo.error_threshold_px:.0f} px (error) / "
            f"{geo.warning_threshold_px:.0f} px (warning). "
            "Re-render at a more square aspect (e.g. `flowchart TB` for "
            "cyclic mermaid graphs), drop the explicit `h:NNNpx` keyword, "
            "or suppress with "
            f"`<!-- anvil-figure-legibility-disable: {Path(ref_path).stem} -->`."
        )

        finding = Finding(
            slide=occ.slide,
            line=occ.line,
            rule=RULE_ID,
            severity=severity,
            message=message,
        )
        if severity == "error":
            result.errors.append(finding)
        elif severity == "warning":
            result.warnings.append(finding)
        else:
            result.infos.append(finding)

    return result


__all__ = [
    "Finding",
    "Geometry",
    "LintResult",
    "RULE_ID",
    "lint_figures",
]
