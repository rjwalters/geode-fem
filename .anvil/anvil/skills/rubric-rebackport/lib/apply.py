"""Plan execution for `anvil:rubric-rebackport` (issue #358).

Takes a :class:`Plan` (from :mod:`plan`) and executes it against the
filesystem. The execution is:

- **Atomic per review** — each ``ReviewPlan`` is snapshotted before
  apply, then either succeeds entirely or rolls back from the snapshot.
- **Subprocess-free for stamping** — ``--stamp-only`` mode does pure
  file rewrites under :mod:`stamp`.
- **Subprocess'd for rescore** — ``--rescore`` mode delegates to
  :mod:`rescore` which can shell out to the per-skill reviewer command
  (or record the rescore as deferred when the hook is absent).

The apply module is the only module in the skill that mutates disk.
All mutation policy (rollback, atomicity) lives here.

Public API
----------

- ``ApplyResult`` — typed summary of an apply run.
- ``apply_plan(plan, *, allow_rescore_subprocess=True)`` — execute a
  plan.
"""

from __future__ import annotations

import shutil
from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Optional, Tuple

from .plan import (
    Mode,
    Plan,
    ReviewPlan,
)
from .rescore import (
    RescoreOutcome,
    invoke_rescore,
)
from .stamp import (
    apply_stamp_meta,
    apply_stamp_progress_rows,
    apply_summary_rubric_block,
)


# Subdirectory under the project tree root used for per-review
# snapshots during apply. Removed on successful apply.
ROLLBACK_SUBDIR = ".anvil-rebackport-rollback"


@dataclass
class ReviewOutcome:
    """Per-review apply outcome."""

    review_id: str
    stamped_meta: bool = False
    stamped_progress_rows: int = 0
    updated_summary_block: bool = False
    rescore_outcome: Optional[RescoreOutcome] = None
    skipped: bool = False
    skip_reason: Optional[str] = None
    error: Optional[str] = None
    notes: List[str] = field(default_factory=list)


@dataclass
class ApplyResult:
    """Typed summary of an apply run.

    Attributes
    ----------
    outcomes
        Per-review outcomes, in plan order.
    rollback_root_cleaned
        True iff the per-run rollback root was removed at the end of
        apply (it is when every review succeeded; left in place when
        any review failed).
    """

    outcomes: List[ReviewOutcome] = field(default_factory=list)
    rollback_root_cleaned: bool = False

    @property
    def failed_reviews(self) -> List[ReviewOutcome]:
        return [o for o in self.outcomes if o.error is not None]

    @property
    def deferred_reviews(self) -> List[ReviewOutcome]:
        return [
            o for o in self.outcomes
            if o.rescore_outcome is not None and o.rescore_outcome.deferred
        ]

    @property
    def applied_reviews(self) -> List[ReviewOutcome]:
        return [
            o for o in self.outcomes
            if (
                o.error is None
                and not o.skipped
                and (
                    o.stamped_meta
                    or o.stamped_progress_rows
                    or o.updated_summary_block
                    or (o.rescore_outcome is not None and o.rescore_outcome.written)
                )
            )
        ]


# ---------------------------------------------------------------------------
# Snapshot / rollback
# ---------------------------------------------------------------------------


def _snapshot_paths_for_review(plan_entry: ReviewPlan) -> List[Path]:
    """Return the paths to snapshot before applying ``plan_entry``.

    For stamp-only: the review dir (for ``_meta.json``), the sibling
    ``_progress.json`` (when scheduled for stamping), and the sibling
    ``_summary.md`` (when scheduled for block update).

    For rescore: nothing — the sidecar path doesn't exist yet, and the
    legacy review dir is untouched. (We still snapshot zero paths so
    the apply step's rollback machinery has a uniform interface.)
    """
    paths: List[Path] = []
    if plan_entry.mode is Mode.STAMP_ONLY:
        if plan_entry.stamp_meta is not None:
            paths.append(plan_entry.stamp_meta.meta_path)
        if plan_entry.stamp_progress_rows is not None:
            paths.append(plan_entry.stamp_progress_rows.progress_path)
        if plan_entry.summary_block is not None:
            paths.append(plan_entry.summary_block.summary_path)
    return paths


def _take_snapshot(
    plan_entry: ReviewPlan, rollback_root: Path
) -> Path:
    """Snapshot the files touched by ``plan_entry`` for rollback.

    Returns the snapshot directory path. The snapshot is a flat copy of
    each touched file under
    ``<rollback_root>/<safe-review-id>/<sanitized-rel-path>``.
    """
    snapshot_dir = rollback_root / _safe_review_id(plan_entry.review_id)
    snapshot_dir.mkdir(parents=True, exist_ok=True)
    for src in _snapshot_paths_for_review(plan_entry):
        if not src.is_file():
            continue
        # Use the source's basename as the snapshot key. Multiple files
        # with the same basename are disambiguated by appending the
        # source's parent dirname.
        dest = snapshot_dir / src.name
        if dest.exists():
            parent_name = src.parent.name
            dest = snapshot_dir / f"{parent_name}__{src.name}"
        shutil.copy2(src, dest)
    return snapshot_dir


def _restore_snapshot(
    plan_entry: ReviewPlan, snapshot_dir: Path
) -> None:
    """Restore files from ``snapshot_dir`` to their original locations."""
    for src in _snapshot_paths_for_review(plan_entry):
        candidate = snapshot_dir / src.name
        if not candidate.is_file():
            parent_name = src.parent.name
            candidate = snapshot_dir / f"{parent_name}__{src.name}"
        if not candidate.is_file():
            continue
        try:
            shutil.copy2(candidate, src)
        except OSError:
            # Best-effort rollback.
            pass


