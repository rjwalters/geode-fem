"""Tests for the per-doc ``rubric_overrides`` block on ``BRIEF.md``.

Covers the issue #296 consolidation that moved the ``rubric_overrides``
shape (formerly the top-level block in ``<thread>/.anvil.json``) into
each ``documents:`` entry of the project-level ``BRIEF.md``. The schema
and reader live in ``anvil/skills/memo/lib/project_brief.py`` (this
absorbed the prior ``anvil_config.py`` which is now deleted).

This file replaces the prior ``test_anvil_config.py`` (sub-issue 1 of
#233) and ``test_anvil_json_examples_roundtrip.py`` (sub-issue 3 of
#233 / #266). The schema field set, the validation rules, and the
loader contract are now per-doc on the BRIEF entry — but the typed
``RubricOverrides`` model (carrying ``memo_subtype``,
``calibrations``, ``target_length``, ``unknown_keys``) and the
``CalibrationOverride`` per-dim model stay byte-identical to PR #267's
contract so the reviewer integration in
``rubric_overrides_suffix.py`` continues to work unchanged.

What this file pins
-------------------

1. **Per-doc on-disk shape parses** — a BRIEF with one or more
   ``documents:`` entries carrying ``rubric_overrides:`` blocks
   produces typed :class:`BriefDocument` entries whose
   ``rubric_overrides`` field is a populated :class:`RubricOverrides`.

2. **Validation discipline (STRICT)** — malformed
   ``memo_subtype`` / ``dim_N_calibration`` / ``target_length`` shapes
   raise ``ValueError`` (BRIEF parser is strict-by-design; the
   historical lenient form lived on the ``.anvil.json`` side).

3. **Unknown-key forward-compat** — keys that are not
   ``memo_subtype`` / ``dim_N_calibration`` / ``target_length`` are
   preserved verbatim under ``RubricOverrides.unknown_keys`` (the
   forward-compat surface from PR #267 is preserved exactly).

4. **load_rubric_overrides_for_slug** — the convenience wrapper
   replaces the prior ``anvil_config.load_rubric_overrides(thread_dir)``
   API. The empty-on-absence contract is byte-identical (every
   absence path returns an empty :class:`RubricOverrides`).

5. **Per-doc ``target_length_overrides``** — the per-version override
   map (formerly ``.anvil.json target_length.overrides``) lives on
   the BRIEF document entry. Per-version lookup via
   ``BriefDocument.target_length_overrides.for_version(N)``.

6. **body_filename_for** — the issue #295 slug-echo helper moved from
   ``anvil_config.py`` to ``project_brief.py`` after the #296
   consolidation. The slug → ``<slug>.md`` contract is unchanged.

7. **Worked-example BRIEF.md template round-trips** — the shipped
   ``templates/BRIEF.rubric-overrides.md.example`` parses cleanly
   through ``load_project_brief_strict`` and its per-doc
   ``rubric_overrides`` blocks load via
   ``load_rubric_overrides_for_slug``.

Per the #58 packaging convention, this filename
(``test_brief_rubric_overrides.py``) is unique across the
``anvil/skills/*/tests/`` tree.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import shutil
import sys
import textwrap
import unittest
import warnings
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Any, Dict


# The memo skill keeps its lib modules under its own ``lib/`` per the
# CLAUDE.md "skill-local first, lib promotion later" pattern. Add it to
# ``sys.path`` so tests import without a package install step — mirrors
# ``test_project_brief.py``.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
_TEMPLATES = _HERE.parent / "templates"
sys.path.insert(0, str(_LIB))

from project_brief import (  # noqa: E402
    BriefDocument,
    CalibrationOverride,
    MAX_DIM,
    MIN_DIM,
    ProjectBrief,
    RubricOverrides,
    TargetLengthOverrides,
    TargetLengthRange,
    body_filename_for,
    load_project_brief,
    load_project_brief_strict,
    load_rubric_overrides_for_slug,
)
from project_discovery import BRIEF_FILENAME  # noqa: E402


# The worked-example template the issue #296 consolidation ships.
_WORKED_EXAMPLE = _TEMPLATES / "BRIEF.rubric-overrides.md.example"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _write_brief(directory: Path, frontmatter: str) -> Path:
    """Write ``<directory>/BRIEF.md`` with the given YAML frontmatter body.

    ``frontmatter`` is the raw text inserted between the opening and
    closing ``---`` delimiters. Caller passes already-dedented text.
    """
    directory.mkdir(parents=True, exist_ok=True)
    brief = directory / BRIEF_FILENAME
    brief.write_text(
        f"---\n{frontmatter}\n---\n\n# Project BRIEF\n",
        encoding="utf-8",
    )
    return brief


def _minimal_brief_frontmatter(*, slug: str = "demo", extras: str = "") -> str:
    """Return a minimal valid BRIEF frontmatter with a single document.

    ``extras`` is interpolated into the document entry. The caller
    passes the extras at **column 0** (no indent); this helper applies
    the document-entry indent (4 spaces) to each non-blank line so the
    extras land as siblings of the entry's ``slug:`` and
    ``artifact_type:`` keys.
    """
    base = textwrap.dedent(
        f"""\
        project: demo-project
        audience:
          - demo audience
        hard_rules: []
        documents:
          - slug: {slug}
            artifact_type: investment-memo
        """
    ).rstrip()
    if extras:
        # Indent each line of extras by 4 spaces so the keys land as
        # siblings of `slug:` and `artifact_type:` (each at column 4).
        indented = "\n".join(
            ("    " + line) if line.strip() else ""
            for line in extras.splitlines()
        )
        return base + "\n" + indented
    return base


class _TmpProjectBase(unittest.TestCase):
    """Per-test temp dir for the project root."""

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self.project_dir = Path(self._td.name) / "project"
        self.project_dir.mkdir(parents=True, exist_ok=True)
        self.addCleanup(self._td.cleanup)


# ---------------------------------------------------------------------------
# load_rubric_overrides_for_slug — absence paths return empty
# ---------------------------------------------------------------------------


class TestLoadRubricOverridesEmptyCases(_TmpProjectBase):
    """All absence paths return an empty :class:`RubricOverrides`.

    Mirrors the lenient-form contract of the retired
    ``anvil_config.load_rubric_overrides`` — every absence yields the
    empty instance, no warning, no raise. The reviewer's zero-impact
    contract (AC3 of PR #265) depends on this.
    """

    def test_missing_brief_returns_empty(self) -> None:
        # No BRIEF.md written.
        result = load_rubric_overrides_for_slug(self.project_dir, "any-slug")
        self.assertIsInstance(result, RubricOverrides)
        self.assertTrue(result.is_empty)

    def test_brief_without_matching_slug_returns_empty(self) -> None:
        _write_brief(self.project_dir, _minimal_brief_frontmatter(slug="alpha"))
        result = load_rubric_overrides_for_slug(self.project_dir, "beta")
        self.assertTrue(result.is_empty)

    def test_brief_with_slug_but_no_rubric_overrides_returns_empty(self) -> None:
        _write_brief(self.project_dir, _minimal_brief_frontmatter(slug="alpha"))
        result = load_rubric_overrides_for_slug(self.project_dir, "alpha")
        self.assertTrue(result.is_empty)

    def test_structurally_invalid_brief_returns_empty(self) -> None:
        """A malformed BRIEF degrades to empty rather than raising.

        The reviewer's zero-impact contract requires that a consumer
        typo in BRIEF.md never break the lifecycle. Other entry points
        (the strict loader, the discovery primitive) surface the error
        loudly; this convenience wrapper swallows.
        """
        # Unknown artifact_type — load_project_brief raises ValueError.
        bad = textwrap.dedent(
            """\
            project: bad-project
            documents:
              - slug: x
                artifact_type: not-registered
            """
        ).rstrip()
        _write_brief(self.project_dir, bad)
        result = load_rubric_overrides_for_slug(self.project_dir, "x")
        self.assertTrue(result.is_empty)


# ---------------------------------------------------------------------------
# load_rubric_overrides_for_slug — happy path
# ---------------------------------------------------------------------------


class TestLoadRubricOverridesHappyPath(_TmpProjectBase):
    """Well-formed BRIEF + matching slug + populated overrides parses cleanly."""

    def test_full_synthesis_brief_shape(self) -> None:
        """The brasidas-synthesis canary shape parses identically to PR #267."""
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              memo_subtype: synthesis-brief
              dim_1_calibration: >-
                decision-framework — score on framework clarity + sub-recommendation sharpness, not on single ranked recommendation
              dim_5_calibration: >-
                defers to underlying market models — score on integration quality not on fresh sizing
              dim_6_calibration: >-
                defers to underlying market models — score on whether financial framing supports positioning
              dim_7_calibration: >-
                target length 9000-13000 words; score against declared target
              target_length: { words: [9000, 13000] }
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="brasidas-synthesis", extras=extras),
        )

        result = load_rubric_overrides_for_slug(self.project_dir, "brasidas-synthesis")

        self.assertFalse(result.is_empty)
        self.assertEqual(result.memo_subtype, "synthesis-brief")
        self.assertEqual(len(result.calibrations), 4)
        self.assertEqual([c.dimension for c in result.calibrations], [1, 5, 6, 7])
        self.assertIn("decision-framework", result.calibration_for(1))
        self.assertIsNone(result.calibration_for(2))
        self.assertIsNotNone(result.target_length)
        assert result.target_length is not None  # type narrowing
        self.assertEqual(result.target_length.min_words, 9000)
        self.assertEqual(result.target_length.max_words, 13000)
        self.assertEqual(result.target_length.source_key, "words")

    def test_pages_conversion_to_words(self) -> None:
        """``pages: [N, M]`` converts at 600 words/page (SKILL.md convention)."""
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              memo_subtype: feedback-memo
              target_length: { pages: [3, 4] }
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="feedback", extras=extras),
        )
        result = load_rubric_overrides_for_slug(self.project_dir, "feedback")
        assert result.target_length is not None
        self.assertEqual(result.target_length.min_words, 1800)
        self.assertEqual(result.target_length.max_words, 2400)
        self.assertEqual(result.target_length.source_key, "pages")

    def test_subtype_only(self) -> None:
        """A ``memo_subtype`` with no calibrations and no target_length is valid."""
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              memo_subtype: decision-framework
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        result = load_rubric_overrides_for_slug(self.project_dir, "x")
        self.assertFalse(result.is_empty)
        self.assertEqual(result.memo_subtype, "decision-framework")
        self.assertEqual(result.calibrations, [])
        self.assertIsNone(result.target_length)

    def test_calibration_only(self) -> None:
        """A single ``dim_N_calibration`` is valid without ``memo_subtype``."""
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              dim_7_calibration: longer is OK
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        result = load_rubric_overrides_for_slug(self.project_dir, "x")
        self.assertFalse(result.is_empty)
        self.assertIsNone(result.memo_subtype)
        self.assertEqual(result.calibration_for(7), "longer is OK")

    def test_all_nine_dims_accepted(self) -> None:
        """Every dim from 1 through 9 is in range (memo rubric is /44 with 9 dims)."""
        dim_lines = "\n".join(
            f"  dim_{n}_calibration: dim {n} note"
            for n in range(MIN_DIM, MAX_DIM + 1)
        )
        extras = "rubric_overrides:\n" + dim_lines
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        result = load_rubric_overrides_for_slug(self.project_dir, "x")
        self.assertEqual(len(result.calibrations), MAX_DIM - MIN_DIM + 1)

    def test_calibration_text_preserved_verbatim(self) -> None:
        """Calibration prose must round-trip exactly — no rewording, no trim."""
        # YAML's plain scalar collapses internal multi-space runs, so we
        # use the literal-block scalar (`|-`) to preserve every byte.
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              dim_1_calibration: |-
                a   b   c   <-- triple spaces preserved
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        result = load_rubric_overrides_for_slug(self.project_dir, "x")
        self.assertEqual(
            result.calibration_for(1),
            "a   b   c   <-- triple spaces preserved",
        )


