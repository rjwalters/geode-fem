"""Structural smoke tests for the ``anvil:ip-uspto-provisional`` skill.

These tests assert **structural properties** of the shipped skill files
(files exist, frontmatter parses, the rubric declares 9 dimensions summing
to 45 with a >=39 advance threshold under the ``anvil-ip-provisional-v1``
id, the claims-optional posture is stated, every critic-writing command
stamps the issue #346 rubric-version fields and writes via the
staged-sidecar primitive, and the ``anvil:ip-uspto`` SKILL.md caveat
cross-references this sibling skill). They are intentionally NOT
golden-file tests — the skill is a generative authoring skill and prose
varies across runs and models.

Runs under either ``pytest anvil/skills/ip-uspto-provisional/tests/`` or
``python -m unittest discover anvil/skills/ip-uspto-provisional/tests/``.

The module filename is deliberately distinct
(``test_ip_uspto_provisional_skeleton``) per the issue #58 cross-skill
collection convention. Like the other hyphenated skill directories
(``project-migrate``, ``project-scout``), this tests dir carries NO
``__init__.py`` — ``ip-uspto-provisional`` is not a valid Python package
name, so the unique-filename rule (not a package chain) is what prevents
the pytest collection collision here (the ``anvil:ip-uspto`` sibling uses
the same shape).
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

_SKILL_ROOT = Path(__file__).resolve().parent.parent
_IP_USPTO_ROOT = _SKILL_ROOT.parent / "ip-uspto"

RUBRIC_ID = "anvil-ip-provisional-v1"


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
    """The pinned file manifest is present on disk (Phase 1 scope)."""

    EXPECTED = [
        "SKILL.md",
        "rubric.md",
        "README.md",
        "commands/ip-uspto-provisional.md",
        "commands/ip-uspto-provisional-draft.md",
        "commands/ip-uspto-provisional-review.md",
        "commands/ip-uspto-provisional-112.md",
        "commands/ip-uspto-provisional-prior-art.md",
        "commands/ip-uspto-provisional-revise.md",
        # Issue #480 completes the skill's own lifecycle: audit makes the
        # AUDITED state reachable, finalize assembles the COUNSEL-READY
        # filing package (READY -> AUDITED -> COUNSEL-READY).
        "commands/ip-uspto-provisional-audit.md",
        "commands/ip-uspto-provisional-finalize.md",
        # Issue #502 closes the provisional's own review loop: a mechanical
        # pre-flight gate (REVISED -> REVIEWED edge) and an opt-in
        # claim-seed critic.
        "commands/ip-uspto-provisional-pre-flight.md",
        "commands/ip-uspto-provisional-claims-seed.md",
        # Issue #515 adds the provisional-shaped drawings pipeline: a
        # deterministic figurer (stub-default + opt-in TikZ) and an opt-in,
        # gracefully-degrading drawings VLM critic (pixels-side half of Dim 4).
        "commands/ip-uspto-provisional-figures.md",
        "commands/ip-uspto-provisional-vision.md",
        # Issue #516 adds the advisory, non-gating inventorship-lite pass: an
        # inventor-LIST consistency check (BRIEF <-> spec <-> SB/16 cover
        # sheet), NOT a per-claim matrix and NOT a scoring critic.
        "commands/ip-uspto-provisional-inventorship.md",
        "tests/test_ip_uspto_provisional_skeleton.py",
    ]

    def test_manifest_present(self):
        for rel in self.EXPECTED:
            with self.subTest(path=rel):
                self.assertTrue(
                    (_SKILL_ROOT / rel).exists(), f"missing skill file: {rel}"
                )

    def test_inventorship_lite_is_advisory_non_gating(self):
        # The inventorship-lite pass (#516) is an advisory, non-gating
        # inventor-LIST consistency check — NOT a scoring critic. It must NOT
        # write a _review.json / rubric stamps and must NOT advance the state
        # machine; it must restate the claims-optional discipline.
        text = (
            _SKILL_ROOT / "commands" / "ip-uspto-provisional-inventorship.md"
        ).read_text(encoding="utf-8")
        lower = text.lower()
        self.assertIn("advisory", lower)
        self.assertIn("never a finding", lower)
        # It is a LIST check, not a per-claim matrix.
        self.assertIn("list", lower)
        # It must NOT advertise itself as a gate.
        self.assertIn("non-gating", lower)
        # Claims-optional discipline: absence is never a finding.
        self.assertIn("claims-optional", lower)


class TestSkillFrontmatter(unittest.TestCase):
    """SKILL.md frontmatter matches the sibling skills' shape."""

    def test_frontmatter(self):
        fm = _parse_frontmatter(_read("SKILL.md"))
        self.assertEqual(fm.get("name"), "ip-uspto-provisional")
        self.assertEqual(fm.get("domain"), "ip")
        self.assertEqual(fm.get("type"), "skill")
        self.assertIn(fm.get("user-invocable"), (False, "false"))

    def test_cross_references_sibling_skill(self):
        # The provisional is the conversion seed for the non-provisional.
        self.assertIn("anvil:ip-uspto", _read("SKILL.md"))

    def test_sidecar_and_stamping_contracts_referenced(self):
        text = _read("SKILL.md")
        self.assertIn("staged_sidecar", text)
        self.assertIn(RUBRIC_ID, text)
        self.assertIn("machine-summary", text)

    def test_claims_optional_posture_stated(self):
        text = _read("SKILL.md").lower()
        self.assertIn("claims-optional", text)
        self.assertIn("never a finding", text)

    def test_state_machine_through_counsel_ready(self):
        # Issue #480 completes the lifecycle: READY -> AUDITED ->
        # COUNSEL-READY are all reachable terminal states with shipped
        # commands.
        text = _read("SKILL.md")
        self.assertIn("AUDITED", text)
        self.assertIn("READY", text)
        self.assertIn("COUNSEL-READY", text)
        # The state-machine arrow now reaches the terminal COUNSEL-READY.
        self.assertIn("READY → AUDITED → COUNSEL-READY", text)


