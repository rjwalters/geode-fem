"""Doc-coverage smoke tests for the ``deck-audit`` generative-imagery
findings shipped under Epic #130 Phase 3G (issue #188).

The Phase 3G acceptance criteria are documentation-coverage assertions
plus a regression guard against silent activation on deterministic decks.
Per the issue body, the three findings (``unattributed-generative-imagery``,
``prompt-claim-divergence``, ``style-incoherence``) MUST:

1. Each be documented in ``commands/deck-audit.md`` with severity and
   detection logic.
2. Be gated behind ``imagery_policy: generative-eligible`` — i.e., the
   doc must explicitly say the checks DO NOT RUN on deterministic-only
   or absent-policy decks. This is the load-bearing zero-behavior-change
   guarantee.
3. Document a suppression directive of the form
   ``<!-- anvil-audit-disable: <finding-name> -->`` per the existing
   marp-lint escape-hatch convention.

These are substring-presence assertions (no LLM call, no parsing of the
actual auditor runtime), following the precedent of
``test_imagery_policy_docs.py`` (#132's prose-spec doc-coverage tests).
Distinct filename per the #58 packaging convention to avoid cross-skill
pytest filename collisions.
"""

from __future__ import annotations

from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/

DECK_AUDIT = _SKILL_ROOT / "commands" / "deck-audit.md"

# The three finding names defined by the Phase 3G issue body.
FINDING_NAMES = (
    "unattributed-generative-imagery",
    "prompt-claim-divergence",
    "style-incoherence",
)

# Severity labels expected per the Phase 3G issue body, paired with their
# finding names. The auditor doc must mark each finding with the correct
# severity — a regression in which the CRITICAL attribution flag drops
# to MINOR (or vice versa) would be a serious release-quality bug.
FINDING_SEVERITIES = {
    "unattributed-generative-imagery": "CRITICAL",
    "prompt-claim-divergence": "MAJOR",
    "style-incoherence": "MINOR",
}


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


# ---------------------------------------------------------------------------
# Section presence
# ---------------------------------------------------------------------------


def test_deck_audit_has_generative_imagery_audit_section():
    """deck-audit.md must have a top-level 'Generative-imagery audit' section.

    Per the Phase 3G issue body, the doc gains a new subsection scoped
    to ``imagery_policy: generative-eligible`` documenting the three
    findings. The section must be discoverable from the doc TOC.
    """
    body = _read(DECK_AUDIT)
    assert "Generative-imagery audit" in body, (
        "deck-audit.md is missing the 'Generative-imagery audit' "
        "section. Per the Phase 3G issue body (#188), the auditor doc "
        "gains a new subsection documenting the three generative-imagery "
        "findings (unattributed-generative-imagery, prompt-claim-divergence, "
        "style-incoherence)."
    )


# ---------------------------------------------------------------------------
# Finding documentation
# ---------------------------------------------------------------------------


def test_deck_audit_documents_all_three_findings():
    """deck-audit.md must name each of the three findings."""
    body = _read(DECK_AUDIT)
    for name in FINDING_NAMES:
        assert name in body, (
            f"deck-audit.md does not mention finding `{name}`. "
            f"All three Phase 3G findings ({', '.join(FINDING_NAMES)}) "
            f"must be documented per the issue #188 acceptance criteria."
        )


def test_deck_audit_findings_have_correct_severity():
    """Each finding must appear near its expected severity label.

    The check is "the severity label appears within ~400 chars of the
    finding name's first occurrence" — wide enough to survive prose
    edits but tight enough to catch a finding losing its severity
    attribution entirely.
    """
    body = _read(DECK_AUDIT)
    for finding, severity in FINDING_SEVERITIES.items():
        idx = body.find(finding)
        assert idx != -1, f"finding `{finding}` missing from deck-audit.md"
        # Window starts at the finding mention (covers heading like
        # "### Finding 1: `<name>` (CRITICAL)") and extends 400 chars.
        # The heading itself is the strongest signal; the body that
        # follows reinforces. We accept either window position.
        # Also accept the heading-side window before the finding name
        # (severities like "(CRITICAL)" sometimes precede the inline
        # finding name in detection-logic prose).
        start = max(0, idx - 200)
        window = body[start : idx + 400]
        assert severity in window, (
            f"finding `{finding}` is documented in deck-audit.md but "
            f"its expected severity `{severity}` does not appear within "
            f"400 chars of the finding mention. Per the issue #188 body, "
            f"`{finding}` is `{severity}`."
        )


