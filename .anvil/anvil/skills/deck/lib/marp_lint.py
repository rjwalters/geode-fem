"""Deck-side re-export of ``anvil.lib.marp_lint`` (promoted per issue #318)."""
from anvil.lib.marp_lint import (
    Finding,
    Geometry,
    LintResult,
    PORTED_RULES,
    UPSTREAM_SHA,
    lint_deck,
    lint_source,
)

__all__ = [
    "Finding",
    "Geometry",
    "LintResult",
    "PORTED_RULES",
    "UPSTREAM_SHA",
    "lint_deck",
    "lint_source",
]
