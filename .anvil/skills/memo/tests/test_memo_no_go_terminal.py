"""Contract tests for the memo NO-GO terminal state (issue #559).

The NO-GO terminal sink is reviewer-prose-and-policy at the memo skill
boundary (see ``anvil/skills/memo/SKILL.md`` §"NO-GO terminal state",
``anvil/skills/memo/commands/memo-review.md`` step 6 / step 7 / step 10,
and ``anvil/skills/memo/commands/memo-revise.md`` step 4) — there is no
new Python module to test directly. What this file covers is the
**contract surface** the LLM-driven memo commands rely on:

1. The canonical NO-GO ``verdict.md`` shape (§"NO-GO terminal state")
   parses cleanly through the framework-side helpers
   ``parse_memo_verdict_no_go`` and ``parse_memo_verdict_kill_rationale``
   and round-trips through ``_adapt_memo_legacy`` to ``Verdict.NO_GO``.
2. The ``_progress.json`` audit-trail field shapes documented in
   ``anvil/lib/snippets/progress.md`` §"`metadata.kill_rationale`" and
   §"`metadata.no_go_overridden` + `metadata.no_go_override_reason`"
   are additive optional fields that pre-#559 readers tolerate via the
   shallow-merge contract (no Pydantic schema, just JSON; the test
   exercises JSON round-trip + the documented absence-tolerance).
3. The ``no_go`` ``CriticalFlag.type`` value composes through the
   existing aggregator pathway — a memo-review critic that emits a
   ``no_go`` flag (per the step-6 ``_classify_no_go_eligibility``
   policy) AND a separate red-team critic that emits
   ``redteam_survives`` (per PR #573) BOTH show up in the aggregated
   verdict, and the aggregated verdict is ``Verdict.NO_GO``. This is
   the load-bearing Path A composition: red-team unchanged + memo-review
   promotes load-bearing SURVIVES → no_go at iteration-budget exhaustion.
4. The NO-GO ``verdict.md`` shape WITHOUT a ``Decision: advance:`` line
   continues to parse — ``parse_memo_verdict_decision`` returns ``None``
   on NO-GO prose (the legacy adapter's pre-#559 advance-line path is
   inactive for NO-GO sidecars).

Per the per-skill test filename convention (#58 — distinct filenames
across skills, ``__init__.py`` chains in every test dir), this file is
named ``test_memo_no_go_terminal.py``.
"""

from __future__ import annotations

import json
from pathlib import Path

from anvil.lib.critics import (
    _adapt_memo_legacy,
    aggregate,
    parse_memo_verdict_decision,
    parse_memo_verdict_kill_rationale,
    parse_memo_verdict_no_go,
)
from anvil.lib.review_schema import (
    CriticalFlag,
    Review,
    Score,
    Verdict,
)


# ---------------------------------------------------------------------------
# Canonical NO-GO verdict.md shape, mirroring SKILL.md §"NO-GO terminal state".
# This is the exact shape memo-review writes at step 10 when the step-6
# _classify_no_go_eligibility policy fires.
# ---------------------------------------------------------------------------

CANONICAL_NO_GO_VERDICT_MD = """\
# NO-GO — terminal

**Verdict**: NO-GO
**Iteration**: 4
**Triggering flag**: redteam_survives
**Source critic**: memo-redteam

## Kill rationale

The red-team objection that the addressable market is two orders of magnitude smaller than the memo's TAM figure (and is structurally constrained by regulatory caps that no execution choice can bypass) SURVIVES four passes of revision. The memo's response in v4 reframes the TAM as a 10-year aspiration but does not reduce the recommendation's funding ask or check size — the thesis as written presupposes a market size that the red-team has demonstrated does not exist.

## Evidence

- investment-memo.4/investment-memo.md:L120-L138 (TAM reframe in v4 §3.2)
- investment-memo.4/investment-memo.md:L210-L225 (recommendation check size unchanged)

## Operator override

To resurrect this thread, run `memo-revise <thread> --override-no-go "<reason>"`.
The override writes a new version dir with `metadata.no_go_overridden = true`
and `metadata.no_go_override_reason = "<verbatim>"`. The NO-GO verdict.md is
preserved as a permanent record of the kill recommendation.
"""

