"""Structural smoke tests for the ``anvil:datasheet`` skill.

These tests assert **structural properties** of the shipped skill files (files
exist, frontmatter parses, the rubric declares 9 dimensions summing to 44 with
a ≥39 advance threshold and names the 5 critical flags, the template carries
the layout conventions + integrity markers, the class defines the
featurecolumns/provenance macros and the navy signature color). They are
intentionally NOT golden-file tests — the skill is a generative authoring
skill and prose will vary across runs and models.

Runs under either ``pytest anvil/skills/datasheet/tests/`` or
``python -m unittest discover anvil/skills/datasheet/tests/``.

The module filename is deliberately distinct (``test_datasheet_skeleton``) and
the package carries an ``__init__.py`` to avoid the cross-skill pytest
collection collision documented in issue #58.
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

    Uses PyYAML when available; falls back to a minimal ``key: value`` parser
    so the test does not hard-depend on PyYAML being installed.
    """
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return {}
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
        "commands/datasheet.md",
        "commands/datasheet-draft.md",
        "commands/datasheet-review.md",
        "commands/datasheet-audit.md",
        "commands/datasheet-revise.md",
        "commands/datasheet-figures.md",
        "templates/anvil-datasheet.cls",
        "templates/datasheet.tex.j2",
        "templates/BRIEF.md.example",
        "lib/__init__.py",
        "lib/pinmap_check.py",
        "lib/buswidth_check.py",
        "tests/__init__.py",
        "tests/test_datasheet_skeleton.py",
    ]

    def test_manifest_present(self):
        for rel in self.EXPECTED:
            with self.subTest(path=rel):
                self.assertTrue(
                    (_SKILL_ROOT / rel).exists(), f"missing skill file: {rel}"
                )

    def test_audit_command_present(self):
        # A datasheet's numbers are commitments a customer designs against —
        # audit is mandatory (the canary's hand audit caught four wrong
        # numbers that read fine in isolation).
        self.assertTrue(
            (_SKILL_ROOT / "commands" / "datasheet-audit.md").exists(),
            "datasheet-audit.md MUST exist (audit is mandatory for datasheets)",
        )


class TestSkillFrontmatter(unittest.TestCase):
    """SKILL.md frontmatter matches the sibling skills' shape."""

    def test_frontmatter(self):
        fm = _parse_frontmatter(_read("SKILL.md"))
        self.assertEqual(fm.get("name"), "datasheet")
        self.assertEqual(fm.get("domain"), "datasheet")
        self.assertEqual(fm.get("type"), "skill")
        self.assertIn(fm.get("user-invocable"), (False, "false"))

    def test_cross_references_sibling_skills(self):
        # SKILL.md MUST cross-reference the proposal skill (the structural +
        # mandatory-audit sibling) and the customer-facing tier peers.
        text = _read("SKILL.md")
        self.assertIn("anvil:proposal", text)
        self.assertIn("anvil:report", text)
        self.assertIn("anvil:memo", text)

    def test_mandatory_audit_state(self):
        # Both critics required to leave DRAFTED — the proposal-style
        # REVIEWED+AUDITED parallel-critic state.
        self.assertIn("REVIEWED+AUDITED", _read("SKILL.md"))

    def test_sidecar_and_stamping_contracts_referenced(self):
        # Critic writes go through the staged-sidecar primitive and carry the
        # v0.4.0 per-review rubric version stamping fields.
        text = _read("SKILL.md")
        self.assertIn("staged_sidecar", text)
        self.assertIn("anvil-datasheet-v1", text)


