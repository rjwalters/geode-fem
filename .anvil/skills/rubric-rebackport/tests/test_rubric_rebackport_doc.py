"""Doc-coverage tests for `anvil:rubric-rebackport` (issue #358).

Pin the CLI flag set, mode-dispatch matrix, and per-skill stamping
contracts from SKILL.md and commands/rubric-rebackport.md. These tests
fail when the prose drifts from the implementation.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent
sys.path.insert(0, str(_SKILL_ROOT))


COMMAND_FILE = _SKILL_ROOT / "commands" / "rubric-rebackport.md"
SKILL_FILE = _SKILL_ROOT / "SKILL.md"


class TestCommandFlags(unittest.TestCase):
    def setUp(self) -> None:
        self.cmd_text = COMMAND_FILE.read_text(encoding="utf-8")

    def test_documents_apply_flag(self) -> None:
        self.assertIn("--apply", self.cmd_text)

    def test_documents_report_flag(self) -> None:
        self.assertIn("--report", self.cmd_text)

    def test_documents_stamp_only_flag(self) -> None:
        self.assertIn("--stamp-only", self.cmd_text)

    def test_documents_rescore_flag(self) -> None:
        self.assertIn("--rescore", self.cmd_text)

    def test_documents_legacy_rubric_flag(self) -> None:
        self.assertIn("--legacy-rubric", self.cmd_text)

    def test_documents_skill_filter_flag(self) -> None:
        self.assertIn("--skill=", self.cmd_text)

    def test_documents_skill_flag_force_set_semantics(self) -> None:
        """Per #374: `--skill=<X>` is a hybrid filter / force-set.

        The commands doc must describe the force-set-on-None behavior
        so operators understand the post-#374 contract change.
        """
        text = self.cmd_text.lower()
        # Any of these terms is sufficient to convey the force-set
        # semantics; the doc must include at least one.
        force_terms = ("force", "assert", "override")
        self.assertTrue(
            any(term in text for term in force_terms),
            "commands/rubric-rebackport.md must describe `--skill=<X>` "
            "as force-set / operator-asserted (one of "
            f"{force_terms} should appear near the --skill flag docs).",
        )

    def test_documents_skill_flag_prior_release_callout(self) -> None:
        """Per #374: callout for the prior-release filter-only behavior.

        The tool shipped one release ago with filter-only semantics;
        documenting the shift is load-bearing for operators with
        existing scripts.
        """
        text = self.cmd_text.lower()
        self.assertIn("prior-release", text)


class TestModeDispatchMatrixDocumented(unittest.TestCase):
    def setUp(self) -> None:
        self.cmd_text = COMMAND_FILE.read_text(encoding="utf-8")
        self.skill_text = SKILL_FILE.read_text(encoding="utf-8")

    def test_apply_and_report_mutex_documented(self) -> None:
        self.assertIn("mutually exclusive", self.cmd_text)

    def test_stamp_and_rescore_mutex_documented(self) -> None:
        # Either command file or SKILL.md should declare the mutex.
        text = self.cmd_text + "\n" + self.skill_text
        self.assertTrue(
            "stamp-only` and `--rescore` are mutually exclusive" in text
            or "stamp_only and rescore are mutually exclusive" in text
            or "--rescore` are mutually exclusive" in text
        )

    def test_rescore_requires_legacy_rubric_documented(self) -> None:
        text = self.cmd_text + "\n" + self.skill_text
        self.assertIn("--rescore` requires", text)


class TestDryRunContractDocumented(unittest.TestCase):
    def setUp(self) -> None:
        self.cmd_text = COMMAND_FILE.read_text(encoding="utf-8")

    def test_dry_run_is_default_mode(self) -> None:
        self.assertIn("default", self.cmd_text.lower())
        self.assertIn("dry-run", self.cmd_text.lower())

    def test_idempotence_documented(self) -> None:
        self.assertIn("Idempotence", self.cmd_text)


class TestPerSkillStampingValuesDocumented(unittest.TestCase):
    def setUp(self) -> None:
        self.skill_text = SKILL_FILE.read_text(encoding="utf-8")

    def test_memo_v2_44_35_documented(self) -> None:
        self.assertIn("anvil-memo-v2", self.skill_text)
        self.assertIn("35", self.skill_text)
        self.assertIn("44", self.skill_text)

    def test_memo_legacy_40_32_documented(self) -> None:
        self.assertIn("anvil-memo-v1-legacy-40", self.skill_text)
        self.assertIn("32", self.skill_text)

    def test_rescore_sidecar_naming_convention_documented(self) -> None:
        self.assertIn(".review.rescore-", self.skill_text)

    # ---- Post-#357 /44 (and /45 for ip-uspto) row coverage (issue #366) ----

    def test_pub_v2_44_row_documented(self) -> None:
        self.assertIn("anvil-pub-v2", self.skill_text)

    def test_report_v2_44_row_documented(self) -> None:
        self.assertIn("anvil-report-v2", self.skill_text)

    def test_deck_v2_44_row_documented(self) -> None:
        self.assertIn("anvil-deck-v2", self.skill_text)

    def test_slides_v2_44_row_documented(self) -> None:
        self.assertIn("anvil-slides-v2", self.skill_text)

    def test_installation_v2_44_row_documented(self) -> None:
        self.assertIn("anvil-installation-v2", self.skill_text)

    def test_ip_uspto_v2_45_row_documented(self) -> None:
        self.assertIn("anvil-ip-uspto-v2", self.skill_text)
        # ip-uspto is /45, not /44 — assert the value is mentioned
        # somewhere in SKILL.md (suffices to cover the table row).
        self.assertIn("45", self.skill_text)


class TestSkillFrontmatter(unittest.TestCase):
    """The frontmatter on SKILL.md must declare the bridge-tool shape."""

    def setUp(self) -> None:
        self.text = SKILL_FILE.read_text(encoding="utf-8")

    def test_skill_name(self) -> None:
        self.assertIn("name: rubric-rebackport", self.text)

    def test_skill_type_is_bridge_tool(self) -> None:
        self.assertIn("type: bridge-tool", self.text)

    def test_user_invocable_is_true(self) -> None:
        self.assertIn("user-invocable: true", self.text)


if __name__ == "__main__":
    unittest.main()
