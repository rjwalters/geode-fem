"""Per-thread state collection for `anvil:project-book` (issue #596).

For each chapter slug, resolve the thread's current version dir via the
canonical ``.latest`` resolver
(``anvil/lib/latest_resolution.py::resolve_latest``), derive its
lifecycle state, locate the highest-N ``.review`` / ``.audit`` critic
siblings, read the review score, and compute the recommended next
command. The result feeds both the staging step (which files to copy /
placeholder) and the build report.

State-aware **but warn-never-block**: a thread that has no version dirs
(EMPTY), or whose resolved version dir lacks the configured chapter
file, is not an error — it is recorded on the returned
:class:`ThreadInfo` (``needs_placeholder=True``) so the staging step can
emit a placeholder chapter and the report can flag it. The compile never
hard-fails on a missing thread.

Score reading composes the framework critic primitives: the highest-N
``<slug>.<N>.review/`` sibling is loaded via
``anvil/lib/critics.py::load_review`` + ``aggregate`` for the numerator,
and its ``_meta.json`` supplies ``rubric_total`` (the denominator) and
``advance_threshold`` (the below-threshold warning boundary). Every read
is failure-tolerant — a malformed or unreadable critic dir degrades the
score to ``None`` (shown as ``—``) rather than raising.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Optional

from anvil.lib.latest_resolution import resolve_latest

# Lifecycle state labels. Ordered by lifecycle rank (low → high); the
# ranking drives both the "furthest stage reached" derivation and the
# READY/AUDITED "quality-clear" boundary.
STATE_EMPTY = "EMPTY"
STATE_DRAFTED = "DRAFTED"
STATE_REVIEWED = "REVIEWED"
STATE_REVISED = "REVISED"
STATE_READY = "READY"
STATE_AUDITED = "AUDITED"
STATE_UNKNOWN = "UNKNOWN"

# States that clear the "not READY/AUDITED" quality warning (AC 3).
_QUALITY_CLEAR_STATES = frozenset({STATE_READY, STATE_AUDITED})

# Version-dir ``_progress.json`` phase → lifecycle rank. Skill-agnostic:
# any skill's phase names that match these keys advance the derived
# state. Unknown phase names are ignored (they never lower the state).
_PHASE_RANK = {
    "draft": 1,
    "review": 2,
    "revise": 3,
    "ready": 4,
    "final": 4,
    "finalize": 4,
    "audit": 5,
}
_RANK_STATE = {
    1: STATE_DRAFTED,
    2: STATE_REVIEWED,
    3: STATE_REVISED,
    4: STATE_READY,
    5: STATE_AUDITED,
}


@dataclass
class ThreadInfo:
    """Collected state for one chapter slug.

    Attributes
    ----------
    slug
        The chapter/thread slug.
    thread_dir
        ``<project>/<slug>/`` (may not exist for unstarted threads).
    resolved_dir
        The **dereferenced** concrete version directory, or ``None``
        when the thread is EMPTY (no version dirs).
    resolved_name
        The concrete version-dir name (``00-introduction.5``) or
        ``None`` when EMPTY.
    chapter_source
        Absolute path to the ``chapter_filename`` inside the resolved
        version dir when it exists; ``None`` otherwise.
    state
        Lifecycle state label (one of the ``STATE_*`` constants).
    score
        Aggregated review numerator (e.g. ``41``), or ``None`` when no
        readable review sibling exists.
    rubric_total
        Review denominator from the review sibling's ``_meta.json``
        (e.g. ``44``), or ``None``.
    advance_threshold
        The below-threshold warning boundary from ``_meta.json``, or
        ``None``.
    audit_state
        ``"clean"`` / ``"flagged"`` / ``"present"`` when an ``.audit``
        sibling exists, else ``None`` (rendered as ``—``).
    next_command
        Recommended next lifecycle command string, or ``None`` when the
        thread is quality-clear (rendered as ``—``).
    needs_placeholder
        True when the staging step must generate a placeholder chapter
        (EMPTY thread OR resolved version dir lacks the chapter file).
    warnings
        Build warnings recorded for the report (never block the compile).
    """

    slug: str
    thread_dir: Path
    resolved_dir: Optional[Path] = None
    resolved_name: Optional[str] = None
    chapter_source: Optional[Path] = None
    state: str = STATE_EMPTY
    score: Optional[int] = None
    rubric_total: Optional[int] = None
    advance_threshold: Optional[int] = None
    audit_state: Optional[str] = None
    next_command: Optional[str] = None
    needs_placeholder: bool = True
    warnings: List[str] = field(default_factory=list)


def _highest_sibling(thread_dir: Path, slug: str, tag: str) -> Optional[Path]:
    """Return the highest-N ``<slug>.<N>.<tag>/`` sibling dir, or ``None``.

    Walk-to-highest, mirroring ``resolve_latest``'s step 3. Non-throwing:
    a missing/unreadable ``thread_dir`` yields ``None``.
    """
    pattern = re.compile(rf"^{re.escape(slug)}\.(\d+)\.{re.escape(tag)}$")
    best_n = -1
    best: Optional[Path] = None
    try:
        children = list(thread_dir.iterdir())
    except OSError:
        return None
    for child in children:
        try:
            if not child.is_dir():
                continue
        except OSError:
            continue
        m = pattern.match(child.name)
        if m is None:
            continue
        n = int(m.group(1))
        if n > best_n:
            best_n = n
            best = child
    return best


def _read_progress_state(version_dir: Path) -> str:
    """Derive a lifecycle state from a version dir's ``_progress.json``.

    A top-level ``state`` string wins when present and recognized.
    Otherwise the furthest completed phase (``state == "done"``) is
    mapped via :data:`_PHASE_RANK`. A version dir with no readable
    progress file is treated as :data:`STATE_DRAFTED` (it has content on
    disk), degrading to :data:`STATE_UNKNOWN` only when the directory
    itself cannot be read.
    """
    progress = version_dir / "_progress.json"
    if not progress.is_file():
        return STATE_DRAFTED
    try:
        data = json.loads(progress.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return STATE_UNKNOWN
    if not isinstance(data, dict):
        return STATE_UNKNOWN

    top = data.get("state")
    if isinstance(top, str) and top.strip().upper() in _RANK_STATE.values():
        return top.strip().upper()

    phases = data.get("phases")
    best_rank = 0
    if isinstance(phases, dict):
        for name, block in phases.items():
            if not isinstance(block, dict):
                continue
            if block.get("state") != "done":
                continue
            rank = _PHASE_RANK.get(str(name).strip().lower())
            if rank is not None and rank > best_rank:
                best_rank = rank
    if best_rank == 0:
        return STATE_DRAFTED
    return _RANK_STATE[best_rank]


def _read_review_score(review_dir: Path) -> tuple[Optional[int], Optional[int], Optional[int]]:
    """Return ``(score, rubric_total, advance_threshold)`` for a review dir.

    The numerator comes from ``critics.load_review`` + ``aggregate`` (so
    both the canonical ``_review.json`` and the legacy prose triple are
    read via one primitive); the denominator + threshold come from the
    sibling ``_meta.json`` version stamp (v0.4.0). Every read is
    failure-tolerant — any error degrades the affected value to ``None``.
    """
    score: Optional[int] = None
    rubric_total: Optional[int] = None
    advance_threshold: Optional[int] = None

    meta = review_dir / "_meta.json"
    if meta.is_file():
        try:
            meta_data = json.loads(meta.read_text(encoding="utf-8"))
            if isinstance(meta_data, dict):
                rt = meta_data.get("rubric_total")
                at = meta_data.get("advance_threshold")
                if isinstance(rt, int):
                    rubric_total = rt
                if isinstance(at, int):
                    advance_threshold = at
        except (OSError, ValueError):
            pass

    try:
        # Imported lazily so a critics-side import problem can never break
        # the whole collect pass; the numerator just degrades to None.
        from anvil.lib.critics import aggregate, load_review

        review = load_review(review_dir)
        agg = aggregate([review])
        if agg.total is not None:
            score = agg.total
    except Exception:
        score = None

    return score, rubric_total, advance_threshold


def _read_audit_state(audit_dir: Path) -> str:
    """Return ``"clean"`` / ``"flagged"`` / ``"present"`` for an audit dir.

    ``clean`` when the audit review loads and carries no critical flags;
    ``flagged`` when it carries at least one; ``present`` when the dir
    exists but cannot be parsed (still surfaced so the report shows an
    audit ran).
    """
    try:
        from anvil.lib.critics import load_review

        review = load_review(audit_dir)
        return "flagged" if review.critical_flags else "clean"
    except Exception:
        return "present"


def _next_command(state: str, artifact_type: Optional[str]) -> Optional[str]:
    """Recommended next lifecycle command for a thread state.

    Uses the BRIEF ``artifact_type`` to name the skill namespace (e.g.
    ``/anvil:memo revise``). ``None`` (rendered ``—``) when the thread is
    quality-clear (READY / AUDITED).
    """
    ns = f"/anvil:{artifact_type}" if artifact_type else "/anvil:<skill>"
    if state == STATE_EMPTY:
        return f"{ns} draft"
    if state in (STATE_DRAFTED, STATE_UNKNOWN):
        return f"{ns} review"
    if state == STATE_REVIEWED:
        return f"{ns} revise"
    if state == STATE_REVISED:
        return f"{ns} review"
    if state == STATE_READY:
        return f"{ns} audit"
    # AUDITED — quality-clear, nothing recommended.
    return None


def collect_thread(
    project_dir: Path,
    slug: str,
    *,
    chapter_filename: str,
    artifact_type: Optional[str] = None,
) -> ThreadInfo:
    """Collect one chapter thread's state. Never raises on per-thread issues."""
    project_dir = Path(project_dir)
    thread_dir = project_dir / slug
    info = ThreadInfo(slug=slug, thread_dir=thread_dir)

    resolved = None
    if thread_dir.is_dir():
        raw = resolve_latest(thread_dir, slug)
        if raw is not None:
            deref = raw.resolve()
            if deref.is_dir():
                resolved = deref

    if resolved is None:
        # EMPTY thread — no version dirs (or a dangling .latest with no
        # fallback). Placeholder chapter; warn but never block.
        info.state = STATE_EMPTY
        info.needs_placeholder = True
        info.next_command = _next_command(STATE_EMPTY, artifact_type)
        info.warnings.append(
            f"`{slug}`: no version directory found (EMPTY) — a placeholder "
            f"chapter was staged so the book still compiles."
        )
        return info

    info.resolved_dir = resolved
    info.resolved_name = resolved.name
    info.state = _read_progress_state(resolved)

    # Chapter source file inside the resolved version dir.
    chapter_source = resolved / chapter_filename
    if chapter_source.is_file():
        info.chapter_source = chapter_source
        info.needs_placeholder = False
    else:
        info.needs_placeholder = True
        info.warnings.append(
            f"`{slug}`: resolved version `{resolved.name}` has no "
            f"`{chapter_filename}` — a placeholder chapter was staged."
        )

    # Score from the highest-N review sibling.
    review_dir = _highest_sibling(thread_dir, slug, "review")
    if review_dir is not None:
        score, rubric_total, advance_threshold = _read_review_score(review_dir)
        info.score = score
        info.rubric_total = rubric_total
        info.advance_threshold = advance_threshold

    # Audit state from the highest-N audit sibling.
    audit_dir = _highest_sibling(thread_dir, slug, "audit")
    if audit_dir is not None:
        info.audit_state = _read_audit_state(audit_dir)
        # A clean audit promotes the reported state to AUDITED (the
        # version-dir progress rarely records the audit phase — it lives
        # in the sibling). A flagged audit leaves the underlying state
        # intact (the thread has not converged; the report warns).
        if info.audit_state == "clean" and info.state in (
            STATE_DRAFTED,
            STATE_REVIEWED,
            STATE_REVISED,
            STATE_READY,
        ):
            info.state = STATE_AUDITED

    # Quality warnings (recorded, never blocking).
    if info.state not in _QUALITY_CLEAR_STATES:
        info.warnings.append(
            f"`{slug}`: state is {info.state} (not READY/AUDITED) — chapter "
            f"included but has not converged."
        )
    if (
        info.score is not None
        and info.advance_threshold is not None
        and info.score < info.advance_threshold
    ):
        info.warnings.append(
            f"`{slug}`: review score {info.score}/"
            f"{info.rubric_total or '?'} is below the advance threshold "
            f"{info.advance_threshold}."
        )

    info.next_command = _next_command(info.state, artifact_type)
    return info


__all__ = [
    "STATE_AUDITED",
    "STATE_DRAFTED",
    "STATE_EMPTY",
    "STATE_READY",
    "STATE_REVIEWED",
    "STATE_REVISED",
    "STATE_UNKNOWN",
    "ThreadInfo",
    "collect_thread",
]