# ---------------------------------------------------------------------------
# Gating: zero behavior change for deterministic-only decks
# ---------------------------------------------------------------------------


def test_deck_audit_gates_findings_on_generative_eligible():
    """The doc must explicitly gate the three findings on imagery_policy.

    This is the load-bearing regression guard against silent activation
    on deterministic-only decks. The doc must say the checks do not run
    on absent / deterministic-only / consumer-provided policies.
    """
    body = _read(DECK_AUDIT)
    assert "generative-eligible" in body, (
        "deck-audit.md does not mention `generative-eligible`. The three "
        "Phase 3G findings MUST be gated behind this policy value per the "
        "issue #188 acceptance criteria and the zero-behavior-change "
        "guarantee for deterministic decks."
    )


def test_deck_audit_documents_deterministic_no_op():
    """The doc must say the checks DO NOT RUN on non-generative policies.

    Per the issue body: "Explicit: when policy is deterministic-only or
    absent, these checks DO NOT RUN." This test asserts the doc carries
    this explicit promise — a regression-guard against silently
    activating the checks on existing (deterministic) decks.
    """
    body = _read(DECK_AUDIT)
    section_idx = body.find("Generative-imagery audit")
    assert section_idx != -1, "Generative-imagery audit section missing."
    # Read the section + a generous look-back to catch the Procedure
    # step 7 gate (which is also load-bearing).
    section = body[max(0, section_idx - 2000) : section_idx + 8000]
    # The doc must explicitly use the word "deterministic-only" inside
    # the gate language (per the issue body's verbatim phrasing).
    assert "deterministic-only" in section, (
        "deck-audit.md does not mention `deterministic-only` near the "
        "Generative-imagery audit section. The doc must explicitly call "
        "out the deterministic-only case as a no-op per issue #188."
    )
    # The doc should also explicitly mention the SKIP / no-op semantic
    # (e.g., 'SKIP', 'no-op', 'DO NOT RUN', 'zero behavior change') so
    # the reader knows what 'gated' actually means at runtime.
    lower = section.lower()
    sentinels = ("skip", "no-op", "do not run", "zero behavior change", "byte-identical")
    assert any(s in lower for s in sentinels), (
        "deck-audit.md mentions deterministic-only near the "
        "Generative-imagery audit section but does not document the "
        "actual no-op semantic. Expected one of: "
        f"{', '.join(sentinels)}."
    )


def test_deck_audit_references_imagery_policy_field_name():
    """The auditor must name the field it gates on by its YAML key.

    Without the literal ``imagery_policy:`` substring near the gate
    documentation, an operator reading the section cannot trace the
    behavior back to the BRIEF.md frontmatter contract documented in
    deck-brief.md and SKILL.md.
    """
    body = _read(DECK_AUDIT)
    assert "imagery_policy" in body, (
        "deck-audit.md does not mention `imagery_policy`. The three "
        "Phase 3G findings gate on this BRIEF.md frontmatter field; the "
        "auditor doc must name the field per issue #188 acceptance "
        "criteria."
    )


# ---------------------------------------------------------------------------
# Suppression directive
# ---------------------------------------------------------------------------


def test_deck_audit_documents_suppression_directive_shape():
    """The doc must document the ``anvil-audit-disable:`` directive.

    Per the issue body, the suppression mechanism is
    ``<!-- anvil-audit-disable: <finding-name> -->`` per the existing
    lint-disable convention. The doc must spell out the shape.
    """
    body = _read(DECK_AUDIT)
    assert "anvil-audit-disable" in body, (
        "deck-audit.md does not document the `anvil-audit-disable:` "
        "directive. Per issue #188 the suppression mechanism follows "
        "the existing `anvil-lint-disable` convention; the auditor doc "
        "must spell out the audit-side variant."
    )


