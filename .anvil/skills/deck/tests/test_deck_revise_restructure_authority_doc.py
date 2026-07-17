"""Doc-coverage tests for the ``deck-revise.md`` restructure-authority
edits under issue #549.

Acceptance criteria:

1. Step 7 (Build a revision plan) documents the reviser's restructure
   authority on `[structural]` findings.
2. Step 8 (Produce revised deck.md) documents the parallel rule:
   reviser MAY change slide order / count / boundaries on
   `[structural]` findings.
3. Step 11 (`_revision-log.md` worked example) carries a row showing
   a restructure resolution.
4. §Notes for the reviser agent carries the "Restructure when the
   critic says so" item.
5. The no-fabrication contract is preserved — restructure changes
   which slides land, not what content is allowed on them.

Substring-presence only, distinct filename
(``test_deck_revise_restructure_authority_doc.py``) per the #58
packaging convention.
"""

from __future__ import annotations

from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/

DECK_REVISE = _SKILL_ROOT / "commands" / "deck-revise.md"


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def test_deck_revise_step7_documents_structural_authority():
    """Step 7 (Build a revision plan) must document restructure
    authority on `[structural]` findings.

    The kind axis must be named (`[structural]`) so the reviser knows
    which finding kind activates the authority.
    """
    body = _read(DECK_REVISE)
    step7_idx = body.find("7. **Build a revision plan**")
    assert step7_idx != -1, (
        "deck-revise.md is missing the `7. **Build a revision "
        "plan**` step header."
    )
    # Step 7 is several bullet items long; read a generous slice.
    step7 = body[step7_idx : step7_idx + 6000]
    assert "[structural]" in step7, (
        "deck-revise.md step 7 must mention the `[structural]` kind "
        "marker. Per issue #549 the reviser gains restructure "
        "authority on findings carrying this marker; the marker "
        "must be named in the step that builds the revision plan."
    )


def test_deck_revise_step7_documents_first_resort():
    """Step 7 must document that restructure is the FIRST resort for
    `[structural]` findings, not the last.

    The framing is load-bearing: the canary failure mode is the
    reviser trying in-place clause edits on a `[structural]` finding
    and accepting the resulting arc problem as unfixable.
    """
    body = _read(DECK_REVISE)
    step7_idx = body.find("7. **Build a revision plan**")
    step7 = body[step7_idx : step7_idx + 6000]
    # Phrasing options that all communicate "first resort, not last".
    first_resort_signals = (
        "first resort",
        "FIRST resort",
        "first resort, not the last",
        "first resort for",
    )
    found = [s for s in first_resort_signals if s in step7]
    assert found, (
        "deck-revise.md step 7 must document that restructure is "
        "the FIRST resort for `[structural]` findings, not the "
        f"last. Expected one of {first_resort_signals!r}."
    )


def test_deck_revise_step7_documents_in_place_default():
    """Step 7 must document that `[in-place]` (or unmarked) findings
    do NOT activate the restructure authority.

    The default-by-omission rule is what keeps the reviser's
    restructure authority narrow.
    """
    body = _read(DECK_REVISE)
    step7_idx = body.find("7. **Build a revision plan**")
    step7 = body[step7_idx : step7_idx + 6000]
    assert "[in-place]" in step7, (
        "deck-revise.md step 7 must mention the `[in-place]` kind "
        "marker. The default-by-omission rule is what keeps the "
        "reviser's restructure authority narrow; without naming "
        "[in-place] as the default the contract is ambiguous."
    )


