"""Tests for `anvil:rubric-rebackport` idempotence (issue #358).

Re-running `--apply` on a fully-stamped project must be a zero-diff
no-op. This is the safety net for operators who lose track of which
projects they've already rebackported.
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


class TestIdempotence(unittest.TestCase):
    def test_fully_stamped_apply_is_noop(self) -> None:
        with TemporaryDirectory() as td:
            project = build_fully_stamped(Path(td))
            before = _tree_hash(project)
            result = run(project, mode=Mode.STAMP_ONLY, apply=True)
            self.assertTrue(result.success)
            after = _tree_hash(project)
            self.assertEqual(
                before, after,
                "apply on fully-stamped project mutated the tree",
            )

    def test_double_apply_legacy_is_idempotent(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            first = run(project, mode=Mode.STAMP_ONLY, apply=True)
            self.assertTrue(first.success)
            tree_after_first = _tree_hash(project)
            second = run(project, mode=Mode.STAMP_ONLY, apply=True)
            self.assertTrue(second.success)
            tree_after_second = _tree_hash(project)
            self.assertEqual(
                tree_after_first,
                tree_after_second,
                "second apply mutated an already-stamped project",
            )

    def test_double_apply_rescore_is_idempotent(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            run(
                project,
                mode=Mode.RESCORE,
                legacy_rubric="anvil-memo-v1-legacy-40",
                apply=True,
                allow_rescore_subprocess=True,
            )
            tree_after_first = _tree_hash(project)
            run(
                project,
                mode=Mode.RESCORE,
                legacy_rubric="anvil-memo-v1-legacy-40",
                apply=True,
                allow_rescore_subprocess=True,
            )
            tree_after_second = _tree_hash(project)
            self.assertEqual(
                tree_after_first, tree_after_second,
                "second rescore apply mutated the tree",
            )


if __name__ == "__main__":
    unittest.main()