def test_deck_audit_suppression_documents_per_finding_use():
    """Each of the three findings must reference the suppression directive.

    The directive's value names the specific finding to suppress
    (per the existing marp_lint convention where the per-finding rule
    name follows the prefix). The doc must demonstrate this binding for
    each of the three findings so the operator knows what string to
    type in the disable comment.
    """
    body = _read(DECK_AUDIT)
    for finding in FINDING_NAMES:
        # Either the directive appears with this finding's name as the
        # value (the strongest binding) OR the directive shape and the
        # finding name both appear within ~500 chars of each other.
        full_directive = f"anvil-audit-disable: {finding}"
        if full_directive in body:
            continue
        # Fallback: directive + finding both within a 500-char window.
        finding_idx = body.find(finding)
        assert finding_idx != -1
        window = body[max(0, finding_idx - 500) : finding_idx + 1500]
        assert "anvil-audit-disable" in window, (
            f"deck-audit.md does not document the suppression directive "
            f"for `{finding}`. Each finding's section must show the "
            f"`<!-- anvil-audit-disable: {finding} -->` shape so the "
            f"operator can suppress it on a specific slide."
        )


# ---------------------------------------------------------------------------
# Cross-references — verifier side of Phase 3F's contract
# ---------------------------------------------------------------------------


def test_deck_audit_references_prompt_journal_primitive():
    """The doc must reference the Phase 2D journal primitive.

    Finding #2 (prompt-claim-divergence) and Finding #3 (style-incoherence)
    both read the prompt journal at ``assets/_prompts.json``. The doc
    must reference the ``prompt_journal.py`` primitive (PR #182, issue
    #177) so the runtime contract is traceable.
    """
    body = _read(DECK_AUDIT)
    assert "prompt_journal" in body or "_prompts.json" in body, (
        "deck-audit.md does not reference the prompt-journal primitive "
        "(`anvil/skills/deck/lib/prompt_journal.py` or `_prompts.json`). "
        "Per issue #188 the auditor reads the journal via "
        "`read_journal()` for findings 2 and 3."
    )


def test_deck_audit_documents_allowed_attribution_phrases():
    """Finding #1 must document allowed attribution phrases.

    Per the issue body, finding `unattributed-generative-imagery` uses
    Phase 3F's allowed/forbidden phrase lists as the single source of
    truth. The auditor doc anchors the v0 allowed-phrase set; Phase 3F
    (#187) references the same set. The smoke test asserts at least the
    canonical "concept render" phrase is documented.
    """
    body = _read(DECK_AUDIT)
    # The canonical attribution phrase per issue #187's body.
    assert "concept render" in body.lower(), (
        "deck-audit.md does not document the `concept render` allowed "
        "attribution phrase. Finding #1 (`unattributed-generative-imagery`) "
        "must reference the v0 allowed-phrase set per issue #188 + #187."
    )


def test_deck_audit_documents_forbidden_phrases():
    """Finding #1 must document at least one forbidden phrase.

    The forbidden-phrase list is the "load-bearing real-world claim"
    side of the contract. The doc must spell at least one of them out
    so the operator (and Phase 3F's reader) understands what triggers
    the CRITICAL finding.
    """
    body = _read(DECK_AUDIT)
    forbidden_candidates = (
        "product screenshot",
        "actual photo",
        "customer deployment",
        "actual user",
        "from the field",
    )
    assert any(p in body.lower() for p in forbidden_candidates), (
        "deck-audit.md does not document any forbidden phrase for "
        f"finding #1. Expected at least one of: {', '.join(forbidden_candidates)}. "
        "These are the v0 falsifiable-claim phrases the auditor flags as "
        "CRITICAL."
    )
