"""Determinism / idempotency contract for `anvil:project-photos` (#599).

Two runs over identical inputs produce byte-identical ``manifest.json``:
entries sorted by stable name, keys sorted, no timestamps in the body.
Also asserts the on-disk file matches the in-memory ``manifest_json`` and
that a re-run over an already-written manifest leaves it byte-identical.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _photos_fixtures import build_project  # noqa: E402
from _project_photos_skill_lib import orchestrate  # noqa: E402


class TestDeterminism(unittest.TestCase):
    def test_two_runs_byte_identical(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, photos = build_project(root, fmt="md")
            a = orchestrate.run(photos, doc)
            first_bytes = a.output_path.read_bytes()
            b = orchestrate.run(photos, doc)
            second_bytes = b.output_path.read_bytes()
            self.assertEqual(first_bytes, second_bytes)
            self.assertEqual(a.manifest_json, b.manifest_json)

    def test_entries_sorted_by_stable(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, photos = build_project(root, fmt="md")
            entries = orchestrate.run(photos, doc).manifest["entries"]
            stables = [e["stable"] for e in entries]
            self.assertEqual(stables, sorted(stables))

    def test_no_timestamp_in_body(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, photos = build_project(root, fmt="md")
            text = orchestrate.run(photos, doc).manifest_json
            # Deterministic body carries only the doc basename, no path/time.
            self.assertIn('"generated_from": "numbering.md"', text)
            for token in ("T00:", "generated_at", "timestamp", str(root)):
                self.assertNotIn(token, text)

    def test_written_file_matches_in_memory(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, photos = build_project(root, fmt="md")
            result = orchestrate.run(photos, doc)
            self.assertEqual(
                result.output_path.read_text(encoding="utf-8"),
                result.manifest_json,
            )

    def test_json_path_override(self) -> None:
        with TemporaryDirectory() as td, TemporaryDirectory() as out:
            root = Path(td)
            doc, photos = build_project(root, fmt="md")
            target = Path(out) / "custom.json"
            result = orchestrate.run(photos, doc, json_path=target)
            self.assertEqual(result.output_path, target)
            self.assertTrue(target.is_file())
            self.assertFalse((doc.parent / "manifest.json").exists())

    def test_csv_and_markdown_entries_identical(self) -> None:
        with TemporaryDirectory() as m, TemporaryDirectory() as c:
            md_doc, md_photos = build_project(Path(m), fmt="md")
            csv_doc, csv_photos = build_project(Path(c), fmt="csv")
            md = orchestrate.run(md_photos, md_doc).manifest["entries"]
            cv = orchestrate.run(csv_photos, csv_doc).manifest["entries"]
            self.assertEqual(md, cv)


if __name__ == "__main__":
    unittest.main()
