"""Tests for ``anvil.skills.memo.lib.memo_image_refs``.

Each test exercises one fixture under ``tests/fixtures/memo_image_refs/``
against ``lint_memo_image_refs``. The fixtures correspond to the seven ACs
on issue #146:

- ``canary_footgun/`` — AC2: body markdown says ``exhibits/fig_a.png`` but
  ``fig_a.png`` is at the version-dir root (the ``cp -r`` footgun shape).
- ``clean_all_present/`` — AC3: every ref resolves; no findings.
- ``urls_and_abs_paths_skipped/`` — AC4: ``http://``, ``https://``, ``/abs``,
  and ``data:`` refs produce zero findings.
- ``suppressed_with_directive/`` — AC5: missing ref with the lint-disable
  directive downgrades to ``info``.
- ``html_img_parity/`` — AC7: ``<img src=...>`` and ``![](...)`` treated
  symmetrically.
- ``missing_file_no_root_match/`` — message-path coverage: a missing ref
  with no same-basename root file omits the cp-r footgun hint.

The remaining ACs (AC1: API shape; AC6: command-doc wiring) are validated
by separate test classes below.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path


# The memo skill keeps the lint module under its own ``lib/`` per the
# CLAUDE.md "skill-local first, lib promotion later" pattern (issue #146).
# Add it to ``sys.path`` here so the tests can import the module without a
# package install step — mirrors ``anvil/skills/deck/tests/test_marp_lint.py``.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from memo_image_refs import (  # noqa: E402
    Finding,
    LintResult,
    RULES,
    lint_memo_image_refs,
    lint_source,
)

_FIXTURES = _HERE / "fixtures" / "memo_image_refs"


class TestCanaryFootgun(unittest.TestCase):
    """AC2 — Bessemer v11 ``cp -r`` footgun.

    The body markdown (``canary_footgun.md`` per the slug-echo convention
    of #295) says ``![x](exhibits/fig_a.png)`` and ``fig_a.png`` is at the
    version-dir root (not inside ``exhibits/``). The lint must emit exactly
    one error with ``rule="memo_image_refs_exist"`` whose message names the
    ref ``exhibits/fig_a.png`` and the resolved path.
    """

    def test_one_error_for_missing_ref(self) -> None:
        result = lint_memo_image_refs(_FIXTURES / "canary_footgun" / "canary_footgun.1")
        self.assertEqual(len(result.errors), 1)
        self.assertEqual(len(result.warnings), 0)
        finding = result.errors[0]
        self.assertEqual(finding.rule, "memo_image_refs_exist")
        self.assertEqual(finding.severity, "error")
        self.assertEqual(finding.ref, "exhibits/fig_a.png")
        # The resolved path should include the missing subdir.
        self.assertIn("exhibits/fig_a.png", finding.resolved_path)

    def test_message_includes_cp_r_footgun_hint(self) -> None:
        """When a same-basename file exists at the version root, the message
        names the cp-r footgun shape so the reviser knows exactly what to fix."""
        result = lint_memo_image_refs(_FIXTURES / "canary_footgun" / "canary_footgun.1")
        msg = result.errors[0].message
        # Message must surface the resolved path AND the version-root hint.
        self.assertIn("exhibits/fig_a.png", msg)
        self.assertIn("cp -r", msg.lower().replace("`", ""))


class TestCleanAllPresent(unittest.TestCase):
    """AC3 — every ref resolves; no findings."""

    def test_no_findings(self) -> None:
        result = lint_memo_image_refs(_FIXTURES / "clean_all_present" / "clean_all_present.1")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(len(result.infos), 0)


class TestUrlsAndAbsPathsSkipped(unittest.TestCase):
    """AC4 — ``http://``, ``https://``, ``/abs``, and ``data:`` refs skipped.

    Even though none of these refs would resolve to a local file, they
    produce ZERO findings because they fall outside the lint's scope.
    """

    def test_no_findings(self) -> None:
        result = lint_memo_image_refs(_FIXTURES / "urls_and_abs_paths_skipped" / "urls_and_abs_paths_skipped.1")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(len(result.infos), 0)


class TestSuppressedWithDirective(unittest.TestCase):
    """AC5 — ``<!-- anvil-lint-disable: memo_image_refs_exist -->`` downgrades
    a missing-image error to ``info``."""

    def test_finding_downgraded_to_info(self) -> None:
        result = lint_memo_image_refs(_FIXTURES / "suppressed_with_directive" / "suppressed_with_directive.1")
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(len(result.infos), 1)
        self.assertEqual(result.infos[0].severity, "info")
        self.assertEqual(result.infos[0].rule, "memo_image_refs_exist")

    def test_review_not_blocked(self) -> None:
        """When the directive suppresses the only finding, ``errors`` is
        empty — which is the signal the review machinery uses to force
        ``advance: false``. With errors empty, advance is not forced false."""
        result = lint_memo_image_refs(_FIXTURES / "suppressed_with_directive" / "suppressed_with_directive.1")
        self.assertEqual(len(result.errors), 0)


class TestHtmlImgParity(unittest.TestCase):
    """AC7 — ``<img src=...>`` and ``![](...)`` treated symmetrically.

    Fixture has one HTML ref (``fig_b.png``, MISSING) and one markdown ref
    (``fig_c.png``, PRESENT). Expected: exactly one error for ``fig_b.png``.
    """

    def test_one_error_for_html_only(self) -> None:
        result = lint_memo_image_refs(_FIXTURES / "html_img_parity" / "html_img_parity.1")
        self.assertEqual(len(result.errors), 1)
        self.assertEqual(result.errors[0].ref, "exhibits/fig_b.png")
        self.assertEqual(result.errors[0].rule, "memo_image_refs_exist")

    def test_single_quoted_html_also_matches(self) -> None:
        """Single-quoted ``src`` attribute should also be detected."""
        memo_dir = _FIXTURES / "html_img_parity" / "html_img_parity.1"
        source = "<img src='exhibits/missing.png'>"
        result = lint_source(source, memo_dir)
        self.assertEqual(len(result.errors), 1)
        self.assertEqual(result.errors[0].ref, "exhibits/missing.png")


class TestMissingFileNoRootMatch(unittest.TestCase):
    """A missing ref with no same-basename file at the version root produces
    a generic ``does not exist`` diagnostic without the cp-r hint."""

    def test_message_omits_cp_r_hint(self) -> None:
        result = lint_memo_image_refs(_FIXTURES / "missing_file_no_root_match" / "missing_file_no_root_match.1")
        self.assertEqual(len(result.errors), 1)
        msg = result.errors[0].message
        # The resolved path is still surfaced.
        self.assertIn("exhibits/fig_x.png", msg)
        # The cp-r footgun hint is NOT in the message when there is no
        # same-basename file at the root.
        self.assertNotIn("cp -r", msg.lower())


class TestLintResultShape(unittest.TestCase):
    """AC1 — ``LintResult`` exposes structured ``Finding``s with the
    documented schema. Field shape is shape-compatible with
    ``marp_lint.Finding`` (rule / severity / message / line) for cross-skill
    consumers."""

    def test_finding_fields(self) -> None:
        result = lint_memo_image_refs(_FIXTURES / "canary_footgun" / "canary_footgun.1")
        finding = result.errors[0]
        self.assertIsInstance(finding, Finding)
        self.assertIsInstance(finding.line, int)
        self.assertIsInstance(finding.rule, str)
        self.assertIsInstance(finding.severity, str)
        self.assertIsInstance(finding.message, str)
        self.assertIsInstance(finding.ref, str)
        self.assertIsInstance(finding.resolved_path, str)
        # ``line`` should be 1-based.
        self.assertGreaterEqual(finding.line, 1)

    def test_to_summary_shape(self) -> None:
        result = lint_memo_image_refs(_FIXTURES / "canary_footgun" / "canary_footgun.1")
        summary = result.to_summary()
        self.assertTrue(summary["ran"])
        self.assertEqual(summary["errors"], 1)
        self.assertEqual(summary["warnings"], 0)
        self.assertIn("errors_by_path", summary)
        self.assertEqual(
            summary["errors_by_path"][0]["rule"], "memo_image_refs_exist"
        )

    def test_rules_advertises_memo_image_refs_exist(self) -> None:
        self.assertIn("memo_image_refs_exist", RULES)

    def test_lint_memo_image_refs_returns_empty_when_body_md_absent(self) -> None:
        """Missing body markdown is not a lint error — return an empty result.
        The orchestrator surfaces source-absence as a discovery error
        separately.

        Body filename echoes the parent directory name per #295; passing
        a directory with no version-dir-shaped parent / no matching
        ``<parent>.md`` body returns an empty result.
        """
        # Pass _FIXTURES itself; the function looks for
        # ``<parent>.md`` (here: ``memo_image_refs/<grandparent>.md``)
        # which does not exist.
        result = lint_memo_image_refs(_FIXTURES)
        self.assertIsInstance(result, LintResult)
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.warnings), 0)
        self.assertEqual(len(result.infos), 0)


class TestSuppressionDirectiveAboveRef(unittest.TestCase):
    """A standalone ``<!-- anvil-lint-disable: memo_image_refs_exist -->``
    on its own line suppresses the next non-blank line. Mirrors marp_lint's
    per-slide directive shape."""

    def test_directive_on_line_above_suppresses(self) -> None:
        memo_dir = _FIXTURES / "html_img_parity" / "html_img_parity.1"  # only exhibits/fig_c.png exists
        source = (
            "# Memo\n"
            "\n"
            "<!-- anvil-lint-disable: memo_image_refs_exist -->\n"
            "![Missing fig](exhibits/missing.png)\n"
        )
        result = lint_source(source, memo_dir)
        self.assertEqual(len(result.errors), 0)
        self.assertEqual(len(result.infos), 1)


class TestMemoReviewDocumentsPreflight(unittest.TestCase):
    """AC6 (doc side) — ``commands/memo-review.md`` documents the pre-flight
    step. Grep for the rule name and the function name so docs drift is
    caught."""

    def test_command_doc_mentions_rule_and_function(self) -> None:
        skill_root = _HERE.parent  # anvil/skills/memo/
        doc_path = skill_root / "commands" / "memo-review.md"
        self.assertTrue(doc_path.is_file(), f"expected {doc_path}")
        text = doc_path.read_text(encoding="utf-8")
        self.assertIn("memo_image_refs_exist", text)
        self.assertIn("lint_memo_image_refs", text)


class TestSkillMdDocumentsLib(unittest.TestCase):
    """AC1 / AC6 (doc side) — ``SKILL.md`` documents the new lib so future
    builders know it exists."""

    def test_skill_md_mentions_lib_module(self) -> None:
        skill_root = _HERE.parent
        text = (skill_root / "SKILL.md").read_text(encoding="utf-8")
        self.assertIn("memo_image_refs", text)


if __name__ == "__main__":
    unittest.main()
