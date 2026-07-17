"""Structural smoke tests for the ``anvil:installation`` skill.

These tests assert **structural properties** of the shipped skill files (files
exist, frontmatter parses, the rubric declares 8 dimensions summing to 40 and
names the 3 critical flags, the template carries all 11 sections and the
participatory gate, the class defines the callout/metricbox boxes and the
signature colors). They are intentionally NOT golden-file tests — the skill is
a generative authoring skill and prose will vary across runs and models. See
``examples/expected-thread.1/README.md`` for the structural-not-golden stance.

Runs under either ``pytest anvil/skills/installation/tests/`` or
``python -m unittest discover anvil/skills/installation/tests/``.

The module filename is deliberately distinct (``test_installation_skeleton``)
and the package carries an ``__init__.py`` to avoid the cross-skill pytest
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
    # contain ``---`` (e.g. ``stage: "ART INSTALLATION --- CONCEPT STAGE"``).
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
        "commands/installation.md",
        "commands/installation-draft.md",
        "commands/installation-review.md",
        "commands/installation-revise.md",
        "commands/installation-figures.md",
        "templates/anvil-installation.cls",
        "templates/installation.tex.j2",
        "templates/figures/.gitkeep",
        "templates/BRIEF.md.example",
        "assets/example-brief.md",
        "assets/figure-conventions.md",
        "examples/expected-thread.1/README.md",
        "tests/__init__.py",
        "tests/test_installation_skeleton.py",
    ]

    def test_manifest_present(self):
        for rel in self.EXPECTED:
            with self.subTest(path=rel):
                self.assertTrue(
                    (_SKILL_ROOT / rel).exists(), f"missing skill file: {rel}"
                )

    def test_no_audit_command(self):
        # v0 defers the audit phase (following anvil:memo); no -audit command.
        self.assertFalse(
            (_SKILL_ROOT / "commands" / "installation-audit.md").exists(),
            "installation-audit.md must NOT exist in v0 (audit deferred per memo)",
        )


class TestSkillFrontmatter(unittest.TestCase):
    """SKILL.md frontmatter matches the sibling skills' shape."""

    def test_frontmatter(self):
        fm = _parse_frontmatter(_read("SKILL.md"))
        self.assertEqual(fm.get("name"), "installation")
        self.assertEqual(fm.get("domain"), "installation")
        self.assertEqual(fm.get("type"), "skill")
        # user-invocable may parse as a bool (yaml) or the string "false".
        self.assertIn(fm.get("user-invocable"), (False, "false"))


class TestCommandFrontmatter(unittest.TestCase):
    """Every command file carries a name/description frontmatter block."""

    COMMANDS = {
        "commands/installation.md": "installation",
        "commands/installation-draft.md": "installation-draft",
        "commands/installation-review.md": "installation-review",
        "commands/installation-revise.md": "installation-revise",
        "commands/installation-figures.md": "installation-figures",
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
    """rubric.md declares exactly 9 dimensions summing to 44 + the 3 flags.

    Post-#357 the installation rubric migrated from /40 (8 dims, ≥32) to
    /44 (9 dims, ≥35) with dim 9 *Rhetorical economy* at weight 4.
    """

    def setUp(self):
        self.text = _read("rubric.md")

    def test_nine_dimensions_sum_to_forty_four(self):
        # Dimension rows look like: | 1 | **Conceptual coherence** | 6 | ... |
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

    def test_three_critical_flags_named(self):
        lowered = self.text.lower()
        self.assertIn("unbuildable as specified", lowered)
        self.assertTrue(
            "safety / consent hazard" in lowered
            or "safety/consent hazard" in lowered,
            "safety/consent hazard flag not named",
        )
        self.assertTrue(
            "concept incoherent" in lowered or "premise not legible" in lowered,
            "concept-incoherent flag not named",
        )

    def test_human_verdict_scorecard_kind(self):
        # Critic siblings stay on the legacy human-verdict triple (no lib change).
        self.assertIn("human-verdict", self.text)


class TestClass(unittest.TestCase):
    """anvil-installation.cls defines the boxes and signature colors."""

    def setUp(self):
        self.text = _read("templates/anvil-installation.cls")

    def test_environments(self):
        self.assertIn(r"\newtcolorbox{callout}", self.text)
        self.assertIn(r"\newtcolorbox{metricbox}", self.text)

    def test_colors(self):
        for color in ("accent", "ink", "bg", "muted", "rule"):
            with self.subTest(color=color):
                self.assertIn(rf"\definecolor{{{color}}}", self.text)
        # the signature amber from the Quiet Place preamble
        self.assertIn("B45309", self.text)

    def test_xelatex_fontspec_with_fallback(self):
        self.assertIn(r"\RequirePackage{fontspec}", self.text)
        self.assertIn("Helvetica Neue", self.text)
        # documented fallback so the class compiles without system fonts
        self.assertIn(r"\IfFontExistsTF", self.text)
        self.assertIn("xelatex", self.text.lower())

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
    """installation.tex.j2 carries all 11 sections, the Premise callout, and
    the participatory gate on sections 6/7/8."""

    def setUp(self):
        self.text = _read("templates/installation.tex.j2")

    def test_documentclass(self):
        self.assertIn(r"\documentclass{anvil-installation}", self.text)

    def test_premise_callout(self):
        self.assertIn(r"\begin{callout}", self.text)
        self.assertIn("Premise", self.text)

    def test_eleven_sections(self):
        # Section 1 (Premise) is a callout; sections 2-11 are \section headings
        # (some titles are templated via Jinja defaults, so match on the marker
        # text the defaults carry).
        required = [
            "Premise",
            "The Frame",
            "Visitor's Hour",
            "Architecture",
            "Language",  # The Light / Sensory Language (section title default)
            "Ritual Act",
            "Consent Structure",
            "Safety Without Surveillance",
            r"References \& Lineage",
            r"Budget \& Operations",
            "Open Decisions",
        ]
        for marker in required:
            with self.subTest(section=marker):
                self.assertIn(marker, self.text, f"section marker absent: {marker}")

    def test_participatory_gate(self):
        # The Ritual Act / Consent / Safety sections are gated on participatory.
        self.assertIn("participatory", self.text)
        self.assertTrue(
            re.search(r"{%\s*if participatory", self.text),
            "no {% if participatory %} gate in the template",
        )

    def test_signature_color_default(self):
        # signature_color falls back to B45309 when the brief omits it.
        self.assertIn("B45309", self.text)
        self.assertIn("signature_color", self.text)


class TestExampleBrief(unittest.TestCase):
    """The Quiet Place grounding brief parses and is participatory."""

    def test_frontmatter(self):
        fm = _parse_frontmatter(_read("assets/example-brief.md"))
        self.assertEqual(fm.get("title"), "Quiet Place")
        self.assertIn(fm.get("participatory"), (True, "true"))
        self.assertEqual(fm.get("signature_color"), "B45309")

    def test_does_not_vendor_full_studio_tex(self):
        # The trimmed grounding brief must not be the full 32KB studio .tex.
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
