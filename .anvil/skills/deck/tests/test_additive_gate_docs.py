"""Doc-coverage smoke tests for issue #547 Part 2 — imagine-then-review
additive-ness gate (extension of dim 8 design critic).

The additive-ness gate lands across four user-facing docs:

1. ``deck-design.md`` — new procedure step 7b (the per-slot additive-ness
   judgment), plus the `non-additive-generative-image` finding type
   appended to § "Identify findings", plus the by-absence marker
   convention in the dim 8 justification example.
2. ``deck-audit.md`` — § "Generative-imagery audit" cross-reference
   noting attribution is enforced here; additive-ness is enforced in
   `deck-design.md`.
3. ``deck-revise.md`` — step 8 gains a cut-vs-re-prompt bullet for
   `non-additive-generative-image` findings.
4. ``SKILL.md`` — § "Asset generation" mentions the imagine-then-review
   framing (one sentence).

Substring-presence only, following the precedent of
``test_imagery_policy_docs.py``: no Marp render, no schema parse.
"""

from __future__ import annotations

from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent

DECK_DESIGN = _SKILL_ROOT / "commands" / "deck-design.md"
DECK_AUDIT = _SKILL_ROOT / "commands" / "deck-audit.md"
DECK_REVISE = _SKILL_ROOT / "commands" / "deck-revise.md"
DECK_IMAGEGEN = _SKILL_ROOT / "commands" / "deck-imagegen.md"
SKILL_MD = _SKILL_ROOT / "SKILL.md"

FINDING_TYPE = "non-additive-generative-image"


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


# ---------------------------------------------------------------------------
# deck-design.md
# ---------------------------------------------------------------------------


def test_deck_design_documents_additive_ness_pass() -> None:
    body = _read(DECK_DESIGN)
    assert "Additive-ness pass" in body, (
        "deck-design.md should carry a procedure step titled "
        "'Additive-ness pass' (the imagine-then-review gate per #547)."
    )


def test_deck_design_documents_finding_type() -> None:
    body = _read(DECK_DESIGN)
    assert FINDING_TYPE in body, (
        f"deck-design.md should name the {FINDING_TYPE!r} finding type "
        f"in § 'Identify findings'."
    )


def test_deck_design_documents_verdict_vocabulary() -> None:
    body = _read(DECK_DESIGN)
    for verdict in ("additive", "neutral", "detracting"):
        assert verdict in body, (
            f"deck-design.md should document the {verdict!r} verdict in the "
            f"additive-ness pass closed-enum vocabulary."
        )


def test_deck_design_references_imagegen_additive_helper() -> None:
    """The doc should reference the skill-local helper module so the
    design critic's procedure has a runtime substrate to call."""
    body = _read(DECK_DESIGN)
    assert "imagegen_additive" in body, (
        "deck-design.md should reference anvil/skills/deck/lib/"
        "imagegen_additive.py — the helper that decides whether the gate "
        "runs and enumerates the per-slot input bundles."
    )


def test_deck_design_documents_skip_for_deterministic_only() -> None:
    """The pass must be documented as a no-op on deterministic-only
    decks (byte-identical output vs pre-#547)."""
    body = _read(DECK_DESIGN)
    # Either phrase qualifies as proper documentation of the no-op semantic.
    assert (
        "no-op" in body or "byte-identical" in body
    ), (
        "deck-design.md should document that the additive-ness pass is a "
        "no-op on non-generative-eligible decks (the byte-identical "
        "backward-compat guarantee)."
    )


def test_deck_design_documents_non_waivable_attribution_composition() -> None:
    """The doc must document that the additive-ness gate is *additional*,
    not an alternative, to the fabrication-attribution contract."""
    body = _read(DECK_DESIGN)
    assert "stacked" in body.lower(), (
        "deck-design.md should explain that the additive-ness check and "
        "the fabrication-attribution contract are STACKED (non-alternative)."
    )


