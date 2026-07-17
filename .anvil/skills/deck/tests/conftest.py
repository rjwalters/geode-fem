"""Per-skill pytest configuration for the deck test suite.

Surfaces the auto-shrink synthetic fixture builder defined in
``fixtures/auto_shrink/conftest.py`` to the tests under this directory.
pytest only auto-loads conftest files from the test file's directory and
its parents; the deck tests live in ``tests/`` while the fixture builder
lives one level deeper in ``tests/fixtures/auto_shrink/`` (kept there
deliberately so the synthetic-fixture infrastructure stays co-located
with the fixture deck.md alongside it). This shim re-exports the
``auto_shrink_fixture_root`` fixture so pytest can find it.
"""

from __future__ import annotations

import sys
from pathlib import Path

# Make the auto_shrink fixture builder importable so we can re-export
# its session-scoped fixture from a location pytest auto-loads.
_FIXTURES = Path(__file__).resolve().parent / "fixtures" / "auto_shrink"
if str(_FIXTURES) not in sys.path:
    sys.path.insert(0, str(_FIXTURES))

# Re-export the fixture defined in fixtures/auto_shrink/conftest.py.
# Importing it here makes pytest discover it through the standard
# conftest mechanism (auto-loaded for any test under tests/).
from conftest import auto_shrink_fixture_root  # noqa: F401,E402
