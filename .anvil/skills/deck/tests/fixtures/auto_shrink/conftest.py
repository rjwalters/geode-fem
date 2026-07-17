"""Synthetic PNG fixture generation for the auto-shrink-detector tests.

The detector reads rendered-deck PNGs; we don't need a real Marp render
to test it. We just need PNGs that look (to the corner-sample +
threshold + argmax detector) like a Marp page where the content
occupies a specific bbox.

Each fixture PNG is a white-background 1280x720 image with one or more
filled black rectangles. Per-rectangle placement controls the three
peer-relative shrink signals the detector measures:

- bottom margin (height − content_bottom)
- top margin   (content_top)
- content area ((content_w × content_h) / (slide_w × slide_h))

Fixtures (F1-F5 are the pre-#562 cases; F6/F7 are the composite-rule
regressions added by issue #562; F8 is a classification-only test
against the committed ``fixtures/auto_shrink/deck.md`` and needs no PNG
generation):

* F1 — Content fills 90% of slide height; not flagged.
* F2 — Content fills 40% of slide height (60% bottom margin); flagged
  against a peer-set of 85%-fill peers (15% bottom margin).
* F3 — A single ``title``-class slide; never flagged because peer count
  is below the threshold.
* F4 — All three peers fill 50% (50% bottom margin). All are above the
  absolute floor but the ratio is 1.0; NONE flagged.
* F5 — Two ``content`` peers at 99% fill (1% bottom margin) and one at
  10% fill (90% bottom margin). The ratio is enormous but the absolute
  floor IS exceeded — this fixture exists primarily to verify the
  reverse: a slide where the ratio is large but absolute is small
  must NOT flag. We construct that by adding the peer triplet
  ``(7%, 9%, 11%)`` bottom margins so the median is 9%, well below
  the 18% floor, and a candidate at ratio 1.5x (13.5% bottom margin)
  still doesn't exceed the floor.
* F6 — Fit-shrunk slide whose bottom margin is within ~1.1x of the
  class median (so the bottom-margin-alone signal does NOT fire), but
  whose top margin is large AND whose content area is well below peer
  median. The composite rule must flag this case (#562 — the GoodBoy
  slides 3/8 fit-shrink mode the pre-#562 detector missed).
* F7 — A slide that fires ONLY the content-area signal (smaller bbox
  vs peers) but neither margin signal. The two-of-three quorum must
  NOT flag — content-area alone is a single signal.

The conftest is loaded by pytest automatically when a test file in
this directory (or above) requests a fixture from it.
"""

from __future__ import annotations

from pathlib import Path

import pytest


# Slide geometry mirrors the deck Marp config: 16:9, 1280x720.
SLIDE_W = 1280
SLIDE_H = 720


def _make_png(out_path: Path, content_bottom_norm: float) -> None:
    """Render a single white-bg PNG with a filled rectangle.

    ``content_bottom_norm`` is the *fill* fraction — i.e., the content
    rectangle extends from y=0 down to y=int(content_bottom_norm * H).
    The resulting ``bottom_margin_norm`` measured by the detector will
    be approximately ``1 - content_bottom_norm``.

    This is the legacy F1–F5 drawer. New fixtures (#562) use
    ``_make_bbox_png`` which controls top/bottom/left/right placement
    independently so all three composite signals (bottom margin, top
    margin, content area) can be set deliberately.
    """
    from PIL import Image, ImageDraw

    img = Image.new("RGB", (SLIDE_W, SLIDE_H), (255, 255, 255))
    draw = ImageDraw.Draw(img)
    # Draw a black rectangle spanning the full slide width from the top
    # down to ``content_bottom_y``. Some left/right padding so the
    # column-bbox check doesn't trip over edge pixels.
    content_bottom_y = max(1, int(SLIDE_H * content_bottom_norm))
    draw.rectangle(
        [(40, 40), (SLIDE_W - 40, content_bottom_y)],
        fill=(0, 0, 0),
    )
    img.save(out_path, "PNG")


def _make_bbox_png(
    out_path: Path,
    *,
    top_norm: float,
    bottom_norm: float,
    left_norm: float = 0.03,
    right_norm: float = 0.97,
) -> None:
    """Render a PNG whose content bbox sits at the requested normalised extents.

    All four arguments are fractions of slide W/H. The drawn rectangle's
    bbox (as measured by ``_content_bbox``) will be approximately:

    - ``top``    = ``int(top_norm * H)``
    - ``bottom`` = ``int(bottom_norm * H)``
    - ``left``   = ``int(left_norm * W)``
    - ``right``  = ``int(right_norm * W)``

    The detector will then compute:

    - ``top_margin_norm    ≈ top_norm``
    - ``bottom_margin_norm ≈ 1 - bottom_norm``
    - ``content_area_norm  ≈ (bottom_norm - top_norm) × (right_norm - left_norm)``

    Used by the F6 / F7 composite-rule fixtures introduced in issue
    #562.
    """
    from PIL import Image, ImageDraw

    img = Image.new("RGB", (SLIDE_W, SLIDE_H), (255, 255, 255))
    draw = ImageDraw.Draw(img)
    top_y = max(1, int(SLIDE_H * top_norm))
    bottom_y = max(top_y + 1, int(SLIDE_H * bottom_norm))
    left_x = max(1, int(SLIDE_W * left_norm))
    right_x = max(left_x + 1, int(SLIDE_W * right_norm))
    draw.rectangle(
        [(left_x, top_y), (right_x, bottom_y)],
        fill=(0, 0, 0),
    )
    img.save(out_path, "PNG")


