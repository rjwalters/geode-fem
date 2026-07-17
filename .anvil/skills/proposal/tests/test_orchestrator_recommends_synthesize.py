"""Orchestrator + state-machine integration tests for the ``SYNTHESIZED`` state.

This module pins the **documented contract** that the portfolio orchestrator
(``commands/proposal.md``) and the skill's state-machine documentation
(``SKILL.md``) recognize the new ``SYNTHESIZED`` transient state and
recommend ``proposal-synthesize`` / ``proposal-revise`` at the right
points in the state machine. The contract is what sub-issue 3 of #246
actually ships — documentation changes to ``commands/proposal.md`` and
``SKILL.md`` plus this test that the documentation says what it has to
say.

Scope is intentionally narrow — the schema and command spec are sub-issue
1 (already merged), the reviser-side consumption is sub-issue 2 (already
merged), and the Studio reproducer integration test is sub-issue 4 (not
yet filed). This module asserts only the prose-level structural
properties of:

1. ``commands/proposal.md`` §Procedure step 3 (state inference)
2. ``commands/proposal.md`` §Procedure step 4 (recommendation dispatch table)
3. ``commands/proposal.md`` §Procedure step 5 (anomaly detection)
4. ``commands/proposal.md`` §Output format (example table row)
5. ``SKILL.md`` §State machine (diagram + evidence row)
6. ``SKILL.md`` §Command dispatch (proposal-synthesize row)

Contracts pinned by this module:

1. **proposal.md documents SYNTHESIZED in the recommendation table.**
   The dispatch table at step 4 has rows for both
   ``REVIEWED+AUDITED`` → ``proposal-synthesize`` and ``SYNTHESIZED``
   → ``proposal-revise``, under the iteration cap, plus the AT-cap
   variant for ``SYNTHESIZED``.
2. **SKILL.md documents the new state-machine row.** The state-machine
   evidence table includes a ``SYNTHESIZED`` row keyed on
   ``<thread>.{N}.synthesis/verdict.md`` + ``gaps.json``, and the
   state-machine diagram includes ``SYNTHESIZED`` between
   ``REVIEWED+AUDITED`` and ``REVISED``.
3. **The orchestrator's REVIEWED+AUDITED → proposal-synthesize
   recommendation is present and parseable.** Step 4's dispatch table is
   structured markdown that downstream tooling (or a future orchestrator
   implementation) can grep.
4. **Backward compat: the fallback path is preserved.** When no
   ``synthesis/`` sibling is present at ``REVIEWED+AUDITED``, the
   orchestrator still functions — the prose explicitly preserves the
   ``REVIEWED+AUDITED`` → ``proposal-revise`` direct path as supported
   (cross-referencing ``proposal-synthesize.md`` §"Backward
   compatibility" and ``proposal-revise.md`` step 6 for the reviser's
   fallback contract).

The module filename is deliberately distinct
(``test_orchestrator_recommends_synthesize``) per the #58 packaging
convention to avoid the cross-skill pytest collection collision.

Runs under either ``pytest anvil/skills/proposal/tests/`` or
``python -m unittest discover anvil/skills/proposal/tests/``.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path


_SKILL_ROOT = Path(__file__).resolve().parent.parent
_PROPOSAL_MD = _SKILL_ROOT / "commands" / "proposal.md"
_SKILL_MD = _SKILL_ROOT / "SKILL.md"


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def _extract_section(text: str, heading: str) -> str:
    """Extract a markdown section by its top-level heading.

    Returns text from the heading line through (but not including) the
    next sibling heading of equal or higher rank. Lines outside the
    section are dropped.
    """

    lines = text.splitlines()
    in_section = False
    target_level = heading.count("#") if heading.startswith("#") else 2
    out: list[str] = []
    for line in lines:
        if line.strip() == heading or (
            line.startswith("## ") and line.strip() == heading
        ):
            in_section = True
            out.append(line)
            continue
        if in_section:
            stripped = line.lstrip("#").rstrip()
            level = len(line) - len(line.lstrip("#"))
            if (
                line.startswith("#")
                and 0 < level <= target_level
                and stripped
            ):
                # Next sibling heading: stop.
                break
            out.append(line)
    return "\n".join(out)


# ---------------------------------------------------------------------------
# commands/proposal.md — state inference (Procedure step 3)
# ---------------------------------------------------------------------------


class TestProposalMdStateInference(unittest.TestCase):
    """Procedure step 3 recognizes ``SYNTHESIZED`` as a valid state."""

    def setUp(self):
        self.text = _read(_PROPOSAL_MD)

    def test_synthesized_state_named(self):
        # The state-inference step MUST name SYNTHESIZED as a state the
        # orchestrator computes from on-disk evidence.
        self.assertIn(
            "SYNTHESIZED",
            self.text,
            "proposal.md must name the SYNTHESIZED state in step 3",
        )

    def test_synthesis_sibling_evidence_named(self):
        # The evidence for the SYNTHESIZED state is the synthesis sibling's
        # verdict.md + gaps.json files at the latest N — per the schema
        # sub-issue's documented output contract.
        self.assertIn(
            "<slug>.{N}.synthesis/verdict.md",
            self.text,
            "step 3 must name <slug>.{N}.synthesis/verdict.md as the "
            "SYNTHESIZED state's evidence",
        )
        self.assertIn(
            "<slug>.{N}.synthesis/gaps.json",
            self.text,
            "step 3 must name <slug>.{N}.synthesis/gaps.json as the "
            "SYNTHESIZED state's machine-readable evidence",
        )

    def test_synthesized_presupposes_reviewed_audited(self):
        # The SYNTHESIZED state presupposes REVIEWED+AUDITED — the
        # synthesizer refuses to run without both critic siblings. The
        # documentation must make this ordering explicit.
        self.assertRegex(
            self.text,
            r"REVIEWED\+AUDITED.*synthesis|synthesis.*REVIEWED\+AUDITED",
            "step 3 must document that SYNTHESIZED presupposes "
            "REVIEWED+AUDITED",
        )

    def test_synthesis_verdict_read(self):
        # Step 2 (read-side) must document that the orchestrator reads
        # the synthesis sibling's verdict — gap count + severity
        # breakdown — for an operator scanning the report.
        self.assertRegex(
            self.text,
            r"synthesis verdict.*gap count|gap count.*synthesis verdict|"
            r"synthesis verdict.*severity",
            "step 2 must read the synthesis sibling's verdict.md "
            "(gap count + severity breakdown) when present",
        )


# ---------------------------------------------------------------------------
# commands/proposal.md — recommendation dispatch table (Procedure step 4)
# ---------------------------------------------------------------------------


class TestProposalMdRecommendationTable(unittest.TestCase):
    """Step 4's dispatch table gains the SYNTHESIZED state rows."""

    def setUp(self):
        self.text = _read(_PROPOSAL_MD)

    def test_reviewed_audited_recommends_synthesize(self):
        # The AC: REVIEWED+AUDITED (either blocks, under iteration cap)
        # → proposal-synthesize <thread>. The recommendation must be
        # parseable as a table row.
        # Look for a table row that ties REVIEWED+AUDITED to
        # proposal-synthesize in the under-cap case.
        self.assertRegex(
            self.text,
            r"\|\s*`REVIEWED\+AUDITED`.*under iteration cap.*\|\s*`proposal-synthesize",
            "step 4 must have a dispatch row: REVIEWED+AUDITED "
            "(either blocks, under iteration cap) → proposal-synthesize <thread>",
        )

    def test_synthesized_recommends_revise(self):
        # The AC: SYNTHESIZED (either blocks, under iteration cap) →
        # proposal-revise <thread>.
        self.assertRegex(
            self.text,
            r"\|\s*`SYNTHESIZED`.*under iteration cap.*\|\s*`proposal-revise",
            "step 4 must have a dispatch row: SYNTHESIZED "
            "(either blocks, under iteration cap) → proposal-revise <thread>",
        )

    def test_synthesized_at_cap_blocks(self):
        # The AT-cap variant of SYNTHESIZED must surface as BLOCKED, just
        # like REVIEWED+AUDITED AT-cap surfaces as BLOCKED. The dispatch
        # table must cover both.
        self.assertRegex(
            self.text,
            r"\|\s*`SYNTHESIZED`.*AT iteration cap.*\|\s*`?BLOCKED",
            "step 4 must have a dispatch row: SYNTHESIZED "
            "(either blocks, AT iteration cap) → BLOCKED",
        )

    def test_reviewed_audited_at_cap_still_blocks(self):
        # Backward compat: the pre-synthesis row (REVIEWED+AUDITED at-cap
        # → BLOCKED) is preserved. This is the safety net for threads
        # that skip synthesis entirely.
        self.assertRegex(
            self.text,
            r"\|\s*`REVIEWED\+AUDITED`.*AT iteration cap.*\|\s*`?BLOCKED",
            "step 4 must preserve the REVIEWED+AUDITED (AT iteration cap) "
            "→ BLOCKED row for backward compatibility",
        )

    def test_fallback_path_preserved_prose(self):
        # The prose under the dispatch table must explicitly document
        # the fallback: a human operator skipping straight from
        # REVIEWED+AUDITED to proposal-revise is supported (the
        # reviser's per-sibling fallback path handles it).
        self.assertIn(
            "fall",
            self.text,
            "step 4 prose must mention the fallback path",
        )
        self.assertRegex(
            self.text,
            r"backward compat|backward-compat",
            "step 4 prose must explicitly preserve backward compatibility "
            "with the pre-synthesis direct REVIEWED+AUDITED → proposal-revise path",
        )

    def test_cross_reference_to_synthesize_md(self):
        # The orchestrator's documentation must cross-reference
        # proposal-synthesize.md (the writer-side contract) so an
        # operator reading proposal.md can find the synthesis command's
        # full specification.
        self.assertIn(
            "proposal-synthesize.md",
            self.text,
            "proposal.md must cross-reference proposal-synthesize.md",
        )


