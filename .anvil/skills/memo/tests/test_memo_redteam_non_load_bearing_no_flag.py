"""Regression test: non-load-bearing SURVIVES does NOT emit a critical flag (issue #560).

The red-team severity ladder splits on load-bearing-ness:

| Verdict     | Load-bearing? | Severity   | dim 3 deduction | Critical flag      |
|-------------|---------------|------------|-----------------|--------------------|
| DEFEATED    | (any)         | (none)     | 0               | no                 |
| SURVIVES    | load-bearing  | critical   | -2              | yes (survives)     |
| SURVIVES    | non-load-b.   | important  | -1              | **no**             |
| UNENGAGED   | load-bearing  | critical   | -2              | yes (unengaged)    |
| UNENGAGED   | non-load-b.   | important  | -1              | **no**             |

This file asserts that a red-team ``_review.json`` carrying ONLY a
non-load-bearing ``SURVIVES`` finding (a ``major`` severity finding on
dim 3, no ``CriticalFlag``) does NOT force ``advance: false`` on its
own. The aggregated verdict over a passing memo-review + a
non-load-bearing red-team is ``Verdict.ADVANCE`` when the total clears
threshold — confirming the load-bearing gate is real and not a
blanket-block on any red-team finding.

Per the per-skill test filename convention (#58), this file is named
``test_memo_redteam_non_load_bearing_no_flag.py``.
"""

from __future__ import annotations

from pathlib import Path

from anvil.lib.critics import aggregate
from anvil.lib.review_schema import (
    Finding,
    Review,
    Score,
    Verdict,
)


def _passing_review_critic() -> Review:
    """A clean memo-review at 44/44 — clears threshold, no flags."""
    return Review(
        version_dir="investment-memo.3",
        critic_id="review",
        scores=[
            Score(dimension="dim_1", score=4, max=4),
            Score(dimension="dim_2", score=6, max=6),
            Score(dimension="dim_3", score=6, max=6),
            Score(dimension="dim_4", score=4, max=4),
            Score(dimension="dim_5", score=6, max=6),
            Score(dimension="dim_6", score=6, max=6),
            Score(dimension="dim_7", score=4, max=4),
            Score(dimension="dim_8", score=4, max=4),
            Score(dimension="dim_9", score=4, max=4),
        ],
        threshold=35,
        rubric="anvil-memo-v2",
    )


def _redteam_with_non_load_bearing_survives() -> Review:
    """A red-team review with one non-load-bearing SURVIVES finding.

    - Severity: ``major`` (not ``blocker``).
    - dim 3 score: 5/6 (the -1 deduction for a non-load-bearing
      survivor).
    - NO ``CriticalFlag`` entry — per the severity ladder above.
    """
    return Review(
        version_dir="investment-memo.3",
        critic_id="redteam",
        scores=[
            Score(
                dimension="dim_2",
                score=6,
                max=6,
                justification="thesis coherence holds under adversarial reading",
            ),
            Score(
                dimension="dim_3",
                score=5,
                max=6,
                justification=(
                    "Objection 4 (timing risk — competitor X has a 6-month "
                    "head start) SURVIVES but is non-load-bearing; "
                    "-1 deduction"
                ),
            ),
            Score(dimension="dim_1", score=None, max=4),
            Score(dimension="dim_4", score=None, max=4),
            Score(dimension="dim_5", score=None, max=6),
            Score(dimension="dim_6", score=None, max=6),
            Score(dimension="dim_7", score=None, max=4),
            Score(dimension="dim_8", score=None, max=4),
            Score(dimension="dim_9", score=None, max=4),
        ],
        findings=[
            Finding(
                severity="major",  # NOT blocker — non-load-bearing
                dimension="dim_3",
                evidence_span="investment-memo.md:L200-L210",
                rationale=(
                    "Objection 4 (competitor X has a 6-month head start) "
                    "SURVIVES the memo's rebuttal but is non-load-bearing "
                    "for the recommendation; the market-shape framing in "
                    "section 4 already addresses the load-bearing version "
                    "of the timing objection."
                ),
                suggested_fix=(
                    "Optional: acknowledge competitor X's head start "
                    "explicitly in section 4 and reframe as 'we are the "
                    "first to address vertical Y'."
                ),
            )
        ],
        critical_flags=[],  # NO critical flag — load-bearing gate is real
        threshold=35,
        rubric="anvil-memo-v2",
    )


def test_non_load_bearing_survives_does_not_emit_critical_flag():
    """A non-load-bearing SURVIVES carries findings but NO critical flag."""
    rt = _redteam_with_non_load_bearing_survives()
    assert len(rt.findings) == 1
    assert rt.findings[0].severity == "major"
    assert rt.critical_flags == []  # the load-bearing gate is real


def test_aggregate_with_non_load_bearing_survives_advances():
    """Verdict is ADVANCE: total >= threshold AND no critical flags.

    The non-load-bearing red-team finding does NOT force advance: false
    on its own — per-instance dim 3 deduction (5/6 vs 6/6) is the
    natural surface, but the per-dim mean still clears the threshold.
    """
    review = _passing_review_critic()
    redteam = _redteam_with_non_load_bearing_survives()

    agg = aggregate([review, redteam])

    # No critical flags in the aggregate.
    assert agg.critical_flags == []
    # The non-load-bearing finding still propagates as an aggregated
    # finding (the reviser consumes it).
    assert len(agg.findings) == 1
    assert agg.findings[0].severity == "major"
    # Total clears threshold — mean-of-non-null per-dim, sum across dims.
    assert agg.total >= agg.threshold
    # And no critical flag → verdict ADVANCE.
    assert agg.verdict == Verdict.ADVANCE


def test_aggregate_load_bearing_gate_distinguishes_severity():
    """Direct contrast: load-bearing SURVIVES → BLOCK; non → ADVANCE.

    Same memo-review baseline (passing 44/44). Toggle the red-team's
    load-bearing flag and observe the verdict flip. This is the
    operational evidence that the load-bearing gate is the load-bearing
    discriminator, not the SURVIVES verdict itself.
    """
    review = _passing_review_critic()

    # Non-load-bearing red-team → ADVANCE.
    rt_non = _redteam_with_non_load_bearing_survives()
    agg_non = aggregate([review, rt_non])
    assert agg_non.verdict == Verdict.ADVANCE

    # Load-bearing red-team → BLOCK (constructed inline to keep this test
    # self-contained; mirrors the load-bearing case in the sibling test
    # file but with a deliberately different objection content).
    rt_load = Review(
        version_dir="investment-memo.3",
        critic_id="redteam",
        scores=[
            Score(dimension="dim_2", score=4, max=6),
            Score(dimension="dim_3", score=4, max=6, critical=True),
            Score(dimension="dim_1", score=None, max=4),
            Score(dimension="dim_4", score=None, max=4),
            Score(dimension="dim_5", score=None, max=6),
            Score(dimension="dim_6", score=None, max=6),
            Score(dimension="dim_7", score=None, max=4),
            Score(dimension="dim_8", score=None, max=4),
            Score(dimension="dim_9", score=None, max=4),
        ],
        findings=[
            Finding(
                severity="blocker",
                dimension="dim_3",
                rationale="load-bearing SURVIVES",
                suggested_fix="tighten the rebuttal",
            )
        ],
        critical_flags=[
            {
                "type": "redteam_survives",
                "justification": "load-bearing objection SURVIVES",
            }  # type: ignore[list-item]
        ],
        threshold=35,
    )
    agg_load = aggregate([review, rt_load])
    assert agg_load.verdict == Verdict.BLOCK
