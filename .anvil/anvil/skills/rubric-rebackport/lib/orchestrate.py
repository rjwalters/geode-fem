"""Top-level command orchestration for `anvil:rubric-rebackport` (issue #358).

Composes :mod:`detect`, :mod:`plan`, :mod:`apply`, and :mod:`verify`
into the operator-facing flows:

- **Dry-run** (`/anvil:rubric-rebackport <tree>`): detect + plan +
  report.
- **Apply** (`/anvil:rubric-rebackport <tree> --apply`): detect +
  plan + report + apply + verify.
- **Report** (`/anvil:rubric-rebackport <tree> --report`): detect +
  plan + report.

The skill's command spec consumes this module's :func:`run` function
as the single entry. Returns a typed :class:`RunResult` and a
markdown report.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Optional

from .apply import ApplyResult, apply_plan
from .detect import (
    ProjectInventory,
    inventory_tree,
)
from .plan import (
    Mode,
    Plan,
    build_plan,
)
from .verify import VerifyResult, verify_rebackport


@dataclass
class RunResult:
    """Typed summary of a :func:`run` invocation.

    Attributes
    ----------
    project_tree
        Absolute path of the project tree.
    mode
        Operator-selected mode.
    plan
        Generated plan (always present, even in dry-run).
    apply_result
        Apply outcome; ``None`` for dry-run / report.
    verify_result
        Verify outcome; ``None`` when apply was skipped.
    report
        Markdown report (printed to stdout by the command).
    success
        True iff the run completed without failures. For dry-run this
        is True when the project tree exists; for apply it's True when
        every planned review either applied successfully or was
        legitimately skipped.
    """

    project_tree: Path
    mode: Mode
    plan: Plan
    apply_result: Optional[ApplyResult] = None
    verify_result: Optional[VerifyResult] = None
    report: str = ""
    success: bool = False


def _format_plan_report(
    project_tree: Path, mode: Mode, plan: Plan, legacy_rubric: Optional[str]
) -> str:
    """Format the plan as a markdown report."""
    lines: List[str] = []
    lines.append(f"# Rubric rebackport: {project_tree.name}")
    lines.append("")
    lines.append(f"**Project tree**: `{project_tree}`")
    lines.append(f"**Mode**: `{mode.value}`")
    lines.append(
        f"**Legacy rubric**: "
        f"`{legacy_rubric}`"
        if legacy_rubric is not None
        else "**Legacy rubric**: (heuristic)"
    )
    lines.append(f"**Reviews in plan**: {len(plan.reviews)}")
    to_change = sum(1 for r in plan.reviews if not r.is_noop and not r.skipped)
    skipped = sum(1 for r in plan.reviews if r.skipped)
    noops = sum(1 for r in plan.reviews if r.is_noop and not r.skipped)
    lines.append(
        f"**Summary**: {to_change} to change, {skipped} skipped, "
        f"{noops} already-current"
    )
    lines.append("")

    if not plan.reviews:
        lines.append("No reviewer-sibling directories found under this tree.")
        lines.append("")
        return "\n".join(lines) + "\n"

    lines.append("## Plan")
    lines.append("")

    for rp in plan.reviews:
        lines.append(f"### `{rp.review_id}`")
        lines.append("")
        if rp.skipped:
            lines.append(f"- SKIPPED: {rp.skip_reason}")
        elif rp.is_noop:
            lines.append("- No actions required (already stamped).")
        else:
            if rp.skill is not None:
                lines.append(f"- Skill: `anvil:{rp.skill}`")
            if rp.rubric is not None:
                lines.append(
                    f"- Target rubric: `{rp.rubric.id}` "
                    f"(total={rp.rubric.total}, "
                    f"threshold={rp.rubric.advance_threshold})"
                )
            if rp.stamp_meta is not None:
                lines.append(
                    f"- Stamp `_meta.json`: rubric_id, rubric_total, "
                    "advance_threshold"
                )
            if rp.stamp_progress_rows is not None:
                lines.append(
                    "- Stamp `_progress.json.metadata.score_history[]` rows: "
                    f"add rubric_id=`{rp.stamp_progress_rows.rubric_id}`"
                )
            if rp.summary_block is not None:
                lines.append(
                    "- Update `_summary.md.rubric` block "
                    f"(dimensions={rp.summary_block.dimensions})"
                )
            if rp.rescore_spec is not None:
                lines.append(
                    f"- Write rescore sidecar: "
                    f"`{rp.rescore_spec.sidecar_path.name}`"
                )
        for note in rp.notes:
            lines.append(f"  - note: {note}")
        lines.append("")

    skipped_entries = [r for r in plan.reviews if r.skipped]
    if skipped_entries:
        lines.append("## Skipped reviews")
        lines.append("")
        for rp in skipped_entries:
            lines.append(f"- `{rp.review_id}`: {rp.skip_reason}")
        lines.append("")

    lines.append("## Verification preview")
    lines.append("")
    lines.append(
        "After apply, every touched `_meta.json` would carry the three "
        "rubric-stamping fields (`rubric_id`, `rubric_total`, "
        "`advance_threshold`)."
    )
    lines.append("")
    return "\n".join(lines) + "\n"


def _format_apply_report(apply_result: ApplyResult) -> str:
    """Format the apply outcome as a markdown report."""
    lines: List[str] = []
    lines.append("## Apply")
    lines.append("")
    lines.append(
        f"- Applied (with changes): {len(apply_result.applied_reviews)}"
    )
    lines.append(
        f"- Deferred (rescore hook absent): "
        f"{len(apply_result.deferred_reviews)}"
    )
    if apply_result.failed_reviews:
        lines.append(
            f"- **Failed**: {len(apply_result.failed_reviews)} reviews:"
        )
        for o in apply_result.failed_reviews:
            lines.append(f"  - `{o.review_id}`: {o.error}")
    lines.append("")
    return "\n".join(lines) + "\n"


def run(
    project_tree: Path,
    *,
    mode: Mode = Mode.STAMP_ONLY,
    legacy_rubric: Optional[str] = None,
    skill_filter: Optional[str] = None,
    apply: bool = False,
    report_only: bool = False,
    allow_rescore_subprocess: bool = True,
) -> RunResult:
    """Execute the rubric-rebackport flow.

    Parameters
    ----------
    project_tree
        Path to the project tree (single thread, project, or portfolio
        root).
    mode
        :class:`Mode.STAMP_ONLY` (default) or :class:`Mode.RESCORE`.
    legacy_rubric
        Operator-asserted legacy rubric id. Required for ``Mode.RESCORE``.
    skill_filter
        Scope to a single skill (optional).
    apply
        When True, run the apply step after detection + planning. When
        False (default), perform a dry-run.
    report_only
        Emit a markdown report and exit. Mutually exclusive with
        ``apply``.
    allow_rescore_subprocess
        Forwarded to :func:`apply.apply_plan`. Defaults to True. Tests
        set False to keep the apply path deterministic without
        spawning reviewer LLMs.

    Returns
    -------
    A :class:`RunResult` carrying the outcome and the formatted report.

    Raises
    ------
    ValueError
        When ``apply`` and ``report_only`` are both True, or when
        ``mode`` is :class:`Mode.RESCORE` without ``legacy_rubric``.
    FileNotFoundError
        When ``project_tree`` does not exist.
    """
    if apply and report_only:
        raise ValueError(
            "apply and report_only are mutually exclusive; pass one or "
            "neither."
        )
    if mode is Mode.RESCORE and (legacy_rubric is None or legacy_rubric == ""):
        raise ValueError(
            "--rescore requires --legacy-rubric=<id>."
        )

    project_tree = Path(project_tree).resolve()
    if not project_tree.is_dir():
        raise FileNotFoundError(
            f"Project tree not found: {project_tree}"
        )

    inv = inventory_tree(project_tree)
    plan = build_plan(
        inv,
        mode=mode,
        legacy_rubric=legacy_rubric,
        skill_filter=skill_filter,
    )

    result = RunResult(
        project_tree=project_tree,
        mode=mode,
        plan=plan,
    )
    report = _format_plan_report(project_tree, mode, plan, legacy_rubric)

    if not apply:
        # Dry-run / report mode.
        result.report = report
        result.success = True
        return result

    # Apply mode.
    apply_result = apply_plan(
        plan,
        allow_rescore_subprocess=allow_rescore_subprocess,
    )
    result.apply_result = apply_result
    report += "\n" + _format_apply_report(apply_result)

    # Verify only if no apply failures.
    if not apply_result.failed_reviews:
        verify_result = verify_rebackport(project_tree, plan)
        result.verify_result = verify_result
        report += "\n" + verify_result.to_report()
        # Success when verify passes AND no deferred rescores in
        # rescore mode (deferred is a real signal the operator needs
        # to see, but it doesn't fail the run — only mutation errors
        # do).
        result.success = verify_result.ok
    else:
        result.success = False

    result.report = report
    return result


__all__ = ["RunResult", "run"]
