"""Numbering-doc parser + deterministic manifest builder (issue #599).

The numbering doc is a **human-authored** map from each original capture
(a phone photo such as ``PXL_20231014_142301456.jpg``) to a stable output
name plus the physical archive item number(s) it depicts. This module
turns that doc into a machine-readable, deterministic ``manifest.json``
provenance map — decoupling the human-authoritative numbering *decision*
from its mechanical *application* (which stays consumer-native).

Design decisions
----------------
- **Two input formats, one parser front door.** ``.csv`` is parsed as
  CSV; anything else (``.md`` / ``.markdown`` / no extension) is parsed
  as a GitHub-flavoured markdown pipe table. Columns are matched by
  header *name*, not position, so column order in the doc is irrelevant.
- **``multi_item`` is derived, never declared.** A stable name whose
  stem ends in ``-multi`` (e.g. ``043-multi.jpg``) sets
  ``multi_item: true``. There is no ``multi_item`` column — deriving it
  keeps the doc and the manifest from disagreeing.
- **Rotation hints normalize to a canonical int or null.** Empty →
  ``null``; a numeric hint must be one of ``0/90/180/270`` (any other
  angle is a hard error naming the row); a small documented descriptive
  vocabulary (``"upside down"`` → 180, etc.) maps to those ints. An
  unrecognized descriptive string is a hard error — a mechanical gate
  fails loud rather than passing garbage downstream.
- **Determinism is a hard contract.** Entries are sorted by stable name;
  the manifest body carries no timestamps; ``generated_from`` records the
  numbering-doc *basename* only (not a path, not an mtime). Two runs over
  identical inputs therefore serialize byte-for-byte identically.
- **The doc is authoritative, not the directory.** Captures listed in
  the doc but absent from the photos dir populate ``missing_captures``
  (and make the run unsuccessful); captures present on disk but absent
  from the doc are silently ignored.
- **Duplicate stable names are a hard error.** Two rows minting the same
  output name would collide on disk; we refuse and name the offending
  rows.
"""

from __future__ import annotations

import csv
import io
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

SCHEMA_VERSION = 1

# Required + optional column names in the numbering doc. Matched
# case-insensitively; surrounding whitespace stripped.
_REQUIRED_COLUMNS = ("original", "stable", "archive_ids")
_OPTIONAL_COLUMNS = ("rotation_hint",)

# Canonical clockwise-degree rotations the pipeline understands.
_VALID_ANGLES = (0, 90, 180, 270)

# Documented descriptive-hint vocabulary → clockwise degrees to apply.
# Keys are lowercased and internal runs of non-alphanumerics collapse to a
# single space before lookup, so "upside-down", "upside down", and
# "Upside  Down" all resolve identically.
_ROTATION_WORDS = {
    "upside down": 180,
    "upsidedown": 180,
    "inverted": 180,
    "flipped": 180,
    "cw": 90,
    "clockwise": 90,
    "rotate right": 90,
    "right": 90,
    "ccw": 270,
    "counterclockwise": 270,
    "counter clockwise": 270,
    "rotate left": 270,
    "left": 270,
}

_MULTI_SUFFIX = "-multi"


class NumberingDocError(ValueError):
    """The numbering doc is malformed (missing columns, bad rows, etc.)."""


class DuplicateStableError(NumberingDocError):
    """Two or more rows mint the same stable output name."""


class RotationHintError(NumberingDocError):
    """A rotation hint is neither a valid angle nor a known descriptor."""


@dataclass(frozen=True)
class Entry:
    """One resolved provenance row."""

    original: str
    stable: str
    archive_ids: tuple[str, ...]
    rotation_hint: Optional[int]
    multi_item: bool
    # 1-based row number in the source doc (data rows only; header
    # excluded) — surfaced in error messages, not in the manifest body.
    row_number: int

    def to_dict(self) -> dict:
        return {
            "original": self.original,
            "stable": self.stable,
            "archive_ids": list(self.archive_ids),
            "rotation_hint": self.rotation_hint,
            "multi_item": self.multi_item,
        }


def _norm_key(text: str) -> str:
    return text.strip().lower()


