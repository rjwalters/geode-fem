"""Regression test: the shipped spec worked-example BRIEF parses (issue #709).

``anvil/skills/spec/examples/botho-bridge-spec/BRIEF.md`` is a **trimmed
snapshot** of a real, committed, terminal-``AUDITED`` ``anvil:spec`` thread in
``botho-project/botho`` (path ``whitepaper/bridge-spec/``, commit ``d8c628dc``;
the wBTH bridge normative spec integrated as whitepaper §11 in botho#945). It
declares ``artifact_type: spec`` and an optional ``code_ref`` companion input
(the mirror image of primer's ``spec_ref``; ``SKILL.md`` §"Code-ref contract").
``spec`` is a registered skill-identity ``ArtifactType`` value
(``project_brief.py``), so the value parses under ``load_project_brief_strict``
today — this test pins the vendored example against the strict loader, the
parsed ``code_ref`` field, the #346 rubric stamps on both critic siblings
(``rubric_id: "anvil-spec-v1"``, ``rubric_total: 44``, audit-grade
``advance_threshold: 39``), and the trim guards (no PDF, no exhibit PNGs
vendored).

Crucially, it asserts the vendored ``code_ref`` glob **does NOT resolve** when
the example is used standalone (it points at botho's own bridge Rust workspace,
which is deliberately not vendored) and that the resolver degrades gracefully to
a structured ``missing: true`` entry rather than raising — the illustrative-only
contract the ``expected-thread.N/README.md`` documents.

Per the #58 packaging convention this filename
(``test_spec_example_brief_parses.py``) is unique across the
``anvil/skills/*/tests/`` tree.

Runs under either ``pytest anvil/skills/spec/tests/`` or
``python -m unittest discover anvil/skills/spec/tests/``.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from anvil.lib.project_brief import (
    ArtifactType,
    load_project_brief_strict,
    resolve_code_ref,
)


_PROJECT = "botho-bridge-spec"
_SLUG = "botho-bridge-spec"
_EXAMPLE_DIR = Path(__file__).resolve().parent.parent / "examples" / _PROJECT
_THREAD_DIR = _EXAMPLE_DIR / _SLUG
_V2 = _THREAD_DIR / f"{_SLUG}.2"
_REVIEW = _THREAD_DIR / f"{_SLUG}.2.review"
_AUDIT = _THREAD_DIR / f"{_SLUG}.2.audit"


class TestShippedSpecExampleBriefParses(unittest.TestCase):
    """The spec skill's worked example must parse under the strict loader."""

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

    def test_shipped_brief_declares_spec_artifact_type(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        doc = next(d for d in brief.documents if d.slug == _SLUG)
        self.assertEqual(doc.artifact_type, ArtifactType.SPEC)

    def test_shipped_brief_declares_code_ref(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        doc = next(d for d in brief.documents if d.slug == _SLUG)
        self.assertTrue(
            getattr(doc, "code_ref", None),
            "the vendored spec BRIEF must keep its illustrative code_ref",
        )

    def test_code_ref_is_illustrative_only_and_does_not_resolve(self) -> None:
        # The code_ref glob points at botho's own bridge Rust workspace, which
        # is NOT vendored here. Standalone it must degrade gracefully: the
        # resolver returns a structured missing:true entry (never raises), so
        # the spec<->implementation consistency audit tier never actually runs
        # against a real path.
        resolved = resolve_code_ref(
            _EXAMPLE_DIR, _SLUG, consumer_root=_EXAMPLE_DIR
        )
        self.assertIsNotNone(
            resolved,
            "code_ref is declared, so the tier activates (resolved is not None)",
        )
        self.assertTrue(
            resolved.missing,
            "the vendored bridge glob must NOT resolve standalone — it is "
            "illustrative-only (points at botho's non-vendored workspace)",
        )
        self.assertEqual(resolved.paths, [])

    def test_body_file_echoes_the_slug(self) -> None:
        body = _V2 / f"{_SLUG}.tex"
        self.assertTrue(
            body.is_file(),
            f"expected slug-echo LaTeX body at {body}",
        )

    def test_terminal_version_is_the_vendored_one(self) -> None:
        # Only the terminal AUDITED version (.2) plus its two critic siblings
        # are vendored; the intermediate .1 trajectory survives in
        # _progress.json (metadata.score_history) + changelog.md.
        prog = json.loads((_V2 / "_progress.json").read_text())
        self.assertEqual(prog["version"], 2)
        self.assertEqual(prog["phases"]["revise"]["state"], "done")
        # The 38/44 -> advance trajectory survives even though only v2 is
        # vendored (the trim keeps the terminal version only).
        history = prog["metadata"]["score_history"]
        totals = [entry["total"] for entry in history]
        self.assertEqual(totals, [38])  # v1 (v2's own total lives in the verdict)
        # The intermediate .1 version dir must NOT have been vendored.
        self.assertFalse(
            (_THREAD_DIR / f"{_SLUG}.1").exists(),
            "the intermediate .1 version must be trimmed (terminal-only)",
        )

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
        # spec<->implementation consistency findings); the review sibling owns
        # scoring.md.
        self.assertTrue((_AUDIT / "findings.md").is_file())
        self.assertTrue((_REVIEW / "scoring.md").is_file())

    def test_critic_meta_carries_rubric_stamps(self) -> None:
        # spec is a /44 rubric with an audit-grade advance threshold (>=39 per
        # SKILL.md / CLAUDE.md); the #346 per-review version stamps must land
        # on both critic siblings.
        for sibling in (_REVIEW, _AUDIT):
            meta = json.loads((sibling / "_meta.json").read_text())
            self.assertEqual(meta["scorecard_kind"], "human-verdict")
            self.assertEqual(meta["rubric_id"], "anvil-spec-v1")
            self.assertEqual(meta["rubric_total"], 44)
            self.assertEqual(meta["advance_threshold"], 39)

    def test_constant_markers_preserved(self) -> None:
        # The example ships WITH `% anvil-const:` markers on its authoritative
        # constants so the constant-consistency gate has real constants to
        # check (an unmarked example teaches false confidence — dogfood #709).
        body = (_V2 / f"{_SLUG}.tex").read_text()
        for name in (
            "wbth_decimals",
            "ring_size",
            "import_epoch_blocks",
            "import_factor_floor",
        ):
            self.assertIn(
                f"anvil-const: name={name}",
                body,
                f"expected `% anvil-const: name={name}` marker in the vendored body",
            )

    def test_no_pdf_vendored(self) -> None:
        # spec's canonical output is the LaTeX source (SKILL.md §Output
        # format); no compiled PDF is vendored.
        pdfs = list(_EXAMPLE_DIR.rglob("*.pdf"))
        self.assertEqual(
            pdfs,
            [],
            f"no compiled PDF may be vendored (LaTeX is source-of-truth): {pdfs}",
        )

    def test_no_exhibit_pngs_vendored(self) -> None:
        # The three exhibit PNGs (~456 KB) are dropped to stay in the
        # vendored-example size envelope; the body's \includegraphics refs
        # dangle standalone (expected, matches primer's dropped-figure
        # precedent).
        pngs = list(_EXAMPLE_DIR.rglob("*.png"))
        self.assertEqual(
            pngs,
            [],
            f"no exhibit PNGs may be vendored: {pngs}",
        )

    def test_example_stays_within_size_envelope(self) -> None:
        # Sibling vendored examples run ~64-156 KB (primer's is ~184 KB after
        # its trim); the trimmed spec example must stay in the same order of
        # magnitude (not megabytes) — a PDF/PNG leak would blow this.
        total = sum(
            p.stat().st_size for p in _EXAMPLE_DIR.rglob("*") if p.is_file()
        )
        self.assertLess(
            total,
            300 * 1024,
            f"vendored spec example is {total // 1024} KB — expected < 300 KB "
            f"(sibling examples run 64-156 KB; a PDF/PNG leak would blow this)",
        )


if __name__ == "__main__":
    unittest.main()
