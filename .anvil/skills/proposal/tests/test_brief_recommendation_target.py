"""Tests for proposal ``load_recommendation_target`` (issue #356).

The helper promotes the informal ``recommendation_target`` frontmatter
key on a proposal thread-level ``<thread>/BRIEF.md`` into a typed signal
the reviewer can dispatch on at dim 8 (Open decisions) scoring time. Per
the issue body, the function:

1. Returns the value verbatim for each of the closed-set members
   (``invest`` / ``pass`` / ``conditional`` / ``undecided``).
2. Returns ``None`` for every absence / malformed path (missing BRIEF,
   no frontmatter, malformed YAML, missing key, value not in the
   closed set including typos like ``Undecided`` or ``tbd``).
3. Never raises — lenient by design, mirroring memo's
   ``load_recommendation_target`` contract.

The unique filename (``test_brief_recommendation_target.py``) avoids
collision with other skills' tests per the #58 packaging convention.

Runs under either ``python -m unittest discover anvil/skills/proposal/tests/``
or ``pytest anvil/skills/proposal/tests/``.
"""

from __future__ import annotations

import sys
import textwrap
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


# Mirror the sys.path shim used in memo's test_brief_recommendation_target.py
# so the skill-local lib imports cleanly without a package install step.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from project_brief import (  # noqa: E402
    BRIEF_FILENAME,
    load_recommendation_target,
)


class _TmpThreadBase(unittest.TestCase):
    """Per-test temp dir mimicking a proposal thread root."""

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self.thread_dir = Path(self._td.name) / "gossamer-lan"
        self.thread_dir.mkdir(parents=True, exist_ok=True)
        self.addCleanup(self._td.cleanup)

    def _write_brief(self, body: str) -> Path:
        """Write the given verbatim body to ``<thread>/BRIEF.md`` and return the path."""
        brief = self.thread_dir / BRIEF_FILENAME
        brief.write_text(body, encoding="utf-8")
        return brief


# ---------------------------------------------------------------------------
# Absence paths — every absence/malformed shape returns None, never raises
# ---------------------------------------------------------------------------


class TestLoadRecommendationTargetAbsencePaths(_TmpThreadBase):
    """The lenient contract — None on every absence path, never raises."""

    def test_missing_brief_returns_none(self) -> None:
        # No BRIEF.md written.
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_missing_thread_dir_returns_none(self) -> None:
        # The thread directory itself doesn't exist.
        missing = self.thread_dir / "does-not-exist"
        self.assertIsNone(load_recommendation_target(missing))

    def test_brief_with_no_frontmatter_returns_none(self) -> None:
        # Body-only BRIEF — no `---` delimiters.
        self._write_brief("# Brief title\n\nFreeform prose with no frontmatter.\n")
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_brief_with_unclosed_frontmatter_returns_none(self) -> None:
        # Opening `---` but no closing delimiter — _extract_frontmatter returns None.
        self._write_brief("---\nrecommendation_target: undecided\n# Brief title\n")
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_brief_with_malformed_yaml_returns_none(self) -> None:
        # YAML that fails to parse.
        body = "---\nrecommendation_target: : [bad\n---\n\n# body\n"
        self._write_brief(body)
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_brief_with_frontmatter_but_missing_key_returns_none(self) -> None:
        # Valid frontmatter, no recommendation_target.
        body = textwrap.dedent(
            """\
            ---
            title: "Gossamer LAN"
            customer_kind: external
            ---

            # Brief
            """
        )
        self._write_brief(body)
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_brief_with_frontmatter_as_list_returns_none(self) -> None:
        # Frontmatter that parses to a list, not a dict.
        body = "---\n- item1\n- item2\n---\n\n# Brief\n"
        self._write_brief(body)
        self.assertIsNone(load_recommendation_target(self.thread_dir))


# ---------------------------------------------------------------------------
# Closed-set validation — only the four registered values pass; everything
# else (typos, capitalization variants, types) resolves to None.
# ---------------------------------------------------------------------------


class TestLoadRecommendationTargetClosedSet(_TmpThreadBase):
    """The closed set is the contract — typos / case variants / bad types return None."""

    def test_typo_undecided_capitalized_returns_none(self) -> None:
        self._write_brief("---\nrecommendation_target: Undecided\n---\n\n# Brief\n")
        # Capitalized "Undecided" is not in the closed set → None.
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_typo_tbd_returns_none(self) -> None:
        self._write_brief("---\nrecommendation_target: tbd\n---\n\n# Brief\n")
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_typo_question_mark_returns_none(self) -> None:
        self._write_brief('---\nrecommendation_target: "?"\n---\n\n# Brief\n')
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_typo_maybe_returns_none(self) -> None:
        self._write_brief("---\nrecommendation_target: maybe\n---\n\n# Brief\n")
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_value_as_integer_returns_none(self) -> None:
        # An integer in the slot — coerced/parsed by YAML as int, rejected.
        self._write_brief("---\nrecommendation_target: 42\n---\n\n# Brief\n")
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_value_as_list_returns_none(self) -> None:
        self._write_brief(
            "---\nrecommendation_target: [invest, pass]\n---\n\n# Brief\n"
        )
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_value_as_null_returns_none(self) -> None:
        # Explicit YAML null.
        self._write_brief("---\nrecommendation_target: null\n---\n\n# Brief\n")
        self.assertIsNone(load_recommendation_target(self.thread_dir))

    def test_value_as_bool_returns_none(self) -> None:
        # A boolean True/False in the slot — yaml.safe_load parses as bool.
        self._write_brief("---\nrecommendation_target: true\n---\n\n# Brief\n")
        self.assertIsNone(load_recommendation_target(self.thread_dir))


