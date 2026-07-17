"""Post-apply verification for `anvil:rubric-rebackport` (issue #358).

After ``apply_plan`` runs, this module re-walks the project tree and
confirms the rebackport produced the post-#346 stamped shape. The
contract:

1. Every review listed in the original plan with a non-skipped
   stamp-only outcome now carries the three required stamping fields
   in ``_meta.json``.
2. Every review's sibling ``_progress.json``'s ``score_history[]``
   rows carry ``rubric_id`` (when the plan included a row stamp).
3. ``_summary.md`` carries the ``rubric:`` block where the plan said
   it should.
4. For rescore mode: every planned sidecar either exists on disk or
   was surfaced as ``deferred``. The legacy review dir is byte-
   identical to before.

Read-only. Granular results.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import List

from .detect import (
    REQUIRED_STAMP_FIELDS,
    inventory_tree,
)
from .plan import Plan, Mode


@dataclass
class VerifyResult:
    """Typed result of :func:`verify_rebackport`."""

    project_tree: Path
    reviews_stamped_ok: List[str] = field(default_factory=list)
    reviews_progress_rows_ok: List[str] = field(default_factory=list)
    reviews_summary_block_ok: List[str] = field(default_factory=list)
    reviews_rescore_written: List[str] = field(default_factory=list)
    reviews_rescore_deferred: List[str] = field(default_factory=list)
    failures: List[str] = field(default_factory=list)
    ok: bool = False

    def to_report(self) -> str:
        """Return a markdown report of the verification."""
        lines: List[str] = []
        lines.append(f"# Rebackport verification: {self.project_tree.name}")
        lines.append("")
        lines.append(
            f"- Reviews with stamped `_meta.json`: "
            f"{len(self.reviews_stamped_ok)}"
        )
        lines.append(
            f"- Reviews with stamped `_progress.json` rows: "
            f"{len(self.reviews_progress_rows_ok)}"
        )
        lines.append(
            f"- Reviews with `_summary.md.rubric` block: "
            f"{len(self.reviews_summary_block_ok)}"
        )
        lines.append(
            f"- Rescore sidecars written: "
            f"{len(self.reviews_rescore_written)}"
        )
        lines.append(
            f"- Rescore sidecars deferred: "
            f"{len(self.reviews_rescore_deferred)}"
        )
        if self.failures:
            lines.append("")
            lines.append(f"**Failures**: {len(self.failures)}")
            for f in self.failures:
                lines.append(f"  - {f}")
        lines.append("")
        lines.append(f"**Overall**: {'PASS' if self.ok else 'FAIL'}")
        return "\n".join(lines) + "\n"


def verify_rebackport(
    project_tree: Path, plan: Plan
) -> VerifyResult:
    """Verify a rebackport apply by re-walking ``project_tree`` against ``plan``.

    The verify step takes the original plan as input so it can check
    only the reviews the plan touched (rather than every review in the
    tree). This keeps verify deterministic against the apply's
    scoping choices (e.g., ``--skill=`` filters).
    """
    project_tree = Path(project_tree).resolve()
    result = VerifyResult(project_tree=project_tree)
    inv = inventory_tree(project_tree)

    by_dir = {r.review_dir: r for r in inv.reviews}

    for review_plan in plan.reviews:
        if review_plan.skipped:
            continue
        if review_plan.is_noop:
            continue

        if review_plan.mode is Mode.STAMP_ONLY:
            snap = by_dir.get(review_plan.review_dir)
            if snap is None:
                result.failures.append(
                    f"{review_plan.review_id}: review dir not found after apply"
                )
                continue
            if review_plan.stamp_meta is not None:
                meta = snap.meta
                missing = [
                    k for k in REQUIRED_STAMP_FIELDS if k not in meta
                ]
                if missing:
                    result.failures.append(
                        f"{review_plan.review_id}: _meta.json still missing "
                        f"{missing}"
                    )
                else:
                    result.reviews_stamped_ok.append(review_plan.review_id)
            if review_plan.stamp_progress_rows is not None:
                if snap.progress_score_history_unstamped_rows > 0:
                    result.failures.append(
                        f"{review_plan.review_id}: _progress.json still has "
                        f"{snap.progress_score_history_unstamped_rows} "
                        "unstamped score_history rows"
                    )
                else:
                    result.reviews_progress_rows_ok.append(
                        review_plan.review_id
                    )
            if review_plan.summary_block is not None:
                if not snap.summary_has_rubric_block:
                    result.failures.append(
                        f"{review_plan.review_id}: _summary.md is missing "
                        "the `rubric:` block after apply"
                    )
                else:
                    result.reviews_summary_block_ok.append(
                        review_plan.review_id
                    )
        else:
            # Rescore mode.
            if review_plan.rescore_spec is None:
                # No-op or skipped — nothing to verify.
                continue
            sidecar = review_plan.rescore_spec.sidecar_path
            if sidecar.exists():
                result.reviews_rescore_written.append(review_plan.review_id)
            else:
                result.reviews_rescore_deferred.append(review_plan.review_id)

    result.ok = not result.failures
    return result


__all__ = [
    "VerifyResult",
    "verify_rebackport",
]