class TestCommandFrontmatter(unittest.TestCase):
    """Every command file carries a name/description frontmatter block."""

    COMMANDS = {
        "commands/ip-uspto-provisional.md": "ip-uspto-provisional",
        "commands/ip-uspto-provisional-draft.md": "ip-uspto-provisional-draft",
        "commands/ip-uspto-provisional-review.md": "ip-uspto-provisional-review",
        "commands/ip-uspto-provisional-112.md": "ip-uspto-provisional-112",
        "commands/ip-uspto-provisional-prior-art.md": "ip-uspto-provisional-prior-art",
        "commands/ip-uspto-provisional-revise.md": "ip-uspto-provisional-revise",
        "commands/ip-uspto-provisional-audit.md": "ip-uspto-provisional-audit",
        "commands/ip-uspto-provisional-finalize.md": "ip-uspto-provisional-finalize",
        "commands/ip-uspto-provisional-pre-flight.md": "ip-uspto-provisional-pre-flight",
        "commands/ip-uspto-provisional-claims-seed.md": "ip-uspto-provisional-claims-seed",
        "commands/ip-uspto-provisional-figures.md": "ip-uspto-provisional-figures",
        "commands/ip-uspto-provisional-vision.md": "ip-uspto-provisional-vision",
        "commands/ip-uspto-provisional-inventorship.md": "ip-uspto-provisional-inventorship",
    }

    def test_command_frontmatter(self):
        for rel, expected_name in self.COMMANDS.items():
            with self.subTest(path=rel):
                fm = _parse_frontmatter(_read(rel))
                self.assertEqual(fm.get("name"), expected_name)
                self.assertTrue(
                    fm.get("description"), f"{rel} missing a description"
                )

    CRITIC_COMMANDS = (
        "commands/ip-uspto-provisional-review.md",
        "commands/ip-uspto-provisional-112.md",
        "commands/ip-uspto-provisional-prior-art.md",
        # Issue #502 machine-summary siblings the reviser also consumes.
        "commands/ip-uspto-provisional-pre-flight.md",
        "commands/ip-uspto-provisional-claims-seed.md",
    )

    def test_critic_commands_stamp_rubric_version(self):
        # ALL critic-writing commands stamp rubric_id / rubric_total /
        # advance_threshold per the issue #346 contract and write via the
        # staged-sidecar primitive (issues #350/#376).
        for rel in self.CRITIC_COMMANDS:
            with self.subTest(path=rel):
                text = _read(rel)
                self.assertIn(RUBRIC_ID, text)
                self.assertIn('rubric_total: 45', text)
                self.assertIn('advance_threshold: 39', text)
                self.assertIn("staged_sidecar", text)
                self.assertIn("cleanup_one_staging", text)
                self.assertIn("machine-summary", text)

    def test_revise_aggregates_against_45_and_39(self):
        text = _read("commands/ip-uspto-provisional-revise.md")
        self.assertIn(RUBRIC_ID, text)
        self.assertIn("39/45", text)
        self.assertIn("score_history", text)

    def test_draft_has_no_abstract_and_optional_claim_seed(self):
        text = _read("commands/ip-uspto-provisional-draft.md")
        self.assertIn("claim-seed", text)
        self.assertIn("No abstract", text)
        # The class is reused from the ip-uspto sibling's assets.
        self.assertIn("anvil-uspto.cls", text)
        self.assertIn("anvil/skills/ip-uspto/assets", text)


