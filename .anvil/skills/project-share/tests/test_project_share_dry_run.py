"""Tests for `anvil:project-share` dry-run no-mutation contract (issue #396).

`--dry-run` must be SHA-256-verifiably side-effect-free: snapshot every
file in the project tree, run `orchestrate.run(project, dry_run=True)`,
snapshot again, and assert byte-identity (mirrors the
rubric-rebackport dry-run suite).
"""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _project_share_skill_lib import orchestrate  # noqa: E402
from _share_fixtures import (  # noqa: E402
    build_full_project,
    build_project_with_unstarted_slug,
)


def _tree_hash(project: Path) -> dict:
    out: dict = {}
    for path in sorted(project.rglob("*")):
        if path.is_file():
            rel = str(path.relative_to(project))
            out[rel] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


class TestDryRunNoMutations(unittest.TestCase):
    def test_dry_run_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            project = build_full_project(Path(td))
            before = _tree_hash(project)
            result = orchestrate.run(project, dry_run=True)
            self.assertTrue(result.success)
            after = _tree_hash(project)
            self.assertEqual(
                before, after, "dry-run mutated the project tree"
            )
            self.assertFalse((project / "SHARE").exists())

    def test_dry_run_with_zip_flag_writes_nothing(self) -> None:
        with TemporaryDirectory() as td:
            project = build_full_project(Path(td))
            before = _tree_hash(project)
            orchestrate.run(project, dry_run=True, zip_output=True)
            after = _tree_hash(project)
            self.assertEqual(before, after)
            self.assertEqual(list(project.glob("*.zip")), [])

    def test_dry_run_over_foreign_out_dir_writes_nothing(self) -> None:
        with TemporaryDirectory() as td:
            project = build_full_project(Path(td))
            foreign = project / "SHARE"
            foreign.mkdir()
            (foreign / "precious.txt").write_text("keep me\n")
            before = _tree_hash(project)
            orchestrate.run(project, dry_run=True)
            after = _tree_hash(project)
            self.assertEqual(before, after)
            self.assertTrue((foreign / "precious.txt").is_file())

    def test_dry_run_with_failed_doc_reports_failure(self) -> None:
        with TemporaryDirectory() as td:
            project = build_project_with_unstarted_slug(Path(td))
            before = _tree_hash(project)
            result = orchestrate.run(project, dry_run=True)
            after = _tree_hash(project)
            self.assertEqual(before, after)
            # The unresolved doc surfaces as a failure even in dry-run.
            self.assertFalse(result.success)
            self.assertIn("unstarted-deck", result.report)


class TestDryRunReport(unittest.TestCase):
    def test_report_carries_plan_details(self) -> None:
        with TemporaryDirectory() as td:
            project = build_full_project(Path(td))
            result = orchestrate.run(project, dry_run=True)
            self.assertIn("dry-run", result.report)
            self.assertIn("00-series-a-deck", result.report)
            self.assertIn("pinned-symlink", result.report)
            self.assertIn("walk-to-highest", result.report)
            self.assertIsNone(result.apply_result)
            self.assertIsNone(result.verify_result)


if __name__ == "__main__":
    unittest.main()