# ---------------------------------------------------------------------------
# Strict validation (BRIEF parser is STRICT on malformed shapes)
# ---------------------------------------------------------------------------


class TestStrictValidation(_TmpProjectBase):
    """Malformed shapes raise ``ValueError`` via the strict loader.

    The BRIEF parser is strict by design (unlike the prior lenient
    ``anvil_config`` loader, which warned + dropped). Per-doc metadata
    is load-bearing for overlay selection in #286; malformed entries
    must fail loudly. The convenience wrapper
    ``load_rubric_overrides_for_slug`` does swallow these errors per
    its zero-impact contract — see TestLoadRubricOverridesEmptyCases.
    """

    def _expect_value_error(self, extras: str, *, expected_substr: str) -> None:
        """Assert the strict loader raises ValueError with the expected substring."""
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        with self.assertRaises(ValueError) as ctx:
            load_project_brief_strict(self.project_dir)
        self.assertIn(expected_substr, str(ctx.exception))

    def test_non_dict_rubric_overrides_raises(self) -> None:
        extras = "rubric_overrides: not-a-dict"
        self._expect_value_error(extras, expected_substr="must be a dict")

    def test_non_string_memo_subtype_raises(self) -> None:
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              memo_subtype: 42
            """
        ).rstrip()
        self._expect_value_error(extras, expected_substr="memo_subtype")

    def test_empty_memo_subtype_raises(self) -> None:
        # Quoted whitespace-only string is what reaches the parser as `"   "`.
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              memo_subtype: "   "
            """
        ).rstrip()
        self._expect_value_error(extras, expected_substr="memo_subtype")

    def test_out_of_range_dim_raises(self) -> None:
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              dim_0_calibration: out of range
            """
        ).rstrip()
        self._expect_value_error(extras, expected_substr="out of range")

    def test_extended_shape_target_length_rejected(self) -> None:
        """The per-version surface is now ``target_length_overrides`` (per-doc), not nested."""
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              target_length:
                default: { words: [1800, 2400] }
            """
        ).rstrip()
        self._expect_value_error(extras, expected_substr="extended-shape")

    def test_target_length_both_words_and_pages_rejected(self) -> None:
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              target_length: { words: [1800, 2400], pages: [3, 4] }
            """
        ).rstrip()
        self._expect_value_error(extras, expected_substr="ambiguous")

    def test_target_length_min_gt_max_rejected(self) -> None:
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              target_length: { words: [3000, 1800] }
            """
        ).rstrip()
        self._expect_value_error(extras, expected_substr="min <= max")