def _collapse(text: str) -> str:
    """Lowercase and collapse non-alphanumeric runs to single spaces."""
    out: list[str] = []
    prev_sep = False
    for ch in text.strip().lower():
        if ch.isalnum():
            out.append(ch)
            prev_sep = False
        else:
            if not prev_sep:
                out.append(" ")
            prev_sep = True
    return "".join(out).strip()


def normalize_rotation(value: Optional[str], *, row_number: int) -> Optional[int]:
    """Normalize a raw rotation-hint cell to a canonical int or ``None``.

    Empty / missing → ``None``. A numeric hint must be one of
    ``0/90/180/270``. A descriptive hint must be in the documented
    vocabulary. Anything else raises :class:`RotationHintError` naming the
    offending row and value.
    """
    if value is None:
        return None
    raw = value.strip()
    if raw == "":
        return None

    # Numeric hint (possibly with a trailing degree sign / "deg").
    numeric = raw.rstrip("°").strip()
    for suffix in ("degrees", "degree", "deg", "°"):
        if numeric.lower().endswith(suffix):
            numeric = numeric[: -len(suffix)].strip()
    if numeric.lstrip("+-").isdigit():
        angle = int(numeric) % 360
        if angle not in _VALID_ANGLES:
            raise RotationHintError(
                f"row {row_number}: rotation hint {raw!r} is not one of "
                f"{_VALID_ANGLES} (normalized to {angle})"
            )
        return angle

    key = _collapse(raw)
    if key in _ROTATION_WORDS:
        return _ROTATION_WORDS[key]

    raise RotationHintError(
        f"row {row_number}: unrecognized rotation hint {raw!r} — use an "
        f"angle in {_VALID_ANGLES} or a known descriptor "
        f"(e.g. 'upside down', 'cw', 'ccw')"
    )


def _split_archive_ids(raw: str) -> tuple[str, ...]:
    return tuple(part.strip() for part in raw.split(",") if part.strip())


def _is_multi(stable: str) -> bool:
    stem = Path(stable).stem
    return stem.endswith(_MULTI_SUFFIX)


def _parse_markdown_table(text: str) -> list[dict[str, str]]:
    """Parse a GitHub-flavoured markdown pipe table into row dicts."""
    rows: list[list[str]] = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or not stripped.startswith("|"):
            continue
        # Split on pipes, dropping the leading/trailing empty cells that a
        # ``| a | b |`` fence produces.
        cells = [c.strip() for c in stripped.strip("|").split("|")]
        rows.append(cells)

    if not rows:
        return []

    header = [_norm_key(c) for c in rows[0]]
    body = rows[1:]

    # Drop the separator row (``|---|---|``) if present.
    def _is_separator(cells: list[str]) -> bool:
        return all(set(c) <= set("-: ") and "-" in c for c in cells) and bool(cells)

    if body and _is_separator(body[0]):
        body = body[1:]

    out: list[dict[str, str]] = []
    for cells in body:
        record: dict[str, str] = {}
        for i, col in enumerate(header):
            record[col] = cells[i].strip() if i < len(cells) else ""
        out.append(record)
    return out


def _parse_csv(text: str) -> list[dict[str, str]]:
    reader = csv.DictReader(io.StringIO(text))
    out: list[dict[str, str]] = []
    for record in reader:
        out.append(
            {
                _norm_key(k): (v or "").strip()
                for k, v in record.items()
                if k is not None
            }
        )
    return out


