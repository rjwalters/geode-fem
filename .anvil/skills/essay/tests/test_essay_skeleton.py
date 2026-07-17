"""Structural smoke tests for the ``anvil:essay`` skill (issue #460).

These tests assert **structural properties** of the shipped skill files
(files exist, frontmatter parses, the rubric declares 9 dimensions
summing to 44 with a >=35 advance threshold under the ``anvil-essay-v1``
id, voice fidelity is the OWNED dim 2, the seven ported critical flags
are named, the review command wires the blocking numeric gate + the
promoted hyperlink resolver + the advisory rhetoric lint, the #346
stamping fields and staged-sidecar primitive appear in the critic-writing
command, the publish-handoff contract is documented, and the deferred
Phase-2+ commands are absent). They are intentionally NOT golden-file
tests — the skill is a generative authoring skill and prose varies
across runs and models.

Runs under either ``pytest anvil/skills/essay/tests/`` or
``python -m unittest discover anvil/skills/essay/tests/``.

The module filename is deliberately distinct (``test_essay_skeleton``)
and the package carries an ``__init__.py`` to avoid the cross-skill
pytest collection collision documented in issue #58.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

_SKILL_ROOT = Path(__file__).resolve().parent.parent
_REPO_ROOT = _SKILL_ROOT.parents[2]

RUBRIC_ID = "anvil-essay-v1"


def _read(rel: str) -> str:
    return (_SKILL_ROOT / rel).read_text(encoding="utf-8")


def _parse_frontmatter(text: str) -> dict:
    """Parse a leading ``---``-delimited YAML frontmatter block.

    Uses PyYAML when available; falls back to a minimal ``key: value``
    parser so the test does not hard-depend on PyYAML being installed.
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
    """The pinned file manifest is present on disk (v1 scope)."""

    EXPECTED = [
        "SKILL.md",
        "rubric.md",
        "README.md",
        "__init__.py",
        "commands/essay.md",
        "commands/essay-draft.md",
        "commands/essay-review.md",
        "commands/essay-revise.md",
        "tests/__init__.py",
        "tests/test_essay_skeleton.py",
    ]

    def test_manifest_present(self):
        for rel in self.EXPECTED:
            with self.subTest(path=rel):
                self.assertTrue(
                    (_SKILL_ROOT / rel).exists(), f"missing skill file: {rel}"
                )

    def test_deferred_commands_absent(self):
        # v1 deliberately ships draft/review/revise/status ONLY (issue
        # #460 curation): no figures, no audit, no publish, no render.
        # Their accidental presence here would mean scope creep landed
        # un-reviewed.
        for stem in (
            "essay-audit",
            "essay-figures",
            "essay-publish",
            "essay-render",
            "essay-migrate",
        ):
            with self.subTest(command=stem):
                self.assertFalse(
                    (_SKILL_ROOT / "commands" / f"{stem}.md").exists(),
                    f"{stem}.md is deferred scope (issue #460 curation)",
                )


class TestSkillFrontmatter(unittest.TestCase):
    """SKILL.md frontmatter matches the sibling skills' shape."""

    def test_frontmatter(self):
        fm = _parse_frontmatter(_read("SKILL.md"))
        self.assertEqual(fm.get("name"), "essay")
        self.assertEqual(fm.get("domain"), "essay")
        self.assertEqual(fm.get("type"), "skill")
        self.assertIn(fm.get("user-invocable"), (False, "false"))

    def test_ready_terminal_with_publish_handoff(self):
        text = _read("SKILL.md")
        self.assertIn("READY", text)
        self.assertIn("Publish handoff contract", text)
        # No AUDITED state in this skill's machine.
        self.assertIn("no `AUDITED` state", text)

    def test_sidecar_stamping_and_scorecard_contracts_referenced(self):
        text = _read("SKILL.md")
        self.assertIn("staged_sidecar", text)
        self.assertIn(RUBRIC_ID, text)
        self.assertIn("human-verdict", text)

    def test_voice_grounding_owned_dim(self):
        text = _read("SKILL.md")
        self.assertIn("voice_grounding.md", text)
        # Essay OWNS voice as dim 2 (the #461 'attach to an owned
        # dimension' shape) — unlike memo's dim-8 calibration suffix.
        self.assertIn("OWNS voice fidelity as rubric dim 2", text)
        self.assertIn("resolve_voice_docs", text)

    def test_markdown_only_body_and_slug_echo(self):
        text = _read("SKILL.md")
        self.assertIn("no PDF render path", text)
        self.assertIn("NOT the surveyed consumer's `post.md`", text)

    def test_failure_mode_catalog(self):
        text = _read("SKILL.md").lower()
        self.assertIn("toaster", text)
        self.assertIn("spread failure", text)


