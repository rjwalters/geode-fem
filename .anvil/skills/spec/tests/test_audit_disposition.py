"""Disposition-discrimination fixtures for ``spec-audit`` (issue #707).

The acceptance-criteria centerpiece of Phase 2: four synthetic spec-vs-code
fixture scenarios modeled directly on the botho near-miss (a spec almost
rewritten to canonize a vestigial code path contradicting an accepted ADR).
Each fixture encodes one bucket of the three-way ``implementation_contradicts_spec``
verdict — plus the fourth, most-subtle variant (an intentional gap WITHOUT a
register entry), which is the exact shape the class exists to prevent from
masquerading as either a defect or a non-issue.

``spec-audit`` is an LLM-driven command doc, not a pure-Python function, so
these tests do NOT execute the audit. Instead they:

1. **Assert the fixtures are well-formed** — each fixture pair carries the
   ground-truth signal the auditor is documented to key on (a bare stale
   constant with no ADR for spec-wrong; an ADR/``## Decisions`` marker plus a
   vestigial code path for code-wrong; a populated ``## Implementation status``
   register row for the registered intentional gap; the SAME target-state
   claim with NO register row for the unregistered near-miss variant).
2. **Assert the machine-checkable disposition surface** —
   ``_summary.md.spec_consistency.disposition_counts`` — is documented with a
   concrete, stable shape, so a future deterministic checker (or a human
   reviewing CI output) can assert on the auditor's classification. Each
   fixture declares its EXPECTED ``disposition_counts`` and the test verifies
   those expectations are internally consistent with the documented accounting
   rules in ``spec-audit.md`` (``contradictions = spec_wrong + code_wrong +
   unregistered``; ``unregistered <= intentional_gap``).

The load-bearing assertion is fixture (2): a ``code-wrong`` case must NEVER
produce a spec-edit recommendation (``expected_spec_edit`` is ``False`` and the
expected disposition is ``code-wrong``, routing to operator escalation).

Per the #58 packaging convention this filename (``test_audit_disposition``)
is unique across the ``anvil/skills/*/tests/`` tree.
"""

from __future__ import annotations

import re
import unittest
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, Optional

_SKILL_ROOT = Path(__file__).resolve().parent.parent


def _read(rel: str) -> str:
    return (_SKILL_ROOT / rel).read_text(encoding="utf-8")


@dataclass
class DispositionFixture:
    """One synthetic spec-vs-code scenario keyed to a single disposition."""

    name: str
    #: The normative spec claim under audit (verbatim, as it would appear in
    #: the LaTeX body).
    spec_claim: str
    #: The contradicting implementation span (verbatim, as it would appear in
    #: the resolved code_ref).
    code_span: str
    #: A ratified-decision marker backing the spec claim (an ADR ref / a
    #: ``## Decisions`` section), or ``None`` when the claim has no backing.
    adr_backing: Optional[str]
    #: The ``## Implementation status`` register row covering the claim, or
    #: ``None`` when no row exists.
    register_row: Optional[str]
    #: Whether the contradicting code reads as a vestigial / dead path.
    code_is_vestigial: bool
    #: The disposition the auditor is documented to reach.
    expected_disposition: str
    #: Whether an operator-escalation block is expected in verdict.md.
    expected_escalation: bool
    #: Whether the auditor may recommend editing the SPEC (True only for
    #: spec-wrong). MUST be False for code-wrong (the near-miss guard).
    expected_spec_edit: bool
    #: The expected _summary.md.spec_consistency.disposition_counts delta this
    #: single fixture contributes.
    expected_counts: Dict[str, int] = field(default_factory=dict)


# --- The four fixtures (three dispositions + the 3b unregistered variant) ----

FIX_SPEC_WRONG = DispositionFixture(
    name="spec-wrong (stale constant, no ADR)",
    # Spec says 3s; the code and every other spec section say 5s; no ADR
    # ratifies 3s → the spec claim is simply stale.
    spec_claim=r"The block-time floor \textbf{MUST} be 3\,s.",
    code_span="const BLOCK_TIME_FLOOR_SECS: u64 = 5;",
    adr_backing=None,  # no ratified decision defends the 3s claim
    register_row=None,  # not a target-state gap — just wrong
    code_is_vestigial=False,  # the 5s code is live, correct, intentional
    expected_disposition="spec-wrong",
    expected_escalation=False,
    expected_spec_edit=True,  # fix the spec claim to match the code
    expected_counts={
        "spec_wrong": 1,
        "code_wrong": 0,
        "intentional_gap": 0,
        "unregistered": 0,
    },
)

