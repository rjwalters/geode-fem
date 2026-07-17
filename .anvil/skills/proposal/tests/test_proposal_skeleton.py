"""Structural smoke tests for the ``anvil:proposal`` skill.

These tests assert **structural properties** of the shipped skill files (files
exist, frontmatter parses, the rubric declares 9 dimensions summing to 44 and
names the 4 critical flags, the template carries all 10 sections, the three
priced tables, and the customer_kind knob, the class defines the
callout/metricbox boxes and the steel-blue signature color). They are
intentionally NOT golden-file tests — the skill is a generative authoring skill
and prose will vary across runs and models. See
``examples/expected-thread.1/README.md`` for the structural-not-golden stance.

Runs under either ``pytest anvil/skills/proposal/tests/`` or
``python -m unittest discover anvil/skills/proposal/tests/``.

The module filename is deliberately distinct (``test_proposal_skeleton``) and
the package carries an ``__init__.py`` to avoid the cross-skill pytest
collection collision documented in issue #58 (two skills each shipping a
``test_marp_lint.py`` under a non-package ``tests/`` dir).
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path


_SKILL_ROOT = Path(__file__).resolve().parent.parent


def _read(rel: str) -> str:
    return (_SKILL_ROOT / rel).read_text(encoding="utf-8")


def _parse_frontmatter(text: str) -> dict:
    """Parse a leading ``---``-delimited YAML frontmatter block.

    Uses PyYAML when available; falls back to a minimal ``key: value`` parser so
    the test does not hard-depend on PyYAML being installed.
    """
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return {}
    # Find the closing fence: the next line that is exactly ``---``. Splitting on
    # a bare ``---`` substring is wrong because frontmatter values may legitimately
    # contain ``---`` (e.g. ``stage: "DESIGN PROPOSAL --- CONCEPT STAGE"``).
    end = None
    for i in range(1, len(lines)):
        if lines[i].strip() == "---":
            end = i
            break
    if end is None:
        return {}
    block = "\n".join(lines[1:end])
    try:
        import yaml  # type: ignore

        data = yaml.safe_load(block)
        return data if isinstance(data, dict) else {}
    except Exception:
        result: dict = {}
        for line in block.splitlines():
            line = line.strip()
            if not line or line.startswith("#") or ":" not in line:
                continue
            key, _, value = line.partition(":")
            result[key.strip()] = value.strip().strip('"').strip("'")
        return result


class TestFilesExist(unittest.TestCase):
    """The pinned file manifest is present on disk."""

    EXPECTED = [
        "SKILL.md",
        "rubric.md",
        "README.md",
        "commands/proposal.md",
        "commands/proposal-draft.md",
        "commands/proposal-review.md",
        "commands/proposal-audit.md",
        "commands/proposal-revise.md",
        "commands/proposal-figures.md",
        "templates/anvil-proposal.cls",
        "templates/proposal.tex.j2",
        "templates/figures/.gitkeep",
        "templates/BRIEF.md.example",
        "assets/example-brief.md",
        "assets/figure-conventions.md",
        "examples/expected-thread.1/README.md",
        "tests/__init__.py",
        "tests/test_proposal_skeleton.py",
    ]

    def test_manifest_present(self):
        for rel in self.EXPECTED:
            with self.subTest(path=rel):
                self.assertTrue(
                    (_SKILL_ROOT / rel).exists(), f"missing skill file: {rel}"
                )

    def test_audit_command_present(self):
        # The substantive divergence from anvil:installation (which deferred
        # audit). A proposal makes priced/sourceable cost claims and link-budget
        # claims — textbook tool_evidence territory — so audit is mandatory.
        # This is the inverse of installation's test_no_audit_command.
        self.assertTrue(
            (_SKILL_ROOT / "commands" / "proposal-audit.md").exists(),
            "proposal-audit.md MUST exist (audit is mandatory for proposals)",
        )


class TestSkillFrontmatter(unittest.TestCase):
    """SKILL.md frontmatter matches the sibling skills' shape."""

    def test_frontmatter(self):
        fm = _parse_frontmatter(_read("SKILL.md"))
        self.assertEqual(fm.get("name"), "proposal")
        self.assertEqual(fm.get("domain"), "proposal")
        self.assertEqual(fm.get("type"), "skill")
        # user-invocable may parse as a bool (yaml) or the string "false".
        self.assertIn(fm.get("user-invocable"), (False, "false"))

    def test_cross_references_report_bookend(self):
        # SKILL.md MUST cross-reference anvil:report as the post-commitment
        # bookend (proposal -> commitment -> report).
        text = _read("SKILL.md")
        self.assertIn("anvil:report", text)
        self.assertIn("anvil:installation", text)
        self.assertIn("anvil:memo", text)


