"""Back-compat shim (issue #382): canonical module is ``anvil/lib/project_discovery.py``.

Promoted from memo-skill-local to the shared framework lib when
``anvil:deck`` / ``anvil:slides`` / ``anvil:proposal`` became the
2nd–4th consumers of the project-org primitives (the CLAUDE.md
"wait for the second consumer before generalizing" trigger). This
shim preserves the historical import path
(``anvil.skills.memo.lib.project_discovery``) so existing memo
consumers and tests keep working unchanged.
"""

from anvil.lib.project_discovery import *  # noqa: F401,F403
