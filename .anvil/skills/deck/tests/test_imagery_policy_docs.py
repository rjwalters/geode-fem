"""Doc-coverage smoke tests for the ``imagery_policy`` BRIEF.md frontmatter
field documented under issue #132 (Phase 1B of Epic #130).

Per issue #132 the field is **prose-spec only** at this stage — runtime
parsing + enforcement land in Phase 2 of Epic #130 (Issues D/E). The
acceptance criteria are documentation-coverage assertions:

1. ``deck-brief.md`` documents ``imagery_policy`` with all three values
   AND the default-fallback rule.
2. ``deck-draft.md`` documents per-policy drafter behavior (one section
   per value) AND the default-fallback rule.
3. ``SKILL.md`` mentions the field in the BRIEF.md frontmatter
   reference.

Substring-presence only, following the precedent of
``test_slide_order_consistency.py``: no Marp render, no schema parse.
Distinct filename (``test_imagery_policy_docs.py``) per the #58
packaging convention to avoid cross-skill pytest filename collisions.
"""

from __future__ import annotations

from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/

DECK_BRIEF = _SKILL_ROOT / "commands" / "deck-brief.md"
DECK_DRAFT = _SKILL_ROOT / "commands" / "deck-draft.md"
SKILL_MD = _SKILL_ROOT / "SKILL.md"

POLICY_VALUES = ("generative-eligible", "consumer-provided", "deterministic-only")


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


# ---------------------------------------------------------------------------
# deck-brief.md
# ---------------------------------------------------------------------------


def test_deck_brief_documents_imagery_policy_field_name():
    """deck-brief.md must name the imagery_policy field at least once."""
    body = _read(DECK_BRIEF)
    assert "imagery_policy" in body, (
        "deck-brief.md does not mention `imagery_policy`. The BRIEF.md "
        "schema section is the canonical reference for this field; "
        "see issue #132 acceptance criteria."
    )


def test_deck_brief_documents_all_three_policy_values():
    """deck-brief.md must enumerate all three closed-enum values."""
    body = _read(DECK_BRIEF)
    for value in POLICY_VALUES:
        assert value in body, (
            f"deck-brief.md does not mention policy value `{value}`. "
            f"All three values ({', '.join(POLICY_VALUES)}) must be "
            "documented per issue #132 acceptance criteria."
        )


def test_deck_brief_documents_deterministic_only_as_default():
    """deck-brief.md must document the default-fallback behavior.

    Strategy: locate the imagery_policy section (anchored on the field
    name) and assert that both ``default`` and ``deterministic-only``
    appear within that section. The window is generous enough to
    survive minor prose edits without losing the association.
    """
    body = _read(DECK_BRIEF)
    # Anchor on the field name (case-sensitive — the field is a YAML
    # key). Find the FIRST occurrence inside the docs section
    # (after the frontmatter example block).
    anchor_idx = body.find("imagery_policy")
    assert anchor_idx != -1, "imagery_policy anchor missing from deck-brief.md."
    # Read 2000 chars from the first imagery_policy mention onward —
    # the field doc block is ~1200 chars; this comfortably covers it.
    section = body[anchor_idx : anchor_idx + 2000]
    assert "default" in section.lower(), (
        "deck-brief.md must mention `default` near the imagery_policy "
        "documentation — operators reading the field spec need the "
        "missing-field behavior."
    )
    assert "deterministic-only" in section, (
        "deck-brief.md mentions `default` somewhere but not near the "
        "imagery_policy section. The default-fallback rule (missing "
        "field → deterministic-only) must be explicitly documented "
        "per issue #132 acceptance criteria."
    )


# ---------------------------------------------------------------------------
# deck-draft.md
# ---------------------------------------------------------------------------


def test_deck_draft_has_respecting_imagery_policy_section():
    """deck-draft.md must have a section gating drafter behavior."""
    body = _read(DECK_DRAFT)
    # Allow either exact wording variant: a section header or a clear
    # gating sentence. The issue #132 spec names the section
    # "Respecting imagery_policy".
    assert "Respecting `imagery_policy`" in body or "Respecting imagery_policy" in body, (
        "deck-draft.md is missing the 'Respecting imagery_policy' "
        "section. Per issue #132 the drafter command must document "
        "per-policy slide-emit behavior."
    )


def test_deck_draft_documents_all_three_policy_values():
    """deck-draft.md must enumerate all three closed-enum values."""
    body = _read(DECK_DRAFT)
    for value in POLICY_VALUES:
        assert value in body, (
            f"deck-draft.md does not mention policy value `{value}`. "
            f"All three values ({', '.join(POLICY_VALUES)}) must have "
            "per-policy behavior documented per issue #132."
        )


def test_deck_draft_documents_default_fallback():
    """deck-draft.md must document the missing-field default behavior.

    Anchor on the "Respecting" section heading and assert ``default``
    and ``deterministic-only`` appear inside that section.
    """
    body = _read(DECK_DRAFT)
    section_idx = body.find("Respecting")
    assert section_idx != -1, "Respecting section anchor missing from deck-draft.md."
    section = body[section_idx:]
    assert "default" in section.lower(), (
        "deck-draft.md 'Respecting imagery_policy' section must mention "
        "`default` — operators reading the drafter spec need to find "
        "the missing-field behavior."
    )
    assert "deterministic-only" in section, (
        "deck-draft.md 'Respecting imagery_policy' section does not "
        "mention `deterministic-only`. The default-fallback rule must "
        "be explicit per issue #132 acceptance criteria."
    )


def test_deck_draft_documents_per_policy_allowed_forbidden():
    """deck-draft.md must call out allowed/forbidden patterns per value.

    The issue body lists "explicit list of allowed/forbidden patterns
    per policy value" as a required section element.
    """
    body = _read(DECK_DRAFT)
    # We look for the section words "Allowed" and "Forbidden" appearing
    # within the imagery_policy region. The structural assertion is
    # weak (substring presence) but catches accidental deletion of the
    # cheat-sheet table or per-policy bullets.
    section_idx = body.find("Respecting")
    assert section_idx != -1, "Respecting section anchor missing."
    section = body[section_idx:]
    assert "Allowed" in section, (
        "deck-draft.md 'Respecting imagery_policy' section does not "
        "mention 'Allowed' — the per-policy allowed/forbidden cheat "
        "sheet is a required spec element per issue #132."
    )
    assert "Forbidden" in section, (
        "deck-draft.md 'Respecting imagery_policy' section does not "
        "mention 'Forbidden' — the per-policy allowed/forbidden cheat "
        "sheet is a required spec element per issue #132."
    )


# ---------------------------------------------------------------------------
# SKILL.md
# ---------------------------------------------------------------------------


def test_skill_md_mentions_imagery_policy():
    """SKILL.md must mention imagery_policy in the BRIEF.md reference."""
    body = _read(SKILL_MD)
    assert "imagery_policy" in body, (
        "SKILL.md does not mention `imagery_policy`. The field shape "
        "must appear in the BRIEF.md frontmatter reference per issue "
        "#132 acceptance criteria."
    )


def test_skill_md_lists_all_three_policy_values():
    """SKILL.md must enumerate the closed-enum values."""
    body = _read(SKILL_MD)
    for value in POLICY_VALUES:
        assert value in body, (
            f"SKILL.md does not mention policy value `{value}`. The "
            "BRIEF.md frontmatter table must enumerate the closed "
            "enum per issue #132."
        )
