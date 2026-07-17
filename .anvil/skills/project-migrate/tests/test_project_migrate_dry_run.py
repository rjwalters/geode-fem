"""Tests for `anvil:project-migrate` dry-run no-mutation contract (issue #297).

The skill's dry-run contract is load-bearing: detect + plan + report must
NEVER mutate the input tree. This file uses the snapshot-and-diff
approach — compute a hash of every file in the project tree before
running `orchestrate.run(project, apply=False)`, run it, then compute
the hash again and assert byte-identity.

Acceptance criterion: dry-run never mutates (verified by snapshot-and-diff).
"""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _project_migrate_skill_lib import orchestrate  # noqa: E402
from _fixtures import (  # noqa: E402
    build_bessemer_shaped,
    build_fully_migrated,
    build_post_283_anvil_json,
    build_pre_283_classic,
)

run = orchestrate.run


def _tree_hash(project: Path) -> dict:
    """Return ``{relative_path: sha256_hex}`` for every file in ``project``."""
    out: dict = {}
    for path in sorted(project.rglob("*")):
        if path.is_file():
            rel = str(path.relative_to(project))
            out[rel] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


class TestDryRunNoMutations(unittest.TestCase):
    def test_pre_283_dry_run_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_pre_283_classic(Path(td))
            before = _tree_hash(project)
            result = run(project, apply=False)
            self.assertTrue(result.success)
            after = _tree_hash(project)
            self.assertEqual(
                before,
                after,
                "dry-run mutated the project tree (pre-#283 fixture)",
            )

    def test_post_283_dry_run_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_post_283_anvil_json(Path(td))
            before = _tree_hash(project)
            result = run(project, apply=False)
            self.assertTrue(result.success)
            after = _tree_hash(project)
            self.assertEqual(
                before,
                after,
                "dry-run mutated the project tree (post-#283 fixture)",
            )

    def test_fully_migrated_dry_run_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_fully_migrated(Path(td))
            before = _tree_hash(project)
            result = run(project, apply=False)
            self.assertTrue(result.success)
            after = _tree_hash(project)
            self.assertEqual(before, after)

    def test_bessemer_dry_run_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_bessemer_shaped(Path(td))
            before = _tree_hash(project)
            result = run(project, apply=False)
            self.assertTrue(result.success)
            after = _tree_hash(project)
            self.assertEqual(
                before,
                after,
                "dry-run mutated the bessemer fixture",
            )

    def test_report_mode_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_pre_283_classic(Path(td))
            before = _tree_hash(project)
            result = run(project, apply=False, report_only=True)
            self.assertTrue(result.success)
            after = _tree_hash(project)
            self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
