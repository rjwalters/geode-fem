"""Regression test: the shipped ax101-family example BRIEF parses (issue #529).

``anvil/skills/datasheet/examples/ax101-family/BRIEF.md`` declares two
``artifact_type: datasheet`` documents. ``datasheet`` is a skill-identity
``ArtifactType`` value (issue #486); registering it is what lets the strict
project-BRIEF loader accept the value. This test pins that the vendored worked
example parses under ``load_project_brief_strict`` and declares the expected
artifact type — the repo must never ship a BRIEF its own parser cannot read.

Beyond the proposal exemplar (which describes but does not vendor a critic
sibling), this worked example ships a REAL stamped ``.review/`` sidecar; the
final test asserts the per-review version stamping contract
(``rubric_id`` / ``rubric_total`` / ``advance_threshold``) against that vendored
file.

Per the #58 packaging convention this filename
(``test_datasheet_example_brief_parses.py``) is unique across the
``anvil/skills/*/tests/`` tree.

Runs under either ``pytest anvil/skills/datasheet/tests/`` or
``python -m unittest discover anvil/skills/datasheet/tests/``.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from anvil.lib.project_brief import (
    ArtifactType,
    load_project_brief_strict,
)


_EXAMPLE_DIR = (
    Path(__file__).resolve().parent.parent / "examples" / "ax101-family"
)
_REVIEW_META = (
    _EXAMPLE_DIR
    / "ax101-objdet"
    / "ax101-objdet.1.review"
    / "_meta.json"
)


class TestShippedExampleBriefParses(unittest.TestCase):
    """The datasheet skill's worked example must parse under the strict loader."""

    def test_example_dir_ships_a_brief(self) -> None:
        self.assertTrue(
            (_EXAMPLE_DIR / "BRIEF.md").is_file(),
            f"expected shipped example BRIEF at {_EXAMPLE_DIR / 'BRIEF.md'}",
        )

    def test_shipped_brief_parses_strict(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        self.assertEqual(brief.project, "ax101-family")
        slugs = [d.slug for d in brief.documents]
        self.assertIn("ax101-objdet", slugs)

    def test_shipped_brief_declares_datasheet_artifact_type(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        doc = next(d for d in brief.documents if d.slug == "ax101-objdet")
        self.assertEqual(doc.artifact_type, ArtifactType.DATASHEET)

    def test_vendored_review_sidecar_carries_stamping_contract(self) -> None:
        # The deliberate superset over the proposal exemplar: a real, vendored,
        # stamped critic sidecar. Assert the v0.4.0 per-review version stamping.
        self.assertTrue(
            _REVIEW_META.is_file(),
            f"expected vendored review sidecar _meta.json at {_REVIEW_META}",
        )
        meta = json.loads(_REVIEW_META.read_text())
        self.assertEqual(meta.get("rubric_id"), "anvil-datasheet-v1")
        self.assertEqual(meta.get("rubric_total"), 44)
        self.assertEqual(meta.get("advance_threshold"), 39)


if __name__ == "__main__":
    unittest.main()