def _safe_review_id(review_id: str) -> str:
    """Turn ``review_id`` into a filesystem-safe directory name."""
    return review_id.replace("/", "__").replace("\\", "__")


# ---------------------------------------------------------------------------
# Per-review apply
# ---------------------------------------------------------------------------


def _apply_stamp_only_review(
    plan_entry: ReviewPlan,
) -> ReviewOutcome:
    """Execute the stamp-only edits for one review (no snapshot/rollback)."""
    outcome = ReviewOutcome(review_id=plan_entry.review_id)

    if plan_entry.stamp_meta is not None:
        changed, err = apply_stamp_meta(plan_entry.stamp_meta)
        if err is not None:
            outcome.error = f"stamp_meta failed: {err}"
            return outcome
        outcome.stamped_meta = changed

    if plan_entry.stamp_progress_rows is not None:
        rows, err = apply_stamp_progress_rows(plan_entry.stamp_progress_rows)
        if err is not None:
            outcome.error = f"stamp_progress_rows failed: {err}"
            return outcome
        outcome.stamped_progress_rows = rows

    if plan_entry.summary_block is not None:
        changed, err = apply_summary_rubric_block(plan_entry.summary_block)
        if err is not None:
            outcome.error = f"summary_block failed: {err}"
            return outcome
        outcome.updated_summary_block = changed

    return outcome


def _apply_rescore_review(
    plan_entry: ReviewPlan, *, allow_subprocess: bool
) -> ReviewOutcome:
    """Execute the rescore mode for one review."""
    outcome = ReviewOutcome(review_id=plan_entry.review_id)
    if plan_entry.rescore_spec is None:
        # Either fully skipped or no-op (sidecar already exists).
        return outcome
    rescore_outcome = invoke_rescore(
        plan_entry.rescore_spec,
        allow_subprocess=allow_subprocess,
    )
    outcome.rescore_outcome = rescore_outcome
    if rescore_outcome.error is not None:
        outcome.error = rescore_outcome.error
    return outcome


def _apply_review_with_snapshot(
    plan_entry: ReviewPlan,
    rollback_root: Path,
    *,
    allow_rescore_subprocess: bool,
) -> ReviewOutcome:
    """Apply one ReviewPlan with snapshot + rollback semantics."""
    if plan_entry.skipped:
        return ReviewOutcome(
            review_id=plan_entry.review_id,
            skipped=True,
            skip_reason=plan_entry.skip_reason,
            notes=list(plan_entry.notes),
        )
    if plan_entry.is_noop:
        return ReviewOutcome(
            review_id=plan_entry.review_id,
            notes=list(plan_entry.notes),
        )

    snapshot_dir = _take_snapshot(plan_entry, rollback_root)
    try:
        if plan_entry.mode is Mode.STAMP_ONLY:
            outcome = _apply_stamp_only_review(plan_entry)
        else:
            outcome = _apply_rescore_review(
                plan_entry,
                allow_subprocess=allow_rescore_subprocess,
            )
        if outcome.error is not None:
            # Roll back.
            _restore_snapshot(plan_entry, snapshot_dir)
            return outcome
        # Success — remove snapshot.
        shutil.rmtree(snapshot_dir, ignore_errors=True)
        return outcome
    except Exception as exc:
        # Unexpected error — roll back.
        try:
            _restore_snapshot(plan_entry, snapshot_dir)
        except Exception:
            pass
        shutil.rmtree(snapshot_dir, ignore_errors=True)
        return ReviewOutcome(
            review_id=plan_entry.review_id,
            error=f"unexpected error: {exc}",
        )


# ---------------------------------------------------------------------------
# Top-level apply
# ---------------------------------------------------------------------------


def apply_plan(
    plan: Plan,
    *,
    allow_rescore_subprocess: bool = True,
) -> ApplyResult:
    """Execute ``plan`` against the filesystem.

    Parameters
    ----------
    plan
        The plan to execute.
    allow_rescore_subprocess
        When False, every rescore is recorded as deferred (no on-disk
        sidecar creation). Tests use this to keep the apply path
        deterministic.
    """
    result = ApplyResult()

    if plan.is_noop:
        for r in plan.reviews:
            result.outcomes.append(
                ReviewOutcome(
                    review_id=r.review_id,
                    skipped=r.skipped,
                    skip_reason=r.skip_reason,
                    notes=list(r.notes),
                )
            )
        result.rollback_root_cleaned = True
        return result

    rollback_root = plan.project_tree / ROLLBACK_SUBDIR
    rollback_root.mkdir(parents=True, exist_ok=True)

    for review_plan in plan.reviews:
        outcome = _apply_review_with_snapshot(
            review_plan,
            rollback_root,
            allow_rescore_subprocess=allow_rescore_subprocess,
        )
        outcome.notes.extend(review_plan.notes)
        result.outcomes.append(outcome)

    # Clean up the rollback root if empty.
    try:
        if rollback_root.is_dir() and not any(rollback_root.iterdir()):
            rollback_root.rmdir()
            result.rollback_root_cleaned = True
    except OSError:
        pass

    return result


__all__ = [
    "ApplyResult",
    "ROLLBACK_SUBDIR",
    "ReviewOutcome",
    "apply_plan",
]
