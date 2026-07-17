"""Regression test: the shipped gossamer-lan example BRIEF parses (issue #386).

``anvil/skills/proposal/examples/gossamer-lan/BRIEF.md`` declares
``artifact_type: proposal``. Before #386 grew the shared
:class:`~anvil.lib.project_brief.ArtifactType` enum with skill-identity
values, the strict parser REJECTED this value — the repo shipped a BRIEF
its own parser could not read. This test pins the fix by loading the
exact shipped example through ``load_project_brief_strict``.

Per the #58 packaging convention this filename
(``test_proposal_example_brief_parses.py``) is unique across the
``anvil/skills/*/tests/`` tree.

Runs under either ``pytest anvil/skills/proposal/tests/`` or
``python -m unittest discover anvil/skills/proposal/tests/``.
"""

from __future__ import annotations

import unittest
from pathlib import Path

from anvil.lib.project_brief import (
    ArtifactType,
    load_project_brief_strict,
)


_EXAMPLE_DIR = (
    Path(__file__).resolve().parent.parent / "examples" / "gossamer-lan"
)


class TestShippedExampleBriefParses(unittest.TestCase):
    """The proposal skill's worked example must parse under the strict loader."""

    def test_example_dir_ships_a_brief(self) -> None:
        self.assertTrue(
            (_EXAMPLE_DIR / "BRIEF.md").is_file(),
            f"expected shipped example BRIEF at {_EXAMPLE_DIR / 'BRIEF.md'}",
        )

    def test_shipped_brief_parses_strict(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        self.assertEqual(brief.project, "gossamer-lan")
        slugs = [d.slug for d in brief.documents]
        self.assertIn("gossamer-lan", slugs)

    def test_shipped_brief_declares_proposal_artifact_type(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        doc = next(d for d in brief.documents if d.slug == "gossamer-lan")
        self.assertEqual(doc.artifact_type, ArtifactType.PROPOSAL)


if __name__ == "__main__":
    unittest.main()
