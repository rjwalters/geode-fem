"""Deterministic post-render detector for Marp's silent CSS auto-shrink.

This module is the post-render companion to ``marp_lint.py``. Marp's
``slide-content-overflow`` behaviour is two-mode in practice:

1. **Loud overflow** — content clips against the safe area and the symptom
   is obvious to the author (text or images cut off). Already detected
   source-side by ``marp_lint.lint_deck`` via the capacity model.
2. **Silent auto-shrink** — Marp's CSS ``fit-to-frame`` rule kicks in
   instead of clipping, scaling the entire ``<section>`` down to fit the
   page. The slide compiles clean; the rendered PDF opens without errors;
   the author can't tell from the markdown source that the slide reads
   small. Two or three auto-shrunk slides in a deck read as "unfinished"
   even though every individual slide is technically intact.

This detector catches mode 2 by reading the rendered PDF directly. It is
NOT a static markdown check (`marp_lint.py` covers that surface, and its
module docstring is explicit about being source-side) and it is NOT a VLM
critic (`deck-vision.md`'s `v1 vertical_overflow` covers the qualitative
"does this look bad?" question with one API call per slide). It is
deterministic, pixel-level, and runs in ~50ms per page at 150 DPI.

How it works
------------

1. The deck-review command has already rendered ``deck.pdf`` to per-page
   PNGs via ``anvil.lib.render.render_pdf_to_pngs`` (the same pipeline
   ``deck-vision`` uses). The detector either reuses an existing PNG dir
   or calls into the same helper.
2. For each PNG, sample the background colour from four corner patches,
   threshold each pixel against that background, and compute the
   row-with-content / column-with-content extents via numpy. The vertical
   ``bottom_margin_norm = (slide_h - content_bottom_y) / slide_h`` is the
   discriminative signal: an auto-shrunk slide has an unusually large
   bottom margin compared to its peers.
3. Classify slides by ``<!-- _class: name -->`` directives in the
   ``deck.md`` source (default ``"content"``). Group bottom-margin
   measurements by class; flag any page whose
   ``bottom_margin_norm > 1.5 * median[class]`` AND
   ``bottom_margin_norm > 0.18`` (both conditions required — the ratio
   catches outliers vs. peers; the absolute floor prevents noise on decks
   whose peers all happen to have small bottom margins).
4. Singleton-class slides (typically: one ``title``, one ``ask``) are
   skipped with a recorded reason — too few peers for a meaningful
   median.

Why pixel-based
---------------

The empirical evaluation in issue #102 ruled out the three PDF-library
candidates:

- ``pypdf`` exposes only the slide *frame* (``mediabox``), which is
  invariant across pages; useless for auto-shrink. The visitor API would
  require rewriting PDF content-stream parsing inside Anvil.
- ``pdfplumber`` adds three transitive Python deps (``pdfminer.six``,
  ``cryptography``, ``charset-normalizer``) plus ``Pillow`` and a binary
  ``pypdfium2`` wheel — high cost for one helper.
- ``pypdfium2`` standalone is a binary wheel and we would still own the
  bbox-aggregation code.

The auto-shrink symptom *is* a pixel symptom (the whole slide content is
bounded into a smaller rectangle), so a pixel-level detector is the right
level of abstraction. The PNG pipeline already exists; this module reuses
it.

Why deck-local (not ``anvil/lib/``)
-----------------------------------

Marp's CSS ``fit-to-frame`` behaviour is structurally absent from LaTeX
skills — they emit overfull-box warnings instead of silently scaling, and
``anvil/lib/render_gate.py`` already catches those. Lifting this detector
into ``anvil/lib/`` would force LaTeX skills to optionally depend on
``Pillow``/``numpy`` for a check that can never fire there. The
``slides`` skill adopting this later follows the ``marp_lint.py``
precedent: ship skill-local first, then promote to ``anvil/lib/`` once a
second consumer materializes (per #318 for ``marp_lint``).

Wiring
------

Called from ``anvil/skills/deck/commands/deck-review.md`` (step 5c, right
after the step-5b ``marp_lint.lint_deck`` call). Findings join
``_summary.md``'s ``lint`` block under a new ``auto_shrink`` sub-key. Any
``severity="error"`` finding ORs into ``lint_critical_flag`` alongside
the source-side errors. Skipped (deps missing) is recorded as an
info-level note; the rest of the review proceeds.

Public API
----------

``detect_auto_shrink(deck_pdf, deck_md) -> AutoShrinkResult``
    The public entry point. Both arguments are paths. Returns an
    :class:`AutoShrinkResult` containing per-page findings plus the
    per-class baseline medians for transparency.
"""