def test_deck_design_documents_tolerant_reader_for_extension_journals() -> None:
    """Issue #621: the step 7b prose must document that a consumer-extension
    journal (unknown per-entry fields, e.g. ``generated_at``) still runs the
    gate rather than degrading to 'no attested slots.'"""
    body = _read(DECK_DESIGN)
    assert "tolerant reader" in body.lower(), (
        "deck-design.md step 7b should describe read_journal as a tolerant "
        "reader (issue #621) so extension journals do not degrade the gate."
    )
    assert "#621" in body, (
        "deck-design.md should cite issue #621 for the tolerant-reader "
        "contract on consumer-extension journals."
    )


# ---------------------------------------------------------------------------
# deck-imagegen.md
# ---------------------------------------------------------------------------


def test_deck_imagegen_documents_tolerant_reader_contract() -> None:
    """Issue #621: deck-imagegen.md (journal consumer) must document the
    tolerant-reader read contract for unknown per-entry fields."""
    body = _read(DECK_IMAGEGEN)
    assert "tolerant reader" in body.lower(), (
        "deck-imagegen.md should document that read_journal is a tolerant "
        "reader — unknown per-entry fields are preserved, not rejected."
    )
    assert "#621" in body, (
        "deck-imagegen.md should cite issue #621 for the tolerant-reader "
        "read contract."
    )


# ---------------------------------------------------------------------------
# deck-audit.md
# ---------------------------------------------------------------------------


def test_deck_audit_cross_references_additive_gate() -> None:
    body = _read(DECK_AUDIT)
    assert "additive" in body.lower(), (
        "deck-audit.md § 'Generative-imagery audit' should cross-reference "
        "the new additive-ness gate in deck-design.md."
    )
    assert "deck-design" in body, (
        "deck-audit.md should explicitly name deck-design.md as the owner "
        "of the additive-ness check (separation of concerns: attribution "
        "here, additive-ness there)."
    )


def test_deck_audit_documents_stacked_contracts() -> None:
    """The two contracts are stacked, not alternatives."""
    body = _read(DECK_AUDIT)
    assert "stacked" in body.lower() or "non-waivable" in body.lower(), (
        "deck-audit.md should document that the attribution + "
        "additive-ness checks are STACKED, not alternatives."
    )


def test_deck_audit_documents_proactive_default_policy_under_skip_rule() -> None:
    """The audit's skip rule (only fires when effective policy is
    generative-eligible) must explicitly mention the proactive
    `default_policy` resolution path as a possible source of
    'effective: generative-eligible'."""
    body = _read(DECK_AUDIT)
    assert "default_policy" in body, (
        "deck-audit.md should document that the effective imagery_policy "
        "(post #547 default_policy resolution) drives the skip rule."
    )


# ---------------------------------------------------------------------------
# deck-revise.md
# ---------------------------------------------------------------------------


def test_deck_revise_documents_cut_vs_reprompt_branch() -> None:
    body = _read(DECK_REVISE)
    assert FINDING_TYPE in body, (
        f"deck-revise.md should name the {FINDING_TYPE!r} finding type and "
        f"document the reviser's branching (cut vs re-prompt)."
    )
    assert "cut" in body.lower()
    assert "re-prompt" in body.lower() or "reprompt" in body.lower()


def test_deck_revise_preserves_fabrication_attribution_under_cut() -> None:
    """Cutting an image cuts its attribution requirement — the doc
    should make this explicit so the reviser does NOT misapply the
    finding as license to strip attribution from a RETAINED image."""
    body = _read(DECK_REVISE)
    # We look for the key phrase that documents the non-waivable
    # contract under the new finding type.
    assert "non-waivable" in body.lower() or (
        "MUST NOT cite this finding" in body
    ), (
        "deck-revise.md should explicitly state that the "
        "non-additive-generative-image finding does NOT grant license to "
        "strip attribution from retained images — the fabrication-"
        "attribution contract is non-waivable."
    )


# ---------------------------------------------------------------------------
# SKILL.md
# ---------------------------------------------------------------------------


def test_skill_md_mentions_additive_gate() -> None:
    body = _read(SKILL_MD)
    assert "additive" in body.lower(), (
        "SKILL.md § 'Asset generation' should mention the imagine-then-"
        "review additive-ness gate (one sentence; canonical contract "
        "stays in deck-design.md)."
    )
