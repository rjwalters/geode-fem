"""Doc-coverage tests for the new ``deck-outline`` command introduced
under issue #549.

The ``deck-outline`` command mirrors the canonical shape of
``slides-outline`` (separate sibling directory ``<thread>.0.outline/``
carrying a freeform ``outline.md``, read-only once written, scorecard
declared ``human-verdict``). The acceptance criteria for this command
are doc-shape assertions:

1. Frontmatter declares ``name: deck-outline``.
2. The command documents Role / Inputs / Outputs / Procedure /
   ``_progress.json`` snippet / Git sync sections.
3. The skippability section lists ALL four structured-brief headings
   (``## Outline`` / ``## Narrative spine`` / ``## Beats`` /
   ``## Slide-by-slide``).
4. The output sibling is described as read-only once written.
5. The scorecard kind declaration mentions ``human-verdict``.
6. The command sets ``for_version: 0`` per the slides-outline precedent
   (so the outline is discoverable as a pre-version sibling).

Substring-presence only, following the precedent of
``test_imagery_policy_docs.py``: no Marp render, no schema parse.
Distinct filename (``test_deck_outline_command_doc.py``) per the #58
packaging convention to avoid cross-skill pytest filename collisions.
"""

from __future__ import annotations

from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/

DECK_OUTLINE = _SKILL_ROOT / "commands" / "deck-outline.md"

SKIP_CHECK_HEADINGS = (
    "## Outline",
    "## Narrative spine",
    "## Beats",
    "## Slide-by-slide",
)


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def test_deck_outline_file_exists():
    """The deck-outline command file must exist at the expected path."""
    assert DECK_OUTLINE.exists(), (
        f"deck-outline.md does not exist at {DECK_OUTLINE}. The new "
        "command is the load-bearing artifact for issue #549; without "
        "it the outline gate is undocumented."
    )


def test_deck_outline_frontmatter_name():
    """Frontmatter must declare ``name: deck-outline``."""
    body = _read(DECK_OUTLINE)
    # The frontmatter block sits at the top of the file. Anchor on the
    # opening fence; assert the name line appears before the closing
    # fence on its own line.
    assert body.startswith("---\n"), (
        "deck-outline.md must open with a YAML frontmatter fence."
    )
    closing_idx = body.find("\n---\n", 4)
    assert closing_idx != -1, (
        "deck-outline.md frontmatter is missing the closing fence."
    )
    frontmatter = body[4:closing_idx]
    assert "name: deck-outline" in frontmatter, (
        "deck-outline.md frontmatter must declare `name: deck-outline` "
        "(not a typo'd or refactored name)."
    )


def test_deck_outline_documents_role_inputs_outputs_procedure():
    """The command must carry the canonical command-shape sections."""
    body = _read(DECK_OUTLINE)
    for required in (
        "**Role**",
        "## Inputs",
        "## Outputs",
        "## Procedure",
    ):
        assert required in body, (
            f"deck-outline.md is missing the required `{required}` "
            "section. The canonical command shape (Role + Inputs + "
            "Outputs + Procedure) mirrors slides-outline.md."
        )


def test_deck_outline_documents_progress_snippet():
    """The command must carry a `_progress.json` snippet section.

    Mirrors slides-outline.md which documents the `_progress.json`
    shape (`for_version: 0`, `phases.outline.state: done`).
    """
    body = _read(DECK_OUTLINE)
    assert "`_progress.json` snippet" in body or "_progress.json snippet" in body, (
        "deck-outline.md is missing the `_progress.json` snippet "
        "section. The snippet documents the outline sibling's progress "
        "schema (for_version: 0, phases.outline.state: done)."
    )


def test_deck_outline_documents_for_version_zero():
    """The `_progress.json` snippet must record `for_version: 0`."""
    body = _read(DECK_OUTLINE)
    assert "for_version" in body and '"for_version": 0' in body, (
        "deck-outline.md must document `\"for_version\": 0` in the "
        "`_progress.json` snippet so the outline sibling is "
        "discoverable as a pre-version sibling (N=0). This mirrors "
        "the slides-outline precedent."
    )