class TestCommandFrontmatter(unittest.TestCase):
    """Every command file carries a name/description frontmatter block."""

    COMMANDS = {
        "commands/proposal.md": "proposal",
        "commands/proposal-draft.md": "proposal-draft",
        "commands/proposal-review.md": "proposal-review",
        "commands/proposal-audit.md": "proposal-audit",
        "commands/proposal-revise.md": "proposal-revise",
        "commands/proposal-figures.md": "proposal-figures",
    }

    def test_command_frontmatter(self):
        for rel, expected_name in self.COMMANDS.items():
            with self.subTest(path=rel):
                fm = _parse_frontmatter(_read(rel))
                self.assertEqual(fm.get("name"), expected_name)
                self.assertTrue(
                    fm.get("description"), f"{rel} missing a description"
                )


class TestRubric(unittest.TestCase):
    """rubric.md declares exactly 9 dimensions summing to 44 + the 4 flags."""

    def setUp(self):
        self.text = _read("rubric.md")

    def test_nine_dimensions_sum_to_forty_four(self):
        # Dimension rows look like: | 1 | **Intent / requirements clarity** | 5 | ... |
        # After issue #244, dim 9 *Rhetorical economy* (weight 4) joins the
        # legacy 8 dims, bringing the total to 44 (≥35 advance threshold).
        rows = re.findall(
            r"^\|\s*([1-9])\s*\|\s*\*\*[^|]+\*\*\s*\|\s*(\d+)\s*\|",
            self.text,
            flags=re.MULTILINE,
        )
        self.assertEqual(
            len(rows), 9, f"expected 9 dimension rows, found {len(rows)}"
        )
        indices = sorted(int(i) for i, _ in rows)
        self.assertEqual(indices, [1, 2, 3, 4, 5, 6, 7, 8, 9])
        total = sum(int(w) for _, w in rows)
        self.assertEqual(total, 44, f"dimension weights sum to {total}, not 44")

    def test_advance_threshold_present(self):
        self.assertTrue(
            re.search(r"(≥\s*35|>=\s*35|\b35/44\b)", self.text),
            "advance threshold of 35 not stated in rubric.md",
        )

    def test_four_critical_flags_named(self):
        lowered = self.text.lower()
        self.assertIn("misses a stated hard constraint", lowered)
        self.assertTrue(
            "not credible" in lowered or "not sourceable" in lowered,
            "cost-not-credible/sourceable flag not named",
        )
        self.assertIn("not deliverable as resourced", lowered)
        self.assertIn("internal inconsistency", lowered)

    def test_human_verdict_scorecard_kind(self):
        # Critic siblings stay on the legacy human-verdict triple (no lib change).
        self.assertIn("human-verdict", self.text)


class TestClass(unittest.TestCase):
    """anvil-proposal.cls defines the boxes and the steel-blue signature color."""

    def setUp(self):
        self.text = _read("templates/anvil-proposal.cls")

    def test_environments(self):
        self.assertIn(r"\newtcolorbox{callout}", self.text)
        self.assertIn(r"\newtcolorbox{metricbox}", self.text)

    def test_colors(self):
        for color in ("accent", "ink", "bg", "muted", "rule"):
            with self.subTest(color=color):
                self.assertIn(rf"\definecolor{{{color}}}", self.text)
        # the steel-blue signature from the Gossamer LAN preamble
        self.assertIn("4A6FA5", self.text)

    def test_xelatex_fontspec_with_fallback(self):
        self.assertIn(r"\RequirePackage{fontspec}", self.text)
        self.assertIn("Helvetica Neue", self.text)
        # documented fallback so the class compiles without system fonts
        self.assertIn(r"\IfFontExistsTF", self.text)
        self.assertIn("xelatex", self.text.lower())

    def test_renamed_title_block_macros(self):
        # The title-block macros are renamed for this skill (not installation's).
        for macro in (
            r"\proposaltitle",
            r"\proposalsubtitle",
            r"\proposalstudio",
            r"\proposaldate",
            r"\proposalstage",
        ):
            with self.subTest(macro=macro):
                self.assertIn(macro, self.text)

    def test_landscape_class_option(self):
        # Issue #247: the class accepts a `landscape` declared option and the
        # geometry block honors it via \ifanvil@landscape. Default is portrait
        # (false), preserving the Gossamer LAN worked example unchanged.
        self.assertIn(r"\DeclareOption{landscape}", self.text)
        self.assertIn(r"\ifanvil@landscape", self.text)
        self.assertIn(r"\anvil@landscapefalse", self.text)

    def test_empty_guards_use_ifdefempty(self):
        # Issue #422: the empty-subtitle/hero guards use etoolbox's
        # expansion-based \ifdefempty rather than \ifx ... \empty, which is
        # prefix-sensitive (false whenever the operands differ in
        # \long/\protected status) and so silently breaks if the macro
        # initialization ever becomes \long.
        self.assertNotIn(r"\ifx\anvil@subtitle\empty", self.text)
        self.assertNotIn(r"\ifx\anvil@hero\empty", self.text)
        self.assertIn(r"\RequirePackage{etoolbox}", self.text)
        self.assertIn(r"\ifdefempty{\anvil@subtitle}", self.text)
        self.assertIn(r"\ifdefempty{\anvil@hero}", self.text)


