"""Tests for ``anvil.skills.rubric-rebackport.lib.detect`` (issue #358).

Coverage:

- Legacy unstamped reviews surface in ``detect_unstamped_reviews``.
- Fully-stamped reviews do NOT surface.
- Partially-stamped (meta done, progress rows not) DO surface.
- Mixed-skill portfolios enumerate every review with the correct skill
  inference.
- An empty / non-anvil directory returns an empty inventory.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _skill_lib import detect  # noqa: E402
from _rebackport_fixtures import (  # noqa: E402
    build_fully_stamped,
    build_legacy_unstamped,
    build_mixed_skill_portfolio,
    build_partially_stamped,
)

detect_unstamped_reviews = detect.detect_unstamped_reviews
inventory_tree = detect.inventory_tree


class TestDetectUnstampedReviews(unittest.TestCase):
    def test_legacy_unstamped_surfaces(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            unstamped = detect_unstamped_reviews(project)
            self.assertEqual(len(unstamped), 1)
            r = unstamped[0]
            self.assertFalse(r.is_stamped)
            self.assertIsNotNone(r.progress_path)
            self.assertEqual(r.progress_score_history_unstamped_rows, 1)
            self.assertEqual(r.inferred_skill, "memo")
            self.assertIn(r.skill_source, {"brief", "body-filename"})

    def test_fully_stamped_does_not_surface(self) -> None:
        with TemporaryDirectory() as td:
            project = build_fully_stamped(Path(td))
            unstamped = detect_unstamped_reviews(project)
            self.assertEqual(len(unstamped), 0)

    def test_partially_stamped_surfaces(self) -> None:
        with TemporaryDirectory() as td:
            project = build_partially_stamped(Path(td))
            unstamped = detect_unstamped_reviews(project)
            # _meta.json is stamped but score_history rows are not, so
            # the review STILL surfaces.
            self.assertEqual(len(unstamped), 1)
            r = unstamped[0]
            self.assertTrue(r.is_stamped)
            self.assertEqual(r.progress_score_history_unstamped_rows, 1)

    def test_mixed_skill_portfolio_enumerates_both(self) -> None:
        with TemporaryDirectory() as td:
            project = build_mixed_skill_portfolio(Path(td))
            inv = inventory_tree(project)
            skills = sorted(
                r.inferred_skill or "unknown" for r in inv.reviews
            )
            self.assertEqual(skills, ["memo", "proposal"])

    def test_empty_dir_returns_empty_inventory(self) -> None:
        with TemporaryDirectory() as td:
            empty = Path(td) / "empty-project"
            empty.mkdir()
            inv = inventory_tree(empty)
            self.assertEqual(inv.reviews, [])

    def test_nonexistent_dir_returns_empty_inventory(self) -> None:
        with TemporaryDirectory() as td:
            missing = Path(td) / "does-not-exist"
            inv = inventory_tree(missing)
            self.assertEqual(inv.reviews, [])


class TestSkillInference(unittest.TestCase):
    def test_memo_review_inferred_via_brief(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            self.assertEqual(len(inv.reviews), 1)
            self.assertEqual(inv.reviews[0].inferred_skill, "memo")

    def test_proposal_review_inferred(self) -> None:
        with TemporaryDirectory() as td:
            project = build_mixed_skill_portfolio(Path(td))
            inv = inventory_tree(project)
            proposal_reviews = [
                r for r in inv.reviews if r.inferred_skill == "proposal"
            ]
            self.assertEqual(len(proposal_reviews), 1)


if __name__ == "__main__":
    unittest.main()