# ---------------------------------------------------------------------------
# commands/proposal.md — anomaly detection (Procedure step 5)
# ---------------------------------------------------------------------------


class TestProposalMdAnomalyDetection(unittest.TestCase):
    """Step 5's anomaly list surfaces stalled REVIEWED+AUDITED + no synthesis."""

    def setUp(self):
        self.text = _read(_PROPOSAL_MD)

    def test_stalled_reviewed_audited_anomaly(self):
        # The AC: a REVIEWED+AUDITED thread stalled with no synthesis
        # > 10 min must be surfaced as a recoverable phase. This is the
        # synthesis-aware variant of the crashed-phase signal.
        self.assertRegex(
            self.text,
            r"stalled in `REVIEWED\+AUDITED`.*no.*synthesis|stalled.*synthesis",
            "step 5 must surface a REVIEWED+AUDITED thread stalled "
            "without a synthesis sibling > 10 min as a recoverable phase",
        )

    def test_recoverable_phase_named(self):
        # The anomaly is keyed as a "recoverable phase" so the operator
        # knows it's actionable (not a crashed phase requiring deletion).
        self.assertIn(
            "recoverable phase",
            self.text,
            "step 5 must classify the stalled-before-synthesis anomaly "
            "as a 'recoverable phase'",
        )

    def test_ten_minute_threshold(self):
        # The 10-minute threshold matches the existing crashed-phase
        # anomaly's threshold, so the orchestrator's anomaly-detection
        # behavior is consistent.
        # Look for "10 minutes" in the synthesis-related anomaly span.
        self.assertRegex(
            self.text,
            r"synthesis.*10 minutes|10 minutes.*synthesis",
            "step 5's synthesis anomaly must use the same 10-minute "
            "threshold as the existing crashed-phase anomaly",
        )

    def test_crashed_synthesis_anomaly(self):
        # An in-progress synthesis phase that crashed mid-write is its
        # own anomaly. The orchestrator must surface it so the operator
        # can re-run after deleting partial output.
        self.assertRegex(
            self.text,
            r"synthesis.*in_progress|in_progress.*synthesis",
            "step 5 must surface a crashed synthesis sibling "
            "(synthesize.state == in_progress) as an anomaly",
        )


