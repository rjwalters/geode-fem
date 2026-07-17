"""Programmatic project-tree builders for `anvil:project-book` tests.

Constructs project trees in tmp dirs: a ``BRIEF.md`` (with an optional
``build:`` block), per-thread version dirs carrying a chapter file +
``_progress.json``, and optional ``.review`` / ``.audit`` critic siblings
with a minimal canonical ``_review.json`` + version-stamped ``_meta.json``.

Kept dependency-light: pure ``pathlib`` + ``json``, no anvil imports, so
the fixtures never fight the skill-lib loader for import order.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import List, Optional

import yaml


def write_brief(
    project_dir: Path,
    *,
    project: str,
    documents: List[dict],
    build: Optional[dict] = None,
) -> Path:
    """Write ``<project_dir>/BRIEF.md`` with the given frontmatter."""
    project_dir.mkdir(parents=True, exist_ok=True)
    fm: dict = {"project": project, "documents": documents}
    if build is not None:
        fm["build"] = build
    text = "---\n" + yaml.safe_dump(fm, sort_keys=False) + "---\n\n# " + project + "\n"
    brief = project_dir / "BRIEF.md"
    brief.write_text(text, encoding="utf-8")
    return brief


def _write_progress(version_dir: Path, phases: List[str]) -> None:
    block = {
        "version": 1,
        "thread": version_dir.parent.name,
        "phases": {
            name: {
                "state": "done",
                "started": "2026-06-01T00:00:00Z",
                "completed": "2026-06-01T00:00:00Z",
            }
            for name in phases
        },
    }
    (version_dir / "_progress.json").write_text(
        json.dumps(block, indent=2) + "\n", encoding="utf-8"
    )


def _write_review_sibling(
    thread_dir: Path,
    slug: str,
    n: int,
    *,
    tag: str,
    score: Optional[int],
    rubric_total: int,
    advance_threshold: int,
    critical: bool = False,
) -> Path:
    review_dir = thread_dir / f"{slug}.{n}.{tag}"
    review_dir.mkdir(parents=True, exist_ok=True)
    scores = []
    if score is not None:
        scores = [
            {
                "dimension": "overall",
                "score": score,
                "max": rubric_total,
                "justification": "fixture score",
            }
        ]
    review = {
        "schema_version": "1",
        "kind": "judgment",
        "version_dir": f"{slug}.{n}",
        "critic_id": tag,
        "scores": scores,
        "findings": [],
        "critical_flags": (
            [{"type": "fixture", "justification": "flagged in fixture"}]
            if critical
            else []
        ),
        "total": score,
        "threshold": advance_threshold,
    }
    if not scores:
        # An honest unscored stub requires unscored=True + empty scores.
        review["unscored"] = True
    (review_dir / "_review.json").write_text(
        json.dumps(review, indent=2) + "\n", encoding="utf-8"
    )
    meta = {
        "critic": tag,
        "role": f"memo-{tag}.md",
        "schema_version": 1,
        "scorecard_kind": "human-verdict",
        "rubric_id": "anvil-memo-v2",
        "rubric_total": rubric_total,
        "advance_threshold": advance_threshold,
    }
    (review_dir / "_meta.json").write_text(
        json.dumps(meta, indent=2) + "\n", encoding="utf-8"
    )
    return review_dir


def make_thread(
    project_dir: Path,
    slug: str,
    *,
    version: Optional[int] = 1,
    chapter: bool = True,
    chapter_filename: str = "chapter.tex",
    chapter_body: Optional[str] = None,
    phases: Optional[List[str]] = None,
    review_score: Optional[int] = None,
    rubric_total: int = 44,
    advance_threshold: int = 39,
    audit: Optional[str] = None,
) -> Path:
    """Create a thread directory under ``project_dir``.

    ``version=None`` → EMPTY thread (dir exists, no version dirs).
    ``chapter=False`` → version dir exists but lacks the chapter file.
    ``review_score`` → write a ``.review`` sibling scored to that total.
    ``audit`` in {``"clean"``, ``"flagged"``} → write a ``.audit`` sibling.
    """
    thread_dir = project_dir / slug
    thread_dir.mkdir(parents=True, exist_ok=True)
    if version is None:
        return thread_dir

    version_dir = thread_dir / f"{slug}.{version}"
    version_dir.mkdir(parents=True, exist_ok=True)
    if chapter:
        body = chapter_body if chapter_body is not None else (
            f"\\chapter{{{slug}}}\nReal content for {slug}.\n"
        )
        (version_dir / chapter_filename).write_text(body, encoding="utf-8")
    _write_progress(version_dir, phases or ["draft"])

    if review_score is not None:
        _write_review_sibling(
            thread_dir,
            slug,
            version,
            tag="review",
            score=review_score,
            rubric_total=rubric_total,
            advance_threshold=advance_threshold,
        )
    if audit is not None:
        _write_review_sibling(
            thread_dir,
            slug,
            version,
            tag="audit",
            score=None,
            rubric_total=rubric_total,
            advance_threshold=advance_threshold,
            critical=(audit == "flagged"),
        )
    return thread_dir


def minimal_master_tex() -> str:
    """A minimal master doc that \\input's each staged chapter.

    Uses ``article`` + ``\\input`` (rather than ``memoir`` + ``\\include``)
    so the fixture compiles under a bare TeX install without extra
    classes. Chapters are staged as ``chapters/<slug>.tex``.
    """
    return (
        "\\documentclass{article}\n"
        "\\begin{document}\n"
        "\\tableofcontents\n"
        "% chapters are \\input by the build; a smoke fixture lists them\n"
        "\\end{document}\n"
    )


def default_documents(
    slugs: List[str], artifact_type: str = "investment-memo"
) -> List[dict]:
    return [{"slug": s, "artifact_type": artifact_type} for s in slugs]


__all__ = [
    "default_documents",
    "make_thread",
    "minimal_master_tex",
    "write_brief",
]