from __future__ import annotations

import re
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


# --- thresholds --------------------------------------------------------------


@dataclass(frozen=True)
class Thresholds:
    """Tunable thresholds for the per-class peer-comparison rule.

    Mirrors ``anvil.lib.marp_lint.Geometry`` so consumers with
    a heavier-padding theme or a different aspect ratio can override
    without monkeypatching internals.

    Issue #562 promoted the detector from a single-signal rule
    (bottom-margin only) to a **two-of-three composite** rule. The three
    peer-relative signals — bottom margin, top margin, content-area —
    each have an outlier-ratio and an absolute-floor threshold; a page
    is flagged when at least two signals fire together. The two-of-three
    quorum keeps false positives low (a single legitimately sparser
    slide does not trigger) while catching fit-to-scale shrink even when
    a single signal (typically bottom margin) sits near the class
    median.
    """

    # --- bottom-margin signal (the original / pre-#562 single signal) ---

    # Outlier ratio: a page is flagged only when its bottom margin is more
    # than this multiple of the per-class median. 1.5 means "50% more
    # whitespace than peers" — empirically the threshold that catches the
    # canary auto-shrink cases without flagging slides that just happen to
    # be a bit lighter.
    median_ratio_threshold: float = 1.5

    # Absolute floor: even an outlier vs. peers is not flagged unless its
    # bottom margin exceeds this fraction of the slide height. 0.18 = 18%
    # of slide height. Prevents noise on decks where every peer slide
    # already has small bottom margins (an "outlier" there might still be
    # only 5% of slide height empty — not a real auto-shrink).
    abs_bottom_margin_floor: float = 0.18

    # --- top-margin signal (#562) ---

    # Outlier ratio for the top-margin signal. Marp fit-to-scale tends
    # to push the top down proportionally; same 1.5x ratio convention.
    top_margin_ratio_threshold: float = 1.5

    # Absolute floor for the top margin. Smaller than the bottom floor
    # (0.10 vs 0.18) because slides typically carry less whitespace at
    # the top — a 10% top margin is already a noticeable gap on a 16:9
    # safe area.
    abs_top_margin_floor: float = 0.10

    # --- content-area signal (#562) ---

    # Outlier ratio for the content-area-norm signal. Inverted vs the
    # other signals: a page is flagged when its content-area is SMALLER
    # than the class median by this factor or more. 0.75 means
    # "shrunk to 75% or less of peer-class content area" — empirically
    # the threshold that fires on the GoodBoy slides 3/8 fit-shrink mode
    # without tripping on slides that just happen to have less content.
    content_area_ratio_threshold: float = 0.75

    # --- composite rule ---

    # Number of signals (out of 3) that must fire for a page to be
    # flagged. Two-of-three is the design contract (issue #562): a
    # single legitimately sparser slide does not trigger; a
    # fit-to-scale-shrunk slide trips ≥2 signals together.
    composite_signals_required: int = 2

    # --- pixel-detection knobs (unchanged from pre-#562) ---

    # Minimum number of pages per class required to compute a baseline
    # median. Classes with fewer pages are skipped (recorded with a
    # reason; never flagged) — a single ``title`` or ``ask`` slide has no
    # peers to compare against.
    min_peers_per_class: int = 3

    # Per-channel tolerance for "this pixel is background". 8 = the pixel
    # must differ from the corner-sampled background by more than 8/255 on
    # any channel to count as content. Tight enough to catch faint text;
    # loose enough to ignore JPEG-style PNG compression noise.
    bg_tolerance: int = 8

    # Margin (in pixels) shaved off each edge before sampling the corner
    # background patches. Avoids picking up theme-edge pixels (e.g., a
    # thin border line) as background. 4px at 150 DPI 1280x720 is well
    # below any deliberate theme decoration width.
    corner_margin_px: int = 4

    # Size (square, in pixels) of each corner background-sampling patch.
    corner_patch_px: int = 16


_DEFAULT_THRESHOLDS = Thresholds()


# --- result types ------------------------------------------------------------


