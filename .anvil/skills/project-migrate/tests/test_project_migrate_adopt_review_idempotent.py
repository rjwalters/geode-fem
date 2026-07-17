"""Idempotence tests for `--adopt-review` (issue #454 — Phase 3a of #432).

Re-running `--apply` on a tree where the stub `_review.json` already
exists is a no-op (empty plan, zero diff).
"""

from __future__ import annotations

import hashlib

from _fixtures import build_adopted_review_threads
from _project_migrate_skill_lib import orchestrate

run_adopt_review = orchestrate.run_adopt_review


def _tree_digest(root) -> str:
    h = hashlib.sha256()
    for p in sorted(root.rglob("*")):
        h.update(str(p.relative_to(root)).encode("utf-8"))
        if p.is_file():
            h.update(p.read_bytes())
    return h.hexdigest()


class TestIdempotent:
    def test_second_apply_is_noop(self, tmp_path):
        project = build_adopted_review_threads(tmp_path)

        first = run_adopt_review(project, apply=True)
        assert first.success
        assert len(first.apply_result.converted) == 4

        after_first = _tree_digest(project)

        second = run_adopt_review(project, apply=True)
        assert second.success
        assert second.plan.is_noop
        # No conversions, no apply mutations.
        assert second.apply_result is None
        # Zero diff between the two runs.
        assert _tree_digest(project) == after_first

    def test_pre_converted_tree_is_noop(self, tmp_path):
        project = build_adopted_review_threads(tmp_path, pre_converted=True)
        before = _tree_digest(project)
        result = run_adopt_review(project, apply=True)
        assert result.success
        assert result.plan.is_noop
        assert _tree_digest(project) == before

    def test_dry_run_then_apply_then_rerun(self, tmp_path):
        project = build_adopted_review_threads(tmp_path)
        # Dry-run is a no-op.
        run_adopt_review(project)
        # Apply converts.
        run_adopt_review(project, apply=True)
        snapshot = _tree_digest(project)
        # A third (dry-run) pass still sees nothing to do.
        third = run_adopt_review(project)
        assert third.plan.is_noop
        assert _tree_digest(project) == snapshot
