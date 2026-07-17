"""Pytest sys.path wiring for `anvil:project-photos` tests (issue #599).

Adds the tests directory itself to sys.path so test modules can do
``from _photos_fixtures import ...`` and
``from _project_photos_skill_lib import ...`` without ceremony, and the
repo root so the suite resolves when run standalone
(``pytest anvil/skills/project-photos/tests/`` — the repo-root wiring in
``tests/conftest.py`` does not apply there).

The skill's lib modules are loaded under a unique package name
(``project_photos_lib``) via ``_project_photos_skill_lib`` to avoid the
cross-skill ``lib`` package collision when this suite runs alongside
other per-skill suites that each ship their own ``lib/`` package
(``project-scout``, ``project-share``, ``project-migrate``,
``rubric-rebackport``). The helper filename is also unique (per the
issue #367 fix — two suites both shipping ``_skill_lib.py`` collide on
``sys.modules['_skill_lib']``).
"""

from __future__ import annotations

import sys
from pathlib import Path

_HERE = Path(__file__).resolve().parent
# tests/ → project-photos/ → skills/ → anvil/ → repo root.
_REPO_ROOT = _HERE.parents[3]

sys.path.insert(0, str(_HERE))
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))
