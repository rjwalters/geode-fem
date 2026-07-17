"""Chapter staging for `anvil:project-book` (issue #596).

Blow-away-rebuilds the configured ``chapters_dir`` and stages one
``<slug>.tex`` per chapter into it — copying the resolved version dir's
chapter file when present, or generating a minimal placeholder chapter
when the thread is EMPTY or its resolved version dir lacks the chapter
file. The placeholder guarantees the consumer's master document always
compiles, even during early project phases.

Safety: the rebuild is **marker-guarded** exactly like
``project-share``. The chapters dir is deleted and recreated only when
it is absent, empty, or carries the ``.anvil-book-build`` marker from a
previous run. A non-empty chapters dir WITHOUT the marker is a hard
refusal — no deletion, no partial write. Because the rebuild is a full
blow-away, stale chapter files from threads removed from ``order``
between runs disappear by construction (the idempotence AC).
"""

from __future__ import annotations

import re
import shutil
from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Optional

from .collect import ThreadInfo
from .config import MARKER_FILENAME

# Chapters-dir states reported by :func:`inspect_chapters_dir`.
CHAPTERS_ABSENT = "absent"
CHAPTERS_EMPTY = "empty"
CHAPTERS_MARKER = "marker"
CHAPTERS_FOREIGN = "foreign"

# Comment marker written at the top of every generated placeholder
# chapter so the report (and a human reading the staged tree) can tell a
# placeholder apart from a real chapter.
PLACEHOLDER_MARKER = "% anvil:project-book placeholder chapter"


@dataclass
class StageResult:
    """Outcome of :func:`stage_chapters`."""

    chapters_dir: Path
    refused: bool = False
    refusal_reason: Optional[str] = None
    staged: List[Path] = field(default_factory=list)
    placeholders: List[str] = field(default_factory=list)
    marker_path: Optional[Path] = None


def _latex_escape_texttt(slug: str) -> str:
    """Escape the LaTeX specials that can appear in a filesystem slug.

    Slugs are constrained to alphanumerics / hyphens / underscores, so
    only the underscore needs escaping inside ``\\texttt{...}``.
    """
    return slug.replace("_", r"\_")


def slug_title(slug: str) -> str:
    """Human title for a slug: strip a leading numeric ordinal, split on
    ``-``/``_``, title-case the words.

    ``00-introduction`` → ``Introduction``; ``01_childhood_years`` →
    ``Childhood Years``; ``appendix`` → ``Appendix``.
    """
    words = re.split(r"[-_]+", slug)
    # Drop a leading pure-numeric ordinal token (``00`` in ``00-intro``).
    if len(words) > 1 and words[0].isdigit():
        words = words[1:]
    titled = " ".join(w[:1].upper() + w[1:] for w in words if w)
    return titled or slug


def placeholder_chapter(slug: str) -> str:
    """Return the LaTeX body for a placeholder chapter."""
    return (
        f"{PLACEHOLDER_MARKER}\n"
        f"\\chapter{{{slug_title(slug)}}}\n"
        f"\\textit{{[Not started — no draft found for thread "
        f"\\texttt{{{_latex_escape_texttt(slug)}}}.]}}\n"
    )


def inspect_chapters_dir(chapters_dir: Path) -> str:
    """Classify the chapters dir for the marker guard.

    Returns one of :data:`CHAPTERS_ABSENT`, :data:`CHAPTERS_EMPTY`,
    :data:`CHAPTERS_MARKER` (previous build — safe to rebuild), or
    :data:`CHAPTERS_FOREIGN` (non-empty without the marker — refuse).
    """
    chapters_dir = Path(chapters_dir)
    if not chapters_dir.exists():
        return CHAPTERS_ABSENT
    if not chapters_dir.is_dir():
        return CHAPTERS_FOREIGN
    try:
        children = list(chapters_dir.iterdir())
    except OSError:
        return CHAPTERS_FOREIGN
    if not children:
        return CHAPTERS_EMPTY
    if (chapters_dir / MARKER_FILENAME).is_file():
        return CHAPTERS_MARKER
    return CHAPTERS_FOREIGN


