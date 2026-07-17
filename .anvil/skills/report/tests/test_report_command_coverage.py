"""Doc-coverage tests for the ``anvil:report`` subject voice tier (issue #613).

These are **substring-assertion** tests over the shipped command files —
the same pattern as ``anvil/skills/essay/tests/test_essay_skeleton.py``
(the PR #604 pilot). They read the command markdown as text and pin the
subject-voice-tier wiring the #613 curation locked:

- ``report-draft.md`` step 8c invokes ``resolve_subject_voice_docs`` and
  records ``metadata.subject_voice_exemplars`` (per-subject transcript map),
  composable with the existing step 8b author voice grounding.
- ``report-review.md`` step 4e resolves the tier; the per-subject pass
  extends the EXISTING dim 8 (Tone & audience calibration) sub-step; the
  ``subject_voice_grounding`` ``_summary.md`` block and the conditional
  Misattribution critical flag (``≥2 subjects``) are documented.
- The rubric stamps stay ``anvil-report-v2`` / 44 / 39 — the flag is
  **additive**, not a rubric-total change.
- ``report-revise.md`` step 6 gains a subject-tier preservation one-liner
  resolving through ``resolve_subject_voice_docs``.
- The byte-identical-when-absent contract is documented in every file.

The module filename is deliberately distinct
(``test_report_command_coverage``) per the #58 packaging convention so it
never collides with another skill's ``test_*`` module under pytest's
default import mode. The tests read files by path only — no cross-module
imports — so no ``__init__.py`` is required (matching the existing
``report/tests`` layout).

Runs under ``pytest anvil/skills/report/tests/`` or
``python -m unittest discover anvil/skills/report/tests/``.
"""

from __future__ import annotations

import unittest
from pathlib import Path

_SKILL_ROOT = Path(__file__).resolve().parent.parent

RUBRIC_ID = "anvil-report-v2"


def _read(rel: str) -> str:
    return (_SKILL_ROOT / rel).read_text(encoding="utf-8")


class TestReportDraftSubjectTier(unittest.TestCase):
    """report-draft.md step 8c: drafter contract (AC6)."""

    def setUp(self):
        self.text = _read("commands/report-draft.md")

    def test_step_8c_present(self):
        self.assertIn("8c.", self.text)

    def test_invokes_resolver(self):
        self.assertIn("resolve_subject_voice_docs", self.text)
        self.assertIn('voice_grounding.md', self.text)
        self.assertIn('"Subject voice tier"', self.text)

    def test_records_per_subject_exemplar_map(self):
        self.assertIn("subject_voice_exemplars", self.text)
        self.assertIn('{"<name>": ["<transcript path>"', self.text)

    def test_composable_with_author_tier(self):
        # Step 8b (author) and step 8c (subject) activate independently.
        self.assertIn("activates independently", self.text)
        self.assertIn("composable with it", self.text)

    def test_byte_identical_when_absent(self):
        self.assertIn("no `subjects` list", self.text)
        self.assertIn("Byte-identical to pre-#613", self.text)


class TestReportReviewSubjectTier(unittest.TestCase):
    """report-review.md steps 4e / 5 / 6 / 9 (AC7–AC10)."""

    def setUp(self):
        self.text = _read("commands/report-review.md")

    def test_step_4e_resolves_and_caches(self):
        self.assertIn("4e.", self.text)
        self.assertIn("resolve_subject_voice_docs", self.text)
        self.assertIn("subject_voice_docs_resolved", self.text)

    def test_dim_8_sub_pass_extension(self):
        # Report folds the per-subject pass into dim 8 (Tone & audience
        # calibration) — where the author voice grounding already lives.
        self.assertIn("Tone & audience calibration", self.text)
        self.assertIn(
            "subject voice tier active — <N> subject(s) scored against "
            "transcript corpora",
            self.text,
        )
        self.assertIn("MUST quote the transcript", self.text)
        self.assertIn("convergence-with-Claude", self.text)

    def test_misattribution_flag_conditional_on_two_subjects(self):
        self.assertIn("Misattribution", self.text)
        self.assertIn("≥2 subjects", self.text)
        self.assertIn("voice-identity failure", self.text)
        self.assertIn("cannot fire", self.text)

    def test_summary_block_name_and_shape(self):
        self.assertIn("subject_voice_grounding", self.text)
        self.assertIn("corpus_files_loaded", self.text)
        self.assertIn("voice_doc_loaded", self.text)
        self.assertIn("exemplars_quoted", self.text)
        self.assertIn("lines_flagged", self.text)
        self.assertIn("NOT emitted at all", self.text)
        # Both blocks emit independently when both tiers active.
        self.assertIn("emits BOTH blocks", self.text)

    def test_rubric_stamps_unchanged(self):
        self.assertIn(f'rubric_id: "{RUBRIC_ID}"', self.text)
        self.assertIn("rubric_total: 44", self.text)
        self.assertIn("advance_threshold: 39", self.text)
        self.assertIn("does NOT change the rubric total", self.text)


class TestReportReviseSubjectTier(unittest.TestCase):
    """report-revise.md step 6: subject one-liner preservation (AC11)."""

    def setUp(self):
        self.text = _read("commands/report-revise.md")

    def test_resolves_subject_voice_docs(self):
        self.assertIn("resolve_subject_voice_docs", self.text)

    def test_preservation_one_liner(self):
        self.assertIn("preserve the subject voice signatures", self.text)
        # A raised Misattribution flag cannot be declined.
        self.assertIn("Misattribution", self.text)
        self.assertIn("never `declined`", self.text)

    def test_byte_identical_when_absent(self):
        self.assertIn("byte-identical to pre-#613", self.text)


