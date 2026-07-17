"""Tests for ``anvil.skills.rubric-rebackport.lib.apply`` (issue #358)."""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _skill_lib import apply_mod, detect, plan  # noqa: E402
from _rebackport_fixtures import (  # noqa: E402
    build_fully_stamped,
    build_legacy_unstamped,
    build_partially_stamped,
)

ROLLBACK_SUBDIR = apply_mod.ROLLBACK_SUBDIR
apply_plan = apply_mod.apply_plan
inventory_tree = detect.inventory_tree
Mode = plan.Mode
build_plan = plan.build_plan


class TestApplyStampOnly(unittest.TestCase):
    def test_legacy_apply_stamps_everything(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            result = apply_plan(p)
            self.assertEqual(len(result.failed_reviews), 0)
            inv2 = inventory_tree(project)
            r2 = inv2.reviews[0]
            self.assertTrue(r2.is_stamped)
            self.assertEqual(
                r2.progress_score_history_unstamped_rows, 0
            )
            self.assertTrue(r2.summary_has_rubric_block)

    def test_apply_writes_correct_rubric_id(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            apply_plan(p)
            meta_path = inv.reviews[0].meta_path
            data = json.loads(meta_path.read_text())
            self.assertEqual(
                data["rubric_id"], "anvil-memo-v1-legacy-40"
            )
            self.assertEqual(data["rubric_total"], 40)
            self.assertEqual(data["advance_threshold"], 32)

    def test_apply_writes_progress_row_rubric_id(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            progress_path = inv.reviews[0].progress_path
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            apply_plan(p)
            data = json.loads(progress_path.read_text())
            history = data["metadata"]["score_history"]
            self.assertEqual(len(history), 1)
            self.assertEqual(
                history[0]["rubric_id"], "anvil-memo-v1-legacy-40"
            )

    def test_apply_with_operator_assertion(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(
                inv,
                mode=Mode.STAMP_ONLY,
                legacy_rubric="anvil-memo-v2",
            )
            apply_plan(p)
            meta_path = inv.reviews[0].meta_path
            data = json.loads(meta_path.read_text())
            self.assertEqual(data["rubric_id"], "anvil-memo-v2")
            self.assertEqual(data["rubric_total"], 44)
            self.assertEqual(data["advance_threshold"], 35)

    def test_apply_preserves_existing_meta_fields(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            meta_path = inv.reviews[0].meta_path
            data_before = json.loads(meta_path.read_text())
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            apply_plan(p)
            data_after = json.loads(meta_path.read_text())
            for k, v in data_before.items():
                if k == "rubric_total":
                    self.assertEqual(data_after[k], v)
                else:
                    self.assertEqual(data_after.get(k), v)

    def test_apply_partially_stamped_only_touches_progress(self) -> None:
        with TemporaryDirectory() as td:
            project = build_partially_stamped(Path(td))
            inv = inventory_tree(project)
            meta_path = inv.reviews[0].meta_path
            meta_before = meta_path.read_text()
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            apply_plan(p)
            meta_after = meta_path.read_text()
            self.assertEqual(meta_before, meta_after)
            inv2 = inventory_tree(project)
            self.assertEqual(
                inv2.reviews[0].progress_score_history_unstamped_rows, 0
            )

    def test_apply_fully_stamped_is_noop(self) -> None:
        with TemporaryDirectory() as td:
            project = build_fully_stamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            result = apply_plan(p)
            self.assertEqual(len(result.applied_reviews), 0)
            self.assertEqual(len(result.failed_reviews), 0)


class TestRollbackHousekeeping(unittest.TestCase):
    def test_rollback_dir_removed_after_success(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            apply_plan(p)
            rollback = project / ROLLBACK_SUBDIR
            self.assertFalse(
                rollback.exists(),
                "rollback dir should be cleaned up on success",
            )


if __name__ == "__main__":
    unittest.main()
