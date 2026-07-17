"""Skill-local build configuration for `anvil:project-book` (issue #596).

Parses the optional ``build:`` block out of the project ``BRIEF.md``
frontmatter into a typed :class:`BookConfig`. Per the curator's design
the parser is **skill-local** — the shared
``anvil/lib/project_brief.py::ProjectBrief`` model is NOT extended
(``ProjectBrief`` is ``extra="forbid"`` on the model but its parse path
explicitly ignores unknown top-level frontmatter keys, so a ``build:``
block is safe to add to a BRIEF today with zero changes to the shared
parser — the same precedent ``project-share``'s ``export:`` block relies
on).

Zero-config contract: a BRIEF with no ``build:`` block at all yields
:class:`BookConfig` defaults — every ``documents:`` entry in BRIEF order,
chapters staged into ``book/chapters/`` as ``<slug>.tex``, and (when a
``master_doc`` is supplied) a compiled ``book/book.pdf``.

Config surface (all fields optional; ``master_doc`` required for compile)::

    build:
      order:                        # authoritative include-list AND order
        - 00-introduction
        - 01-childhood
        - appendix
      master_doc: book/book.tex     # consumer-owned master document
      chapters_dir: book/chapters   # where to stage per-thread chapter files
      chapter_filename: chapter.tex # per-thread filename to stage
      out_pdf: book/book.pdf        # output PDF path

``order`` semantics (locked at curation): when present it is the
authoritative include-list and ordering — slugs omitted from ``order``
are excluded (with an informational note); slugs in ``order`` that do
not appear in BRIEF ``documents:`` are a hard error. The cross-check
against the BRIEF happens in :mod:`orchestrate` (this module has no
knowledge of the documents list).

Path fields (``master_doc`` / ``chapters_dir`` / ``out_pdf``) are
project-root-relative and must stay inside the project tree: absolute
paths and ``..`` traversal are rejected at parse time. ``chapters_dir``
MAY contain path separators (it is a nested build dir like
``book/chapters``, unlike ``project-share``'s bare ``out`` name);
``chapter_filename`` must be a bare filename (no separators).
"""

from __future__ import annotations

from pathlib import PurePosixPath
from typing import Any, List, Optional

import yaml
from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator

# The BRIEF filename and frontmatter delimiter mirror
# ``anvil/lib/project_brief.py`` (single on-disk convention).
BRIEF_FILENAME = "BRIEF.md"
_FRONTMATTER_DELIM = "---"

# The top-level BRIEF frontmatter key this skill owns.
BUILD_FRONTMATTER_KEY = "build"

# Framework defaults for the staging + compile contract.
DEFAULT_CHAPTERS_DIR = "book/chapters"
DEFAULT_CHAPTER_FILENAME = "chapter.tex"

# Marker file written at the start of each apply run into the chapters
# dir; its presence authorizes the blow-away rebuild. Mirrors
# ``project-share``'s ``EXPORT.md`` guard.
MARKER_FILENAME = ".anvil-book-build"

# Report filename written at the project root on every apply-mode run.
REPORT_FILENAME = "BOOK_REPORT.md"


def _validate_rel_path(value: str, field_name: str) -> str:
    """Reject absolute paths and ``..`` traversal; normalize separators.

    Returns the POSIX-normalized relative path string. Raises
    ``ValueError`` (surfaced by pydantic as a validation error) on an
    absolute path, an empty value, or any ``..`` component.
    """
    if not value or not value.strip():
        raise ValueError(
            f"build.{field_name} must be a non-empty path; got {value!r}."
        )
    raw = value.strip()
    if raw.startswith("/") or raw.startswith("\\"):
        raise ValueError(
            f"build.{field_name} must be project-root-relative (no leading "
            f"slash); got {value!r}."
        )
    # Normalize backslashes to forward slashes so a Windows-authored BRIEF
    # is accepted, then walk the parts for traversal / anchor.
    pp = PurePosixPath(raw.replace("\\", "/"))
    if pp.is_absolute():
        raise ValueError(
            f"build.{field_name} must be relative; got {value!r}."
        )
    parts = pp.parts
    if any(part == ".." for part in parts):
        raise ValueError(
            f"build.{field_name} must not escape the project root with "
            f"`..`; got {value!r}."
        )
    normalized = "/".join(part for part in parts if part not in (".",))
    if not normalized:
        raise ValueError(
            f"build.{field_name} must not resolve to the project root; "
            f"got {value!r}."
        )
    return normalized


