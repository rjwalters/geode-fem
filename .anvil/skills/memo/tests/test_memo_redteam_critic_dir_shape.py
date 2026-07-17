"""Discovery + aggregation tests for the memo red-team critic sibling (issue #560).

The red-team critic (``anvil/skills/memo/commands/memo-redteam.md``) is a
new opt-in critic sibling at ``<thread>.{N}.redteam/`` that consumes the
existing canonical ``_review.json`` schema (``anvil/lib/review_schema.py``)
and is discovered by ``anvil/lib/critics.py::discover_critics`` without
any aggregator change. The new ``CriticalFlag.type`` vocabulary values —
``"redteam_survives"`` and ``"redteam_unengaged"`` — are skill-defined per
the schema (the ``type`` field on ``CriticalFlag`` is documented as
"Skill-defined; the lib does not enforce a vocabulary"), so the schema
accepts them without a version bump.

This test builds a synthetic ``<thread>.{N}.redteam/`` sibling with a
known ``_review.json`` payload carrying one load-bearing ``SURVIVES``
critical flag, then asserts:

1. ``discover_critics`` finds the sibling alongside the standard
   ``.review/`` sibling.
2. ``load_review`` parses the red-team's ``_review.json`` without error.
3. ``aggregate`` returns ``Verdict.BLOCK`` with the red-team's flag in the
   merged ``critical_flags`` list — regardless of whether the rubric
   total clears threshold.

The contract here is "discovery is filesystem-only; aggregator unions
critical flags across critics; the red-team plugs into the existing
pathway with zero aggregator change."

Per the per-skill test filename convention (#58 — distinct filenames
across skills, ``__init__.py`` chains in every test dir), this file is
named ``test_memo_redteam_critic_dir_shape.py``.
"""

from __future__ import annotations

import json
from pathlib import Path

from anvil.lib.critics import (
    CANONICAL_REVIEW_FILENAME,
    aggregate,
    discover_critics,
    load_review,
)
from anvil.lib.review_schema import (
    CriticalFlag,
    Finding,
    Review,
    Score,
    Verdict,
)


def _make_redteam_review(
    *,
    version_dir: str = "investment-memo.3",
    load_bearing_survives: bool = True,
) -> Review:
    """Build a canonical red-team Review payload.

    The red-team owns dim 2 + dim 3 (the two dims a kill-case attacks);
    other dims carry ``score: None`` per the aggregator's mean-of-non-null
    contract. When ``load_bearing_survives=True``, the review carries one
    load-bearing ``SURVIVES`` finding + the matching ``redteam_survives``
    critical flag.
    """
    scores = [
        # dim 2 + dim 3: owned by the red-team
        Score(
            dimension="dim_2",
            score=4 if load_bearing_survives else 6,
            max=6,
            critical=False,
            justification="thesis coherence under adversarial reading",
        ),
        Score(
            dimension="dim_3",
            score=4 if load_bearing_survives else 6,
            max=6,
            critical=load_bearing_survives,  # mirror the per-instance ladder
            justification=(
                "Objection 1 (mask cost dominates unit economics) SURVIVES "
                "— -2 + critical flag"
                if load_bearing_survives
                else "all red-team objections DEFEATED"
            ),
        ),
        # all other dims: not owned by the red-team
        Score(dimension="dim_1", score=None, max=4),
        Score(dimension="dim_4", score=None, max=4),
        Score(dimension="dim_5", score=None, max=6),
        Score(dimension="dim_6", score=None, max=6),
        Score(dimension="dim_7", score=None, max=4),
        Score(dimension="dim_8", score=None, max=4),
        Score(dimension="dim_9", score=None, max=4),
    ]
    findings = []
    critical_flags = []
    if load_bearing_survives:
        findings.append(
            Finding(
                severity="blocker",
                dimension="dim_3",
                evidence_span="investment-memo.md:L120-L140",
                rationale=(
                    "Objection 1 (FinFET mask cost dominates Pericles.3 unit "
                    "economics) is load-bearing for the recommendation; the "
                    "memo's rebuttal in section 5 leans on a comparable that "
                    "does not survive scrutiny."
                ),
                suggested_fix=(
                    "Either model the mask cost explicitly in section 6 or "
                    "explicitly scope it out of the recommendation."
                ),
            )
        )
        critical_flags.append(
            CriticalFlag(
                type="redteam_survives",
                justification=(
                    "Load-bearing objection (mask cost dominates unit "
                    "economics) SURVIVES the memo's rebuttal — the comparable "
                    "the memo leans on does not hold under adversarial "
                    "scrutiny; the recommendation depends on the rebuttal "
                    "winning."
                ),
                evidence_span="investment-memo.md:L120-L140",
            )
        )

    return Review(
        version_dir=version_dir,
        critic_id="redteam",
        scores=scores,
        findings=findings,
        critical_flags=critical_flags,
        threshold=35,
        rubric="anvil-memo-v2",
    )


