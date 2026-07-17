"""Pytest sys.path wiring for `anvil:rubric-rebackport` tests (issue #358).

Adds the tests directory itself to sys.path so test modules can do
``from _fixtures import ...`` and ``from _skill_lib import ...``
without ceremony.

The skill's lib modules are loaded under a unique package name
(``rubric_rebackport_lib``) via ``_skill_lib`` to avoid cross-skill
collisions when this test suite runs alongside other per-skill test
suites that each ship their own ``lib/`` package (e.g.,
``project-migrate``).
"""

from __future__ import annotations

import sys
from pathlib import Path


_HERE = Path(__file__).resolve().parent

sys.path.insert(0, str(_HERE))