# ---------------------------------------------------------------------------
# Happy paths — each closed-set value parses verbatim.
# ---------------------------------------------------------------------------


class TestLoadRecommendationTargetHappyPaths(_TmpThreadBase):
    """Each registered value parses verbatim from a well-formed BRIEF."""

    def test_undecided_returns_undecided(self) -> None:
        body = textwrap.dedent(
            """\
            ---
            title: "Gossamer LAN"
            subtitle: "Estate-grade fiber backbone"
            studio: "Spheresemi"
            customer_kind: external
            recommendation_target: undecided
            ---

            # Brief: Gossamer LAN

            Body prose.
            """
        )
        self._write_brief(body)
        self.assertEqual(load_recommendation_target(self.thread_dir), "undecided")

    def test_invest_returns_invest(self) -> None:
        self._write_brief("---\nrecommendation_target: invest\n---\n\n# Brief\n")
        self.assertEqual(load_recommendation_target(self.thread_dir), "invest")

    def test_pass_returns_pass(self) -> None:
        self._write_brief("---\nrecommendation_target: pass\n---\n\n# Brief\n")
        self.assertEqual(load_recommendation_target(self.thread_dir), "pass")

    def test_conditional_returns_conditional(self) -> None:
        self._write_brief("---\nrecommendation_target: conditional\n---\n\n# Brief\n")
        self.assertEqual(load_recommendation_target(self.thread_dir), "conditional")

    def test_quoted_string_value_returns_verbatim(self) -> None:
        # YAML allows quoted values; they normalize to bare strings.
        self._write_brief(
            '---\nrecommendation_target: "undecided"\n---\n\n# Brief\n'
        )
        self.assertEqual(load_recommendation_target(self.thread_dir), "undecided")

    def test_with_other_frontmatter_keys_returns_value(self) -> None:
        # The helper extracts only the one key; surrounding keys are ignored.
        body = textwrap.dedent(
            """\
            ---
            title: "Test Proposal"
            subtitle: "test"
            studio: "Test Studio"
            customer_kind: external
            orientation: portrait
            recommendation_target: invest
            stage: "DESIGN PROPOSAL --- CONCEPT STAGE"
            ---

            # Brief
            """
        )
        self._write_brief(body)
        self.assertEqual(load_recommendation_target(self.thread_dir), "invest")


# ---------------------------------------------------------------------------
# Never raises — defensive contract guards
# ---------------------------------------------------------------------------


class TestLoadRecommendationTargetNeverRaises(_TmpThreadBase):
    """Even on adversarial inputs the helper is contractually lenient — never raises."""

    def test_string_input_is_coerced_to_path(self) -> None:
        # Callers may pass a string by accident; the helper should be lenient.
        result = load_recommendation_target(str(self.thread_dir))  # type: ignore[arg-type]
        # No BRIEF written → None, NOT an exception.
        self.assertIsNone(result)

    def test_none_input_returns_none(self) -> None:
        # An adversarial caller passes None; the helper should not raise.
        result = load_recommendation_target(None)  # type: ignore[arg-type]
        self.assertIsNone(result)

    def test_empty_brief_returns_none(self) -> None:
        # Completely empty file — no frontmatter, no body.
        self._write_brief("")
        self.assertIsNone(load_recommendation_target(self.thread_dir))


# ---------------------------------------------------------------------------
# Template integration — the shipped BRIEF.md.example carries
# recommendation_target: undecided per issue #356.
# ---------------------------------------------------------------------------


class TestTemplateIntegration(unittest.TestCase):
    """The shipped proposal BRIEF template parses through the helper end-to-end."""

    def test_shipped_template_resolves_to_undecided(self) -> None:
        """The shipped BRIEF.md.example demonstrates the undecided default."""
        with TemporaryDirectory() as td:
            thread_dir = Path(td) / "demo-thread"
            thread_dir.mkdir()
            # Copy the shipped template into a thread-shaped layout.
            template = (
                _HERE.parent / "templates" / "BRIEF.md.example"
            )
            assert template.is_file(), (
                "missing template BRIEF.md.example — the integration "
                "test depends on the shipped example"
            )
            (thread_dir / BRIEF_FILENAME).write_text(
                template.read_text(encoding="utf-8"), encoding="utf-8"
            )
            self.assertEqual(
                load_recommendation_target(thread_dir),
                "undecided",
                "the shipped BRIEF.md.example MUST carry "
                "`recommendation_target: undecided` as the documented default "
                "(issue #356)",
            )


if __name__ == "__main__":
    unittest.main()
