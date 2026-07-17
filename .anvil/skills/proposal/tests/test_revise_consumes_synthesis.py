"""Reviser-side consumption tests for the synthesis ``gaps.json`` contract.

This module pins the **documented contract** that ``proposal-revise.md``
prefers the synthesis sibling's ``gaps.json`` over walking per-sibling
critic findings, with a backward-compatibility fallback when no
``synthesis/`` sibling is present. The contract is what sub-issue 2 of
#246 actually ships — a documentation change to the reviser command file
and a test that the documentation says what it has to say.

Scope is intentionally narrow — orchestrator + state-machine integration
in ``commands/proposal.md`` + ``SKILL.md`` is sub-issue 3; the Studio
reproducer integration test is sub-issue 4. This module asserts only the
prose-level structural properties of ``proposal-revise.md`` Procedure
steps 6, 7, and 9 (the changelog row format).

Contracts pinned by this module:

1. **Step 6 documents discover + prefer + fallback**: the step names
   ``synthesis/gaps.json`` as the preferred source, names the pydantic
   schema module the reviser validates against, and documents the
   absent/invalid fallback to per-sibling reading.
2. **Step 7 documents the gap + singleton walk**: the step walks
   ``gaps`` (one coordinated response per gap) and ``singletons`` (one
   response per finding) instead of walking per-critic findings, and
   names critical / blocker / should-fix / nice-to-have as the severity
   ordering for the gap path.
3. **Step 9 documents the gap-ID + contributing-findings row shape**:
   the canonical multi-contributor row format (``synthesis <gap-id>
   (<sibling>.<ref>, ...)``) is shown in the documented changelog table,
   and the per-sibling fallback row format
   (``<thread>.<N>.<sibling> (<severity>)``) is preserved unchanged.
4. **Fallback path preservation**: the per-sibling reading path
   continues to discover the audit findings file via the documented
   tolerant filename aliases (``findings.md`` /  ``claim-log.md`` /
   ``audit-findings.md``), and the pre-synthesis changelog row format
   still appears in the file verbatim for threads with no
   ``synthesis/`` sibling.

The module filename is deliberately distinct
(``test_revise_consumes_synthesis``) per the #58 packaging convention to
avoid the cross-skill pytest collection collision.

Runs under either ``pytest anvil/skills/proposal/tests/`` or
``python -m unittest discover anvil/skills/proposal/tests/``.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path


_SKILL_ROOT = Path(__file__).resolve().parent.parent
_REVISE_PATH = _SKILL_ROOT / "commands" / "proposal-revise.md"


def _read_revise() -> str:
    return _REVISE_PATH.read_text(encoding="utf-8")


# Procedure-step extraction. proposal-revise.md uses a hand-rolled
# numbered list (``1. **Discover state**``, ``2. **Resume check**``, ...)
# so the test parses each top-level numbered step into its own block.
# Sub-bullets stay attached to their parent step. The next top-level
# numbered marker terminates the current block.
_STEP_OPEN_RE = re.compile(r"^(\d+)\.\s+\*\*")


def _extract_steps(text: str) -> dict:
    """Split the ``## Procedure`` section into ``{step_number: text}``.

    Top-level numbered markers (``1. **Discover state**``) open a step;
    the step ends at the next top-level numbered marker or the next
    ``## `` section heading. Sub-bullets and code fences stay attached
    to the open step.
    """

    lines = text.splitlines()
    # Locate the Procedure section.
    in_procedure = False
    procedure_lines: list[str] = []
    for line in lines:
        if line.strip() == "## Procedure":
            in_procedure = True
            continue
        if in_procedure and line.startswith("## "):
            # Next top-level section ends Procedure.
            break
        if in_procedure:
            procedure_lines.append(line)

    steps: dict = {}
    current_step: int | None = None
    current_block: list[str] = []
    for line in procedure_lines:
        m = _STEP_OPEN_RE.match(line)
        if m:
            if current_step is not None:
                steps[current_step] = "\n".join(current_block)
            current_step = int(m.group(1))
            current_block = [line]
        elif current_step is not None:
            current_block.append(line)
    if current_step is not None:
        steps[current_step] = "\n".join(current_block)
    return steps


class TestStep6DiscoversSynthesis(unittest.TestCase):
    """Step 6 documents discover + prefer + fallback for ``gaps.json``."""

    def setUp(self):
        self.steps = _extract_steps(_read_revise())
        self.assertIn(6, self.steps, "proposal-revise.md missing step 6")
        self.step6 = self.steps[6]

    def test_step6_names_synthesis_gaps_json(self):
        # The step MUST name the synthesis sibling's gaps.json path so
        # the reviser knows where to look.
        self.assertIn(
            "<thread>.{N}.synthesis/gaps.json",
            self.step6,
            "step 6 must name <thread>.{N}.synthesis/gaps.json as the "
            "synthesis sibling input",
        )

    def test_step6_documents_prefer(self):
        # The contract is that gaps.json is the PREFERRED source when
        # present (not just one of several sources).
        self.assertRegex(
            self.step6,
            r"prefer\b",
            "step 6 must document that gaps.json is preferred over "
            "per-sibling reading when present",
        )

    def test_step6_documents_fallback(self):
        # The fallback path is the rollout safety net — when gaps.json
        # is absent (or schema-invalid), the reviser MUST fall back to
        # per-sibling finding reading.
        self.assertIn(
            "fall back",
            self.step6,
            "step 6 must document the fallback path when gaps.json is absent",
        )
        # The fallback must explicitly cover the "absent" case.
        self.assertIn(
            "absent",
            self.step6,
            "step 6 must describe the fallback for the absent case",
        )

    def test_step6_documents_schema_validation_fallback(self):
        # The fallback also fires when gaps.json exists but fails
        # schema validation — this is the safety net for a partially
        # written or corrupted file.
        self.assertRegex(
            self.step6,
            r"(schema[- ]?valid|schema validation)",
            "step 6 must document the schema-validation fallback "
            "case (gaps.json present but invalid)",
        )

    def test_step6_names_pydantic_schema_module(self):
        # The reviser validates against the pinned pydantic schema
        # module, not an ad-hoc parser.
        self.assertIn(
            "anvil/skills/proposal/lib/synthesis_schema.py",
            self.step6,
            "step 6 must name the pydantic schema module the reviser "
            "validates gaps.json against",
        )

    def test_step6_per_sibling_reading_preserved(self):
        # The pre-synthesis per-sibling reading path is preserved
        # verbatim as the fallback — this is the backward-compat
        # safety net documented in proposal-synthesize.md.
        self.assertIn(
            "<thread>.{N}.review/verdict.md",
            self.step6,
            "step 6 fallback path must still read the review sibling's verdict.md",
        )
        self.assertIn(
            "<thread>.{N}.audit/verdict.md",
            self.step6,
            "step 6 fallback path must still read the audit sibling's verdict.md",
        )

    def test_step6_tolerant_findings_filename_preserved(self):
        # The audit findings tolerant-read aliases (findings.md /
        # claim-log.md / audit-findings.md) are preserved on the
        # fallback path — this is the existing #135 / #255 contract.
        for canonical in ("findings.md", "claim-log.md", "audit-findings.md"):
            with self.subTest(filename=canonical):
                self.assertIn(
                    canonical,
                    self.step6,
                    f"step 6 fallback path must still document {canonical} "
                    "as a tolerant-read audit findings filename",
                )


class TestStep7WalksGapsAndSingletons(unittest.TestCase):
    """Step 7 documents the gap + singleton walk on the synthesis path."""

    def setUp(self):
        self.steps = _extract_steps(_read_revise())
        self.assertIn(7, self.steps, "proposal-revise.md missing step 7")
        self.step7 = self.steps[7]

    def test_step7_walks_gaps(self):
        # The synthesis path walks `gaps` (each with a single
        # coordinated response) instead of walking per-critic findings.
        self.assertRegex(
            self.step7,
            r"\bGap\b|`gaps`|walk\s+`?gaps`?",
            "step 7 must document walking gaps from gaps.json",
        )
        # And explicitly NOT layering per-contributing-finding responses.
        self.assertIn(
            "Do NOT layer",
            self.step7,
            "step 7 must say the reviser does NOT layer multiple "
            "responses per contributing finding",
        )

    def test_step7_walks_singletons(self):
        # The singleton path keeps the "one finding, one response"
        # framing for findings that did not cluster.
        self.assertRegex(
            self.step7,
            r"\bSingleton\b|`singletons`",
            "step 7 must document walking singletons from gaps.json",
        )
        self.assertIn(
            "one finding, one response",
            self.step7,
            "step 7 must preserve the 'one finding, one response' "
            "framing for singletons",
        )

    def test_step7_recommended_response_drives_plan(self):
        # The gap-level recommended_response is the single coordinated
        # response per gap — this is the structural fix for #246.
        self.assertIn(
            "recommended_response",
            self.step7,
            "step 7 must name `recommended_response` as the gap's "
            "single coordinated response field",
        )

    def test_step7_severity_ordering(self):
        # Critical first, then blocker, then should-fix, then
        # nice-to-have. This is the AC's severity ordering requirement.
        # The ordering should be documented in one contiguous span so
        # the reviser sees the ladder, not scattered mentions.
        for sev in ("critical", "blocker", "should-fix", "nice-to-have"):
            with self.subTest(severity=sev):
                self.assertIn(
                    sev,
                    self.step7,
                    f"step 7 must name '{sev}' in the severity ordering",
                )

    def test_step7_critical_first(self):
        # The AC says: "gaps with severity: critical are addressed
        # first". This MUST be explicit, not implied.
        self.assertRegex(
            self.step7,
            r"critical.*first|critical.*are addressed first",
            "step 7 must document that critical gaps are addressed first",
        )

    def test_step7_fallback_path_preserved(self):
        # The pre-synthesis filter logic (comments.md severity buckets
        # blocker / major / minor / nit) is preserved on the fallback
        # path so existing in-flight threads continue to work.
        for bucket in ("blocker", "major", "minor", "nit"):
            with self.subTest(severity_bucket=bucket):
                self.assertIn(
                    bucket,
                    self.step7,
                    f"step 7 fallback path must preserve the '{bucket}' "
                    "comments.md severity bucket",
                )


class TestStep9ChangelogFormat(unittest.TestCase):
    """Step 9 documents the gap-ID + contributing-findings row format."""

    def setUp(self):
        self.text = _read_revise()
        self.steps = _extract_steps(self.text)
        self.assertIn(9, self.steps, "proposal-revise.md missing step 9")
        self.step9 = self.steps[9]

    def test_step9_documents_synthesis_row_shape(self):
        # The canonical row format names the gap ID first, then the
        # contributing-finding refs as `<sibling>.<ref>` tokens inside
        # parentheses. This is the AC's "Source: synthesis g-12lp-mask-cost
        # (review.dim6.comment.3, audit.findings.12lp_line,
        # perspective.candidates.cluster_foundry_pricing)" shape.
        self.assertIn(
            "g-12lp-mask-cost",
            self.step9,
            "step 9 must show the canonical 12LP+ canary gap ID in "
            "the documented synthesis-source row format",
        )
        # And the three contributing-finding refs from the canary case.
        for ref in (
            "review.dim6.comment.3",
            "audit.findings.12lp_line",
            "perspective.candidates.cluster_foundry_pricing",
        ):
            with self.subTest(ref=ref):
                self.assertIn(
                    ref,
                    self.step9,
                    f"step 9 must include the contributing-finding "
                    f"ref '{ref}' in the documented row format",
                )

    def test_step9_synthesis_token_present(self):
        # The row is keyed by the literal `synthesis <gap-id>` prefix
        # so downstream tooling can grep changelogs for synthesis-
        # sourced rows vs. per-sibling-fallback rows.
        self.assertRegex(
            self.step9,
            r"synthesis\s+g-",
            "step 9 must show the literal 'synthesis g-<id>' source "
            "prefix in the documented row format",
        )

    def test_step9_per_sibling_fallback_format_preserved(self):
        # The pre-synthesis row format
        # `<thread>.<N>.<sibling> (<severity>)` is preserved unchanged
        # for the fallback path. The acceptance criteria say: "When
        # falling back to per-sibling reading, the existing row format
        # is preserved unchanged."
        # The canonical examples in the file use `gossamer-lan.1.audit`
        # / `gossamer-lan.1.review` — those exact tokens must survive
        # so existing example threads keep working.
        for source in (
            "gossamer-lan.1.audit",
            "gossamer-lan.1.review",
        ):
            with self.subTest(source=source):
                self.assertIn(
                    source,
                    self.step9,
                    f"step 9 must preserve the per-sibling fallback "
                    f"row format example using '{source}'",
                )

    def test_step9_severity_parenthetical_preserved(self):
        # The per-sibling fallback row carries a parenthetical severity
        # suffix (`(critical)`, `(blocker)`, `(major)`, etc.).
        # Preserve these tokens so the fallback row shape stays
        # bit-for-bit compatible with prior changelogs.
        for sev_tag in ("(critical)", "(blocker)", "(major)"):
            with self.subTest(severity_tag=sev_tag):
                self.assertIn(
                    sev_tag,
                    self.step9,
                    f"step 9 fallback row format must preserve the "
                    f"'{sev_tag}' severity parenthetical",
                )

    def test_step9_deferred_section_preserved(self):
        # The `Deferred to next iteration (scope: <level>)` section
        # is the operator's TODO signal — it MUST be preserved across
        # both source formats.
        self.assertIn(
            "Deferred to next iteration",
            self.step9,
            "step 9 must preserve the 'Deferred to next iteration' "
            "section header",
        )

    def test_step9_declined_resolution_preserved(self):
        # `Resolution: declined — <one-line reason>` is the convention
        # for findings the reviser disagrees with. The AC implies this
        # convention applies under both source formats.
        self.assertRegex(
            self.step9,
            r"declined\b",
            "step 9 must preserve the 'declined' resolution convention",
        )


class TestFallbackPathPreservedVerbatim(unittest.TestCase):
    """A thread with no synthesis sibling reads identically to the pre-synthesis behavior."""

    def setUp(self):
        self.text = _read_revise()

    def test_audit_findings_tolerant_read_intact(self):
        # The #135 / #255 tolerant-read contract for the audit findings
        # filename — findings.md, claim-log.md, audit-findings.md — is
        # part of the per-sibling reading path and MUST survive the
        # synthesis layer addition.
        for canonical in (
            "<thread>.{N}.audit/findings.md",
            "<thread>.{N}.audit/claim-log.md",
            "<thread>.{N}.audit/audit-findings.md",
        ):
            with self.subTest(path=canonical):
                self.assertIn(
                    canonical,
                    self.text,
                    f"fallback path must still document {canonical} "
                    "as a tolerant-read candidate",
                )

    def test_review_sibling_reading_preserved(self):
        # The reviewer's three deliverables (verdict / scoring /
        # comments) are still read on the fallback path.
        for rel in (
            "<thread>.{N}.review/verdict.md",
            "scoring.md",
            "comments.md",
        ):
            with self.subTest(file=rel):
                self.assertIn(
                    rel,
                    self.text,
                    f"fallback path must still read the reviewer's '{rel}'",
                )

    def test_other_critics_sibling_reading_preserved(self):
        # The "every other <thread>.{N}.<critic>/ sibling discovered
        # on disk" sentence is the catch-all for opt-in domain critics.
        # The fallback path keeps this surface area.
        self.assertIn(
            "Every other `<thread>.{N}.<critic>/` sibling",
            self.text,
            "fallback path must still document the catch-all "
            "for opt-in <critic>/ siblings",
        )

    def test_per_sibling_changelog_example_preserved(self):
        # The four canonical example rows (audit critical / audit
        # major / review blocker / review major) are preserved
        # verbatim. This is the AC's "preserved unchanged" property.
        for example_note in (
            "Materials subtotal off by $200 (sum mismatch)",
            "Transceiver qty 14 but topology has 7 spokes",
            'Design proposes surface raceway — violates "no conduit"',
            "Deliverability story is a contractor phone number",
        ):
            with self.subTest(note=example_note):
                self.assertIn(
                    example_note,
                    self.text,
                    "fallback path changelog example MUST be preserved "
                    f"verbatim: missing example note '{example_note}'",
                )

    def test_critical_invariants_preserved(self):
        # The "Critical flags MUST be addressed" pathway is preserved
        # under both source formats — `--scope critical-only` does NOT
        # skip critical-flag handling. This is the structural
        # invariant from the §"CLI flags" critical invariants block.
        self.assertIn(
            "Audit-critical-flag and review-critical-flag findings",
            self.text,
            "the critical-flag-must-address invariant must survive "
            "the synthesis layer addition",
        )

    def test_sub_threshold_dimension_lifts_preserved(self):
        # Sub-threshold dimension lifts are independent of comment
        # severity AND of synthesis source — they always go into the
        # revision plan regardless of `--scope` and regardless of
        # whether gaps.json was present.
        self.assertRegex(
            self.text,
            r"sub-threshold dimension lifts",
            "the sub-threshold-dimension-lift invariant must survive "
            "the synthesis layer addition",
        )


class TestSchemaImportableFromReviserPath(unittest.TestCase):
    """The reviser's named pydantic schema module is importable.

    The reviser command file names ``anvil/skills/proposal/lib/
    synthesis_schema.py`` as the validation target. This test pins
    that the module is actually importable from the test harness so
    the reviser's documented path is not a dangling reference.
    """

    def test_synthesis_schema_module_imports(self):
        # If sub-issue 1 of #246 (already merged at aa6a37e) regresses
        # the module path, this test catches it before the reviser
        # tries to validate at runtime.
        from anvil.skills.proposal.lib.synthesis_schema import (
            GapList,
            SCHEMA_VERSION,
        )

        self.assertEqual(SCHEMA_VERSION, "1")
        # GapList must be instantiable with the minimum required field.
        gl = GapList(for_version=1)
        self.assertEqual(gl.schema_version, "1")
        self.assertEqual(gl.gaps, [])
        self.assertEqual(gl.singletons, [])


if __name__ == "__main__":
    unittest.main()
