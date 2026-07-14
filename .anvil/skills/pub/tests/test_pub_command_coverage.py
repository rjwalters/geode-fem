"""Doc-coverage tests for the ``anvil:pub`` subject voice tier (issue #613).

These are **substring-assertion** tests over the shipped command files —
the same pattern as ``anvil/skills/essay/tests/test_essay_skeleton.py``
(the PR #604 pilot). They read the command markdown as text and pin the
subject-voice-tier wiring the #613 curation locked:

- ``pub-draft.md`` step 3c invokes ``resolve_subject_voice_docs`` and
  records ``metadata.subject_voice_exemplars`` (per-subject transcript map).
- ``pub-review.md`` step 4e resolves the tier; the dim 7 sub-pass, the
  ``subject_voice_grounding`` ``_summary.md`` block, and the conditional
  Misattribution critical flag (``≥2 subjects``) are all documented.
- The rubric stamps stay ``anvil-pub-v2`` / 44 / 35 — the flag is
  **additive**, not a rubric-total change.
- ``pub-revise.md`` is DELIBERATELY out of scope (AC12): it carries no
  subject voice tier wiring, and no ``subject_voice_grounding`` block.
- The byte-identical-when-absent contract is documented in both files.

The module filename is deliberately distinct (``test_pub_command_coverage``)
per the #58 packaging convention so it never collides with another skill's
``test_*`` module under pytest's default import mode. The tests read files
by path only — no cross-module imports — so no ``__init__.py`` is required
(matching the existing ``pub/tests`` layout).

Runs under ``pytest anvil/skills/pub/tests/`` or
``python -m unittest discover anvil/skills/pub/tests/``.
"""

from __future__ import annotations

import unittest
from pathlib import Path

_SKILL_ROOT = Path(__file__).resolve().parent.parent

RUBRIC_ID = "anvil-pub-v2"


def _read(rel: str) -> str:
    return (_SKILL_ROOT / rel).read_text(encoding="utf-8")


class TestPubDraftSubjectTier(unittest.TestCase):
    """pub-draft.md step 3c: drafter contract (AC1)."""

    def setUp(self):
        self.text = _read("commands/pub-draft.md")

    def test_step_3c_present(self):
        self.assertIn("3c.", self.text)

    def test_invokes_resolver(self):
        self.assertIn("resolve_subject_voice_docs", self.text)
        self.assertIn('voice_grounding.md', self.text)
        self.assertIn('"Subject voice tier"', self.text)

    def test_records_per_subject_exemplar_map(self):
        self.assertIn("subject_voice_exemplars", self.text)
        # The per-subject map shape is documented.
        self.assertIn('{"<name>": ["<transcript path>"', self.text)

    def test_byte_identical_when_absent(self):
        self.assertIn("no `subjects` list", self.text)
        self.assertIn("Byte-identical to pre-#613", self.text)

    def test_declared_but_missing_surfaces_major(self):
        self.assertIn("missing: true", self.text)
        self.assertIn("never raises", self.text)


class TestPubReviewSubjectTier(unittest.TestCase):
    """pub-review.md steps 4e / 5 / 6 / 10 (AC2–AC5)."""

    def setUp(self):
        self.text = _read("commands/pub-review.md")

    def test_step_4e_resolves_and_caches(self):
        self.assertIn("4e.", self.text)
        self.assertIn("resolve_subject_voice_docs", self.text)

    def test_dim_7_sub_pass_folds_in(self):
        # Pub folds the per-subject pass into dim 7 (Prose & structural
        # quality) — pub has no owned voice dimension.
        self.assertIn("Prose & structural quality (D7)", self.text)
        self.assertIn(
            "subject voice tier active — <N> subject(s) scored against "
            "transcript corpora, subject-voice deductions must quote transcripts",
            self.text,
        )
        # Quote-the-transcript deduction discipline.
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
        # NOT emitted when inactive — no ran:false entry.
        self.assertIn("NOT emitted at all", self.text)

    def test_rubric_stamps_unchanged(self):
        # The flag is additive — rubric total/threshold do not move.
        self.assertIn(f'rubric_id: "{RUBRIC_ID}"', self.text)
        self.assertIn("rubric_total: 44", self.text)
        self.assertIn("advance_threshold: 35", self.text)
        # The additive-not-total-change promise is stated explicitly.
        self.assertIn("does NOT change the rubric total", self.text)


class TestPubReviseNotWired(unittest.TestCase):
    """pub-revise.md MUST NOT receive subject-tier wiring (AC12)."""

    def setUp(self):
        self.text = _read("commands/pub-revise.md")

    def test_no_subject_voice_resolver(self):
        self.assertNotIn("resolve_subject_voice_docs", self.text)

    def test_no_subject_voice_grounding_block(self):
        self.assertNotIn("subject_voice_grounding", self.text)

    def test_no_subject_voice_exemplars(self):
        self.assertNotIn("subject_voice_exemplars", self.text)


if __name__ == "__main__":
    unittest.main()
