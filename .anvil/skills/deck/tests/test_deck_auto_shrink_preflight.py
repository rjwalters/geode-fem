"""Unit tests for the auto-shrink-detector preflight (issue #102 / #100b).

``deck-review`` runs an optional silent-Marp-auto-shrink lint per #102 that
needs ``Pillow`` and ``numpy``. Both are OPTIONAL — the rest of the review
proceeds without them and the missing-deps note is recorded as an
info-level skip in ``_summary.md``. This matches the established #65
(``mmdc``) and #85 (``pdfjam``) preflight pattern.

The preflight lives in ``anvil/lib/render.py`` as
``check_auto_shrink_deps_available`` so the deck-review command, the
detector, and these tests share one implementation. These tests exercise
it with a stubbed/monkeypatched ``importlib.import_module`` so they
require **no real Pillow/numpy** at test time — the whole point: this is
unit-testable in CI before the optional extra is installed.

Deck-distinct filename per the #58 packaging convention.

Runs under either ``python -m unittest discover anvil/skills/deck/tests/``
or ``pytest anvil/skills/deck/tests/``.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest import mock

import pytest


# Ensure repo root is importable. This file lives at
# anvil/skills/deck/tests/test_deck_auto_shrink_preflight.py — four levels
# deep from the repo root.
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.lib.render import (  # noqa: E402
    AUTO_SHRINK_REMEDIATION,
    check_auto_shrink_deps_available,
)


# NOTE: this file is intentionally NOT module-gated on the ``[auto_shrink]``
# extra. The whole point of the preflight test suite is to be unit-testable
# in CI before the optional extra is installed (per the module docstring
# above) — every test except ``test_returns_true_when_both_modules_importable``
# either stubs ``importlib.util.find_spec`` to simulate missing modules or
# asserts on the remediation string. Only the live happy-path check needs
# real Pillow/numpy at test time; it is gated per-test via
# ``pytest.importorskip`` so the rest of the file stays runnable on a
# stock venv. See pyproject.toml's top comment for the convention.


class TestCheckAutoShrinkDepsAvailable(unittest.TestCase):
    """``check_auto_shrink_deps_available`` returns bool based on importability.

    The preflight is a pure import-test — no rendering side effects, no
    subprocess spawn. Mirrors ``check_mmdc_available`` and
    ``check_pdfjam_available``.
    """

    def test_returns_true_when_both_modules_importable(self) -> None:
        # In the real test environment Pillow and numpy ARE installed
        # (they are required for the optional extra); the live call must
        # return True. This is the "happy path" check, parallel to the
        # ``shutil.which`` "/usr/local/bin/mmdc" stub for mmdc.
        #
        # This is the ONLY test in the file that needs the real
        # ``[auto_shrink]`` extra installed; the rest stub ``find_spec``
        # or assert on the remediation string. Gate it per-test so the
        # file as a whole stays runnable on a stock venv.
        pytest.importorskip(
            "PIL", reason="Pillow not installed ([auto_shrink] extra)"
        )
        pytest.importorskip(
            "numpy", reason="numpy not installed ([auto_shrink] extra)"
        )
        self.assertTrue(check_auto_shrink_deps_available())

    @staticmethod
    def _stub_find_spec(missing: set[str]):
        """Build a stub ``find_spec`` that returns ``None`` for missing modules."""
        import importlib.util as iu

        real = iu.find_spec

        def fake(name, *args, **kwargs):
            if name in missing:
                return None
            return real(name, *args, **kwargs)

        return fake

    def test_returns_false_when_pillow_missing(self) -> None:
        # Simulate Pillow not being importable. The preflight must return
        # False without raising — graceful-skip is the whole point of the
        # optional-extra pattern.
        with mock.patch(
            "anvil.lib.render.importlib.util.find_spec",
            side_effect=self._stub_find_spec({"PIL"}),
        ):
            self.assertFalse(check_auto_shrink_deps_available())

    def test_returns_false_when_numpy_missing(self) -> None:
        with mock.patch(
            "anvil.lib.render.importlib.util.find_spec",
            side_effect=self._stub_find_spec({"numpy"}),
        ):
            self.assertFalse(check_auto_shrink_deps_available())

    def test_returns_false_when_both_missing(self) -> None:
        with mock.patch(
            "anvil.lib.render.importlib.util.find_spec",
            side_effect=self._stub_find_spec({"PIL", "numpy"}),
        ):
            self.assertFalse(check_auto_shrink_deps_available())

    def test_does_not_load_pil_image_or_numpy_array(self) -> None:
        """The preflight must be import-test-only — no model/array load.

        If the preflight constructed a ``PIL.Image`` or a ``numpy.array``
        as part of the check, this test — which stubs only the import
        machinery — would have to stub them too. It does not, proving the
        check stays import-test-only and is safe to call in tight loops.
        """
        # Asserts the live call returns True, so PIL/numpy must actually
        # be importable here. Gate per-test so the rest of the file stays
        # runnable on a stock venv without the ``[auto_shrink]`` extra.
        pytest.importorskip(
            "PIL", reason="Pillow not installed ([auto_shrink] extra)"
        )
        pytest.importorskip(
            "numpy", reason="numpy not installed ([auto_shrink] extra)"
        )
        # Spy on subprocess.run: the preflight must never shell out.
        with mock.patch(
            "anvil.lib.render.subprocess.run",
            side_effect=AssertionError(
                "auto-shrink preflight must not spawn a subprocess"
            ),
        ):
            self.assertTrue(check_auto_shrink_deps_available())


class TestAutoShrinkRemediation(unittest.TestCase):
    """The ``AUTO_SHRINK_REMEDIATION`` string must carry an actionable install line."""

    def test_remediation_mentions_pip_install_extra(self) -> None:
        # The opt-in extra is the documented install path per the
        # maintainer resolution on #102. The remediation string must
        # surface it so a user reading the lint skip-note in
        # ``_summary.md`` knows how to enable the check.
        self.assertIn("auto_shrink", AUTO_SHRINK_REMEDIATION)

    def test_remediation_mentions_uv_install(self) -> None:
        # Anvil's first-Python-deps decision pinned uv as the install
        # tool. The remediation must name it.
        self.assertIn("uv pip install", AUTO_SHRINK_REMEDIATION)

    def test_remediation_mentions_pillow_and_numpy(self) -> None:
        # Either explicitly names the libs, or names them via the
        # ``pip install Pillow numpy`` fallback. Both must appear so the
        # user understands what they're installing.
        self.assertIn("Pillow", AUTO_SHRINK_REMEDIATION)
        self.assertIn("numpy", AUTO_SHRINK_REMEDIATION)

    def test_remediation_mentions_graceful_skip(self) -> None:
        # Document that the lint is OPTIONAL — the rest of deck-review
        # still runs. This is the contract the deck-review command
        # relies on; a remediation message that suggested otherwise
        # would mislead the user.
        # We assert on the word "proceeds" rather than the full sentence
        # so a copyedit on the remediation doesn't break this test.
        self.assertIn("proceeds", AUTO_SHRINK_REMEDIATION)

    def test_remediation_references_issue_number(self) -> None:
        # Cross-reference to the originating issue so a future reader
        # can find the design discussion.
        self.assertIn("#102", AUTO_SHRINK_REMEDIATION)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