class TestTemplate(unittest.TestCase):
    """proposal.tex.j2 carries all 10 sections, the Premise callout, the three
    priced tables, the customer_kind knob, and the steel-blue default."""

    def setUp(self):
        self.text = _read("templates/proposal.tex.j2")

    def test_documentclass(self):
        # The template now emits the class option from the brief's `orientation`
        # frontmatter (default portrait → empty option set). Match the line
        # structurally rather than pinning the exact `\documentclass{...}` form.
        self.assertTrue(
            re.search(r"\\documentclass\[[^\]]*\]\{anvil-proposal\}", self.text),
            "template must reference the anvil-proposal class with a (possibly "
            "empty) option set",
        )

    def test_premise_callout(self):
        self.assertIn(r"\begin{callout}", self.text)
        self.assertIn("Premise", self.text)

    def test_ten_sections(self):
        # Section 1 (Premise) is a callout; sections 2-10 are \section headings
        # (some titles are templated via Jinja defaults, so match on the marker
        # text the defaults carry).
        required = [
            "Premise",
            "The Idea",
            "Topology",
            "Core Subsystem",  # generalized core-subsystem title default
            "Interfaces",
            "Coverage",
            "Bill of Materials",
            "Installation",  # Installation / Operating Notes
            r"References \& Compliance",
            "Open Decisions",
        ]
        for marker in required:
            with self.subTest(section=marker):
                self.assertIn(marker, self.text, f"section marker absent: {marker}")

    def test_priced_tables_pre_wired(self):
        # The three priced tables are the heart of a proposal: a multi-section
        # BOM (Materials subtotal), a labor estimate (Labor subtotal), and a
        # project total (Total project cost).
        self.assertIn("Materials subtotal", self.text)
        self.assertIn("Labor subtotal", self.text)
        self.assertIn("Total project cost", self.text)
        # the multi-section BOM uses \multicolumn section headers + booktabs rules
        self.assertIn(r"\multicolumn{4}{@{}l}", self.text)
        for rule in (r"\toprule", r"\midrule", r"\bottomrule"):
            with self.subTest(rule=rule):
                self.assertIn(rule, self.text)

    def test_customer_kind_knob(self):
        # A single optional frontmatter key drives the stage default and the
        # reviewer's reading of dim 7. It must default to external.
        self.assertIn("customer_kind", self.text)
        self.assertIn("INTERNAL BUILD SPEC", self.text)
        self.assertTrue(
            re.search(r'customer_kind[^\n]*default\("external"\)', self.text),
            "customer_kind must default to external in the template",
        )

    def test_orientation_knob(self):
        # The `orientation: portrait | landscape` frontmatter key (issue #247)
        # mirrors the customer_kind precedent: optional key, default portrait,
        # propagates into the \documentclass[...] option set so the class file's
        # geometry block switches to landscape letter when set.
        self.assertIn("orientation", self.text)
        self.assertTrue(
            re.search(r'orientation[^\n]*default\("portrait"\)', self.text),
            "orientation must default to portrait in the template",
        )
        # The landscape branch must emit the `landscape` class option.
        self.assertIn("landscape", self.text)

    def test_signature_color_default(self):
        # signature_color falls back to 4A6FA5 (steel blue) when omitted.
        self.assertIn("4A6FA5", self.text)
        self.assertIn("signature_color", self.text)


class TestExampleBrief(unittest.TestCase):
    """The Gossamer LAN grounding brief parses and is external/steel-blue."""

    def test_frontmatter(self):
        fm = _parse_frontmatter(_read("assets/example-brief.md"))
        self.assertEqual(fm.get("title"), "Gossamer LAN")
        self.assertEqual(fm.get("signature_color"), "4A6FA5")
        self.assertEqual(fm.get("customer_kind"), "external")

    def test_does_not_vendor_full_studio_tex(self):
        # The trimmed grounding brief must not be the full studio .tex.
        brief = _read("assets/example-brief.md")
        self.assertLess(
            len(brief), 20000, "example-brief.md looks like a vendored full .tex"
        )


class TestExampleReadme(unittest.TestCase):
    """The examples README states the structural-not-golden contract."""

    def test_structural_not_golden(self):
        text = _read("examples/expected-thread.1/README.md").lower()
        self.assertIn("structural", text)
        self.assertTrue(
            "not a strict golden file" in text or "not a golden file" in text,
            "examples README must state it is not a golden file",
        )


if __name__ == "__main__":
    unittest.main()