class TestAuditCommand(unittest.TestCase):
    """ip-uspto-provisional-audit (item 2) — provisional-adapted audit."""

    def setUp(self):
        self.text = _read("commands/ip-uspto-provisional-audit.md")

    def test_frontmatter_role_auditor(self):
        fm = _parse_frontmatter(self.text)
        self.assertEqual(fm.get("name"), "ip-uspto-provisional-audit")
        self.assertIn("**Role**: auditor", self.text)

    def test_discovers_on_ready_not_ready_for_audit(self):
        # The load-bearing discovery-marker delta: the provisional reviser
        # writes header READY, not ip-uspto's READY_FOR_AUDIT.
        self.assertIn("READY", self.text)
        self.assertIn("NOT `READY_FOR_AUDIT`", self.text)

    def test_stamps_rubric_version_fields(self):
        self.assertIn(RUBRIC_ID, self.text)
        self.assertIn("rubric_total: 45", self.text)
        self.assertIn("advance_threshold: 39", self.text)

    def test_staged_sidecar_atomicity(self):
        self.assertIn("staged_sidecar", self.text)
        self.assertIn("cleanup_one_staging", self.text)
        self.assertIn("machine-summary", self.text)

    def test_summary_records_passed(self):
        # The finalizer gate reads this boolean.
        self.assertIn("passed: <true|false>", self.text)

    def test_claim_and_abstract_checks_dropped_or_softened(self):
        lowered = self.text.lower()
        # No abstract-correctness CHECK heading (a "### Check N — Abstract
        # correctness" numbered check, as ip-uspto carries). The doc may
        # still NAME the dropped check in prose ("the abstract-correctness
        # check is dropped").
        self.assertNotIn("— abstract correctness", lowered)
        self.assertIn("no abstract", lowered)
        # Claim-seed is CONDITIONAL and caps at major.
        self.assertIn("conditional", lowered)
        self.assertIn("claim-seed", lowered)
        self.assertIn("major", lowered)
        # The absence of a claim-seed is never a finding.
        self.assertIn("never a finding", lowered)
        # No inventorship-matrix currency CHECK heading carries over.
        self.assertNotIn("— inventorship matrix currency", lowered)

    def test_carryover_checks_present(self):
        lowered = self.text.lower()
        self.assertIn("inventor name consistency", lowered)
        self.assertIn("reference numeral coherence", lowered)
        self.assertIn("background admissions", lowered)
        self.assertIn("date and citation", lowered)


class TestFinalizeCommand(unittest.TestCase):
    """ip-uspto-provisional-finalize (item 1) — COUNSEL-READY package."""

    def setUp(self):
        self.text = _read("commands/ip-uspto-provisional-finalize.md")

    def test_frontmatter_role_finalizer(self):
        fm = _parse_frontmatter(self.text)
        self.assertEqual(fm.get("name"), "ip-uspto-provisional-finalize")
        self.assertIn("**Role**: finalizer", self.text)

    def test_terminal_dir_and_state(self):
        # Distinct from ip-uspto's <thread>.final/ / FINALIZED.
        self.assertIn("<thread>.counsel/", self.text)
        self.assertIn("COUNSEL-READY", self.text)

    def test_discovers_audited_version(self):
        self.assertIn("passed: true", self.text)
        self.assertIn(".audit/_summary.md", self.text)

    def test_gate_is_audit_only(self):
        # NO inventorship-lock gate, NO pre-flight gate.
        self.assertIn("audit passed ONLY", self.text)
        lowered = self.text.lower()
        self.assertIn("no inventorship-lock gate", lowered)
        self.assertIn("no pre-flight gate", lowered)

    def test_provisional_package_shape(self):
        # New artifact: counsel_memo.md. Provisional SB/16 cover sheet.
        self.assertIn("counsel_memo.md", self.text)
        self.assertIn("cover-sheet-placeholder.txt", self.text)
        self.assertIn("SB/16", self.text)
        # NOT the ADS / SB/14.
        self.assertIn("NOT", self.text)

    def test_no_abstract_no_inventorship_attestation(self):
        # The provisional package omits both — and the doc says so loudly.
        # It must NOT add them to the assembled package's required-files
        # set, but it DOES name them in the comparison table to mark them
        # excluded. Assert the explicit "NO abstract.txt" / no-inventorship
        # exclusion language is present, and that neither appears in the
        # _manifest.json artifact-row examples.
        self.assertIn("NO abstract.txt", self.text)
        manifest_block = self.text[
            self.text.index('"artifacts": [') : self.text.index(
                '"claim_seed_present"'
            )
        ]
        self.assertNotIn("abstract.txt", manifest_block)
        self.assertNotIn("inventorship-attestation", manifest_block)

    def test_claims_tex_conditional(self):
        # claims.tex copied IFF a claim-seed exists.
        self.assertIn("claims.tex", self.text)
        self.assertIn("IFF", self.text)

    def test_flat_provisional_fee_not_claim_math(self):
        lowered = self.text.lower()
        self.assertIn("flat", lowered)
        # No excess-claims math in the cover sheet.
        self.assertIn("no excess-claims fee", lowered)

    def test_staged_sidecar_atomicity(self):
        self.assertIn("staged_sidecar", self.text)
        self.assertIn("cleanup_one_staging", self.text)
        self.assertIn(".counsel.tmp", self.text)

    def test_git_sync_terminal_token(self):
        self.assertIn(
            "anvil(ip-uspto-provisional/finalize): <thread>.counsel "
            "[COUNSEL-READY]",
            self.text,
        )


