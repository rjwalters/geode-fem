"""Doc-coverage tests for the SKILL.md changes that wire the new
``deck-outline`` command into the deck state machine and command
dispatch table (issue #549).

Acceptance criteria:

1. The state-machine string carries `OUTLINED` between `BRIEF_DONE`
   and `DRAFTED`.
2. The state-machine evidence table carries an `OUTLINED` row whose
   evidence cell names `<thread>.0.outline/outline.md`.
3. The command dispatch table carries a `deck-outline <thread>` row.
4. The "Skill-specific phases" section carries a `deck-outline`
   paragraph that names the driving-argument + per-slide-beat shape.
5. The "non-gating" framing is explicit: outline absence does NOT
   block drafting.

Substring-presence only, distinct filename
(``test_deck_outline_state_machine_doc.py``) per the #58 packaging
convention.
"""

from __future__ import annotations

from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/

SKILL_MD = _SKILL_ROOT / "SKILL.md"


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def test_skill_md_state_machine_includes_outlined():
    """The state-machine arrow-string must include `OUTLINED`.

    The shape post-#549 is:
        EMPTY -> BRIEF_DONE -> OUTLINED -> DRAFTED -> ...
    """
    body = _read(SKILL_MD)
    # The arrow-string lives inside a fenced code block under
    # ## State machine. Anchor on the section heading.
    section_idx = body.find("## State machine")
    assert section_idx != -1, (
        "SKILL.md is missing the `## State machine` section."
    )
    # Read enough of the section to span the arrow-string + the
    # evidence table.
    section = body[section_idx : section_idx + 4000]
    assert "OUTLINED" in section, (
        "SKILL.md state-machine string does not include `OUTLINED`. "
        "Per issue #549 the new state must sit between BRIEF_DONE "
        "and DRAFTED in the arrow-string."
    )
    # Ordering check: BRIEF_DONE precedes OUTLINED precedes DRAFTED.
    brief_done_idx = section.find("BRIEF_DONE")
    outlined_idx = section.find("OUTLINED")
    drafted_idx = section.find("DRAFTED")
    assert -1 < brief_done_idx < outlined_idx < drafted_idx, (
        "SKILL.md state-machine ordering is wrong. Expected "
        "BRIEF_DONE -> OUTLINED -> DRAFTED; got order "
        f"BRIEF_DONE@{brief_done_idx}, OUTLINED@{outlined_idx}, "
        f"DRAFTED@{drafted_idx}."
    )


def test_skill_md_evidence_table_has_outlined_row():
    """The state-machine evidence table must carry an `OUTLINED` row.

    The evidence cell must name `<thread>.0.outline/outline.md` per
    the slides-outline precedent.
    """
    body = _read(SKILL_MD)
    section_idx = body.find("## State machine")
    section = body[section_idx : section_idx + 6000]
    # Look for the evidence row containing the OUTLINED state.
    assert "`OUTLINED`" in section, (
        "SKILL.md state-machine evidence table is missing the "
        "OUTLINED row. The evidence-table row contract requires a "
        "`OUTLINED` cell with `<thread>.0.outline/outline.md` "
        "evidence."
    )
    # The evidence cell must name the outline-sibling path.
    assert "<thread>.0.outline/outline.md" in section, (
        "SKILL.md state-machine OUTLINED evidence row must name "
        "`<thread>.0.outline/outline.md`. Mirrors the slides-outline "
        "evidence-row precedent."
    )


def test_skill_md_command_dispatch_has_deck_outline_row():
    """The command-dispatch table must carry a deck-outline row."""
    body = _read(SKILL_MD)
    section_idx = body.find("## Command dispatch")
    assert section_idx != -1, (
        "SKILL.md is missing the `## Command dispatch` section."
    )
    section = body[section_idx : section_idx + 6000]
    # The row format is `| \`deck-outline <thread>\` | <role> | ...`.
    assert "deck-outline <thread>" in section or "`deck-outline" in section, (
        "SKILL.md command-dispatch table is missing the "
        "`deck-outline <thread>` row. Mirrors the deck-perspective "
        "row format."
    )


def test_skill_md_skill_specific_phases_documents_deck_outline():
    """The Skill-specific phases section must carry a deck-outline
    paragraph.

    The paragraph names (a) the driving-argument + per-slide-beat
    shape and (b) the non-gating posture (absence does not block
    drafting).
    """
    body = _read(SKILL_MD)
    section_idx = body.find("## Skill-specific phases")
    assert section_idx != -1, (
        "SKILL.md is missing the `## Skill-specific phases` section."
    )
    section = body[section_idx : section_idx + 6000]
    # The phase must be named.
    assert "**Outline**" in section or "deck-outline" in section, (
        "SKILL.md Skill-specific phases section is missing a "
        "deck-outline paragraph. The paragraph documents the phase "
        "(driving argument + per-slide beat assignment) and the "
        "non-gating posture."
    )
    # Driving argument and beat assignment named.
    deck_outline_idx = section.find("deck-outline")
    assert deck_outline_idx != -1, (
        "Skill-specific phases section must reference `deck-outline` "
        "by name in the outline paragraph."
    )
    outline_para = section[deck_outline_idx : deck_outline_idx + 2000]
    assert "driving argument" in outline_para.lower(), (
        "deck-outline paragraph in Skill-specific phases must name "
        "the `driving argument` element of outline.md."
    )
    assert "beat" in outline_para.lower(), (
        "deck-outline paragraph in Skill-specific phases must name "
        "the per-slide `beat` element of outline.md."
    )


def test_skill_md_outline_is_documented_non_gating():
    """SKILL.md must explicitly document outline absence as non-gating.

    Mirrors the perspective non-gating paragraph already in
    SKILL.md state-machine. The outline sibling is opt-in input,
    not required output.
    """
    body = _read(SKILL_MD)
    # Find the state-machine section and check for the non-gating
    # framing.
    section_idx = body.find("## State machine")
    section = body[section_idx : section_idx + 8000]
    # Phrasing options that all communicate the non-gating posture.
    non_gating_signals = (
        "non-gating",
        "does NOT block",
        "does not block",
        "outline sibling",
    )
    found = [s for s in non_gating_signals if s in section]
    assert found, (
        "SKILL.md state-machine section must document the outline "
        "sibling's non-gating posture. Expected one of "
        f"{non_gating_signals!r} near the OUTLINED row."
    )


def test_skill_md_documents_outline_read_only_once_written():
    """SKILL.md must restate the read-only-once-written contract
    for the outline sibling.

    Mirrors the deck-outline.md command and the slides-outline
    precedent.
    """
    body = _read(SKILL_MD)
    assert "read-only once written" in body, (
        "SKILL.md must mention the outline sibling is "
        "`read-only once written`. This is the canonical contract "
        "from slides-outline that deck-outline mirrors; the SKILL.md "
        "state-machine paragraph carries the consumer-facing summary."
    )