class TestCommandFrontmatter(unittest.TestCase):
    """Every command file carries a name/description frontmatter block."""

    COMMANDS = {
        "commands/datasheet.md": "datasheet",
        "commands/datasheet-draft.md": "datasheet-draft",
        "commands/datasheet-review.md": "datasheet-review",
        "commands/datasheet-audit.md": "datasheet-audit",
        "commands/datasheet-revise.md": "datasheet-revise",
        "commands/datasheet-figures.md": "datasheet-figures",
    }

    def test_command_frontmatter(self):
        for rel, expected_name in self.COMMANDS.items():
            with self.subTest(path=rel):
                fm = _parse_frontmatter(_read(rel))
                self.assertEqual(fm.get("name"), expected_name)
                self.assertTrue(
                    fm.get("description"), f"{rel} missing a description"
                )

    def test_critic_commands_stamp_rubric_version(self):
        # Both critic-writing commands stamp rubric_id / rubric_total /
        # advance_threshold per the v0.4.0 contract and write via the
        # staged-sidecar primitive.
        for rel in (
            "commands/datasheet-review.md",
            "commands/datasheet-audit.md",
        ):
            with self.subTest(path=rel):
                text = _read(rel)
                self.assertIn("anvil-datasheet-v1", text)
                self.assertIn('"rubric_total": 44', text)
                self.assertIn('"advance_threshold": 39', text)
                self.assertIn("staged_sidecar", text)

    def test_review_runs_deterministic_preflight(self):
        text = _read("commands/datasheet-review.md")
        self.assertIn("compile_and_gate", text)
        self.assertIn("check_pinmap", text)
        self.assertIn("check_buswidths", text)

    def test_audit_covers_the_canary_failure_modes(self):
        text = _read("commands/datasheet-audit.md")
        # Four-valued verdict schedule for the spec back-check.
        for verdict in ("VERIFIED", "UNVERIFIED", "CONTRADICTED", "NOT-IN-REFS"):
            with self.subTest(verdict=verdict):
                self.assertIn(verdict, text)
        # Mechanical checks + the rev-history gate + SKU coherence.
        self.assertIn("check_pinmap", text)
        self.assertIn("check_buswidths", text)
        self.assertIn("revision-history", text.lower())
        lowered = text.lower()
        self.assertTrue(
            "sku coherence" in lowered or "sku-coherence" in lowered,
            "audit command must document the shared-die SKU-coherence step",
        )


class TestRubric(unittest.TestCase):
    """rubric.md declares 9 dimensions summing to 44, ≥39, and the 5 flags."""

    def setUp(self):
        self.text = _read("rubric.md")

    def test_nine_dimensions_sum_to_forty_four(self):
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

    def test_dim_nine_is_rhetorical_economy(self):
        # The /44 convention: dim 9 is Rhetorical economy at weight 4.
        self.assertTrue(
            re.search(
                r"^\|\s*9\s*\|\s*\*\*Rhetorical economy\*\*\s*\|\s*4\s*\|",
                self.text,
                flags=re.MULTILINE,
            ),
            "dim 9 must be Rhetorical economy at weight 4",
        )

    def test_advance_threshold_is_customer_facing_tier(self):
        # ≥39, NOT the general ≥35 tier — datasheets are customer-facing.
        self.assertTrue(
            re.search(r"(≥\s*39|>=\s*39|\b39/44\b)", self.text),
            "advance threshold of 39 not stated in rubric.md",
        )
        self.assertIsNone(
            re.search(r"advance threshold[^\n]*35", self.text, re.IGNORECASE),
            "rubric must not declare a 35 advance threshold",
        )

    def test_rubric_id_declared(self):
        self.assertIn("anvil-datasheet-v1", self.text)

    def test_five_critical_flags_named(self):
        lowered = self.text.lower()
        self.assertIn("spec contradicts source-of-truth", lowered)
        self.assertTrue(
            "pin-map / bus-width violation" in lowered
            or "pin-map violation" in lowered,
            "pin-map/bus-width flag not named",
        )
        self.assertIn("spec change without revision-history entry", lowered)
        self.assertIn("pre-silicon value presented as measured/final", lowered)
        self.assertIn("shared-die spec divergence", lowered)

    def test_human_verdict_scorecard_kind(self):
        # Critic siblings stay on the legacy human-verdict triple (no lib change).
        self.assertIn("human-verdict", self.text)