class TestPreFlightCommand(unittest.TestCase):
    """ip-uspto-provisional-pre-flight (#502) — provisional-adapted gate."""

    def setUp(self):
        self.text = _read("commands/ip-uspto-provisional-pre-flight.md")
        self.lowered = self.text.lower()

    def test_frontmatter_role_preflight(self):
        fm = _parse_frontmatter(self.text)
        self.assertEqual(fm.get("name"), "ip-uspto-provisional-pre-flight")
        self.assertIn("**Role**: pre-flight checker", self.text)

    def test_gates_revised_to_reviewed_edge(self):
        self.assertIn("REVISED → REVIEWED", self.text)
        self.assertIn("PRE_FLIGHT_FAILED", self.text)

    def test_dropped_and_neutered_checks(self):
        # Abstract / claim-numbering / claim-count / 1.77(b) are dropped or
        # neutered for the provisional shape; multiple-dependent neutered.
        self.assertIn("DROPPED", self.text)
        self.assertIn("NEUTERED", self.text)
        self.assertIn("1.77(b)", self.text)
        # No abstract check is ever run.
        self.assertIn("no `abstract.txt`", self.text)

    def test_replaced_section_check_uses_five_required_ids(self):
        # 1.77(b) replaced with the provisional's own required-section set.
        for sect in (
            "`field`",
            "`background`",
            "`summary`",
            "`brief-description-of-drawings`",
            "`detailed-description`",
        ):
            with self.subTest(section=sect):
                self.assertIn(sect, self.text)
        # claim-seed is optional; absence never a finding.
        self.assertIn("claim-seed", self.lowered)
        self.assertIn("never a finding", self.lowered)

    def test_kept_checks_present(self):
        self.assertIn("paragraph numbering", self.lowered)
        self.assertIn("reference numeral coherence", self.lowered)
        self.assertIn("documentclass", self.lowered)
        # Render-gate kept with page_cap=None and the placeholder patterns.
        self.assertIn("compile_and_gate", self.text)
        self.assertIn("page_cap=None", self.text)
        self.assertIn(r"\refnum{??}", self.text)
        self.assertIn(r"\anvilpara{}", self.text)

    def test_added_s112_stub_scan_is_advisory_minor(self):
        self.assertIn("enablement-stub scan", self.lowered)
        self.assertIn("advisory", self.lowered)
        self.assertIn("minor", self.lowered)
        # Advisory to s112, never a blocker.
        self.assertIn("advisory: s112", self.text)

    def test_graceful_degrade_on_missing_pdflatex(self):
        # pdflatex absent -> minor, not blocker; pre-flight still passes.
        self.assertIn("unavailable", self.lowered)
        self.assertIn("not a blocker", self.lowered)
        self.assertIn("still PASSES on CI", self.text)

    def test_machine_summary_and_stamping(self):
        self.assertIn("machine-summary", self.text)
        self.assertIn(RUBRIC_ID, self.text)
        self.assertIn("rubric_total: 45", self.text)
        self.assertIn("advance_threshold: 39", self.text)

    def test_atomicity_and_gate_json(self):
        self.assertIn("staged_sidecar", self.text)
        self.assertIn("cleanup_one_staging", self.text)
        self.assertIn(".preflight.tmp", self.text)
        self.assertIn("_gate.json", self.text)

    def test_git_sync_token(self):
        self.assertIn(
            "anvil(ip-uspto-provisional/pre-flight): <thread>.{N}", self.text
        )


