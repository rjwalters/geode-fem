"""Tests for ``anvil.skills.memo.lib.project_brief`` (issue #285).

Covers the typed parser for the project-level ``BRIEF.md`` schema
shipped as sub-deliverable 2 of #283. The discovery primitive
(sub-deliverable 1 / #284) is tested independently in
``test_project_discovery.py``; this file is scoped to the parser
behavior.

Test coverage map (from issue #285 AC list):

- **Well-formed BRIEF** — every field parses and the typed model
  matches the on-disk shape.
- **Missing optional fields** — empty ``audience`` and ``hard_rules``
  are tolerated (lists default to empty).
- **Unknown ``artifact_type``** — closed-ended enum rejects unknown
  values with a clear error listing the registered set.
- **Slug-directory mismatch (Open Question #1)** — listed-but-missing
  warns; on-disk-but-unlisted raises.
- **Duplicate slugs** — within the documents list raises with the
  offending slug + indices.
- **Empty documents list** — raises (a BRIEF with empty documents does
  not even pass the layout-dispatch gate in #284).
- **Missing slug** — required field, raises.
- **Malformed ``target_length``** — flat-shape only, integer bounds,
  min<=max.
- **Absence-tolerant** — lenient returns None for missing BRIEF;
  strict raises FileNotFoundError.
- **On-disk fixtures** — a well-formed BRIEF fixture under
  ``fixtures/project_brief_parser/`` exercises the canonical
  brains-for-robots shape end-to-end (regression anchor for sub-
  deliverable 3 / #286 when it wires the overlay selector).

Fixtures live under ``fixtures/project_brief_parser/`` — distinct from
``fixtures/project_brief/`` (created by #284 for discovery tests) so
the two test files do not collide on the same fixtures tree.

Per the #58 packaging convention, this file's filename
(``test_project_brief.py``) is unique across the
``anvil/skills/*/tests/`` tree so the cross-skill pytest discovery
does not collide on basename.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import sys
import textwrap
import unittest
import warnings
from pathlib import Path
from tempfile import TemporaryDirectory


# The memo skill keeps its lib modules under its own ``lib/`` per the
# CLAUDE.md "skill-local first, lib promotion later" pattern. Add it to
# ``sys.path`` so tests import without a package install step — mirrors
# ``test_anvil_config.py`` and ``test_project_discovery.py`` exactly.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from project_brief import (  # noqa: E402
    ArtifactType,
    BriefDocument,
    DEFAULT_MAX_ITERATIONS,
    ProjectBrief,
    REGISTERED_ARTIFACT_TYPES,
    TargetLengthRange,
    load_project_brief,
    load_project_brief_strict,
)
from project_discovery import BRIEF_FILENAME  # noqa: E402


_FIXTURES = _HERE / "fixtures" / "project_brief_parser"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _write_brief(
    directory: Path,
    frontmatter: str,
    body: str = "\n# Project BRIEF\n",
) -> Path:
    """Write ``<directory>/BRIEF.md`` with the given frontmatter body.

    ``frontmatter`` is the raw text inserted between the opening and
    closing ``---`` delimiters (without the delimiters themselves).
    Caller passes already-dedented text (``textwrap.dedent(...).rstrip()``)
    — we do NOT re-dedent here because an interpolated multi-line value
    breaks the common-leading-whitespace rule that ``textwrap.dedent``
    relies on.
    """
    directory.mkdir(parents=True, exist_ok=True)
    brief = directory / BRIEF_FILENAME
    brief.write_text(
        f"---\n{frontmatter}\n---\n{body}",
        encoding="utf-8",
    )
    return brief


def _well_formed_frontmatter() -> str:
    """Return a canonical, valid project BRIEF frontmatter body.

    Mirrors the brains-for-robots fixture shape so tests that need a
    baseline "everything works" parse can use this directly.
    """
    return textwrap.dedent(
        """\
        project: brains-for-robots
        audience:
          - Sphere internal leadership (primary)
          - VC investors (secondary)
        hard_rules:
          - Avoid speculative claims without an evidence anchor.
          - Cite every number; cite every claim with a defensible mechanism.
        documents:
          - slug: investment-memo
            artifact_type: investment-memo
            target_length: { words: [8000, 11000] }
          - slug: latency-wall
            artifact_type: position-paper
            target_length: { words: [5000, 8000] }
          - slug: technical-vision
            artifact_type: vision-document
            target_length: { words: [3000, 4500] }
          - slug: execution-plan
            artifact_type: tactical-plan
            target_length: { words: [3000, 4500] }
          - slug: team-thesis
            artifact_type: descriptive-thesis
            target_length: { words: [2500, 4000] }
        """
    ).rstrip()


class _TmpProjectBase(unittest.TestCase):
    """Per-test temp dir for the project root."""

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self.project_dir = Path(self._td.name) / "project"
        self.project_dir.mkdir(parents=True, exist_ok=True)
        self.addCleanup(self._td.cleanup)


# ---------------------------------------------------------------------------
# Well-formed BRIEF
# ---------------------------------------------------------------------------


class TestWellFormedBrief(_TmpProjectBase):
    """A canonical BRIEF parses cleanly through both loaders."""

    def test_lenient_parses_canonical_brief(self) -> None:
        _write_brief(self.project_dir, _well_formed_frontmatter())
        brief = load_project_brief(self.project_dir)

        self.assertIsNotNone(brief)
        assert brief is not None  # for type narrowing
        self.assertEqual(brief.project, "brains-for-robots")
        self.assertEqual(len(brief.audience), 2)
        self.assertIn("Sphere internal leadership (primary)", brief.audience)
        self.assertEqual(len(brief.hard_rules), 2)
        self.assertEqual(len(brief.documents), 5)

    def test_strict_parses_canonical_brief(self) -> None:
        _write_brief(self.project_dir, _well_formed_frontmatter())
        brief = load_project_brief_strict(self.project_dir)

        self.assertEqual(brief.project, "brains-for-robots")
        self.assertEqual(len(brief.documents), 5)

    def test_documents_are_typed_brief_document_instances(self) -> None:
        _write_brief(self.project_dir, _well_formed_frontmatter())
        brief = load_project_brief_strict(self.project_dir)

        for doc in brief.documents:
            self.assertIsInstance(doc, BriefDocument)
            self.assertIsInstance(doc.artifact_type, ArtifactType)

    def test_artifact_types_match_registered_enum(self) -> None:
        _write_brief(self.project_dir, _well_formed_frontmatter())
        brief = load_project_brief_strict(self.project_dir)

        types = {doc.artifact_type for doc in brief.documents}
        expected = {
            ArtifactType.INVESTMENT_MEMO,
            ArtifactType.POSITION_PAPER,
            ArtifactType.VISION_DOCUMENT,
            ArtifactType.TACTICAL_PLAN,
            ArtifactType.DESCRIPTIVE_THESIS,
        }
        self.assertEqual(types, expected)

    def test_target_length_words_pass_through(self) -> None:
        _write_brief(self.project_dir, _well_formed_frontmatter())
        brief = load_project_brief_strict(self.project_dir)

        first = brief.documents[0]
        self.assertEqual(first.slug, "investment-memo")
        self.assertIsNotNone(first.target_length)
        assert first.target_length is not None
        self.assertEqual(first.target_length.min_words, 8000)
        self.assertEqual(first.target_length.max_words, 11000)
        self.assertEqual(first.target_length.source_key, "words")

    def test_document_for_slug_accessor(self) -> None:
        _write_brief(self.project_dir, _well_formed_frontmatter())
        brief = load_project_brief_strict(self.project_dir)

        doc = brief.document_for_slug("latency-wall")
        self.assertIsNotNone(doc)
        assert doc is not None
        self.assertEqual(doc.artifact_type, ArtifactType.POSITION_PAPER)

        missing = brief.document_for_slug("nonexistent")
        self.assertIsNone(missing)


# ---------------------------------------------------------------------------
# Missing optional fields
# ---------------------------------------------------------------------------


class TestMissingOptionalFields(_TmpProjectBase):
    """Empty ``audience`` and ``hard_rules`` lists are tolerated."""

    def test_empty_audience_list(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            audience: []
            hard_rules: []
            documents:
              - slug: only-doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(brief.audience, [])

    def test_empty_hard_rules_list(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            audience: [internal]
            hard_rules: []
            documents:
              - slug: only-doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(brief.hard_rules, [])

    def test_audience_and_hard_rules_omitted_entirely(self) -> None:
        """Both list fields default to empty when the key is absent."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: only-doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(brief.audience, [])
        self.assertEqual(brief.hard_rules, [])

    def test_target_length_optional(self) -> None:
        """A document without target_length is allowed."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: only-doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertIsNone(brief.documents[0].target_length)


# ---------------------------------------------------------------------------
# Unknown artifact_type
# ---------------------------------------------------------------------------


class TestUnknownArtifactType(_TmpProjectBase):
    """Closed-ended enum rejects unknown values with a clear error."""

    def test_unknown_artifact_type_raises_with_registered_set(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: weird-doc
                artifact_type: pamphlet
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)

        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)

        msg = str(cm.exception)
        # The error must surface the offending value.
        self.assertIn("pamphlet", msg)
        # The error must list the registered set so the operator can self-correct.
        for registered in REGISTERED_ARTIFACT_TYPES:
            self.assertIn(registered, msg)

    def test_unknown_artifact_type_lenient_also_raises(self) -> None:
        """Lenient still raises on schema violations (only absence is None)."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: weird-doc
                artifact_type: pamphlet
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError):
            load_project_brief(self.project_dir)

    def test_non_string_artifact_type(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: weird-doc
                artifact_type: 42
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("artifact_type", str(cm.exception))

    def test_missing_artifact_type(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: weird-doc
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("artifact_type", str(cm.exception))
        self.assertIn("required", str(cm.exception))


# ---------------------------------------------------------------------------
# Legacy input aliases for renamed artifact types (issue #694)
# ---------------------------------------------------------------------------


class TestLegacyArtifactTypeInputAlias(_TmpProjectBase):
    """Issue #694: the `pub` skill was renamed to `paper`.

    A consumer BRIEF authored before the rename may still carry
    ``artifact_type: pub``. The parser accepts it as an input alias and
    normalizes to the canonical ``ArtifactType.PAPER`` — nothing emits
    the legacy string. New BRIEFs use ``artifact_type: paper`` directly.
    """

    def _brief_with_type(self, artifact_type: str) -> str:
        return textwrap.dedent(
            f"""\
            project: tiny
            documents:
              - slug: my-paper
                artifact_type: {artifact_type}
            """
        ).rstrip()

    def test_legacy_pub_normalizes_to_paper(self) -> None:
        _write_brief(self.project_dir, self._brief_with_type("pub"))
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.document_for_slug("my-paper")
        self.assertIsNotNone(doc)
        assert doc is not None
        self.assertEqual(doc.artifact_type, ArtifactType.PAPER)
        # Input-only: the normalized value is the canonical string.
        self.assertEqual(doc.artifact_type.value, "paper")

    def test_canonical_paper_parses_directly(self) -> None:
        _write_brief(self.project_dir, self._brief_with_type("paper"))
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.document_for_slug("my-paper")
        assert doc is not None
        self.assertEqual(doc.artifact_type, ArtifactType.PAPER)


# ---------------------------------------------------------------------------
# Skill-identity artifact types (issue #386)
# ---------------------------------------------------------------------------


class TestSkillIdentityArtifactTypes(_TmpProjectBase):
    """Issue #386: deck / slides / proposal are registered values.

    For non-memo documents ``artifact_type`` identifies which skill owns
    the thread; it selects NO memo rubric overlay (memo's overlay
    dispatch excludes them via ``MEMO_ARTIFACT_TYPES``).
    """

    def test_skill_identity_values_accepted(self) -> None:
        for value in ("deck", "slides", "proposal"):
            with self.subTest(artifact_type=value):
                fm = textwrap.dedent(
                    f"""\
                    project: tiny
                    documents:
                      - slug: some-thread
                        artifact_type: {value}
                    """
                ).rstrip()
                _write_brief(self.project_dir, fm)
                brief = load_project_brief_strict(self.project_dir)
                self.assertEqual(
                    brief.documents[0].artifact_type, ArtifactType(value)
                )

    def test_datasheet_artifact_type_round_trips(self) -> None:
        """Issue #486: a validated BRIEF carrying ``artifact_type:
        datasheet`` round-trips through strict validation to the
        registered enum member — the carrier rubric-rebackport's
        BRIEF-route inference (#484) was missing."""
        fm = textwrap.dedent(
            """\
            project: parts
            documents:
              - slug: ax101-objdet
                artifact_type: datasheet
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(
            brief.documents[0].artifact_type, ArtifactType.DATASHEET
        )
        self.assertEqual(brief.documents[0].artifact_type.value, "datasheet")

    def test_pitch_deck_rejected_listing_all_registered_values(self) -> None:
        """The studio's informal 'pitch-deck' stays unregistered — the
        closed-ended error lists all eighteen registered values so the
        operator can self-correct."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: series-a-deck
                artifact_type: pitch-deck
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("pitch-deck", msg)
        self.assertEqual(len(REGISTERED_ARTIFACT_TYPES), 18)
        for registered in REGISTERED_ARTIFACT_TYPES:
            self.assertIn(registered, msg)

    def test_memo_subset_excludes_skill_identity_values(self) -> None:
        from project_brief import MEMO_ARTIFACT_TYPES  # noqa: PLC0415

        self.assertEqual(
            MEMO_ARTIFACT_TYPES,
            frozenset(
                {
                    ArtifactType.INVESTMENT_MEMO,
                    ArtifactType.POSITION_PAPER,
                    ArtifactType.TACTICAL_PLAN,
                    ArtifactType.VISION_DOCUMENT,
                    ArtifactType.DESCRIPTIVE_THESIS,
                    ArtifactType.CHALLENGE_MEMO,
                    ArtifactType.STRATEGY_MEMO,
                }
            ),
        )
        for skill_identity in (
            ArtifactType.DECK,
            ArtifactType.SLIDES,
            ArtifactType.PROPOSAL,
            ArtifactType.PAPER,
            ArtifactType.REPORT,
            ArtifactType.IP_USPTO,
            ArtifactType.IP_USPTO_PROVISIONAL,
            ArtifactType.ESSAY,
            ArtifactType.DATASHEET,
            ArtifactType.PRIMER,
            ArtifactType.SPEC,
        ):
            self.assertNotIn(skill_identity, MEMO_ARTIFACT_TYPES)

    def test_skill_identity_set_is_explicit(self) -> None:
        """Issue #394: the #386 guard is re-keyed onto an explicit set.

        Issue #408 grew the set with ``pub`` (research-paper threads —
        the project-migrate BRIEF-synthesis registry gap; the skill was
        renamed ``pub`` → ``paper`` under #694, so the enum member is now
        ``ArtifactType.PAPER`` and the legacy ``pub`` string is an input
        alias); issue #432
        grew it with ``report`` (the vN report-dir adoption mode's
        inferred type — the same registry-gap shape); issue #440 grew
        it with ``ip-uspto`` / ``ip-uspto-provisional`` (the
        letter-family adoption mode's REQUIRED --artifact-type values
        — same registry-gap shape, legal-artifact stakes); issue #460
        grew it with ``essay`` (the ``anvil:essay`` artifact class —
        short-form voice-grounded essays own their threads in a shared
        project BRIEF); issue #486 grew it with ``datasheet`` (the
        ``anvil:datasheet`` artifact class — shipped #418/#421 before
        this registry pattern was consistently applied, backfilled so
        rubric-rebackport's BRIEF-route inference has a validated
        carrier); issues #686/#687 grew it with ``primer`` (the
        ``anvil:primer`` artifact class — long-form pedagogical
        explainers own their threads in a shared project BRIEF, same
        skill-identity shape); issues #697/#706 grew it with ``spec``
        (the ``anvil:spec`` artifact class — normative technical
        specifications maintained against an implementation own their
        threads in a shared project BRIEF, same skill-identity shape)."""
        from project_brief import (  # noqa: PLC0415
            SKILL_IDENTITY_ARTIFACT_TYPES,
        )

        self.assertEqual(
            SKILL_IDENTITY_ARTIFACT_TYPES,
            frozenset(
                {
                    ArtifactType.DECK,
                    ArtifactType.SLIDES,
                    ArtifactType.PROPOSAL,
                    ArtifactType.PAPER,
                    ArtifactType.REPORT,
                    ArtifactType.IP_USPTO,
                    ArtifactType.IP_USPTO_PROVISIONAL,
                    ArtifactType.ESSAY,
                    ArtifactType.DATASHEET,
                    ArtifactType.PRIMER,
                    ArtifactType.SPEC,
                }
            ),
        )


# ---------------------------------------------------------------------------
# Registered memo genres added under #394 (challenge-memo / strategy-memo)
# ---------------------------------------------------------------------------


class TestCanaryMemoGenres(_TmpProjectBase):
    """Issue #394: the canary-proven challenge-memo / strategy-memo
    genres are registered memo subtypes — a BRIEF declaring either
    parses cleanly to the enum member."""

    def test_new_memo_genres_accepted(self) -> None:
        for value in ("challenge-memo", "strategy-memo"):
            with self.subTest(artifact_type=value):
                fm = textwrap.dedent(
                    f"""\
                    project: tiny
                    documents:
                      - slug: some-thread
                        artifact_type: {value}
                    """
                ).rstrip()
                _write_brief(self.project_dir, fm)
                brief = load_project_brief_strict(self.project_dir)
                self.assertEqual(
                    brief.documents[0].artifact_type, ArtifactType(value)
                )
                self.assertIsInstance(
                    brief.documents[0].artifact_type, ArtifactType
                )


# ---------------------------------------------------------------------------
# Slug-directory mismatch (Open Question #1)
# ---------------------------------------------------------------------------


class TestSlugDirectoryDivergence(_TmpProjectBase):
    """Asymmetric rule: warn on listed-but-missing; error on on-disk-but-unlisted."""

    def _make_thread_dir(self, slug: str) -> None:
        """Create ``<project>/<slug>/<slug>.1/memo.md`` — a "started thread"."""
        thread = self.project_dir / slug
        thread.mkdir(parents=True, exist_ok=True)
        v1 = thread / f"{slug}.1"
        v1.mkdir(parents=True, exist_ok=True)
        (v1 / "memo.md").write_text("# memo\n", encoding="utf-8")

    def test_listed_but_missing_warns(self) -> None:
        """A BRIEF entry with no on-disk directory triggers a UserWarning."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: started-doc
                artifact_type: investment-memo
              - slug: not-yet-started
                artifact_type: position-paper
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        self._make_thread_dir("started-doc")

        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            brief = load_project_brief_strict(
                self.project_dir, validate_dirs=True
            )

        # Returned brief is unchanged.
        self.assertEqual(len(brief.documents), 2)

        # Exactly one warning emitted; mentions the missing slug.
        warning_msgs = [str(w.message) for w in caught]
        listed_but_missing = [
            m for m in warning_msgs if "not-yet-started" in m
        ]
        self.assertEqual(
            len(listed_but_missing),
            1,
            f"Expected exactly one warning naming 'not-yet-started'; got {warning_msgs}",
        )

    def test_on_disk_but_unlisted_raises(self) -> None:
        """A directory present on disk but absent from BRIEF.documents raises."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: listed-doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        self._make_thread_dir("listed-doc")
        self._make_thread_dir("unlisted-doc")  # configuration drift

        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(
                self.project_dir, validate_dirs=True
            )

        msg = str(cm.exception)
        self.assertIn("unlisted-doc", msg)
        # The error must mention "drift" or "not listed" so the operator
        # understands what's wrong.
        self.assertTrue(
            "drift" in msg.lower() or "not listed" in msg.lower(),
            f"Error message should mention configuration drift: {msg}",
        )

    def test_validate_dirs_off_by_default(self) -> None:
        """Without ``validate_dirs=True``, divergence is not checked."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: listed-doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        self._make_thread_dir("unlisted-doc")

        # No exception, no warning.
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(brief.project, "tiny")
        # No divergence warnings.
        unlisted_warnings = [
            w for w in caught if "unlisted-doc" in str(w.message)
        ]
        self.assertEqual(unlisted_warnings, [])

    def test_research_dir_not_treated_as_thread_root(self) -> None:
        """``research/`` (no version dirs) is not flagged as on-disk-but-unlisted."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: started-doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        self._make_thread_dir("started-doc")
        # research/ is project-level infrastructure, not a thread root.
        (self.project_dir / "research").mkdir()
        (self.project_dir / "research" / "evidence.md").write_text(
            "# evidence\n", encoding="utf-8"
        )

        # Should not raise.
        brief = load_project_brief_strict(
            self.project_dir, validate_dirs=True
        )
        self.assertEqual(brief.project, "tiny")

    def test_dotted_sibling_dir_not_treated_as_thread_root(self) -> None:
        """``.review/`` / ``.audit/`` siblings are skipped (not thread roots)."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: started-doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        self._make_thread_dir("started-doc")
        (self.project_dir / ".cache").mkdir()

        # Should not raise.
        brief = load_project_brief_strict(
            self.project_dir, validate_dirs=True
        )
        self.assertEqual(brief.project, "tiny")


# ---------------------------------------------------------------------------
# Duplicate slugs
# ---------------------------------------------------------------------------


class TestDuplicateSlugs(_TmpProjectBase):
    """Slugs must be unique within the BRIEF's documents list."""

    def test_duplicate_slug_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: dup
                artifact_type: investment-memo
              - slug: dup
                artifact_type: position-paper
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)

        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)

        msg = str(cm.exception)
        self.assertIn("'dup'", msg)
        # The error should name both indices so the author knows where to
        # look (offending entry + first occurrence).
        self.assertIn("0", msg)
        self.assertIn("1", msg)


# ---------------------------------------------------------------------------
# Empty documents list / missing documents key
# ---------------------------------------------------------------------------


class TestEmptyDocumentsList(_TmpProjectBase):
    """The documents list must be non-empty."""

    def test_empty_documents_list_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents: []
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)

        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("documents", msg)
        self.assertIn("non-empty", msg)

    def test_missing_documents_key_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            audience: [test]
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)

        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("documents", msg)

    def test_documents_as_non_list_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents: a-string
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)

        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("documents", str(cm.exception))


