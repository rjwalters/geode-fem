"""Deck-skill-local helpers.

These modules implement small in-skill primitives that the deck commands
(`deck-draft`, `deck-revise`, `deck-figures`, `deck-imagegen`, …) lean
on. They live here rather than under ``anvil/lib/`` because they are
skill-specific (the Marp overflow lint mirrors a marp-vscode diagnostic;
the prompt-journal schema is part of the `deck-imagegen` contract) and
because anvil's v0 policy is to let the framework ``lib/`` emerge from
observed duplication rather than design it up-front.

Public modules:

- :mod:`prompt_journal` — Read/write primitive for the
  ``<thread>.{N}/assets/_prompts.json`` journal (Epic #130 Phase 2D).
- :mod:`imagegen` — Generative-imagery orchestration runtime for the
  ``deck-imagegen`` command (Epic #130 Phase 2E). Loads the consumer
  adapter from ``.anvil/config.json``, dispatches one PNG per
  imagery-marker in ``deck.md``, and writes the prompt journal.
- :mod:`imagegen_phrases` — Canonical allowed-attribution and
  forbidden-documentary phrase lists for the fabrication-attribution
  contract (Epic #130 Phase 3F/3G; issue #195 consolidation). Single
  source of truth shared by ``deck-draft``, ``deck-revise``, and
  ``deck-audit``; exposes ``ALLOWED_ATTRIBUTION_PHRASES``,
  ``FORBIDDEN_DOCUMENTARY_PHRASES`` (both ``frozenset[str]``),
  ``has_attribution_phrase(text)``, and ``find_forbidden_phrases(text)``.

If a future skill needs the same primitives, the lift to
``anvil/lib/`` is mechanical.
"""