class BookConfig(BaseModel):
    """Typed view of the BRIEF ``build:`` frontmatter block.

    Attributes
    ----------
    order
        Optional explicit chapter ordering. When present it is the
        authoritative include-list AND ordering for the book. When
        absent (``None``), every BRIEF ``documents:`` entry is a chapter
        in BRIEF order.
    master_doc
        Consumer-owned master LaTeX document (project-root-relative).
        Required for the compile step; when ``None`` the run is
        **staging-only** (chapters staged, no compile, report still
        produced).
    chapters_dir
        Directory (project-root-relative) the per-thread chapter files
        are staged into. Blow-away-rebuilt on each apply run. Defaults
        to :data:`DEFAULT_CHAPTERS_DIR`.
    chapter_filename
        Per-thread filename to look for in each resolved version dir.
        Defaults to :data:`DEFAULT_CHAPTER_FILENAME`. A bare filename —
        no path separators.
    out_pdf
        Output PDF path (project-root-relative). When ``None`` it
        defaults to ``<chapters_dir>/../book.pdf`` (see
        :meth:`resolved_out_pdf`).
    """

    model_config = ConfigDict(extra="forbid")

    order: Optional[List[str]] = Field(default=None)
    master_doc: Optional[str] = Field(default=None)
    chapters_dir: str = Field(default=DEFAULT_CHAPTERS_DIR)
    chapter_filename: str = Field(default=DEFAULT_CHAPTER_FILENAME)
    out_pdf: Optional[str] = Field(default=None)

    @field_validator("order")
    @classmethod
    def _order_entries_nonempty_unique(
        cls, value: Optional[List[str]]
    ) -> Optional[List[str]]:
        if value is None:
            return None
        seen = set()
        for i, entry in enumerate(value):
            if not isinstance(entry, str) or not entry.strip():
                raise ValueError(
                    f"build.order[{i}] must be a non-empty string; got "
                    f"{entry!r}. Suggested fix: list document slugs only."
                )
            if entry in seen:
                raise ValueError(
                    f"build.order lists slug {entry!r} more than once. "
                    f"Suggested fix: remove the duplicate entry."
                )
            seen.add(entry)
        return value

    @field_validator("master_doc")
    @classmethod
    def _master_doc_rel(cls, value: Optional[str]) -> Optional[str]:
        if value is None:
            return None
        return _validate_rel_path(value, "master_doc")

    @field_validator("chapters_dir")
    @classmethod
    def _chapters_dir_rel(cls, value: str) -> str:
        return _validate_rel_path(value, "chapters_dir")

    @field_validator("chapter_filename")
    @classmethod
    def _chapter_filename_bare(cls, value: str) -> str:
        if not value or not value.strip():
            raise ValueError(
                "build.chapter_filename must be a non-empty filename; got "
                f"{value!r}."
            )
        raw = value.strip()
        if "/" in raw or "\\" in raw:
            raise ValueError(
                f"build.chapter_filename must be a bare filename (no path "
                f"separators); got {value!r}. Suggested fix: use a single "
                f"name like `chapter.tex`."
            )
        if raw in (".", ".."):
            raise ValueError(
                f"build.chapter_filename must be a real filename; got "
                f"{value!r}."
            )
        return raw

    @field_validator("out_pdf")
    @classmethod
    def _out_pdf_rel(cls, value: Optional[str]) -> Optional[str]:
        if value is None:
            return None
        return _validate_rel_path(value, "out_pdf")

    def resolved_out_pdf(self) -> str:
        """Return the effective output PDF path (project-root-relative).

        When ``out_pdf`` is declared it is used verbatim; otherwise it
        defaults to ``<chapters_dir>/../book.pdf`` — i.e. ``book.pdf``
        in the parent of the chapters dir (``book/book.pdf`` for the
        default ``book/chapters`` chapters dir; a bare ``book.pdf`` when
        ``chapters_dir`` has no parent).
        """
        if self.out_pdf is not None:
            return self.out_pdf
        parent = PurePosixPath(self.chapters_dir).parent
        if str(parent) in (".", ""):
            return "book.pdf"
        return str(parent / "book.pdf")


