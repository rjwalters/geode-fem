"""Skill-local export configuration for `anvil:project-share` (issue #396).

Parses the optional ``export:`` block out of the project ``BRIEF.md``
frontmatter into a typed :class:`ExportConfig`. Per the curator's design
the parser is **skill-local** — the shared
``anvil/lib/project_brief.py::ProjectBrief`` model is NOT extended
(``ProjectBrief`` is ``extra="forbid"`` on the model but its parse path
``_parse_brief_body`` explicitly ignores unknown top-level frontmatter
keys, so an ``export:`` block is safe to add to a BRIEF today with zero
changes to the shared parser).

Zero-config contract: a BRIEF with no ``export:`` block at all yields
:class:`ExportConfig` defaults — full export of every ``documents:``
entry in BRIEF order, refs + research + assets included, the default
strip list applied, output to ``SHARE/``.

Config surface (all fields optional)::

    export:
      order: [series-a-deck, investment-memo, ...]  # include-list AND order
      include_research: true     # default true
      include_refs: true         # default true
      include_assets: true       # default true
      strip: [...]               # default: _progress.json, changelog.md,
                                 #          "_*.json", "*.tmp", ".tmp*"
      out: SHARE                 # default SHARE

``order`` semantics (locked at curation): when present it is the
authoritative include-list and ordering — slugs omitted from ``order``
are excluded from the export (with an informational note); slugs in
``order`` that do not appear in BRIEF ``documents:`` are a hard error.
The cross-check against the BRIEF happens in :mod:`plan` (this module
has no knowledge of the documents list).
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, List, Optional

import yaml
from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator

# The BRIEF filename and frontmatter delimiter mirror
# ``anvil/lib/project_brief.py`` (single on-disk convention).
BRIEF_FILENAME = "BRIEF.md"
_FRONTMATTER_DELIM = "---"

# The top-level BRIEF frontmatter key this skill owns.
EXPORT_FRONTMATTER_KEY = "export"

# Default output directory name. The skill is named ``project-share``
# (not ``project-export``) because (a) it matches this directory and
# (b) ``anvil/lib/export_schema.py`` already owns the word "export"
# for an unrelated meaning (emitting JSON Schema documents).
DEFAULT_OUT = "SHARE"

# Default strip list: anvil bookkeeping that must never reach an
# outside recipient. ``_*.json`` covers ``_progress.json`` /
# ``_meta.json`` / any future underscore-prefixed sidecar JSON;
# ``.tmp*`` covers the ``staged_sidecar`` staging convention
# (``anvil/lib/sidecar.py``).
DEFAULT_STRIP = (
    "_progress.json",
    "changelog.md",
    "_*.json",
    "*.tmp",
    ".tmp*",
)


class ExportConfig(BaseModel):
    """Typed view of the BRIEF ``export:`` frontmatter block.

    Attributes
    ----------
    order
        Optional explicit doc ordering. When present it is the
        authoritative include-list AND ordering for the export. When
        absent (``None``), every BRIEF ``documents:`` entry exports in
        BRIEF order.
    include_research
        Copy the shared ``<project>/research/`` pool to
        ``<out>/research/`` (default True).
    include_refs
        Copy per-thread ``<project>/<slug>/refs/`` into each doc folder
        (default True; only when non-empty).
    include_assets
        Copy subdirectories of the resolved version dir (``figures/``,
        ``exhibits/``, ...) alongside the source body. When False, only
        top-level files of the version dir are copied (default True).
    strip
        Filename patterns (``fnmatch`` against every path component)
        omitted from the export. Defaults to :data:`DEFAULT_STRIP`.
    out
        Output directory name under the project root (default
        ``SHARE``). Must be a bare directory name — no path separators.
    """

    model_config = ConfigDict(extra="forbid")

    order: Optional[List[str]] = Field(default=None)
    include_research: bool = Field(default=True)
    include_refs: bool = Field(default=True)
    include_assets: bool = Field(default=True)
    strip: List[str] = Field(default_factory=lambda: list(DEFAULT_STRIP))
    out: str = Field(default=DEFAULT_OUT)

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
                    f"export.order[{i}] must be a non-empty string; got "
                    f"{entry!r}. Suggested fix: list document slugs only."
                )
            if entry in seen:
                raise ValueError(
                    f"export.order lists slug {entry!r} more than once. "
                    f"Suggested fix: remove the duplicate entry."
                )
            seen.add(entry)
        return value

    @field_validator("strip")
    @classmethod
    def _strip_entries_nonempty(cls, value: List[str]) -> List[str]:
        for i, entry in enumerate(value):
            if not isinstance(entry, str) or not entry.strip():
                raise ValueError(
                    f"export.strip[{i}] must be a non-empty string "
                    f"pattern; got {entry!r}."
                )
        return value

    @field_validator("out")
    @classmethod
    def _out_is_bare_dirname(cls, value: str) -> str:
        if not value or not value.strip():
            raise ValueError(
                "export.out must be a non-empty directory name; got "
                f"{value!r}."
            )
        if "/" in value or "\\" in value:
            raise ValueError(
                f"export.out must be a bare directory name under the "
                f"project root (no path separators); got {value!r}. "
                f"Suggested fix: use a single name like `SHARE`."
            )
        if value in (".", ".."):
            raise ValueError(
                f"export.out must not be {value!r} — it would resolve to "
                f"the project root (or above) and the marker-guarded "
                f"rebuild would refuse it anyway."
            )
        return value


def _extract_frontmatter(text: str) -> Optional[dict]:
    """Extract the YAML frontmatter from ``text`` and return it as a dict.

    Mirrors ``anvil/lib/project_brief.py::_extract_frontmatter`` (which
    itself mirrors ``project_discovery``'s copy) so all three parsers
    accept exactly the same on-disk delimiter convention. Kept local
    rather than importing the shared private helper — per-skill libs do
    not reach into another module's underscore namespace.
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


def load_export_config(project_dir: Path) -> ExportConfig:
    """Load the ``export:`` block from ``<project_dir>/BRIEF.md``.

    Returns
    -------
    ExportConfig
        Parsed config; all-defaults when the BRIEF has no ``export:``
        block (the zero-config contract).

    Raises
    ------
    FileNotFoundError
        When ``<project_dir>/BRIEF.md`` does not exist.
    ValueError
        When the BRIEF has no parseable frontmatter, or the ``export:``
        block is present but malformed (wrong type, unknown key, bad
        ``out`` name, non-string ``order`` / ``strip`` entries).
    """
    project_dir = Path(project_dir)
    brief_path = project_dir / BRIEF_FILENAME
    if not brief_path.is_file():
        raise FileNotFoundError(
            f"No BRIEF found at {brief_path}. The export reads its "
            f"config (and the documents list) from the project BRIEF."
        )
    text = brief_path.read_text(encoding="utf-8")
    fm = _extract_frontmatter(text)
    if fm is None:
        raise ValueError(
            f"BRIEF at {brief_path} has no parseable YAML frontmatter."
        )

    raw: Any = fm.get(EXPORT_FRONTMATTER_KEY)
    if raw is None:
        return ExportConfig()
    if not isinstance(raw, dict):
        raise ValueError(
            f"BRIEF.export must be a mapping (an `export:` block with "
            f"keys); got {type(raw).__name__} at {brief_path}."
        )
    try:
        return ExportConfig(**raw)
    except ValidationError as exc:
        raise ValueError(
            f"BRIEF.export at {brief_path} failed schema validation: {exc}"
        ) from exc


__all__ = [
    "BRIEF_FILENAME",
    "DEFAULT_OUT",
    "DEFAULT_STRIP",
    "EXPORT_FRONTMATTER_KEY",
    "ExportConfig",
    "load_export_config",
]