@dataclass
class ContentBbox:
    """The pixel extent of "content" rows/columns on one rendered slide.

    Three peer-relative shrink signals are exposed as properties:

    - :attr:`bottom_margin_norm` — fraction of slide height empty BELOW
      the lowest content row. Pre-#562 this was the only signal; it
      catches Marp's fit-to-frame when the shrunk content sits up near
      the top of the safe area.
    - :attr:`top_margin_norm` — fraction of slide height empty ABOVE the
      first content row. Catches the fit-shrunk-and-centred case the
      bottom-margin-alone rule missed (issue #562).
    - :attr:`content_area_norm` — ``(content_w × content_h) / (slide_w ×
      slide_h)``, the fraction of slide area filled by the content bbox.
      The strongest peer-relative signal of fit-to-scale shrink: when
      Marp scales the section, the bbox area drops measurably even when
      bottom-margin stays close to the class median.
    """

    top: int
    bottom: int  # inclusive
    left: int
    right: int  # inclusive
    width: int  # full PNG width
    height: int  # full PNG height

    @property
    def bottom_margin_norm(self) -> float:
        """Fraction of slide height that is empty below the lowest content row."""
        if self.height == 0:
            return 0.0
        return max(0.0, (self.height - 1 - self.bottom) / self.height)

    @property
    def top_margin_norm(self) -> float:
        """Fraction of slide height empty above the first content row.

        Issue #562: Marp's fit-to-frame scaling can leave a proportionally
        large gap at the TOP of the slide (when the content is vertically
        centred in the safe area or when the title scales down with the
        rest of the section). A top-margin peer-vs-median ratio catches
        the auto-shrink cases the bottom-margin-only rule misses.
        """
        if self.height == 0:
            return 0.0
        return max(0.0, self.top / self.height)

    @property
    def content_area_norm(self) -> float:
        """Fraction of slide area filled by the content bbox.

        Issue #562: ``(content_w × content_h) / (slide_w × slide_h)``.
        This is the strongest peer-relative shrink signal — when Marp
        scales the section, the bbox area shrinks proportionally even
        when bottom-margin or top-margin alone don't budge. A fit-shrunk
        slide with bottom-margin near the class median can still show
        clearly-reduced bbox area vs peers.
        """
        if self.height == 0 or self.width == 0:
            return 0.0
        content_h = max(0, self.bottom - self.top + 1)
        content_w = max(0, self.right - self.left + 1)
        return (content_h * content_w) / float(self.height * self.width)


@dataclass
class AutoShrinkFinding:
    """One per-slide finding from the detector.

    Shape mirrors the marp_lint ``Finding`` schema so the deck-review
    command can fold this into the same ``lint`` block without a separate
    serialiser. ``rule`` is always ``"auto-shrink-fit-compression"`` for
    consistency with the upstream-rule-naming convention used by
    marp_lint.

    Post-#562 the finding carries the full triplet of peer-relative
    signals (bottom margin, top margin, content area) so the reviser
    sees WHICH signals fired and the per-class medians for each. The
    legacy ``ratio`` field is preserved as the bottom-margin ratio for
    backwards compatibility with downstream readers; new consumers
    should read the per-signal fields directly.
    """

    slide: int
    class_name: str
    # --- bottom margin signal (pre-#562) ---
    bottom_margin_norm: float
    median_bottom_margin_norm: float
    ratio: float  # = bottom_margin_norm / median (legacy field)
    # --- composite signals (#562) ---
    top_margin_norm: float = 0.0
    median_top_margin_norm: float = 0.0
    content_area_norm: float = 0.0
    median_content_area_norm: float = 0.0
    signals_fired: tuple = ()  # subset of ("bottom_margin", "top_margin", "content_area")
    # --- shared fields ---
    severity: str = "error"  # reserved for future warning tier
    message: str = ""
    rule: str = "auto-shrink-fit-compression"

    def to_dict(self) -> dict:
        return {
            "slide": self.slide,
            "class_name": self.class_name,
            "bottom_margin_norm": round(self.bottom_margin_norm, 4),
            "median_bottom_margin_norm": round(
                self.median_bottom_margin_norm, 4
            ),
            "ratio": round(self.ratio, 3),
            "top_margin_norm": round(self.top_margin_norm, 4),
            "median_top_margin_norm": round(self.median_top_margin_norm, 4),
            "content_area_norm": round(self.content_area_norm, 4),
            "median_content_area_norm": round(
                self.median_content_area_norm, 4
            ),
            "signals_fired": list(self.signals_fired),
            "rule": self.rule,
            "severity": self.severity,
            "message": self.message,
        }