def _extract_frontmatter(text: str) -> Optional[dict]:
    """Extract the YAML frontmatter from ``text`` and return it as a dict.

    Mirrors ``anvil/lib/project_brief.py::_extract_frontmatter`` (and
    ``project-share``'s copy) so all parsers accept the same on-disk
    delimiter convention. Kept local rather than importing the shared
    private helper — per-skill libs do not reach into another module's
    underscore namespace.
    """
    lines = text.splitlines()
    if lines and lines[0].startswith("﻿"):
        lines[0] = lines[0][1:]

    first_idx = 0
    while first_idx < len(lines) and lines[first_idx].strip() == "":
        first_idx += 1
    if first_idx >= len(lines):
        return None
    if lines[first_idx].strip() != _FRONTMATTER_DELIM:
        return None

    body_start = first_idx + 1
    close_idx = None
    for i in range(body_start, len(lines)):
        if lines[i].strip() == _FRONTMATTER_DELIM:
            close_idx = i
            break
    if close_idx is None:
        return None

    yaml_text = "\n".join(lines[body_start:close_idx])
    try:
        parsed = yaml.safe_load(yaml_text)
    except yaml.YAMLError:
        return None
    if not isinstance(parsed, dict):
        return None
    return parsed


def load_book_config(project_dir) -> BookConfig:
    """Load the ``build:`` block from ``<project_dir>/BRIEF.md``.

    Returns
    -------
    BookConfig
        Parsed config; all-defaults when the BRIEF has no ``build:``
        block (the zero-config contract).

    Raises
    ------
    FileNotFoundError
        When ``<project_dir>/BRIEF.md`` does not exist.
    ValueError
        When the BRIEF has no parseable frontmatter, or the ``build:``
        block is present but malformed (wrong type, unknown key, bad
        path field, non-string ``order`` entries).
    """
    from pathlib import Path

    project_dir = Path(project_dir)
    brief_path = project_dir / BRIEF_FILENAME
    if not brief_path.is_file():
        raise FileNotFoundError(
            f"No BRIEF found at {brief_path}. project-book reads its build "
            f"config (and the documents list) from the project BRIEF."
        )
    text = brief_path.read_text(encoding="utf-8")
    fm = _extract_frontmatter(text)
    if fm is None:
        raise ValueError(
            f"BRIEF at {brief_path} has no parseable YAML frontmatter."
        )

    raw: Any = fm.get(BUILD_FRONTMATTER_KEY)
    if raw is None:
        return BookConfig()
    if not isinstance(raw, dict):
        raise ValueError(
            f"BRIEF.build must be a mapping (a `build:` block with keys); "
            f"got {type(raw).__name__} at {brief_path}."
        )
    try:
        return BookConfig(**raw)
    except ValidationError as exc:
        raise ValueError(
            f"BRIEF.build at {brief_path} failed schema validation: {exc}"
        ) from exc


__all__ = [
    "BRIEF_FILENAME",
    "BUILD_FRONTMATTER_KEY",
    "DEFAULT_CHAPTERS_DIR",
    "DEFAULT_CHAPTER_FILENAME",
    "MARKER_FILENAME",
    "REPORT_FILENAME",
    "BookConfig",
    "load_book_config",
]
