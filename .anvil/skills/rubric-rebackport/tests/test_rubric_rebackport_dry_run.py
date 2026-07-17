"""Tests for `anvil:rubric-rebackport` dry-run no-mutation contract (issue #358).

The skill's dry-run contract is load-bearing: detect + plan + report
must NEVER mutate the input tree. This file uses the snapshot-and-diff
approach — compute a hash of every file in the project tree before
running `orchestrate.run(project, apply=False)`, run it, then compute
the hash again and assert byte-identity.
"""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _skill_lib import orchestrate, plan  # noqa: E402
from _rebackport_fixtures import (  # noqa: E402
    build_fully_stamped,
    build_legacy_unstamped,
    build_mixed_skill_portfolio,
    build_partially_stamped,
)

run = orchestrate.run
Mode = plan.Mode


def _tree_hash(project: Path) -> dict:
    out: dict = {}
    for path in sorted(project.rglob("*")):
        if path.is_file():
            rel = str(path.relative_to(project))
            out[rel] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


class TestDryRunNoMutations(unittest.TestCase):
    def test_legacy_dry_run_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            before = _tree_hash(project)
            result = run(project, mode=Mode.STAMP_ONLY, apply=False)
            self.assertTrue(result.success)
            after = _tree_hash(project)
            self.assertEqual(
                before, after, "dry-run mutated the project tree"
            )

    def test_partially_stamped_dry_run_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_partially_stamped(Path(td))
            before = _tree_hash(project)
            run(project, mode=Mode.STAMP_ONLY, apply=False)
            after = _tree_hash(project)
            self.assertEqual(before, after)

    def test_fully_stamped_dry_run_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_fully_stamped(Path(td))
            before = _tree_hash(project)
            run(project, mode=Mode.STAMP_ONLY, apply=False)
            after = _tree_hash(project)
            self.assertEqual(before, after)

    def test_mixed_portfolio_dry_run_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_mixed_skill_portfolio(Path(td))
            before = _tree_hash(project)
            run(project, mode=Mode.STAMP_ONLY, apply=False)
            after = _tree_hash(project)
            self.assertEqual(before, after)

    def test_report_mode_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            before = _tree_hash(project)
            result = run(
                project, mode=Mode.STAMP_ONLY, apply=False,
                report_only=True,
            )
            self.assertTrue(result.success)
            after = _tree_hash(project)
            self.assertEqual(before, after)

    def test_rescore_dry_run_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            before = _tree_hash(project)
            run(
                project,
                mode=Mode.RESCORE,
                legacy_rubric="anvil-memo-v1-legacy-40",
                apply=False,
            )
            after = _tree_hash(project)
            self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