# ---------------------------------------------------------------------------
# commands/proposal.md — example output table
# ---------------------------------------------------------------------------


class TestProposalMdExampleOutput(unittest.TestCase):
    """The example output table shows the new SYNTHESIZED state row."""

    def setUp(self):
        self.text = _read(_PROPOSAL_MD)

    def test_example_includes_synthesized_row(self):
        # The example output table must include a SYNTHESIZED row so
        # operators reading the doc see the new state in action.
        # The state name must appear in a table cell context (between
        # pipes), not just in prose.
        self.assertRegex(
            self.text,
            r"\|\s*SYNTHESIZED\s*\|",
            "the example output table must include a row showing the "
            "SYNTHESIZED state",
        )

    def test_example_includes_synthesize_recommendation_in_table(self):
        # The example output table must show proposal-synthesize as the
        # recommended next command for at least one thread (the
        # REVIEWED+AUDITED row).
        self.assertRegex(
            self.text,
            r"\|\s*proposal-synthesize\s+\w[\w-]*\s*\|",
            "the example output table must include a row recommending "
            "proposal-synthesize <thread>",
        )


# ---------------------------------------------------------------------------
# SKILL.md — state machine diagram + evidence table
# ---------------------------------------------------------------------------


class TestSkillMdStateMachine(unittest.TestCase):
    """SKILL.md state-machine docs include the new SYNTHESIZED row."""

    def setUp(self):
        self.text = _read(_SKILL_MD)

    def test_state_machine_diagram_includes_synthesized(self):
        # The state-machine ASCII diagram must include SYNTHESIZED
        # between REVIEWED+AUDITED and REVISED. This is the visible
        # signal that the state-machine has expanded.
        self.assertRegex(
            self.text,
            r"REVIEWED\+AUDITED\s*→\s*SYNTHESIZED\s*→\s*REVISED",
            "SKILL.md state-machine diagram must include "
            "REVIEWED+AUDITED → SYNTHESIZED → REVISED",
        )

    def test_evidence_table_includes_synthesized_row(self):
        # The evidence table must have a SYNTHESIZED row keyed on
        # <thread>.{N}.synthesis/verdict.md + gaps.json existence at
        # the latest N. Match a table row where the first cell is
        # `SYNTHESIZED` and the row mentions both files.
        self.assertRegex(
            self.text,
            r"\|\s*`SYNTHESIZED`\s*\|[^|]*synthesis/verdict\.md[^|]*gaps\.json",
            "SKILL.md evidence table must include a SYNTHESIZED row "
            "naming both verdict.md and gaps.json",
        )

    def test_evidence_row_presupposes_reviewed_audited(self):
        # The row must document that SYNTHESIZED presupposes
        # REVIEWED+AUDITED. This is structurally important: the
        # synthesizer refuses to run without both critic siblings.
        # Extract the SYNTHESIZED row and check for the presupposition.
        synth_row_match = re.search(
            r"\|\s*`SYNTHESIZED`\s*\|[^\n]+",
            self.text,
        )
        self.assertIsNotNone(
            synth_row_match,
            "SKILL.md evidence table must have a SYNTHESIZED row",
        )
        row = synth_row_match.group(0)  # type: ignore[union-attr]
        self.assertIn(
            "REVIEWED+AUDITED",
            row,
            "the SYNTHESIZED evidence row must document that it "
            "presupposes REVIEWED+AUDITED",
        )

    def test_diagram_documents_synthesis_optional(self):
        # The diagram or its accompanying prose must document that
        # synthesis is the v0-recommended pre-revise step but optional
        # (the reviser falls back to per-sibling reading when absent).
        # This is the backward-compat safety net.
        self.assertRegex(
            self.text,
            r"falls? back|fallback",
            "SKILL.md state-machine section must document the "
            "fallback path when .synthesis/ is absent",
        )


