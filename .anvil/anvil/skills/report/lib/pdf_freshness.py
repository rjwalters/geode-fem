"""PDF existence + freshness check for ``report-review`` step 4c.

This module implements the deterministic stat-only check documented in
``anvil/skills/report/commands/report-review.md`` step 4c. It is the
load-bearing implementation of the contract documented in
``anvil/skills/report/commands/report-figures.md`` "Validation by file
existence" — the figurer claims the reviewer scores Dimension 7 in
part on whether ``report.pdf`` exists; this module is how the reviewer
actually enforces that.

**Why this exists (and is additive over #64's render-gate)**: the
render-gate at ``anvil/lib/render_gate.py`` is invoked in
``report-review`` step 4b, but step 4b deliberately makes the gate
fail open on a missing ``report.pdf`` — the comment at L50 of
``report-review.md`` is explicit: "the gate fails open with a clear
stdout message ... The review proceeds normally." Separately, the
render-gate has no concept of source/output mtime ordering — so a
stale PDF (figurer ran on version N, then ``report.md`` was edited
in-place without re-running figures) passes 4b cleanly.

This check closes both gaps:

- **Non-fail-open existence**: a missing ``report.pdf`` emits a
  ``major`` finding and caps Dim 7.
- **Freshness (mtime ordering)**: if ``report.pdf`` is older than
  ``report.md``, emits a ``major`` finding and caps Dim 7.

The check is **not** a critical-flag short-circuit. ``major`` severity
at the rubric-cap level is the right calibration: the reviewer can
still substantively evaluate the markdown; the missing/stale PDF
affects ADVANCE via the rubric total (capped Dim 7 ≤ 2/4 contributes
≤ 2 to the /40 total), not via critical-flag short-circuit.

The check is also **mechanical**: no LLM call, no PDF parse, no
network. Pure ``os.stat``.

This module is skill-local rather than a framework primitive in
``anvil/lib/`` because the cap value (2/4) and the finding shape are
report-specific tuning points. If/when a second skill needs the same
primitive, the lift to ``anvil/lib/`` is mechanical.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional


# The Dimension 7 cap when the PDF is missing or stale. The rubric's
# "Scoring guidance" anchors ~50% weight at "partial; one significant
# weakness" — floor(weight/2) = 2 for Dim 7's weight of 4.
DIM7_CAP_WHEN_MISSING_OR_STALE = 2


# The Dimension 7 weight (mirrors ``rubric.md`` row 7).
DIM7_WEIGHT = 4


# The dimension name for the finding. Mirrors the rubric row label and
# the conventions used by other report-skill findings.
DIM7_NAME = "format_presentation_quality"


@dataclass(frozen=True)
class PdfFreshnessFinding:
    """A single Dim-7 finding emitted by the step-4c check.

    The shape mirrors the existing ``Finding`` shape used elsewhere in
    the report skill's ``comments.md`` outputs — no schema change, no
    ``_review.json`` change. Fields:

    - ``severity``: always ``"major"``. The check never emits a
      ``critical`` flag (a missing/stale PDF affects ADVANCE through
      the rubric cap, not through critical-flag short-circuit).
    - ``dimension``: the dimension name (always Dim 7).
    - ``rationale``: prose explanation suitable for ``comments.md``.
    - ``evidence_span``: filesystem evidence pointer (e.g. the
      ``report.pdf`` path, with mtimes when stale).
    - ``suggested_fix``: operator-actionable instruction.
    - ``dim7_cap``: the cap to apply to Dim 7's score (always
      ``DIM7_CAP_WHEN_MISSING_OR_STALE`` when this is non-``None``).
    """

    severity: str
    dimension: str
    rationale: str
    evidence_span: str
    suggested_fix: str
    dim7_cap: int


@dataclass(frozen=True)
class PdfFreshnessResult:
    """The outcome of one step-4c invocation.

    - ``finding`` is ``None`` iff the PDF exists AND is fresher than
      ``report.md`` (the happy path; Dim 7 not capped).
    - ``dim7_cap`` is ``None`` iff there is no cap to apply (the happy
      path); otherwise it is ``DIM7_CAP_WHEN_MISSING_OR_STALE``.
    - ``pdf_exists`` / ``pdf_mtime`` / ``md_mtime`` expose the raw
      stat data for callers that want to log it (e.g. into the
      ``_meta.json`` of the review sibling). Mtimes are POSIX
      timestamps (``float``) or ``None`` when the file is absent.
    """

    finding: Optional[PdfFreshnessFinding]
    dim7_cap: Optional[int]
    pdf_exists: bool
    pdf_mtime: Optional[float]
    md_mtime: Optional[float]


def _iso(ts: float) -> str:
    """Format a POSIX timestamp as ISO-8601 UTC.

    Mirrors ``anvil/lib/snippets/timestamp.md``.
    """
    return (
        datetime.fromtimestamp(ts, tz=timezone.utc)
        .isoformat()
        .replace("+00:00", "Z")
    )


def check_pdf_freshness(
    version_dir: Path,
    *,
    pdf_name: str = "report.pdf",
    md_name: str = "report.md",
) -> PdfFreshnessResult:
    """Check that ``report.pdf`` exists and is newer than ``report.md``.

    This is the load-bearing primitive behind ``report-review`` step
    4c. It is **mechanical**: pure ``Path.exists()`` + ``Path.stat()``
    — no LLM call, no PDF parse, no network. Safe to invoke
    unconditionally as a pre-flight check.

    Args:
        version_dir: the ``<thread>.{N}/`` directory whose ``report.md``
            and ``report.pdf`` are being checked.
        pdf_name: filename of the rendered deliverable; defaults to
            ``report.pdf`` per the figurer contract.
        md_name: filename of the markdown source; defaults to
            ``report.md``.

    Returns:
        A :class:`PdfFreshnessResult`. When the PDF is missing or
        stale, ``result.finding`` is a :class:`PdfFreshnessFinding`
        with severity ``major`` and a Dim-7 cap of
        ``DIM7_CAP_WHEN_MISSING_OR_STALE``. When the PDF exists and is
        fresher than the source, ``result.finding`` is ``None`` and
        Dim-7 is uncapped.

    Behaviour summary (mirrors ``report-review.md`` step 4c):

    - **Missing PDF**: severity ``major``, rationale "Rendered
      deliverable not built — figurer has not run on this version (or
      its output was deleted). Run report-figures before review can
      score Dimension 7 substantively.", evidence_span =
      ``"{version_dir}/{pdf_name}"``, suggested_fix = ``"Run
      report-figures <project>/<thread>"``, cap = 2/4.
    - **Stale PDF** (``pdf_mtime < md_mtime``): severity ``major``,
      rationale "Rendered deliverable is stale — report.md was
      modified after report.pdf was built. The PDF the recipient
      would see does not reflect the current source.", evidence_span
      includes both mtimes (ISO-8601 UTC), suggested_fix = ``"Re-run
      report-figures to refresh the deliverable"``, cap = 2/4.
    - **Fresh PDF** (``pdf_mtime >= md_mtime``): no finding, no cap.
    """
    pdf_path = version_dir / pdf_name
    md_path = version_dir / md_name

    pdf_exists = pdf_path.exists()
    md_mtime: Optional[float] = (
        md_path.stat().st_mtime if md_path.exists() else None
    )
    pdf_mtime: Optional[float] = (
        pdf_path.stat().st_mtime if pdf_exists else None
    )

    if not pdf_exists:
        finding = PdfFreshnessFinding(
            severity="major",
            dimension=DIM7_NAME,
            rationale=(
                "Rendered deliverable not built — figurer has not run "
                "on this version (or its output was deleted). Run "
                "report-figures before review can score Dimension 7 "
                "substantively."
            ),
            evidence_span=f"{version_dir}/{pdf_name}",
            suggested_fix="Run report-figures <project>/<thread>",
            dim7_cap=DIM7_CAP_WHEN_MISSING_OR_STALE,
        )
        return PdfFreshnessResult(
            finding=finding,
            dim7_cap=DIM7_CAP_WHEN_MISSING_OR_STALE,
            pdf_exists=False,
            pdf_mtime=None,
            md_mtime=md_mtime,
        )

    # PDF exists. Check freshness ordering. If md_path is missing we
    # cannot compare — but the reviewer would not be running at all in
    # that case (step 1 of report-review.md requires report.md to
    # exist to discover the version), so this branch is documentation
    # rather than an enforcement path.
    if md_mtime is None:
        return PdfFreshnessResult(
            finding=None,
            dim7_cap=None,
            pdf_exists=True,
            pdf_mtime=pdf_mtime,
            md_mtime=None,
        )

    assert pdf_mtime is not None  # for the type checker
    if pdf_mtime < md_mtime:
        finding = PdfFreshnessFinding(
            severity="major",
            dimension=DIM7_NAME,
            rationale=(
                "Rendered deliverable is stale — report.md was "
                "modified after report.pdf was built. The PDF the "
                "recipient would see does not reflect the current "
                "source."
            ),
            evidence_span=(
                f"{version_dir}/{pdf_name} (mtime: {_iso(pdf_mtime)}) "
                f"older than {version_dir}/{md_name} "
                f"(mtime: {_iso(md_mtime)})"
            ),
            suggested_fix=(
                "Re-run report-figures to refresh the deliverable"
            ),
            dim7_cap=DIM7_CAP_WHEN_MISSING_OR_STALE,
        )
        return PdfFreshnessResult(
            finding=finding,
            dim7_cap=DIM7_CAP_WHEN_MISSING_OR_STALE,
            pdf_exists=True,
            pdf_mtime=pdf_mtime,
            md_mtime=md_mtime,
        )

    # Fresh PDF: no finding, no cap.
    return PdfFreshnessResult(
        finding=None,
        dim7_cap=None,
        pdf_exists=True,
        pdf_mtime=pdf_mtime,
        md_mtime=md_mtime,
    )
