"""Programmatic fixtures for `anvil:project-photos` tests (issue #599).

Builders drop a numbering doc and a sibling photos directory of tiny
placeholder image files under a caller-provided root. The placeholder
bytes are deterministic (a fixed 1x1-ish payload keyed by name) so the
SHA-256 zero-mutation test has stable content to hash — the skill never
reads image *bytes*, only the directory listing, so the payload can be
anything.

Layout produced by :func:`build_project`::

    <root>/
      numbering.md          # or numbering.csv
      photos/
        PXL_...jpg
        ...

Keeping the numbering doc BESIDE (not inside) the photos dir means the
default ``manifest.json`` write lands next to the doc, leaving the photos
dir provably untouched — the read-only contract under test.
"""

from __future__ import annotations

from pathlib import Path

# The four capture filenames the default doc references.
CAPTURE_A = "PXL_20231014_142301456.jpg"
CAPTURE_B = "PXL_20231014_143002789.jpg"
CAPTURE_C = "PXL_20231014_150511001.jpg"
CAPTURE_D = "PXL_20231014_151233222.jpg"

ALL_CAPTURES = (CAPTURE_A, CAPTURE_B, CAPTURE_C, CAPTURE_D)


def _placeholder_bytes(name: str) -> bytes:
    return (f"PLACEHOLDER-IMAGE:{name}\n").encode("utf-8")


def build_photos_dir(
    root: Path, captures=ALL_CAPTURES, *, extra_untracked=()
) -> Path:
    """Create ``<root>/photos/`` populated with placeholder image files.

    ``extra_untracked`` files are present on disk but (by convention) not
    referenced by the doc — used to prove they are silently ignored.
    """
    photos = Path(root) / "photos"
    photos.mkdir(parents=True, exist_ok=True)
    for name in list(captures) + list(extra_untracked):
        (photos / name).write_bytes(_placeholder_bytes(name))
    return photos


# Default markdown numbering doc exercising every column variant:
# - plain single-archive entry (CAPTURE_A → 042.jpg)
# - multi-item + descriptive rotation (CAPTURE_B → 043-multi.jpg, "upside down")
# - unnumbered capture with x-prefix + numeric rotation (CAPTURE_C → x017.jpg, 90)
# - series-prefixed name, no rotation (CAPTURE_D → wedding-005.jpg)
_DEFAULT_MD = f"""# Archive numbering

Human-authoritative capture → archive map.

| original | stable | archive_ids | rotation_hint |
|---|---|---|---|
| {CAPTURE_A} | 042.jpg | 42 | |
| {CAPTURE_B} | 043-multi.jpg | 43, 44 | upside down |
| {CAPTURE_C} | x017.jpg | 17 | 90 |
| {CAPTURE_D} | wedding-005.jpg | 5 | |
"""

# Same content as CSV, with the columns in a different order to prove
# name-based (not positional) column matching.
_DEFAULT_CSV = f"""stable,rotation_hint,original,archive_ids
042.jpg,,{CAPTURE_A},42
043-multi.jpg,upside down,{CAPTURE_B},"43, 44"
x017.jpg,90,{CAPTURE_C},17
wedding-005.jpg,,{CAPTURE_D},5
"""


def build_numbering_doc(root: Path, *, fmt: str = "md") -> Path:
    """Write the default numbering doc; ``fmt`` is ``"md"`` or ``"csv"``."""
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    if fmt == "csv":
        doc = root / "numbering.csv"
        doc.write_text(_DEFAULT_CSV, encoding="utf-8")
    else:
        doc = root / "numbering.md"
        doc.write_text(_DEFAULT_MD, encoding="utf-8")
    return doc


def build_project(root: Path, *, fmt: str = "md", extra_untracked=()) -> tuple:
    """Build a full fixture: numbering doc + photos dir. Returns (doc, dir)."""
    doc = build_numbering_doc(root, fmt=fmt)
    photos = build_photos_dir(root, extra_untracked=extra_untracked)
    return doc, photos


def write_doc(root: Path, body: str, *, name: str = "numbering.md") -> Path:
    """Write an arbitrary doc body (for edge-case / error tests)."""
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    doc = root / name
    doc.write_text(body, encoding="utf-8")
    return doc