class TestCommandFrontmatter(unittest.TestCase):
    """Every command file carries a name/description frontmatter block."""

    COMMANDS = {
        "commands/essay.md": "essay",
        "commands/essay-draft.md": "essay-draft",
        "commands/essay-review.md": "essay-review",
        "commands/essay-revise.md": "essay-revise",
    }

    def test_command_frontmatter(self):
        for rel, expected_name in self.COMMANDS.items():
            with self.subTest(path=rel):
                fm = _parse_frontmatter(_read(rel))
                self.assertEqual(fm.get("name"), expected_name)
                self.assertTrue(
                    fm.get("description"), f"{rel} missing a description"
                )


class TestReviewCommandWiring(unittest.TestCase):
    """essay-review wires the gates and contracts per the #460 curation."""

    def setUp(self):
        self.text = _read("commands/essay-review.md")

    def test_stamps_rubric_version_and_uses_staged_sidecar(self):
        # Per-review version stamping (#346) + atomic sidecar (#350/#376).
        self.assertIn(RUBRIC_ID, self.text)
        self.assertIn("rubric_total: 44", self.text)
        self.assertIn("advance_threshold: 35", self.text)
        self.assertIn("staged_sidecar", self.text)
        self.assertIn("cleanup_one_staging", self.text)
        self.assertIn("human-verdict", self.text)

    def test_numeric_gate_is_blocking(self):
        # The #462 hook built for this skill: --write-review --blocking.
        self.assertIn("anvil.lib.numeric_consistency", self.text)
        self.assertIn("--blocking", self.text)
        self.assertIn("--write-review", self.text)

    def test_hyperlink_gate_uses_promoted_module(self):
        # Promoted under THIS issue (second consumer of #335).
        self.assertIn("anvil.lib.hyperlink_resolver", self.text)
        self.assertIn("critical_broken_cross_thread_anchor", self.text)

    def test_rhetoric_lint_stays_advisory(self):
        self.assertIn("lint_rhetoric", self.text)
        self.assertIn("do NOT escalate severities", self.text)

    def test_rhetoric_lint_resolves_consumer_rules(self):
        # #479: voice.rhetoric_rules wired into step 3c via the
        # memo-render step 4g contract, ported to the DIRECT
        # lint_rhetoric call (kwarg is extra_rules_path=, not the
        # gate's rhetoric_rules_path=).
        self.assertIn("resolve_rhetoric_rules", self.text)
        self.assertIn("extra_rules_path=", self.text)
        # All three branches of the forwarding contract.
        self.assertIn("omit the `extra_rules_path=` kwarg", self.text)
        self.assertIn("extra_rules_path=entry.paths[0]", self.text)
        self.assertIn("still pass the path", self.text)
        # Declared-but-missing surfaces, never silently opts out.
        self.assertIn("Do NOT silently omit the kwarg", self.text)

    def test_example_coherence_llm_pass_present(self):
        # blog-review step-2.5 port; no detector by design (#462 gate 1).
        self.assertIn("Example-coherence", self.text)
        self.assertIn("physically needs", self.text)

    def test_voice_contract_absent_is_major_finding_not_crash(self):
        self.assertIn("`major` finding recommending the operator declare", self.text)
        self.assertIn("not a crash", self.text)

    def test_gate_record_file_in_sidecar_manifest(self):
        self.assertIn("_gate.json", self.text)

    def test_link_coverage_judgment_is_major_never_critical(self):
        self.assertIn("never critical", self.text)


class TestDraftAndReviseCommands(unittest.TestCase):
    def test_draft_records_voice_exemplars(self):
        text = _read("commands/essay-draft.md")
        self.assertIn("voice_exemplars", text)
        self.assertIn("resolve_voice_docs", text)
        # Slug-echo body, never the consumer's legacy filename.
        self.assertIn("echoes the slug", text)
        self.assertIn("post.md", text)  # named as the anti-pattern

    def test_revise_appends_score_history_against_35_of_44(self):
        text = _read("commands/essay-revise.md")
        self.assertIn("score_history", text)
        self.assertIn(RUBRIC_ID, text)
        self.assertIn('"threshold": 35', text)
        # Voice-preserve reviser contract (#461 one-liner).
        self.assertIn("voice signatures", text)

    def test_orchestrator_is_read_only_and_ready_terminal(self):
        text = _read("commands/essay.md")
        self.assertIn("read-only", text.lower())
        self.assertIn("terminal", text.lower())
        self.assertNotIn("essay-figures", text)
        self.assertNotIn("essay-audit", text)


