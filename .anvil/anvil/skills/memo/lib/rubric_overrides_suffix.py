"""Back-compat shim (issue #393): canonical module is ``anvil/lib/rubric_overrides_suffix.py``.

Promoted from memo-skill-local to the shared framework lib when
``anvil:deck`` became the second consumer of the calibration-suffix /
waiver-normalization primitives (the CLAUDE.md "wait for the second
consumer before generalizing" trigger — same shape as the issue #382
promotion of ``project_brief.py`` / ``project_discovery.py``). This
shim preserves the historical import path
(``anvil.skills.memo.lib.rubric_overrides_suffix``) so existing memo
consumers and tests keep working unchanged.
"""

from anvil.lib.rubric_overrides_suffix import *  # noqa: F401,F403