FIX_CODE_WRONG = DispositionFixture(
    name="code-wrong (ADR-backed spec vs vestigial code path)",
    # The spec claim is backed by an accepted ADR; the code contradicts it via
    # a vestigial/dead path → the CODE is wrong. This is the botho near-miss.
    spec_claim=(
        r"Per ADR-014, all output signatures \textbf{MUST} use domain "
        r"separation tag \texttt{0x02}."
    ),
    code_span="const SIG_DOMAIN_TAG: u8 = 0x01; // TODO: legacy, unused?",
    adr_backing="ADR-014 (accepted) in refs/adr-014-domain-separation.md",
    register_row=None,  # NOT a target-state gap — the code is defective
    code_is_vestigial=True,  # the 0x01 tag is a dead/legacy path
    expected_disposition="code-wrong",
    expected_escalation=True,  # operator escalation, blocks advance
    expected_spec_edit=False,  # NEVER rewrite the spec toward the code
    expected_counts={
        "spec_wrong": 0,
        "code_wrong": 1,
        "intentional_gap": 0,
        "unregistered": 0,
    },
)

FIX_INTENTIONAL_GAP_REGISTERED = DispositionFixture(
    name="intentional-gap (registered target-state)",
    # The spec describes target-state behavior the live code does not yet
    # implement, WITH a correctly-populated register row → suppressed.
    spec_claim=(
        r"All outputs \textbf{MUST} carry an ML-DSA-65 post-quantum signature."
    ),
    code_span="fn sign(msg: &[u8]) -> Ed25519Sig { /* classical */ }",
    adr_backing=None,
    register_row=(
        "| Output signatures | Ed25519 (classical) | ML-DSA-65 on all "
        "outputs | target-state | botho#902 |"
    ),
    code_is_vestigial=False,  # the classical path is the accepted LIVE state
    expected_disposition="intentional-gap",
    expected_escalation=False,  # registered → suppressed, no escalation
    expected_spec_edit=False,  # neither edit the spec nor the code
    expected_counts={
        "spec_wrong": 0,
        "code_wrong": 0,
        "intentional_gap": 1,  # counted (registered + unregistered both count)
        "unregistered": 0,  # it IS registered
    },
)

FIX_INTENTIONAL_GAP_UNREGISTERED = DispositionFixture(
    name="intentional-gap UNREGISTERED (the near-miss shape)",
    # Same target-state claim as above, but with NO register row → must be
    # flagged as unregistered: NOT silently passed, NOT escalated as
    # code-wrong, NOT auto-fixed as spec-wrong.
    spec_claim=(
        r"All outputs \textbf{MUST} carry an ML-DSA-65 post-quantum signature."
    ),
    code_span="fn sign(msg: &[u8]) -> Ed25519Sig { /* classical */ }",
    adr_backing=None,
    register_row=None,  # THE near-miss: an intentional gap with no register row
    code_is_vestigial=False,
    expected_disposition="intentional-gap",
    expected_escalation=False,  # the fix is a register-add, not code fix
    expected_spec_edit=False,  # do NOT rewrite the spec claim
    expected_counts={
        "spec_wrong": 0,
        "code_wrong": 0,
        "intentional_gap": 1,
        "unregistered": 1,  # flagged: the subtle near-miss bucket
    },
)

ALL_FIXTURES = [
    FIX_SPEC_WRONG,
    FIX_CODE_WRONG,
    FIX_INTENTIONAL_GAP_REGISTERED,
    FIX_INTENTIONAL_GAP_UNREGISTERED,
]


class TestFixturesAreWellFormed(unittest.TestCase):
    """Each fixture carries the ground-truth signal its disposition keys on."""

    def test_all_four_dispositions_are_represented(self):
        dispositions = {f.expected_disposition for f in ALL_FIXTURES}
        self.assertEqual(
            dispositions, {"spec-wrong", "code-wrong", "intentional-gap"}
        )
        # The two intentional-gap fixtures differ only in registration.
        gap = [
            f for f in ALL_FIXTURES if f.expected_disposition == "intentional-gap"
        ]
        self.assertEqual(len(gap), 2)
        self.assertEqual(gap[0].spec_claim, gap[1].spec_claim)
        self.assertNotEqual(
            gap[0].expected_counts["unregistered"],
            gap[1].expected_counts["unregistered"],
            "the registered vs unregistered gap fixtures must differ on the "
            "unregistered count — that IS the discrimination under test",
        )

    def test_spec_wrong_has_no_adr_and_no_register_row(self):
        f = FIX_SPEC_WRONG
        self.assertIsNone(f.adr_backing, "spec-wrong must have no ratified ADR")
        self.assertIsNone(f.register_row)
        self.assertFalse(f.code_is_vestigial, "spec-wrong code is the live truth")

    def test_code_wrong_has_adr_backing_and_vestigial_code(self):
        f = FIX_CODE_WRONG
        self.assertIsNotNone(
            f.adr_backing, "code-wrong requires a ratified-decision marker"
        )
        self.assertTrue(
            f.code_is_vestigial, "code-wrong code reads as vestigial drift"
        )
        self.assertIsNone(f.register_row, "code-wrong is not a target-state gap")

    def test_registered_gap_has_a_target_state_register_row(self):
        f = FIX_INTENTIONAL_GAP_REGISTERED
        self.assertIsNotNone(f.register_row)
        self.assertIn("target-state", f.register_row)

    def test_unregistered_gap_is_the_registered_gap_minus_its_row(self):
        reg = FIX_INTENTIONAL_GAP_REGISTERED
        unreg = FIX_INTENTIONAL_GAP_UNREGISTERED
        # Identical spec claim + code, differing ONLY in the register row.
        self.assertEqual(reg.spec_claim, unreg.spec_claim)
        self.assertEqual(reg.code_span, unreg.code_span)
        self.assertIsNotNone(reg.register_row)
        self.assertIsNone(unreg.register_row)


