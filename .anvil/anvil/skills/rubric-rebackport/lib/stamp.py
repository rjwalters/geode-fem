"""Stamping primitives for `anvil:rubric-rebackport` (issue #358).

The atomic file-rewrite primitives that the apply step calls when in
``--stamp-only`` mode. Each function takes one of the edit-primitive
dataclasses from :mod:`plan` and performs the rewrite on disk.

All writes are atomic (temp file + ``os.replace``). On any I/O failure
the original file is left intact and the function raises ``OSError``;
the apply step's per-review snapshot is the rollback safety net for
multi-file rewrites (e.g., stamp + progress row + summary block where
the second write fails).

Subprocess-free. Stdlib-only.
"""

from __future__ import annotations

import json
import os
import re
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from .plan import (
    ProgressRowStamp,
    StampOp,
    SummaryRubricBlock,
)


_TMP_SUFFIX = ".rebackport.tmp"


def _atomic_write_text(path: Path, text: str) -> None:
    """Write ``text`` to ``path`` atomically (temp file + os.replace)."""
    tmp = path.with_name(path.name + _TMP_SUFFIX)
    tmp.write_text(text, encoding="utf-8")
    os.replace(str(tmp), str(path))


def _atomic_write_json(path: Path, data: Dict[str, object]) -> None:
    """Serialize ``data`` to JSON and atomic-write to ``path``.

    Uses ``indent=2`` + trailing newline to match the existing on-disk
    convention used by the per-skill review-writers (a glance at any of
    the canary fixtures shows this shape).
    """
    text = json.dumps(data, indent=2) + "\n"
    _atomic_write_text(path, text)


# ---------------------------------------------------------------------------
# _meta.json stamping
# ---------------------------------------------------------------------------


def apply_stamp_meta(op: StampOp) -> Tuple[bool, Optional[str]]:
    """Apply a :class:`StampOp` to a ``_meta.json`` file.

    Returns ``(changed, error)``:

    - ``changed`` — True iff the file was actually rewritten (vs. a
      no-op when the fields already match).
    - ``error`` — None on success; a short diagnostic on failure.

    The function never deletes or relocates fields the caller didn't
    set; it merges the three stamping fields into the existing payload.
    """
    try:
        with op.meta_path.open("r", encoding="utf-8") as fh:
            meta = json.load(fh)
    except OSError as exc:
        return False, f"read failed: {exc}"
    except json.JSONDecodeError as exc:
        return False, f"JSON parse failed: {exc}"
    if not isinstance(meta, dict):
        return False, "top-level JSON is not an object"

    changed = False
    if meta.get("rubric_id") != op.rubric_id:
        meta["rubric_id"] = op.rubric_id
        changed = True
    if op.rubric_total is not None and meta.get("rubric_total") != op.rubric_total:
        meta["rubric_total"] = op.rubric_total
        changed = True
    if (
        op.advance_threshold is not None
        and meta.get("advance_threshold") != op.advance_threshold
    ):
        meta["advance_threshold"] = op.advance_threshold
        changed = True

    if not changed:
        return False, None
    try:
        _atomic_write_json(op.meta_path, meta)
    except OSError as exc:
        return False, f"write failed: {exc}"
    return True, None


# ---------------------------------------------------------------------------
# _progress.json score_history row stamping
# ---------------------------------------------------------------------------


def apply_stamp_progress_rows(
    op: ProgressRowStamp,
) -> Tuple[int, Optional[str]]:
    """Apply a :class:`ProgressRowStamp` to a ``_progress.json``.

    Walks ``metadata.score_history[]`` and adds ``rubric_id`` to every
    row that lacks it. Returns ``(rows_stamped, error)``.

    Rows that already carry ``rubric_id`` are left alone (so a re-run is
    idempotent even when the operator changes ``--legacy-rubric`` mid-flight).
    """
    try:
        with op.progress_path.open("r", encoding="utf-8") as fh:
            progress = json.load(fh)
    except OSError as exc:
        return 0, f"read failed: {exc}"
    except json.JSONDecodeError as exc:
        return 0, f"JSON parse failed: {exc}"
    if not isinstance(progress, dict):
        return 0, "top-level JSON is not an object"

    metadata = progress.get("metadata")
    if not isinstance(metadata, dict):
        return 0, None  # No metadata block; nothing to stamp.
    history = metadata.get("score_history")
    if not isinstance(history, list):
        return 0, None

    stamped = 0
    for row in history:
        if not isinstance(row, dict):
            continue
        if row.get("rubric_id"):
            continue
        row["rubric_id"] = op.rubric_id
        stamped += 1

    if stamped == 0:
        return 0, None

    try:
        _atomic_write_json(op.progress_path, progress)
    except OSError as exc:
        return 0, f"write failed: {exc}"
    return stamped, None


# ---------------------------------------------------------------------------
# _summary.md rubric block stamping
# ---------------------------------------------------------------------------


# Regex that matches an existing ``rubric:`` YAML block in a
# ``_summary.md`` frontmatter. Captures the indentation of the key and
# the block contents (everything up to the next sibling top-level key
# or the closing ``---``). We do NOT try to re-parse the YAML — we
# replace the block as a single text region.
_YAML_RUBRIC_BLOCK_RE = re.compile(
    r"^(rubric:\s*\n(?:(?:[ \t]+.*|\s*)\n)*?)(?=^[A-Za-z_]|\Z|^---\s*$)",
    re.MULTILINE,
)