def _make_review_critic(
    *,
    version_dir: str = "investment-memo.3",
    total_clears_threshold: bool = True,
) -> Review:
    """Build a passable memo-review payload.

    Used as a co-critic in aggregation tests: the red-team's critical
    flag should force ``Verdict.BLOCK`` regardless of whether the
    memo-review's per-dim total clears the threshold.
    """
    # Score chosen so the memo-review's total reads as a passing 36/44
    # (above the 35 threshold) when ``total_clears_threshold=True``. When
    # the test wants a sub-threshold total, halve the per-dim scores.
    s = 4 if total_clears_threshold else 1
    return Review(
        version_dir=version_dir,
        critic_id="review",
        scores=[
            Score(dimension="dim_1", score=s, max=4),
            Score(dimension="dim_2", score=6 if total_clears_threshold else 2, max=6),
            Score(dimension="dim_3", score=6 if total_clears_threshold else 2, max=6),
            Score(dimension="dim_4", score=s, max=4),
            Score(dimension="dim_5", score=6 if total_clears_threshold else 2, max=6),
            Score(dimension="dim_6", score=6 if total_clears_threshold else 2, max=6),
            Score(dimension="dim_7", score=s, max=4),
            Score(dimension="dim_8", score=s, max=4),
            Score(dimension="dim_9", score=s, max=4),
        ],
        threshold=35,
        rubric="anvil-memo-v2",
    )


def _write_sidecar(critic_dir: Path, review: Review) -> None:
    critic_dir.mkdir(parents=True, exist_ok=True)
    (critic_dir / CANONICAL_REVIEW_FILENAME).write_text(
        review.model_dump_json(indent=2)
    )


def test_discover_finds_redteam_sibling(tmp_path):
    """discover_critics finds <thread>.{N}.redteam/ alongside .review/."""
    (tmp_path / "investment-memo.3").mkdir()
    _write_sidecar(tmp_path / "investment-memo.3.review", _make_review_critic())
    _write_sidecar(tmp_path / "investment-memo.3.redteam", _make_redteam_review())

    siblings = discover_critics(tmp_path / "investment-memo.3")
    names = sorted(s.name for s in siblings)
    assert names == [
        "investment-memo.3.redteam",
        "investment-memo.3.review",
    ]


def test_load_review_parses_redteam_review_json(tmp_path):
    """load_review parses a red-team _review.json without error."""
    redteam_dir = tmp_path / "investment-memo.3.redteam"
    _write_sidecar(redteam_dir, _make_redteam_review())

    loaded = load_review(redteam_dir)
    assert loaded.critic_id == "redteam"
    assert loaded.version_dir == "investment-memo.3"
    assert loaded.rubric == "anvil-memo-v2"
    # One load-bearing SURVIVES → one critical flag of type redteam_survives.
    assert len(loaded.critical_flags) == 1
    assert loaded.critical_flags[0].type == "redteam_survives"
    # One blocker finding.
    assert len(loaded.findings) == 1
    assert loaded.findings[0].severity == "blocker"
    assert loaded.findings[0].dimension == "dim_3"


def test_aggregate_unions_redteam_flag_forces_block(tmp_path):
    """Aggregating memo-review + red-team produces Verdict.BLOCK.

    The red-team's redteam_survives critical flag enters the union at
    aggregate-time; the existing ``compute_verdict`` rule
    ("any critical flag → BLOCK regardless of total") fires.
    """
    review = _make_review_critic(total_clears_threshold=True)  # clean 36/44
    redteam = _make_redteam_review(load_bearing_survives=True)

    agg = aggregate([review, redteam])

    # Red-team's critical flag is unioned in.
    flag_types = sorted(f.type for f in agg.critical_flags)
    assert "redteam_survives" in flag_types
    # Verdict is BLOCK regardless of the memo-review's passing 36/44.
    assert agg.verdict == Verdict.BLOCK
    # Both critics' ids surface in the audit trail.
    assert sorted(agg.critic_ids) == ["redteam", "review"]


def test_aggregate_block_even_when_review_critic_passes(tmp_path):
    """The red-team's flag dominates even when memo-review is clean.

    Regression guard: a memo-review that would otherwise advance is
    blocked by a load-bearing redteam_survives flag — the load-bearing
    gate is real.
    """
    # memo-review at 44/44 with no findings — the maximally-clean case.
    review = Review(
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
    )
    redteam = _make_redteam_review(load_bearing_survives=True)

    agg = aggregate([review, redteam])

    # The aggregate total is dominated by the mean-of-non-null per-dim
    # rule (red-team's dim 3 = 4 pulls the dim 3 mean below the review's
    # 6/6 — 5/6 after rounding) but it remains > threshold. The verdict
    # is still BLOCK because of the critical flag.
    assert agg.total >= agg.threshold  # not blocked by total
    assert agg.verdict == Verdict.BLOCK  # blocked by critical flag


def test_redteam_review_round_trips_through_json(tmp_path):
    """Schema round-trip: build, write, load, validate."""
    original = _make_redteam_review()
    sidecar = tmp_path / "investment-memo.3.redteam"
    _write_sidecar(sidecar, original)

    # Reload via filesystem discovery + load_review.
    siblings = discover_critics(tmp_path / "investment-memo.3")
    assert siblings == [sidecar]

    loaded = load_review(sidecar)
    # Reserialize and recompare structurally (critical flag types preserved,
    # finding severity preserved, version_dir preserved).
    assert loaded.critic_id == original.critic_id
    assert loaded.version_dir == original.version_dir
    assert (
        sorted(f.type for f in loaded.critical_flags)
        == sorted(f.type for f in original.critical_flags)
    )
    # Validate the raw JSON on disk parses cleanly via the schema directly.
    raw = json.loads(
        (sidecar / CANONICAL_REVIEW_FILENAME).read_text()
    )
    re_validated = Review.model_validate(raw)
    assert re_validated.critic_id == "redteam"