class TestRubric(unittest.TestCase):
    """rubric.md declares 9 dims summing to 44, >=35, voice-dominant."""

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

    def test_dim_two_is_voice_fidelity_dominant(self):
        # Voice is the OWNED, highest-weighted dimension (weight 7).
        self.assertTrue(
            re.search(
                r"^\|\s*2\s*\|\s*\*\*Voice fidelity\*\*\s*\|\s*7\s*\|",
                self.text,
                flags=re.MULTILINE,
            ),
            "dim 2 must be Voice fidelity at weight 7",
        )

    def test_dim_nine_is_rhetorical_economy_load_bearing(self):
        self.assertTrue(
            re.search(
                r"^\|\s*9\s*\|\s*\*\*Rhetorical economy\*\*\s*\|\s*5\s*\|",
                self.text,
                flags=re.MULTILINE,
            ),
            "dim 9 must be Rhetorical economy at weight 5",
        )
        self.assertIn("load-bearing", self.text.lower())

    def test_advance_threshold_is_general_tier(self):
        self.assertTrue(
            re.search(r"(≥\s*35|>=\s*35|\b35/44\b)", self.text),
            "advance threshold of 35 not stated in rubric.md",
        )
        # The declared threshold is 35 (the line may legitimately
        # mention the >=39 band it is NOT using, so assert the positive
        # declaration rather than blacklisting "39" on the line).
        self.assertTrue(
            re.search(
                r"threshold to advance is \*\*≥35/44\*\*",
                self.text,
                re.IGNORECASE,
            ),
            "rubric must declare the >=35/44 advance threshold",
        )

    def test_rubric_id_declared(self):
        self.assertIn(RUBRIC_ID, self.text)

    def test_seven_critical_flags_named(self):
        lowered = self.text.lower()
        for flag in (
            "anti-stance violation",
            "out-of-standing claim",
            "generic ai cadence",
            "factual error",
            "unattributed borrowing",
            "example-coherence failure",
            "numeric-consistency failure",
        ):
            with self.subTest(flag=flag):
                self.assertIn(flag, lowered, f"missing critical flag: {flag}")

    def test_corpus_quote_rule_on_voice_deductions(self):
        # The load-bearing #461 discipline: every voice deduction quotes
        # a corpus exemplar.
        self.assertIn("MUST quote a corpus exemplar", self.text)

    def test_stamping_fields_in_meta_example(self):
        self.assertIn(f'"rubric_id": "{RUBRIC_ID}"', self.text)
        self.assertIn('"rubric_total": 44', self.text)
        self.assertIn('"advance_threshold": 35', self.text)

    def test_human_verdict_scorecard_kind(self):
        self.assertIn("human-verdict", self.text)


class TestRegistryIntegration(unittest.TestCase):
    """essay is registered as a skill-identity artifact type (#439/#457
    precedent) and an agent registration exists per lifecycle phase."""

    def test_artifact_type_registered_as_skill_identity(self):
        import sys

        if str(_REPO_ROOT) not in sys.path:
            sys.path.insert(0, str(_REPO_ROOT))
        from anvil.lib.project_brief import (
            ArtifactType,
            MEMO_ARTIFACT_TYPES,
            REGISTERED_ARTIFACT_TYPES,
            SKILL_IDENTITY_ARTIFACT_TYPES,
        )

        self.assertIn("essay", REGISTERED_ARTIFACT_TYPES)
        self.assertIn(ArtifactType.ESSAY, SKILL_IDENTITY_ARTIFACT_TYPES)
        # NOT a memo subtype — selects no memo rubric overlay.
        self.assertNotIn(ArtifactType.ESSAY, MEMO_ARTIFACT_TYPES)

    def test_lifecycle_agents_generated(self):
        agents_dir = _REPO_ROOT / "anvil" / "agents"
        for name in (
            "anvil-essay-drafter.md",
            "anvil-essay-reviewer.md",
            "anvil-essay-reviser.md",
        ):
            with self.subTest(agent=name):
                self.assertTrue(
                    (agents_dir / name).is_file(),
                    f"missing agent registration {name} "
                    f"(run scripts/generate-anvil-agents.py)",
                )
        # No audit/figures commands → no auditor/figurer agents.
        for name in ("anvil-essay-auditor.md", "anvil-essay-figurer.md"):
            with self.subTest(agent=name):
                self.assertFalse(
                    (agents_dir / name).exists(),
                    f"{name} should not exist — the phase is deferred scope",
                )


if __name__ == "__main__":
    unittest.main()
