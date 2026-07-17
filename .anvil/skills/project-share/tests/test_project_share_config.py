"""Tests for `anvil:project-share` config parsing (issue #396).

The ``export:`` BRIEF frontmatter block is skill-local: parsed by
``lib/config.py::ExportConfig``, never by the shared ``ProjectBrief``
model. Zero-config (no ``export:`` block) must yield full defaults.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _project_share_skill_lib import config as config_mod  # noqa: E402
from _share_fixtures import build_full_project  # noqa: E402

ExportConfig = config_mod.ExportConfig
load_export_config = config_mod.load_export_config
DEFAULT_STRIP = config_mod.DEFAULT_STRIP
DEFAULT_OUT = config_mod.DEFAULT_OUT


class TestZeroConfigDefaults(unittest.TestCase):
    def test_no_export_block_yields_defaults(self) -> None:
        with TemporaryDirectory() as td:
            project = build_full_project(Path(td))
            cfg = load_export_config(project)
            self.assertIsNone(cfg.order)
            self.assertTrue(cfg.include_research)
            self.assertTrue(cfg.include_refs)
            self.assertTrue(cfg.include_assets)
            self.assertEqual(cfg.strip, list(DEFAULT_STRIP))
            self.assertEqual(cfg.out, DEFAULT_OUT)

    def test_default_strip_covers_bookkeeping(self) -> None:
        self.assertIn("_progress.json", DEFAULT_STRIP)
        self.assertIn("changelog.md", DEFAULT_STRIP)
        self.assertIn("_*.json", DEFAULT_STRIP)
        self.assertIn(".tmp*", DEFAULT_STRIP)


class TestExportBlockParsing(unittest.TestCase):
    def test_full_block_parses(self) -> None:
        block = (
            "export:\n"
            "  order: [investment-memo, series-a-deck]\n"
            "  include_research: false\n"
            "  include_refs: false\n"
            "  include_assets: false\n"
            "  strip: [\"*.secret\"]\n"
            "  out: DATAROOM\n"
        )
        with TemporaryDirectory() as td:
            project = build_full_project(Path(td), export_block=block)
            cfg = load_export_config(project)
            self.assertEqual(
                cfg.order, ["investment-memo", "series-a-deck"]
            )
            self.assertFalse(cfg.include_research)
            self.assertFalse(cfg.include_refs)
            self.assertFalse(cfg.include_assets)
            self.assertEqual(cfg.strip, ["*.secret"])
            self.assertEqual(cfg.out, "DATAROOM")

    def test_partial_block_keeps_other_defaults(self) -> None:
        block = "export:\n  include_research: false\n"
        with TemporaryDirectory() as td:
            project = build_full_project(Path(td), export_block=block)
            cfg = load_export_config(project)
            self.assertFalse(cfg.include_research)
            self.assertTrue(cfg.include_refs)
            self.assertEqual(cfg.out, DEFAULT_OUT)
            self.assertEqual(cfg.strip, list(DEFAULT_STRIP))


class TestMalformedExportBlock(unittest.TestCase):
    def _project_with(self, block: str, td: str) -> Path:
        return build_full_project(Path(td), export_block=block)

    def test_non_mapping_export_block_raises(self) -> None:
        with TemporaryDirectory() as td:
            project = self._project_with("export: just-a-string\n", td)
            with self.assertRaisesRegex(ValueError, "must be a mapping"):
                load_export_config(project)

    def test_unknown_key_raises(self) -> None:
        with TemporaryDirectory() as td:
            project = self._project_with(
                "export:\n  include_briefs: true\n", td
            )
            with self.assertRaises(ValueError):
                load_export_config(project)

    def test_out_with_path_separator_raises(self) -> None:
        with TemporaryDirectory() as td:
            project = self._project_with(
                "export:\n  out: nested/SHARE\n", td
            )
            with self.assertRaisesRegex(ValueError, "path separators"):
                load_export_config(project)

    def test_out_dotdot_raises(self) -> None:
        with TemporaryDirectory() as td:
            project = self._project_with("export:\n  out: '..'\n", td)
            with self.assertRaises(ValueError):
                load_export_config(project)

    def test_order_non_string_entry_raises(self) -> None:
        with TemporaryDirectory() as td:
            project = self._project_with(
                "export:\n  order:\n    - 42\n", td
            )
            with self.assertRaises(ValueError):
                load_export_config(project)

    def test_order_duplicate_entry_raises(self) -> None:
        with TemporaryDirectory() as td:
            project = self._project_with(
                "export:\n  order: [a-doc, a-doc]\n", td
            )
            with self.assertRaisesRegex(ValueError, "more than once"):
                load_export_config(project)

    def test_missing_brief_raises_file_not_found(self) -> None:
        with TemporaryDirectory() as td:
            empty = Path(td) / "no-brief"
            empty.mkdir()
            with self.assertRaises(FileNotFoundError):
                load_export_config(empty)


if __name__ == "__main__":
    unittest.main()
