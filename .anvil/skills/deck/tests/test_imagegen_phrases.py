"""Behavioral contract tests for ``anvil/skills/deck/lib/imagegen_phrases.py``.

This module is the canonical source for the generative-imagery
attribution phrase lists shipped under Epic #130 (PR #191's deck-audit
findings, PR #192's deck-draft / deck-revise contract). The
consolidation lifted three previously duplicated inline lists to a
single Python module — these tests pin the resulting frozensets and
helper semantics so future edits stay aligned with the prose specs in
the three command docs.

Distinct filename per the #58 packaging convention to avoid cross-skill
pytest filename collisions.
"""

from __future__ import annotations

import re
from pathlib import Path

import pytest

from anvil.skills.deck.lib.imagegen_phrases import (
    ALLOWED_ATTRIBUTION_PHRASES,
    FORBIDDEN_DOCUMENTARY_PHRASES,
    find_forbidden_phrases,
    has_attribution_phrase,
)


_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/
_COMMANDS = _SKILL_ROOT / "commands"

DECK_DRAFT = _COMMANDS / "deck-draft.md"
DECK_REVISE = _COMMANDS / "deck-revise.md"
DECK_AUDIT = _COMMANDS / "deck-audit.md"


# ---------------------------------------------------------------------------
# Frozenset identity / immutability
# ---------------------------------------------------------------------------


def test_allowed_phrases_is_frozenset():
    """The exported allowed-attribution list MUST be a frozenset.

    Consumers (the audit code path, future deck-side checks) treat the
    constant as read-only data. ``frozenset`` enforces immutability at
    the type level — a mutable ``set`` would let an accidental
    ``ALLOWED_ATTRIBUTION_PHRASES.add(...)`` silently divert the doc
    contract from the runtime contract.
    """
    assert isinstance(ALLOWED_ATTRIBUTION_PHRASES, frozenset)


def test_forbidden_phrases_is_frozenset():
    """The exported forbidden-documentary list MUST be a frozenset."""
    assert isinstance(FORBIDDEN_DOCUMENTARY_PHRASES, frozenset)


def test_allowed_and_forbidden_are_disjoint():
    """No phrase is simultaneously allowed and forbidden — sanity gate.

    A phrase listed in both sets would be a self-contradicting contract.
    The auditor's "forbidden wins" rule would silently fire CRITICAL on
    a drafter who used permitted vocabulary; this test catches the
    inconsistency at unit-test time instead of at audit time.
    """
    overlap = ALLOWED_ATTRIBUTION_PHRASES & FORBIDDEN_DOCUMENTARY_PHRASES
    assert overlap == frozenset(), (
        f"phrases appear in BOTH allowed and forbidden sets: {sorted(overlap)}"
    )


def test_all_phrases_are_lowercase():
    """Canonical entries are lowercase — matchers normalize input to lower().

    A mixed-case entry would silently never match (because
    ``find_forbidden_phrases`` lowercases the haystack and tests
    membership against the literal). This test pins the lowercase
    convention so a future edit doesn't accidentally introduce
    ``"Product Screenshot"`` and break the matcher.
    """
    for p in ALLOWED_ATTRIBUTION_PHRASES:
        assert p == p.lower(), f"allowed phrase {p!r} is not lowercase"
    for p in FORBIDDEN_DOCUMENTARY_PHRASES:
        assert p == p.lower(), f"forbidden phrase {p!r} is not lowercase"


# ---------------------------------------------------------------------------
# find_forbidden_phrases
# ---------------------------------------------------------------------------


def test_find_forbidden_returns_empty_on_clean_text():
    """Clean attribution-only text emits no forbidden findings."""
    assert find_forbidden_phrases("Concept render of the factory floor") == []
    assert find_forbidden_phrases("Aspirational mockup — dark theme") == []
    assert find_forbidden_phrases("") == []


