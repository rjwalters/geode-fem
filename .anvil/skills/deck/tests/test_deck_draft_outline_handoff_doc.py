"""Doc-coverage tests for the ``deck-draft.md`` outline-handoff edits
under issue #549.

Acceptance criteria:

1. The Reads frontmatter line in deck-draft.md names
   ``<thread>.0.outline/outline.md`` as an optional load-bearing spine.
2. The Inputs section enumerates the outline sibling in parallel to
   the perspective sibling.
3. Step 5 (Read inputs) mentions reading ``outline.md`` from
   ``<thread>.0.outline/`` and the skip-check on the four
   structured-brief headings.
4. Step 6 (Plan the slide order) documents outline-honoring slide
   order — the standard fundraising order is the fallback when no
   outline is loaded.
5. §Notes for the drafter agent carries the "Honor the outline" item.

Substring-presence only, distinct filename
(``test_deck_draft_outline_handoff_doc.py``) per the #58 packaging
convention.
"""

from __future__ import annotations

from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/

DECK_DRAFT = _SKILL_ROOT / "commands" / "deck-draft.md"

SKIP_CHECK_HEADINGS = (
    "## Outline",
    "## Narrative spine",
    "## Beats",
    "## Slide-by-slide",
)


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def test_deck_draft_reads_frontmatter_names_outline_sibling():
    """The Reads line at the top of deck-draft.md must name the
    outline sibling.

    Mirrors the slides-draft Reads line. The drafter is the consumer
    of outline.md; the Reads frontmatter is where downstream tooling
    looks to enumerate the drafter's inputs.
    """
    body = _read(DECK_DRAFT)
    # The Reads line is in the header block at the top of the file.
    header_block = body[:2000]
    assert ".0.outline/outline.md" in header_block, (
        "deck-draft.md Reads frontmatter must name the optional "
        "`<thread>.0.outline/outline.md` sibling. Mirrors the "
        "slides-draft Reads line that names the outline sibling."
    )


def test_deck_draft_inputs_section_enumerates_outline():
    """The Inputs section must enumerate the outline sibling.

    The Inputs section is the consumer-facing enumeration of every
    file the drafter reads; the outline must appear in parallel to
    the perspective sibling.
    """
    body = _read(DECK_DRAFT)
    section_idx = body.find("## Inputs")
    assert section_idx != -1, "deck-draft.md is missing `## Inputs`."
    section = body[section_idx : section_idx + 6000]
    assert ".0.outline/outline.md" in section, (
        "deck-draft.md Inputs section must enumerate the outline "
        "sibling. The outline-handoff contract requires the outline "
        "to be documented as a drafter input in parallel to the "
        "perspective sibling."
    )


def test_deck_draft_step5_reads_outline_md():
    """Step 5 (Read inputs) must mention reading `outline.md`
    from `<thread>.0.outline/`.

    This is the load step — the drafter checks for the outline
    sibling alongside `refs/` and `assets/` enumeration.
    """
    body = _read(DECK_DRAFT)
    # Find step 5 by anchoring on `5. **Read inputs**`.
    step5_idx = body.find("5. **Read inputs**")
    assert step5_idx != -1, (
        "deck-draft.md is missing the `5. **Read inputs**` step "
        "header."
    )
    # Step 5 is one large paragraph; read 6KB to span it.
    step5 = body[step5_idx : step5_idx + 6000]
    assert "outline.md" in step5, (
        "deck-draft.md step 5 must mention reading `outline.md` from "
        "the outline sibling. The outline-handoff contract requires "
        "the drafter to check for the sibling at load time."
    )
    assert ".0.outline/" in step5, (
        "deck-draft.md step 5 must name the `<thread>.0.outline/` "
        "sibling path. The outline-handoff contract requires the "
        "drafter to check the canonical N=0 sibling location."
    )


def test_deck_draft_step5_documents_skip_check_headings():
    """Step 5 must document the skip-check on structured-brief
    headings.

    The skip-check is the contract between deck-outline and
    deck-draft: if BRIEF.md already carries a structured outline
    section, the drafter satisfies the outline gate from the brief
    directly without requiring a separate `<thread>.0.outline/`
    sibling.
    """
    body = _read(DECK_DRAFT)
    step5_idx = body.find("5. **Read inputs**")
    step5 = body[step5_idx : step5_idx + 6000]
    for heading in SKIP_CHECK_HEADINGS:
        assert heading in step5, (
            f"deck-draft.md step 5 must enumerate the structured-brief "
            f"heading `{heading}` as part of the outline skip-check. "
            "The contract between deck-outline and deck-draft requires "
            "both files to list the same set of skip-check headings."
        )


def test_deck_draft_step6_outline_honoring_slide_order():
    """Step 6 (Plan the slide order) must document outline-honoring
    slide order.

    The standard fundraising order becomes the fallback applied only
    when no outline is loaded.
    """
    body = _read(DECK_DRAFT)
    step6_idx = body.find("6. **Plan the slide order**")
    assert step6_idx != -1, (
        "deck-draft.md is missing the `6. **Plan the slide order**` "
        "step header."
    )
    step6 = body[step6_idx : step6_idx + 6000]
    # The step must mention the outline-honoring behavior AND the
    # fallback semantic.
    assert "outline" in step6.lower(), (
        "deck-draft.md step 6 must mention the outline. The "
        "canonical fundraising slide order is the fallback when no "
        "outline is loaded; outline-honoring is the default when "
        "the outline is present."
    )
    # The drafter MUST honor the outline when present.
    must_honor_signals = (
        "MUST honor",
        "must honor",
    )
    found = any(s in step6 for s in must_honor_signals)
    assert found, (
        "deck-draft.md step 6 must explicitly require the drafter to "
        "honor the outline's per-slide beat assignment when an "
        "outline is loaded. Expected one of "
        f"{must_honor_signals!r}."
    )


def test_deck_draft_notes_section_carries_honor_the_outline_item():
    """§Notes for the drafter agent must carry the "Honor the outline"
    item.

    Mirrors slides-draft.md §"Notes for the drafter agent" which
    carries the same item.
    """
    body = _read(DECK_DRAFT)
    section_idx = body.find("## Notes for the drafter agent")
    assert section_idx != -1, (
        "deck-draft.md is missing the `## Notes for the drafter "
        "agent` section."
    )
    section = body[section_idx : section_idx + 4000]
    assert "Honor the outline" in section, (
        "deck-draft.md §Notes for the drafter agent must carry the "
        "`Honor the outline` item. Mirrors slides-draft.md §Notes "
        "for the drafter agent."
    )
    # The item must mention the outline sibling path so the drafter
    # knows where to find it.
    notes_idx = section.find("Honor the outline")
    note_body = section[notes_idx : notes_idx + 1000]
    assert ".0.outline/" in note_body or "outline.md" in note_body, (
        "deck-draft.md `Honor the outline` note must name the "
        "outline sibling path (`<thread>.0.outline/` or `outline.md`) "
        "so the drafter knows where to read it from."
    )
