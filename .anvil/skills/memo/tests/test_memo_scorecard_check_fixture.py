"""End-to-end fixture test for scorecard arithmetic validation (issue #392).

Reproduces the studio canary shape: ``investment-memo.5.review/`` recorded
a verdict of **44/44** (``advance: true``) while its own ``scoring.md``
weight table summed to **48**. The malformed scorecard survived a full
revision cycle undetected — the thread was treated as READY at a score
that was arithmetically impossible under the canonical /44 rubric.

``anvil/lib/scorecard_check.py::check_review_dir`` must flag the canary
sidecar and return zero findings for the known-good sibling
(``investment-memo.6.review/``, 41/44).

Per the per-skill test filename convention (#58 — distinct filenames
across skills, ``__init__.py`` chains in every test dir), this file is
named ``test_memo_scorecard_check_fixture.py``.
"""

from __future__ import annotations

from pathlib import Path

from anvil.lib.scorecard_check import (
    ADVANCE_INCONSISTENT,
    SEVERITY_ERROR,
    TOTAL_MISMATCH,
    WEIGHTS_SUM_MISMATCH,
    check_review_dir,
)

FIXTURES = Path(__file__).parent / "fixtures" / "scorecard_check"
CANARY_DIR = FIXTURES / "investment-memo.5.review"
GOOD_DIR = FIXTURES / "investment-memo.6.review"


def test_canary_48_weight_table_under_44_verdict_is_flagged():
    """The studio canary sidecar produces the weights_sum_mismatch finding."""
    findings = check_review_dir(CANARY_DIR)
    codes = [f.code for f in findings]
    assert WEIGHTS_SUM_MISMATCH in codes
    mismatch = next(f for f in findings if f.code == WEIGHTS_SUM_MISMATCH)
    assert mismatch.severity == SEVERITY_ERROR
    assert mismatch.compact == "weights_sum_mismatch: 48 != 44"


def test_canary_declared_total_disagrees_with_table_sum():
    """The declared 44 cannot equal the 48-point table the verdict sits on."""
    findings = check_review_dir(CANARY_DIR)
    mismatch = next(
        f for f in findings if f.code == TOTAL_MISMATCH
    )
    assert mismatch.detail == "declared 44 != computed 48"


def test_canary_findings_are_errors_at_read_time():
    """Read-time consumers see error-severity findings and must treat the
    sidecar's verdict as advisory (the sidecar itself is immutable — the
    check never mutates it)."""
    before = sorted(p.name for p in CANARY_DIR.iterdir())
    findings = check_review_dir(CANARY_DIR)
    after = sorted(p.name for p in CANARY_DIR.iterdir())
    assert before == after, "check_review_dir must not mutate the sidecar"
    assert any(f.severity == SEVERITY_ERROR for f in findings)
    # The canary's advance: true rests on the malformed scorecard — but
    # its declared 44 >= 35 and no critical flags, so the inconsistency
    # surfaces through the arithmetic findings, not the advance check.
    assert ADVANCE_INCONSISTENT not in [f.code for f in findings]


def test_known_good_review_zero_findings():
    """AC6: a well-formed review produces zero findings — byte-identical
    downstream behavior."""
    assert check_review_dir(GOOD_DIR) == []