@dataclass
class AutoShrinkResult:
    """All findings + per-class baselines + the skipped/reason channel.

    ``skipped=True`` means the detector deliberately did NOT run for the
    whole deck — either ``Pillow``/``numpy`` were missing (graceful skip
    per the #65/#85 preflight pattern) or the input PDF was absent. In
    either case ``findings`` is empty and ``reason`` carries the
    explanation. Use the :data:`AUTO_SHRINK_REMEDIATION` constant from
    ``anvil.lib.render`` to surface install instructions when the skip is
    caused by missing deps.
    """

    findings: list[AutoShrinkFinding] = field(default_factory=list)
    skipped: bool = False
    reason: Optional[str] = None
    # Bottom-margin median per class (kept for backwards compatibility).
    per_class_medians: dict[str, float] = field(default_factory=dict)
    skipped_classes: dict[str, str] = field(default_factory=dict)
    # Post-#562: full median triplet per class. Each value is a dict with
    # keys ``bottom_margin``, ``top_margin``, ``content_area``. The legacy
    # ``per_class_medians`` field above is preserved (bottom_margin only)
    # so existing readers don't break; new consumers can read the
    # extended triplet directly.
    per_class_medians_extended: dict[str, dict[str, float]] = field(
        default_factory=dict
    )

    def to_dict(self) -> dict:
        return {
            "ran": not self.skipped,
            "skipped": self.skipped,
            "reason": self.reason,
            "errors": sum(
                1 for f in self.findings if f.severity == "error"
            ),
            "warnings": sum(
                1 for f in self.findings if f.severity == "warning"
            ),
            "infos": sum(
                1 for f in self.findings if f.severity == "info"
            ),
            "findings": [f.to_dict() for f in self.findings],
            "per_class_medians": {
                cls: round(v, 4)
                for cls, v in self.per_class_medians.items()
            },
            "per_class_medians_extended": {
                cls: {k: round(v, 4) for k, v in trip.items()}
                for cls, trip in self.per_class_medians_extended.items()
            },
            "skipped_classes": dict(self.skipped_classes),
        }


# --- slide classification ----------------------------------------------------

# Matches a per-slide Marp class directive. We accept any non-empty class
# name (Marp's only constraint is that it be a valid CSS class). The
# detector classifies a slide as ``content`` when no directive is present
# — matches the deck rubric assumption that any slide without an explicit
# class is a content slide.
_CLASS_DIRECTIVE_RE = re.compile(
    r"^\s*<!--\s*_class:\s*(?P<class>[A-Za-z][A-Za-z0-9_\-]*)\s*-->\s*$",
    re.MULTILINE,
)

# Slide separator (a bare ``---`` on its own line). Identical convention
# to ``marp_lint._SLIDE_BREAK_RE`` — we deliberately re-derive it locally
# rather than importing across modules so the two checks evolve
# independently.
_SLIDE_BREAK_RE = re.compile(r"^---\s*$", re.MULTILINE)


def _classify_slides(deck_md_source: str) -> list[str]:
    """Return one class name per slide (1-indexed), reading ``deck.md``.

    A slide's class is taken from the first ``<!-- _class: name -->`` HTML
    comment within the slide block; absent that, the class defaults to
    ``"content"``. Frontmatter (the leading YAML block bracketed by
    ``---``) is skipped — its ``class:`` field is the deck-wide default,
    not a per-slide override.

    Parameters
    ----------
    deck_md_source:
        Full contents of ``deck.md``.

    Returns
    -------
    A list where ``result[i]`` is the class for slide ``i+1``. The list
    length equals the number of non-empty slides found in the source
    (matches the same effective-empty pruning ``marp_lint._split_slides``
    applies).
    """
    lines = deck_md_source.splitlines()
    n = len(lines)
    start_idx = 0
    # Skip leading YAML frontmatter (``--- ... ---``).
    if n > 0 and lines[0].strip() == "---":
        for j in range(1, n):
            if lines[j].strip() == "---":
                start_idx = j + 1
                break

    # Split into per-slide source chunks on subsequent ``---`` lines.
    slide_sources: list[str] = []
    current_start = start_idx
    for i in range(start_idx, n):
        if lines[i].strip() == "---":
            chunk = "\n".join(lines[current_start:i])
            slide_sources.append(chunk)
            current_start = i + 1
    if current_start < n:
        slide_sources.append("\n".join(lines[current_start:n]))

    # Drop pure-whitespace tail entries; classify each remaining slide.
    classes: list[str] = []
    for src in slide_sources:
        if not src.strip():
            continue
        m = _CLASS_DIRECTIVE_RE.search(src)
        classes.append(m.group("class") if m else "content")
    return classes