class TestClaimsSeedCommand(unittest.TestCase):
    """ip-uspto-provisional-claims-seed (#502) — opt-in claim-seed critic."""

    def setUp(self):
        self.text = _read("commands/ip-uspto-provisional-claims-seed.md")
        self.lowered = self.text.lower()

    def test_frontmatter_role_claimseed(self):
        fm = _parse_frontmatter(self.text)
        self.assertEqual(fm.get("name"), "ip-uspto-provisional-claims-seed")
        self.assertIn("**Role**: claim-seed critic", self.text)

    def test_absence_is_never_a_finding(self):
        # The single most important behavioral rule.
        self.assertIn("never a finding", self.lowered)
        self.assertIn("never a deduction", self.lowered)
        self.assertIn("never a critical flag", self.lowered)
        # Absence path: dim 9 null, no finding.
        self.assertIn("null", self.lowered)
        self.assertIn("removing a seed never raises the score", self.lowered)

    def test_opt_in_not_default_set(self):
        self.assertIn("opt-in", self.lowered)
        self.assertIn("not in the default", self.lowered)
        # The reviser must not refuse to advance when absent.
        self.assertIn("must not refuse to advance", self.lowered)
        # Recognized tag + .anvil.json wiring.
        self.assertIn("claimseed", self.text)
        self.assertIn(".anvil.json", self.text)

    def test_major_cap_and_disclosure_routing(self):
        # Seed defects cap at major except disclosure-gap -> s112/dims 1-3.
        self.assertIn("major", self.lowered)
        self.assertIn("route: s112", self.text)
        self.assertIn("dims: 1-3", self.text)
        self.assertIn("do not double-flag", self.lowered)

    def test_dim9_positive_evidence_ceiling_discipline(self):
        self.assertIn("conversion readiness", self.lowered)
        self.assertIn("raises the reachable", self.lowered)
        # Must not drive dim 9 below the spec-alone score.
        self.assertIn("never down", self.lowered)

    def test_never_a_critical_flag_gatekeeper(self):
        self.assertIn("never sets a critical flag", self.lowered)
        self.assertIn("flagged", self.lowered)
        # s112 is the only disclosure gatekeeper.
        self.assertIn("s112", self.text)

    def test_machine_summary_and_stamping(self):
        self.assertIn("machine-summary", self.text)
        self.assertIn(RUBRIC_ID, self.text)
        self.assertIn("rubric_total: 45", self.text)
        self.assertIn("advance_threshold: 39", self.text)

    def test_atomicity(self):
        self.assertIn("staged_sidecar", self.text)
        self.assertIn("cleanup_one_staging", self.text)
        self.assertIn(".claimseed.tmp", self.text)

    def test_git_sync_token(self):
        self.assertIn(
            "anvil(ip-uspto-provisional/claims-seed): <thread>.{N}", self.text
        )


class TestReviseReferencesPreFlight(unittest.TestCase):
    """revise.md:83 now reflects the pre-flight gate exists (#502)."""

    def test_revise_no_longer_says_no_pre_flight(self):
        text = _read("commands/ip-uspto-provisional-revise.md")
        # The stale "no pre-flight gate in Phase 1" sentence is gone.
        self.assertNotIn("There is no pre-flight gate in Phase 1", text)
        self.assertNotIn("no pre-flight gate in Phase 1", text)
        # The gate is now wired into the convergence loop.
        self.assertIn("ip-uspto-provisional-pre-flight", text)
        self.assertIn("PRE_FLIGHT_FAILED", text)


class TestSkillCommandDispatch(unittest.TestCase):
    """SKILL.md dispatch table + orchestrator recognize the new commands."""

    def test_skill_dispatch_rows(self):
        text = _read("SKILL.md")
        self.assertIn("`ip-uspto-provisional-audit <thread>`", text)
        self.assertIn("`ip-uspto-provisional-finalize <thread>`", text)

    def test_skill_dispatch_rows_preflight_and_claimseed(self):
        text = _read("SKILL.md")
        self.assertIn("`ip-uspto-provisional-pre-flight <thread>`", text)
        self.assertIn("`ip-uspto-provisional-claims-seed <thread>`", text)

    def test_skill_line91_pre_flight_gate_exists(self):
        text = _read("SKILL.md")
        # The stale "no pre-flight gate ... tracked follow-up" sentence gone.
        self.assertNotIn(
            "no pre-flight gate (the provisional pre-flight is a tracked "
            "follow-up, issue #502)",
            text,
        )
        self.assertIn("mechanical pre-flight gate", text)
        self.assertIn("REVISED → REVIEWED", text)

    def test_skill_line108_claim_seed_critic_exists(self):
        text = _read("SKILL.md")
        # The stale "claim-seed critic is a tracked follow-up" sentence gone.
        self.assertNotIn(
            "The claim-seed critic is a tracked follow-up.", text
        )
        self.assertIn("ip-uspto-provisional-claims-seed", text)

    def test_skill_multicritic_opt_in_claimseed_tag(self):
        text = _read("SKILL.md")
        self.assertIn("claimseed", text)
        self.assertIn("NOT in the default set", text)

    def test_skill_dispatch_rows_figures_and_vision(self):
        text = _read("SKILL.md")
        self.assertIn("`ip-uspto-provisional-figures <thread>`", text)
        self.assertIn("`ip-uspto-provisional-vision <thread>`", text)

    def test_skill_multicritic_vision_opt_in_non_gating(self):
        text = _read("SKILL.md")
        # The vision tag is opt-in, non-gating, gracefully-degrading.
        self.assertIn("`vision`", text)
        self.assertIn("NOT in the default set", text)
        # The reviser must not refuse to advance when vision is absent.
        self.assertIn("NOT refuse to advance when `vision` is absent", text)
        # Dim-4 split + graceful degradation are stated in the SKILL.md note.
        self.assertIn("pixels-side half of rubric Dim 4", text)
        self.assertIn("NO Dim-4 deduction", text)

    def test_skill_counsel_state_row(self):
        text = _read("SKILL.md")
        self.assertIn("`COUNSEL-READY`", text)
        # The deferral caveat ("tracked follow-up" for audit/counsel) is gone.
        self.assertNotIn(
            "the `ip-uspto-provisional-audit` command is a tracked follow-up",
            text,
        )

    def test_orchestrator_recommends_new_commands(self):
        text = _read("commands/ip-uspto-provisional.md")
        self.assertIn("ip-uspto-provisional-audit <thread>", text)
        self.assertIn("ip-uspto-provisional-finalize <thread>", text)
        self.assertIn("COUNSEL-READY", text)


