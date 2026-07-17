"""Parsing + manifest-contract tests for `anvil:project-photos` (#599).

Covers: manifest roundtrip, all column variants (md + csv), multi-item
derivation, unnumbered (x-prefix) + series-prefixed names, rotation-hint
normalization (numeric + descriptive), missing-capture surfacing,
duplicate-stable-name hard error, and the doc-is-authoritative edge cases
(empty photos dir, header-only doc, captures-on-disk-but-absent-from-doc).
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _photos_fixtures import (  # noqa: E402
    ALL_CAPTURES,
    CAPTURE_A,
    CAPTURE_B,
    CAPTURE_C,
    build_photos_dir,
    build_project,
    write_doc,
)
from _project_photos_skill_lib import manifest, orchestrate  # noqa: E402


def _by_stable(entries: list[dict]) -> dict[str, dict]:
    return {e["stable"]: e for e in entries}


class TestManifestRoundtrip(unittest.TestCase):
    def test_markdown_roundtrip_all_variants(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, _ = build_project(root, fmt="md")
            result = orchestrate.run(root / "photos", doc)
            self.assertTrue(result.success, result.warnings)
            data = result.manifest
            self.assertEqual(data["schema_version"], 1)
            self.assertEqual(data["generated_from"], "numbering.md")
            self.assertEqual(data["missing_captures"], [])

            entries = _by_stable(data["entries"])
            self.assertEqual(set(entries), {
                "042.jpg", "043-multi.jpg", "wedding-005.jpg", "x017.jpg",
            })

            plain = entries["042.jpg"]
            self.assertEqual(plain["original"], CAPTURE_A)
            self.assertEqual(plain["archive_ids"], ["42"])
            self.assertIsNone(plain["rotation_hint"])
            self.assertFalse(plain["multi_item"])

    def test_csv_matches_markdown(self) -> None:
        """Name-based column matching: CSV (reordered cols) == markdown."""
        with TemporaryDirectory() as td_md, TemporaryDirectory() as td_csv:
            md_doc, _ = build_project(Path(td_md), fmt="md")
            csv_doc, _ = build_project(Path(td_csv), fmt="csv")
            md = orchestrate.run(Path(td_md) / "photos", md_doc).manifest
            cv = orchestrate.run(Path(td_csv) / "photos", csv_doc).manifest
            # generated_from differs by filename; compare the entries.
            self.assertEqual(md["entries"], cv["entries"])
            self.assertEqual(cv["generated_from"], "numbering.csv")


class TestColumnVariants(unittest.TestCase):
    def test_multi_item_derived_from_stable_name(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, _ = build_project(root, fmt="md")
            entries = _by_stable(orchestrate.run(root / "photos", doc).manifest["entries"])
            self.assertTrue(entries["043-multi.jpg"]["multi_item"])
            self.assertEqual(entries["043-multi.jpg"]["archive_ids"], ["43", "44"])
            self.assertFalse(entries["042.jpg"]["multi_item"])

    def test_unnumbered_and_series_prefixed_names(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, _ = build_project(root, fmt="md")
            entries = _by_stable(orchestrate.run(root / "photos", doc).manifest["entries"])
            self.assertIn("x017.jpg", entries)         # unnumbered capture
            self.assertIn("wedding-005.jpg", entries)  # series prefix
            self.assertFalse(entries["x017.jpg"]["multi_item"])


class TestRotationNormalization(unittest.TestCase):
    def test_descriptive_upside_down_maps_to_180(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, _ = build_project(root, fmt="md")
            entries = _by_stable(orchestrate.run(root / "photos", doc).manifest["entries"])
            self.assertEqual(entries["043-multi.jpg"]["rotation_hint"], 180)

    def test_numeric_hint_preserved(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, _ = build_project(root, fmt="md")
            entries = _by_stable(orchestrate.run(root / "photos", doc).manifest["entries"])
            self.assertEqual(entries["x017.jpg"]["rotation_hint"], 90)

    def test_normalize_rotation_unit(self) -> None:
        self.assertIsNone(manifest.normalize_rotation("", row_number=1))
        self.assertIsNone(manifest.normalize_rotation(None, row_number=1))
        self.assertEqual(manifest.normalize_rotation("180", row_number=1), 180)
        self.assertEqual(manifest.normalize_rotation("90°", row_number=1), 90)
        self.assertEqual(manifest.normalize_rotation("270 deg", row_number=1), 270)
        self.assertEqual(manifest.normalize_rotation("Upside-Down", row_number=1), 180)
        self.assertEqual(manifest.normalize_rotation("inverted", row_number=1), 180)
        self.assertEqual(manifest.normalize_rotation("cw", row_number=1), 90)
        self.assertEqual(manifest.normalize_rotation("ccw", row_number=1), 270)

    def test_invalid_angle_is_error(self) -> None:
        with self.assertRaises(manifest.RotationHintError):
            manifest.normalize_rotation("45", row_number=3)

    def test_unknown_descriptor_is_error(self) -> None:
        with self.assertRaises(manifest.RotationHintError):
            manifest.normalize_rotation("diagonally-ish", row_number=7)


class TestMissingCaptures(unittest.TestCase):
    def test_missing_capture_surfaced_and_unsuccessful(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            from _photos_fixtures import CAPTURE_D, build_numbering_doc
            doc = build_numbering_doc(root, fmt="md")
            # Photos dir is missing CAPTURE_C and CAPTURE_D.
            build_photos_dir(root, captures=(CAPTURE_A, CAPTURE_B))
            result = orchestrate.run(root / "photos", doc)
            self.assertFalse(result.success)
            self.assertEqual(result.missing_captures, sorted([CAPTURE_C, CAPTURE_D]))
            self.assertEqual(result.manifest["missing_captures"], result.missing_captures)

    def test_extra_on_disk_captures_ignored(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc = None
            from _photos_fixtures import build_numbering_doc
            doc = build_numbering_doc(root, fmt="md")
            build_photos_dir(root, captures=ALL_CAPTURES, extra_untracked=("random_selfie.jpg",))
            result = orchestrate.run(root / "photos", doc)
            self.assertTrue(result.success, result.warnings)
            stables = {e["stable"] for e in result.manifest["entries"]}
            self.assertNotIn("random_selfie.jpg", stables)
            self.assertEqual(result.missing_captures, [])


class TestDuplicateStableHardError(unittest.TestCase):
    def test_duplicate_stable_names_raise_naming_rows(self) -> None:
        body = (
            "| original | stable | archive_ids |\n"
            "|---|---|---|\n"
            "| a.jpg | 001.jpg | 1 |\n"
            "| b.jpg | 002.jpg | 2 |\n"
            "| c.jpg | 001.jpg | 3 |\n"
        )
        with TemporaryDirectory() as td:
            doc = write_doc(Path(td), body)
            with self.assertRaises(manifest.DuplicateStableError) as ctx:
                manifest.parse_numbering_doc(doc)
            msg = str(ctx.exception)
            self.assertIn("001.jpg", msg)
            self.assertIn("1", msg)  # row 1
            self.assertIn("3", msg)  # row 3

    def test_run_reports_duplicate_as_unsuccessful(self) -> None:
        body = (
            "| original | stable | archive_ids |\n"
            "|---|---|---|\n"
            "| a.jpg | dup.jpg | 1 |\n"
            "| b.jpg | dup.jpg | 2 |\n"
        )
        with TemporaryDirectory() as td:
            root = Path(td)
            doc = write_doc(root, body)
            build_photos_dir(root, captures=("a.jpg", "b.jpg"))
            result = orchestrate.run(root / "photos", doc)
            self.assertFalse(result.success)
            self.assertTrue(any("duplicate" in w.lower() for w in result.warnings))
            # A parse error means nothing is written.
            self.assertIsNone(result.output_path)


class TestEdgeCases(unittest.TestCase):
    def test_header_only_doc_is_empty_manifest(self) -> None:
        body = "| original | stable | archive_ids | rotation_hint |\n|---|---|---|---|\n"
        with TemporaryDirectory() as td:
            root = Path(td)
            doc = write_doc(root, body)
            build_photos_dir(root, captures=())
            result = orchestrate.run(root / "photos", doc)
            self.assertTrue(result.success, result.warnings)
            self.assertEqual(result.manifest["entries"], [])
            self.assertEqual(result.manifest["missing_captures"], [])

    def test_doc_with_no_table_is_error(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc = write_doc(root, "# Just a heading\n\nNo table here.\n")
            build_photos_dir(root, captures=())
            result = orchestrate.run(root / "photos", doc)
            self.assertFalse(result.success)
            self.assertTrue(any("no table" in w.lower() for w in result.warnings))

    def test_missing_required_column_is_error(self) -> None:
        body = "| original | stable |\n|---|---|\n| a.jpg | 1.jpg |\n"
        with TemporaryDirectory() as td:
            doc = write_doc(Path(td), body)
            with self.assertRaises(manifest.NumberingDocError) as ctx:
                manifest.parse_numbering_doc(doc)
            self.assertIn("archive_ids", str(ctx.exception))

    def test_empty_photos_dir_reports_all_missing(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            doc, _ = build_project(root, fmt="md")
            # Wipe the photos dir contents but keep the dir.
            for p in (root / "photos").iterdir():
                p.unlink()
            result = orchestrate.run(root / "photos", doc)
            self.assertFalse(result.success)
            self.assertEqual(len(result.missing_captures), len(ALL_CAPTURES))


if __name__ == "__main__":
    unittest.main()
