"""Slides-side re-export of ``anvil.lib.marp_lint`` (promoted per issue #318).

The previous file was a ~90-line ``importlib.util.spec_from_file_location``
shim that loaded the deck-side module by file path — which broke at import
time when slides was installed without deck. After promotion to
``anvil/lib/marp_lint.py``, this shim is a direct package import.
"""
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