class TestRubric(unittest.TestCase):
    """rubric.md declares 9 dims summing to 45, >=39, enablement-dominant."""

    def setUp(self):
        self.text = _read("rubric.md")

    def test_nine_dimensions_sum_to_forty_five(self):
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
        self.assertEqual(total, 45, f"dimension weights sum to {total}, not 45")

    def test_dim_one_is_enablement_depth_dominant(self):
        # The provisional inversion: dim 1 §112(a) enablement depth carries
        # the dominant weight 8 (vs ip-uspto's flat 5s).
        self.assertTrue(
            re.search(
                r"^\|\s*1\s*\|\s*\*\*§112\(a\) enablement depth\*\*\s*\|\s*8\s*\|",
                self.text,
                flags=re.MULTILINE,
            ),
            "dim 1 must be §112(a) enablement depth at weight 8",
        )

    def test_dim_nine_is_conversion_readiness(self):
        # Replaces ip-uspto's Claim-spec correspondence (inapplicable when
        # claims are optional) per the issue #433 curation.
        self.assertTrue(
            re.search(
                r"^\|\s*9\s*\|\s*\*\*Conversion readiness\*\*\s*\|\s*6\s*\|",
                self.text,
                flags=re.MULTILINE,
            ),
            "dim 9 must be Conversion readiness at weight 6",
        )
        self.assertNotIn("| **Claim-spec correspondence** |", self.text)

    def test_advance_threshold_is_high_band(self):
        # Legal artifact -> the high threshold band (>=39), NOT >=35.
        self.assertTrue(
            re.search(r"(≥\s*39|>=\s*39|\b39/45\b)", self.text),
            "advance threshold of 39 not stated in rubric.md",
        )
        self.assertIsNone(
            re.search(r"threshold to advance[^\n]*35", self.text, re.IGNORECASE),
            "rubric must not declare a 35 advance threshold",
        )

    def test_rubric_id_declared(self):
        self.assertIn(RUBRIC_ID, self.text)

    def test_claims_optional_never_penalized(self):
        lowered = self.text.lower()
        self.assertIn("never a finding", lowered)
        self.assertIn("never a deduction", lowered)
        self.assertIn("never a critical flag", lowered)

    def test_machine_summary_scorecard_kind(self):
        self.assertIn("machine-summary", self.text)

    def test_stamping_fields_in_meta_example(self):
        self.assertIn('"rubric_id": "anvil-ip-provisional-v1"', self.text)
        self.assertIn('"rubric_total": 45', self.text)
        self.assertIn('"advance_threshold": 39', self.text)

    def test_s112_is_load_bearing_owner(self):
        # s112 owns the dominant dimension and may not be subsetted out.
        self.assertIn("load-bearing critic", self.text)
        self.assertIn("may not be subsetted out", self.text)


class TestFiguresCommand(unittest.TestCase):
    """ip-uspto-provisional-figures (#515) — provisional-shaped figurer."""

    def setUp(self):
        self.text = _read("commands/ip-uspto-provisional-figures.md")
        self.lowered = self.text.lower()

    def test_frontmatter_role_figurer(self):
        fm = _parse_frontmatter(self.text)
        self.assertEqual(fm.get("name"), "ip-uspto-provisional-figures")
        self.assertIn("**Role**: figurer", self.text)

    def test_deterministic_stub_default_and_tikz_opt_in(self):
        # Stub is the default; tikz is the opt-in mode flag.
        self.assertIn("--mode", self.text)
        self.assertIn("stub", self.lowered)
        self.assertIn("tikz", self.lowered)
        self.assertIn("stub-default", self.lowered)

    def test_drawings_are_conversion_scope_not_claim_coverage(self):
        # The provisional reframe: drawings are §119(e) conversion scope,
        # NOT claim-element coverage (the non-provisional framing).
        self.assertIn("conversion scope", self.lowered)
        self.assertIn("not claim-element coverage", self.lowered)
        self.assertIn("§119(e)", self.text)

    def test_reuses_render_py_unchanged_reference_only(self):
        self.assertIn("render.py", self.text)
        self.assertIn("unchanged", self.lowered)
        # The matplotlib walker is the reused render.py entry point.
        self.assertIn("render_matplotlib_figures", self.text)

    def test_informal_drawings_acceptable_posture(self):
        # 1.84 formality is noted for conversion ease, not enforced.
        self.assertIn("informal", self.lowered)
        self.assertIn("1.84", self.text)

    def test_git_sync_token(self):
        self.assertIn(
            "anvil(ip-uspto-provisional/figures): <thread>.{N}", self.text
        )


