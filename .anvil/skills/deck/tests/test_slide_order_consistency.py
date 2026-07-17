"""Slide-order consistency tests for the anvil:deck skill.

Per issue #86: ``deck-draft.md`` step 6 and ``deck-narrative.md``'s
``comments.md`` example shipped with conflicting canonical orders, and
``slide-archetypes.md`` was in a partial-conflict state (Why-now ahead
of Solution, but Product/Competition still in the old drafter order).
The substantive ground truth — what a fresh ``deck-draft`` run actually
emits — is ``templates/deck.md.j2`` and its parallel
``templates/speaker-notes.md.j2``.

The canonical 12-slide order (Title → Ask), pinned in the issue #86
curator comment, is:

    1 Title → 2 Problem → 3 Why now → 4 Solution → 5 Competition →
    6 Product → 7 Market → 8 Traction → 9 Business model → 10 Team →
    11 Financials → 12 Ask

These tests assert (a) the token sequence appears IN ORDER in every
surface that documents the slide structure, and (b) the specific
"misordered" string the narrative critic previously shipped — which
described the now-canonical order as misordered — is absent.

Tests are substring-presence + sequence-position only, following the
precedent of ``tests/lib/test_snippet_contents.py``. No fixture
execution, no Marp render — distinct from ``test_marp_smoke.py`` and
``test_deck_vision.py`` (per #58 file-naming convention).
"""

from __future__ import annotations

from pathlib import Path


_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/

DECK_DRAFT = _SKILL_ROOT / "commands" / "deck-draft.md"
DECK_NARRATIVE = _SKILL_ROOT / "commands" / "deck-narrative.md"
DECK_TEMPLATE = _SKILL_ROOT / "templates" / "deck.md.j2"
SPEAKER_NOTES_TEMPLATE = _SKILL_ROOT / "templates" / "speaker-notes.md.j2"
SLIDE_ARCHETYPES = _SKILL_ROOT / "assets" / "slide-archetypes.md"


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def _assert_tokens_in_order(body: str, tokens: list[str], source_label: str) -> None:
    """Assert that ``tokens`` appear in ``body`` strictly in order.

    Each token is searched for as a substring, starting from the index
    just after the previous match. Raises AssertionError naming the
    first token that is missing or out of order.
    """
    cursor = 0
    for i, tok in enumerate(tokens):
        idx = body.find(tok, cursor)
        assert idx != -1, (
            f"{source_label}: token {i + 1}/{len(tokens)} {tok!r} not found "
            f"after position {cursor}. Tokens already matched in order: "
            f"{tokens[:i]!r}."
        )
        cursor = idx + len(tok)


# ---------------------------------------------------------------------------
# Canonical sequences per file (one per surface, since each surface uses
# slightly different heading text but expresses the same order).
# ---------------------------------------------------------------------------

# Drafter step-6 bullets: ``- **Slide N**: <Name>`` form.
DECK_DRAFT_STEP6_SEQUENCE = [
    "**Slide 1**: Title",
    "**Slide 2**: Problem",
    "**Slide 3**: Why now",
    "**Slide 4**: Solution",
    "**Slide 5**: Competition",
    "**Slide 6**: Product",
    "**Slide 7**: Market",
    "**Slide 8**: Traction",
    "**Slide 9**: Business model",
    "**Slide 10**: Team",
    "**Slide 11**: Financials",
    "**Slide 12**: Ask",
]

# Template (deck.md.j2): the title slide uses ``<!-- _class: title -->`` and
# the ask slide uses ``<!-- _class: ask -->`` + ``## Raising`` rather than
# a literal ``## Title`` / ``## Ask`` heading. Use the actual surface tokens.
DECK_TEMPLATE_SEQUENCE = [
    "_class: title",
    "## The problem",
    "## Why now",
    "## Our solution",
    "## Competition",
    "## Product",
    "## Market",
    "## Traction",
    "## Business model",
    "## Team",
    "## Financials",
    "_class: ask",
]