# --- per-PNG bbox detection --------------------------------------------------


def _content_bbox(
    png: Path, *, bg_tolerance: int, corner_margin_px: int, corner_patch_px: int
) -> Optional[ContentBbox]:
    """Return the bounding rectangle of "content" rows/cols in one PNG.

    Algorithm:

    1. Load the PNG into an RGB numpy array (drop alpha — auto-shrink
       does not care about transparency).
    2. Sample four corner patches (after shaving ``corner_margin_px`` off
       each edge to avoid theme-border pixels), take the median colour
       across all four — that's the background.
    3. Build a per-pixel boolean mask: True where any of the three RGB
       channels differs from the background colour by more than
       ``bg_tolerance``. That's the "content" mask.
    4. Reduce by row and column to find ``argmax`` / ``argmin`` extents.
       Returns ``None`` for a completely blank PNG (no rows have content)
       so the caller can skip it gracefully.

    Returns ``None`` if Pillow or numpy are missing — preflight should
    have caught that already, but we double-check so a stray call doesn't
    raise ``ImportError`` inside a critic loop.
    """
    try:
        import numpy as np
        from PIL import Image
    except ImportError:
        return None

    with Image.open(png) as im:
        arr = np.asarray(im.convert("RGB"), dtype=np.int16)

    h, w, _ = arr.shape
    if h < 2 * corner_margin_px + corner_patch_px:
        return None
    if w < 2 * corner_margin_px + corner_patch_px:
        return None

    # Sample four corner patches and take the per-channel median across
    # all four to estimate the background. Median (not mean) so a single
    # dark-theme-corner-decoration pixel doesn't drag the estimate.
    cm = corner_margin_px
    cp = corner_patch_px
    patches = [
        arr[cm : cm + cp, cm : cm + cp],
        arr[cm : cm + cp, w - cm - cp : w - cm],
        arr[h - cm - cp : h - cm, cm : cm + cp],
        arr[h - cm - cp : h - cm, w - cm - cp : w - cm],
    ]
    stacked = np.concatenate([p.reshape(-1, 3) for p in patches], axis=0)
    bg = np.median(stacked, axis=0).astype(np.int16)  # shape (3,)

    diff = np.abs(arr - bg)  # shape (h, w, 3)
    # A pixel is "content" if ANY channel differs from background by more
    # than the tolerance.
    content_mask = (diff > bg_tolerance).any(axis=2)  # shape (h, w)

    if not content_mask.any():
        return None

    rows_with_content = content_mask.any(axis=1)  # shape (h,)
    cols_with_content = content_mask.any(axis=0)  # shape (w,)

    # argmax on a bool array returns the index of the first True.
    top = int(np.argmax(rows_with_content))
    bottom = int(h - 1 - np.argmax(rows_with_content[::-1]))
    left = int(np.argmax(cols_with_content))
    right = int(w - 1 - np.argmax(cols_with_content[::-1]))

    return ContentBbox(
        top=top, bottom=bottom, left=left, right=right, width=w, height=h
    )


# --- public entry point ------------------------------------------------------


def _ensure_pngs(
    deck_pdf: Path,
    png_dir: Optional[Path],
) -> tuple[list[Path], Optional[Path]]:
    """Resolve (or render) the per-page PNG list.

    If ``png_dir`` is provided and contains ``page-*.png`` files, we use
    them as-is (the deck-review pipeline already renders for the vision
    critic; we reuse to avoid a second ~1s/slide render). Otherwise we
    render into a fresh temp dir.

    Returns ``(pngs, tempdir_to_cleanup_or_None)``. The caller is
    responsible for not unlinking the second member if it is ``None``
    (i.e., we reused a caller-owned directory).
    """
    from anvil.lib.render import render_pdf_to_pngs

    if png_dir is not None and any(png_dir.glob("page-*.png")):
        return (
            sorted(
                png_dir.glob("page-*.png"),
                key=lambda p: int(p.stem.rsplit("-", 1)[1]),
            ),
            None,
        )

    tmp = Path(tempfile.mkdtemp(prefix="anvil-auto-shrink-"))
    pngs = render_pdf_to_pngs(deck_pdf, tmp)
    return pngs, tmp


