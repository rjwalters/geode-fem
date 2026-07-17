"""Tests for stamp-only mode primitives (issue #358).

Coverage for the file-rewrite primitives in :mod:`lib.stamp`:

- ``apply_stamp_meta`` writes the three required fields and is idempotent.
- ``apply_stamp_progress_rows`` adds ``rubric_id`` to legacy rows.
- ``apply_summary_rubric_block`` updates YAML frontmatter and JSON
  variants of `_summary.md`.
- Per-skill stamping value contracts: memo /44 writes (44, 35);
  memo /40 writes (40, 32); proposal /44 writes (44, 35).
"""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _skill_lib import plan, stamp  # noqa: E402


KNOWN_RUBRICS = plan.KNOWN_RUBRICS
ProgressRowStamp = plan.ProgressRowStamp
StampOp = plan.StampOp
SummaryRubricBlock = plan.SummaryRubricBlock
apply_stamp_meta = stamp.apply_stamp_meta
apply_stamp_progress_rows = stamp.apply_stamp_progress_rows
apply_summary_rubric_block = stamp.apply_summary_rubric_block


class TestApplyStampMeta(unittest.TestCase):
    def test_stamp_writes_three_fields(self) -> None:
        with TemporaryDirectory() as td:
            meta_path = Path(td) / "_meta.json"
            meta_path.write_text(
                json.dumps({"critic": "review", "rubric_total": 40}) + "\n"
            )
            op = StampOp(
                meta_path=meta_path,
                rubric_id="anvil-memo-v1-legacy-40",
                rubric_total=40,
                advance_threshold=32,
            )
            changed, err = apply_stamp_meta(op)
            self.assertIsNone(err)
            self.assertTrue(changed)
            data = json.loads(meta_path.read_text())
            self.assertEqual(data["rubric_id"], "anvil-memo-v1-legacy-40")
            self.assertEqual(data["rubric_total"], 40)
            self.assertEqual(data["advance_threshold"], 32)
            self.assertEqual(data["critic"], "review")

    def test_stamp_is_idempotent(self) -> None:
        with TemporaryDirectory() as td:
            meta_path = Path(td) / "_meta.json"
            meta_path.write_text(
                json.dumps(
                    {
                        "critic": "review",
                        "rubric_id": "anvil-memo-v2",
                        "rubric_total": 44,
                        "advance_threshold": 35,
                    }
                ) + "\n"
            )
            op = StampOp(
                meta_path=meta_path,
                rubric_id="anvil-memo-v2",
                rubric_total=44,
                advance_threshold=35,
            )
            changed, err = apply_stamp_meta(op)
            self.assertIsNone(err)
            self.assertFalse(
                changed, "no-op stamp should report changed=False"
            )

    def test_stamp_meta_missing_file(self) -> None:
        with TemporaryDirectory() as td:
            meta_path = Path(td) / "_meta.json"
            op = StampOp(meta_path=meta_path, rubric_id="x")
            changed, err = apply_stamp_meta(op)
            self.assertIsNotNone(err)
            self.assertFalse(changed)


class TestApplyStampProgressRows(unittest.TestCase):
    def test_adds_rubric_id_to_legacy_rows(self) -> None:
        with TemporaryDirectory() as td:
            progress_path = Path(td) / "_progress.json"
            progress_path.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "thread": "memo",
                        "metadata": {
                            "score_history": [
                                {"iteration": 1, "total": 30, "threshold": 32},
                                {"iteration": 2, "total": 33, "threshold": 32},
                            ]
                        },
                    }
                ) + "\n"
            )
            op = ProgressRowStamp(
                progress_path=progress_path,
                rubric_id="anvil-memo-v1-legacy-40",
            )
            rows, err = apply_stamp_progress_rows(op)
            self.assertIsNone(err)
            self.assertEqual(rows, 2)
            data = json.loads(progress_path.read_text())
            for row in data["metadata"]["score_history"]:
                self.assertEqual(
                    row["rubric_id"], "anvil-memo-v1-legacy-40"
                )

    def test_skips_already_stamped_rows(self) -> None:
        with TemporaryDirectory() as td:
            progress_path = Path(td) / "_progress.json"
            progress_path.write_text(
                json.dumps(
                    {
                        "metadata": {
                            "score_history": [
                                {
                                    "iteration": 1,
                                    "rubric_id": "anvil-memo-v1-legacy-40",
                                },
                                {"iteration": 2},
                            ]
                        }
                    }
                ) + "\n"
            )
            op = ProgressRowStamp(
                progress_path=progress_path,
                rubric_id="anvil-memo-v1-legacy-40",
            )
            rows, _ = apply_stamp_progress_rows(op)
            self.assertEqual(rows, 1)
            data = json.loads(progress_path.read_text())
            self.assertEqual(
                data["metadata"]["score_history"][0]["rubric_id"],
                "anvil-memo-v1-legacy-40",
            )
            self.assertEqual(
                data["metadata"]["score_history"][1]["rubric_id"],
                "anvil-memo-v1-legacy-40",
            )

    def test_no_metadata_block_is_noop(self) -> None:
        with TemporaryDirectory() as td:
            progress_path = Path(td) / "_progress.json"
            progress_path.write_text(json.dumps({"version": 1}) + "\n")
            op = ProgressRowStamp(
                progress_path=progress_path, rubric_id="anvil-memo-v2"
            )
            rows, err = apply_stamp_progress_rows(op)
            self.assertIsNone(err)
            self.assertEqual(rows, 0)


