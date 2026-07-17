"""Thin re-export of the shared parity lint, promoted to ``anvil/lib/parity.py``.

Per CLAUDE.md §"Skill-local first, lib promotion later" the second-consumer
trigger fired on the merge of PR #215 (this memo-side mirror). Issue #317 is
the canonical one-line import-path swap promoting both skill-local mirrors
(``anvil/skills/deck/lib/parity_lint.py``, ``anvil/skills/memo/lib/parity_lint.py``)
into ``anvil/lib/parity.py``. This module is now a thin re-export preserving
the memo-side public surface verbatim — the memo-review step 4d invocation
and the doc-coverage tests reference this skill-local module path, so the
re-export contract is load-bearing.

The only meaningful API asymmetry between the two skill-local modules was
the positional-arg order on ``lint_source`` — deck-side took
``(deck_source, memo_source)`` while memo-side took ``(memo_source,
deck_source)``. Both contracts are preserved: the shared
``anvil.lib.parity.lint_source`` accepts ``(deck_source, memo_source)``
positional order with a keyword-only ``rule`` argument, and this module's
``lint_source`` wrapper flips back to the memo-side ``(memo_source,
deck_source)`` order. The memo-side symmetry test
(``test_symmetry_with_deck_side_lint_source``) exploits this asymmetry by
importing both modules and calling them with opposite argument orders; the
wrapper preserves the contract.

See ``anvil/lib/parity.py``'s module docstring for the full Phase A / Phase B
contract, canary anchor (Citation Clear ~50–60% completion), and deferred
follow-on list. See WORK_LOG.md PR #200 / PR #205 / PR #215 / this PR for
the promotion history.
"""

from anvil.lib.parity import (  # noqa: F401
    EXTRACTORS,
    Finding,
    LintResult,
    UNIT_VOCABULARY,
    lint_memo_deck_parity,
)
from anvil.lib.parity import RULES_MEMO as RULES  # noqa: F401


def lint_source(memo_source: str, deck_source: str) -> LintResult:
    """Memo-side ``lint_source`` wrapper.

    Flips the positional arg order vs. the shared core in
    ``anvil.lib.parity.lint_source`` and pins ``rule="memo_deck_parity"``
    so the existing memo-side contract (and the symmetry test that imports
    both ``lint_source`` variants) is preserved verbatim.
    """
    from anvil.lib.parity import lint_source as _shared

    return _shared(deck_source, memo_source, rule="memo_deck_parity")


__all__ = [
    "EXTRACTORS",
    "Finding",
    "LintResult",
    "RULES",
    "UNIT_VOCABULARY",
    "lint_memo_deck_parity",
    "lint_source",
]
