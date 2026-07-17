"""Top-level command orchestration for `anvil:project-book` (issue #596).

Composes :mod:`config`, :mod:`collect`, :mod:`stage`, :mod:`compile`, and
:mod:`report` into the operator-facing flow:

- **Build** (``/anvil:project-book <project>``): load the ``build:``
  config + BRIEF ``documents:``, resolve ordering, collect per-thread
  state, marker-guarded blow-away rebuild of the chapters dir (stage
  chapters + placeholders), two-pass XeLaTeX compile of the consumer
  ``master_doc`` → ``out_pdf``, write ``BOOK_REPORT.md``.
- **Dry-run** (``--dry-run``): everything up to (but not including) the
  first write — the full per-thread plan is rendered and returned, the
  project tree stays byte-identical.

The build never blocks on quality: EMPTY / below-READY / below-threshold
threads warn in the report but always produce a compilable book (via
placeholder chapters). The only hard failures are structural: a
marker-guard refusal, a path collision, an ``order`` slug missing from
``documents:``, or xelatex absent (the compile is skipped with a
remediation, staging preserved).
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Optional, Tuple

from anvil.lib.project_brief import load_project_brief_strict

from .collect import ThreadInfo, collect_thread
from .compile import CompileResult, compile_master
from .config import REPORT_FILENAME, BookConfig, load_book_config
from .report import render_report, write_report
from .stage import StageResult, gitignore_suggestion, stage_chapters

# Ordering-source labels recorded in the report.
ORDERING_BUILD_ORDER = "`build.order` (BRIEF override)"
ORDERING_DOCUMENTS = "`documents:` (BRIEF default)"


@dataclass
class RunResult:
    """Typed summary of a :func:`run` invocation."""

    project_dir: Path
    config: BookConfig
    threads: List[ThreadInfo] = field(default_factory=list)
    excluded_slugs: List[str] = field(default_factory=list)
    ordering_source: str = ORDERING_DOCUMENTS
    stage_result: Optional[StageResult] = None
    compile_result: Optional[CompileResult] = None
    report_path: Optional[Path] = None
    report: str = ""
    gitignore_note: Optional[str] = None
    success: bool = False


def resolve_ordering(
    brief_slugs: List[str], config: BookConfig
) -> Tuple[List[str], List[str], str]:
    """Apply ``build.order`` semantics.

    Returns ``(ordered_slugs, excluded_slugs, ordering_source)``.

    Raises
    ------
    ValueError
        When ``build.order`` names a slug that does not appear in the
        BRIEF ``documents:`` list (hard error naming the slug).
    """
    if config.order is None:
        return list(brief_slugs), [], ORDERING_DOCUMENTS
    known = set(brief_slugs)
    unknown = [s for s in config.order if s not in known]
    if unknown:
        raise ValueError(
            f"build.order names slugs that do not appear in "
            f"BRIEF.documents: {unknown}. Suggested fix: remove the unknown "
            f"entries or add matching `documents:` entries."
        )
    ordered = list(config.order)
    excluded = [s for s in brief_slugs if s not in set(ordered)]
    return ordered, excluded, ORDERING_BUILD_ORDER


def _is_within(child: Path, parent: Path) -> bool:
    """True when ``child`` equals or is nested under ``parent``."""
    try:
        child_r = child.resolve()
        parent_r = parent.resolve()
    except OSError:
        child_r, parent_r = child, parent
    try:
        return os.path.commonpath([child_r, parent_r]) == str(parent_r)
    except ValueError:
        # Different drives / anchors — cannot be within.
        return False


def check_collisions(
    project_dir: Path, brief_slugs: List[str], config: BookConfig
) -> None:
    """Refuse configs whose blow-away chapters dir or output PDF would
    clobber source-of-truth.

    Raises ``ValueError`` on:
    - ``chapters_dir`` equal to / containing a document thread dir (the
      blow-away rebuild would delete a thread).
    - ``master_doc`` nested inside ``chapters_dir`` (it would be deleted
      before it could compile).
    - ``out_pdf`` nested inside ``chapters_dir`` (it would be deleted).
    - ``chapters_dir`` equal to the project root.
    """
    project_dir = Path(project_dir)
    chapters_dir = project_dir / config.chapters_dir

    if chapters_dir.resolve() == project_dir.resolve():
        raise ValueError(
            "build.chapters_dir must not resolve to the project root — the "
            "marker-guarded rebuild would attempt to delete the whole "
            "project."
        )

    for slug in brief_slugs:
        thread_dir = project_dir / slug
        if _is_within(thread_dir, chapters_dir):
            raise ValueError(
                f"build.chapters_dir ({config.chapters_dir!r}) contains the "
                f"`{slug}` thread directory; the blow-away rebuild would "
                f"delete source-of-truth. Suggested fix: point chapters_dir "
                f"at a dedicated build path like `book/chapters`."
            )

    if config.master_doc is not None:
        master = project_dir / config.master_doc
        if _is_within(master, chapters_dir):
            raise ValueError(
                f"build.master_doc ({config.master_doc!r}) is inside "
                f"build.chapters_dir ({config.chapters_dir!r}); it would be "
                f"deleted by the blow-away rebuild before it could compile. "
                f"Keep the master document outside the chapters dir."
            )

    out_pdf = project_dir / config.resolved_out_pdf()
    if _is_within(out_pdf, chapters_dir):
        raise ValueError(
            f"build.out_pdf ({config.resolved_out_pdf()!r}) is inside "
            f"build.chapters_dir ({config.chapters_dir!r}); it would be "
            f"deleted by the blow-away rebuild. Point out_pdf outside the "
            f"chapters dir."
        )


def run(
    project_dir: Path,
    *,
    dry_run: bool = False,
    pdfinfo_path: Optional[str] = None,
) -> RunResult:
    """Execute the project-book flow.

    Parameters
    ----------
    project_dir
        The project root (the directory carrying ``BRIEF.md``).
    dry_run
        When True, collect + plan + report only — writes nothing.
    pdfinfo_path
        Testability override forwarded to the compile gate.

    Returns
    -------
    RunResult
        Carrying the per-thread collection, staging/compile outcomes, and
        the rendered (and, in apply mode, written) report.

    Raises
    ------
    FileNotFoundError
        When ``project_dir`` or its ``BRIEF.md`` does not exist.
    ValueError
        On a malformed BRIEF / ``build:`` block, a ``build.order`` slug
        not in ``documents:``, or a chapters-dir / out-pdf collision.
    """
    project_dir = Path(project_dir).resolve()
    if not project_dir.is_dir():
        raise FileNotFoundError(f"Project root not found: {project_dir}")

    config = load_book_config(project_dir)
    brief = load_project_brief_strict(project_dir)
    brief_slugs = [doc.slug for doc in brief.documents]

    ordered, excluded, ordering_source = resolve_ordering(brief_slugs, config)
    check_collisions(project_dir, brief_slugs, config)

    result = RunResult(
        project_dir=project_dir,
        config=config,
        excluded_slugs=excluded,
        ordering_source=ordering_source,
    )

    for slug in ordered:
        doc = brief.document_for_slug(slug)
        artifact_type: Optional[str] = None
        if doc is not None:
            at = doc.artifact_type
            # Registered types are str-enum members (``str(member)`` leaks
            # the ``ArtifactType.X`` class-qualified name on Python < 3.12);
            # consumer-declared types stay plain strings. Prefer ``.value``.
            artifact_type = getattr(at, "value", None) or str(at)
        info = collect_thread(
            project_dir,
            slug,
            chapter_filename=config.chapter_filename,
            artifact_type=artifact_type,
        )
        result.threads.append(info)

    out_pdf_rel = config.resolved_out_pdf()

    if dry_run:
        result.report = render_report(
            project_name=brief.project,
            threads=result.threads,
            ordering_source=ordering_source,
            excluded_slugs=excluded,
            master_doc=config.master_doc,
            out_pdf=out_pdf_rel,
            compile_result=None,
            gitignore_note=None,
        )
        # Dry-run success mirrors apply-mode's non-structural tolerance:
        # placeholders and quality warnings do not make it fail.
        result.success = True
        return result

    # --- Apply -------------------------------------------------------------
    chapters_dir = project_dir / config.chapters_dir
    stage_result = stage_chapters(chapters_dir, result.threads)
    result.stage_result = stage_result

    compile_result: Optional[CompileResult] = None
    if not stage_result.refused and config.master_doc is not None:
        master = project_dir / config.master_doc
        out_pdf = project_dir / out_pdf_rel
        compile_result = compile_master(
            master,
            out_pdf,
            extra_source_paths=list(stage_result.staged),
            pdfinfo_path=pdfinfo_path,
        )
        result.compile_result = compile_result

    result.gitignore_note = gitignore_suggestion(project_dir, config.chapters_dir)

    report = render_report(
        project_name=brief.project,
        threads=result.threads,
        ordering_source=ordering_source,
        excluded_slugs=excluded,
        master_doc=config.master_doc,
        out_pdf=out_pdf_rel,
        compile_result=compile_result,
        refusal_reason=stage_result.refusal_reason,
        gitignore_note=result.gitignore_note,
    )
    result.report = report
    result.report_path = write_report(project_dir, REPORT_FILENAME, report)

    # Success is structural-only. Placeholders / below-READY / below-
    # threshold threads never fail the run (build-does-not-block-on-
    # quality). Failures: marker-guard refusal, xelatex absent, or a gate
    # failure on the master compile.
    if stage_result.refused:
        result.success = False
    elif compile_result is not None and compile_result.xelatex_missing:
        result.success = False
    elif compile_result is not None and not compile_result.ok:
        result.success = False
    else:
        result.success = True

    return result


__all__ = [
    "ORDERING_BUILD_ORDER",
    "ORDERING_DOCUMENTS",
    "RunResult",
    "check_collisions",
    "resolve_ordering",
    "run",
]