# ---------------------------------------------------------------------------
# SKILL.md — command dispatch table
# ---------------------------------------------------------------------------


class TestSkillMdCommandDispatch(unittest.TestCase):
    """SKILL.md command-dispatch table includes a row for proposal-synthesize."""

    def setUp(self):
        self.text = _read(_SKILL_MD)

    def test_dispatch_includes_proposal_synthesize_row(self):
        # A row in the Command dispatch table for the synthesizer
        # command. Match a row whose first cell is `proposal-synthesize
        # <thread>`.
        self.assertRegex(
            self.text,
            r"\|\s*`proposal-synthesize <thread>`\s*\|",
            "SKILL.md command-dispatch table must include a row for "
            "proposal-synthesize <thread>",
        )

    def test_synthesize_row_writes_synthesis_sibling(self):
        # The Writes cell must name the synthesis sibling and the
        # documented output files (verdict.md, synthesis.md, gaps.json).
        # Pull the row and verify the structural properties.
        row_match = re.search(
            r"\|\s*`proposal-synthesize <thread>`\s*\|[^\n]+",
            self.text,
        )
        self.assertIsNotNone(
            row_match, "missing proposal-synthesize row"
        )
        row = row_match.group(0)  # type: ignore[union-attr]
        self.assertIn(
            "synthesis/",
            row,
            "proposal-synthesize row must write to <thread>.{N}.synthesis/",
        )
        for output_file in ("verdict.md", "synthesis.md", "gaps.json"):
            with self.subTest(output_file=output_file):
                self.assertIn(
                    output_file,
                    row,
                    f"proposal-synthesize row must name {output_file} "
                    "as a written output file",
                )

    def test_synthesize_row_reads_both_critic_siblings(self):
        # The Reads cell must name BOTH .review/ and .audit/ as REQUIRED
        # inputs, matching the synthesizer's documented precondition
        # (refuses to run without both critic siblings).
        row_match = re.search(
            r"\|\s*`proposal-synthesize <thread>`\s*\|[^\n]+",
            self.text,
        )
        self.assertIsNotNone(row_match)
        row = row_match.group(0)  # type: ignore[union-attr]
        self.assertIn(
            ".review/",
            row,
            "proposal-synthesize row must read <thread>.{N}.review/",
        )
        self.assertIn(
            ".audit/",
            row,
            "proposal-synthesize row must read <thread>.{N}.audit/",
        )

    def test_synthesize_row_marks_optional_non_gating(self):
        # The dispatch row must mark proposal-synthesize as optional /
        # non-gating so an operator reading the table sees that the
        # reviser still works without it.
        row_match = re.search(
            r"\|\s*`proposal-synthesize <thread>`\s*\|[^\n]+",
            self.text,
        )
        self.assertIsNotNone(row_match)
        row = row_match.group(0)  # type: ignore[union-attr]
        self.assertRegex(
            row,
            r"optional|non-gating|recommended",
            "proposal-synthesize row must mark the command as optional "
            "or non-gating (with reviser fallback)",
        )


