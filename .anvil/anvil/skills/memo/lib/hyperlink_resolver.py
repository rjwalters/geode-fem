"""Back-compat shim (issue #460): canonical module is ``anvil/lib/hyperlink_resolver.py``.

Promoted from memo-skill-local to the shared framework lib when
``anvil:essay`` (#460) became the second consumer of the deterministic
hyperlink-resolution critic (the CLAUDE.md "wait for the second
consumer before generalizing" trigger; same promotion pattern as the
#382 project-org primitives and the #393 ``rubric_overrides_suffix``).
This shim preserves the historical import path
(``anvil.skills.memo.lib.hyperlink_resolver``) so existing memo
consumers and tests keep working unchanged.

The historical CLI invocation
(``python -m anvil.skills.memo.lib.hyperlink_resolver <version_dir>``)
also keeps working via the ``__main__`` guard below; the canonical
invocation is ``python -m anvil.lib.hyperlink_resolver``.
"""

from anvil.lib.hyperlink_resolver import *  # noqa: F401,F403

# ``main`` is in the canonical module's ``__all__``, but re-export it
# explicitly so static readers (and the __main__ guard) see it.
from anvil.lib.hyperlink_resolver import main  # noqa: F401

if __name__ == "__main__":
    raise SystemExit(main())
