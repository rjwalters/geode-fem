"""Rubric overlay selection for non-investment-memo artifact types
(issue #286, sub-deliverable 3 of #283; absorbs closed #278).

Given a thread root, walk up to find ``<project>/BRIEF.md``, parse its
``documents:`` list, find the entry matching the thread's slug, read its
``artifact_type``, and return the matching rubric overlay resolved
two-tier (issue #394) — consumer first, shipped second:

1. **Consumer tier**:
   ``<consumer>/.anvil/skills/memo/rubric_overlays/<artifact-type>.json``
   where ``<consumer>`` carries the ``.anvil/`` install marker. This is
   how a consumer declares a memo genre with no framework release, and
   also how a consumer recalibrates a *shipped* type without forking
   anvil (consumer wins on collision — the ``discover_venue_rubric``
   precedent from the paper skill).
2. **Shipped tier**: ``anvil/skills/memo/rubric_overlays/<artifact-type>.json``.

Both tiers are parsed with the same strict schema (dim-key validation,
filename ↔ declared-type consistency, ``OverlayLoadError`` on any
malformation).

The overlay declares per-dimension ``weight_adjustments`` (deltas
applied to the base ``rubric.md`` weights) plus optional
``calibration_prose`` strings that the reviewer attaches to its
calibration suffix (analogous to the per-thread ``rubric_overrides``
``dim_N_calibration`` mechanism from issue #233).

Composition order (top-to-bottom precedence, last-wins):

    base /44 rubric (rubric.md)
        + artifact-type overlay (this module)
            + per-doc rubric_overrides (project_brief.py / issue #233 + #296)

The investment-memo overlay is identity (zero adjustments) — a thread
with ``artifact_type: investment-memo`` in its project BRIEF behaves
byte-identically to a thread with no project BRIEF at all (the v0
status quo).

Public API
----------

``load_overlay(artifact_type, consumer_overlays_dir=None) -> RubricOverlay``
    Load the overlay JSON for one artifact type (a registered
    ``ArtifactType`` member or a consumer-declared type string —
    issue #394). Resolution order: ``consumer_overlays_dir`` first
    (when supplied), shipped ``OVERLAYS_DIR`` second. Raises
    ``OverlayLoadError`` (subclass of ``ValueError``) on missing or
    malformed overlay files.

``select_overlay_for_thread(thread_dir, project_dir=None, consumer_root=None) -> RubricOverlay | None``
    Resolve a thread's overlay by walking to the project BRIEF, finding
    the matching slug, reading its ``artifact_type``, and loading the
    overlay (two-tier — consumer first, shipped second). Returns
    ``None`` when no project BRIEF is found (back-compat for threads
    outside the portfolio-as-thread-root layout) or when the thread's
    slug is not listed in the BRIEF. Raises a clear skill-mismatch
    ``OverlayLoadError`` when the entry declares a non-memo
    skill-identity type — issue #386, keyed on the explicit
    ``SKILL_IDENTITY_ARTIFACT_TYPES`` set (``deck`` / ``slides`` /
    ``proposal``) since #394, so consumer-declared memo types don't
    trip the rejection: silently scoring a deck against the memo rubric
    would be worse than failing loudly.

``RubricOverlay``
    Typed Pydantic model. Fields: ``artifact_type``, ``description``,
    ``weight_adjustments`` (dict ``"dim_1"``...``"dim_9"`` → int delta),
    ``calibration_prose`` (dict ``"dim_1"``...``"dim_9"`` → str).

``OVERLAYS_DIR``
    Path to the shipped overlay JSON directory.

Skill-local first
-----------------

Lives under ``anvil/skills/memo/lib/`` per CLAUDE.md "skill-local first,
lib promotion later". Lib promotion is queued for the second-consumer
trigger (the proposal skill may eventually want artifact-type overlays).

No new Python deps
------------------

Reuses ``pydantic`` (already declared) and stdlib ``json``,
``pathlib``, ``typing``. Imports from the sibling ``project_brief``
(``ArtifactType``, ``load_project_brief``) and ``project_discovery``
(``discover_thread_root``) modules.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Dict, Optional, Union

from pydantic import BaseModel, ConfigDict, Field, ValidationError

from anvil.skills.memo.lib.project_brief import (
    ArtifactType,
    MEMO_ARTIFACT_TYPES,
    SKILL_IDENTITY_ARTIFACT_TYPES,
    consumer_overlay_dir_for,
    load_project_brief,
)
from anvil.skills.memo.lib.project_discovery import discover_thread_root


OVERLAYS_DIR: Path = Path(__file__).parent.parent / "rubric_overlays"

# Dim keys recognized in weight_adjustments and calibration_prose. The
# base rubric in `rubric.md` defines dims 1-9 summing to 44; an overlay
# may carry a key for any subset of these.
_DIM_KEYS: tuple[str, ...] = tuple(f"dim_{n}" for n in range(1, 10))


class OverlayLoadError(ValueError):
    """Raised when an overlay JSON file cannot be loaded or validated."""


class RubricOverlay(BaseModel):
    """A rubric overlay for one artifact type.

    ``weight_adjustments`` is a sparse dict: a key ``"dim_3": 2`` adds
    +2 to the base rubric's dim 3 weight. Negative values reduce
    weight. Missing keys mean "no adjustment". The reviewer is
    responsible for applying the adjustment and clamping to non-negative
    integers (no overlay shipped today drives any dim negative).

    ``calibration_prose`` is a sparse dict of per-dim prose strings the
    reviewer appends to its calibration suffix, mirroring the per-thread
    ``dim_N_calibration`` mechanism from issue #233 but selected by the
    project BRIEF's ``artifact_type`` rather than per-thread config.
    """

    model_config = ConfigDict(extra="forbid", frozen=True)

    # Union keeps already-typed ArtifactType instances as enum members
    # while letting consumer-declared types (issue #394) — and the
    # plain strings JSON parsing produces — pass through as validated
    # str. str-enum members and plain strings interoperate for
    # equality / hashing, so callers can compare either way.
    artifact_type: Union[ArtifactType, str] = Field(
        ...,
        description="The artifact type this overlay applies to — a "
        "registered ArtifactType value or a consumer-declared type "
        "(issue #394).",
    )
    description: str = Field(
        ...,
        description="One-paragraph rationale for the per-dim choices; "
        "shown in overlay registry docs.",
    )
    weight_adjustments: Dict[str, int] = Field(
        default_factory=dict,
        description="Sparse dict of dim_N → integer delta. Keys not "
        "in dim_1..dim_9 are rejected.",
    )
    calibration_prose: Dict[str, str] = Field(
        default_factory=dict,
        description="Sparse dict of dim_N → prose string the reviewer "
        "appends to its calibration suffix.",
    )

    def is_identity(self) -> bool:
        """True iff every weight adjustment is 0 and no calibration prose.

        The investment-memo overlay is the canonical identity overlay —
        it exists so the registry is complete, but applying it is a no-op.
        """
        if any(v != 0 for v in self.weight_adjustments.values()):
            return False
        if any(self.calibration_prose.values()):
            return False
        return True


def _validate_dim_keys(d: Dict[str, object], field: str, source: Path) -> None:
    """Raise OverlayLoadError if any key in ``d`` is not dim_1..dim_9."""
    for key in d:
        if key not in _DIM_KEYS:
            raise OverlayLoadError(
                f"{source}: {field!r} contains unknown key {key!r}. "
                f"Allowed keys: {list(_DIM_KEYS)}."
            )


def _artifact_type_value(artifact_type: Union[ArtifactType, str]) -> str:
    """Return the string value of a registered-or-consumer artifact type."""
    if isinstance(artifact_type, ArtifactType):
        return artifact_type.value
    return str(artifact_type)


def load_overlay(
    artifact_type: Union[ArtifactType, str],
    consumer_overlays_dir: Optional[Path] = None,
) -> RubricOverlay:
    """Load the overlay JSON for one artifact type (two-tier per #394).

    Resolution order — first existing file wins:

    1. ``<consumer_overlays_dir>/<type>.json`` (when supplied) — the
       consumer tier. Lets a consumer declare new memo genres AND
       recalibrate shipped types without forking anvil.
    2. ``OVERLAYS_DIR/<type>.json`` — the shipped registry.

    Both tiers are parsed with identical strictness.

    Raises
    ------
    OverlayLoadError
        If no overlay file exists in either tier, the file contains
        invalid JSON, fails schema validation, declares the wrong
        artifact_type, or uses an unknown dim key in
        weight_adjustments / calibration_prose.
    """
    type_value = _artifact_type_value(artifact_type)
    if (
        not type_value
        or "/" in type_value
        or "\\" in type_value
        or type_value in {".", ".."}
    ):
        raise OverlayLoadError(
            f"artifact_type {type_value!r} is not a valid overlay slug "
            "(must be a bare filename stem with no path separators)."
        )
    candidates = []
    if consumer_overlays_dir is not None:
        candidates.append(Path(consumer_overlays_dir) / f"{type_value}.json")
    candidates.append(OVERLAYS_DIR / f"{type_value}.json")

    overlay_path = next((p for p in candidates if p.is_file()), None)
    if overlay_path is None:
        registered = sorted(p.stem for p in OVERLAYS_DIR.glob("*.json"))
        consumer_note = ""
        if consumer_overlays_dir is not None:
            consumer_dir = Path(consumer_overlays_dir)
            consumer_types = (
                sorted(p.stem for p in consumer_dir.glob("*.json"))
                if consumer_dir.is_dir()
                else []
            )
            consumer_note = (
                f" Consumer overlays at {consumer_dir}: {consumer_types}."
            )
        raise OverlayLoadError(
            f"No overlay file found for artifact_type={type_value!r} "
            f"at {candidates[-1]}. Registered overlays: {registered}."
            f"{consumer_note}"
        )
    try:
        raw = json.loads(overlay_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise OverlayLoadError(f"{overlay_path}: invalid JSON — {exc}") from exc

    if not isinstance(raw, dict):
        raise OverlayLoadError(
            f"{overlay_path}: top-level must be a JSON object, got "
            f"{type(raw).__name__}."
        )

    # Dim-key validation BEFORE Pydantic — Pydantic's dict[str, int]
    # accepts any string keys; we want a clear error message naming
    # the unknown key.
    if isinstance(raw.get("weight_adjustments"), dict):
        _validate_dim_keys(raw["weight_adjustments"], "weight_adjustments", overlay_path)
    if isinstance(raw.get("calibration_prose"), dict):
        _validate_dim_keys(raw["calibration_prose"], "calibration_prose", overlay_path)

    try:
        overlay = RubricOverlay(**raw)
    except ValidationError as exc:
        raise OverlayLoadError(f"{overlay_path}: schema error — {exc}") from exc

    # Filename ↔ artifact_type consistency. Catches typos where the
    # overlay JSON declares one type but lives under a different filename.
    # str-value comparison works uniformly across enum members and
    # consumer-declared plain strings.
    if _artifact_type_value(overlay.artifact_type) != type_value:
        raise OverlayLoadError(
            f"{overlay_path}: declares artifact_type="
            f"{_artifact_type_value(overlay.artifact_type)!r} but expected "
            f"{type_value!r} (filename mismatch)."
        )

    return overlay


def select_overlay_for_thread(
    thread_dir: Path,
    project_dir: Optional[Path] = None,
    consumer_root: Optional[Path] = None,
) -> Optional[RubricOverlay]:
    """Resolve and load the overlay for a thread, or None if not applicable.

    Walks up from ``thread_dir`` via :func:`project_discovery.discover_thread_root`
    to find the project BRIEF. If the thread's slug appears in the BRIEF's
    ``documents:`` list, loads the overlay matching that entry's
    ``artifact_type`` (two-tier resolution — consumer overlay registry
    first, shipped registry second; issue #394). Returns ``None`` when:

    - No project BRIEF is found on the walk-upward path (classic layout
      thread — preserves v0 behavior; no overlay applied).
    - The project BRIEF is found but the thread's slug is not in its
      ``documents:`` list (the operator may have added the thread on
      disk but not yet registered it — degrade silently to identity).

    Raises
    ------
    OverlayLoadError
        Propagated from :func:`load_overlay` if the overlay file is
        missing or malformed.

    Parameters
    ----------
    thread_dir
        The thread root directory (e.g. ``<project>/investment-memo/``).
    project_dir
        Optional project root override. When supplied, the function
        skips :func:`discover_thread_root` and reads the BRIEF directly
        from ``<project_dir>/BRIEF.md``. Useful for callers that already
        know the project root.
    consumer_root
        Optional explicit consumer root for the #394 consumer overlay
        tier. When ``None`` (default) the consumer root is discovered
        by walking upward from the project root to the ``.anvil/``
        install marker; when no marker exists the consumer tier is
        skipped (shipped overlays only).
    """
    thread_dir = Path(thread_dir)

    if project_dir is None:
        discovery = discover_thread_root(thread_dir)
        if discovery is None or discovery.project_root is None:
            return None
        project_dir = discovery.project_root
        thread_slug = discovery.slug
    else:
        project_dir = Path(project_dir)
        thread_slug = thread_dir.name

    brief = load_project_brief(project_dir, consumer_root=consumer_root)
    if brief is None:
        return None

    for entry in brief.documents:
        if entry.slug == thread_slug:
            # Skill-identity guard (issue #386; re-keyed explicit under
            # #394). Skill-identity artifact types (deck / slides /
            # proposal) are registered in the shared enum but select NO
            # memo overlay — a memo command running against such a
            # thread is operator error that deserves a loud,
            # self-explaining failure instead of a confusing "No
            # overlay file found" message (or worse, a silent identity
            # overlay scoring a deck against the memo rubric). The
            # guard is keyed on the explicit SKILL_IDENTITY set, NOT
            # "anything outside MEMO_ARTIFACT_TYPES" — consumer-declared
            # memo types (#394) are legitimately outside the registered
            # memo subset and must flow through to load_overlay.
            if entry.artifact_type in SKILL_IDENTITY_ARTIFACT_TYPES:
                memo_values = sorted(t.value for t in MEMO_ARTIFACT_TYPES)
                raise OverlayLoadError(
                    f"Thread {thread_slug!r} declares artifact_type="
                    f"{entry.artifact_type.value!r} in the project BRIEF — "
                    f"this looks like a {entry.artifact_type.value!r} "
                    f"thread, not a memo. Memo rubric overlays apply only "
                    f"to memo artifact types: {memo_values}. Run the "
                    f"owning skill's commands (anvil:"
                    f"{entry.artifact_type.value}) against this thread "
                    f"instead, or fix the BRIEF entry if the type is wrong."
                )
            # Memo-registered OR consumer-declared types resolve
            # two-tier (consumer wins). Defense in depth: a type that
            # is neither (e.g. its consumer overlay JSON was deleted
            # after the BRIEF was parsed) fails loudly inside
            # load_overlay with the available-set error.
            return load_overlay(
                entry.artifact_type,
                consumer_overlays_dir=consumer_overlay_dir_for(
                    project_dir, consumer_root
                ),
            )

    # Thread is in a project but not listed in the BRIEF — preserve
    # v0 behavior (no overlay).
    return None


__all__ = [
    "OVERLAYS_DIR",
    "OverlayLoadError",
    "RubricOverlay",
    "load_overlay",
    "select_overlay_for_thread",
]
