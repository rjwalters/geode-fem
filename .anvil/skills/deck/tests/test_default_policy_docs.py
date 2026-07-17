"""Doc-coverage smoke tests for issue #547 — `deck.imagegen.default_policy`.

The consumer-level proactive-imagery override added in issue #547 lands
across four user-facing docs:

1. ``deck-imagegen.md`` — Preconditions table + Procedure step 2 +
   Failure-modes table (the resolution order and the source-naming
   contract on `_progress.json` `reason`).
2. ``deck-imagegen-adapter.md`` — Consumer-registration snippet shows
   `default_policy` alongside `backend`; a dedicated subsection
   documents the resolution order and the closed-enum contract.
3. ``deck-brief.md`` — § "imagery_policy" gains a "Consumer-level
   default override" subsection naming the config key + resolution
   order.
4. ``SKILL.md`` — § "Asset generation" mentions the override (one
   sentence; canonical contract stays in `deck-imagegen.md`).

Substring-presence only, following the precedent of
``test_imagery_policy_docs.py``: no Marp render, no schema parse.
Distinct filename per the #58 packaging convention.
"""

from __future__ import annotations

from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent

DECK_IMAGEGEN = _SKILL_ROOT / "commands" / "deck-imagegen.md"
DECK_IMAGEGEN_ADAPTER = _SKILL_ROOT / "commands" / "deck-imagegen-adapter.md"
DECK_BRIEF = _SKILL_ROOT / "commands" / "deck-brief.md"
SKILL_MD = _SKILL_ROOT / "SKILL.md"


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


# ---------------------------------------------------------------------------
# deck-imagegen.md
# ---------------------------------------------------------------------------


def test_deck_imagegen_names_default_policy_key() -> None:
    """deck-imagegen.md must name the new config key at least once."""
    body = _read(DECK_IMAGEGEN)
    assert "default_policy" in body, (
        "deck-imagegen.md does not mention `default_policy`. The "
        "Preconditions table should document the resolution order; see "
        "issue #547."
    )


def test_deck_imagegen_documents_resolution_order() -> None:
    """The resolution order (BRIEF → config → built-in) must be present."""
    body = _read(DECK_IMAGEGEN)
    assert "imagery_policy:" in body
    assert "deck.imagegen.default_policy" in body
    assert "built-in" in body.lower(), (
        "deck-imagegen.md should mention the built-in fallback in the "
        "resolution order so consumers understand backward compat."
    )


def test_deck_imagegen_documents_source_naming_in_reason() -> None:
    """The `_progress.json` `reason` field must name the source per #547."""
    body = _read(DECK_IMAGEGEN)
    # At least one of the source-naming phrases must appear.
    sources = ("BRIEF.md", ".anvil/config.json")
    assert all(s in body for s in sources), (
        "deck-imagegen.md should name BOTH BRIEF.md and .anvil/config.json "
        "as candidate sources for the `_progress.json` reason field — the "
        "operator needs to see which decided."
    )


# ---------------------------------------------------------------------------
# deck-imagegen-adapter.md
# ---------------------------------------------------------------------------


def test_adapter_snippet_includes_default_policy() -> None:
    """The Consumer-registration JSON snippet must show `default_policy`."""
    body = _read(DECK_IMAGEGEN_ADAPTER)
    assert "default_policy" in body
    # The dedicated subsection title.
    assert "Optional: `deck.imagegen.default_policy`" in body, (
        "deck-imagegen-adapter.md should carry a dedicated subsection for "
        "the override with title 'Optional: `deck.imagegen.default_policy`' "
        "(or equivalent)."
    )


def test_adapter_documents_resolution_order_with_closed_enum() -> None:
    body = _read(DECK_IMAGEGEN_ADAPTER)
    # Closed-enum values.
    assert "generative-eligible" in body
    assert "consumer-provided" in body
    assert "deterministic-only" in body
    # Resolution order phrase (loose check).
    assert "Resolution order" in body or "resolution order" in body


# ---------------------------------------------------------------------------
# deck-brief.md
# ---------------------------------------------------------------------------


def test_deck_brief_documents_consumer_level_override() -> None:
    body = _read(DECK_BRIEF)
    assert "default_policy" in body, (
        "deck-brief.md § 'imagery_policy' should document the consumer-level "
        "default override and the resolution order."
    )
    assert "Consumer-level default override" in body, (
        "deck-brief.md should include a subsection named 'Consumer-level "
        "default override' (or similar) under § 'imagery_policy'."
    )


def test_deck_brief_documents_per_thread_opt_out() -> None:
    """A consumer who set `default_policy: generative-eligible` can still
    opt out per-thread by setting `imagery_policy: deterministic-only` in
    a single BRIEF. This is the load-bearing rule that lets a B2B / technical
    thread coexist with an aesthetic-craft portfolio under one config."""
    body = _read(DECK_BRIEF)
    # Loose check: the doc must mention that BRIEF wins over config.
    assert "BRIEF" in body
    assert "wins" in body.lower() or "precedence" in body.lower() or (
        "highest priority" in body.lower()
    ), (
        "deck-brief.md should document that BRIEF.md `imagery_policy` "
        "wins over the consumer-level `default_policy`."
    )


# ---------------------------------------------------------------------------
# SKILL.md
# ---------------------------------------------------------------------------


def test_skill_md_mentions_default_policy() -> None:
    body = _read(SKILL_MD)
    assert "default_policy" in body, (
        "SKILL.md § 'Asset generation' should mention the consumer-level "
        "default_policy override (one sentence is sufficient; canonical "
        "contract stays in deck-imagegen.md)."
    )


def test_skill_md_preserves_non_waivable_attribution_contract() -> None:
    """The fabrication-attribution contract MUST be documented as
    non-waivable, including under the proactive `default_policy` override.
    This is the load-bearing safety contract on issue #547."""
    body = _read(SKILL_MD)
    # The non-waivability + the proactive-default mention must coexist.
    assert "non-waivable" in body.lower() or "non waivable" in body.lower(), (
        "SKILL.md should document that the fabrication-attribution "
        "contract is non-waivable under any imagery_policy resolution "
        "(including the proactive default_policy override)."
    )