# Standard scoring.md + comments.md for the prose triple — required because
# _adapt_memo_legacy reads all three files. The scoring/comments are
# uninteresting for NO-GO recognition; the NO-GO trigger is the verdict.md
# line.
SCORING_MD = """\
| # | Dimension | Weight | Score | Justification |
|---|-----------|--------|-------|---------------|
| 1 | Recommendation clarity | 5 | 4 | clear recommendation |
| 2 | Thesis coherence | 6 | 4 | thesis stressed by red-team SURVIVES |
"""

COMMENTS_MD = """\
## Severity: blocker

- **blocker**: red-team SURVIVES on load-bearing TAM objection — no-go

## Severity: major

(none)
"""


# ---------------------------------------------------------------------------
# 1. Canonical NO-GO verdict.md shape parses cleanly through the helpers.
# ---------------------------------------------------------------------------


def test_canonical_no_go_verdict_md_is_recognized():
    """The §SKILL.md verdict.md shape is recognized by parse_memo_verdict_no_go."""
    assert parse_memo_verdict_no_go(CANONICAL_NO_GO_VERDICT_MD) is True


def test_canonical_no_go_kill_rationale_is_extracted_verbatim():
    """The kill-rationale paragraph extracts verbatim from the canonical shape."""
    rationale = parse_memo_verdict_kill_rationale(CANONICAL_NO_GO_VERDICT_MD)
    assert rationale is not None
    # Verbatim load-bearing phrases survive the extraction.
    assert "addressable market is two orders of magnitude smaller" in rationale
    assert "SURVIVES four passes of revision" in rationale
    # The extractor stops at the next heading; subsequent sections are excluded.
    assert "Evidence" not in rationale
    assert "Operator override" not in rationale


def test_canonical_no_go_verdict_md_carries_no_advance_decision_line():
    """The NO-GO shape DELIBERATELY omits ``Decision: advance: ...`` — the
    NO-GO branch in _adapt_memo_legacy fires from the ``Verdict: NO-GO``
    line, NOT from a missing-advance fall-through."""
    assert parse_memo_verdict_decision(CANONICAL_NO_GO_VERDICT_MD) is None


def test_legacy_adapter_emits_no_go_verdict_from_canonical_shape(tmp_path):
    """End-to-end: a critic sidecar carrying the canonical NO-GO verdict.md
    + a minimal scoring.md + comments.md adapts to a Review with
    Verdict.NO_GO."""
    critic_dir = tmp_path / "investment-memo.4.review"
    critic_dir.mkdir()
    (critic_dir / "verdict.md").write_text(CANONICAL_NO_GO_VERDICT_MD)
    (critic_dir / "scoring.md").write_text(SCORING_MD)
    (critic_dir / "comments.md").write_text(COMMENTS_MD)

    review = _adapt_memo_legacy(critic_dir)
    assert review.verdict == Verdict.NO_GO


# ---------------------------------------------------------------------------
# 2. _progress.json audit-trail field shapes are additive optional JSON.
#    The shallow-merge contract preserves them; the absence of the fields
#    on a pre-#559 thread is byte-identical to today.
# ---------------------------------------------------------------------------