class TestClass(unittest.TestCase):
    """anvil-datasheet.cls defines the macros and the navy signature color."""

    def setUp(self):
        self.text = _read("templates/anvil-datasheet.cls")

    def test_environments(self):
        self.assertIn(r"\newtcolorbox{callout}", self.text)
        self.assertIn(r"\newtcolorbox{specbox}", self.text)
        self.assertIn(r"\newenvironment{featurecolumns}", self.text)

    def test_colors(self):
        for color in ("accent", "ink", "bg", "muted", "rule"):
            with self.subTest(color=color):
                self.assertIn(rf"\definecolor{{{color}}}", self.text)
        # the navy signature — ANVIL_NAVY from the shared figure palette
        self.assertIn("1F4E7A", self.text)

    def test_xelatex_fontspec_with_fallback(self):
        self.assertIn(r"\RequirePackage{fontspec}", self.text)
        self.assertIn("Helvetica Neue", self.text)
        self.assertIn(r"\IfFontExistsTF", self.text)
        self.assertIn("xelatex", self.text.lower())

    def test_empty_subtitle_guard_uses_ifdefempty(self):
        # Issue #422: the empty-subtitle guard uses etoolbox's
        # expansion-based \ifdefempty rather than \ifx ... \empty, which is
        # prefix-sensitive (false whenever the operands differ in
        # \long/\protected status) and so silently breaks if the macro
        # initialization ever becomes \long. (No hero guard in this class.)
        self.assertNotIn(r"\ifx\anvil@subtitle\empty", self.text)
        self.assertIn(r"\RequirePackage{etoolbox}", self.text)
        self.assertIn(r"\ifdefempty{\anvil@subtitle}", self.text)

    def test_title_block_macros(self):
        for macro in (
            r"\datasheetpart",
            r"\datasheettitle",
            r"\datasheetsubtitle",
            r"\datasheetcompany",
            r"\datasheetdate",
            r"\datasheetrev",
            r"\datasheetstatus",
        ):
            with self.subTest(macro=macro):
                self.assertIn(macro, self.text)

    def test_provenance_macros(self):
        # The measured-vs-projected provenance surface (rubric dim 4).
        for macro in (
            r"\newcommand{\est}",
            r"\newcommand{\simval}",
            r"\newcommand{\meas}",
            r"\newcommand{\preliminarynotice}",
        ):
            with self.subTest(macro=macro):
                self.assertIn(macro, self.text)

    def test_rev_footer(self):
        # Consistent rev/footer block: part + rev + date in the page footer.
        self.assertIn(r"\fancyfoot", self.text)
        self.assertIn(r"Rev \anvil@rev", self.text)


class TestTemplate(unittest.TestCase):
    """datasheet.tex.j2 carries the layout conventions, the sections, the
    integrity markers, and the provenance/status knobs."""

    def setUp(self):
        self.text = _read("templates/datasheet.tex.j2")

    def test_documentclass(self):
        self.assertIn(r"\documentclass{anvil-datasheet}", self.text)

    def test_two_column_first_page(self):
        self.assertIn(r"\begin{featurecolumns}", self.text)
        self.assertIn("Key Features", self.text)
        self.assertIn("Applications", self.text)

    def test_sections(self):
        required = [
            "General Description",
            "Ordering Information",
            "Functional Description",
            "Absolute Maximum Ratings",
            "Recommended Operating Conditions",
            "Performance Characteristics",
            "Pin Configuration and Functions",
            "Typical Application",
            "Revision History",
        ]
        for marker in required:
            with self.subTest(section=marker):
                self.assertIn(marker, self.text, f"section marker absent: {marker}")

    def test_fresh_page_major_sections(self):
        # Performance Characteristics and Pin Configuration start on a fresh
        # page (canary layout convention 3).
        self.assertGreaterEqual(
            self.text.count(r"\clearpage"),
            2,
            "expected at least two pre-wired \\clearpage fresh-page breaks",
        )

    def test_integrity_markers_pre_wired(self):
        self.assertIn("anvil-pinmap-begin", self.text)
        self.assertIn("anvil-pinmap-end", self.text)
        self.assertIn("anvil-bus:", self.text)

    def test_status_knob(self):
        # status defaults to preliminary and drives the banner + notice.
        self.assertTrue(
            re.search(r'status[^\n]*default\("preliminary"\)', self.text),
            "status must default to preliminary in the template",
        )
        self.assertIn("PRELIMINARY", self.text)
        self.assertIn(r"\preliminarynotice", self.text)

    def test_signature_color_default(self):
        # signature_color falls back to 1F4E7A (navy) when omitted.
        self.assertIn("1F4E7A", self.text)
        self.assertIn("signature_color", self.text)

    def test_provenance_labels_in_spec_tables(self):
        self.assertIn(r"\est{", self.text.replace("\\\\est{", r"\est{"))
        self.assertIn("simval", self.text)


class TestBriefExample(unittest.TestCase):
    """The reference brief parses and carries the skill's knobs."""

    def test_frontmatter(self):
        fm = _parse_frontmatter(_read("templates/BRIEF.md.example"))
        self.assertTrue(fm.get("part_number"))
        self.assertEqual(fm.get("status"), "preliminary")
        self.assertEqual(fm.get("signature_color"), "1F4E7A")
        self.assertTrue(fm.get("family"))


if __name__ == "__main__":
    unittest.main()