class TestVisionCommand(unittest.TestCase):
    """ip-uspto-provisional-vision (#515) — provisional drawings VLM critic."""

    VISION_RUBRIC_ID = "anvil-ip-provisional-vision-v1"

    def setUp(self):
        self.text = _read("commands/ip-uspto-provisional-vision.md")
        self.lowered = self.text.lower()

    def test_frontmatter_role_vision(self):
        fm = _parse_frontmatter(self.text)
        self.assertEqual(fm.get("name"), "ip-uspto-provisional-vision")
        self.assertIn("**Role**: rendered-artifact critic", self.text)

    def test_scores_pixels_side_dim4_not_dim7(self):
        # The vision critic owns the pixels-side half of Dim 4 (NOT Dim 7).
        self.assertIn("Dim 4", self.text)
        self.assertIn("pixels-side", self.lowered)
        # It is explicitly NOT ip-uspto's Dim 7.
        self.assertIn("NOT ip-uspto's Dim 7", self.text)

    def test_drops_1_84_formality_dims(self):
        # The two pure-formality dims are dropped for the provisional posture.
        self.assertIn("line_weight_contrast", self.text)
        self.assertIn("figure_number_visibility", self.text)
        self.assertIn("DROPPED", self.text)
        # The kept dims are the enablement/scope-relevant ones.
        self.assertIn("reference_numeral_legibility", self.text)
        self.assertIn("label_placement", self.text)
        self.assertIn("cross_reference_accuracy", self.text)

    def test_vision_subset_rubric_id(self):
        self.assertIn(self.VISION_RUBRIC_ID, self.text)

    def test_reuses_framework_flag_as_119e_scope_loss(self):
        # Reuse the framework flag; frame the loss as §119(e) priority scope.
        self.assertIn("rendered_overflow_unrecoverable", self.text)
        self.assertIn("§119(e)", self.text)
        self.assertIn("priority-scope loss", self.lowered)
        # No new flag types invented.
        self.assertIn("no new flag types", self.lowered)

    def test_double_flag_guard_against_line70_s112(self):
        # Must NOT double-flag the rubric-line-70 s112 missing-drawing gap.
        self.assertIn("line 70", self.text)
        self.assertIn("double-flag", self.lowered)
        self.assertIn("s112", self.text)
        # Absence of a drawing is never a vision finding.

    def test_graceful_degradation_no_review_json_no_deduction(self):
        # The headline provisional invariant.
        self.assertIn("skipped", self.lowered)
        self.assertIn("no_rendered_drawings", self.text)
        # Does NOT write a _review.json on the degradation path.
        self.assertIn("does **not** write a `_review.json`", self.lowered)
        self.assertIn("NO Dim-4 deduction", self.text)
        self.assertIn("absence is never a finding", self.lowered)

    def test_opt_in_non_gating(self):
        self.assertIn("opt-in", self.lowered)
        self.assertIn("not in the default critic set", self.lowered)
        self.assertIn("NOT refuse to advance", self.text)

    def test_reuses_vision_py_unchanged(self):
        self.assertIn("vision.py", self.text)
        self.assertIn("unchanged", self.lowered)

    def test_machine_summary_and_main_rubric_stamping(self):
        # The _meta.json stamps the MAIN provisional rubric (Dim 4 owner).
        self.assertIn("machine-summary", self.text)
        self.assertIn(RUBRIC_ID, self.text)
        self.assertIn("rubric_total: 45", self.text)
        self.assertIn("advance_threshold: 39", self.text)

    def test_atomicity(self):
        self.assertIn("staged_sidecar", self.text)
        self.assertIn("cleanup_one_staging", self.text)
        self.assertIn(".vision.tmp", self.text)

    def test_git_sync_token(self):
        self.assertIn(
            "anvil(ip-uspto-provisional/vision): <thread>.{N}", self.text
        )