def _write_deck_md(out_path: Path, class_directives: list[str]) -> None:
    """Write a minimal deck.md whose slide classes match the per-page list."""
    parts = ["---", "marp: true", "theme: anvil-deck", "---", ""]
    for i, cls in enumerate(class_directives, start=1):
        if i > 1:
            parts.append("---")
            parts.append("")
        if cls != "content":
            parts.append(f"<!-- _class: {cls} -->")
            parts.append("")
        parts.append(f"# Slide {i}")
        parts.append("")
    out_path.write_text("\n".join(parts), encoding="utf-8")


@pytest.fixture(scope="session")
def auto_shrink_fixture_root(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """Build the F1-F5 fixture directories under a session-scoped tmp_path.

    Each F-case gets its own directory:

    ``F1/``  one slide PNG (90% fill) + deck.md with one ``content`` slide.
            Used to assert NO finding for a not-shrunk slide; we add two
            extra 88%/92% peers so the class has the required 3 peers.

    ``F2/``  three slides — two peers at 85% fill (15% bottom margin) and
            one at 40% fill (60% bottom margin); detector should flag
            slide 3.

    ``F3/``  one ``title``-class slide. Detector must record a skipped
            class with reason and emit NO finding.

    ``F4/``  three ``content`` peers, all at 50% fill (50% bottom
            margin). Median 50%, ratio 1.0; absolute exceeds floor but
            ratio doesn't — NONE flagged.

    ``F5/``  three ``content`` peers at 7%/9%/11% bottom-margin
            (i.e., ~93%/91%/89% fills) + one candidate at 13.5%
            bottom margin (86.5% fill). Median is 9%; candidate is
            1.5x median (matches the boundary) but absolute (13.5%)
            is below the 18% floor — NOT flagged.
    """
    root = tmp_path_factory.mktemp("auto_shrink_fixtures")

    def _setup(case_dir: Path, fills: list[float], classes: list[str]) -> None:
        case_dir.mkdir()
        for i, fill in enumerate(fills, start=1):
            _make_png(case_dir / f"page-{i}.png", content_bottom_norm=fill)
        _write_deck_md(case_dir / "deck.md", classes)
        # An empty stub PDF so the detector's existence check passes; the
        # PNGs are pre-rendered in this dir so _ensure_pngs never invokes
        # the real pdftoppm chain.
        (case_dir / "deck.pdf").write_bytes(b"%PDF-stub\n")

    # --- F1: all slides healthy (NOT flagged) ---
    _setup(root / "F1", [0.90, 0.88, 0.92], ["content"] * 3)

    # --- F2: outlier auto-shrunk slide on slide 3 (FLAGGED) ---
    # peers at 85% fill (bm ~15%); slide 3 at 40% fill (bm ~60%).
    _setup(root / "F2", [0.85, 0.85, 0.40], ["content"] * 3)

    # --- F3: singleton title slide (NEVER flagged; recorded as skipped) ---
    # Deliberately use a deeply-shrunk fill (bm ~70%) to prove the
    # singleton-skip rule wins over the absolute-floor rule — a singleton
    # class must never be flagged regardless of its individual margins.
    _setup(root / "F3", [0.30], ["title"])

    # --- F4: all peers equally light (NONE flagged; ratio=1.0) ---
    _setup(root / "F4", [0.50, 0.50, 0.50], ["content"] * 3)

    # --- F5: candidate's ratio AT the boundary but absolute < floor ---
    # Peers at bm 7%, 9%, 11% (median 9%); candidate at bm 13.5%
    # (= 1.5x median, AT the ratio boundary, but 13.5% < 18% absolute
    # floor). Detector must NOT flag — both conditions are required.
    _setup(
        root / "F5",
        [0.93, 0.91, 0.89, 0.865],
        ["content"] * 4,
    )

    # --- F6: fit-shrunk slide whose bottom margin is near peer median ---
    # This is the issue-#562 regression — the GoodBoy slides 3/8 mode
    # the pre-#562 single-signal detector missed.
    #
    # Three peer slides: large top-margin-aware bboxes with content
    # filling top=5%/bottom=85% (top margin ~5%, bottom margin ~15%,
    # content area ~78%).
    #
    # Slide 4 is the fit-shrunk candidate: top=20%, bottom=68%,
    # left=18%, right=82% — bottom margin ~32% (~2.1x peer median;
    # passes the bottom-margin signal), top margin ~20% (~4x peer
    # median; passes the top-margin signal), content area ~31% of
    # slide (~40% of peer median; passes the content-area signal).
    # All three signals fire → ≥2 of 3 → flagged.
    #
    # Note: synthesising a "bottom-margin within 1.1x of median" while
    # keeping the other two signals firing is awkward with the simple
    # rectangle drawer because reducing top + bottom shrinks the bbox
    # symmetrically. We rely on the composite rule's tolerance: ANY
    # two-of-three combination triggers, and the canary mode (Marp
    # fit-to-scale) typically fires all three together. This fixture
    # confirms the composite rule activates on the multi-signal mode
    # the pre-#562 rule would have missed if ONLY top-margin and
    # content-area fired; the F6_TopAndContentOnly subcase below pins
    # that exact two-of-three combination.
    case = root / "F6"
    case.mkdir()
    # Peers — content area ~76% (top 5%, bottom 85%, columns 5%-95%).
    for i in range(3):
        _make_bbox_png(
            case / f"page-{i + 1}.png",
            top_norm=0.05,
            bottom_norm=0.85,
            left_norm=0.05,
            right_norm=0.95,
        )
    # Candidate (slide 4) — fit-shrunk:
    #   top 20%, bottom 68%, columns 18%-82%
    #   → top-margin ~20% (4x peer median ~5%; fires top-margin signal)
    #   → bottom-margin ~32% (2.1x peer median ~15%; fires bottom-margin signal)
    #   → content area = (0.68 - 0.20) × (0.82 - 0.18) = 0.48 × 0.64 = ~0.31
    #     vs peer median ~0.76; 0.41x peer median; fires content-area signal
    _make_bbox_png(
        case / "page-4.png",
        top_norm=0.20,
        bottom_norm=0.68,
        left_norm=0.18,
        right_norm=0.82,
    )
    _write_deck_md(case / "deck.md", ["content"] * 4)
    (case / "deck.pdf").write_bytes(b"%PDF-stub\n")

    # --- F6_TopAndContent: fit-shrunk with bottom-margin SAFE ---
    # The harder sub-case: bottom margin within ~1.1x of class median
    # (single bottom-margin signal does NOT fire), but top margin AND
    # content area both fire. Two-of-three quorum must still flag.
    #
    # Peer slides — top 5%, bottom 85% (bm ~15%, tm ~5%, area ~76%).
    # Candidate — top 25%, bottom 84% (bm ~16% — within 1.1x of
    # peer median, so bottom-margin signal does NOT fire), columns
    # 25%-75% (area = 0.59 × 0.5 = ~0.295 — 39% of peer median; fires
    # content-area signal). Top margin 25% (~5x peer median; fires
    # top-margin signal). Composite quorum: 2 of 3 → flagged.
    case = root / "F6_TopAndContent"
    case.mkdir()
    for i in range(3):
        _make_bbox_png(
            case / f"page-{i + 1}.png",
            top_norm=0.05,
            bottom_norm=0.85,
            left_norm=0.05,
            right_norm=0.95,
        )
    _make_bbox_png(
        case / "page-4.png",
        top_norm=0.25,
        bottom_norm=0.84,
        left_norm=0.25,
        right_norm=0.75,
    )
    _write_deck_md(case / "deck.md", ["content"] * 4)
    (case / "deck.pdf").write_bytes(b"%PDF-stub\n")

    # --- F7: only content-area fires — two-of-three quorum NOT met ---
    # The candidate has shrunken content area vs peers (single signal)
    # but its margins sit right at the peer medians. The composite rule
    # must NOT flag.
    #
    # Peer slides — top 5%, bottom 85%, columns 5%-95% (area ~76%).
    # Candidate — top 5%, bottom 85% (margins identical to peers; no
    # margin signal fires), columns 35%-65% (narrow column; area =
    # 0.80 × 0.30 = ~0.24, vs peer median ~0.76 → 32% of peer median;
    # content-area signal fires alone).
    case = root / "F7"
    case.mkdir()
    for i in range(3):
        _make_bbox_png(
            case / f"page-{i + 1}.png",
            top_norm=0.05,
            bottom_norm=0.85,
            left_norm=0.05,
            right_norm=0.95,
        )
    _make_bbox_png(
        case / "page-4.png",
        top_norm=0.05,
        bottom_norm=0.85,
        left_norm=0.35,
        right_norm=0.65,
    )
    _write_deck_md(case / "deck.md", ["content"] * 4)
    (case / "deck.pdf").write_bytes(b"%PDF-stub\n")

    return root
