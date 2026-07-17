"""Tests for the rubric_overrides reviewer-integration helper (issue #265).

Covers sub-issue 2 of #233 — the reviewer-side suffix attachment that wires
the typed loader at ``anvil/skills/memo/lib/project_brief.py`` (absorbed the
schema from the prior ``anvil_config.py`` under the issue #296
consolidation) into the ``memo-review`` lifecycle.

The four AC scenarios from #265 each have a dedicated test class:

1. **Suffix attached when override present** — ``TestSuffixAttached``: a
   dimension with ``dim_N_calibration`` set carries the verbatim suffix on
   its justification after ``apply_calibration_to_justification`` runs.
2. **Suffix absent when override absent** — ``TestSuffixAbsent``: a
   dimension with no ``dim_N_calibration`` declared returns the input
   justification byte-for-byte unchanged.
3. **Suffix only on dimensions with calibration** — ``TestPerDimDispatch``:
   a ``RubricOverrides`` with ``dim_1`` and ``dim_5`` calibrations attaches
   suffixes ONLY to dims 1 and 5; dims 2, 3, 4, 6, 7, 8, 9 are byte-
   identical to their inputs.
4. **Zero-impact when overrides absent** — ``TestZeroImpactAbsent``: when
   the loader returns ``None`` or an empty ``RubricOverrides`` (no
   ``BRIEF.md``, no per-doc ``rubric_overrides:`` block, no matching
   document entry, or a malformed BRIEF), the helper is a byte-identical
   pass-through across all 9 dimensions.

A fifth class — ``TestVerbatimContract`` — pins the load-bearing audit-
trail contract: the override text is reproduced verbatim with no rewording,
truncation, or whitespace normalization. This is the AC2 "calibration
suffix is verbatim from the override text" contract from #265.

Per the issue #58 cross-skill packaging convention, this file uses a
distinct filename (``test_rubric_overrides_suffix_wiring.py``) so it does
not collide with any other skill's test module under ``pytest`` discovery.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import sys
import textwrap
import unittest
from pathlib import Path


# The memo skill keeps its lib modules under its own ``lib/`` per the
# CLAUDE.md "skill-local first, lib promotion later" pattern. Add it to
# ``sys.path`` so tests import without a package install step — mirrors
# ``test_brief_rubric_overrides.py`` and ``test_memo_image_refs.py``.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from project_brief import (  # noqa: E402
    CalibrationOverride,
    RubricOverrides,
)
from rubric_overrides_suffix import (  # noqa: E402
    CALIBRATION_PREFIX,
    ScoreLike,
    apply_calibration_to_justification,
    apply_calibrations_to_scores,
    format_calibration_suffix,
)


def _overrides_with(**kwargs) -> RubricOverrides:
    """Build a ``RubricOverrides`` from ``dim_N=text`` keyword args.

    Convenience constructor for tests:

        _overrides_with(dim_1="text 1", dim_5="text 5")

    builds an ``RubricOverrides`` with two calibration entries (dims 1, 5).
    Other fields (``memo_subtype``, ``target_length``, ``unknown_keys``)
    default to None / empty per the schema.
    """
    calibrations = []
    for key, text in kwargs.items():
        if not key.startswith("dim_"):
            raise ValueError(f"unrecognized kw {key!r}; use dim_N=text shape")
        dim = int(key.removeprefix("dim_"))
        calibrations.append(CalibrationOverride(dimension=dim, text=text))
    calibrations.sort(key=lambda c: c.dimension)
    return RubricOverrides(calibrations=calibrations)


# ---------------------------------------------------------------------------
# AC 1: Suffix attached when override present
# ---------------------------------------------------------------------------


class TestSuffixAttached(unittest.TestCase):
    """A dimension with ``dim_N_calibration`` set carries the verbatim suffix."""

    def test_suffix_appended_to_existing_justification(self) -> None:
        """The suffix is appended in-line with a single-space separator."""
        overrides = _overrides_with(dim_1="decision-framework — score on framework clarity")
        result = apply_calibration_to_justification(
            "Reviewer's prose for dim 1.", overrides, dimension=1
        )
        self.assertEqual(
            result,
            "Reviewer's prose for dim 1. calibration applied: "
            "decision-framework — score on framework clarity",
        )

    def test_suffix_uses_calibration_prefix_constant(self) -> None:
        """The contract prefix ``calibration applied: `` is exported."""
        # The trailing space in the prefix is load-bearing — downstream
        # consumers grep for the literal anchor. Pin it here.
        self.assertEqual(CALIBRATION_PREFIX, "calibration applied: ")
        overrides = _overrides_with(dim_3="evidence quality calibration")
        result = apply_calibration_to_justification(
            "Dim 3 prose.", overrides, dimension=3
        )
        self.assertIn(CALIBRATION_PREFIX, result)
        self.assertTrue(result.endswith("evidence quality calibration"))

    def test_suffix_alone_when_justification_none(self) -> None:
        """When justification is ``None``, suffix becomes the entire justification.

        Load-bearing path: a reviewer that scored full weight without writing
        justification prose MUST still record the calibration in the audit
        trail.
        """
        overrides = _overrides_with(dim_7="target length 9000-13000 words")
        result = apply_calibration_to_justification(None, overrides, dimension=7)
        self.assertEqual(
            result, "calibration applied: target length 9000-13000 words"
        )

    def test_suffix_alone_when_justification_empty(self) -> None:
        """Empty-string justification is treated the same as ``None``."""
        overrides = _overrides_with(dim_5="defers to market models")
        result = apply_calibration_to_justification("", overrides, dimension=5)
        self.assertEqual(result, "calibration applied: defers to market models")

    def test_format_suffix_is_verbatim(self) -> None:
        """``format_calibration_suffix`` returns ``prefix + text`` verbatim."""
        self.assertEqual(
            format_calibration_suffix("verbatim text — em-dashes and ümlauts"),
            "calibration applied: verbatim text — em-dashes and ümlauts",
        )


# ---------------------------------------------------------------------------
# AC 2 / AC 3: Suffix absent when override absent
# ---------------------------------------------------------------------------


class TestSuffixAbsent(unittest.TestCase):
    """A dimension with no ``dim_N_calibration`` returns input unchanged."""

    def test_no_calibration_for_this_dim_returns_input(self) -> None:
        overrides = _overrides_with(dim_1="dim 1 calibration")
        result = apply_calibration_to_justification(
            "Dim 2 reviewer prose.", overrides, dimension=2
        )
        self.assertEqual(result, "Dim 2 reviewer prose.")

    def test_no_calibration_for_this_dim_preserves_none(self) -> None:
        """Input ``None`` is preserved as ``None`` (not normalized)."""
        overrides = _overrides_with(dim_1="dim 1 calibration")
        result = apply_calibration_to_justification(None, overrides, dimension=2)
        self.assertIsNone(result)

    def test_no_calibration_for_this_dim_preserves_empty_string(self) -> None:
        overrides = _overrides_with(dim_9="dim 9 calibration")
        result = apply_calibration_to_justification("", overrides, dimension=3)
        self.assertEqual(result, "")


# ---------------------------------------------------------------------------
# AC 4: Suffix only on dimensions with calibration set
# ---------------------------------------------------------------------------


class TestPerDimDispatch(unittest.TestCase):
    """Calibrations are applied per-dimension — non-calibrated dims pass through."""

    def test_subset_calibrated_others_passthrough(self) -> None:
        """``dim_1`` and ``dim_5`` calibrated; dims 2/3/4/6/7/8/9 unchanged."""
        overrides = _overrides_with(
            dim_1="dim 1 calibration",
            dim_5="dim 5 calibration",
        )
        # Build per-dimension justifications "Justification for dim N."
        justifications = {n: f"Justification for dim {n}." for n in range(1, 10)}
        results = {
            n: apply_calibration_to_justification(j, overrides, dimension=n)
            for n, j in justifications.items()
        }
        # Dim 1 and 5 carry the suffix:
        self.assertEqual(
            results[1],
            "Justification for dim 1. calibration applied: dim 1 calibration",
        )
        self.assertEqual(
            results[5],
            "Justification for dim 5. calibration applied: dim 5 calibration",
        )
        # Dim 2, 3, 4, 6, 7, 8, 9 are byte-identical to input:
        for n in (2, 3, 4, 6, 7, 8, 9):
            self.assertEqual(
                results[n],
                f"Justification for dim {n}.",
                f"dim {n} should be unchanged (no calibration declared)",
            )

    def test_batch_helper_matches_single_dim_helper(self) -> None:
        """``apply_calibrations_to_scores`` is the batch form of the single helper."""
        overrides = _overrides_with(
            dim_1="dim 1 cal",
            dim_5="dim 5 cal",
            dim_9="dim 9 cal",
        )
        scores = [
            ScoreLike(dimension=n, justification=f"Just for {n}.")
            for n in range(1, 10)
        ]
        result = apply_calibrations_to_scores(scores, overrides)
        by_dim = {s.dimension: s.justification for s in result}
        # Calibrated dims:
        self.assertEqual(
            by_dim[1], "Just for 1. calibration applied: dim 1 cal"
        )
        self.assertEqual(
            by_dim[5], "Just for 5. calibration applied: dim 5 cal"
        )
        self.assertEqual(
            by_dim[9], "Just for 9. calibration applied: dim 9 cal"
        )
        # Non-calibrated dims:
        for n in (2, 3, 4, 6, 7, 8):
            self.assertEqual(by_dim[n], f"Just for {n}.")

    def test_batch_helper_preserves_other_fields(self) -> None:
        """Score / weight / name are carried through unchanged."""
        overrides = _overrides_with(dim_2="dim 2 cal")
        scores = [
            ScoreLike(
                dimension=2,
                justification="dim 2 prose",
                score=5,
                weight=6,
                name="Thesis coherence",
            )
        ]
        result = apply_calibrations_to_scores(scores, overrides)
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0].dimension, 2)
        self.assertEqual(result[0].score, 5)
        self.assertEqual(result[0].weight, 6)
        self.assertEqual(result[0].name, "Thesis coherence")
        self.assertEqual(
            result[0].justification,
            "dim 2 prose calibration applied: dim 2 cal",
        )

    def test_batch_helper_does_not_mutate_input(self) -> None:
        """The input list is preserved byte-for-byte after the batch call."""
        overrides = _overrides_with(dim_1="cal text")
        scores = [
            ScoreLike(dimension=1, justification="orig 1"),
            ScoreLike(dimension=2, justification="orig 2"),
        ]
        _ = apply_calibrations_to_scores(scores, overrides)
        self.assertEqual(scores[0].justification, "orig 1")
        self.assertEqual(scores[1].justification, "orig 2")


# ---------------------------------------------------------------------------
# AC 3 (continued): Zero-impact when overrides absent
# ---------------------------------------------------------------------------


class TestZeroImpactAbsent(unittest.TestCase):
    """When the loader returns ``None`` or empty, every dim is unchanged."""

    def test_none_overrides_returns_input_unchanged(self) -> None:
        """Loader returning ``None`` (defensive) yields a byte-identical pass-through."""
        for dim in range(1, 10):
            result = apply_calibration_to_justification(
                f"Dim {dim} justification.", None, dimension=dim
            )
            self.assertEqual(result, f"Dim {dim} justification.")

    def test_none_overrides_preserves_none_justification(self) -> None:
        result = apply_calibration_to_justification(None, None, dimension=1)
        self.assertIsNone(result)

    def test_empty_rubric_overrides_returns_input_unchanged(self) -> None:
        """An empty ``RubricOverrides`` (loader's no-overrides return) passes through."""
        empty = RubricOverrides()
        self.assertTrue(empty.is_empty)
        for dim in range(1, 10):
            result = apply_calibration_to_justification(
                f"Dim {dim}.", empty, dimension=dim
            )
            self.assertEqual(result, f"Dim {dim}.")

    def test_batch_helper_zero_impact_with_none(self) -> None:
        """``apply_calibrations_to_scores`` with ``None`` overrides is byte-identical."""
        scores = [
            ScoreLike(dimension=n, justification=f"prose {n}")
            for n in range(1, 10)
        ]
        result = apply_calibrations_to_scores(scores, None)
        for orig, new in zip(scores, result):
            self.assertEqual(orig.dimension, new.dimension)
            self.assertEqual(orig.justification, new.justification)

    def test_batch_helper_zero_impact_with_empty_overrides(self) -> None:
        """``apply_calibrations_to_scores`` with empty overrides is byte-identical."""
        scores = [
            ScoreLike(dimension=n, justification=f"prose {n}")
            for n in range(1, 10)
        ]
        result = apply_calibrations_to_scores(scores, RubricOverrides())
        for orig, new in zip(scores, result):
            self.assertEqual(orig.dimension, new.dimension)
            self.assertEqual(orig.justification, new.justification)

    def test_overrides_with_only_subtype_is_zero_impact(self) -> None:
        """A consumer who sets ``memo_subtype`` only (no ``dim_N_calibration``)
        sees zero suffix attachments.

        Load-bearing: ``memo_subtype`` is opaque metadata; it is NOT a
        per-dim calibration trigger by itself. The suffix only fires when
        ``dim_N_calibration`` is explicitly declared.
        """
        overrides = RubricOverrides(memo_subtype="synthesis-brief")
        for dim in range(1, 10):
            result = apply_calibration_to_justification(
                f"Dim {dim}.", overrides, dimension=dim
            )
            self.assertEqual(result, f"Dim {dim}.")

    def test_overrides_with_only_target_length_is_zero_impact(self) -> None:
        """``target_length`` inside ``rubric_overrides`` does NOT trigger a suffix.

        Per #265 issue body, ``target_length`` wiring is the drafter /
        reviser concern (``memo-draft`` / ``memo-revise`` honor it as the
        per-version target). The reviewer's calibration-suffix path is
        scoped to ``dim_N_calibration`` only.
        """
        from project_brief import TargetLengthRange

        overrides = RubricOverrides(
            target_length=TargetLengthRange(
                min_words=9000, max_words=13000, source_key="words"
            )
        )
        for dim in range(1, 10):
            result = apply_calibration_to_justification(
                f"Dim {dim}.", overrides, dimension=dim
            )
            self.assertEqual(result, f"Dim {dim}.")


# ---------------------------------------------------------------------------
# AC 2: Verbatim contract — no rewording, no truncation, no normalization
# ---------------------------------------------------------------------------


class TestVerbatimContract(unittest.TestCase):
    """The override text is reproduced verbatim — the audit trail is load-bearing."""

    def test_internal_whitespace_preserved(self) -> None:
        """Multi-space internal whitespace is not collapsed."""
        text = "double  space  AND  triple   space"
        overrides = _overrides_with(dim_1=text)
        result = apply_calibration_to_justification(
            "dim 1 prose.", overrides, dimension=1
        )
        self.assertIn(text, result)

    def test_em_dashes_and_unicode_preserved(self) -> None:
        text = "decision-framework — sub-recommendations sharp; “portfolio shape” deferred"
        overrides = _overrides_with(dim_1=text)
        result = apply_calibration_to_justification(None, overrides, dimension=1)
        self.assertEqual(result, f"calibration applied: {text}")

    def test_newlines_preserved(self) -> None:
        """Override text containing newlines is reproduced verbatim."""
        text = "line one\nline two"
        overrides = _overrides_with(dim_4="placeholder")
        # Build manually so the empty-prefix check doesn't fire:
        overrides.calibrations[0] = CalibrationOverride(dimension=4, text=text)
        result = apply_calibration_to_justification(
            "dim 4 prose.", overrides, dimension=4
        )
        self.assertIn(text, result)
        self.assertIn("\n", result)

    def test_long_override_not_truncated(self) -> None:
        """Long calibration prose round-trips at full length."""
        text = "x" * 5000
        overrides = _overrides_with(dim_2=text)
        result = apply_calibration_to_justification(
            "dim 2 prose.", overrides, dimension=2
        )
        # The full 5000-char text appears verbatim at the tail.
        self.assertTrue(result.endswith(text))

    def test_override_text_not_lowercased(self) -> None:
        """Case is preserved (the author's wording is the audit trail)."""
        text = "DEFERS to underlying MARKET models"
        overrides = _overrides_with(dim_5=text)
        result = apply_calibration_to_justification(
            "dim 5 prose.", overrides, dimension=5
        )
        self.assertIn(text, result)


# ---------------------------------------------------------------------------
# Integration: loader -> overrides -> suffix pipeline
# ---------------------------------------------------------------------------


class TestLoaderIntegration(unittest.TestCase):
    """The loader + suffix helper compose into the end-to-end reviewer path.

    These tests exercise the full pipeline a reviewer agent runs:

    1. Read ``<project>/BRIEF.md`` via ``load_rubric_overrides_for_slug``.
    2. Apply the returned ``RubricOverrides`` to a list of per-dim scores
       via ``apply_calibrations_to_scores``.

    This is the integration test for the AC contract:
    *"reads ``rubric_overrides`` from the matching ``documents:`` entry
    of the project BRIEF and applies per-dimension calibration prose as
    a suffix to each affected dimension's justification"* — updated from
    PR #265's prior ``.anvil.json`` reader contract by the issue #296
    consolidation.
    """

    def _write_brief(self, project_dir: Path, frontmatter: str) -> None:
        project_dir.mkdir(parents=True, exist_ok=True)
        (project_dir / "BRIEF.md").write_text(
            f"---\n{frontmatter}\n---\n\n# Project BRIEF\n",
            encoding="utf-8",
        )

    def test_full_synthesis_brief_pipeline(self) -> None:
        """The brasidas-synthesis canary shape (issue #233 worked example)
        produces the expected per-dim suffix attachments."""
        from tempfile import TemporaryDirectory

        from project_brief import load_rubric_overrides_for_slug

        with TemporaryDirectory() as td:
            project_dir = Path(td) / "studio-canary"
            frontmatter = textwrap.dedent(
                """\
                project: studio-canary
                audience: [studio]
                hard_rules: []
                documents:
                  - slug: brasidas-synthesis
                    artifact_type: descriptive-thesis
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
                """
            ).rstrip()
            self._write_brief(project_dir, frontmatter)

            overrides = load_rubric_overrides_for_slug(
                project_dir, "brasidas-synthesis"
            )
            self.assertFalse(overrides.is_empty)

            # Build all 9 per-dim scores with placeholder prose.
            scores = [
                ScoreLike(
                    dimension=n,
                    justification=f"Dim {n} reviewer prose for synthesis brief.",
                    score=4,
                    weight=4,
                )
                for n in range(1, 10)
            ]
            result = apply_calibrations_to_scores(scores, overrides)
            by_dim = {s.dimension: s.justification for s in result}

            # Dims 1, 5, 6, 7 carry their suffixes:
            self.assertIn("calibration applied: decision-framework", by_dim[1])
            self.assertIn(
                "calibration applied: defers to underlying market models",
                by_dim[5],
            )
            self.assertIn(
                "calibration applied: defers to underlying market models",
                by_dim[6],
            )
            self.assertIn(
                "calibration applied: target length 9000-13000 words",
                by_dim[7],
            )

            # Dims 2, 3, 4, 8, 9 are unchanged:
            for n in (2, 3, 4, 8, 9):
                self.assertEqual(
                    by_dim[n],
                    f"Dim {n} reviewer prose for synthesis brief.",
                    f"dim {n} should be unchanged",
                )

    def test_project_without_brief_is_zero_impact(self) -> None:
        """A project with no ``BRIEF.md`` produces byte-identical output.

        Load-bearing pre-#233 compat: existing memos that live outside a
        project layout (or under a project root that does not carry a
        BRIEF) see no behavior change after the reviewer is wired to
        load rubric_overrides.
        """
        from tempfile import TemporaryDirectory

        from project_brief import load_rubric_overrides_for_slug

        with TemporaryDirectory() as td:
            project_dir = Path(td) / "legacy-project"
            project_dir.mkdir()
            # No BRIEF.md written.

            overrides = load_rubric_overrides_for_slug(project_dir, "some-slug")
            self.assertTrue(overrides.is_empty)

            scores = [
                ScoreLike(dimension=n, justification=f"dim {n} prose")
                for n in range(1, 10)
            ]
            result = apply_calibrations_to_scores(scores, overrides)
            for orig, new in zip(scores, result):
                self.assertEqual(orig.justification, new.justification)

    def test_project_with_brief_but_no_rubric_overrides_block_is_zero_impact(
        self,
    ) -> None:
        """A project BRIEF with a document entry but no ``rubric_overrides:``
        block sees zero suffix attachments."""
        from tempfile import TemporaryDirectory

        from project_brief import load_rubric_overrides_for_slug

        with TemporaryDirectory() as td:
            project_dir = Path(td) / "uncalibrated-project"
            frontmatter = textwrap.dedent(
                """\
                project: uncalibrated-project
                audience: [me]
                hard_rules: []
                documents:
                  - slug: thread-with-config
                    artifact_type: investment-memo
                    target_length: { words: [1800, 2400] }
                """
            ).rstrip()
            self._write_brief(project_dir, frontmatter)

            overrides = load_rubric_overrides_for_slug(
                project_dir, "thread-with-config"
            )
            self.assertTrue(overrides.is_empty)

            scores = [
                ScoreLike(dimension=n, justification=f"dim {n} prose")
                for n in range(1, 10)
            ]
            result = apply_calibrations_to_scores(scores, overrides)
            for orig, new in zip(scores, result):
                self.assertEqual(orig.justification, new.justification)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
