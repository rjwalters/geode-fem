"""Regression test: the shipped primer worked-example BRIEF parses (issue #693).

``anvil/skills/primer/examples/botho/BRIEF.md`` is a **trimmed snapshot** of the
real ``anvil:primer`` dogfood run in ``botho-project/botho`` (path
``docs/primer/``, commit ``32626b48``; botho-project/botho#881 → PR #900). It
declares ``artifact_type: primer`` and an optional ``spec_ref`` companion input
(``SKILL.md`` §"Spec-ref contract"). ``primer`` is a registered skill-identity
``ArtifactType`` value (``project_brief.py``), so the value parses under
``load_project_brief_strict`` today — this test pins the vendored example against
the strict loader, the parsed ``spec_ref`` field, the slug-echo body-naming rule
(``<slug>.md``), the #346 rubric stamps on both critic siblings, and the trim
guards (no PDF, no full-resolution exhibit PNGs vendored).

Crucially, it asserts the vendored ``spec_ref`` glob **does NOT resolve** when
the example is used standalone (it points at botho's own whitepaper, which is
deliberately not vendored) and that the resolver degrades gracefully to a
structured ``missing: true`` entry rather than raising — the illustrative-only
contract the ``expected-thread.N/README.md`` documents.

Per the #58 packaging convention this filename
(``test_primer_example_brief_parses.py``) is unique across the
``anvil/skills/*/tests/`` tree.

Runs under either ``pytest anvil/skills/primer/tests/`` or
``python -m unittest discover anvil/skills/primer/tests/``.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from anvil.lib.project_brief import (
    ArtifactType,
    load_project_brief_strict,
    resolve_spec_ref,
)


_PROJECT = "botho"
_SLUG = "botho-from-the-basics"
_EXAMPLE_DIR = Path(__file__).resolve().parent.parent / "examples" / _PROJECT
_THREAD_DIR = _EXAMPLE_DIR / _SLUG
_V3 = _THREAD_DIR / f"{_SLUG}.3"
_REVIEW = _THREAD_DIR / f"{_SLUG}.3.review"
_AUDIT = _THREAD_DIR / f"{_SLUG}.3.audit"


class TestShippedPrimerExampleBriefParses(unittest.TestCase):
    """The primer skill's worked example must parse under the strict loader."""

    def test_example_dir_ships_a_brief(self) -> None:
        self.assertTrue(
            (_EXAMPLE_DIR / "BRIEF.md").is_file(),
            f"expected shipped example BRIEF at {_EXAMPLE_DIR / 'BRIEF.md'}",
        )

    def test_shipped_brief_parses_strict(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        self.assertEqual(brief.project, _PROJECT)
        slugs = [d.slug for d in brief.documents]
        self.assertIn(_SLUG, slugs)

    def test_shipped_brief_declares_primer_artifact_type(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        doc = next(d for d in brief.documents if d.slug == _SLUG)
        self.assertEqual(doc.artifact_type, ArtifactType.PRIMER)

    def test_shipped_brief_declares_spec_ref(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        doc = next(d for d in brief.documents if d.slug == _SLUG)
        self.assertTrue(
            getattr(doc, "spec_ref", None),
            "the vendored primer BRIEF must keep its illustrative spec_ref",
        )

    def test_spec_ref_is_illustrative_only_and_does_not_resolve(self) -> None:
        # The spec_ref glob points at botho's own whitepaper, which is NOT
        # vendored here. Standalone it must degrade gracefully: the resolver
        # returns a structured missing:true entry (never raises), so the
        # spec-consistency audit tier never actually runs against a real path.
        resolved = resolve_spec_ref(
            _EXAMPLE_DIR, _SLUG, consumer_root=_EXAMPLE_DIR
        )
        self.assertIsNotNone(
            resolved,
            "spec_ref is declared, so the tier activates (resolved is not None)",
        )
        self.assertTrue(
            resolved.missing,
            "the vendored whitepaper glob must NOT resolve standalone — it is "
            "illustrative-only (points at botho's non-vendored whitepaper)",
        )
        self.assertEqual(resolved.paths, [])

    def test_body_file_echoes_the_slug(self) -> None:
        body = _V3 / f"{_SLUG}.md"
        self.assertTrue(
            body.is_file(),
            f"expected slug-echo body at {body}",
        )

    def test_terminal_version_progress_records_audited_lifecycle(self) -> None:
        prog = json.loads((_V3 / "_progress.json").read_text())
        self.assertEqual(prog["version"], 3)
        self.assertEqual(prog["phases"]["revise"]["state"], "done")
        # The full 41 -> 43 -> 44 trajectory survives even though only v3 is
        # vendored (the trim keeps the terminal AUDITED version only).
        history = prog["metadata"]["score_history"]
        totals = [entry["total"] for entry in history]
        self.assertEqual(totals, [41, 43])  # v1, v2 (v3's own total lives in the verdict)

    def test_both_critic_siblings_exist(self) -> None:
        for sibling in (_REVIEW, _AUDIT):
            self.assertTrue(
                (sibling / "verdict.md").is_file(),
                f"expected verdict.md in {sibling}",
            )
            self.assertTrue(
                (sibling / "_meta.json").is_file(),
                f"expected _meta.json in {sibling}",
            )
        # The audit sibling owns findings.md (per-claim factual +
        # spec-consistency findings); the review sibling owns scoring.md.
        self.assertTrue((_AUDIT / "findings.md").is_file())
        self.assertTrue((_REVIEW / "scoring.md").is_file())

    def test_critic_meta_carries_rubric_stamps(self) -> None:
        for sibling in (_REVIEW, _AUDIT):
            meta = json.loads((sibling / "_meta.json").read_text())
            self.assertEqual(meta["scorecard_kind"], "human-verdict")
            self.assertEqual(meta["rubric_id"], "anvil-primer-v1")
            self.assertEqual(meta["rubric_total"], 44)
            self.assertEqual(meta["advance_threshold"], 35)

    def test_no_pdf_vendored(self) -> None:
        # primer's canonical output is markdown source (SKILL.md §Output
        # format); the ~1.2 MB compiled PDF is dropped to stay in the
        # vendored-example size envelope.
        pdfs = list(_EXAMPLE_DIR.rglob("*.pdf"))
        self.assertEqual(
            pdfs,
            [],
            f"no compiled PDF may be vendored (markdown is source-of-truth): {pdfs}",
        )

    def test_no_full_resolution_exhibit_pngs_vendored(self) -> None:
        # The exhibit PNGs (~1.1 MB) are dropped; the .mmd mermaid sources are
        # kept so the body's figure references resolve to a diagram source.
        pngs = list(_EXAMPLE_DIR.rglob("*.png"))
        self.assertEqual(
            pngs,
            [],
            f"no full-resolution exhibit PNGs may be vendored: {pngs}",
        )
        mmds = sorted(p.name for p in _V3.glob("exhibits/*.mmd"))
        self.assertEqual(
            len(mmds),
            5,
            f"expected the 5 .mmd mermaid sources to be kept, found: {mmds}",
        )

    def test_example_stays_within_size_envelope(self) -> None:
        # Sibling vendored examples run ~64-156 KB; the trimmed primer example
        # must stay in the same order of magnitude (not megabytes).
        total = sum(
            p.stat().st_size for p in _EXAMPLE_DIR.rglob("*") if p.is_file()
        )
        self.assertLess(
            total,
            300 * 1024,
            f"vendored primer example is {total // 1024} KB — expected < 300 KB "
            f"(sibling examples run 64-156 KB; a PDF/PNG leak would blow this)",
        )


if __name__ == "__main__":
    unittest.main()