def test_find_forbidden_returns_all_matches_in_compound_text():
    """A corpus containing multiple forbidden phrases yields all of them.

    The auditor's CRITICAL finding cites every forbidden phrase
    present, not just the first. The list is returned sorted so the
    message ordering is deterministic across runs.
    """
    text = "actual photo from the field of customer deployment"
    found = find_forbidden_phrases(text)
    assert "actual photo" in found
    assert "from the field" in found
    assert "customer deployment" in found
    assert found == sorted(found), "results must be sorted for determinism"


def test_find_forbidden_is_case_insensitive():
    """Mixed-case input matches the canonical lowercase entry."""
    assert find_forbidden_phrases("Product Screenshot of the dashboard") == [
        "product screenshot"
    ]
    assert find_forbidden_phrases("PRODUCT SCREENSHOT") == ["product screenshot"]
    assert find_forbidden_phrases("Customer Deployment in Q3") == [
        "customer deployment"
    ]


def test_find_forbidden_substring_semantics_for_captured_at():
    """``captured at`` matches arbitrary location suffixes via substring.

    This is the canonical use case for substring semantics — a literal
    ``captured at <location>`` entry would not survive any concrete
    location string. The ``captured at`` prefix MUST match
    ``captured at NYC office``, ``captured at the Acme HQ``, etc.
    """
    assert "captured at" in find_forbidden_phrases("captured at NYC office")
    assert "captured at" in find_forbidden_phrases("captured at the Acme HQ in Q3")
    assert "captured at" in find_forbidden_phrases("Image was captured at sunrise")


def test_find_forbidden_returns_deduplicated_results():
    """Repeated phrases collapse to a single canonical entry."""
    text = "actual photo, actual photo, actual photo"
    assert find_forbidden_phrases(text) == ["actual photo"]


def test_find_forbidden_no_false_positive_on_clean_attribution():
    """Allowed attribution language does not trigger forbidden matches."""
    for allowed in ALLOWED_ATTRIBUTION_PHRASES:
        assert find_forbidden_phrases(allowed) == [], (
            f"allowed phrase {allowed!r} unexpectedly matched a forbidden phrase"
        )


# ---------------------------------------------------------------------------
# has_attribution_phrase
# ---------------------------------------------------------------------------


def test_has_attribution_true_for_each_canonical_phrase():
    """Every allowed phrase by itself satisfies the attribution check."""
    for phrase in ALLOWED_ATTRIBUTION_PHRASES:
        assert has_attribution_phrase(phrase), f"{phrase!r} did not match itself"


def test_has_attribution_false_on_silent_corpus():
    """Text with no attribution language fails the check.

    This is the trigger for the auditor's ``unattributed-generative-
    imagery`` CRITICAL finding — a corpus that describes the image but
    fails to disclose its synthetic origin.
    """
    assert has_attribution_phrase("Dashboard view") is False
    assert has_attribution_phrase("Factory floor at mid-shift") is False
    assert has_attribution_phrase("") is False


def test_has_attribution_is_case_insensitive():
    """Mixed-case attribution language still satisfies the check."""
    assert has_attribution_phrase("Concept Render of the dashboard")
    assert has_attribution_phrase("ASPIRATIONAL MOCKUP")
    assert has_attribution_phrase("Illustrative Scene — backdrop")


def test_has_attribution_accepts_substring_in_longer_text():
    """An allowed phrase embedded in surrounding prose still matches."""
    assert has_attribution_phrase(
        "This concept render shows the v2 dashboard in dark mode"
    )
    assert has_attribution_phrase(
        "*Aspirational mockup* — depicts the planned hardware layout"
    )


def test_has_attribution_accepts_hyphenated_variant():
    """The hyphenated variants from PR #191 are first-class allowed.

    The hyphenation policy is documented in the module docstring:
    ``concept-render`` (italic caption variant) is explicitly
    enumerated alongside ``concept render``. This test pins that
    behavior so a future "normalize hyphens" refactor doesn't drop
    the explicit hyphen entries from the doc contract.
    """
    assert has_attribution_phrase("*concept-render* of the v2 dashboard")
    assert has_attribution_phrase("aspirational-mockup variant")
    assert has_attribution_phrase("illustrative-scene backdrop")


