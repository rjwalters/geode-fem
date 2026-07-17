"""Back-compat shim (issue #382): canonical module is ``anvil/lib/project_brief.py``.

Promoted from memo-skill-local to the shared framework lib when
``anvil:deck`` / ``anvil:slides`` / ``anvil:proposal`` became the
2nd–4th consumers of the project-org primitives (the CLAUDE.md
"wait for the second consumer before generalizing" trigger). This
shim preserves the historical import path
(``anvil.skills.memo.lib.project_brief``) so existing memo
consumers and tests keep working unchanged.
"""

from anvil.lib.project_brief import *  # noqa: F401,F403

# Names the canonical module exposes as module attributes without listing
# them in ``__all__`` (historically importable from this path — e.g. the
# proposal-side tests import ``BRIEF_FILENAME`` from a top-level
# ``project_brief`` module). A star import would drop them; re-export
# explicitly to keep the historical surface intact.
from anvil.lib.project_brief import (  # noqa: F401
    BRIEF_FILENAME,
    DOCUMENTS_FRONTMATTER_KEY,
)
