"""Single-entry orchestration for `anvil:project-photos` (issue #599).

``run(photos_dir, numbering_doc, ...)`` parses the numbering doc, builds
the deterministic manifest, and (unless ``dry_run``) writes it. The scan
is **strictly read-only over the source images**: the only write anywhere
is the operator-requested ``manifest.json`` output — beside the numbering
doc by default, or to ``json_path`` when given. ``dry_run`` writes
nothing.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from .manifest import NumberingDocError, build_manifest


@dataclass
class PhotosResult:
    success: bool
    manifest: dict
    manifest_json: str
    output_path: Optional[Path]
    missing_captures: list = field(default_factory=list)
    warnings: list = field(default_factory=list)


def _serialize(manifest: dict) -> str:
    """Deterministic serialization: sorted keys, trailing newline.

    ``sort_keys=True`` fixes intra-dict key order; the ``entries`` list is
    already sorted by stable name in :func:`build_manifest`. Together
    these guarantee byte-identical output across runs.
    """
    return json.dumps(manifest, indent=2, sort_keys=True) + "\n"


def run(
    photos_dir: Path,
    numbering_doc: Path,
    dry_run: bool = False,
    json_path: Optional[Path] = None,
) -> PhotosResult:
    """Build the provenance manifest for ``photos_dir`` from ``numbering_doc``.

    ``success`` is False when the numbering doc is malformed (parse error
    surfaced in ``warnings``) or when any capture listed in the doc is
    missing from ``photos_dir`` (surfaced in ``missing_captures``). The
    operator-facing command translates a False result into a nonzero exit.

    Output location:
    - ``dry_run=True`` → nothing is written (``output_path`` is None).
    - ``json_path`` given → written there.
    - otherwise → ``manifest.json`` beside the numbering doc.
    """
    photos_dir = Path(photos_dir).resolve()
    numbering_doc = Path(numbering_doc).resolve()
    warnings: list = []

    try:
        manifest, missing = build_manifest(numbering_doc, photos_dir)
    except NumberingDocError as exc:
        return PhotosResult(
            success=False,
            manifest={},
            manifest_json="",
            output_path=None,
            missing_captures=[],
            warnings=[str(exc)],
        )

    manifest_json = _serialize(manifest)

    if not photos_dir.is_dir():
        warnings.append(f"photos dir {photos_dir} is not a directory")
    if missing:
        warnings.append(
            f"{len(missing)} capture(s) listed in "
            f"{numbering_doc.name} are missing from {photos_dir.name}/: "
            + ", ".join(missing)
        )

    output_path: Optional[Path] = None
    if not dry_run:
        if json_path is not None:
            output_path = Path(json_path)
        else:
            output_path = numbering_doc.parent / "manifest.json"
        output_path.write_text(manifest_json, encoding="utf-8")

    success = photos_dir.is_dir() and not missing

    return PhotosResult(
        success=success,
        manifest=manifest,
        manifest_json=manifest_json,
        output_path=output_path,
        missing_captures=missing,
        warnings=warnings,
    )


__all__ = ["PhotosResult", "run"]