def _render_yaml_rubric_block(block: SummaryRubricBlock) -> str:
    """Render the ``rubric:`` YAML block for a ``_summary.md`` frontmatter."""
    lines: List[str] = ["rubric:"]
    lines.append(f"  id: {block.rubric_id}")
    lines.append(f"  total: {block.rubric_total}")
    lines.append(f"  advance_threshold: {block.advance_threshold}")
    lines.append(f"  dimensions: {block.dimensions}")
    if block.prior_rubric_inferred:
        lines.append("  prior_rubric_id: null")
        lines.append("  prior_rubric_inferred: \"/40-legacy\"")
    return "\n".join(lines) + "\n"


def apply_summary_rubric_block(
    block: SummaryRubricBlock,
) -> Tuple[bool, Optional[str]]:
    """Add or update the ``rubric:`` block in a ``_summary.md`` file.

    Behaviors:

    - If the file already carries a ``rubric:`` YAML block in its
      frontmatter, the block is replaced with a freshly-rendered one.
    - If the file has YAML frontmatter but no ``rubric:`` block, the
      block is appended at the end of the frontmatter (just before
      the closing ``---``).
    - If the file has no frontmatter, a new frontmatter is added at
      the very top wrapping the ``rubric:`` block.
    - If the file is JSON-shaped (top-level JSON object), the
      ``rubric`` key is added or overwritten in the JSON object.

    Returns ``(changed, error)``.
    """
    try:
        text = block.summary_path.read_text(encoding="utf-8")
    except OSError as exc:
        return False, f"read failed: {exc}"

    # Try JSON path first — _summary.md files in some fixtures are
    # JSON-shaped (see tests/skills/memo/fixtures/.../summary_with_rubric_block.json).
    stripped = text.lstrip()
    if stripped.startswith("{"):
        try:
            parsed = json.loads(text)
        except json.JSONDecodeError:
            parsed = None
        if isinstance(parsed, dict):
            rubric_block_data: Dict[str, object] = {
                "id": block.rubric_id,
                "total": block.rubric_total,
                "advance_threshold": block.advance_threshold,
                "dimensions": block.dimensions,
            }
            if block.prior_rubric_inferred:
                rubric_block_data["prior_rubric_id"] = None
                rubric_block_data["prior_rubric_inferred"] = "/40-legacy"
            existing = parsed.get("rubric")
            if existing == rubric_block_data:
                return False, None
            parsed["rubric"] = rubric_block_data
            try:
                _atomic_write_json(block.summary_path, parsed)
            except OSError as exc:
                return False, f"write failed: {exc}"
            return True, None

    # YAML / markdown path.
    rendered_block = _render_yaml_rubric_block(block)
    new_text, changed = _splice_yaml_rubric_block(text, rendered_block)
    if not changed:
        return False, None
    try:
        _atomic_write_text(block.summary_path, new_text)
    except OSError as exc:
        return False, f"write failed: {exc}"
    return True, None


def _splice_yaml_rubric_block(text: str, rendered_block: str) -> Tuple[str, bool]:
    """Splice the ``rubric:`` block into the markdown frontmatter.

    Returns ``(new_text, changed)``. When the new block matches what's
    already there byte-for-byte, ``changed`` is False and ``new_text``
    is the original.
    """
    fm_open, fm_close = _find_frontmatter_bounds(text)
    if fm_open is None or fm_close is None:
        # No frontmatter — synthesize one at the top.
        new_text = (
            "---\n"
            + rendered_block
            + "---\n"
            + text
        )
        return new_text, True

    fm_text = text[fm_open:fm_close]
    # Search for an existing rubric: block in the frontmatter.
    match = _YAML_RUBRIC_BLOCK_RE.search(fm_text)
    if match is not None:
        existing_block = match.group(1)
        if existing_block == rendered_block:
            return text, False
        new_fm = (
            fm_text[: match.start()]
            + rendered_block
            + fm_text[match.end():]
        )
    else:
        # Append to the end of the frontmatter.
        if not fm_text.endswith("\n"):
            fm_text = fm_text + "\n"
        new_fm = fm_text + rendered_block

    new_text = text[:fm_open] + new_fm + text[fm_close:]
    return new_text, True


def _find_frontmatter_bounds(text: str) -> Tuple[Optional[int], Optional[int]]:
    """Return (open_idx, close_idx) of the YAML frontmatter body, or (None, None).

    ``open_idx`` is the offset of the first character AFTER the opening
    ``---\\n``; ``close_idx`` is the offset of the closing ``---``.
    """
    # Skip leading whitespace.
    cursor = 0
    while cursor < len(text) and text[cursor] in " \t\n\r":
        cursor += 1
    if not text.startswith("---", cursor):
        return None, None
    # Move past the opening delimiter line.
    end_of_open_line = text.find("\n", cursor)
    if end_of_open_line == -1:
        return None, None
    body_start = end_of_open_line + 1
    # Search for the closing ``---`` at column 0 on its own line.
    body_end = None
    search_cursor = body_start
    while True:
        idx = text.find("\n---", search_cursor - 1)
        if idx == -1:
            return None, None
        # Verify the ``---`` is at column 0 of a line and the line
        # contains nothing else.
        line_end = text.find("\n", idx + 1)
        if line_end == -1:
            line_end = len(text)
        line = text[idx + 1:line_end].strip()
        if line == "---":
            body_end = idx + 1
            break
        search_cursor = line_end + 1
    return body_start, body_end


__all__ = [
    "apply_stamp_meta",
    "apply_stamp_progress_rows",
    "apply_summary_rubric_block",
]