def _median(values: list[float]) -> float:
    """Median of a sorted list of floats. Empty list returns 0.0."""
    if not values:
        return 0.0
    sorted_vals = sorted(values)
    mid = len(sorted_vals) // 2
    if len(sorted_vals) % 2 == 1:
        return sorted_vals[mid]
    return 0.5 * (sorted_vals[mid - 1] + sorted_vals[mid])


def _format_composite_message(
    *,
    slide_idx: int,
    cls: str,
    fired: list[str],
    bbox: ContentBbox,
    medians_triplet: dict[str, float],
) -> str:
    """Compose the AutoShrinkFinding message naming the signals that fired.

    Issue #562: the reviser should see WHICH signals tripped (bottom
    margin vs top margin vs content area) so the fix hint is actionable.
    Each fired signal contributes a short phrase quoting the page's
    value and the class median; the message ends with the same fix
    hint the pre-#562 single-signal rule produced.
    """
    phrases: list[str] = []
    if "bottom_margin" in fired:
        bm_med = medians_triplet["bottom_margin"]
        if bm_med <= 0:
            ratio_str = "inf"
        else:
            ratio_str = f"{bbox.bottom_margin_norm / bm_med:.2f}x"
        phrases.append(
            f"bottom-margin {bbox.bottom_margin_norm * 100:.1f}% "
            f"(class median {bm_med * 100:.1f}%, {ratio_str})"
        )
    if "top_margin" in fired:
        tm_med = medians_triplet["top_margin"]
        if tm_med <= 0:
            ratio_str = "inf"
        else:
            ratio_str = f"{bbox.top_margin_norm / tm_med:.2f}x"
        phrases.append(
            f"top-margin {bbox.top_margin_norm * 100:.1f}% "
            f"(class median {tm_med * 100:.1f}%, {ratio_str})"
        )
    if "content_area" in fired:
        ca_med = medians_triplet["content_area"]
        if ca_med <= 0:
            pct_str = "n/a"
        else:
            pct_str = f"{(bbox.content_area_norm / ca_med) * 100:.0f}%"
        phrases.append(
            f"content-area {bbox.content_area_norm * 100:.1f}% of slide "
            f"(class median {ca_med * 100:.1f}%; {pct_str} of class median)"
        )
    signals_clause = "; ".join(phrases)
    return (
        f"Slide {slide_idx} (class '{cls}') shows fit-to-scale shrink: "
        f"{signals_clause}. Marp likely fit-to-frame-scaled this page — "
        "trim 10–20 words from the densest element or move one bullet to "
        "a peer slide so the content fits without auto-shrink."
    )