# ---------------------------------------------------------------------------
# Missing slug
# ---------------------------------------------------------------------------


class TestMissingSlug(_TmpProjectBase):
    """slug is a required field on every document entry."""

    def test_missing_slug_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)

        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("slug", msg)

    def test_empty_string_slug_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: ""
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)

        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("slug", str(cm.exception))

    def test_whitespace_only_slug_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: "   "
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("slug", str(cm.exception))


# ---------------------------------------------------------------------------
# Malformed target_length
# ---------------------------------------------------------------------------


class TestMalformedTargetLength(_TmpProjectBase):
    """The flat target_length shape is the only accepted form."""

    def test_target_length_pages_converts_to_words(self) -> None:
        """``pages: [n, m]`` is accepted and converts at 600 wpp."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: short-doc
                artifact_type: investment-memo
                target_length: { pages: [4, 6] }
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        tl = brief.documents[0].target_length
        self.assertIsNotNone(tl)
        assert tl is not None
        self.assertEqual(tl.source_key, "pages")
        self.assertEqual(tl.min_words, 2400)
        self.assertEqual(tl.max_words, 3600)

    def test_target_length_with_both_keys_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                target_length: { words: [100, 200], pages: [1, 2] }
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("ambiguous", str(cm.exception).lower())

    def test_target_length_min_greater_than_max_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                target_length: { words: [200, 100] }
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("min <= max", str(cm.exception))

    def test_target_length_negative_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                target_length: { words: [-100, 200] }
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("non-negative", str(cm.exception))

    def test_target_length_three_element_list_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                target_length: { words: [100, 200, 300] }
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("2-element", str(cm.exception))

    def test_target_length_non_dict_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                target_length: 1000
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("target_length", str(cm.exception))

    def test_target_length_extended_shape_keys_rejected(self) -> None:
        """``default`` / ``overrides`` (extended shape) is rejected at BRIEF level."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                target_length:
                  default: { words: [100, 200] }
                  overrides: {}
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("extended-shape", str(cm.exception))

    def test_target_length_neither_words_nor_pages_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                target_length: { paragraphs: [10, 20] }
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        # The unknown key ('paragraphs') is rejected because the loader
        # doesn't see words/pages, OR because of a more specific message.
        self.assertIn("target_length", msg)