def test_deck_revise_step8_documents_restructure_authority():
    """Step 8 (Produce revised deck.md) must document the parallel
    rule: reviser MAY change slide order / count / boundaries on
    `[structural]` findings.
    """
    body = _read(DECK_REVISE)
    step8_idx = body.find("8. **Produce revised `deck.md`**")
    assert step8_idx != -1, (
        "deck-revise.md is missing the `8. **Produce revised "
        "deck.md**` step header."
    )
    # Step 8 has many sub-rules; read a generous slice.
    step8 = body[step8_idx : step8_idx + 8000]
    assert "[structural]" in step8, (
        "deck-revise.md step 8 must mention the `[structural]` kind "
        "marker. Per issue #549 the reviser MAY change slide order / "
        "count / boundaries when resolving a `[structural]` finding."
    )
    # The three operations the reviser gains: order, count,
    # boundaries (merge/split).
    operation_signals = (
        "slide order",
        "slide count",
        "slide boundaries",
    )
    for signal in operation_signals:
        assert signal in step8, (
            f"deck-revise.md step 8 must name `{signal}` as one of "
            "the restructure operations the reviser MAY perform on "
            "`[structural]` findings."
        )


def test_deck_revise_step8_preserves_no_fabrication_contract():
    """Step 8 must explicitly preserve the no-fabrication contract.

    Restructure changes WHICH slides land, not WHAT content is
    allowed on them. The no-fabrication contract is unchanged.
    """
    body = _read(DECK_REVISE)
    step8_idx = body.find("8. **Produce revised `deck.md`**")
    step8 = body[step8_idx : step8_idx + 8000]
    assert "no-fabrication" in step8, (
        "deck-revise.md step 8 must mention the no-fabrication "
        "contract in the same block as the restructure-authority "
        "rule. Restructure changes WHICH slides land, not WHAT "
        "content is allowed on them — the contract is preserved."
    )


def test_deck_revise_step11_worked_example_has_structural_row():
    """Step 11 (`_revision-log.md` worked example) must carry a row
    showing a restructure resolution.

    The worked example is the canonical reference for next reviser
    runs; the restructure resolution must appear so the canonical
    pattern is documented.
    """
    body = _read(DECK_REVISE)
    step11_idx = body.find("11. **Write `_revision-log.md`**")
    assert step11_idx != -1, (
        "deck-revise.md is missing the `11. **Write "
        "_revision-log.md**` step header."
    )
    # Read a generous slice — the worked example is several tables
    # long.
    step11 = body[step11_idx : step11_idx + 8000]
    assert "[structural]" in step11, (
        "deck-revise.md step 11 worked example must include a "
        "`[structural]` finding row. Without a worked restructure "
        "example, next reviser runs have no canonical pattern for "
        "how to record a reorder / merge / split / drop resolution."
    )
    # The resolution must name a specific slide reorder operation.
    reorder_signals = (
        "Slides reordered",
        "slides reordered",
        "moved to Slide",
    )
    found = [s for s in reorder_signals if s in step11]
    assert found, (
        "deck-revise.md step 11 worked example `[structural]` row "
        f"must name a specific reorder operation. Expected one of "
        f"{reorder_signals!r}."
    )


def test_deck_revise_notes_section_has_restructure_item():
    """§Notes for the reviser agent must carry the "Restructure when
    the critic says so" item.

    Mirrors the deck-draft "Honor the outline" item shape: a named
    contract item that the reviser-agent reads as priority guidance.
    """
    body = _read(DECK_REVISE)
    section_idx = body.find("## Notes for the reviser agent")
    assert section_idx != -1, (
        "deck-revise.md is missing the `## Notes for the reviser "
        "agent` section."
    )
    section = body[section_idx : section_idx + 6000]
    assert "Restructure when the critic says so" in section, (
        "deck-revise.md §Notes for the reviser agent must carry the "
        "`Restructure when the critic says so` item. This is the "
        "named contract item the reviser-agent reads as priority "
        "guidance per issue #549."
    )
    # The item must name the `[structural]` kind marker.
    item_idx = section.find("Restructure when the critic says so")
    item_body = section[item_idx : item_idx + 2000]
    assert "[structural]" in item_body, (
        "deck-revise.md `Restructure when the critic says so` note "
        "must reference the `[structural]` kind marker. Without the "
        "marker the agent cannot detect when to activate the "
        "restructure authority."
    )