# ---------------------------------------------------------------------------
# Union preservation — every #191 + #192 phrase survives
# ---------------------------------------------------------------------------


# The auditor list per PR #191 / issue #188, as documented on `main` at
# consolidation time. Re-derived from grepping deck-audit.md at build time
# per the curator note (issue #195 comment 4593643843).
_PR191_AUDIT_ALLOWED = frozenset({
    "concept render",
    "concept-render",
    "aspirational mockup",
    "aspirational-mockup",
    "illustrative scene",
    "illustrative-scene",
    "illustrative render",
    "concept illustration",
})

_PR191_AUDIT_FORBIDDEN = frozenset({
    "product screenshot",
    "actual photo",
    "actual photograph",
    "customer deployment",
    "customer in production",
    "actual user",
    "real user",
    "from the field",
    "in production at",
    "live deployment",
})

# The drafter+reviser list per PR #192 / issue #187, as documented on
# `main` at consolidation time. Re-derived from grepping deck-draft.md
# and deck-revise.md at build time.
_PR192_DRAFT_FORBIDDEN = frozenset({
    "product screenshot",
    "actual photo",
    "real photograph",
    "customer deployment",
    "actual user",
    "real user",
    "from the field",
    "taken on-site",
    "captured at",
    "customer environment",
    "production deployment",
})


def test_allowed_set_preserves_pr191_audit_list():
    """Every allowed phrase the PR #191 auditor enumerated survives.

    Acceptance criterion: "Union of #191 + #192's forbidden lists
    preserved (no phrase silently dropped)" (issue #195) — the
    allowed-list analog of the same rule.
    """
    missing = _PR191_AUDIT_ALLOWED - ALLOWED_ATTRIBUTION_PHRASES
    assert missing == frozenset(), (
        f"PR #191 allowed phrases silently dropped: {sorted(missing)}"
    )


def test_forbidden_set_preserves_pr191_audit_list():
    """Every forbidden phrase the PR #191 auditor enumerated survives."""
    missing = _PR191_AUDIT_FORBIDDEN - FORBIDDEN_DOCUMENTARY_PHRASES
    assert missing == frozenset(), (
        f"PR #191 forbidden phrases silently dropped: {sorted(missing)}"
    )


def test_forbidden_set_preserves_pr192_drafter_list():
    """Every forbidden phrase the PR #192 drafter/reviser enumerated survives.

    This is the consolidation contract: the union of the two
    pre-consolidation lists must equal the post-consolidation
    canonical set — no silent drops, no asymmetric drift.
    """
    missing = _PR192_DRAFT_FORBIDDEN - FORBIDDEN_DOCUMENTARY_PHRASES
    assert missing == frozenset(), (
        f"PR #192 forbidden phrases silently dropped: {sorted(missing)}"
    )


# ---------------------------------------------------------------------------
# Cross-doc reconciliation — phrases the auditor flags must be findable
# ---------------------------------------------------------------------------


def test_auditor_enumerated_forbidden_phrases_all_match_via_helper():
    """Every forbidden phrase in the canonical set matches its own literal.

    A regression-guard against an entry that is in the set but
    structurally cannot be found (e.g., trailing whitespace, accidental
    capitalisation — see the lowercase test above for the canonical
    case). This test exercises the public matcher rather than direct
    membership so any future change to ``find_forbidden_phrases``'s
    matching algorithm is caught.

    Note: substring semantics mean that a phrase containing another
    forbidden phrase (e.g., ``"actual photograph"`` contains
    ``"actual photo"``) will match *both*. The contract is therefore
    "the phrase matches itself", not "the phrase matches itself and
    nothing else". The size-equality test above guards against unintended
    additions.
    """
    for phrase in FORBIDDEN_DOCUMENTARY_PHRASES:
        matched = find_forbidden_phrases(phrase)
        assert phrase in matched, (
            f"forbidden phrase {phrase!r} does not match itself via the helper"
        )


# ---------------------------------------------------------------------------
# Doc-coverage: command docs cross-reference the canonical module
# ---------------------------------------------------------------------------