# ---------------------------------------------------------------------------
# SKILL.md — brief synthesis-role subsection
# ---------------------------------------------------------------------------


class TestSkillMdSynthesisSubsection(unittest.TestCase):
    """SKILL.md introduces the synthesis role with a cross-reference."""

    def setUp(self):
        self.text = _read(_SKILL_MD)

    def test_synthesis_role_introduced(self):
        # The AC: a brief subsection introducing the synthesis role.
        # Look for the substantive description of what the synthesizer
        # does — consolidate cross-critic findings into a gap list.
        self.assertRegex(
            self.text,
            r"consolidat\w*\s+cross-critic|cross-critic\s+findings",
            "SKILL.md must introduce the synthesis role as consolidating "
            "cross-critic findings",
        )

    def test_synthesis_subsection_cross_references_command(self):
        # The subsection must cross-reference commands/proposal-synthesize.md
        # so a reader of SKILL.md can find the full command spec.
        self.assertIn(
            "commands/proposal-synthesize.md",
            self.text,
            "SKILL.md synthesis subsection must cross-reference "
            "commands/proposal-synthesize.md",
        )

    def test_synthesis_subsection_cross_references_schema(self):
        # The subsection must cross-reference the pydantic schema module
        # so a reader knows where the gaps.json contract lives.
        self.assertIn(
            "anvil/skills/proposal/lib/synthesis_schema.py",
            self.text,
            "SKILL.md synthesis subsection must cross-reference "
            "anvil/skills/proposal/lib/synthesis_schema.py",
        )

    def test_synthesis_non_gating_documented(self):
        # The subsection must document that the synthesis sibling is
        # non-gating — the reviser falls back to per-sibling reading
        # when absent. This is the backward-compat safety net the
        # rollout depends on.
        self.assertRegex(
            self.text,
            r"non-gating|falls? back|fallback",
            "SKILL.md synthesis subsection must document the non-gating "
            "/ fallback contract",
        )