def test_deck_outline_documents_git_sync_section():
    """The command must carry the standard git-sync section."""
    body = _read(DECK_OUTLINE)
    assert "## Git sync" in body, (
        "deck-outline.md is missing the `## Git sync` section. The "
        "per-phase commit hook contract is documented per "
        "anvil/lib/snippets/git_sync.md across every write-bearing "
        "command in the skill."
    )
    # The git-sync block names the specific staging target + commit
    # message format for the outline phase.
    assert "anvil(deck/outline)" in body, (
        "deck-outline.md git-sync block must name the per-phase "
        "commit message format `anvil(deck/outline): <thread>.0 "
        "[OUTLINED]`."
    )


def test_deck_outline_skip_check_lists_all_four_headings():
    """Skippability section must enumerate ALL four structured-brief
    headings the drafter's skip-check looks for.

    The headings (`## Outline`, `## Narrative spine`, `## Beats`,
    `## Slide-by-slide`) are the contract between deck-outline and
    deck-draft step 5 — both must list the same set. If one drifts
    the gate silently breaks.
    """
    body = _read(DECK_OUTLINE)
    for heading in SKIP_CHECK_HEADINGS:
        assert heading in body, (
            f"deck-outline.md skippability section is missing the "
            f"structured-brief heading `{heading}`. The skip-check "
            "contract between deck-outline and deck-draft step 5 "
            "requires all four headings to be enumerated identically."
        )


def test_deck_outline_documents_read_only_once_written():
    """The outline sibling must be documented as read-only once written.

    Mirrors the slides-outline contract. The drafter and reviser do
    not modify outline.md; if the operator wants to re-outline, they
    delete the sibling and re-run.
    """
    body = _read(DECK_OUTLINE)
    assert "read-only once written" in body, (
        "deck-outline.md must document that the outline sibling is "
        "`read-only once written`. The drafter and reviser do not "
        "modify outline.md; this is the canonical contract from "
        "slides-outline that deck-outline mirrors."
    )


def test_deck_outline_declares_human_verdict_scorecard_kind():
    """The `_meta.json` declaration must name `human-verdict`.

    Per anvil/lib/snippets/scorecard_kind.md, the outline sibling
    consumes its outputs as narrative (the drafter reads outline.md
    as load-bearing prompt context), not as a programmatic partial
    scorecard. The kind declaration is `human-verdict`.
    """
    body = _read(DECK_OUTLINE)
    assert "human-verdict" in body, (
        "deck-outline.md must declare `scorecard_kind: human-verdict` "
        "in the `_meta.json` documentation. The drafter consumes "
        "outline.md as narrative spine, not as a partial scorecard."
    )


def test_deck_outline_documents_driving_argument_and_per_slide_assignment():
    """The outline.md shape must name (a) one driving argument and
    (b) per-slide beat + claim assignment.

    These are the two load-bearing elements that distinguish the
    deck-outline shape from the slides-outline (talk) shape. The
    deck-outline is per-slide; the slides-outline is per-beat.
    """
    body = _read(DECK_OUTLINE)
    assert "driving argument" in body.lower(), (
        "deck-outline.md must name `driving argument` as the load-"
        "bearing top-level element of outline.md. This is what "
        "distinguishes the deck-outline shape from the slides-outline "
        "(talk) shape."
    )
    assert "beat" in body.lower() and "claim" in body.lower(), (
        "deck-outline.md must document the per-slide beat + claim "
        "assignment. Every slide names (a) its beat in the driving "
        "argument and (b) the one-line claim it lands."
    )


def test_deck_outline_documents_optional_perspective_input():
    """The outliner should read `<thread>.0.perspective/` if present.

    Perspective is documented as load-bearing context for the
    competition / market / why-now beats. Mirrors the deck-draft
    contract.
    """
    body = _read(DECK_OUTLINE)
    assert ".0.perspective/" in body, (
        "deck-outline.md must mention the optional "
        "`<thread>.0.perspective/` sibling as load-bearing context "
        "for the competition / market / why-now beats. The outliner "
        "consumes the same substrate the drafter does."
    )
