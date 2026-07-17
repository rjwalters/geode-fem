"""Unit tests for the ``mmdc`` (mermaid-cli) preflight (issue #65 / #70).

``slides-figures`` must check that ``mmdc`` is on PATH before any
``mmdc → PNG`` diagram render, because inline ```mermaid fences do NOT render
as diagrams in the canonical ``marp --pdf`` output (verified, issue #65) — so
``mmdc`` is REQUIRED for any slide deck with a diagram, not a fallback.

The preflight lives in ``anvil/lib/render.py`` as
``check_mmdc_available`` so the figurer and the smoke test
share one implementation. These tests exercise it with a stubbed/monkeypatched
``shutil.which`` so they require **no real ``mmdc`` and no Chromium** at test
time (the whole point: this is unit-testable in CI without the ~300MB+
headless-Chromium toolchain).

Slides-distinct filename per the #58 packaging convention (the deck and slides
test suites share a pytest rootdir; a bare ``test_mmdc.py`` could collide).
Mirrors ``anvil/skills/deck/tests/test_deck_mmdc_preflight.py``.

Runs under either ``python -m unittest discover anvil/skills/slides/tests/``
or ``pytest anvil/skills/slides/tests/``.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest import mock

# Ensure repo root is importable. This file lives at
# anvil/skills/slides/tests/test_slides_mmdc_preflight.py — four levels deep
# from the repo root (same depth as the deck-side mirror).
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.lib.render import (  # noqa: E402
    MMDC_REMEDIATION,
    check_mmdc_available,
)


class TestCheckMmdcAvailable(unittest.TestCase):
    """``check_mmdc_available`` returns a bool based on PATH presence only."""

    def test_returns_true_when_mmdc_on_path(self) -> None:
        with mock.patch(
            "anvil.lib.render.shutil.which",
            return_value="/usr/local/bin/mmdc",
        ) as which:
            self.assertTrue(check_mmdc_available())
            which.assert_called_once_with("mmdc")

    def test_returns_false_when_mmdc_absent(self) -> None:
        with mock.patch(
            "anvil.lib.render.shutil.which", return_value=None
        ) as which:
            self.assertFalse(check_mmdc_available())
            which.assert_called_once_with("mmdc")

    def test_does_not_launch_chromium(self) -> None:
        """The preflight must be a pure PATH check — no subprocess spawn.

        If the preflight shelled out to ``mmdc`` (which would launch
        Chromium), this test — which stubs only ``shutil.which`` — would have
        to stub ``subprocess`` too. It does not, proving the check stays
        binary-presence-only and is safe to run in CI without Chromium.
        """
        with mock.patch(
            "anvil.lib.render.subprocess.run",
            side_effect=AssertionError("preflight must not spawn a subprocess"),
        ):
            with mock.patch(
                "anvil.lib.render.shutil.which",
                return_value="/usr/local/bin/mmdc",
            ):
                self.assertTrue(check_mmdc_available())


class TestMmdcRemediation(unittest.TestCase):
    """``MMDC_REMEDIATION`` carries the full install story for the blocker."""

    def test_remediation_message_is_actionable(self) -> None:
        """The blocker message must carry the full install story (issue #65)."""
        # npm install command.
        self.assertIn("@mermaid-js/mermaid-cli", MMDC_REMEDIATION)
        # The ~300MB+ Chromium download note.
        self.assertIn("Chromium", MMDC_REMEDIATION)
        # The CI/container --no-sandbox guidance.
        self.assertIn("--puppeteerConfigFile", MMDC_REMEDIATION)
        self.assertIn("--no-sandbox", MMDC_REMEDIATION)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