class TestApplySummaryRubricBlock(unittest.TestCase):
    def test_yaml_summary_gains_rubric_block(self) -> None:
        with TemporaryDirectory() as td:
            summary_path = Path(td) / "_summary.md"
            summary_path.write_text(
                "---\n"
                "for_version: 1\n"
                "critical_flag: false\n"
                "---\n"
                "\n# Summary\n"
            )
            block = SummaryRubricBlock(
                summary_path=summary_path,
                rubric_id="anvil-memo-v2",
                rubric_total=44,
                advance_threshold=35,
                dimensions=9,
                prior_rubric_inferred=True,
            )
            changed, err = apply_summary_rubric_block(block)
            self.assertIsNone(err)
            self.assertTrue(changed)
            text = summary_path.read_text()
            self.assertIn("rubric:", text)
            self.assertIn("id: anvil-memo-v2", text)
            self.assertIn("total: 44", text)
            self.assertIn("advance_threshold: 35", text)
            self.assertIn("/40-legacy", text)

    def test_json_summary_gains_rubric_block(self) -> None:
        with TemporaryDirectory() as td:
            summary_path = Path(td) / "_summary.md"
            summary_path.write_text(
                json.dumps({"critic": "review", "for_version": 1}) + "\n"
            )
            block = SummaryRubricBlock(
                summary_path=summary_path,
                rubric_id="anvil-memo-v2",
                rubric_total=44,
                advance_threshold=35,
                dimensions=9,
                prior_rubric_inferred=False,
            )
            changed, err = apply_summary_rubric_block(block)
            self.assertIsNone(err)
            self.assertTrue(changed)
            data = json.loads(summary_path.read_text())
            self.assertIn("rubric", data)
            self.assertEqual(data["rubric"]["id"], "anvil-memo-v2")
            self.assertEqual(data["rubric"]["total"], 44)

    def test_yaml_summary_with_existing_rubric_block_updates_in_place(
        self,
    ) -> None:
        with TemporaryDirectory() as td:
            summary_path = Path(td) / "_summary.md"
            summary_path.write_text(
                "---\n"
                "for_version: 1\n"
                "rubric:\n"
                "  id: anvil-memo-v1\n"
                "  total: 40\n"
                "---\n"
            )
            block = SummaryRubricBlock(
                summary_path=summary_path,
                rubric_id="anvil-memo-v2",
                rubric_total=44,
                advance_threshold=35,
                dimensions=9,
            )
            changed, _ = apply_summary_rubric_block(block)
            self.assertTrue(changed)
            text = summary_path.read_text()
            self.assertIn("id: anvil-memo-v2", text)
            self.assertNotIn("id: anvil-memo-v1", text)


class TestPerSkillStampingValues(unittest.TestCase):
    """Pin the per-skill stamping contracts from SKILL.md."""

    def test_memo_v2_writes_44_and_35(self) -> None:
        ri = KNOWN_RUBRICS[("memo", 44)]
        self.assertEqual(ri.id, "anvil-memo-v2")
        self.assertEqual(ri.total, 44)
        self.assertEqual(ri.advance_threshold, 35)

    def test_memo_v1_legacy_writes_40_and_32(self) -> None:
        ri = KNOWN_RUBRICS[("memo", 40)]
        self.assertEqual(ri.id, "anvil-memo-v1-legacy-40")
        self.assertEqual(ri.total, 40)
        self.assertEqual(ri.advance_threshold, 32)

    def test_proposal_v2_writes_44_and_35(self) -> None:
        ri = KNOWN_RUBRICS[("proposal", 44)]
        self.assertEqual(ri.id, "anvil-proposal-v2")
        self.assertEqual(ri.total, 44)
        self.assertEqual(ri.advance_threshold, 35)


if __name__ == "__main__":
    unittest.main()