# speaker-notes.md.j2: ``## Slide N — <Name>`` form.
SPEAKER_NOTES_SEQUENCE = [
    "## Slide 1 — Title",
    "## Slide 2 — The problem",
    "## Slide 3 — Why now",
    "## Slide 4 — Our solution",
    "## Slide 5 — Competition",
    "## Slide 6 — Product",
    "## Slide 7 — Market",
    "## Slide 8 — Traction",
    "## Slide 9 — Business model",
    "## Slide 10 — Team",
    "## Slide 11 — Financials",
    "## Slide 12 — The ask",
]

# slide-archetypes.md: ``## N. <Name>`` numbered section headings.
SLIDE_ARCHETYPES_SEQUENCE = [
    "## 1. Title",
    "## 2. Problem",
    "## 3. Why now",
    "## 4. Solution",
    "## 5. Competition",
    "## 6. Product",
    "## 7. Market",
    "## 8. Traction",
    "## 9. Business model",
    "## 10. Team",
    "## 11. Financials",
    "## 12. Ask",
]


# ---------------------------------------------------------------------------
# Positive: canonical order present, in order, in every surface.
# ---------------------------------------------------------------------------


def test_deck_draft_step6_uses_canonical_order():
    """deck-draft step 6 must list the 12 canonical slides in order."""
    body = _read(DECK_DRAFT)
    _assert_tokens_in_order(
        body, DECK_DRAFT_STEP6_SEQUENCE, "commands/deck-draft.md (step 6)"
    )


def test_deck_template_uses_canonical_order():
    """templates/deck.md.j2 must emit slides in the 12 canonical positions."""
    body = _read(DECK_TEMPLATE)
    _assert_tokens_in_order(
        body, DECK_TEMPLATE_SEQUENCE, "templates/deck.md.j2"
    )


def test_speaker_notes_template_uses_canonical_order():
    """templates/speaker-notes.md.j2 must mirror deck.md.j2 slide order."""
    body = _read(SPEAKER_NOTES_TEMPLATE)
    _assert_tokens_in_order(
        body, SPEAKER_NOTES_SEQUENCE, "templates/speaker-notes.md.j2"
    )


def test_slide_archetypes_use_canonical_numbering():
    """assets/slide-archetypes.md numbered headings must follow canonical order.

    This is the file that had the partial-conflict state per issue #86:
    Why-now was already #3 (matching narrative critic) but Product was
    still #5 and Competition still #6 (matching the old drafter order).
    After the fix, Competition becomes #5 and Product becomes #6.
    """
    body = _read(SLIDE_ARCHETYPES)
    _assert_tokens_in_order(
        body, SLIDE_ARCHETYPES_SEQUENCE, "assets/slide-archetypes.md"
    )


# ---------------------------------------------------------------------------
# Negative: the pre-fix misorder example string the narrative critic
# shipped — describing the now-canonical order as misordered — must be
# absent. Regression on this assertion means a future edit re-introduced
# the contradictory example.
# ---------------------------------------------------------------------------


# This is the exact substring the narrative critic's ``comments.md``
# example previously contained at line 102. If a future edit reverts
# the example to call the canonical order misordered, this test fails.
NARRATIVE_PRIOR_MISORDER_STRING = (
    "Current: Title → Problem → Solution → Why-now "
    "→ Product → Market → Competition → Traction "
    "→ Business model → Team → Financials → Ask"
)


def test_deck_narrative_example_does_not_describe_canonical_as_misordered():
    """The narrative critic's comments.md example must not name the
    canonical order as a misorder.

    The pre-fix shipped example said:
        "Current: Title → Problem → Solution → Why-now → Product →
         Market → Competition → Traction → Business model → Team →
         Financials → Ask. The Competition slide is out of place
         (recommended order: ...canonical...)."
    where the "Current" sequence is in fact the OLD drafter step-6
    order. That example was self-contradicting in two ways: (a) it
    described the drafter's own template as the misordered case, and
    (b) post-fix the drafter no longer emits that sequence at all.
    """
    body = _read(DECK_NARRATIVE)
    assert NARRATIVE_PRIOR_MISORDER_STRING not in body, (
        "deck-narrative.md still contains the pre-fix misorder example "
        "string that described the (now canonical) shipped slide order "
        "as out-of-place. The example must be rewritten to either "
        "illustrate a different misorder or drop the 'Current:' framing "
        "altogether — see issue #86."
    )