def test_progress_json_no_go_audit_trail_round_trips(tmp_path):
    """A _progress.json carrying the documented NO-GO audit-trail fields
    round-trips through json.dump / json.load with byte-stable shape."""
    progress = {
        "version": 1,
        "thread": "investment-memo",
        "phases": {
            "review": {
                "state": "done",
                "started": "2026-06-15T14:00:00Z",
                "completed": "2026-06-15T14:12:00Z",
            }
        },
        "metadata": {
            "iteration": 4,
            "max_iterations": 4,
            "kill_rationale": (
                "Load-bearing TAM objection SURVIVES four passes; "
                "recommendation check size unchanged."
            ),
        },
        "termination_reason": "NO_GO",
    }
    path = tmp_path / "_progress.json"
    path.write_text(json.dumps(progress, indent=2))
    loaded = json.loads(path.read_text())

    assert loaded["termination_reason"] == "NO_GO"
    assert "kill_rationale" in loaded["metadata"]
    assert loaded["metadata"]["kill_rationale"].startswith(
        "Load-bearing TAM objection"
    )


def test_progress_json_no_go_override_audit_trail_round_trips(tmp_path):
    """A resurrected version dir carries the override audit-trail fields
    documented in `memo-revise.md` step 5 (`metadata.no_go_overridden`
    + `metadata.no_go_override_reason`); both are absent on the default
    (non-override) path."""
    override_reason = (
        "new evidence: customer Y signed LOI on 2026-06-14 — addresses "
        "redteam objection #2 about adoption traction."
    )
    progress = {
        "version": 1,
        "thread": "investment-memo",
        "phases": {
            "draft": {
                "state": "done",
                "started": "2026-06-15T15:00:00Z",
                "completed": "2026-06-15T15:12:00Z",
            }
        },
        "metadata": {
            "iteration": 5,
            "max_iterations": 5,
            "no_go_overridden": True,
            "no_go_override_reason": override_reason,
        },
    }
    path = tmp_path / "_progress.json"
    path.write_text(json.dumps(progress, indent=2))
    loaded = json.loads(path.read_text())

    assert loaded["metadata"]["no_go_overridden"] is True
    # Verbatim — no normalization / trimming / truncation.
    assert loaded["metadata"]["no_go_override_reason"] == override_reason


def test_progress_json_pre_559_thread_byte_identical_to_today(tmp_path):
    """A pre-#559 _progress.json (no NO-GO fields anywhere) is the legacy
    contract — the additive fields' absence is the default. This pins the
    backwards-compat shape: existing threads must continue to round-trip
    without any NO-GO bookkeeping."""
    progress = {
        "version": 1,
        "thread": "investment-memo",
        "phases": {"draft": {"state": "done"}},
        "metadata": {"iteration": 1, "max_iterations": 4},
    }
    path = tmp_path / "_progress.json"
    path.write_text(json.dumps(progress, indent=2))
    loaded = json.loads(path.read_text())

    assert "termination_reason" not in loaded
    assert "kill_rationale" not in loaded["metadata"]
    assert "no_go_overridden" not in loaded["metadata"]
    assert "no_go_override_reason" not in loaded["metadata"]


# ---------------------------------------------------------------------------
# 3. Path A composition with PR #573: memo-review emits the promoted no_go
#    flag alongside the red-team's preserved redteam_survives flag; the
#    aggregator returns Verdict.NO_GO.
# ---------------------------------------------------------------------------


def _make_memo_review_with_no_go_promotion(
    *,
    version_dir: str = "investment-memo.4",
) -> Review:
    """Build the memo-review Review payload as it would land at iteration 4
    of a max_iterations=4 thread when the step-6 _classify_no_go_eligibility
    policy fires on a load-bearing red-team SURVIVES."""
    return Review(
        version_dir=version_dir,
        critic_id="memo-review",
        scores=[
            Score(dimension="dim_1", score=4, max=5),
            Score(dimension="dim_2", score=4, max=6),
            Score(dimension="dim_3", score=3, max=6),
            Score(dimension="dim_4", score=4, max=5),
            Score(dimension="dim_5", score=3, max=4),
            Score(dimension="dim_6", score=4, max=5),
            Score(dimension="dim_7", score=4, max=4),
            Score(dimension="dim_8", score=4, max=5),
            Score(dimension="dim_9", score=3, max=4),
        ],
        critical_flags=[
            CriticalFlag(
                type="no_go",
                justification=(
                    "Load-bearing red-team objection (TAM two orders of "
                    "magnitude smaller than memo's figure) SURVIVES four "
                    "passes of revision; iteration budget exhausted."
                ),
                evidence_span=(
                    "investment-memo.4/investment-memo.md:L120-L138"
                ),
            ),
        ],
        threshold=35,
    )