def detect_auto_shrink(
    deck_pdf: Path,
    deck_md: Path,
    *,
    thresholds: Optional[Thresholds] = None,
    png_dir: Optional[Path] = None,
) -> AutoShrinkResult:
    """Detect Marp silent auto-shrink across all slides in ``deck_pdf``.

    Per-class peer-comparison rule (composite, post-issue-#562):

    1. Classify each slide via ``<!-- _class: ... -->`` directives in
       ``deck_md`` (default class: ``"content"``).
    2. Compute the per-PNG content bounding box; derive three peer-
       relative signals — ``bottom_margin_norm``, ``top_margin_norm``,
       and ``content_area_norm`` (bbox area / slide area).
    3. For each class with at least ``thresholds.min_peers_per_class``
       pages, compute the per-class median for each signal.
    4. Evaluate three signal-fired predicates per page:

       - **bottom-margin** — ``bottom_margin_norm > 1.5 × class_median``
         AND ``> 0.18``
       - **top-margin** — ``top_margin_norm > 1.5 × class_median``
         AND ``> 0.10`` (#562)
       - **content-area** — ``content_area_norm < 0.75 × class_median``
         (#562; inverted polarity — content shrunk vs peers)

       A page is flagged when at least ``composite_signals_required``
       (default: 2) signals fire. The two-of-three quorum keeps false
       positives low (a single legitimately sparser slide does not
       trigger) while catching the fit-to-scale shrink the pre-#562
       single-signal rule missed.
    5. Classes with fewer than ``min_peers_per_class`` pages are recorded
       in ``skipped_classes`` and never flagged — a singleton class
       (typical: one ``title``, one ``ask``) has no peers to compare
       against.

    Parameters
    ----------
    deck_pdf:
        Path to the rendered ``deck.pdf``. Must exist; if absent the
        function returns an ``AutoShrinkResult(skipped=True, reason=...)``
        (matches the graceful-skip contract documented in the deck-review
        command).
    deck_md:
        Path to the deck markdown source. Used for ``_class:`` directive
        classification only — we do NOT re-parse it for content.
    thresholds:
        Optional override. Defaults to ``Thresholds()`` (1.5x median,
        18% absolute floor, 3 peers minimum).
    png_dir:
        Optional path to a directory that already contains ``page-*.png``
        files from a prior ``render_pdf_to_pngs`` call (e.g., the dir the
        ``deck-vision`` critic rendered into). When provided, the
        detector reuses those PNGs rather than re-rendering.

    Returns
    -------
    An :class:`AutoShrinkResult` carrying per-slide findings plus the
    per-class median baselines (for transparency in ``_summary.md``).
    """
    from anvil.lib.render import (  # local import to keep top-level fast
        AUTO_SHRINK_REMEDIATION,
        RenderError,
        check_auto_shrink_deps_available,
    )

    if not check_auto_shrink_deps_available():
        return AutoShrinkResult(skipped=True, reason=AUTO_SHRINK_REMEDIATION)

    deck_pdf = Path(deck_pdf)
    deck_md = Path(deck_md)
    if not deck_pdf.exists():
        return AutoShrinkResult(
            skipped=True,
            reason=(
                f"deck.pdf not found at {deck_pdf} — run `deck-figures` "
                "before `deck-review` to render the PDF."
            ),
        )
    if not deck_md.exists():
        return AutoShrinkResult(
            skipped=True,
            reason=f"deck.md not found at {deck_md}.",
        )

    th = thresholds or _DEFAULT_THRESHOLDS

    try:
        pngs, tmp_to_clean = _ensure_pngs(deck_pdf, png_dir)
    except (RenderError, FileNotFoundError) as exc:
        return AutoShrinkResult(
            skipped=True,
            reason=f"failed to render PDF to PNGs: {exc}",
        )

    try:
        classes = _classify_slides(deck_md.read_text(encoding="utf-8"))

        # If the slide-class count and the PNG count disagree, fall back
        # to numbering by PNG order and class "content" for any extras.
        # This preserves the "never crash; record findings best-effort"
        # contract — a mid-deck blank class is not worth raising over.
        if len(classes) < len(pngs):
            classes = classes + ["content"] * (len(pngs) - len(classes))
        elif len(classes) > len(pngs):
            classes = classes[: len(pngs)]

        # Per-page bbox + bottom-margin.
        page_records: list[tuple[int, str, Optional[ContentBbox]]] = []
        for idx, (png, cls) in enumerate(zip(pngs, classes), start=1):
            bbox = _content_bbox(
                png,
                bg_tolerance=th.bg_tolerance,
                corner_margin_px=th.corner_margin_px,
                corner_patch_px=th.corner_patch_px,
            )
            page_records.append((idx, cls, bbox))

        # Group per-page bboxes by class. Skip pages whose bbox is None
        # (blank PNG / detector couldn't run on this image). Each entry
        # carries the full bbox so we can read all three composite
        # signals (bottom margin, top margin, content area) per #562.
        by_class: dict[str, list[tuple[int, ContentBbox]]] = {}
        for slide_idx, cls, bbox in page_records:
            if bbox is None:
                continue
            by_class.setdefault(cls, []).append((slide_idx, bbox))

        # Compute medians for classes with enough peers; record
        # too-few-peers reasons for the others. The composite rule
        # (#562) needs three medians per class — bottom margin, top
        # margin, and content area — so we compute the triplet here.
        medians: dict[str, float] = {}  # legacy field: bottom-margin only
        medians_extended: dict[str, dict[str, float]] = {}
        skipped_classes: dict[str, str] = {}
        for cls, observations in by_class.items():
            if len(observations) < th.min_peers_per_class:
                skipped_classes[cls] = (
                    f"only {len(observations)} page(s) in class "
                    f"'{cls}' — minimum {th.min_peers_per_class} required "
                    "for a peer-median comparison."
                )
                continue
            bm_vals = sorted(b.bottom_margin_norm for _, b in observations)
            tm_vals = sorted(b.top_margin_norm for _, b in observations)
            ca_vals = sorted(b.content_area_norm for _, b in observations)
            medians[cls] = _median(bm_vals)
            medians_extended[cls] = {
                "bottom_margin": _median(bm_vals),
                "top_margin": _median(tm_vals),
                "content_area": _median(ca_vals),
            }

        # Composite-signal flag rule (issue #562). For each page we
        # evaluate three independent peer-relative signals; a page is
        # flagged when at least ``composite_signals_required`` of them
        # fire. Each signal requires BOTH the per-class ratio condition
        # AND the absolute-floor condition — same shape as the pre-#562
        # single-signal rule, applied per signal.
        findings: list[AutoShrinkFinding] = []
        for cls, observations in by_class.items():
            if cls not in medians_extended:
                continue
            trip = medians_extended[cls]
            for slide_idx, bbox in observations:
                fired: list[str] = []
                bm = bbox.bottom_margin_norm
                tm = bbox.top_margin_norm
                ca = bbox.content_area_norm

                # 1. Bottom-margin signal — large gap below content.
                if bm > th.abs_bottom_margin_floor:
                    bm_med = trip["bottom_margin"]
                    if bm_med <= 0:
                        bm_ratio = float("inf")
                    else:
                        bm_ratio = bm / bm_med
                    if bm_ratio > th.median_ratio_threshold:
                        fired.append("bottom_margin")

                # 2. Top-margin signal (#562) — large gap above content.
                if tm > th.abs_top_margin_floor:
                    tm_med = trip["top_margin"]
                    if tm_med <= 0:
                        tm_ratio = float("inf")
                    else:
                        tm_ratio = tm / tm_med
                    if tm_ratio > th.top_margin_ratio_threshold:
                        fired.append("top_margin")

                # 3. Content-area signal (#562) — bbox shrunk vs peers.
                # Inverted polarity vs the margin signals: a flag fires
                # when content area is SMALLER than the median (the
                # bbox has shrunk). No absolute floor needed — a tiny
                # bbox is always suspicious; the ratio test is what
                # protects against legitimately-sparse peer sets.
                ca_med = trip["content_area"]
                if ca_med > 0:
                    ca_ratio = ca / ca_med
                    if ca_ratio < th.content_area_ratio_threshold:
                        fired.append("content_area")

                if len(fired) < th.composite_signals_required:
                    continue

                # Compose a message that names the signals that fired
                # so the reviser has actionable rationale (issue #562).
                message = _format_composite_message(
                    slide_idx=slide_idx,
                    cls=cls,
                    fired=fired,
                    bbox=bbox,
                    medians_triplet=trip,
                )
                # Legacy ratio field — bottom-margin ratio for backwards
                # compatibility. When the bottom-margin signal didn't
                # fire, we still report the ratio honestly (computed
                # against the class median); the message clarifies
                # which signals actually triggered the finding.
                bm_med_for_field = trip["bottom_margin"]
                if bm_med_for_field <= 0:
                    legacy_ratio = float("inf") if bm > 0 else 0.0
                else:
                    legacy_ratio = bm / bm_med_for_field

                findings.append(
                    AutoShrinkFinding(
                        slide=slide_idx,
                        class_name=cls,
                        bottom_margin_norm=bm,
                        median_bottom_margin_norm=trip["bottom_margin"],
                        ratio=legacy_ratio,
                        top_margin_norm=tm,
                        median_top_margin_norm=trip["top_margin"],
                        content_area_norm=ca,
                        median_content_area_norm=trip["content_area"],
                        signals_fired=tuple(fired),
                        severity="error",
                        message=message,
                    )
                )

        findings.sort(key=lambda f: f.slide)

        return AutoShrinkResult(
            findings=findings,
            skipped=False,
            reason=None,
            per_class_medians=medians,
            per_class_medians_extended=medians_extended,
            skipped_classes=skipped_classes,
        )
    finally:
        if tmp_to_clean is not None:
            # Best-effort cleanup; if the temp dir is gone or in use we
            # leave it for the OS — never raise from a finally.
            try:
                for child in tmp_to_clean.glob("*"):
                    try:
                        child.unlink()
                    except OSError:
                        pass
                tmp_to_clean.rmdir()
            except OSError:
                pass


__all__ = [
    "AutoShrinkFinding",
    "AutoShrinkResult",
    "ContentBbox",
    "Thresholds",
    "detect_auto_shrink",
]
