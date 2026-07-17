"""Tests for the report-vision critic wiring.

The report-vision critic is a per-skill consumer of the framework
``VisionCritic`` primitive (``anvil/lib/vision.py``, landed in #30 / PR
#49). Unlike ``deck-vision`` it composes a *report-specific* four-dim
rubric — figure legibility, wide-table overflow, page-break/layout
artifacts, and palette adherence — instead of using the deck-shaped
``default_vision_rubric()``.

The VLM call is stubbed with a callback that simulates the expected
detection. Real Anthropic calls are out of scope here (see
``tests/lib/test_vision.py::test_real_anthropic_vlm_smoke`` for the
opt-in smoke path).

The smoke fixture (``fixtures/vision/repro_wide_table_overflow.md``)
reproduces the report's signature rendered defect: a wide specification
table that overflows the page text block at render time, silently
dropping load-bearing columns. The stub callback encodes the expected
vision detection for that fixture.

This test file is named ``test_report_vision.py`` (not ``test_vision.py``
or a generic name shared with the deck/slides skills) to avoid the known
pytest rootdir filename-collision across skills.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Dict

# Ensure repo root is importable. This file lives at
# anvil/skills/report/tests/test_report_vision.py — four levels deep from
# the repo root (mirrors anvil/skills/deck/tests/test_deck_vision.py).
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
# Report-specific vision rubric (composed from framework primitives — no lib
# changes; mirrors the inline rubric documented in
# anvil/skills/report/commands/report-vision.md).
# ---------------------------------------------------------------------------

REPORT_VISION_DIMENSIONS = (
    VisionDimension(
        name="figure_legibility",
        max=5,
        description=(
            "Embedded figures and chart labels are readable at the "
            "recipient's page scale."
        ),
    ),
    VisionDimension(
        name="table_overflow",
        max=5,
        description=(
            "Wide specification tables fit within the page text block; no "
            "columns clipped at the right margin."
        ),
    ),
    VisionDimension(
        name="layout_artifacts",
        max=5,
        description=(
            "Page-break / flow quality: no orphaned headings, widow lines, "
            "or figures split across a page boundary."
        ),
    ),
    VisionDimension(
        name="palette_adherence",
        max=5,
        description=(
            "Embedded charts match the report theme palette rather than "
            "default matplotlib colors."
        ),
    ),
)


def report_vision_rubric() -> VisionRubric:
    return VisionRubric(
        dimensions=REPORT_VISION_DIMENSIONS,
        rubric_id="anvil-report-vision-v1",
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
            _clean_score_row(d.name, d.max) for d in REPORT_VISION_DIMENSIONS
        ],
        "findings": [],
        "critical_flags": [],
    }


# ---------------------------------------------------------------------------
# Rubric composition
# ---------------------------------------------------------------------------


def test_report_rubric_owns_four_dims_scored_out_of_twenty():
    """The report vision rubric is the four report-specific dims, /20."""
    rubric = report_vision_rubric()
    names = [d.name for d in rubric.dimensions]
    assert names == [
        "figure_legibility",
        "table_overflow",
        "layout_artifacts",
        "palette_adherence",
    ]
    assert rubric.max_total() == 20
    assert rubric.rubric_id == "anvil-report-vision-v1"


# ---------------------------------------------------------------------------
# Fixture presence
# ---------------------------------------------------------------------------


def test_fixture_report_present():
    """The wide-table-overflow repro report exists under the fixtures dir."""
    assert (FIXTURES / "repro_wide_table_overflow.md").exists()


# ---------------------------------------------------------------------------
# Wide-table overflow — the report's signature rendered defect
# ---------------------------------------------------------------------------


def _make_stub_for_table_overflow(images, prompt):
    """Stub returning the expected detection for the wide-table repro.

    A 10-column spec table overflows the page text block; the rightmost
    columns ("Lead time", "Unit cost") are clipped and lost. This is a
    ``rendered_overflow_unrecoverable`` critical flag because the dropped
    columns carry load-bearing procurement values.
    """
    payload = _baseline_payload()
    for s in payload["scores"]:
        if s["dimension"] == "table_overflow":
            s["score"] = 0
            s["critical"] = True
            s["justification"] = (
                "Table 1 on page 1 overflows the page text block; the "
                "'Lead time' and 'Unit cost' columns are clipped at the "
                "right margin and lost. Load-bearing procurement data the "
                "recipient never sees."
            )
            s["fix"] = (
                "Split the table, drop to a landscape orientation, or move "
                "low-priority columns to a separate appendix table."
            )
    payload["critical_flags"].append(
        {
            "type": CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE,
            "justification": (
                "The per-component unit cost and lead time are clipped past "
                "the right page margin. A procurement reviewer cannot make "
                "the buy decision the report is for without these columns; "
                "their absence is invisible because the table looks "
                "complete up to the clip."
            ),
            "evidence_span": "report.pdf:page=1",
        }
    )
    payload["findings"].append(
        {
            "severity": "blocker",
            "dimension": "table_overflow",
            "rationale": (
                "Spec table page 1 clipped after the 'Material' column; "
                "'Supplier', 'Lead time', 'Unit cost' are off-page."
            ),
            "suggested_fix": (
                "Restructure the table — split or rotate to landscape."
            ),
            "evidence_span": "report.pdf:page=1",
        }
    )
    return payload


def test_vision_detects_wide_table_overflow(tmp_path):
    """report-vision asserts expected detections for the wide-table repro."""
    fixture_image = tmp_path / "page-1.png"
    fixture_image.write_bytes(b"\x89PNG fake")

    critic = VisionCritic(
        critic_id="report-vision",
        callback=_make_stub_for_table_overflow,
    )
    review = critic.critique(
        images=[fixture_image],
        rubric=report_vision_rubric(),
        version_dir="acme-q2/findings.1",
        rendered_artifact="report.pdf",
    )

    overflow = next(
        s for s in review.scores if s.dimension == "table_overflow"
    )
    assert overflow.score == 0
    assert overflow.critical is True

    flags = [cf.type for cf in review.critical_flags]
    assert CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE in flags


# ---------------------------------------------------------------------------
# Review shape: kind=vision, rendered_artifact, totals, unowned dims null
# ---------------------------------------------------------------------------


def test_vision_review_shape_and_kind():
    """The produced Review is kind=vision with report.pdf rendered_artifact."""
    review = VisionCritic(
        critic_id="report-vision",
        callback=lambda images, prompt: _baseline_payload(),
    ).critique(
        images=[],
        rubric=report_vision_rubric(),
        version_dir="acme-q2/findings.1",
        rendered_artifact="report.pdf",
    )

    assert isinstance(review, Review)
    assert review.kind == Kind.VISION
    assert review.rendered_artifact == "report.pdf"
    assert review.critic_id == "report-vision"
    assert review.rubric == "anvil-report-vision-v1"

    # All four owned dims scored; threshold is the rubric max (/20).
    scored_dims = {s.dimension for s in review.scores}
    assert scored_dims == {
        "figure_legibility",
        "table_overflow",
        "layout_artifacts",
        "palette_adherence",
    }
    assert review.threshold == 20
    # Baseline payload scores each dim at max-1 = 4; total = 16.
    assert review.total == 16


# ---------------------------------------------------------------------------
# Aggregation: the vision sibling discovers + aggregates cleanly alongside a
# main-rubric critic that owns the report's 9 dims (vision puts those null).
# ---------------------------------------------------------------------------


def test_vision_review_discovers_and_aggregates(tmp_path):
    """A written vision _review.json is discovered + aggregated cleanly."""
    from anvil.lib.critics import aggregate, discover_critics, load_review

    # Lay out a version dir with two sibling critic dirs:
    #   acme-q2/findings.1.review/   (a standard judgment-kind review)
    #   acme-q2/findings.1.vision/   (the report-vision sibling)
    # discover_critics enumerates siblings in the version dir's PARENT
    # matching "<version_dir.name>.<tag>", so we pass the version dir path.
    project = tmp_path / "acme-q2"
    version_dir = project / "findings.1"
    review_dir = project / "findings.1.review"
    vision_dir = project / "findings.1.vision"
    review_dir.mkdir(parents=True)
    vision_dir.mkdir(parents=True)

    # A standard judgment-kind review sibling (a stand-in for report-review).
    # The version_dir string MUST match the vision review's for aggregate().
    # It scores one main-rubric dimension; the schema requires a non-empty
    # scores list. The four vision dims are disjoint, so the merge is clean.
    from anvil.lib.review_schema import Score

    main_review = Review(
        schema_version="1",
        kind=Kind.JUDGMENT,
        version_dir="acme-q2/findings.1",
        critic_id="report-review",
        scores=[
            Score(
                dimension="format_presentation",
                score=3,
                max=4,
                justification="Source-side format read; layout unverified.",
            )
        ],
    )
    (review_dir / "_review.json").write_text(
        main_review.model_dump_json(indent=2)
    )

    vision_review = VisionCritic(
        critic_id="report-vision",
        callback=_make_stub_for_table_overflow,
    ).critique(
        images=[tmp_path / "page-1.png"],
        rubric=report_vision_rubric(),
        version_dir="acme-q2/findings.1",
        rendered_artifact="report.pdf",
    )
    (vision_dir / "_review.json").write_text(
        vision_review.model_dump_json(indent=2)
    )

    # Discovery finds both sibling dirs.
    found_dirs = discover_critics(version_dir)
    found_names = {p.name for p in found_dirs}
    assert "findings.1.review" in found_names
    assert "findings.1.vision" in found_names

    # Load + aggregate merges cleanly; the vision critical flag forces BLOCK.
    reviews = [load_review(p) for p in found_dirs]
    critic_ids = {r.critic_id for r in reviews}
    assert "report-review" in critic_ids
    assert "report-vision" in critic_ids

    agg = aggregate(reviews)
    assert agg.verdict == Verdict.BLOCK
    flag_types = {cf.type for cf in agg.critical_flags}
    assert CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE in flag_types


# ---------------------------------------------------------------------------
# Command spec presence
# ---------------------------------------------------------------------------


def test_report_vision_command_spec_exists():
    """anvil/skills/report/commands/report-vision.md is present and canonical."""
    cmd = (
        _REPO_ROOT
        / "anvil"
        / "skills"
        / "report"
        / "commands"
        / "report-vision.md"
    )
    assert cmd.exists()
    text = cmd.read_text()
    # The four owned dims are documented.
    for d in REPORT_VISION_DIMENSIONS:
        assert d.name in text
    # The reused critical-flag type is documented.
    assert CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE in text
    # The progress / meta / review shapes are referenced.
    assert "_progress.json" in text
    assert "_meta.json" in text
    assert "_review.json" in text
    # Pandoc render path reference (report uses pandoc, not Marp).
    assert "render_pandoc_to_pdf" in text
    assert "render_pdf_to_pngs" in text