# ---------------------------------------------------------------------------
# Cross-file consistency
# ---------------------------------------------------------------------------


class TestCrossFileConsistency(unittest.TestCase):
    """SKILL.md and proposal.md agree on the SYNTHESIZED state's evidence."""

    def test_both_files_name_synthesis_verdict_and_gaps_json(self):
        # The two files MUST agree on the evidence: SYNTHESIZED is
        # detected by the presence of verdict.md + gaps.json under
        # <thread>.{N}.synthesis/. If they drift, the orchestrator
        # implementation and the state-machine doc would disagree.
        proposal_md = _read(_PROPOSAL_MD)
        skill_md = _read(_SKILL_MD)
        for path in ("synthesis/verdict.md", "synthesis/gaps.json"):
            with self.subTest(path=path):
                self.assertIn(
                    path,
                    proposal_md,
                    f"proposal.md must name {path} as evidence",
                )
                self.assertIn(
                    path,
                    skill_md,
                    f"SKILL.md must name {path} as evidence",
                )

    def test_both_files_recognize_synthesized_state(self):
        # Both files MUST use the same state name (`SYNTHESIZED`),
        # not variants like `SYNTHESIS-DONE` or `SYNTHESIS_COMPLETE`.
        proposal_md = _read(_PROPOSAL_MD)
        skill_md = _read(_SKILL_MD)
        self.assertIn(
            "SYNTHESIZED",
            proposal_md,
            "proposal.md must use the canonical state name SYNTHESIZED",
        )
        self.assertIn(
            "SYNTHESIZED",
            skill_md,
            "SKILL.md must use the canonical state name SYNTHESIZED",
        )

    def test_both_files_preserve_backward_compat(self):
        # Both files MUST document the backward-compat path: when
        # synthesis/ is absent, the orchestrator (and the reviser) still
        # function. This is the rollout safety net.
        proposal_md = _read(_PROPOSAL_MD)
        skill_md = _read(_SKILL_MD)
        self.assertRegex(
            proposal_md,
            r"backward compat|backward-compat|falls? back|fallback",
            "proposal.md must preserve the backward-compat path",
        )
        self.assertRegex(
            skill_md,
            r"backward compat|backward-compat|falls? back|fallback",
            "SKILL.md must preserve the backward-compat path",
        )


# ---------------------------------------------------------------------------
# Schema module sanity (cross-check with sub-issue 1 + 2)
# ---------------------------------------------------------------------------


class TestSchemaModuleStillImportable(unittest.TestCase):
    """The pydantic schema module named by SKILL.md is importable.

    SKILL.md cross-references ``anvil/skills/proposal/lib/synthesis_schema.py``
    as the canonical gaps.json contract. This test pins that the module
    is actually importable, so the cross-reference is not a dangling
    pointer — mirrors the safety-net check in
    ``test_revise_consumes_synthesis.py``.
    """

    def test_synthesis_schema_module_imports(self):
        from anvil.skills.proposal.lib.synthesis_schema import (
            GapList,
            SCHEMA_VERSION,
        )

        self.assertEqual(SCHEMA_VERSION, "1")
        # GapList must accept the minimum-required field.
        gl = GapList(for_version=1)
        self.assertEqual(gl.schema_version, "1")


if __name__ == "__main__":
    unittest.main()