# After consolidation, the three command docs should reference the
# canonical module name rather than duplicating the list. The acceptance
# criterion is "Three command docs reference the module as canonical"
# (issue #195). We do a soft substring check on the module path/name —
# strong enough to catch a future doc rewrite that silently re-inlines
# the lists, lenient enough to survive prose rephrasing.


_CANONICAL_REFERENCE_PATTERNS = (
    "imagegen_phrases",  # module name (filename + import path both contain this)
)


@pytest.mark.parametrize("doc", [DECK_DRAFT, DECK_REVISE, DECK_AUDIT])
def test_command_doc_references_canonical_module(doc: Path):
    """deck-draft.md, deck-revise.md, deck-audit.md must point at the module.

    The consolidation contract: the command docs continue to display
    the key vocabulary (the drafter agent needs to see it inline at
    prompt-render time per the curator note on issue #195), but they
    MUST also name the canonical Python source so a reader knows where
    additions land. This test fires when a doc loses its
    canonical-source pointer.
    """
    body = doc.read_text(encoding="utf-8")
    assert any(pat in body for pat in _CANONICAL_REFERENCE_PATTERNS), (
        f"{doc.name} does not reference any of "
        f"{_CANONICAL_REFERENCE_PATTERNS!r}. Per issue #195, the three "
        "command docs must cross-reference "
        "`anvil/skills/deck/lib/imagegen_phrases.py` as the canonical "
        "source of truth (the inline phrase examples remain for the "
        "drafter agent's benefit, but the authoritative list lives in "
        "Python)."
    )


# ---------------------------------------------------------------------------
# Suppression-contract no-op verification (regression guard)
# ---------------------------------------------------------------------------


def test_module_does_not_implement_suppression_logic():
    """The suppression directive is enforced upstream, not in this module.

    The curator's Test Plan calls out the interaction: a slide carrying
    ``<!-- anvil-audit-disable: unattributed-generative-imagery -->``
    is still expected to produce the same matcher output here; only
    the auditor's runtime downgrades the finding's severity. This
    regression-guard pins the contract by asserting the helpers return
    their normal answers even when the suppression directive is in the
    corpus — i.e., the module is intentionally suppression-unaware.
    """
    text_with_suppression = (
        "<!-- anvil-audit-disable: unattributed-generative-imagery -->\n"
        "Product screenshot of the dashboard"
    )
    # The matcher MUST still flag the forbidden phrase. Whether the
    # auditor downgrades the resulting finding to info is decided
    # downstream of this module.
    assert find_forbidden_phrases(text_with_suppression) == ["product screenshot"]
    # And the attribution check still reports no attribution language.
    assert has_attribution_phrase(text_with_suppression) is False


# ---------------------------------------------------------------------------
# Set-size sanity (regression guard against accidental wholesale changes)
# ---------------------------------------------------------------------------


def test_allowed_set_size_matches_consolidated_union():
    """The allowed set is exactly the union of PR #191 (the only source).

    PR #192 did not enumerate allowed phrases (the divergence was
    asymmetric); PR #191 is the canonical allowed-side source. The
    union therefore equals PR #191's 8 entries.
    """
    assert ALLOWED_ATTRIBUTION_PHRASES == _PR191_AUDIT_ALLOWED


def test_forbidden_set_size_matches_consolidated_union():
    """The forbidden set is exactly the union of PR #191 and PR #192.

    Sanity gate: if a future edit adds or drops entries, this test
    fires to make sure the addition is intentional (and the curator's
    "union preserved" acceptance criterion is re-verified).
    """
    expected_union = _PR191_AUDIT_FORBIDDEN | _PR192_DRAFT_FORBIDDEN
    assert FORBIDDEN_DOCUMENTARY_PHRASES == expected_union, (
        "consolidated forbidden set diverges from the documented "
        f"PR #191 + PR #192 union; symmetric diff: "
        f"{sorted(FORBIDDEN_DOCUMENTARY_PHRASES ^ expected_union)}"
    )
