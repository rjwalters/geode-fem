"""Tests for ``anvil.skills.rubric-rebackport.lib.verify`` (issue #358).

Covers post-apply verification:

- After a stamp-only apply, every touched ``_meta.json`` carries all
  three required fields, and every touched progress file's
  score_history rows carry rubric_id.
- An apply failure produces verify failures (no false-positive PASS).
- Fully-stamped input verifies PASS without writing.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _skill_lib import apply_mod, detect, orchestrate, plan, verify  # noqa: E402
from _rebackport_fixtures import (  # noqa: E402
    build_fully_stamped,
    build_legacy_unstamped,
    build_mixed_skill_portfolio,
)

apply_plan = apply_mod.apply_plan
inventory_tree = detect.inventory_tree
run = orchestrate.run
Mode = plan.Mode
build_plan = plan.build_plan
verify_rebackport = verify.verify_rebackport


class TestVerifyAfterStampOnly(unittest.TestCase):
    def test_legacy_apply_then_verify_passes(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            result = run(project, mode=Mode.STAMP_ONLY, apply=True)
            self.assertTrue(result.success)
            self.assertIsNotNone(result.verify_result)
            self.assertTrue(result.verify_result.ok)
            self.assertEqual(
                len(result.verify_result.reviews_stamped_ok), 1
            )

    def test_mixed_portfolio_verify_covers_both_skills(self) -> None:
        with TemporaryDirectory() as td:
            project = build_mixed_skill_portfolio(Path(td))
            result = run(project, mode=Mode.STAMP_ONLY, apply=True)
            self.assertTrue(result.success)
            self.assertEqual(
                len(result.verify_result.reviews_stamped_ok), 2
            )

    def test_fully_stamped_verify_passes_without_apply(self) -> None:
        with TemporaryDirectory() as td:
            project = build_fully_stamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            verify_result = verify_rebackport(project, p)
            self.assertTrue(verify_result.ok)


class TestVerifyFailuresSurface(unittest.TestCase):
    def test_unstamped_review_after_partial_apply_surfaces_failure(
        self,
    ) -> None:
        """Construct a plan/apply mismatch and confirm verify catches it."""
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            apply_plan(p)
            inv2 = inventory_tree(project)
            meta_path = inv2.reviews[0].meta_path
            import json
            data = json.loads(meta_path.read_text())
            data.pop("rubric_id", None)
            meta_path.write_text(json.dumps(data, indent=2) + "\n")
            verify_result = verify_rebackport(project, p)
            self.assertFalse(verify_result.ok)
            self.assertEqual(len(verify_result.failures), 1)


if __name__ == "__main__":
    unittest.main()