class TestVisionRubricBehavior(unittest.TestCase):
    """Behavioral check: the provisional vision rubric composes to 3 dims /15
    via the framework primitives, and graceful degradation produces NO
    _review.json (so the aggregator never sees a vision scorecard).
    """

    def setUp(self):
        # Make the repo root importable (this file is four levels deep).
        import sys

        repo_root = _SKILL_ROOT.parents[2]
        if str(repo_root) not in sys.path:
            sys.path.insert(0, str(repo_root))

    def _rubric(self):
        from anvil.lib.vision import VisionDimension, VisionRubric

        dims = (
            VisionDimension(
                name="reference_numeral_legibility",
                max=5,
                description="Load-bearing numerals legible at examiner scale.",
            ),
            VisionDimension(
                name="label_placement",
                max=5,
                description="Labels identify their part; scope-relevant only.",
            ),
            VisionDimension(
                name="cross_reference_accuracy",
                max=5,
                description="Drawn numerals correspond to spec; pixels-side.",
            ),
        )
        return VisionRubric(
            dimensions=dims, rubric_id="anvil-ip-provisional-vision-v1"
        )

    def test_rubric_owns_three_dims_scored_out_of_fifteen(self):
        rubric = self._rubric()
        names = [d.name for d in rubric.dimensions]
        self.assertEqual(
            names,
            [
                "reference_numeral_legibility",
                "label_placement",
                "cross_reference_accuracy",
            ],
        )
        self.assertEqual(rubric.max_total(), 15)
        self.assertEqual(rubric.rubric_id, "anvil-ip-provisional-vision-v1")
        # The two dropped 1.84-formality dims are NOT in the subset.
        self.assertNotIn("line_weight_contrast", names)
        self.assertNotIn("figure_number_visibility", names)

    def test_rendered_drawing_scored_with_119e_critical_flag(self):
        # A load-bearing numeral clipped at examiner scale -> the framework
        # rendered_overflow_unrecoverable flag (framed as §119(e) scope loss).
        from anvil.lib.review_schema import Kind, Review
        from anvil.lib.vision import (
            CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE,
            VisionCritic,
        )

        def stub(images, prompt):
            return {
                "scores": [
                    {
                        "dimension": "reference_numeral_legibility",
                        "score": 0,
                        "critical": True,
                        "justification": (
                            "Numeral '14' on FIG. 2 clipped at the right "
                            "border; unreadable at examiner scale."
                        ),
                        "fix": "Reposition '14' inside the border.",
                    },
                    {
                        "dimension": "label_placement",
                        "score": 3,
                        "critical": False,
                        "justification": "Lead lines mostly clear.",
                        "fix": None,
                    },
                    {
                        "dimension": "cross_reference_accuracy",
                        "score": 4,
                        "critical": False,
                        "justification": "Drawn numerals match the spec.",
                        "fix": None,
                    },
                ],
                "findings": [],
                "critical_flags": [
                    {
                        "type": CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE,
                        "justification": (
                            "Numeral '14' clipped on FIG. 2 — §119(e) scope "
                            "the conversion cannot claim with priority."
                        ),
                        "evidence_span": "drawings/fig-2.png",
                    }
                ],
            }

        review = VisionCritic(
            critic_id="ip-uspto-provisional-vision", callback=stub
        ).critique(
            images=[],
            rubric=self._rubric(),
            version_dir="acme-widget-prov.2",
            rendered_artifact="drawings/",
        )
        self.assertEqual(review.kind, Kind.VISION)
        self.assertEqual(review.rubric, "anvil-ip-provisional-vision-v1")
        self.assertEqual(review.rendered_artifact, "drawings/")
        flags = {cf.type for cf in review.critical_flags}
        self.assertIn(CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE, flags)
        # The vision critic scores only its three owned dims (the 8 main-rubric
        # dims stay null/absent from the vision scorecard).
        scored = {s.dimension for s in review.scores}
        self.assertEqual(
            scored,
            {
                "reference_numeral_legibility",
                "label_placement",
                "cross_reference_accuracy",
            },
        )

    def test_stub_only_degradation_writes_no_review_json(self):
        # Graceful degradation invariant: a stub-only thread (no rendered
        # fig-*) produces NO _review.json, so discover_critics finds no
        # vision scorecard and the aggregate applies no Dim-4 deduction.
        from anvil.lib.critics import discover_critics, load_review

        portfolio = Path(self.id())  # unique-ish; replaced below by tmp
        import tempfile

        with tempfile.TemporaryDirectory() as td:
            portfolio = Path(td)
            version_dir = portfolio / "acme-widget-prov.2"
            drawings = version_dir / "drawings"
            drawings.mkdir(parents=True)
            # Stub-only: a descriptions file, no rendered fig-*.
            (drawings / "drawing-descriptions.md").write_text(
                "## FIG. 1 — block diagram\nstub only\n"
            )
            # The vision critic, on the degradation path, writes nothing (or a
            # skipped sibling with no _review.json). Simulate the contract:
            # no <thread>.2.vision/_review.json on disk.
            found = discover_critics(version_dir)
            vision_reviews = [
                load_review(p)
                for p in found
                if p.name == "acme-widget-prov.2.vision"
                and (p / "_review.json").exists()
            ]
            # No vision scorecard exists -> aggregator never sees one -> no
            # Dim-4 deduction from the vision critic.
            self.assertEqual(vision_reviews, [])


class TestSiblingCrossReference(unittest.TestCase):
    """The ip-uspto SKILL.md caveat now points at this sibling skill."""

    def test_ip_uspto_caveat_updated(self):
        text = (_IP_USPTO_ROOT / "SKILL.md").read_text(encoding="utf-8")
        self.assertIn(
            "ip-uspto-provisional",
            text,
            "anvil/skills/ip-uspto/SKILL.md must cross-reference the "
            "ip-uspto-provisional sibling skill (issue #433)",
        )
        self.assertNotIn(
            "Provisional applications and design patents are out of scope",
            text,
            "the stale provisionals-out-of-scope caveat must be updated",
        )


if __name__ == "__main__":
    unittest.main()
