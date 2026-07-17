"""Idempotence tests for mixed-skill migrations (issue #382).

Re-running ``--apply`` on a migrated mixed memo + deck + proposal
project must be a zero-diff no-op — the same contract the memo-only
suite pins in ``test_project_migrate_idempotent.py``, extended to the
shapes where deck.md / proposal.tex bodies are retained (the relaxed
FULLY_MIGRATED body check is what makes this pass).

Per the #58 packaging convention this filename is unique across the
``anvil/skills/*/tests/`` tree.
"""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _project_migrate_skill_lib import orchestrate  # noqa: E402
from _fixtures import (  # noqa: E402
    build_aldus_shaped_deck,
    build_mixed_memo_deck_proposal,
)

run = orchestrate.run


def _tree_digest(root: Path) -> str:
    """Stable digest of a tree: sorted relpaths + per-file content hashes."""
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        rel = path.relative_to(root)
        digest.update(str(rel).encode("utf-8"))
        if path.is_file():
            digest.update(path.read_bytes())
    return digest.hexdigest()


class TestMixedIdempotence(unittest.TestCase):
    def test_mixed_reapply_is_noop(self) -> None:
        with TemporaryDirectory() as td:
            project = build_mixed_memo_deck_proposal(Path(td))
            first = run(project, apply=True)
            self.assertTrue(first.success)
            before = _tree_digest(project)

            second = run(project, apply=True)
            self.assertTrue(second.success)
            self.assertTrue(second.plan.is_noop)
            after = _tree_digest(project)
            self.assertEqual(
                before, after,
                "re-apply on a migrated mixed project changed the tree",
            )

    def test_aldus_deck_reapply_is_noop(self) -> None:
        with TemporaryDirectory() as td:
            project = build_aldus_shaped_deck(Path(td))
            first = run(project, apply=True)
            self.assertTrue(first.success)
            before = _tree_digest(project)

            second = run(project, apply=True)
            self.assertTrue(second.success)
            self.assertTrue(second.plan.is_noop)
            after = _tree_digest(project)
            self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
