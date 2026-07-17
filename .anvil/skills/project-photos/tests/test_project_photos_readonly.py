"""Zero-mutation contract for `anvil:project-photos` (#599, AC 2).

The skill is strictly read-only over the source images: it lists the
photos directory to detect missing captures but never opens, renames,
rotates, or crops an image byte. SHA-256 tree check (project-scout's
``test_project_scout_readonly.py::_tree_hash`` pattern) across every code
path: default write-beside-doc, ``--json`` override, ``--dry-run``, and a
run with missing captures. ``--dry-run`` additionally writes nothing
anywhere.
"""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _photos_fixtures import (  # noqa: E402
    CAPTURE_A,
    CAPTURE_B,
    build_photos_dir,
    build_numbering_doc,
    build_project,
)
from _project_photos_skill_lib import orchestrate  # noqa: E402


def _tree_hash(root: Path) -> dict:
    out: dict = {}
    for path in sorted(root.rglob("*")):
        if path.is_file():
            rel = str(path.relative_to(root))
            out[rel] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


class TestZeroMutation(unittest.TestCase):
    def test_photos_dir_untouched_default_run(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, photos = build_project(root, fmt="md")
            before = _tree_hash(photos)
            result = orchestrate.run(photos, doc)
            self.assertTrue(result.success, result.warnings)
            self.assertEqual(before, _tree_hash(photos), "photos dir mutated")

    def test_photos_dir_untouched_every_mode(self) -> None:
        with TemporaryDirectory() as td, TemporaryDirectory() as out:
            root = Path(td)
            doc, photos = build_project(root, fmt="md")
            before = _tree_hash(photos)
            for kwargs in (
                {},
                {"dry_run": True},
                {"json_path": Path(out) / "m.json"},
            ):
                with self.subTest(kwargs=kwargs):
                    orchestrate.run(photos, doc, **kwargs)
                    self.assertEqual(before, _tree_hash(photos))

    def test_photos_dir_untouched_when_captures_missing(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc = build_numbering_doc(root, fmt="md")
            photos = build_photos_dir(root, captures=(CAPTURE_A, CAPTURE_B))
            before = _tree_hash(photos)
            result = orchestrate.run(photos, doc)
            self.assertFalse(result.success)
            self.assertEqual(before, _tree_hash(photos))


class TestDryRunWritesNothing(unittest.TestCase):
    def test_dry_run_writes_no_file(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, photos = build_project(root, fmt="md")
            # Hash the ENTIRE project root, not just photos.
            before = _tree_hash(root)
            result = orchestrate.run(photos, doc, dry_run=True)
            self.assertIsNone(result.output_path)
            self.assertFalse((doc.parent / "manifest.json").exists())
            self.assertEqual(before, _tree_hash(root), "dry-run wrote a file")
            # The manifest is still computed and available in-memory.
            self.assertTrue(result.manifest_json)
            self.assertEqual(result.manifest["schema_version"], 1)

    def test_second_run_after_write_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, photos = build_project(root, fmt="md")
            orchestrate.run(photos, doc)
            after_first = _tree_hash(root)
            orchestrate.run(photos, doc)
            self.assertEqual(after_first, _tree_hash(root))


if __name__ == "__main__":
    unittest.main()
