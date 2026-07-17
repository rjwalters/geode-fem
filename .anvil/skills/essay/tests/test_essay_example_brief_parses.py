"""Regression test: the shipped essay worked-example BRIEF parses (issue #531).

``anvil/skills/essay/examples/the-version-dir-is-the-unit/BRIEF.md`` declares
``artifact_type: essay`` and a top-level ``voice:`` block (the issue #461
voice/persona grounding-docs contract). ``essay`` is already a registered
skill-identity ``ArtifactType`` value (``project_brief.py``), so the value
parses under ``load_project_brief_strict`` today — this test pins the vendored
example against the strict loader, the parsed ``voice:`` block, the slug-echo
body-naming rule (``<slug>.md``, NOT ``post.md``), and the markdown-only
no-leak guard.

Per the #58 packaging convention this filename
(``test_essay_example_brief_parses.py``) is unique across the
``anvil/skills/*/tests/`` tree.

Runs under either ``pytest anvil/skills/essay/tests/`` or
``python -m unittest discover anvil/skills/essay/tests/``.
"""

from __future__ import annotations

import unittest
from pathlib import Path

from anvil.lib.project_brief import (
    ArtifactType,
    load_project_brief_strict,
    resolve_voice_docs,
)


_PROJECT = "the-version-dir-is-the-unit"
_SLUG = "the-version-dir-is-the-unit"
_EXAMPLE_DIR = (
    Path(__file__).resolve().parent.parent / "examples" / _PROJECT
)


class TestShippedEssayExampleBriefParses(unittest.TestCase):
    """The essay skill's worked example must parse under the strict loader."""

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

    def test_shipped_brief_declares_essay_artifact_type(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        doc = next(d for d in brief.documents if d.slug == _SLUG)
        self.assertEqual(doc.artifact_type, ArtifactType.ESSAY)

    def test_shipped_brief_carries_active_voice_block(self) -> None:
        brief = load_project_brief_strict(_EXAMPLE_DIR)
        self.assertIsNotNone(
            brief.voice,
            "the essay worked example must declare a voice: block (issue #461)",
        )
        self.assertFalse(
            brief.voice.is_empty,
            "the voice: block must declare at least one grounding doc",
        )

    def test_voice_docs_resolve_to_files_on_disk(self) -> None:
        # Voice docs live at the project root, so project-root-first
        # resolution finds them; pass the project dir as consumer_root so
        # the test never depends on a ``.anvil/`` marker walk.
        resolved = resolve_voice_docs(
            _EXAMPLE_DIR, consumer_root=_EXAMPLE_DIR
        )
        self.assertTrue(
            resolved,
            "resolve_voice_docs must return entries for the active block",
        )
        for entry in resolved:
            self.assertFalse(
                entry.missing,
                f"declared voice doc {entry.declared!r} resolved to nothing",
            )
            for path in entry.paths:
                self.assertTrue(
                    Path(path).is_file(),
                    f"resolved voice path does not exist: {path}",
                )

    def test_body_file_echoes_the_slug(self) -> None:
        body = (
            _EXAMPLE_DIR / _SLUG / f"{_SLUG}.1" / f"{_SLUG}.md"
        )
        self.assertTrue(
            body.is_file(),
            f"expected slug-echo body at {body}",
        )

    def test_no_post_md_leak_anywhere(self) -> None:
        leaks = list(_EXAMPLE_DIR.rglob("post.md"))
        self.assertEqual(
            leaks,
            [],
            f"essay is markdown-only with slug-echo bodies; no post.md "
            f"may leak into the vendored tree, found: {leaks}",
        )

    def test_markdown_only_no_render_artifacts(self) -> None:
        # essay ships no .tex/.cls/.pdf/figures (SKILL.md markdown-only).
        for pattern in ("*.tex", "*.cls", "*.pdf"):
            hits = list(_EXAMPLE_DIR.rglob(pattern))
            self.assertEqual(
                hits,
                [],
                f"essay is markdown-only; unexpected {pattern}: {hits}",
            )
        figures = list(_EXAMPLE_DIR.rglob("figures"))
        self.assertEqual(
            figures,
            [],
            f"essay has no figures phase; unexpected figures dir: {figures}",
        )

    def test_review_meta_carries_rubric_stamps(self) -> None:
        import json

        meta_path = (
            _EXAMPLE_DIR
            / _SLUG
            / f"{_SLUG}.1.review"
            / "_meta.json"
        )
        self.assertTrue(meta_path.is_file(), f"missing {meta_path}")
        meta = json.loads(meta_path.read_text())
        self.assertEqual(meta["scorecard_kind"], "human-verdict")
        self.assertEqual(meta["rubric_id"], "anvil-essay-v1")
        self.assertEqual(meta["rubric_total"], 44)
        self.assertEqual(meta["advance_threshold"], 35)


if __name__ == "__main__":
    unittest.main()