# ---------------------------------------------------------------------------
# Absence-tolerant behavior
# ---------------------------------------------------------------------------


class TestAbsenceTolerant(_TmpProjectBase):
    """Lenient returns None on absence; strict raises FileNotFoundError."""

    def test_lenient_returns_none_when_no_brief(self) -> None:
        result = load_project_brief(self.project_dir)
        self.assertIsNone(result)

    def test_lenient_returns_none_when_no_frontmatter(self) -> None:
        (self.project_dir / BRIEF_FILENAME).write_text(
            "# A BRIEF with no frontmatter at all\n", encoding="utf-8"
        )
        result = load_project_brief(self.project_dir)
        self.assertIsNone(result)

    def test_lenient_returns_none_when_yaml_unparseable(self) -> None:
        (self.project_dir / BRIEF_FILENAME).write_text(
            "---\nproject: tiny\ndocuments: [unclosed\n---\n# body\n",
            encoding="utf-8",
        )
        result = load_project_brief(self.project_dir)
        self.assertIsNone(result)

    def test_strict_raises_filenotfound_when_no_brief(self) -> None:
        with self.assertRaises(FileNotFoundError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("No BRIEF found", msg)
        self.assertIn(BRIEF_FILENAME, msg)

    def test_strict_raises_valueerror_when_no_frontmatter(self) -> None:
        (self.project_dir / BRIEF_FILENAME).write_text(
            "# A BRIEF with no frontmatter at all\n", encoding="utf-8"
        )
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("frontmatter", str(cm.exception))


# ---------------------------------------------------------------------------
# Project-name required
# ---------------------------------------------------------------------------


class TestProjectField(_TmpProjectBase):
    """``project`` is required, non-empty string."""

    def test_missing_project_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            audience: [test]
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("project", str(cm.exception))

    def test_empty_project_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: ""
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("project", str(cm.exception))


# ---------------------------------------------------------------------------
# Audience / hard_rules type errors
# ---------------------------------------------------------------------------


class TestAudienceHardRulesValidation(_TmpProjectBase):
    """audience / hard_rules must be lists of strings when present."""

    def test_audience_as_string_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            audience: "just a string"
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("audience", str(cm.exception))

    def test_audience_with_non_string_entry_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            audience: [valid, 42]
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        self.assertIn("audience", str(cm.exception))


# ---------------------------------------------------------------------------
# Audience dict-shape (issue #546)
# ---------------------------------------------------------------------------


class TestAudienceDictShape(_TmpProjectBase):
    """``audience: {primary, secondary, ...}`` dict shape normalization.

    Issue #546 — the studio's canonical multi-thread BRIEF convention
    uses the dict form. Before this fix, ``_normalize_string_list``
    hard-rejected the dict shape, which silently routed drafters around
    the entire parser (the bare ``except`` in render_gate's theme
    discovery swallowed the ValueError, losing paired-override
    validation and silently disabling the ``theme:`` system). These
    tests pin the new acceptance contract.
    """

    def test_dict_with_primary_and_secondary(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            audience:
              primary: Sphere internal leadership
              secondary: VC investors
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(
            brief.audience,
            ["Sphere internal leadership", "VC investors"],
        )

    def test_dict_with_list_values_flattens_per_role(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            audience:
              primary:
                - Sphere leadership
                - Sphere board
              secondary: VC investors
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(
            brief.audience,
            ["Sphere leadership", "Sphere board", "VC investors"],
        )

    def test_dict_role_precedence_order_not_yaml_order(self) -> None:
        # primary appears AFTER tertiary in YAML insertion order; the
        # flattener must still emit primary first.
        fm = textwrap.dedent(
            """\
            project: tiny
            audience:
              tertiary: Tertiary audience
              primary: Primary audience
              secondary: Secondary audience
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(
            brief.audience,
            ["Primary audience", "Secondary audience", "Tertiary audience"],
        )

    def test_dict_with_unknown_subkey_warns_and_preserves_value(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            audience:
              primary: Sphere
              quaternary: Future role
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with warnings.catch_warnings(record=True) as captured:
            warnings.simplefilter("always")
            brief = load_project_brief_strict(self.project_dir)
        # Unknown keys land at the tail of the flattened list.
        self.assertEqual(brief.audience, ["Sphere", "Future role"])
        # The breadcrumb names the unknown sub-key explicitly.
        unknown_warnings = [
            w for w in captured
            if "quaternary" in str(w.message)
            and "audience" in str(w.message)
        ]
        self.assertTrue(
            unknown_warnings,
            f"expected an audience-sub-key warning naming "
            f"'quaternary'; got {[str(w.message) for w in captured]}",
        )

    def test_dict_with_non_string_value_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            audience:
              primary: 42
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("audience", msg)
        self.assertIn("primary", msg)

    def test_dict_with_non_string_list_entry_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            audience:
              primary:
                - valid
                - 42
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("audience", msg)
        self.assertIn("primary", msg)

    def test_backward_compat_flat_list_still_parses(self) -> None:
        # Pin the back-compat path: the legacy flat list continues to
        # parse exactly as it did before this helper was introduced.
        fm = textwrap.dedent(
            """\
            project: tiny
            audience:
              - Sphere internal leadership
              - VC investors
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(
            brief.audience,
            ["Sphere internal leadership", "VC investors"],
        )

    def test_dict_audience_does_not_bypass_max_iterations_check(self) -> None:
        # Load-bearing regression pin (#546 acceptance gate): a BRIEF
        # using the dict-form audience must still trigger the
        # ``max_iterations`` paired-override validator downstream.
        # Before the fix, drafters who wrote the dict shape silently
        # routed around the entire parser via render_gate's bare
        # ``except`` — losing this validation and silently disabling
        # the theme system.
        fm = textwrap.dedent(
            """\
            project: tiny
            audience:
              primary: Sphere internal leadership
              secondary: VC investors
            documents:
              - slug: doc
                artifact_type: investment-memo
                max_iterations: 12
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        # The paired-override validator fires with both field names.
        self.assertIn("max_iterations", msg)
        self.assertIn("iteration_cap_rationale", msg)


# ---------------------------------------------------------------------------
# Unknown keys on document entries
# ---------------------------------------------------------------------------


class TestUnknownDocumentKeys(_TmpProjectBase):
    """Unknown keys on a document entry raise a clear error."""

    def test_unknown_key_on_document_raises(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                max_iterations: 8
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("max_iterations", msg)


# ---------------------------------------------------------------------------
# Per-document render_engine override (issue #320)
# ---------------------------------------------------------------------------


class TestDocumentRenderEngine(_TmpProjectBase):
    """Per-document ``render_engine`` knob (issue #320).

    The BRIEF parser enforces a closed allowlist of three values
    (``"weasyprint"``, ``"xelatex"``, ``"wkhtmltopdf"``). The actual
    runtime fallthrough (requested-but-unavailable-on-PATH) lives in
    ``anvil.lib.render_gate._select_memo_engine`` — these tests pin
    the parse-time contract only.
    """

    def test_brief_document_accepts_render_engine_weasyprint(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                render_engine: weasyprint
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(len(brief.documents), 1)
        self.assertEqual(brief.documents[0].render_engine, "weasyprint")

    def test_brief_document_accepts_render_engine_xelatex(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                render_engine: xelatex
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(brief.documents[0].render_engine, "xelatex")

    def test_brief_document_accepts_render_engine_wkhtmltopdf(self) -> None:
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                render_engine: wkhtmltopdf
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(brief.documents[0].render_engine, "wkhtmltopdf")

    def test_brief_document_rejects_invalid_render_engine(self) -> None:
        """Unknown engine value → ``ValueError`` listing the valid set."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                render_engine: foo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        # Error must name the offending value AND surface the valid set
        # so the operator can self-correct without reading source.
        self.assertIn("render_engine", msg)
        self.assertIn("foo", msg)
        self.assertIn("weasyprint", msg)
        self.assertIn("xelatex", msg)
        self.assertIn("wkhtmltopdf", msg)

    def test_brief_document_render_engine_optional(self) -> None:
        """Entries without ``render_engine:`` parse cleanly (back-compat).

        Every existing BRIEF in the canary continues to load without
        change after #320 — the field is purely additive.
        """
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertIsNone(brief.documents[0].render_engine)

    def test_brief_document_render_engine_persists_to_model(self) -> None:
        """Round-trip: ``render_engine: xelatex`` in YAML frontmatter →
        ``BriefDocument.render_engine == 'xelatex'`` on the parsed model.

        Regression anchor for the plumbing into
        ``_progress.json.metadata.render_engine_requested`` at draft and
        revise time: the value the drafter / reviser reads off the
        BRIEF document model must equal the value the operator wrote
        in the BRIEF frontmatter (no normalization, no coercion).
        """
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                render_engine: xelatex
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.document_for_slug("doc")
        self.assertIsNotNone(doc)
        assert doc is not None  # type narrowing
        self.assertEqual(doc.render_engine, "xelatex")


# ---------------------------------------------------------------------------
# Per-document latex_header_includes override (issue #347)
# ---------------------------------------------------------------------------


class TestDocumentLatexHeaderIncludes(_TmpProjectBase):
    """Per-document ``latex_header_includes`` knob (issue #347).

    The BRIEF parser only enforces type (string-or-None) and normalizes
    empty / whitespace-only inputs to ``None``. The contents are opaque
    LaTeX — no value-shape enforcement, no engine-conditional gating at
    parse time. The runtime engine-scoping
    (xelatex-only ``--include-in-header`` injection) lives in
    ``anvil.lib.render_gate._render_memo_source`` — these tests pin
    the parse-time contract only.
    """

    def test_brief_document_accepts_latex_header_includes_multiline(self) -> None:
        """A YAML block-literal preamble parses verbatim onto the model."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                render_engine: xelatex
                latex_header_includes: |
                  \\usepackage{xcolor}
                  \\definecolor{green}{HTML}{059669}
                  \\usepackage{tabularx}
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(len(brief.documents), 1)
        value = brief.documents[0].latex_header_includes
        self.assertIsNotNone(value)
        assert value is not None  # type narrowing
        # Every load-bearing token survives the round-trip verbatim.
        self.assertIn("\\usepackage{xcolor}", value)
        self.assertIn("\\definecolor{green}{HTML}{059669}", value)
        self.assertIn("\\usepackage{tabularx}", value)

    def test_brief_document_accepts_latex_header_includes_single_line(self) -> None:
        """A single-line quoted preamble parses as a flat string."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                latex_header_includes: "\\\\usepackage{xcolor}"
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(
            brief.documents[0].latex_header_includes, "\\usepackage{xcolor}"
        )

    def test_brief_document_rejects_non_string_latex_header_includes(self) -> None:
        """Non-string types (lists, dicts, ints) raise with a clear path."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                latex_header_includes:
                  - "\\\\usepackage{xcolor}"
                  - "\\\\usepackage{tabularx}"
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("latex_header_includes", msg)
        self.assertIn("must be a string", msg)

    def test_brief_document_treats_empty_latex_header_includes_as_none(self) -> None:
        """Empty / whitespace-only values normalize to ``None`` so a YAML
        author can write the key with no value and get back-compat behavior
        (no preamble include, identical to the absence-of-key case).
        """
        # Whitespace-only (multi-line of spaces).
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                latex_header_includes: "   "
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertIsNone(brief.documents[0].latex_header_includes)

    def test_brief_document_latex_header_includes_optional(self) -> None:
        """Entries without ``latex_header_includes:`` parse cleanly.

        Every existing BRIEF in the canary continues to load without
        change after #347 — the field is purely additive.
        """
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertIsNone(brief.documents[0].latex_header_includes)

    def test_brief_document_latex_header_includes_persists_to_model(self) -> None:
        """Round-trip: ``latex_header_includes`` in YAML frontmatter →
        ``BriefDocument.latex_header_includes`` on the parsed model
        (no normalization beyond whitespace-only → ``None``).

        Regression anchor for the plumbing into
        ``_progress.json.metadata.latex_header_includes_resolved`` at
        draft and revise time: the value the drafter / reviser reads
        off the BRIEF document model must equal the value the operator
        wrote in the BRIEF frontmatter, byte for byte.
        """
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                render_engine: xelatex
                latex_header_includes: |
                  \\usepackage{xcolor}
                  \\definecolor{ink}{HTML}{0f172a}
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.document_for_slug("doc")
        self.assertIsNotNone(doc)
        assert doc is not None  # type narrowing
        value = doc.latex_header_includes
        self.assertIsNotNone(value)
        assert value is not None
        # The block-literal preserves every line and only strips the
        # last trailing newline; the assertion targets specific
        # substrings to avoid a brittle whitespace match while still
        # pinning the verbatim-preservation contract.
        self.assertIn("\\usepackage{xcolor}", value)
        self.assertIn("\\definecolor{ink}{HTML}{0f172a}", value)


# ---------------------------------------------------------------------------
# Per-document iteration-cap override (issue #349)
# ---------------------------------------------------------------------------


class TestDocumentIterationCapOverride(_TmpProjectBase):
    """Per-document ``max_iterations`` + ``iteration_cap_rationale`` paired
    override (issue #349).

    The override is **paired**: both keys must be present and well-formed
    for the override to take effect, OR both must be absent. Setting one
    without the other is a schema violation at parse time.

    Validation contract (mirrors the deck skill's `.anvil.json` precedent
    documented at ``anvil/skills/deck/SKILL.md`` §"Per-thread override
    contract"):

    - ``max_iterations`` set with a non-empty ``iteration_cap_rationale``
      → honor the override.
    - ``max_iterations`` set WITHOUT a valid rationale (missing, empty,
      whitespace-only) → ``ValueError``.
    - ``iteration_cap_rationale`` set WITHOUT a ``max_iterations`` value
      → ``ValueError``.
    - ``max_iterations < DEFAULT_MAX_ITERATIONS`` (with or without
      rationale) → ``ValueError``. The override may raise the cap but not
      lower it below the principled default.
    - Both keys absent → no override, default cap applies.
    """

    def test_paired_override_parses(self) -> None:
        """Both keys present + well-formed → override honored."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: aldus
                artifact_type: investment-memo
                max_iterations: 5
                iteration_cap_rationale: |
                  Operator-extended to 5 on 2026-06-08. Reason: v4 verdict
                  34/44 vs floor 35, gap is design-side; reviewer
                  identified memo-revise can close it.
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        self.assertEqual(len(brief.documents), 1)
        doc = brief.documents[0]
        self.assertEqual(doc.max_iterations, 5)
        self.assertIsNotNone(doc.iteration_cap_rationale)
        assert doc.iteration_cap_rationale is not None  # type narrowing
        # The rationale's load-bearing tokens survive verbatim.
        self.assertIn("Operator-extended to 5", doc.iteration_cap_rationale)
        self.assertIn("design-side", doc.iteration_cap_rationale)

    def test_override_both_keys_absent_yields_none(self) -> None:
        """Default cap path: neither key on the document entry.

        Every legacy BRIEF (pre-#349) lands here. The default cap of 4
        applies at the consumer side (drafter / reviser default).
        """
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.documents[0]
        self.assertIsNone(doc.max_iterations)
        self.assertIsNone(doc.iteration_cap_rationale)

    def test_max_iterations_without_rationale_raises(self) -> None:
        """The paired-override contract rejects unjustified raises."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                max_iterations: 5
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("max_iterations", msg)
        self.assertIn("iteration_cap_rationale", msg)
        # The error names BOTH fields and the load-bearing rationale.
        self.assertIn("paired-override", msg)

    def test_max_iterations_with_empty_rationale_raises(self) -> None:
        """Whitespace-only rationale normalizes to ``None`` → paired-
        validator rejects the now-orphaned ``max_iterations``.
        """
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                max_iterations: 5
                iteration_cap_rationale: "   "
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("max_iterations", msg)
        self.assertIn("iteration_cap_rationale", msg)

    def test_rationale_without_max_iterations_raises(self) -> None:
        """Rationale alone is also an unbalanced paired override."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                iteration_cap_rationale: |
                  Stale rationale left over from a prior override.
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("max_iterations", msg)
        self.assertIn("iteration_cap_rationale", msg)

    def test_max_iterations_below_default_raises(self) -> None:
        """The override may raise the cap but not lower it below the
        principled default (4)."""
        fm = textwrap.dedent(
            f"""\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                max_iterations: 3
                iteration_cap_rationale: |
                  Trying to lower the cap is rejected even with a rationale.
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("max_iterations", msg)
        # The error names the floor explicitly so the operator can
        # self-correct without grepping the codebase.
        self.assertIn(str(DEFAULT_MAX_ITERATIONS), msg)

    def test_max_iterations_at_default_is_accepted(self) -> None:
        """The default value itself is allowed (no-op raise but still
        records the rationale in BRIEF.md for the audit trail).
        """
        fm = textwrap.dedent(
            f"""\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                max_iterations: {DEFAULT_MAX_ITERATIONS}
                iteration_cap_rationale: |
                  Explicit acknowledgement that the default cap applies
                  to this thread — used to silence the discoverability
                  pointer.
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.documents[0]
        self.assertEqual(doc.max_iterations, DEFAULT_MAX_ITERATIONS)
        self.assertIsNotNone(doc.iteration_cap_rationale)

    def test_max_iterations_non_integer_raises(self) -> None:
        """Non-integer types are rejected at parse time."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                max_iterations: "five"
                iteration_cap_rationale: |
                  Trying to slip a string past the validator.
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("max_iterations", msg)
        self.assertIn("integer", msg)

    def test_max_iterations_bool_rejected(self) -> None:
        """Booleans masquerade as integers in YAML — reject explicitly so
        a stray ``true`` / ``false`` never silently degrades the override.
        """
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                max_iterations: true
                iteration_cap_rationale: |
                  Boolean snuck in for max_iterations.
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("max_iterations", msg)

    def test_rationale_non_string_raises(self) -> None:
        """Non-string rationale types (lists, dicts, ints) are rejected."""
        fm = textwrap.dedent(
            """\
            project: tiny
            documents:
              - slug: doc
                artifact_type: investment-memo
                max_iterations: 5
                iteration_cap_rationale:
                  - "Multiple reasons"
                  - "Crammed into a list"
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        with self.assertRaises(ValueError) as cm:
            load_project_brief_strict(self.project_dir)
        msg = str(cm.exception)
        self.assertIn("iteration_cap_rationale", msg)
        self.assertIn("must be a string", msg)

    def test_override_persists_to_document_for_slug(self) -> None:
        """The lookup helper surfaces the elevated cap + rationale.

        Regression anchor for the plumbing into
        ``_progress.json.metadata.max_iterations`` and
        ``_progress.json.metadata.iteration_cap_rationale`` at draft and
        revise time: the values the drafter / reviser reads off the
        BRIEF document model must equal the values the operator wrote
        in the BRIEF frontmatter, byte for byte.
        """
        fm = textwrap.dedent(
            """\
            project: aldus-portfolio
            documents:
              - slug: aldus
                artifact_type: investment-memo
                max_iterations: 6
                iteration_cap_rationale: |
                  Well-conditioned thread: v1=27, v2=29, v3=31, v4=34.
                  Monotonic improvement, 0-critical first at v4. One extra
                  pass to land Sphere outcome detail.
              - slug: other
                artifact_type: position-paper
            """
        ).rstrip()
        _write_brief(self.project_dir, fm)
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.document_for_slug("aldus")
        self.assertIsNotNone(doc)
        assert doc is not None  # type narrowing
        self.assertEqual(doc.max_iterations, 6)
        self.assertIsNotNone(doc.iteration_cap_rationale)
        assert doc.iteration_cap_rationale is not None
        self.assertIn("Well-conditioned thread", doc.iteration_cap_rationale)
        self.assertIn("Sphere outcome detail", doc.iteration_cap_rationale)

        # Sibling doc with no override carries both fields as None — the
        # override is per-document, not per-project.
        other = brief.document_for_slug("other")
        self.assertIsNotNone(other)
        assert other is not None
        self.assertIsNone(other.max_iterations)
        self.assertIsNone(other.iteration_cap_rationale)


# ---------------------------------------------------------------------------
# On-disk fixture (the brains-for-robots canary shape)
# ---------------------------------------------------------------------------


class TestOnDiskFixture(unittest.TestCase):
    """End-to-end parse against the on-disk fixture.

    Regression anchor for sub-deliverable 3 (#286) when it wires the
    overlay selector — the fixture covers the canonical five-document
    project shape and should remain parseable through both loaders as
    the parser evolves.
    """

    def test_brains_for_robots_fixture_parses(self) -> None:
        fixture = _FIXTURES / "brains-for-robots"
        self.assertTrue(
            fixture.exists(),
            f"fixture missing: {fixture}",
        )

        brief = load_project_brief_strict(fixture)
        self.assertEqual(brief.project, "brains-for-robots")
        self.assertEqual(len(brief.documents), 5)

        slugs = {doc.slug for doc in brief.documents}
        self.assertEqual(
            slugs,
            {
                "investment-memo",
                "latency-wall",
                "technical-vision",
                "execution-plan",
                "team-thesis",
            },
        )

    def test_well_formed_minimal_fixture_parses(self) -> None:
        """A minimal one-document BRIEF parses too."""
        fixture = _FIXTURES / "minimal-one-doc"
        self.assertTrue(
            fixture.exists(),
            f"fixture missing: {fixture}",
        )

        brief = load_project_brief_strict(fixture)
        self.assertEqual(brief.project, "minimal")
        self.assertEqual(len(brief.documents), 1)
        self.assertEqual(brief.documents[0].slug, "only-doc")
        self.assertEqual(
            brief.documents[0].artifact_type, ArtifactType.INVESTMENT_MEMO
        )


if __name__ == "__main__":
    unittest.main()