# ---------------------------------------------------------------------------
# Forward-compat unknown keys
# ---------------------------------------------------------------------------


class TestUnknownKeyForwardCompat(_TmpProjectBase):
    """Unknown keys inside ``rubric_overrides`` are preserved + warned."""

    def test_unknown_key_preserved_and_warned(self) -> None:
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              memo_subtype: synthesis-brief
              concision_discipline:
                penalty_per_word: 0.05
              future_knob: TBD
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            brief = load_project_brief_strict(self.project_dir)

        overrides = brief.documents[0].rubric_overrides
        assert overrides is not None
        self.assertEqual(overrides.memo_subtype, "synthesis-brief")
        self.assertEqual(
            overrides.unknown_keys.get("concision_discipline"),
            {"penalty_per_word": 0.05},
        )
        self.assertEqual(overrides.unknown_keys.get("future_knob"), "TBD")

        msgs = [str(w.message) for w in caught]
        self.assertTrue(
            any("concision_discipline" in m for m in msgs),
            f"expected unknown-key warning, got {msgs}",
        )

    def test_unknown_key_only_makes_is_empty_false(self) -> None:
        """A block with only unknown keys still reports ``is_empty == False``."""
        extras = textwrap.dedent(
            """\
            rubric_overrides:
              future_knob: active
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            brief = load_project_brief_strict(self.project_dir)
        overrides = brief.documents[0].rubric_overrides
        assert overrides is not None
        self.assertFalse(overrides.is_empty)


# ---------------------------------------------------------------------------
# target_length_overrides (per-version) on the BRIEF document entry
# ---------------------------------------------------------------------------


class TestTargetLengthOverrides(_TmpProjectBase):
    """Per-doc ``target_length_overrides`` (per-version) parses + resolves."""

    def test_per_version_overrides_parse(self) -> None:
        extras = textwrap.dedent(
            """\
            target_length: { words: [1800, 2400] }
            target_length_overrides:
              "1": { words: [2000, 2600] }
              "2": { words: [1700, 2200] }
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.documents[0]
        self.assertIsNotNone(doc.target_length_overrides)
        assert doc.target_length_overrides is not None
        v1 = doc.target_length_overrides.for_version(1)
        v2 = doc.target_length_overrides.for_version(2)
        v3 = doc.target_length_overrides.for_version(3)
        assert v1 is not None and v2 is not None
        self.assertEqual((v1.min_words, v1.max_words), (2000, 2600))
        self.assertEqual((v2.min_words, v2.max_words), (1700, 2200))
        self.assertIsNone(v3)

    def test_per_version_overrides_accept_pages(self) -> None:
        extras = textwrap.dedent(
            """\
            target_length_overrides:
              "1": { pages: [3, 4] }
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.documents[0]
        assert doc.target_length_overrides is not None
        v1 = doc.target_length_overrides.for_version(1)
        assert v1 is not None
        # 3 pages = 1800 words; 4 pages = 2400 words (at 600 wpp).
        self.assertEqual((v1.min_words, v1.max_words), (1800, 2400))
        self.assertEqual(v1.source_key, "pages")

    def test_non_integer_version_key_rejected(self) -> None:
        extras = textwrap.dedent(
            """\
            target_length_overrides:
              v1: { words: [1800, 2400] }
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        with self.assertRaises(ValueError) as ctx:
            load_project_brief_strict(self.project_dir)
        self.assertIn("positive integer", str(ctx.exception))

    def test_negative_version_key_rejected(self) -> None:
        extras = textwrap.dedent(
            """\
            target_length_overrides:
              "0": { words: [1800, 2400] }
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        with self.assertRaises(ValueError) as ctx:
            load_project_brief_strict(self.project_dir)
        self.assertIn("positive integer", str(ctx.exception))

    def test_empty_overrides_dict_is_valid(self) -> None:
        # YAML `target_length_overrides: {}` produces an empty dict; should be
        # accepted (the resolver simply finds no overrides for any version).
        extras = textwrap.dedent(
            """\
            target_length_overrides: {}
            """
        ).rstrip()
        _write_brief(
            self.project_dir,
            _minimal_brief_frontmatter(slug="x", extras=extras),
        )
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.documents[0]
        assert doc.target_length_overrides is not None
        self.assertEqual(doc.target_length_overrides.overrides, {})
        self.assertIsNone(doc.target_length_overrides.for_version(1))


# ---------------------------------------------------------------------------
# Schema-level constraint coverage
# ---------------------------------------------------------------------------


class TestSchemaConstraints(unittest.TestCase):
    """Direct model-construction tests — guard the public type surface."""

    def test_calibration_override_rejects_out_of_range(self) -> None:
        from pydantic import ValidationError

        with self.assertRaises(ValidationError):
            CalibrationOverride(dimension=0, text="x")
        with self.assertRaises(ValidationError):
            CalibrationOverride(dimension=10, text="x")

    def test_calibration_override_rejects_empty_text(self) -> None:
        from pydantic import ValidationError

        with self.assertRaises(ValidationError):
            CalibrationOverride(dimension=1, text="")

    def test_target_length_range_rejects_negative(self) -> None:
        from pydantic import ValidationError

        with self.assertRaises(ValidationError):
            TargetLengthRange(min_words=-1, max_words=100, source_key="words")

    def test_rubric_overrides_rejects_extra_fields(self) -> None:
        from pydantic import ValidationError

        with self.assertRaises(ValidationError):
            RubricOverrides(memo_subtype="x", undeclared_field=1)  # type: ignore[call-arg]

    def test_target_length_overrides_for_version_returns_none_for_absent(self) -> None:
        overrides = TargetLengthOverrides(overrides={})
        self.assertIsNone(overrides.for_version(1))
        self.assertIsNone(overrides.for_version(99))


# ---------------------------------------------------------------------------
# body_filename_for (issue #295 helper, moved from anvil_config.py)
# ---------------------------------------------------------------------------


class TestBodyFilenameFor(unittest.TestCase):
    """Tests for ``body_filename_for`` — the #295 slug-echo helper."""

    def test_simple_slug_echoes(self) -> None:
        self.assertEqual(body_filename_for("investment-memo"), "investment-memo.md")
        self.assertEqual(body_filename_for("latency-wall"), "latency-wall.md")
        self.assertEqual(body_filename_for("acme-seed"), "acme-seed.md")

    def test_slug_with_underscores_and_digits_echoes(self) -> None:
        self.assertEqual(body_filename_for("q3_thesis_update_2"), "q3_thesis_update_2.md")

    def test_empty_slug_raises(self) -> None:
        with self.assertRaises(ValueError):
            body_filename_for("")

    def test_non_string_slug_raises(self) -> None:
        with self.assertRaises(ValueError):
            body_filename_for(None)  # type: ignore[arg-type]
        with self.assertRaises(ValueError):
            body_filename_for(42)  # type: ignore[arg-type]


# ---------------------------------------------------------------------------
# Worked-example BRIEF template round-trips
# ---------------------------------------------------------------------------


class TestWorkedExampleRoundtrip(_TmpProjectBase):
    """The ``templates/BRIEF.rubric-overrides.md.example`` template parses cleanly.

    The template demonstrates both the synthesis-brief and feedback-memo
    canary shapes from issue #233 (now consolidated under the project-
    level BRIEF per #296). A round-trip check here guards against
    template drift: if someone edits the example with a typo, this
    test fails before the consumer who copies the file does.
    """

    def setUp(self) -> None:
        super().setUp()
        # Copy the worked-example template into a temp project dir under
        # the BRIEF.md filename. The example file's name carries
        # `.example` for operator clarity, but the on-disk filename for
        # production use is BRIEF.md.
        self.assertTrue(_WORKED_EXAMPLE.is_file(), f"missing: {_WORKED_EXAMPLE}")
        shutil.copy(str(_WORKED_EXAMPLE), str(self.project_dir / BRIEF_FILENAME))

    def test_template_strict_parse_succeeds(self) -> None:
        """The template parses through ``load_project_brief_strict`` without raising."""
        try:
            brief = load_project_brief_strict(self.project_dir)
        except ValueError as exc:
            self.fail(f"worked-example BRIEF failed strict validation: {exc}")
        self.assertIsInstance(brief, ProjectBrief)
        self.assertEqual(brief.project, "studio-2026-q2")

    def test_template_carries_synthesis_brief_calibrations(self) -> None:
        result = load_rubric_overrides_for_slug(
            self.project_dir, "brasidas-synthesis"
        )
        self.assertEqual(result.memo_subtype, "synthesis-brief")
        dims = sorted(c.dimension for c in result.calibrations)
        self.assertEqual(dims, [1, 5, 6, 7])
        for entry in result.calibrations:
            self.assertTrue(entry.text.strip(), f"empty calibration on dim {entry.dimension}")

    def test_template_carries_feedback_memo_calibrations(self) -> None:
        result = load_rubric_overrides_for_slug(
            self.project_dir, "raytheon-pitch-strategy"
        )
        self.assertEqual(result.memo_subtype, "feedback-memo")
        dims = sorted(c.dimension for c in result.calibrations)
        self.assertEqual(dims, [1, 4, 5, 6, 7])

    def test_template_carries_synthesis_brief_target_length_overrides(self) -> None:
        """The synthesis-brief entry declares per-version target_length_overrides."""
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.document_for_slug("brasidas-synthesis")
        assert doc is not None
        assert doc.target_length_overrides is not None
        v1 = doc.target_length_overrides.for_version(1)
        v2 = doc.target_length_overrides.for_version(2)
        assert v1 is not None and v2 is not None
        # The template ships v1=[10000,14000], v2=[9000,13000].
        self.assertEqual((v1.min_words, v1.max_words), (10000, 14000))
        self.assertEqual((v2.min_words, v2.max_words), (9000, 13000))

    def test_template_loads_without_warnings(self) -> None:
        """The shipped template should not emit unknown-key warnings."""
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            load_project_brief_strict(self.project_dir)
        # The directory-divergence warning fires only with validate_dirs=True.
        # The unknown-key warning would fire only if the template carries an
        # unrecognized rubric_overrides key. Neither should fire here.
        msgs = [str(w.message) for w in caught]
        self.assertEqual(
            msgs,
            [],
            f"worked-example BRIEF emitted unexpected warnings: {msgs}",
        )

    def test_template_demonstrates_iteration_cap_override(self) -> None:
        """The template demonstrates the issue #349 paired override.

        The `raytheon-pitch-strategy` entry in
        ``templates/BRIEF.rubric-overrides.md.example`` carries both
        ``max_iterations`` and ``iteration_cap_rationale`` so operators
        have a worked example to copy. A regression here would mean the
        template silently lost the demonstration.
        """
        brief = load_project_brief_strict(self.project_dir)
        doc = brief.document_for_slug("raytheon-pitch-strategy")
        assert doc is not None
        # The template ships max_iterations: 5 + non-empty rationale.
        self.assertEqual(doc.max_iterations, 5)
        self.assertIsNotNone(doc.iteration_cap_rationale)
        assert doc.iteration_cap_rationale is not None
        self.assertIn("Operator-extended", doc.iteration_cap_rationale)


if __name__ == "__main__":
    unittest.main()
