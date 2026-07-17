"""Thin re-export of the shared parity lint, promoted to ``anvil/lib/parity.py``.

Per CLAUDE.md §"Skill-local first, lib promotion later" the second-consumer
trigger fired on the merge of PR #215 (memo-side mirror). Issue #317 is the
canonical one-line import-path swap promoting both skill-local mirrors
(``anvil/skills/deck/lib/parity_lint.py``, ``anvil/skills/memo/lib/parity_lint.py``)
into ``anvil/lib/parity.py``. This module is now a thin re-export preserving
the deck-side public surface verbatim — the deck-review step 5d invocation
and the doc-coverage tests reference this skill-local module path, so the
re-export contract is load-bearing.

See ``anvil/lib/parity.py``'s module docstring for the full Phase A / Phase B
contract, canary anchor (Citation Clear ~50–60% completion), and deferred
follow-on list. See WORK_LOG.md PR #205 / PR #215 / this PR for the
promotion history.
"""

from anvil.lib.parity import (  # noqa: F401
    EXTRACTORS,
    Finding,
    LintResult,
    UNIT_VOCABULARY,
    lint_deck_memo_parity,
    lint_source,
)
from anvil.lib.parity import RULES_DECK as RULES  # noqa: F401


__all__ = [
    "EXTRACTORS",
    "Finding",
    "LintResult",
    "RULES",
    "UNIT_VOCABULARY",
    "lint_deck_memo_parity",
    "lint_source",
]
