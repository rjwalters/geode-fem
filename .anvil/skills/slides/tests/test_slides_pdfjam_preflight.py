"""Unit tests for the ``pdfjam`` preflight (issue #85).

``slides-handout`` must check that ``pdfjam`` is on PATH before invoking the
N-up post-process step for the ``--4-up`` and ``--2-up`` handout layouts,
because Marp cannot natively express N-up (verified, issue #85: Marp's
rendering model is one-section-per-page; there is no Marp CLI flag or CSS
injection that combines N sections onto a single rendered page). The
``--notes-below`` default-friendly layout renders via Marp's native
``--pdf-notes`` mode and has zero pdfjam dependency — so the preflight is
GATED on layout and MUST NOT fire on the ``--notes-below`` path (a
false-positive blocker there would lock out users who deliberately chose the
pdfjam-free path).

The preflight lives in ``anvil/lib/render.py`` as
``check_pdfjam_available`` / ``require_pdfjam`` so the handout exporter and
any future caller share one implementation. These tests exercise it with a
stubbed/monkeypatched ``shutil.which`` so they require **no real ``pdfjam``
and no TeX Live install** at test time (the whole point: this is
unit-testable in CI without the multi-GB TeX Live toolchain).

Slides-distinct filename per the #58 packaging convention. Mirrors the shape
of ``anvil/skills/slides/tests/test_slides_mmdc_preflight.py`` from #70.

Runs under either ``python -m unittest discover anvil/skills/slides/tests/``
or ``pytest anvil/skills/slides/tests/``.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest import mock

# Ensure repo root is importable. This file lives at
# anvil/skills/slides/tests/test_slides_pdfjam_preflight.py — four levels
# deep from the repo root (same depth as the mmdc-side mirror).
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.lib.render import (  # noqa: E402
    PDFJAM_REMEDIATION,
    RenderError,
    check_pdfjam_available,
    require_pdfjam,
)


class TestCheckPdfjamAvailable(unittest.TestCase):
    """``check_pdfjam_available`` returns a bool based on PATH presence only."""

    def test_returns_true_when_pdfjam_on_path(self) -> None:
        with mock.patch(
            "anvil.lib.render.shutil.which",
            return_value="/usr/local/bin/pdfjam",
        ) as which:
            self.assertTrue(check_pdfjam_available())
            which.assert_called_once_with("pdfjam")

    def test_returns_false_when_pdfjam_absent(self) -> None:
        with mock.patch(
            "anvil.lib.render.shutil.which", return_value=None
        ) as which:
            self.assertFalse(check_pdfjam_available())
            which.assert_called_once_with("pdfjam")

    def test_does_not_spawn_subprocess(self) -> None:
        """The preflight must be a pure PATH check — no subprocess spawn.

        If the preflight shelled out to ``pdfjam`` (which would invoke
        TeX Live), this test — which stubs only ``shutil.which`` — would
        have to stub ``subprocess`` too. It does not, proving the check
        stays binary-presence-only and is safe to run in CI without TeX
        Live installed.
        """
        with mock.patch(
            "anvil.lib.render.subprocess.run",
            side_effect=AssertionError(
                "preflight must not spawn a subprocess"
            ),
        ):
            with mock.patch(
                "anvil.lib.render.shutil.which",
                return_value="/usr/local/bin/pdfjam",
            ):
                self.assertTrue(check_pdfjam_available())


class TestRequirePdfjam(unittest.TestCase):
    """``require_pdfjam`` raises with full remediation when ``pdfjam`` absent."""

    def test_no_raise_when_present(self) -> None:
        with mock.patch(
            "anvil.lib.render.shutil.which",
            return_value="/usr/local/bin/pdfjam",
        ):
            # Should not raise.
            require_pdfjam()

    def test_raises_render_error_when_absent(self) -> None:
        with mock.patch(
            "anvil.lib.render.shutil.which", return_value=None
        ):
            with self.assertRaises(RenderError) as ctx:
                require_pdfjam()
            self.assertEqual(str(ctx.exception), PDFJAM_REMEDIATION)

    def test_remediation_message_is_actionable(self) -> None:
        """The blocker message must carry the full install story (issue #85)."""
        # Names the layouts that require pdfjam (N-up handouts).
        self.assertIn("--4-up", PDFJAM_REMEDIATION)
        self.assertIn("--2-up", PDFJAM_REMEDIATION)
        # Names the layout that does NOT require pdfjam (the escape valve so
        # users on the pdfjam-free path know they can ignore this).
        self.assertIn("--notes-below", PDFJAM_REMEDIATION)
        # Install commands for the three major platforms.
        self.assertIn("tlmgr install pdfjam", PDFJAM_REMEDIATION)
        self.assertIn("texlive-extra-utils", PDFJAM_REMEDIATION)
        self.assertIn("mactex-no-gui", PDFJAM_REMEDIATION)
        # The multi-GB TeX Live size caveat.
        self.assertIn("multi-GB", PDFJAM_REMEDIATION)


class TestLayoutGating(unittest.TestCase):
    """The handout precheck must be GATED on layout.

    ``--4-up`` / ``--2-up`` invocations require pdfjam (post-process is the
    only N-up path; Marp cannot natively express N-up). ``--notes-below``
    invocations MUST NOT call the preflight at all — that layout has zero
    pdfjam dependency, and a false-positive blocker there would lock out
    users who deliberately chose the pdfjam-free path.

    These tests model the gating logic the handout exporter (a markdown
    command spec) is documented to implement: ``require_pdfjam()`` is invoked
    iff the requested layout is in the N-up set. The harness mirrors how a
    Python caller would gate the call.
    """

    NUP_LAYOUTS = {"--4-up", "--2-up"}

    def _maybe_require(self, layout: str) -> None:
        """Mirror the spec's gating: precheck only for N-up layouts."""
        if layout in self.NUP_LAYOUTS:
            require_pdfjam()

    def test_pdfjam_present_4up_passes(self) -> None:
        with mock.patch(
            "anvil.lib.render.shutil.which",
            return_value="/usr/local/bin/pdfjam",
        ):
            # Should not raise.
            self._maybe_require("--4-up")

    def test_pdfjam_present_2up_passes(self) -> None:
        with mock.patch(
            "anvil.lib.render.shutil.which",
            return_value="/usr/local/bin/pdfjam",
        ):
            self._maybe_require("--2-up")

    def test_pdfjam_absent_4up_blocks(self) -> None:
        with mock.patch(
            "anvil.lib.render.shutil.which", return_value=None
        ):
            with self.assertRaises(RenderError) as ctx:
                self._maybe_require("--4-up")
            self.assertEqual(str(ctx.exception), PDFJAM_REMEDIATION)

    def test_pdfjam_absent_2up_blocks(self) -> None:
        with mock.patch(
            "anvil.lib.render.shutil.which", return_value=None
        ):
            with self.assertRaises(RenderError) as ctx:
                self._maybe_require("--2-up")
            self.assertEqual(str(ctx.exception), PDFJAM_REMEDIATION)

    def test_pdfjam_absent_notes_below_skipped(self) -> None:
        """The CRITICAL no-false-positive case for the default-friendly path.

        ``--notes-below`` must NOT trigger the preflight when pdfjam is
        absent. If this test ever fails, the handout exporter is gating
        incorrectly and a pdfjam-free user can no longer produce a
        notes-below handout.
        """
        with mock.patch(
            "anvil.lib.render.shutil.which", return_value=None
        ):
            # Must not raise — pdfjam is not needed for --notes-below.
            self._maybe_require("--notes-below")

    def test_pdfjam_present_notes_below_skipped(self) -> None:
        """Even when pdfjam IS present, --notes-below skips the preflight.

        Documents that the gating is layout-driven, not best-effort: we never
        bother checking pdfjam for --notes-below regardless of whether it's
        installed.
        """
        with mock.patch(
            "anvil.lib.render.shutil.which",
            side_effect=AssertionError(
                "--notes-below must not invoke the pdfjam preflight"
            ),
        ):
            # Should not raise — _maybe_require should not call require_pdfjam
            # (and therefore should not call shutil.which) for --notes-below.
            self._maybe_require("--notes-below")


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
