"""Atomicity tests for the red-team critic sibling (issue #560).

The red-team critic writes its sibling dir via the shared atomicity
primitive at ``anvil/lib/sidecar.py`` (the same primitive consumed by
45+ other critic-writing commands across the artifact-class skills).
The contract — staged_sidecar's manifest-verify-then-rename-atomically
shape, cleanup_one_staging's per-critic parallel-safe sweep — has its
own dedicated test suite at ``tests/lib/test_sidecar.py``; this file
is the skill-local discovery and shape regression-test anchor for the
red-team's specific use of the primitive.

The four assertions:

1. ``staging_path_for(<thread>.{N}.redteam)`` returns the expected
   leading-dot ``.tmp/`` shape.
2. A successful ``staged_sidecar`` context manager block renames the
   staging dir to the final ``<thread>.{N}.redteam/`` name.
3. A mid-cycle interrupt (simulated by manually creating the staging dir
   without going through the context manager) is removed by
   ``cleanup_one_staging`` on the next invocation; the final-named dir
   never exists in partial form.
4. ``discover_critics`` does NOT discover the staging dir (leading-dot
   shape is invisible to the discovery glob).

Per the per-skill test filename convention (#58), this file is named
``test_memo_redteam_atomicity.py``.
"""

from __future__ import annotations

from pathlib import Path

from anvil.lib.critics import discover_critics
from anvil.lib.sidecar import (
    cleanup_one_staging,
    staged_sidecar,
    staging_path_for,
)


def test_staging_path_shape_for_redteam(tmp_path):
    """staging_path_for produces the documented leading-dot .tmp/ shape."""
    final = tmp_path / "investment-memo.3.redteam"
    staging = staging_path_for(final)
    # Same parent (required for POSIX rename(2) atomicity).
    assert staging.parent == final.parent
    # Leading-dot + .tmp suffix.
    assert staging.name == ".investment-memo.3.redteam.tmp"


def test_staged_sidecar_clean_completion_renames_atomically(tmp_path):
    """A clean exit from staged_sidecar produces the final-named dir."""
    final = tmp_path / "investment-memo.3.redteam"
    required = ["_review.json", "objections.md", "_meta.json", "_progress.json"]

    with staged_sidecar(final_dir=final, required_files=required) as staging:
        # The staging dir lives at the leading-dot shape.
        assert staging.name == ".investment-memo.3.redteam.tmp"
        assert staging.exists()
        # The final-named dir does NOT exist yet — the rename happens at
        # context exit.
        assert not final.exists()
        # Write the required manifest files into the staging dir.
        for name in required:
            (staging / name).write_text("{}" if name.endswith(".json") else "stub")

    # On clean exit: staging dir gone, final-named dir present.
    assert not staging.exists()
    assert final.exists()
    # All four files made it through.
    for name in required:
        assert (final / name).exists()


def test_cleanup_one_staging_removes_orphan_redteam_staging(tmp_path):
    """A mid-cycle interrupt leaves a staging dir; cleanup_one_staging removes it.

    Simulates the killed-mid-run case: the staging dir exists, but the
    context manager never exited cleanly. The next invocation's
    cleanup_one_staging sweep removes the orphan; the final-named dir
    never exists in partial form.
    """
    final = tmp_path / "investment-memo.3.redteam"
    staging = staging_path_for(final)

    # Manually create the orphan staging dir (no atomic rename happened).
    staging.mkdir(parents=True, exist_ok=True)
    (staging / "_review.json").write_text('{"partial": true}')

    assert staging.exists()
    assert not final.exists()

    # Cleanup sweep removes the orphan.
    removed = cleanup_one_staging(final)
    assert removed is True
    assert not staging.exists()
    # The final-named dir still doesn't exist — partial output never
    # made it to the discoverable name.
    assert not final.exists()


def test_cleanup_one_staging_idempotent_on_no_staging(tmp_path):
    """cleanup_one_staging is idempotent: no-op on missing staging dir."""
    final = tmp_path / "investment-memo.3.redteam"
    # No staging dir exists.
    removed = cleanup_one_staging(final)
    assert removed is False  # nothing removed


def test_staging_dir_invisible_to_discover_critics(tmp_path):
    """A leading-dot .redteam.tmp/ staging dir is NOT discovered.

    Regression guard: discover_critics requires the candidate's name to
    start with ``<slug>.`` (no leading dot allowed; see anvil/lib/sidecar.py
    docstring §"safe from accidental discovery"). The red-team's staging
    dir uses the leading-dot + .tmp suffix shape and MUST NOT be picked
    up by discovery even when the matching version dir exists.
    """
    version_dir = tmp_path / "investment-memo.3"
    version_dir.mkdir()
    # Create the staging dir for the redteam sidecar (mid-cycle interrupt
    # shape) AND a normal review sibling.
    redteam_staging = staging_path_for(tmp_path / "investment-memo.3.redteam")
    redteam_staging.mkdir(parents=True)
    (redteam_staging / "_review.json").write_text('{"partial": true}')

    review_dir = tmp_path / "investment-memo.3.review"
    review_dir.mkdir()
    (review_dir / "_review.json").write_text("{}")

    siblings = discover_critics(version_dir)
    sibling_names = sorted(s.name for s in siblings)
    # The review sibling is found.
    # (The synthetic stub _review.json may not be valid; we assert at
    # the discovery layer only, which is filesystem-only — load_review is
    # NOT called.)
    assert "investment-memo.3.review" in sibling_names
    # The staging dir is INVISIBLE to discovery (leading-dot guard).
    for name in sibling_names:
        assert not name.startswith("."), (
            f"discover_critics should never return a leading-dot dir; got {name}"
        )
    assert ".investment-memo.3.redteam.tmp" not in sibling_names


def test_clean_redteam_sidecar_is_discoverable(tmp_path):
    """End-to-end: clean staged_sidecar → discover_critics finds it."""
    version_dir = tmp_path / "investment-memo.3"
    version_dir.mkdir()

    final = tmp_path / "investment-memo.3.redteam"
    required = ["_review.json", "objections.md", "_meta.json", "_progress.json"]

    with staged_sidecar(final_dir=final, required_files=required) as staging:
        # A minimally-valid _review.json for the discovery layer (no
        # schema validation here — discover_critics is filesystem-only).
        (staging / "_review.json").write_text("{}")
        (staging / "objections.md").write_text("# Objections")
        (staging / "_meta.json").write_text("{}")
        (staging / "_progress.json").write_text("{}")

    # Now discovery sees the renamed final dir.
    siblings = discover_critics(version_dir)
    sibling_names = [s.name for s in siblings]
    assert "investment-memo.3.redteam" in sibling_names