def parse_numbering_doc(doc_path: Path) -> list[Entry]:
    """Parse the numbering doc at ``doc_path`` into resolved entries.

    Format is chosen by extension: ``.csv`` → CSV; everything else →
    markdown pipe table. Raises :class:`NumberingDocError` (or a
    subclass) on any structural problem.
    """
    doc_path = Path(doc_path)
    if not doc_path.is_file():
        raise NumberingDocError(f"numbering doc not found: {doc_path}")

    text = doc_path.read_text(encoding="utf-8")
    if doc_path.suffix.lower() == ".csv":
        records = _parse_csv(text)
    else:
        records = _parse_markdown_table(text)

    if not records:
        # A well-formed doc with a header but no data rows is valid (empty
        # manifest). A doc with no recognizable table at all is not.
        header_present = _detect_header(text, doc_path)
        if not header_present:
            raise NumberingDocError(
                f"{doc_path.name}: no table found (expected a markdown pipe "
                f"table or CSV with columns {_REQUIRED_COLUMNS})"
            )
        return []

    present = set(records[0].keys())
    missing_cols = [c for c in _REQUIRED_COLUMNS if c not in present]
    if missing_cols:
        raise NumberingDocError(
            f"{doc_path.name}: missing required column(s): "
            f"{', '.join(missing_cols)} (have: {', '.join(sorted(present))})"
        )

    entries: list[Entry] = []
    for i, record in enumerate(records, start=1):
        original = record.get("original", "").strip()
        stable = record.get("stable", "").strip()
        archive_raw = record.get("archive_ids", "").strip()
        if not original and not stable and not archive_raw:
            # Blank row — skip silently (markdown docs often have them).
            continue
        if not original:
            raise NumberingDocError(f"row {i}: empty 'original' column")
        if not stable:
            raise NumberingDocError(f"row {i}: empty 'stable' column")
        if not archive_raw:
            raise NumberingDocError(
                f"row {i}: empty 'archive_ids' column (use a comma-separated "
                f"list of archive item numbers)"
            )
        rotation = normalize_rotation(
            record.get("rotation_hint"), row_number=i
        )
        entries.append(
            Entry(
                original=original,
                stable=stable,
                archive_ids=_split_archive_ids(archive_raw),
                rotation_hint=rotation,
                multi_item=_is_multi(stable),
                row_number=i,
            )
        )

    _check_duplicate_stables(entries)
    return entries


def _detect_header(text: str, doc_path: Path) -> bool:
    """Best-effort: does the doc contain a header naming the required cols?"""
    if doc_path.suffix.lower() == ".csv":
        first = text.splitlines()[0] if text.strip() else ""
        cols = {_norm_key(c) for c in first.split(",")}
    else:
        cols = set()
        for line in text.splitlines():
            s = line.strip()
            if s.startswith("|"):
                cols = {_norm_key(c) for c in s.strip("|").split("|")}
                break
    return all(c in cols for c in _REQUIRED_COLUMNS)


def _check_duplicate_stables(entries: list[Entry]) -> None:
    seen: dict[str, list[int]] = {}
    for entry in entries:
        seen.setdefault(entry.stable, []).append(entry.row_number)
    dupes = {name: rows for name, rows in seen.items() if len(rows) > 1}
    if dupes:
        parts = [
            f"{name!r} (rows {', '.join(str(r) for r in rows)})"
            for name, rows in sorted(dupes.items())
        ]
        raise DuplicateStableError(
            "duplicate stable name(s) in numbering doc: " + "; ".join(parts)
        )


def find_missing_captures(entries: list[Entry], photos_dir: Path) -> list[str]:
    """Return sorted originals listed in the doc but absent from the dir.

    Reads only the directory *listing* — no image bytes are opened.
    """
    photos_dir = Path(photos_dir)
    present: set[str] = set()
    if photos_dir.is_dir():
        present = {p.name for p in photos_dir.iterdir() if p.is_file()}
    missing = {e.original for e in entries if e.original not in present}
    return sorted(missing)


def build_manifest(
    doc_path: Path, photos_dir: Path
) -> tuple[dict, list[str]]:
    """Parse the doc and assemble the deterministic manifest dict.

    Returns ``(manifest, missing_captures)``. Entries are sorted by
    stable name; ``generated_from`` is the doc basename only; no
    timestamps appear anywhere in the body.
    """
    entries = parse_numbering_doc(doc_path)
    missing = find_missing_captures(entries, photos_dir)
    sorted_entries = sorted(entries, key=lambda e: e.stable)
    manifest = {
        "schema_version": SCHEMA_VERSION,
        "generated_from": Path(doc_path).name,
        "entries": [e.to_dict() for e in sorted_entries],
        "missing_captures": missing,
    }
    return manifest, missing


__all__ = [
    "SCHEMA_VERSION",
    "Entry",
    "NumberingDocError",
    "DuplicateStableError",
    "RotationHintError",
    "normalize_rotation",
    "parse_numbering_doc",
    "find_missing_captures",
    "build_manifest",
]
