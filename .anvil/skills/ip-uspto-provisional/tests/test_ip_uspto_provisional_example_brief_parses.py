"""Regression test: the vendored acme-widget-prov example BRIEF parses (#530).

``anvil/skills/ip-uspto-provisional/examples/acme-widget-prov/BRIEF.md``
declares ``artifact_type: ip-uspto-provisional``. This test pins the worked
example by loading the exact shipped project BRIEF through
``load_project_brief_strict`` and asserting the structural contract documented
in ``examples/expected-thread.1/README.md`` (the version dir ships a
``\\documentclass{anvil-uspto}`` ``spec.tex`` with a copied ``anvil-uspto.cls``,
an OPTIONAL ``claims.tex`` claim-seed, and a ``machine-summary`` ``s112`` critic
sidecar stamped against the ``/45`` ``anvil-ip-provisional-v1`` rubric).

Per the #58 packaging convention this filename
(``test_ip_uspto_provisional_example_brief_parses.py``) is unique across the
``anvil/skills/*/tests/`` tree. Like the sibling
``test_ip_uspto_provisional_skeleton.py``, this ``tests/`` dir carries NO
``__init__.py`` (``ip-uspto-provisional`` is not a valid Python package name) —
the unique filename alone prevents the pytest collection collision.

Runs under either ``pytest anvil/skills/ip-uspto-provisional/tests/`` or
``python -m unittest discover anvil/skills/ip-uspto-provisional/tests/``.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from anvil.lib import evidence_check
from anvil.lib.project_brief import (
    ArtifactType,
    load_project_brief_strict,
)

_EXAMPLES_DIR = Path(__file__).resolve().parent.parent / "examples"
_PROJECT_DIR = _EXAMPLES_DIR / "acme-widget-prov"
_VERSION_DIR = _PROJECT_DIR / "acme-widget-prov.1"
_S112_DIR = _PROJECT_DIR / "acme-widget-prov.1.s112"

RUBRIC_ID = "anvil-ip-provisional-v1"


class TestShippedExampleBriefParses(unittest.TestCase):
    """The provisional worked example must parse under the strict loader."""

    def test_example_dir_ships_a_project_brief(self) -> None:
        self.assertTrue(
            (_PROJECT_DIR / "BRIEF.md").is_file(),
            f"expected shipped project BRIEF at {_PROJECT_DIR / 'BRIEF.md'}",
        )

    def test_project_brief_parses_strict(self) -> None:
        brief = load_project_brief_strict(_PROJECT_DIR)
        self.assertEqual(brief.project, "acme-widget-prov")
        slugs = [d.slug for d in brief.documents]
        self.assertIn("acme-widget-prov", slugs)

    def test_brief_declares_ip_uspto_provisional_artifact_type(self) -> None:
        brief = load_project_brief_strict(_PROJECT_DIR)
        doc = next(
            d for d in brief.documents if d.slug == "acme-widget-prov"
        )
        self.assertEqual(doc.artifact_type, ArtifactType.IP_USPTO_PROVISIONAL)

    def test_thread_level_brief_present(self) -> None:
        # The ip-uspto-intake-shaped inventor brief (the disclosure denominator).
        self.assertTrue(
            (_PROJECT_DIR / "acme-widget-prov" / "BRIEF.md").is_file()
        )


class TestVersionDirStructure(unittest.TestCase):
    """The acme-widget-prov.1/ version dir compiles standalone per the README."""

    def test_spec_declares_anvil_uspto_class(self) -> None:
        tex = (_VERSION_DIR / "spec.tex").read_text(encoding="utf-8")
        self.assertIn("\\documentclass{anvil-uspto}", tex)

    def test_five_required_sections_present(self) -> None:
        tex = (_VERSION_DIR / "spec.tex").read_text(encoding="utf-8")
        for macro in (
            "\\fieldoftheinvention",
            "\\background",
            "\\summary",
            "\\briefdescriptionofdrawings",
            "\\detaileddescription",
        ):
            with self.subTest(macro=macro):
                self.assertIn(macro, tex)

    def test_no_abstract(self) -> None:
        # Provisionals omit the abstract entirely.
        tex = (_VERSION_DIR / "spec.tex").read_text(encoding="utf-8")
        self.assertNotIn("\\abstract", tex)

    def test_class_copied_for_standalone_compile(self) -> None:
        self.assertTrue(
            (_VERSION_DIR / "anvil-uspto.cls").is_file(),
            "anvil-uspto.cls must be copied alongside spec.tex",
        )

    def test_optional_claim_seed_present(self) -> None:
        # The seed is OPTIONAL; this example includes it to exercise dim 9.
        self.assertTrue((_VERSION_DIR / "claims.tex").is_file())

    def test_progress_records_drafted_iteration_one(self) -> None:
        prog = json.loads(
            (_VERSION_DIR / "_progress.json").read_text(encoding="utf-8")
        )
        self.assertEqual(prog["phases"]["draft"]["state"], "done")
        self.assertEqual(prog["metadata"]["iteration"], 1)


class TestS112MachineSummarySidecar(unittest.TestCase):
    """The natural critic is s112 (machine-summary), NOT a human-verdict .review/."""

    def test_sidecar_is_named_s112_not_review(self) -> None:
        self.assertTrue(
            _S112_DIR.is_dir(),
            "expected the s112 critic sidecar at acme-widget-prov.1.s112/",
        )
        self.assertFalse(
            (_PROJECT_DIR / "acme-widget-prov.1.review").exists(),
            "the natural critic is s112 (machine-summary), not .review/",
        )

    def test_machine_summary_files_present(self) -> None:
        for name in ("_summary.md", "findings.md", "_meta.json", "_progress.json"):
            with self.subTest(name=name):
                self.assertTrue((_S112_DIR / name).is_file())

    def test_meta_stamps_machine_summary_and_rubric(self) -> None:
        meta = json.loads(
            (_S112_DIR / "_meta.json").read_text(encoding="utf-8")
        )
        self.assertEqual(meta["scorecard_kind"], "machine-summary")
        self.assertEqual(meta["rubric_id"], RUBRIC_ID)
        self.assertEqual(meta["rubric_total"], 45)
        self.assertEqual(meta["advance_threshold"], 39)

    def test_summary_carries_rubric_block_and_owned_dims(self) -> None:
        summary = (_S112_DIR / "_summary.md").read_text(encoding="utf-8")
        # The /45 ip rubric, not /44.
        self.assertIn('"total": 45', summary)
        self.assertIn('"advance_threshold": 39', summary)
        self.assertIn(RUBRIC_ID, summary)
        # s112 owns dims 1, 2, 3, 9; the rest are null (n/a — see owning critic).
        self.assertIn("n/a — see", summary)

    def test_evidence_check_is_non_vacuous(self) -> None:
        """The vendored machine-summary specimen exercises the quote check.

        Regression guard for #536: the s112 ``_summary.md`` carries the
        scored dims (1, 2, 3, 9) in a markdown TABLE — the shape the ip
        commands instruct and ``critics.py`` parses — NOT a fenced-JSON
        ``dimensions`` block. Before #536, ``evidence_check`` read only the
        JSON block and reported ``dimensions_checked == 0`` (a vacuous
        pass: the quotes were never machine-checked). After the table
        fallback the check is live: it examines all four owned dims and
        finds zero fabricated quotes (the justifications quote ``spec.tex``
        verbatim — verified by the #534 judge).
        """
        result = evidence_check.check_version_dir(
            _VERSION_DIR, scoring=_S112_DIR / "_summary.md"
        )
        self.assertEqual(
            result.dimensions_checked,
            4,
            "expected the 4 owned dims (1, 2, 3, 9) to be checked, not a "
            f"vacuous 0; got {result.dimensions_checked} "
            f"(findings: {result.findings})",
        )
        self.assertTrue(
            result.passed(),
            f"genuine quotes must yield zero findings; got {result.findings}",
        )


if __name__ == "__main__":
    unittest.main()
