"""Slides-side mirror of ``anvil/skills/deck/tests/test_marp_smoke.py``.

The smoke-test logic (frontmatter parsing, lint assertions, conditional
Marp CLI render) is identical between deck and slides — only the fixture
content differs (talk-flavored theorem + sequence diagram vs. deck-flavored
investor-MathJax + sequence diagram). To prevent drift between the two
sides, this module **does not duplicate the test logic**; instead it imports
the deck-side test classes directly (``anvil.skills.deck.tests`` is an
importable package because the ``__init__.py`` chain is in place), then
re-runs them against the slides-side fixture by rebinding the deck module's
``_FIXTURE`` constant before test discovery.

Post-#318 this file is a normal direct import — no ``importlib.util.spec_from_file_location``
shim. The previous shim existed to bridge the file-path-resolution era of
``marp_lint`` (when slides' ``lib/marp_lint.py`` itself loaded the deck
module by file path); after ``marp_lint`` was promoted to ``anvil/lib/`` in
#318, both sides import the canonical module directly and no file-path
gymnastics are needed.

The deck-side ``test_marp_smoke`` module exposes:

- ``_parse_frontmatter`` — minimal YAML-subset parser (stdlib-only).
- ``TestFixtureFrontmatter`` — asserts ``math: mathjax`` and ``html: true``
  are pinned in the fixture frontmatter.
- ``TestMarpConfigFile`` — asserts ``anvil/lib/marp/config.yml`` exists with
  the load-bearing keys.
- ``TestFixturePassesLint`` — asserts the fixture passes ``slide-content-overflow``.
- ``TestMarpRenders`` — conditional render test (skipped without Marp CLI).
- ``TestMermaidDiagramDoesNotLeakAsRawCode`` — #65 regression guard.

This mirror re-runs each of these against the slides-side fixture by
rebinding the module-level ``_FIXTURE`` constant before unittest discovery.
"""

from __future__ import annotations

import unittest
from pathlib import Path

from anvil.skills.deck.tests import test_marp_smoke as _deck_module


_HERE = Path(__file__).resolve().parent

# Rebind the fixture path to the slides-side fixture so the inherited test
# classes exercise slides content (theorem statement + sequence diagram)
# rather than deck content (investor-MathJax + sequence diagram).
_SLIDES_FIXTURE = _HERE / "fixtures" / "marp-smoke" / "deck.md"


# Subclassing with an overridden ``_FIXTURE`` would also work, but the
# deck-side module references the fixture at module scope; the cleanest
# mirror is to monkeypatch the module-level constant and re-export the
# test classes.
_deck_module._FIXTURE = _SLIDES_FIXTURE


TestFixtureFrontmatter = _deck_module.TestFixtureFrontmatter
TestMarpConfigFile = _deck_module.TestMarpConfigFile
TestFixturePassesLint = _deck_module.TestFixturePassesLint
TestMarpRenders = _deck_module.TestMarpRenders
TestMermaidDiagramDoesNotLeakAsRawCode = _deck_module.TestMermaidDiagramDoesNotLeakAsRawCode


class TestSlidesFixtureMatchesPin(unittest.TestCase):
    """Slides-specific assertion: the fixture uses the slides theme.

    A bug here would mean the slides-side fixture accidentally copied the
    deck-side theme reference; this catches drift between the two fixtures.
    """

    def test_fixture_uses_slides_theme(self) -> None:
        fm = _deck_module._parse_frontmatter(_SLIDES_FIXTURE)
        self.assertEqual(
            fm.get("theme"),
            "anvil-slides-theme",
            "slides smoke fixture must use the slides theme; "
            f"got {fm.get('theme')!r}",
        )


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