def _make_redteam_review_with_survives(
    *,
    version_dir: str = "investment-memo.4",
) -> Review:
    """Build the red-team Review payload as it lands per PR #573 — a
    load-bearing redteam_survives critical flag, unchanged by #559."""
    return Review(
        version_dir=version_dir,
        critic_id="memo-redteam",
        scores=[
            # Red-team owns dim 2 + dim 3; other dims null per
            # mean-of-non-null aggregation.
            Score(dimension="dim_1", score=None, max=5),
            Score(dimension="dim_2", score=4, max=6),
            Score(dimension="dim_3", score=3, max=6),
            Score(dimension="dim_4", score=None, max=5),
            Score(dimension="dim_5", score=None, max=4),
            Score(dimension="dim_6", score=None, max=5),
            Score(dimension="dim_7", score=None, max=4),
            Score(dimension="dim_8", score=None, max=5),
            Score(dimension="dim_9", score=None, max=4),
        ],
        critical_flags=[
            CriticalFlag(
                type="redteam_survives",
                justification=(
                    "TAM objection on regulatory cap SURVIVES; the memo's "
                    "response reframes but does not engage the regulatory "
                    "constraint."
                ),
            ),
        ],
    )


def test_aggregator_returns_no_go_when_memo_review_promotes_to_no_go():
    """The Path A composition: memo-review's step-6 promotion writes a
    no_go critical flag alongside the red-team's redteam_survives. The
    aggregator unions both flags and returns Verdict.NO_GO."""
    memo_review = _make_memo_review_with_no_go_promotion()
    redteam_review = _make_redteam_review_with_survives()
    agg = aggregate([memo_review, redteam_review])

    assert agg.verdict == Verdict.NO_GO

    # Both flags survive in the aggregated list — the audit trail records
    # the underlying signal AND the policy decision to escalate.
    types = {cf.type for cf in agg.critical_flags}
    assert "no_go" in types
    assert "redteam_survives" in types


def test_aggregator_does_not_emit_no_go_without_step_6_promotion():
    """A redteam_survives flag alone (without memo-review's promotion to
    no_go) routes to Verdict.BLOCK — the pre-#559 behavior. The step-6
    promotion lives in memo-review markdown spec, not in the aggregator
    or the red-team critic; this test pins the no-promotion-no-NO-GO
    contract that keeps escalation policy out of the critic layer."""
    redteam_review = _make_redteam_review_with_survives()
    # Memo-review WITHOUT the step-6 promotion — no no_go flag emitted.
    memo_review_no_promotion = Review(
        version_dir="investment-memo.4",
        critic_id="memo-review",
        scores=[
            Score(dimension="dim_1", score=4, max=5),
            Score(dimension="dim_2", score=4, max=6),
            Score(dimension="dim_3", score=3, max=6),
            Score(dimension="dim_4", score=4, max=5),
            Score(dimension="dim_5", score=3, max=4),
            Score(dimension="dim_6", score=4, max=5),
            Score(dimension="dim_7", score=4, max=4),
            Score(dimension="dim_8", score=4, max=5),
            Score(dimension="dim_9", score=3, max=4),
        ],
        critical_flags=[],  # no no_go promotion
        threshold=35,
    )
    agg = aggregate([memo_review_no_promotion, redteam_review])

    assert agg.verdict == Verdict.BLOCK
    types = {cf.type for cf in agg.critical_flags}
    assert "no_go" not in types
    assert "redteam_survives" in types
