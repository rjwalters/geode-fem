"""Tests for the ip-uspto-vision critic wiring.

The ip-uspto-vision critic is a per-skill consumer of the framework
``VisionCritic`` primitive (``anvil/lib/vision.py``, landed in #30 / PR
#49). It is the odd one out among the per-skill vision critics: it
critiques the patent **drawings only** (line art, reference numerals,
lead lines), NOT a rendered spec PDF. The spec prose is a text artifact
covered by the source-side text critics (review / s101 / s112 / claims /
priorart).

Like ``report-vision`` it composes a *skill-specific* rubric instead of
``default_vision_rubric()`` (the default six dims are deck-shaped). The
ip-uspto drawing rubric is five USPTO-drawing dimensions:
reference-numeral legibility, line weight / contrast, label placement,
figure-number visibility, and the pixels-side half of cross-reference
accuracy.

The VLM call is stubbed with a callback that simulates the expected
detection. Real Anthropic calls are out of scope here (see
``tests/lib/test_vision.py`` for the opt-in smoke path).

The smoke fixture (``fixtures/vision/repro_drawing_defects.md``)
documents the drawing's signature rendered defect: a reference numeral
clipped / unreadable at examiner scale. The stub callback encodes the
expected vision detection for that fixture.

This test file is named ``test_ip_uspto_vision.py`` (not
``test_vision.py`` or a generic name shared with the deck/slides/report/
paper skills) to avoid the known pytest rootdir filename-collision across
skills.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Dict

# Ensure repo root is importable. This file lives at
# anvil/skills/ip-uspto/tests/test_ip_uspto_vision.py — four levels deep
# from the repo root (mirrors anvil/skills/report/tests/test_report_vision.py).
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.lib.review_schema import Kind, Review, Verdict  # noqa: E402
from anvil.lib.vision import (  # noqa: E402
    CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE,
    VisionCritic,
    VisionDimension,
    VisionRubric,
)


FIXTURES = _HERE / "fixtures" / "vision"


# ---------------------------------------------------------------------------
# ip-uspto drawing vision rubric (composed from framework primitives — no lib
# changes; mirrors the inline rubric documented in
# anvil/skills/ip-uspto/commands/ip-uspto-vision.md).
# ---------------------------------------------------------------------------

IP_USPTO_VISION_DIMENSIONS = (
    VisionDimension(
        name="reference_numeral_legibility",
        max=5,
        description=(
            "Every reference numeral is readable at the scale a USPTO "
            "examiner views the reduced sheet."
        ),
    ),
    VisionDimension(
        name="line_weight_contrast",
        max=5,
        description=(
            "37 CFR 1.84(l): black ink line art on white, uniform well-"
            "defined line weights, no gray fills or low-contrast color."
        ),
    ),
    VisionDimension(
        name="label_placement",
        max=5,
        description=(
            "Numeral labels and lead lines placed cleanly: no overlap with "
            "line art or each other, none outside the drawing border."
        ),
    ),
    VisionDimension(
        name="figure_number_visibility",
        max=5,
        description=(
            "37 CFR 1.84(u): every drawing/view carries a visible, "
            "unclipped 'FIG. N' label."
        ),
    ),
    VisionDimension(
        name="cross_reference_accuracy",
        max=5,
        description=(
            "Numerals drawn on the figures correspond to numerals the spec "
            "describes (pixels-side half of rubric Dim 7)."
        ),
    ),
)


def ip_uspto_vision_rubric() -> VisionRubric:
    return VisionRubric(
        dimensions=IP_USPTO_VISION_DIMENSIONS,
        rubric_id="anvil-ip-uspto-vision-v1",
    )


def _clean_score_row(dim_name: str, max_: int) -> Dict:
    return {
        "dimension": dim_name,
        "score": max_ - 1,
        "critical": False,
        "justification": "Default clean score for this fixture.",
        "fix": None,
    }


def _baseline_payload() -> Dict:
    """An "all clean" payload that individual fixtures perturb."""
    return {
        "scores": [
            _clean_score_row(d.name, d.max) for d in IP_USPTO_VISION_DIMENSIONS
        ],
        "findings": [],
        "critical_flags": [],
    }


# ---------------------------------------------------------------------------
# Rubric composition
# ---------------------------------------------------------------------------


def test_ip_uspto_rubric_owns_five_dims_scored_out_of_twentyfive():
    """The ip-uspto drawing vision rubric is the five drawing dims, /25."""
    rubric = ip_uspto_vision_rubric()
    names = [d.name for d in rubric.dimensions]
    assert names == [
        "reference_numeral_legibility",
        "line_weight_contrast",
        "label_placement",
        "figure_number_visibility",
        "cross_reference_accuracy",
    ]
    assert rubric.max_total() == 25
    assert rubric.rubric_id == "anvil-ip-uspto-vision-v1"


# ---------------------------------------------------------------------------
# Fixture presence
# ---------------------------------------------------------------------------


def test_fixture_drawing_present():
    """The drawing-defects repro exists under the fixtures dir."""
    assert (FIXTURES / "repro_drawing_defects.md").exists()


# ---------------------------------------------------------------------------
# Clipped / unreadable reference numeral — the ip-uspto signature drawing defect
# ---------------------------------------------------------------------------


def _make_stub_for_clipped_numeral(images, prompt):
    """Stub returning the expected detection for the drawing-defects repro.

    On FIG. 2 the reference numeral '14' (processor) is clipped at the
    right drawing border and overlaps the lead line for '16'; at the
    examiner's reduced sheet scale it is unreadable. This is a
    ``rendered_overflow_unrecoverable`` critical flag because the clipped
    numeral carries load-bearing part identification the examiner cannot
    see. The cross-section hatching is rendered low-contrast gray
    (37 CFR 1.84(l)).
    """
    payload = _baseline_payload()
    for s in payload["scores"]:
        if s["dimension"] == "reference_numeral_legibility":
            s["score"] = 0
            s["critical"] = True
            s["justification"] = (
                "Reference numeral '14' on FIG. 2 is clipped at the right "
                "drawing border and unreadable at examiner sheet scale."
            )
            s["fix"] = (
                "Reposition numeral '14' inside the drawing border and "
                "increase its rendered size in the SVG source."
            )
        elif s["dimension"] == "label_placement":
            s["score"] = 1
            s["justification"] = (
                "The lead line for '14' overlaps numeral '16'; the two "
                "labels collide and neither clearly points to its part."
            )
            s["fix"] = "Separate the lead lines for '14' and '16'."
        elif s["dimension"] == "line_weight_contrast":
            s["score"] = 2
            s["justification"] = (
                "Cross-section hatching rendered in light gray rather than "
                "black ink — low contrast against the white background."
            )
            s["fix"] = "Render hatching as solid black per 37 CFR 1.84(l)."
    payload["critical_flags"].append(
        {
            "type": CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE,
            "justification": (
                "Reference numeral '14' (processor) is clipped at the right "
                "drawing border on FIG. 2. The examiner cannot determine "
                "which part numeral 14 identifies; the load-bearing "
                "identification present in the source is lost at render time."
            ),
            "evidence_span": "drawings/fig-2.png",
        }
    )
    payload["findings"].append(
        {
            "severity": "blocker",
            "dimension": "reference_numeral_legibility",
            "rationale": (
                "Numeral '14' clipped at the FIG. 2 border; '14' and '16' "
                "lead lines overlap so neither numeral is legible."
            ),
            "suggested_fix": (
                "Reposition '14' inside the border and separate the lead "
                "lines in the drawing source."
            ),
            "evidence_span": "drawings/fig-2.png",
        }
    )
    return payload


def test_vision_detects_clipped_reference_numeral(tmp_path):
    """ip-uspto-vision asserts expected detections for the drawing repro."""
    fixture_image = tmp_path / "fig-2.png"
    fixture_image.write_bytes(b"\x89PNG fake")

    critic = VisionCritic(
        critic_id="ip-uspto-vision",
        callback=_make_stub_for_clipped_numeral,
    )
    review = critic.critique(
        images=[fixture_image],
        rubric=ip_uspto_vision_rubric(),
        version_dir="acme-widget.2",
        rendered_artifact="drawings/",
    )

    legibility = next(
        s for s in review.scores if s.dimension == "reference_numeral_legibility"
    )
    assert legibility.score == 0
    assert legibility.critical is True

    flags = [cf.type for cf in review.critical_flags]
    assert CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE in flags


# ---------------------------------------------------------------------------
# Review shape: kind=vision, rendered_artifact=drawings/, totals, owned dims
# ---------------------------------------------------------------------------


def test_vision_review_shape_and_kind():
    """The produced Review is kind=vision with drawings/ rendered_artifact."""
    review = VisionCritic(
        critic_id="ip-uspto-vision",
        callback=lambda images, prompt: _baseline_payload(),
    ).critique(
        images=[],
        rubric=ip_uspto_vision_rubric(),
        version_dir="acme-widget.2",
        rendered_artifact="drawings/",
    )

    assert isinstance(review, Review)
    assert review.kind == Kind.VISION
    assert review.rendered_artifact == "drawings/"
    assert review.critic_id == "ip-uspto-vision"
    assert review.rubric == "anvil-ip-uspto-vision-v1"

    # All five owned dims scored; threshold is the rubric max (/25).
    scored_dims = {s.dimension for s in review.scores}
    assert scored_dims == {
        "reference_numeral_legibility",
        "line_weight_contrast",
        "label_placement",
        "figure_number_visibility",
        "cross_reference_accuracy",
    }
    assert review.threshold == 25
    # Baseline payload scores each of five dims at max-1 = 4; total = 20.
    assert review.total == 20


def test_vision_does_not_score_main_rubric_dims():
    """The vision critic owns only dv1-dv5; it never scores the 8 main dims.

    Guards the scope boundary: ip-uspto-vision is drawings-only and must
    leave the patent's prose/claims/statutory dimensions to the text
    critics (their scores stay null from the vision critic's perspective).
    """
    review = VisionCritic(
        critic_id="ip-uspto-vision",
        callback=lambda images, prompt: _baseline_payload(),
    ).critique(
        images=[],
        rubric=ip_uspto_vision_rubric(),
        version_dir="acme-widget.2",
        rendered_artifact="drawings/",
    )
    scored_dims = {s.dimension for s in review.scores}
    # None of the eight main-rubric dimension names appear.
    for main_dim in (
        "claim_breadth",
        "s112a",
        "s112b",
        "s101",
        "novelty",
        "specification_completeness",
        "drawing_text_correspondence",
        "formal_compliance",
    ):
        assert main_dim not in scored_dims


# ---------------------------------------------------------------------------
# Aggregation: the vision sibling discovers + aggregates cleanly alongside a
# source-side critic that owns the patent's main dims (vision puts those null).
# ---------------------------------------------------------------------------


def test_vision_review_discovers_and_aggregates(tmp_path):
    """A written vision _review.json is discovered + aggregated cleanly."""
    from anvil.lib.critics import aggregate, discover_critics, load_review
    from anvil.lib.review_schema import Score

    # Lay out a version dir with two sibling critic dirs:
    #   acme-widget.2.review/   (a source-side machine-summary review)
    #   acme-widget.2.vision/   (the ip-uspto-vision sibling)
    # discover_critics enumerates siblings in the version dir's PARENT
    # matching "<version_dir.name>.<tag>", so we pass the version dir path.
    portfolio = tmp_path / "portfolio"
    version_dir = portfolio / "acme-widget.2"
    review_dir = portfolio / "acme-widget.2.review"
    vision_dir = portfolio / "acme-widget.2.vision"
    review_dir.mkdir(parents=True)
    vision_dir.mkdir(parents=True)

    # A source-side review sibling (stand-in for ip-uspto-review). It scores
    # the main-rubric drawing-text-correspondence dimension (Dim 7) from the
    # source. The five vision dims are disjoint, so the merge is clean. The
    # version_dir string MUST match the vision review's for aggregate().
    main_review = Review(
        schema_version="1",
        kind=Kind.JUDGMENT,
        version_dir="acme-widget.2",
        critic_id="ip-uspto-review",
        scores=[
            Score(
                dimension="drawing_text_correspondence",
                score=4,
                max=5,
                justification="Source-side refnum cross-check; pixels unverified.",
            )
        ],
    )
    (review_dir / "_review.json").write_text(
        main_review.model_dump_json(indent=2)
    )

    vision_review = VisionCritic(
        critic_id="ip-uspto-vision",
        callback=_make_stub_for_clipped_numeral,
    ).critique(
        images=[tmp_path / "fig-2.png"],
        rubric=ip_uspto_vision_rubric(),
        version_dir="acme-widget.2",
        rendered_artifact="drawings/",
    )
    (vision_dir / "_review.json").write_text(
        vision_review.model_dump_json(indent=2)
    )

    # Discovery finds both sibling dirs.
    found_dirs = discover_critics(version_dir)
    found_names = {p.name for p in found_dirs}
    assert "acme-widget.2.review" in found_names
    assert "acme-widget.2.vision" in found_names

    # Load + aggregate merges cleanly; the vision critical flag forces BLOCK.
    reviews = [load_review(p) for p in found_dirs]
    critic_ids = {r.critic_id for r in reviews}
    assert "ip-uspto-review" in critic_ids
    assert "ip-uspto-vision" in critic_ids

    agg = aggregate(reviews)
    assert agg.verdict == Verdict.BLOCK
    flag_types = {cf.type for cf in agg.critical_flags}
    assert CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE in flag_types


# ---------------------------------------------------------------------------
# Command spec presence
# ---------------------------------------------------------------------------


def test_ip_uspto_vision_command_spec_exists():
    """anvil/skills/ip-uspto/commands/ip-uspto-vision.md is present + canonical."""
    cmd = (
        _REPO_ROOT
        / "anvil"
        / "skills"
        / "ip-uspto"
        / "commands"
        / "ip-uspto-vision.md"
    )
    assert cmd.exists()
    text = cmd.read_text()
    # The five owned dims are documented.
    for d in IP_USPTO_VISION_DIMENSIONS:
        assert d.name in text
    # The reused critical-flag type is documented.
    assert CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE in text
    # The canonical progress / meta / review shapes are referenced.
    assert "_progress.json" in text
    assert "_meta.json" in text
    assert "_review.json" in text
    # Drawings-only scope: matplotlib walker is referenced; spec PDF render is
    # explicitly out of scope (no Marp / pandoc spec render).
    assert "render_matplotlib_figures" in text
    assert "drawings/" in text
    # rendered_artifact is the drawings, not a spec PDF.
    assert 'rendered_artifact="drawings/"' in text or "rendered_artifact=drawings/" in text
