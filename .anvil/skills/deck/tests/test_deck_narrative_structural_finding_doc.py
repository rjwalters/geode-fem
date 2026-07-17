"""Doc-coverage tests for the ``deck-narrative.md`` finding-kind axis
edits under issue #549.

Acceptance criteria:

1. Step 8 (Identify additional findings) documents the
   ``[structural]`` vs ``[in-place]`` kind axis on findings, with
   ``[in-place]`` as the default.
2. Step 10 (Write findings.md) worked example carries both kind
   markers — at least one ``[in-place]`` and at least one
   ``[structural]`` finding.
3. The old "rather than reordering — Team's canonical slot is Slide 10"
   framing (which read as "no reorders allowed") is removed or
   rephrased as an in-place exemplar that does NOT carry the
   "no reorders" framing.
4. The kind axis is documented as orthogonal to severity.

Substring-presence only, distinct filename
(``test_deck_narrative_structural_finding_doc.py``) per the #58
packaging convention.
"""

from __future__ import annotations

from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/

DECK_NARRATIVE = _SKILL_ROOT / "commands" / "deck-narrative.md"


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def test_deck_narrative_documents_structural_kind_marker():
    """The kind axis introducing `[structural]` must be documented."""
    body = _read(DECK_NARRATIVE)
    assert "[structural]" in body, (
        "deck-narrative.md must document the `[structural]` kind "
        "marker on findings. The kind axis is the contract that "
        "gives the reviser explicit restructure authority on "
        "slide-level reorder findings; without the marker the "
        "deck-revise step 7 / step 8 restructure-authority edits "
        "have no signal to detect."
    )


def test_deck_narrative_documents_in_place_kind_marker():
    """The kind axis introducing `[in-place]` (the default) must be
    documented.

    `[in-place]` is the default — every finding without an explicit
    `[structural]` marker is treated as `[in-place]`. The contract
    must be explicit.
    """
    body = _read(DECK_NARRATIVE)
    assert "[in-place]" in body, (
        "deck-narrative.md must document the `[in-place]` kind "
        "marker on findings. `[in-place]` is the default kind; the "
        "marker is the explicit signal that the finding is resolved "
        "by a clause-level edit on the slide as it stands."
    )


def test_deck_narrative_step8_documents_kind_axis():
    """Step 8 (Identify additional findings) must document the kind
    axis classification.

    Step 8 is where the critic decides which kind to mark a finding.
    The kind-axis rule belongs there (not buried in step 10).
    """
    body = _read(DECK_NARRATIVE)
    step8_idx = body.find("8. **Identify additional findings**")
    assert step8_idx != -1, (
        "deck-narrative.md is missing the `8. **Identify additional "
        "findings**` step header."
    )
    # Read a generous slice — step 8 carries the new kind-axis
    # paragraph.
    step8 = body[step8_idx : step8_idx + 6000]
    assert "[structural]" in step8, (
        "deck-narrative.md step 8 must document the `[structural]` "
        "kind marker. The kind-axis classification belongs in step 8 "
        "(where findings are identified), not buried in the step 10 "
        "worked example."
    )
    assert "[in-place]" in step8, (
        "deck-narrative.md step 8 must document the `[in-place]` "
        "kind marker. The kind axis is binary; both kinds must be "
        "named in the same paragraph."
    )


def test_deck_narrative_step8_documents_kind_is_orthogonal_to_severity():
    """Step 8 must document that the kind axis is orthogonal to
    severity.

    A `[major][structural]` finding is a slide-level reorder that
    blocks advance; a `[minor][in-place]` finding is a clause edit
    that doesn't. The orthogonality must be explicit so the reviser
    doesn't conflate the two.
    """
    body = _read(DECK_NARRATIVE)
    step8_idx = body.find("8. **Identify additional findings**")
    step8 = body[step8_idx : step8_idx + 6000]
    # Phrasing options that all communicate orthogonality.
    orthogonality_signals = (
        "orthogonal",
        "in addition to the severity",
        "severity axis",
    )
    found = [s for s in orthogonality_signals if s in step8]
    assert found, (
        "deck-narrative.md step 8 must document that the kind axis "
        "is orthogonal to the severity axis. Expected one of "
        f"{orthogonality_signals!r} near the kind-axis paragraph."
    )


def test_deck_narrative_step10_worked_example_has_both_kinds():
    """Step 10 worked example must show BOTH `[in-place]` and
    `[structural]` findings.

    The worked example is the canonical reference the next narrative
    critic agent reads; if it shows only one kind, the agent will
    conflate the two.
    """
    body = _read(DECK_NARRATIVE)
    step10_idx = body.find("10. **Write `findings.md`")
    assert step10_idx != -1, (
        "deck-narrative.md is missing the `10. **Write findings.md` "
        "step header."
    )
    # The worked example sits inside a fenced block after the step
    # header.
    step10 = body[step10_idx : step10_idx + 4000]
    assert "[in-place]" in step10, (
        "deck-narrative.md step 10 worked example must include an "
        "`[in-place]` finding. The worked example is the canonical "
        "reference for next critic runs; both kinds must appear."
    )
    assert "[structural]" in step10, (
        "deck-narrative.md step 10 worked example must include a "
        "`[structural]` finding. Without a worked `[structural]` "
        "example, the canary's specific failure mode "
        "(Competition splitting the product story) has no canonical "
        "example for next critic runs."
    )


def test_deck_narrative_removes_no_reorders_framing():
    """The old "rather than reordering — Team's canonical slot is
    Slide 10" framing must be removed or rephrased.

    The original line read as a blanket "no reorders allowed"
    instruction to the reviser. Per issue #549 this framing flips
    to "allow reorder if kind is [structural]". The line either
    disappears from the worked example, or is rephrased to preserve
    the in-place rationale while removing the "no reorders" framing.

    Acceptance: the exact original phrase "rather than reordering —
    Team's canonical slot is Slide 10" must NOT appear verbatim.
    """
    body = _read(DECK_NARRATIVE)
    # The em-dash variant from the original line.
    forbidden_em = "rather than reordering — Team's canonical slot is Slide 10"
    # Also check the ASCII -- variant defensively.
    forbidden_dash = "rather than reordering -- Team's canonical slot is Slide 10"
    assert forbidden_em not in body, (
        f"deck-narrative.md still carries the no-reorders framing "
        f"`{forbidden_em}`. Per issue #549 this framing must be "
        "removed or rephrased — the reviser now has explicit "
        "restructure authority on `[structural]` findings."
    )
    assert forbidden_dash not in body, (
        f"deck-narrative.md still carries the no-reorders framing "
        f"`{forbidden_dash}`. Per issue #549 this framing must be "
        "removed or rephrased."
    )


def test_deck_narrative_documents_structural_for_reorder_merge_split_drop():
    """The `[structural]` kind must be documented as covering
    reorder / merge / split / drop slide-level operations.

    These four operations are the restructure authority the reviser
    gains under issue #549. The kind marker is the signal.
    """
    body = _read(DECK_NARRATIVE)
    # All four operations should be named somewhere in the
    # kind-axis documentation.
    for op in ("reorder", "merge", "split", "drop"):
        assert op in body.lower(), (
            f"deck-narrative.md must mention `{op}` as part of the "
            "`[structural]` kind-axis documentation. The four "
            "operations (reorder / merge / split / drop) are the "
            "restructure authority the reviser gains on `[structural]` "
            "findings."
        )