def stage_chapters(chapters_dir: Path, threads: List[ThreadInfo]) -> StageResult:
    """Marker-guarded blow-away rebuild of ``chapters_dir``.

    Writes one ``<slug>.tex`` per thread (in list order), copying the
    resolved chapter file or generating a placeholder. Refuses (no
    deletion) when the chapters dir is non-empty and unmarked.
    """
    chapters_dir = Path(chapters_dir)
    result = StageResult(chapters_dir=chapters_dir)

    state = inspect_chapters_dir(chapters_dir)
    if state == CHAPTERS_FOREIGN:
        result.refused = True
        result.refusal_reason = (
            f"Refusing to rebuild {chapters_dir}: the directory is "
            f"non-empty and does not carry the `{MARKER_FILENAME}` marker "
            f"from a previous build. It may not be ours to delete. "
            f"Suggested fix: move or remove the directory, or point "
            f"`build.chapters_dir` at a dedicated path."
        )
        return result

    # Blow away & rebuild: the chapters dir is a disposable build
    # artifact. Stale files from removed/reordered threads disappear by
    # construction.
    if chapters_dir.exists():
        shutil.rmtree(chapters_dir)
    chapters_dir.mkdir(parents=True)

    marker = chapters_dir / MARKER_FILENAME
    marker.write_text(
        "# anvil:project-book staging marker\n"
        "# Presence of this file authorizes the marker-guarded blow-away "
        "rebuild of this directory.\n",
        encoding="utf-8",
    )
    result.marker_path = marker

    for info in threads:
        target = chapters_dir / f"{info.slug}.tex"
        if info.needs_placeholder or info.chapter_source is None:
            target.write_text(placeholder_chapter(info.slug), encoding="utf-8")
            result.placeholders.append(info.slug)
        else:
            shutil.copy2(info.chapter_source, target)
        result.staged.append(target)

    return result


def _gitignore_covers(line: str, rel: str) -> bool:
    """Heuristic: does one .gitignore line cover the chapters-dir path?"""
    stripped = rel.rstrip("/")
    candidates = {
        stripped,
        f"{stripped}/",
        f"/{stripped}",
        f"/{stripped}/",
        f"**/{stripped}",
        f"**/{stripped}/",
    }
    # Also treat a parent-dir ignore (``book/``) as covering
    # ``book/chapters``.
    parent = stripped.split("/")[0]
    candidates.update({parent, f"{parent}/", f"/{parent}", f"/{parent}/"})
    return line in candidates


def gitignore_suggestion(project_dir: Path, chapters_rel: str) -> Optional[str]:
    """One-line suggestion when the chapters dir isn't gitignored.

    Walks up from ``project_dir`` to the enclosing git root, checking
    every ``.gitignore`` on the way. Returns ``None`` when covered or
    when no enclosing git repo exists. Never edits any file (mirrors
    ``project-share``).
    """
    project_dir = Path(project_dir).resolve()
    current = project_dir
    in_repo = False
    for _ in range(64):
        gitignore = current / ".gitignore"
        if gitignore.is_file():
            try:
                for raw in gitignore.read_text(encoding="utf-8").splitlines():
                    line = raw.strip()
                    if not line or line.startswith("#"):
                        continue
                    if _gitignore_covers(line, chapters_rel):
                        return None
            except OSError:
                pass
        if (current / ".git").exists():
            in_repo = True
            break
        if current.parent == current:
            break
        current = current.parent
    if not in_repo:
        return None
    return (
        f"`{chapters_rel}/` is a build artifact and is not covered by your "
        f".gitignore — consider adding `{chapters_rel}/` (and the output "
        f"PDF) to it."
    )


__all__ = [
    "CHAPTERS_ABSENT",
    "CHAPTERS_EMPTY",
    "CHAPTERS_FOREIGN",
    "CHAPTERS_MARKER",
    "PLACEHOLDER_MARKER",
    "StageResult",
    "gitignore_suggestion",
    "inspect_chapters_dir",
    "placeholder_chapter",
    "slug_title",
    "stage_chapters",
]