class TestReportPandocDefaultsParseContract(unittest.TestCase):
    """The #701 pandoc-3.x defaults-file fix: report's page-number CSS is
    threaded via ``variables: header-includes`` (a list of raw strings), NOT
    a top-level ``include-in-header: - text:`` block — which does not parse in
    a pandoc *defaults* file (``include-in-header`` there expects file paths).

    Mirrors ``TestRenderGateHardeningContract`` in
    ``anvil/skills/primer/tests/test_primer_command_coverage.py`` (PR #699):
    a static schema-assertion pair plus an opportunistic real-render smoke
    test that skips gracefully when the toolchain is absent.
    """

    _ASSET_REL = "assets/pandoc-defaults.yaml"

    def _asset_text(self) -> str:
        return (_SKILL_ROOT / self._ASSET_REL).read_text(encoding="utf-8")

    def test_pandoc_defaults_asset_exists(self):
        asset = _SKILL_ROOT / self._ASSET_REL
        self.assertTrue(
            asset.exists(),
            f"missing report pandoc defaults asset: {asset}",
        )

    def test_no_toplevel_include_in_header_key(self):
        """The invalid top-level ``include-in-header:`` block (with the
        inline ``- text:`` form) must be gone — that is the exact form that
        fails to parse under pandoc 3.x."""
        text = self._asset_text()
        # No unindented (top-level) include-in-header key. A commented mention
        # in the schema note is fine, so match the YAML key at column 0.
        self.assertNotIn(
            "\ninclude-in-header:",
            text,
            "top-level include-in-header: block must be removed (does not "
            "parse in a pandoc defaults file)",
        )

    def test_page_numbers_threaded_via_variables_header_includes(self):
        """Exactly one ``header-includes:`` key, nested under ``variables:``,
        carrying the page-number CSS content verbatim after it."""
        text = self._asset_text()
        # Exactly one header-includes key (the parse-valid inline path).
        self.assertEqual(text.count("header-includes:"), 1)
        # It lives under the variables: block (report keeps a single block,
        # mirroring primer's single-block discipline).
        variables_pos = text.index("\nvariables:")
        header_pos = text.index("header-includes:")
        self.assertGreater(
            header_pos,
            variables_pos,
            "header-includes: must be nested under the variables: block",
        )
        # The header-includes key is indented (nested), not at column 0.
        self.assertIn("  header-includes:", text)
        self.assertNotIn("\nheader-includes:", text)
        # The page-number CSS content is preserved verbatim, after the key.
        for needle in (
            "@page {",
            "@bottom-right {",
            'content: "Page " counter(page) " of " counter(pages);',
        ):
            self.assertIn(needle, text)
            self.assertGreater(
                text.rindex(needle),
                header_pos,
                f"{needle!r} must appear in the header-includes block",
            )

    def test_schema_note_comment_present(self):
        """The primer-style schema-note comment (adapted for CSS/weasyprint)
        must be carried over so the distinction is documented at the asset."""
        text = self._asset_text()
        self.assertIn("variables.header-includes", text)
        self.assertIn("expects a list of *file paths*", text)

    def test_pandoc_defaults_parses_and_footers_when_tools_available(self):
        """Real-render smoke: the patched defaults file must parse under
        pandoc 3.x (no Aeson exception) and the page-number footer must fire
        under weasyprint — ``pdftotext`` on the output shows ``Page 1 of 1``.

        Skipped when pandoc/weasyprint/pdftotext are absent (CI without a
        toolchain), matching the opportunistic-smoke discipline in
        ``anvil/skills/primer/tests/test_primer_command_coverage.py``.
        """
        import shutil
        import subprocess
        import tempfile

        for tool in ("pandoc", "weasyprint", "pdftotext"):
            if shutil.which(tool) is None:
                self.skipTest(f"{tool} not on PATH; skipping real-render smoke")

        asset = _SKILL_ROOT / self._ASSET_REL

        with tempfile.TemporaryDirectory() as d:
            work = Path(d)
            body = work / "report.md"
            body.write_text(
                "---\ntitle: Smoke Report\n---\n\n"
                "# A section\n\n"
                "Some body text so the document is non-empty and renders to "
                "a page.\n",
                encoding="utf-8",
            )
            pdf = work / "report.pdf"
            proc = subprocess.run(
                [
                    "pandoc",
                    str(body),
                    "-o",
                    str(pdf),
                    "--defaults",
                    str(asset),
                ],
                capture_output=True,
                text=True,
            )
            # The defaults file must PARSE + COMPILE (exit 0). A parse failure
            # surfaces as the "Aeson exception" this fix removes.
            self.assertEqual(
                proc.returncode, 0, f"pandoc failed: {proc.stderr[-800:]}"
            )
            self.assertTrue(pdf.exists() and pdf.stat().st_size > 0)

            # Functional check: the "Page N of M" footer fired under
            # weasyprint. pdftotext renders the @bottom-right counter content.
            txt = subprocess.run(
                ["pdftotext", str(pdf), "-"],
                capture_output=True,
                text=True,
            )
            self.assertEqual(txt.returncode, 0, f"pdftotext failed: {txt.stderr}")
            self.assertIn(
                "Page 1 of 1",
                txt.stdout,
                "page-number footer did not render under weasyprint",
            )


if __name__ == "__main__":
    unittest.main()