class TestDispositionRoutingInvariants(unittest.TestCase):
    """The load-bearing routing invariants per fixture (the near-miss guard)."""

    def test_code_wrong_never_recommends_a_spec_edit(self):
        # The single most important assertion in this phase: a code-wrong
        # case must NEVER route to a spec edit.
        self.assertFalse(
            FIX_CODE_WRONG.expected_spec_edit,
            "a code-wrong finding must NEVER produce a spec-edit recommendation "
            "— that is the botho near-miss",
        )
        self.assertTrue(
            FIX_CODE_WRONG.expected_escalation,
            "a code-wrong finding must route to operator escalation",
        )

    def test_only_spec_wrong_permits_a_spec_edit(self):
        for f in ALL_FIXTURES:
            with self.subTest(fixture=f.name):
                if f.expected_disposition == "spec-wrong":
                    self.assertTrue(f.expected_spec_edit)
                else:
                    self.assertFalse(
                        f.expected_spec_edit,
                        f"{f.name}: only spec-wrong may edit the spec",
                    )

    def test_registered_gap_does_not_escalate_unregistered_does_get_flagged(self):
        self.assertFalse(FIX_INTENTIONAL_GAP_REGISTERED.expected_escalation)
        # The unregistered gap is flagged (unregistered count == 1) even though
        # it does not need an operator escalation — its fix is a register-add.
        self.assertEqual(
            FIX_INTENTIONAL_GAP_UNREGISTERED.expected_counts["unregistered"], 1
        )
        self.assertEqual(
            FIX_INTENTIONAL_GAP_REGISTERED.expected_counts["unregistered"], 0
        )


class TestDispositionCountsAccounting(unittest.TestCase):
    """Each fixture's expected_counts obey the documented accounting rules."""

    def test_unregistered_never_exceeds_intentional_gap(self):
        for f in ALL_FIXTURES:
            with self.subTest(fixture=f.name):
                c = f.expected_counts
                self.assertLessEqual(
                    c["unregistered"],
                    c["intentional_gap"],
                    "unregistered is a subset of intentional_gap",
                )

    def test_contradictions_formula_holds_for_the_aggregate(self):
        # spec-audit.md documents: contradictions = spec_wrong + code_wrong +
        # unregistered (register-suppressed intentional gaps are clean passes,
        # NOT blocking contradictions).
        agg = {"spec_wrong": 0, "code_wrong": 0, "intentional_gap": 0, "unregistered": 0}
        for f in ALL_FIXTURES:
            for k in agg:
                agg[k] += f.expected_counts[k]
        contradictions = (
            agg["spec_wrong"] + agg["code_wrong"] + agg["unregistered"]
        )
        # Across the four fixtures: 1 spec-wrong + 1 code-wrong + 1 unregistered
        # = 3 blocking contradictions; the registered gap is a clean pass.
        self.assertEqual(agg["spec_wrong"], 1)
        self.assertEqual(agg["code_wrong"], 1)
        self.assertEqual(agg["intentional_gap"], 2)  # registered + unregistered
        self.assertEqual(agg["unregistered"], 1)
        self.assertEqual(contradictions, 3)


class TestSummaryContractIsConcrete(unittest.TestCase):
    """The disposition surface these fixtures assert on is documented as a
    stable _summary.md contract in spec-audit.md (so a future deterministic
    checker can assert on it, per the issue's test-plan note)."""

    def test_disposition_counts_keys_match_the_command_doc(self):
        doc = _read("commands/spec-audit.md")
        self.assertIn("disposition_counts", doc)
        for key in FIX_SPEC_WRONG.expected_counts:
            with self.subTest(key=key):
                self.assertIn(key, doc)

    def test_command_doc_documents_the_contradictions_accounting(self):
        doc = _read("commands/spec-audit.md")
        # The formula that TestDispositionCountsAccounting asserts on is stated
        # in the command doc, not invented here.
        self.assertTrue(
            re.search(
                r"contradictions.{0,40}spec_wrong.{0,20}code_wrong.{0,20}unregistered",
                doc,
            ),
            "spec-audit.md must document the contradictions accounting formula",
        )
        self.assertIn("unregistered <= intentional_gap", doc)


if __name__ == "__main__":
    unittest.main()
